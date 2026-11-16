// updater.rs — background auto-update.
//
// Only a copy that our own installer put on disk updates itself. A Store
// package, a Scoop or portable copy and a dev build stop before anything else
// (`channel`), so they never ask the network anything: the Store, Scoop or a
// fresh download updates those.
//
// An installer copy asks the GitHub Releases API once a day. When a newer
// release publishes the Windows installer it downloads it and checks two
// things before running it: the SHA-256 GitHub reports for the asset, and the
// Ed25519 signature the release pipeline made with the release key. The
// public half of that key is built in here (`RELEASE_KEYS`); the private half
// lives only with the owner and in the release secret. An installer without a
// signature, or with one that does not verify, is refused — never installed.
//
// The handoff starts the installer itself, with no script in between: its
// path goes to CreateProcessW as UTF-16, so a Cyrillic, `&` or `%` profile or
// TEMP path works like any other, and the switches are plain ASCII. The app
// then exits so its files can be replaced, and the installer starts the new
// version itself (`RELAUNCH_SWITCH`, read by installer/ailimits.iss). Each
// attempt is written down first (`ATTEMPT_FILE`, beside the config): a version
// whose attempt did not end with that version running is never tried again by
// itself, so a failing upgrade cannot take the widget down after every start.
//
// The installer closes the widget for a few seconds, so a verified update
// waits until the user has been away for five minutes (no input, or the
// session locked) before it is downloaded and run (`IDLE_BEFORE_INSTALL`).
// An update that did not finish is told to the user once, at the next start
// (`unfinished_update_note`).
//
// The whole feature is gated behind the `auto_update` config flag (a menu
// toggle), passed in as a shared atomic so the loop honours live changes.
// Its requests follow the proxy set in Windows, as the providers' do.

use anyhow::{anyhow, bail, Context, Result};
use ring::digest::{digest, SHA256};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
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
/// How long the user must have been away (no input, or the session locked)
/// before an update is installed: the widget closes while the installer
/// runs, and should not vanish under someone's cursor. The same five
/// minutes after which the scheduler pauses its polling.
const IDLE_BEFORE_INSTALL_SECS: u64 = 300;
/// While an update waits for that, how often to look again.
const IDLE_RECHECK: Duration = Duration::from_secs(60);

/// The release signing keys this build trusts: Ed25519 public keys. A
/// signature names its key by `key_id`, so a later key can be added here
/// (and the old one dropped a release after) without breaking an update.
/// The private key is never in the repository: the owner keeps it, and the
/// release workflow signs with it from the `AILIMITS_RELEASE_SIGNING_KEY`
/// secret (tools/release_sign.rs).
pub const RELEASE_KEYS: &[[u8; 32]] = &[
    // Key id 1507bd92b1a4bb7b, made 2026-11-16.
    [
        0x86, 0x91, 0x88, 0x60, 0x23, 0x42, 0x6a, 0x4f, 0x88, 0x27, 0x79, 0xd7, 0x54, 0xfe, 0xaf,
        0x85, 0x0e, 0x97, 0x7d, 0x5e, 0x70, 0x71, 0x00, 0x13, 0xc0, 0x92, 0xe4, 0xac, 0xa0, 0x2c,
        0x0d, 0xd9,
    ],
];

/// What a signature asset's name adds to the signed file's:
/// `AiLimits-Setup-0.7.1.exe.sig`.
pub const SIGNATURE_SUFFIX: &str = ".sig";

/// The first word of a signature file: its format, so a later one can be told
/// apart.
const SIGNATURE_FORMAT: &str = "ailimits-signature-v1";

/// A signature file is one short line; anything longer is not one.
const SIGNATURE_MAX_BYTES: usize = 1024;

/// Asks the installer to start the app again once the upgrade is done. The
/// installer reads it as `{param:RELAUNCH}` (installer/ailimits.iss, [Run]),
/// and only in a silent run.
pub(crate) const RELAUNCH_SWITCH: &str = "/RELAUNCH=1";

/// The installer's switches: a silent upgrade that closes whatever still
/// holds the app's files, never restarts Windows, and starts the app again.
/// ASCII only — nothing from any path is part of the command line but the
/// installer's own path, which Windows receives as UTF-16.
const INSTALLER_SWITCHES: &[&str] = &[
    "/VERYSILENT",
    "/SUPPRESSMSGBOXES",
    "/NORESTART",
    "/CLOSEAPPLICATIONS",
    RELAUNCH_SWITCH,
];

/// The record of the last update attempt, beside the config.
const ATTEMPT_FILE: &str = "update-attempt.json";

/// The handoff script 0.6.0 to 0.7.0 wrote to %TEMP%. Where it failed (a
/// Cyrillic or `&` path) it stayed behind, beside the installer it fetched.
const LEGACY_SCRIPT: &str = "ailimits-update.cmd";

/// A version as the updater compares it.
type Version = (u32, u32, u32);

/// How this copy of the app is updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Put on disk by our installer: it updates itself.
    Installer,
    /// A Microsoft Store package: the Store updates it.
    Store,
    /// Scoop, the portable zip or a dev build: updated through that channel.
    Unmanaged,
}

