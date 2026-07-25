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
    let installer = download_and_verify(client, &update).await?;
    info!("update verified; launching installer for a silent upgrade");
    // Diverges: replaces this process with the installer + a relaunch.
    launch_installer_and_exit(&installer)
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

/// Hand off to the downloaded installer for a silent, in-place upgrade, then exit.
///
/// The running exe holds a lock on itself, so a detached `cmd` shell (which does
/// NOT lock our files) drives the sequence: force-close this process, run the
/// installer silently, relaunch the freshly installed exe, then delete the
/// downloaded installer. We spawn it detached and exit immediately so the
/// installer never races our own file lock.
#[cfg(target_os = "windows")]
fn launch_installer_and_exit(installer: &Path) -> ! {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW | DETACHED_PROCESS — no console flash, survives our exit.
    const FLAGS: u32 = 0x0800_0000 | 0x0000_0008;

    let inst = installer.display();
    let relaunch = std::env::current_exe()
        .ok()
        .map(|p| format!(" & start \"\" \"{}\"", p.display()))
        .unwrap_or_default();
    let script = format!(
        "taskkill /im ailimits.exe /f >nul 2>&1 & \
         \"{inst}\" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART{relaunch} \
         & del /q \"{inst}\" >nul 2>&1"
    );
    let _ = std::process::Command::new("cmd")
        .args(["/C", &script])
        .creation_flags(FLAGS)
        .spawn();
    std::process::exit(0);
}

#[cfg(not(target_os = "windows"))]
fn launch_installer_and_exit(_installer: &Path) -> ! {
    // Non-Windows builds ship no installer; nothing to hand off to.
    std::process::exit(0);
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
}
