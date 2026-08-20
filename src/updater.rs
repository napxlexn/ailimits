// updater.rs — background auto-update.
//
// Polls the GitHub Releases API and, when a newer release publishes a Windows
// installer, downloads it, verifies its SHA-256 against the release metadata,
// then hands off to the installer for a silent, in-place upgrade and exits.
//
// The whole feature is gated behind the `auto_update` config flag (a menu
// toggle), passed in as a shared atomic so the loop honours live changes.
// Integrity is enforced: a release without a `sha256` asset digest, or a
// download whose hash does not match, is refused — never installed.

use anyhow::{bail, Context, Result};
use ring::digest::{digest, SHA256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// GitHub repository that publishes releases.
const REPO: &str = "napxlexn/ailimits";
/// Delay before the first check so startup stays snappy.
const INITIAL_DELAY: Duration = Duration::from_secs(30);
/// Interval between checks.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// HTTP timeout — generous enough to download the installer (~4 MB).
const HTTP_TIMEOUT: Duration = Duration::from_secs(120);

/// A newer release with a hash-verifiable Windows installer asset.
struct Update {
    version: String,
    asset_url: String,
    sha256: String,
}

/// Background loop: check on an interval while `enabled` is true. On a confirmed,
/// hash-verified newer release it installs and exits the process, so a successful
/// upgrade never returns here.
pub async fn run(enabled: Arc<AtomicBool>) {
    tokio::time::sleep(INITIAL_DELAY).await;
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            warn!("auto-update disabled: {e:#}");
            return;
        }
    };
    loop {
        if enabled.load(Ordering::Relaxed) {
            if let Err(e) = check_and_install(&client).await {
                // A failed check is non-fatal: log and try again next cycle.
                debug!("auto-update check skipped: {e:#}");
            }
        }
        tokio::time::sleep(CHECK_INTERVAL).await;
    }
}

fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(concat!("ailimits/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("build updater HTTP client")
}

async fn check_and_install(client: &reqwest::Client) -> Result<()> {
    let Some(update) = fetch_latest(client).await? else {
        return Ok(());
    };
    info!(
        "update available: v{} (running v{})",
        update.version,
        env!("CARGO_PKG_VERSION")
    );
    // Only a copy that our OWN installer put on disk may silently reinstall
    // itself. A Scoop/portable/dev copy running the installer would not update
    // the running copy at all — it would create a SECOND install in the Inno
    // directory and relaunch that one, leaving the manager-owned copy stale
    // and the user with two AI Limits. Those copies are updated by whatever
    // installed them (scoop update, a fresh zip, cargo build).
    if !running_from_managed_install() {
        info!(
            "skipping silent self-update: this copy is not managed by the installer              (portable, Scoop or a dev build) — update it through its own channel"
        );
        return Ok(());
    }
    let installer = download_and_verify(client, &update).await?;
    info!("update verified; launching installer for a silent upgrade");
    // Diverges on success: the installer replaces this process. A failed
    // handoff must NOT take the app down — stay on the current version.
    if let Err(e) = launch_installer_and_exit(&installer) {
        warn!("update handoff failed, staying on the current version: {e:#}");
    }
    Ok(())
}

/// Query the latest release; return Some only when it is strictly newer than the
/// running build AND carries a `.exe` asset with a usable sha256 digest.
async fn fetch_latest(client: &reqwest::Client) -> Result<Option<Update>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body: serde_json::Value = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let tag = body
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let latest = parse_version(tag).context("unparseable release tag")?;
    let current = parse_version(env!("CARGO_PKG_VERSION")).expect("crate version parses");
    if latest <= current {
        return Ok(None);
    }

    let asset = body
        .get("assets")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .find(|a| {
            a.get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| n.to_ascii_lowercase().ends_with(".exe"))
        })
        .context("release has no .exe installer asset")?;

    let asset_url = asset
        .get("browser_download_url")
        .and_then(|v| v.as_str())
        .context("installer asset has no download url")?
        .to_string();
    // GitHub reports the asset digest as "sha256:<hex>". No digest → refuse.
    let sha256 = asset
        .get("digest")
        .and_then(|v| v.as_str())
        .and_then(|d| d.strip_prefix("sha256:"))
        .context("installer asset has no sha256 digest — refusing to install unverified")?
        .to_string();

    Ok(Some(Update {
        version: normalize(tag),
        asset_url,
        sha256,
    }))
}

/// Download the installer, verify its SHA-256, and write it to a temp path.
async fn download_and_verify(client: &reqwest::Client, update: &Update) -> Result<PathBuf> {
    let bytes = client
        .get(&update.asset_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    let actual = hex_lower(digest(&SHA256, &bytes).as_ref());
    if !actual.eq_ignore_ascii_case(&update.sha256) {
        bail!(
            "installer sha256 mismatch (expected {}, got {actual}) — aborting",
            update.sha256
        );
    }

    let dest = std::env::temp_dir().join(format!("AiLimits-Setup-{}.exe", update.version));
    tokio::fs::write(&dest, &bytes)
        .await
        .with_context(|| format!("write installer to {}", dest.display()))?;
    Ok(dest)
}

/// Where the STANDARD install puts the exe: `{localappdata}\AiLimits\ailimits.exe`
/// (installer/ailimits.iss default). This is only a guess, not a guarantee:
/// the installer is interactive and does not set `DisableDirPage`, so a user
/// can pick a different directory, and Inno reuses that choice for later
/// silent upgrades too. Callers must verify the path actually exists before
/// relying on it — see `relaunch_target`.
fn installed_exe_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("AiLimits").join("ailimits.exe"))
}