/// The channel, from the two local facts it depends on. Pure for testing.
fn channel_of(packaged: bool, managed: bool) -> Channel {
    if packaged {
        // A Store copy's files sit in a read-only package directory: an
        // installer run there could not replace them and would leave a
        // second, unmanaged copy beside the managed one.
        Channel::Store
    } else if managed {
        Channel::Installer
    } else {
        // A Scoop/portable/dev copy running the installer would not update
        // the running copy at all — it would create a SECOND install in the
        // Inno directory and relaunch that one, leaving the manager-owned
        // copy stale and the user with two AI Limits.
        Channel::Unmanaged
    }
}

/// How this copy is updated. Decided from local facts only — the package
/// identity and the installer's own uninstall entry — never the network.
pub fn channel() -> Channel {
    channel_of(
        crate::platform::is_packaged(),
        running_from_managed_install(),
    )
}

/// Whether this copy updates itself: the only copies the Auto-update menu
/// item does anything for.
pub fn self_update_supported() -> bool {
    channel() == Channel::Installer
}

/// A newer release with a verifiable Windows installer.
struct Update {
    /// The version, without a leading `v`.
    version: String,
    /// The installer's download URL.
    installer_url: String,
    /// The SHA-256 GitHub reports for the installer, hex.
    sha256: String,
    /// The signature's download URL.
    signature_url: String,
}

/// Background loop: check on an interval while `enabled` is true. On a
/// confirmed, verified newer release it installs and exits the process, so a
/// successful upgrade never returns here.
pub async fn run(enabled: Arc<AtomicBool>) {
    // Before anything else: a copy something else updates never asks the
    // network anything, whatever the Auto-update setting says.
    match channel() {
        Channel::Installer => {}
        Channel::Store => {
            info!("auto-update: this copy is a Store package — the Store updates it");
            return;
        }
        Channel::Unmanaged => {
            info!(
                "auto-update: this copy is not managed by the installer (portable, Scoop or a \
                 dev build) — update it through its own channel"
            );
            return;
        }
    }
    tokio::time::sleep(INITIAL_DELAY).await;

    let current = current_version();
    let state = state_dir();
    match settle_attempt(&state, current) {
        Settled::Nothing => {}
        Settled::Installed(v) => info!("auto-update: now running v{v}, the update finished"),
        Settled::Failed(v) => warn!(
            "auto-update: the update to v{v} did not finish (still running v{}); it is not \
             tried again by itself — install it from https://github.com/{REPO}/releases",
            env!("CARGO_PKG_VERSION")
        ),
    }
    remove_legacy_leftovers(&std::env::temp_dir(), current);

    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            warn!("auto-update disabled: {e:#}");
            return;
        }
    };
    loop {
        if enabled.load(Ordering::Relaxed) {
            if let Err(e) = check_and_install(&client, &state, current, &enabled).await {
                // A failed check is non-fatal: log and try again next cycle.
                debug!("auto-update check skipped: {e:#}");
            }
        }
        tokio::time::sleep(CHECK_INTERVAL).await;
    }
}