/// Where the Inno uninstall entry says the app is installed, if it exists.
/// Written by the installer itself (per-user key, `{AppId}_is1`), so unlike
/// `installed_exe_path()` it is correct even for a custom install directory.
#[cfg(target_os = "windows")]
fn inno_install_location() -> Option<PathBuf> {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
    };
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{7A1B9C44-5E2D-4F8A-9C3B-AILIMITS0001}_is1"),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
        .is_err()
        {
            return None;
        }
        let mut buf = [0u8; 1040];
        let mut len = buf.len() as u32;
        let ok = RegQueryValueExW(
            key,
            w!("InstallLocation"),
            None,
            None,
            Some(buf.as_mut_ptr()),
            Some(&mut len),
        )
        .is_ok();
        let _ = RegCloseKey(key);
        if !ok {
            return None;
        }
        let u16s: Vec<u16> = buf[..len as usize]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        let s = String::from_utf16_lossy(&u16s);
        (!s.is_empty()).then(|| PathBuf::from(s))
    }
}

/// Whether the running exe lives inside the directory our installer manages.
/// Decided by canonicalized paths, so the registry's trailing backslash and
/// any case difference do not matter. Pure on its inputs for testability.
fn exe_is_inside(current_exe: Option<&Path>, install_dir: Option<&Path>) -> bool {
    let (Some(exe), Some(dir)) = (current_exe, install_dir) else {
        return false;
    };
    let (Ok(exe), Ok(dir)) = (exe.canonicalize(), dir.canonicalize()) else {
        // A location that cannot be resolved cannot be trusted to upgrade.
        return false;
    };
    exe.parent() == Some(dir.as_path())
}

#[cfg(target_os = "windows")]
fn running_from_managed_install() -> bool {
    exe_is_inside(
        std::env::current_exe().ok().as_deref(),
        inno_install_location().as_deref(),
    )
}

#[cfg(not(target_os = "windows"))]
fn running_from_managed_install() -> bool {
    false
}

/// Pick where to relaunch after an update: the standard install path if it
/// actually exists on disk, else the exe that is currently running.
///
/// `installed_exe_path()` is only correct for the default install location;
/// for a custom install directory it names a path nothing ever wrote to, and
/// relaunching a path that does not exist silently loses the app — the
/// update installs fine but AI Limits never reappears. Falling back to the
/// running exe still fixes the reinstall-forever loop from the second update
/// onward, because once the installer has written to the standard directory
/// that path exists. Split out as a pure function so this fallback is
/// testable without touching `dirs::data_local_dir()` or the real installer
/// directory.
fn relaunch_target(standard: Option<PathBuf>, current: Option<PathBuf>) -> Option<PathBuf> {
    standard.filter(|p| p.exists()).or(current)
}

/// Hand off to the downloaded installer for a silent, in-place upgrade, then exit.
///
/// The running exe holds a lock on itself, so a detached `cmd` shell (which does
/// NOT lock our files) drives the sequence: force-close this process, run the
/// installer silently, relaunch the freshly installed exe, then delete the
/// downloaded installer. We spawn it detached and exit immediately so the
/// installer never races our own file lock.
#[cfg(target_os = "windows")]
fn launch_installer_and_exit(installer: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW | DETACHED_PROCESS — no console flash, survives our exit.
    const FLAGS: u32 = 0x0800_0000 | 0x0000_0008;

    let inst = installer.display();
    let relaunch = relaunch_target(installed_exe_path(), std::env::current_exe().ok())
        .map(|p| format!("start \"\" \"{}\"\r\n", p.display()))
        .unwrap_or_default();
    let script = format!(
        "@echo off\r\n\
         taskkill /im ailimits.exe /f >nul 2>&1\r\n\
         ping -n 2 127.0.0.1 >nul\r\n\
         \"{inst}\" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART\r\n\
         {relaunch}\
         del /q \"{inst}\" >nul 2>&1\r\n\
         del /q \"%~f0\" >nul 2>&1\r\n"
    );

    let bat = std::env::temp_dir().join("ailimits-update.cmd");
    std::fs::write(&bat, script)
        .with_context(|| format!("write the update script to {}", bat.display()))?;

    // Absolute interpreter: a planted cmd.exe in the working directory must
    // not be able to hijack the handoff (same rule as hooks.rs).
    std::process::Command::new(crate::hooks::system_cmd_exe())
        .raw_arg(format!("/C \"{}\"", bat.display()))
        .creation_flags(FLAGS)
        .spawn()
        .context("spawn the update handoff")?;

    // The handoff is running; leave so the installer can replace our files.
    std::process::exit(0);
}

#[cfg(not(target_os = "windows"))]
fn launch_installer_and_exit(_installer: &Path) -> Result<()> {
    // Non-Windows builds ship no installer; nothing to hand off to.
    anyhow::bail!("no installer handoff on this platform")
}

/// Parse "v1.2.3" / "1.2.3" into a comparable tuple; any pre-release/build
/// suffix is ignored (releases ship plain `vX.Y.Z` tags).
fn parse_version(tag: &str) -> Option<(u32, u32, u32)> {
    let core = tag.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next().unwrap_or(core);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// The version string without a leading `v`, for display and the temp filename.
fn normalize(tag: &str) -> String {
    tag.trim().trim_start_matches(['v', 'V']).to_string()
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tags_with_and_without_prefix() {
        assert_eq!(parse_version("v0.5.3"), Some((0, 5, 3)));
        assert_eq!(parse_version("0.5.3"), Some((0, 5, 3)));
        assert_eq!(parse_version("v1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("v0.6.0-rc1"), Some((0, 6, 0)));
        assert_eq!(parse_version("nightly"), None);
    }

    #[test]
    fn newer_release_compares_greater() {
        let cur = parse_version("0.5.3").unwrap();
        assert!(parse_version("0.5.4").unwrap() > cur);
        assert!(parse_version("0.6.0").unwrap() > cur);
        assert!(parse_version("1.0.0").unwrap() > cur);
        // Same or older must NOT trigger an update.
        assert!(parse_version("0.5.3").unwrap() <= cur);
        assert!(parse_version("0.5.2").unwrap() <= cur);
    }

    #[test]
    fn hex_lower_matches_known_vector() {
        // SHA-256("") — the canonical empty-input digest.
        let d = digest(&SHA256, b"");
        assert_eq!(
            hex_lower(d.as_ref()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn installed_path_targets_the_installer_directory() {
        // The installer writes to {localappdata}\AiLimits\ailimits.exe; the
        // relaunch must aim there, not at whatever copy happens to be running.
        let p = installed_exe_path().expect("local data dir resolves on Windows");
        assert!(p.ends_with("AiLimits/ailimits.exe") || p.ends_with(r"AiLimits\ailimits.exe"));
    }

    /// The self-update guard: only a copy inside the installer-managed
    /// directory may reinstall itself. A Scoop or portable copy running the
    /// installer would create a second install and leave itself stale.
    #[test]
    fn a_copy_inside_the_managed_dir_is_recognised() {
        let dir = std::env::temp_dir().join("ailimits_managed_test");
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("ailimits.exe");
        std::fs::write(&exe, b"stub").unwrap();
        // The registry stores the location with a trailing separator and its
        // own casing; canonicalisation must absorb both.
        let reg_style = format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR);
        assert!(exe_is_inside(
            Some(exe.as_path()),
            Some(std::path::Path::new(&reg_style))
        ));
        let _ = std::fs::remove_file(&exe);
    }

    #[test]
    fn a_copy_elsewhere_or_with_no_install_record_is_not_managed() {
        let dir = std::env::temp_dir().join("ailimits_managed_test2");
        std::fs::create_dir_all(&dir).unwrap();
        let elsewhere = std::env::temp_dir().join("ailimits_elsewhere.exe");
        std::fs::write(&elsewhere, b"stub").unwrap();
        assert!(
            !exe_is_inside(Some(elsewhere.as_path()), Some(dir.as_path())),
            "an exe outside the recorded install dir must not self-update"
        );
        assert!(
            !exe_is_inside(Some(elsewhere.as_path()), None),
            "no uninstall record means no managed install"
        );
        assert!(
            !exe_is_inside(
                Some(elsewhere.as_path()),
                Some(std::path::Path::new("Z:/does/not/exist"))
            ),
            "an unresolvable location cannot be trusted to upgrade"
        );
        let _ = std::fs::remove_file(&elsewhere);
    }

    #[test]
    fn relaunch_prefers_the_standard_path_when_it_exists() {
        // A real file inside a temp dir stands in for a standard-location
        // install; nothing outside the temp dir is touched.
        let dir =
            std::env::temp_dir().join(format!("ailimits-relaunch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let standard = dir.join("ailimits.exe");
        std::fs::write(&standard, b"stub").unwrap();
        let current = dir.join("current.exe"); // never created — must be ignored

        let target = relaunch_target(Some(standard.clone()), Some(current));
        assert_eq!(target, Some(standard));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn relaunch_falls_back_to_the_running_exe_when_the_standard_path_is_absent() {
        // A custom install directory means installed_exe_path() names a
        // location the installer never wrote to. The path below is never
        // created, so it must not exist; the running exe must win instead.
        let dir =
            std::env::temp_dir().join(format!("ailimits-relaunch-test-{}-b", std::process::id()));
        let missing_standard = dir.join("ailimits.exe");
        let current = dir.join("current.exe");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&current, b"stub").unwrap();
        assert!(!missing_standard.exists());

        let target = relaunch_target(Some(missing_standard), Some(current.clone()));
        assert_eq!(target, Some(current));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn relaunch_yields_none_when_neither_path_is_available() {
        assert_eq!(relaunch_target(None, None), None);
    }
}