fn build_client() -> Result<reqwest::Client> {
    // The proxy set in Windows, once the app follows it (as the providers'
    // clients do); an HTTP(S)_PROXY variable still wins.
    crate::providers::proxy::with_windows_proxy(reqwest::Client::builder())
        .timeout(HTTP_TIMEOUT)
        .user_agent(concat!("ailimits/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("build updater HTTP client")
}

/// The running build's version.
fn current_version() -> Version {
    parse_version(env!("CARGO_PKG_VERSION")).expect("crate version parses")
}

/// Where the attempt record lives: the folder of the config (and the cache
/// and log), which the uninstaller removes.
fn state_dir() -> PathBuf {
    let config = crate::config::storage::config_path();
    config
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

async fn check_and_install(
    client: &reqwest::Client,
    state: &Path,
    current: Version,
    enabled: &AtomicBool,
) -> Result<()> {
    let Some(update) = fetch_latest(client, current).await? else {
        return Ok(());
    };
    info!(
        "update available: v{} (running v{})",
        update.version,
        env!("CARGO_PKG_VERSION")
    );
    if attempt_failed(state, &update.version, current) {
        info!(
            "skipping v{}: its update did not finish the last time — it is not tried again by \
             itself",
            update.version
        );
        return Ok(());
    }

    // Only while the user is away: the installer closes the widget for a
    // few seconds. Downloaded once that moment has come, and run only if the
    // user is still away when the download is through.
    let installer = loop {
        if !wait_until_away(enabled).await {
            info!(
                "auto-update was switched off while v{} waited",
                update.version
            );
            return Ok(());
        }
        let installer = download_and_verify(client, &update).await?;
        if away(crate::platform::user_idle_secs()) {
            break installer;
        }
        let _ = std::fs::remove_file(&installer);
        debug!("the user came back during the download; the update waits again");
    };
    // Written down BEFORE the handoff: should the upgrade fail, the next
    // start knows, and does not try this version again.
    let attempt = Attempt {
        version: update.version.clone(),
        installer: installer.clone(),
        told: false,
    };
    if let Err(e) = record_attempt(state, &attempt) {
        let _ = std::fs::remove_file(&installer);
        return Err(e.context("record the update attempt — not installing without it"));
    }
    info!("update verified; starting the installer for a silent upgrade");
    // Diverges on success: the installer replaces this process. A failed
    // handoff must NOT take the app down — stay on the current version.
    if let Err(e) = launch_installer_and_exit(&installer) {
        warn!("update handoff failed, staying on the current version: {e:#}");
        let _ = std::fs::remove_file(&installer);
    }
    Ok(())
}

/// Whether the user has been away long enough for an update to install:
/// `idle_secs` without input (`platform::user_idle_secs`, which a locked
/// session keeps counting).
fn away(idle_secs: u64) -> bool {
    idle_secs >= IDLE_BEFORE_INSTALL_SECS
}

/// Wait until the user is away (`away`). False when auto-update was
/// switched off meanwhile: nothing is installed then.
async fn wait_until_away(enabled: &AtomicBool) -> bool {
    let mut said = false;
    loop {
        if !enabled.load(Ordering::Relaxed) {
            return false;
        }
        if away(crate::platform::user_idle_secs()) {
            return true;
        }
        if !said {
            info!(
                "the update waits until the user has been away for {} minutes",
                IDLE_BEFORE_INSTALL_SECS / 60
            );
            said = true;
        }
        tokio::time::sleep(IDLE_RECHECK).await;
    }
}

/// Query the latest release; Some only when it is strictly newer than the
/// running build and carries a verifiable installer (see `release_update`).
async fn fetch_latest(client: &reqwest::Client, current: Version) -> Result<Option<Update>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body: serde_json::Value = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    release_update(&body, current)
}

/// The update a release offers: None when it is not newer than `current`;
/// an error when it is newer but its installer cannot be verified — no
/// `AiLimits-Setup-<version>.exe`, no sha256 digest for it, or no signature
/// beside it. Pure on its inputs for testability.
fn release_update(release: &serde_json::Value, current: Version) -> Result<Option<Update>> {
    let tag = release
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let latest = parse_version(tag).context("unparseable release tag")?;
    if latest <= current {
        return Ok(None);
    }
    let version = normalize(tag);
    let name = installer_file_name(&version);

    let installer =
        named_asset(release, &name).with_context(|| format!("release has no {name} asset"))?;
    let installer_url = download_url(installer)?;
    // GitHub reports the asset digest as "sha256:<hex>". No digest → refuse.
    let sha256 = installer
        .get("digest")
        .and_then(|v| v.as_str())
        .and_then(|d| d.strip_prefix("sha256:"))
        .context("installer asset has no sha256 digest — refusing to install unverified")?
        .to_string();
    let signature =
        named_asset(release, &format!("{name}{SIGNATURE_SUFFIX}")).with_context(|| {
            format!("release has no signature for {name} — refusing to install it unsigned")
        })?;
    let signature_url = download_url(signature)?;

    Ok(Some(Update {
        version,
        installer_url,
        sha256,
        signature_url,
    }))
}

/// The installer's file name for a version: installer/ailimits.iss
/// `OutputBaseFilename`. The release workflow checks the tag, Cargo.toml and
/// the installer script agree, and the signature covers this name, so an
/// installer published under another version's name does not verify.
pub fn installer_file_name(version: &str) -> String {
    format!("AiLimits-Setup-{version}.exe")
}

/// The release asset with exactly this name (ignoring case). Any other asset
/// — another `.exe` included — is never run as the installer.
fn named_asset<'a>(release: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    release
        .get("assets")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .find(|a| {
            a.get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
}

fn download_url(asset: &serde_json::Value) -> Result<String> {
    asset
        .get("browser_download_url")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .context("release asset has no download url")
}

/// Download the signature and the installer, verify both checks, and write
/// the installer to a temp path.
async fn download_and_verify(client: &reqwest::Client, update: &Update) -> Result<PathBuf> {
    let signature = client
        .get(&update.signature_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if signature.len() > SIGNATURE_MAX_BYTES {
        bail!("the installer's signature file is not one — refusing");
    }
    let signature = String::from_utf8(signature.to_vec())
        .map_err(|_| anyhow!("the installer's signature file is not text — refusing"))?;
    // Read before the 4 MB download: a malformed signature fails here.
    ReleaseSignature::parse(&signature)?;

    let bytes = client
        .get(&update.installer_url)
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
    let name = installer_file_name(&update.version);
    verify_release(&name, &bytes, &signature)
        .with_context(|| format!("{name} failed its signature check — refusing to install it"))?;

    let dest = std::env::temp_dir().join(&name);
    tokio::fs::write(&dest, &bytes)
        .await
        .with_context(|| format!("write installer to {}", dest.display()))?;
    Ok(dest)
}

// ─── Release signatures ──────────────────────────────────────────

/// What a release signature signs: the published file's name and the SHA-256
/// of its bytes. The name carries the version, so a signed installer cannot
/// be passed off as another version.
pub fn signed_message(file_name: &str, sha256: &[u8]) -> Vec<u8> {
    format!(
        "AI Limits release signature v1\nfile {file_name}\nsha256 {}\n",
        hex_lower(sha256)
    )
    .into_bytes()
}

/// A key's id: the first 8 bytes of the SHA-256 of the public key.
pub fn key_id(public_key: &[u8; 32]) -> [u8; 8] {
    let mut id = [0u8; 8];
    id.copy_from_slice(&digest(&SHA256, public_key).as_ref()[..8]);
    id
}

/// The text of a signature file: format, key id and signature, one line.
pub fn encode_signature(key: &[u8; 8], signature: &[u8; 64]) -> String {
    format!(
        "{SIGNATURE_FORMAT} {} {}\n",
        hex_lower(key),
        hex_lower(signature)
    )
}

/// A signature file, read.
struct ReleaseSignature {
    key: [u8; 8],
    signature: [u8; 64],
}

impl ReleaseSignature {
    fn parse(text: &str) -> Result<Self> {
        let mut parts = text.split_whitespace();
        let (Some(format), Some(key), Some(signature), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            bail!("not a release signature");
        };
        if format != SIGNATURE_FORMAT {
            bail!("unknown release signature format {format:?}");
        }
        let key = hex_decode(key)
            .and_then(|k| <[u8; 8]>::try_from(k).ok())
            .context("release signature has no readable key id")?;
        let signature = hex_decode(signature)
            .and_then(|s| <[u8; 64]>::try_from(s).ok())
            .context("release signature is not readable")?;
        Ok(Self { key, signature })
    }
}

/// Check `bytes`, published as `file_name`, against the text of its
/// signature file, with the release keys built into this exe.
pub fn verify_release(file_name: &str, bytes: &[u8], signature: &str) -> Result<()> {
    verify_with_keys(RELEASE_KEYS, file_name, bytes, signature)
}

fn verify_with_keys(
    keys: &[[u8; 32]],
    file_name: &str,
    bytes: &[u8],
    signature: &str,
) -> Result<()> {
    let signature = ReleaseSignature::parse(signature)?;
    let key = keys
        .iter()
        .find(|k| key_id(k) == signature.key)
        .context("signed with a key this build does not trust")?;
    let message = signed_message(file_name, digest(&SHA256, bytes).as_ref());
    UnparsedPublicKey::new(&ED25519, key)
        .verify(&message, &signature.signature)
        .map_err(|_| anyhow!("the signature does not match {file_name}"))
}

// ─── Attempts ────────────────────────────────────────────────────

/// One update attempt: the version it installs and where its installer is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Attempt {
    version: String,
    installer: PathBuf,
    /// Whether the user was told that this attempt did not finish
    /// (`unfinished_update_note`). Absent in records of 0.7.1's first runs.
    #[serde(default)]
    told: bool,
}

/// What the attempt record says at startup.
#[derive(Debug, PartialEq)]
enum Settled {
    /// No attempt on record.
    Nothing,
    /// The attempted version (or a later one) is running: it worked.
    Installed(String),
    /// An older version is still running: the attempt did not finish.
    Failed(String),
}

fn record_attempt(state: &Path, attempt: &Attempt) -> Result<()> {
    std::fs::create_dir_all(state).with_context(|| format!("create {}", state.display()))?;
    let text = serde_json::to_string(attempt).context("write the attempt record")?;
    let path = state.join(ATTEMPT_FILE);
    std::fs::write(&path, text).with_context(|| format!("write {}", path.display()))
}

fn read_attempt(state: &Path) -> Option<Attempt> {
    let text = std::fs::read_to_string(state.join(ATTEMPT_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Settle the attempt on record against the running version: a finished one
/// is forgotten, a failed one is kept so it is not tried again. Either way
/// its downloaded installer is deleted (an installer that is still running
/// cannot be, and is tried again on the next start). An unreadable record is
/// dropped.
fn settle_attempt(state: &Path, current: Version) -> Settled {
    let path = state.join(ATTEMPT_FILE);
    if !path.exists() {
        return Settled::Nothing;
    }
    let Some(attempt) = read_attempt(state) else {
        let _ = std::fs::remove_file(&path);
        return Settled::Nothing;
    };
    let Some(version) = parse_version(&attempt.version) else {
        let _ = std::fs::remove_file(&path);
        return Settled::Nothing;
    };
    let gone = match std::fs::remove_file(&attempt.installer) {
        Ok(()) => true,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    };
    if version <= current {
        if gone {
            let _ = std::fs::remove_file(&path);
        }
        Settled::Installed(attempt.version)
    } else {
        Settled::Failed(attempt.version)
    }
}

/// What to tell the user, once, when the last update did not finish: the
/// version it was for is still not running. The record notes that it was
/// told, so the next start stays quiet; it stays otherwise, so that version
/// is not tried again by itself. None when there is nothing to tell, and for
/// a copy that does not update itself. Reads one small file of the app's
/// own, no network.
pub fn unfinished_update_note() -> Option<String> {
    if !self_update_supported() {
        return None;
    }
    let version = untold_failure(&state_dir(), current_version())?;
    Some(format!(
        "The update to {version} did not finish, so AI Limits is still {}. It is not tried \
         again by itself: install {version} from github.com/{REPO}/releases.",
        env!("CARGO_PKG_VERSION")
    ))
}

/// The version of an attempt that did not finish and was not told yet,
/// marking it told. Pure on the folder for testing.
fn untold_failure(state: &Path, current: Version) -> Option<String> {
    let mut attempt = read_attempt(state)?;
    let version = parse_version(&attempt.version)?;
    if version <= current || attempt.told {
        return None;
    }
    attempt.told = true;
    if let Err(e) = record_attempt(state, &attempt) {
        warn!("auto-update: could not note that the failed update was told: {e:#}");
    }
    Some(attempt.version)
}

/// Whether `version` was attempted before and did not install: the record
/// names it and the running build is still older.
fn attempt_failed(state: &Path, version: &str, current: Version) -> bool {
    read_attempt(state).is_some_and(|a| {
        a.version == version && parse_version(&a.version).is_some_and(|v| v > current)
    })
}

/// What a failed handoff of 0.6.0 to 0.7.0 left in `temp`: the script, and
/// the installer of the version now running. Nothing else is touched.
fn remove_legacy_leftovers(temp: &Path, current: Version) {
    let _ = std::fs::remove_file(temp.join(LEGACY_SCRIPT));
    let (major, minor, patch) = current;
    let installer = installer_file_name(&format!("{major}.{minor}.{patch}"));
    let _ = std::fs::remove_file(temp.join(installer));
}

// ─── Handoff ─────────────────────────────────────────────────────

/// Where the Inno uninstall entry says the app is installed, if it exists.
/// Written by the installer itself (per-user key, `{AppId}_is1`), so it is
/// correct for a custom install directory too.
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
            None,
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
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
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

/// The installer, started directly: no shell and no script, so no path is
/// ever written into text a code page could mangle. Pure for testability.
fn installer_command(installer: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(installer);
    command
        .args(INSTALLER_SWITCHES)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
}

/// Start the downloaded installer for a silent, in-place upgrade, then exit.
///
/// The running exe holds a lock on its own file, so the app leaves at once:
/// the installer spends far longer unpacking itself than this process takes
/// to end, and `/CLOSEAPPLICATIONS` closes it should it still be there. The
/// installer then replaces the files in the directory its uninstall entry
/// names — the one this copy runs from (see `channel`) — and starts the app
/// from there (`RELAUNCH_SWITCH`).
#[cfg(target_os = "windows")]
fn launch_installer_and_exit(installer: &Path) -> Result<()> {
    installer_command(installer)
        .spawn()
        .context("start the installer")?;
    // The installer is running; leave so it can replace our files.
    std::process::exit(0);
}

#[cfg(not(target_os = "windows"))]
fn launch_installer_and_exit(_installer: &Path) -> Result<()> {
    // Non-Windows builds ship no installer; nothing to hand off to.
    anyhow::bail!("no installer handoff on this platform")
}

// ─── Small helpers ───────────────────────────────────────────────

/// Parse "v1.2.3" / "1.2.3" into a comparable tuple; any pre-release/build
/// suffix is ignored (releases ship plain `vX.Y.Z` tags).
fn parse_version(tag: &str) -> Option<Version> {
    let core = tag.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next().unwrap_or(core);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// The version string without a leading `v`, for display and the file name.
fn normalize(tag: &str) -> String {
    tag.trim().trim_start_matches(['v', 'V']).to_string()
}

/// Lower-case hex of `bytes`.
pub fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Bytes from hex (either case); None on an odd length or a non-hex digit.
pub fn hex_decode(text: &str) -> Option<Vec<u8>> {
    let (pairs, odd) = text.as_bytes().as_chunks::<2>();
    if !odd.is_empty() {
        return None;
    }
    let nibble = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    pairs
        .iter()
        .map(|&[high, low]| Some((nibble(high)? << 4) | nibble(low)?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    /// Paths an update has to survive: Cyrillic, `&`, `%`, brackets, spaces
    /// and the other characters a batch file reads as syntax.
    const AWKWARD_DIRS: &[&str] = &[
        "Вася Пупкін",
        "Tom&Jerry",
        "100% done",
        "dir (x86) [1]",
        "a^b!c;d,e=f",
        "Їжак & Co (100%)",
    ];

    /// A throwaway signing key, never the release key.
    fn test_key() -> Ed25519KeyPair {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
    }

    fn public(key: &Ed25519KeyPair) -> [u8; 32] {
        key.public_key().as_ref().try_into().unwrap()
    }

    fn sign(key: &Ed25519KeyPair, name: &str, bytes: &[u8]) -> String {
        let message = signed_message(name, digest(&SHA256, bytes).as_ref());
        let signature: [u8; 64] = key.sign(&message).as_ref().try_into().unwrap();
        encode_signature(&key_id(&public(key)), &signature)
    }

    /// A fresh folder under %TEMP% whose name holds `awkward`.
    fn temp_dir_named(awkward: &str, test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ailimits-updater-{test}-{}-{awkward}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn release(tag: &str, assets: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "tag_name": tag, "assets": assets })
    }

    fn asset(name: &str, digest: Option<&str>) -> serde_json::Value {
        let mut a = serde_json::json!({
            "name": name,
            "browser_download_url": format!("https://example.invalid/{name}"),
        });
        if let Some(d) = digest {
            a["digest"] = serde_json::Value::String(d.to_string());
        }
        a
    }

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
    fn hex_decodes_what_it_encodes_and_nothing_else() {
        let bytes = [0x00, 0x7f, 0x80, 0xff, 0x12, 0xab];
        assert_eq!(hex_decode(&hex_lower(&bytes)).unwrap(), bytes);
        assert_eq!(hex_decode("ABcd").unwrap(), [0xab, 0xcd]);
        assert_eq!(hex_decode(""), Some(vec![]));
        assert_eq!(hex_decode("abc"), None, "odd length");
        assert_eq!(hex_decode("zz"), None, "not hex");
        assert_eq!(hex_decode("ая"), None, "not ASCII");
    }

    // ─── Which copies update themselves (C-11) ───────────────────

    #[test]
    fn only_an_installer_copy_updates_itself() {
        assert_eq!(channel_of(false, true), Channel::Installer);
        assert_eq!(channel_of(true, false), Channel::Store);
        // A Store package is the Store's, wherever it seems to sit.
        assert_eq!(channel_of(true, true), Channel::Store);
        assert_eq!(channel_of(false, false), Channel::Unmanaged);
    }

    #[tokio::test]
    async fn a_copy_the_installer_did_not_place_stands_down_at_once() {
        // The test binary sits in target\, never in the installer's
        // directory: the loop must end before its first delay, so before
        // any request could be made (the first one waits 30 s).
        assert_ne!(channel(), Channel::Installer);
        let ran = tokio::time::timeout(
            Duration::from_millis(500),
            run(Arc::new(AtomicBool::new(true))),
        )
        .await;
        assert!(ran.is_ok(), "run() returned without waiting or asking");
    }

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

    // ─── What a release must carry (C-02) ────────────────────────

    #[test]
    fn a_newer_release_with_a_signed_installer_is_an_update() {
        let r = release(
            "v9.9.9",
            serde_json::json!([
                asset("ailimits-auth.exe", Some("sha256:00")),
                asset("AiLimits-Setup-9.9.9.exe", Some("sha256:abc123")),
                asset("AiLimits-Setup-9.9.9.exe.sig", None),
                asset("AiLimits-Portable-9.9.9.zip", Some("sha256:ff")),
            ]),
        );
        let u = release_update(&r, (0, 7, 0)).unwrap().expect("an update");
        assert_eq!(u.version, "9.9.9");
        assert_eq!(
            u.installer_url,
            "https://example.invalid/AiLimits-Setup-9.9.9.exe"
        );
        assert_eq!(u.sha256, "abc123");
        assert_eq!(
            u.signature_url,
            "https://example.invalid/AiLimits-Setup-9.9.9.exe.sig"
        );
    }

    #[test]
    fn the_same_or_an_older_release_is_no_update() {
        let r = release("v0.7.0", serde_json::json!([]));
        assert!(release_update(&r, (0, 7, 0)).unwrap().is_none());
        assert!(release_update(&r, (0, 7, 1)).unwrap().is_none());
    }

    #[test]
    fn an_unsigned_or_unverifiable_release_is_refused() {
        let installer = asset("AiLimits-Setup-9.9.9.exe", Some("sha256:abc"));
        let signature = asset("AiLimits-Setup-9.9.9.exe.sig", None);

        let unsigned = release("v9.9.9", serde_json::json!([installer.clone()]));
        let e = release_update(&unsigned, (0, 7, 0)).err().expect("refused");
        assert!(format!("{e:#}").contains("unsigned"), "{e:#}");

        let no_digest = release(
            "v9.9.9",
            serde_json::json!([asset("AiLimits-Setup-9.9.9.exe", None), signature.clone()]),
        );
        assert!(release_update(&no_digest, (0, 7, 0)).is_err());

        // Only the installer named for the tag is taken: not another .exe,
        // and not an installer for some other version.
        for other in [
            "ailimits.exe",
            "ailimits-auth.exe",
            "AiLimits-Setup-0.7.1.exe",
            "evil-AiLimits-Setup-9.9.9.exe",
            "AiLimits-Setup-9.9.9.exe.sig",
        ] {
            let r = release(
                "v9.9.9",
                serde_json::json!([asset(other, Some("sha256:abc")), signature.clone()]),
            );
            assert!(release_update(&r, (0, 7, 0)).is_err(), "{other} taken");
        }

        // The name is matched without regard to case.
        let shouty = release(
            "v9.9.9",
            serde_json::json!([
                asset("ailimits-setup-9.9.9.EXE", Some("sha256:abc")),
                asset("AILIMITS-SETUP-9.9.9.EXE.SIG", None)
            ]),
        );
        assert!(release_update(&shouty, (0, 7, 0)).unwrap().is_some());
    }

    #[test]
    fn a_signed_installer_verifies_and_any_change_does_not() {
        let key = test_key();
        let keys = [public(&key)];
        let name = "AiLimits-Setup-9.9.9.exe";
        let bytes = b"an installer".as_slice();
        let signature = sign(&key, name, bytes);

        assert!(verify_with_keys(&keys, name, bytes, &signature).is_ok());
        // Another file.
        assert!(verify_with_keys(&keys, name, b"an installeR", &signature).is_err());
        // The same file under another version's name.
        assert!(verify_with_keys(&keys, "AiLimits-Setup-9.9.10.exe", bytes, &signature).is_err());
        // Only the signer's own key verifies it.
        let other = public(&test_key());
        assert!(verify_with_keys(&[other], name, bytes, &signature).is_err());
        // A key this build does not know.
        let stranger = test_key();
        let foreign = sign(&stranger, name, bytes);
        let e = verify_with_keys(&keys, name, bytes, &foreign).unwrap_err();
        assert!(format!("{e:#}").contains("does not trust"), "{e:#}");
        // A flipped bit in the signature.
        let mut tampered = signature.clone().into_bytes();
        let last = tampered.len() - 2;
        tampered[last] = if tampered[last] == b'0' { b'1' } else { b'0' };
        let tampered = String::from_utf8(tampered).unwrap();
        assert!(verify_with_keys(&keys, name, bytes, &tampered).is_err());
    }

    #[test]
    fn a_signature_file_is_one_line_of_a_known_format() {
        let key = test_key();
        let signature = sign(&key, "f", b"x");
        assert!(signature.is_ascii());
        assert_eq!(signature.lines().count(), 1);
        assert!(signature.starts_with("ailimits-signature-v1 "));
        assert!(ReleaseSignature::parse(&signature).is_ok());
        // Whitespace around it (a CRLF from an editor) does not matter.
        assert!(ReleaseSignature::parse(&format!("  {}\r\n", signature.trim())).is_ok());

        for bad in [
            "",
            "ailimits-signature-v1",
            "ailimits-signature-v2 0011223344556677 00",
            "ailimits-signature-v1 00112233 00",
            "ailimits-signature-v1 0011223344556677 zz",
            "minisign signature",
        ] {
            assert!(ReleaseSignature::parse(bad).is_err(), "{bad:?} parsed");
        }
        let extra = format!("{} trailing", signature.trim());
        assert!(ReleaseSignature::parse(&extra).is_err());
    }

    #[test]
    fn the_built_in_release_key_is_a_real_key() {
        assert!(!RELEASE_KEYS.is_empty());
        for key in RELEASE_KEYS {
            assert_ne!(key, &[0u8; 32], "a placeholder, not a key");
            // A signature by some other key is refused under it, so the key
            // at least parses as a point the verifier accepts to try.
            let stranger = test_key();
            let foreign = sign(&stranger, "f", b"x");
            assert!(verify_release("f", b"x", &foreign).is_err());
        }
        let ids: std::collections::HashSet<_> = RELEASE_KEYS.iter().map(key_id).collect();
        assert_eq!(ids.len(), RELEASE_KEYS.len(), "key ids are unique");
        // The id the comment beside the key gives.
        assert_eq!(hex_lower(&key_id(&RELEASE_KEYS[0])), "1507bd92b1a4bb7b");
    }

    // ─── The handoff (C-01) ──────────────────────────────────────

    #[test]
    fn the_installer_starts_directly_whatever_the_path() {
        for awkward in AWKWARD_DIRS {
            let installer = Path::new(r"C:\Users")
                .join(awkward)
                .join(r"AppData\Local\Temp")
                .join("AiLimits-Setup-9.9.9.exe");
            let command = installer_command(&installer);
            // The installer itself is the program — no cmd.exe, no script —
            // and its path is handed over as it is, UTF-16 to CreateProcessW.
            assert_eq!(command.get_program(), installer.as_os_str(), "{awkward}");
            let args: Vec<&str> = command
                .get_args()
                .map(|a| a.to_str().expect("an ASCII switch"))
                .collect();
            assert_eq!(args, INSTALLER_SWITCHES, "{awkward}");
            assert!(args.iter().all(|a| a.is_ascii() && a.starts_with('/')));
            assert!(
                args.iter().all(|a| !a.contains(awkward)),
                "no path in the switches"
            );
        }
    }

    #[test]
    fn the_upgrade_is_silent_closes_the_app_and_brings_it_back() {
        for switch in [
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/CLOSEAPPLICATIONS",
        ] {
            assert!(INSTALLER_SWITCHES.contains(&switch), "{switch}");
        }
        assert!(INSTALLER_SWITCHES.contains(&RELAUNCH_SWITCH));
    }

    #[test]
    fn the_installer_relaunches_on_the_switch_the_updater_passes() {
        // installer/ailimits.iss must start the app again on exactly this
        // switch, in a silent run only: without it a self-update would leave
        // the widget closed.
        let iss = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("installer/ailimits.iss"),
        )
        .expect("the installer script");
        let (name, value) = RELAUNCH_SWITCH
            .trim_start_matches('/')
            .split_once('=')
            .unwrap();
        assert!(
            iss.contains(&format!("ExpandConstant('{{param:{name}|0}}') = '{value}'")),
            "ailimits.iss reads {RELAUNCH_SWITCH}"
        );
        assert!(iss.contains("WizardSilent"), "a silent run only");
        let relaunch = iss
            .lines()
            .find(|l| l.contains("Check: RelaunchAfterUpdate"))
            .expect("a [Run] entry for the relaunch");
        assert!(relaunch.contains(r#"Filename: "{app}\{#AppExe}""#));
        assert!(relaunch.contains("nowait"), "the installer must not wait");
        assert!(
            !relaunch.contains("skipifsilent"),
            "it is for the silent run"
        );
    }

    // ─── Attempts (C-01) ─────────────────────────────────────────

    #[test]
    fn a_finished_attempt_is_forgotten_and_its_installer_deleted() {
        for (i, awkward) in AWKWARD_DIRS.iter().enumerate() {
            let state = temp_dir_named(awkward, &format!("ok{i}"));
            let installer = state.join("AiLimits-Setup-0.7.1.exe");
            std::fs::write(&installer, b"stub").unwrap();
            let attempt = Attempt {
                version: "0.7.1".into(),
                installer: installer.clone(),
                told: false,
            };
            record_attempt(&state, &attempt).unwrap();
            assert_eq!(read_attempt(&state), Some(attempt), "{awkward}");

            assert_eq!(
                settle_attempt(&state, (0, 7, 1)),
                Settled::Installed("0.7.1".into())
            );
            assert!(!installer.exists(), "{awkward}: installer deleted");
            assert!(!state.join(ATTEMPT_FILE).exists(), "{awkward}: record gone");
            assert_eq!(settle_attempt(&state, (0, 7, 1)), Settled::Nothing);
            std::fs::remove_dir_all(&state).unwrap();
        }
    }

    #[test]
    fn a_failed_attempt_is_kept_and_not_tried_again() {
        for (i, awkward) in AWKWARD_DIRS.iter().enumerate() {
            let state = temp_dir_named(awkward, &format!("fail{i}"));
            let installer = state.join("AiLimits-Setup-0.7.2.exe");
            std::fs::write(&installer, b"stub").unwrap();
            record_attempt(
                &state,
                &Attempt {
                    version: "0.7.2".into(),
                    installer: installer.clone(),
                    told: false,
                },
            )
            .unwrap();

            // Still on 0.7.1 at the next start: the upgrade did not happen.
            assert_eq!(
                settle_attempt(&state, (0, 7, 1)),
                Settled::Failed("0.7.2".into())
            );
            assert!(!installer.exists(), "{awkward}: the 4 MB do not stay");
            assert!(state.join(ATTEMPT_FILE).exists(), "{awkward}: remembered");
            assert!(attempt_failed(&state, "0.7.2", (0, 7, 1)));
            // A later release is tried as usual.
            assert!(!attempt_failed(&state, "0.7.3", (0, 7, 1)));
            // Once it runs (installed by hand), the record is settled.
            assert_eq!(
                settle_attempt(&state, (0, 7, 2)),
                Settled::Installed("0.7.2".into())
            );
            assert!(!attempt_failed(&state, "0.7.2", (0, 7, 2)));
            std::fs::remove_dir_all(&state).unwrap();
        }
    }

    /// An update that did not finish is told once: the record notes it,
    /// stays (the version is still not tried again), and a record written
    /// before the note existed reads as not told.
    #[test]
    fn a_failed_update_is_told_once() {
        let state = temp_dir_named("Вася & (100%)", "told");
        std::fs::write(
            state.join(ATTEMPT_FILE),
            r#"{"version":"0.7.2","installer":"C:\\x\\AiLimits-Setup-0.7.2.exe"}"#,
        )
        .unwrap();
        assert_eq!(untold_failure(&state, (0, 7, 1)), Some("0.7.2".into()));
        assert_eq!(untold_failure(&state, (0, 7, 1)), None, "told once");
        assert!(
            attempt_failed(&state, "0.7.2", (0, 7, 1)),
            "still not retried"
        );
        // A finished one has nothing to tell.
        record_attempt(
            &state,
            &Attempt {
                version: "0.7.2".into(),
                installer: state.join("x.exe"),
                told: false,
            },
        )
        .unwrap();
        assert_eq!(untold_failure(&state, (0, 7, 2)), None);
        std::fs::remove_dir_all(&state).unwrap();
    }

    /// An update installs only once the user has been away five minutes.
    #[test]
    fn an_update_waits_for_the_user_to_be_away() {
        assert!(!away(0));
        assert!(!away(IDLE_BEFORE_INSTALL_SECS - 1));
        assert!(away(IDLE_BEFORE_INSTALL_SECS));
        assert!(away(u64::MAX));
    }

    #[test]
    fn no_record_or_a_broken_one_means_nothing_failed() {
        let state = temp_dir_named("plain", "none");
        assert_eq!(settle_attempt(&state, (0, 7, 1)), Settled::Nothing);
        assert!(!attempt_failed(&state, "0.7.2", (0, 7, 1)));

        for broken in ["", "{", "{\"version\":\"x\",\"installer\":\"y\"}", "[]"] {
            std::fs::write(state.join(ATTEMPT_FILE), broken).unwrap();
            assert!(!attempt_failed(&state, "0.7.2", (0, 7, 1)), "{broken:?}");
            assert_eq!(settle_attempt(&state, (0, 7, 1)), Settled::Nothing);
            assert!(!state.join(ATTEMPT_FILE).exists(), "{broken:?} dropped");
        }
        std::fs::remove_dir_all(&state).unwrap();
    }

    #[test]
    fn a_record_can_be_written_into_a_folder_that_does_not_exist_yet() {
        let root = temp_dir_named("Вася & (100%)", "mkdir");
        let state = root.join("AiLimits");
        let attempt = Attempt {
            version: "9.9.9".into(),
            installer: root.join("Їжак").join("AiLimits-Setup-9.9.9.exe"),
            told: false,
        };
        record_attempt(&state, &attempt).unwrap();
        assert_eq!(read_attempt(&state), Some(attempt));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn only_the_old_handoff_leftovers_are_cleaned() {
        let temp = temp_dir_named("Tom&Jerry (Вася) 100%", "legacy");
        let script = temp.join(LEGACY_SCRIPT);
        let stale = temp.join("AiLimits-Setup-0.7.1.exe");
        let newer = temp.join("AiLimits-Setup-0.7.2.exe");
        let other = temp.join("something-else.cmd");
        for f in [&script, &stale, &newer, &other] {
            std::fs::write(f, b"x").unwrap();
        }
        remove_legacy_leftovers(&temp, (0, 7, 1));
        assert!(!script.exists());
        assert!(!stale.exists(), "the installer of the running version");
        assert!(newer.exists(), "not the running version's: left alone");
        assert!(other.exists(), "not ours: left alone");
        // Nothing there is not an error.
        remove_legacy_leftovers(&temp, (0, 7, 1));
        std::fs::remove_dir_all(&temp).unwrap();
    }
}
