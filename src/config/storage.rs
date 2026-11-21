// config/storage.rs — reading and writing config.toml.
// Format documentation: docs/en/CONFIG.md.

use super::schema::{Config, PendingNotice};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Config path: %APPDATA%\AiLimits\config.toml.
///
/// Debug builds only: `AILIMITS_CONFIG_DIR` puts the config (and the cache
/// and log beside it) in another folder, so a fresh install or an upgrade can
/// be tried without touching the real one. Release builds ignore it.
pub fn config_path() -> PathBuf {
    #[cfg(debug_assertions)]
    if let Some(dir) = std::env::var_os("AILIMITS_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir).join("config.toml");
    }
    dirs::config_dir()
        // Fall back to the current directory if %APPDATA% is unavailable.
        .unwrap_or_else(|| PathBuf::from("."))
        .join("AiLimits")
        .join("config.toml")
}

/// Load the config, or create the default one on first launch. The same as
/// `load()`, without the word on what went wrong (still in the log, and
/// still keeping a file that could not be used from being saved over).
pub async fn load_or_default() -> Result<Config> {
    Ok(load().await.config)
}

/// A config as loaded at startup, and what went wrong with the file, if
/// anything did.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub config: Config,
    /// None: the file was read and used (or created on a first start).
    pub problem: Option<LoadProblem>,
}

/// Why the config file was not used as it stands. Each case keeps the file:
/// it is never overwritten with defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadProblem {
    /// The file is there but could not be read, even after retrying for
    /// about two seconds (a lock held by a scanner or a backup, the folder's
    /// permissions). It is left exactly as it is and nothing is saved over
    /// it until the app restarts; meanwhile the app runs on a config in
    /// memory that reads nothing and asks nothing.
    Unreadable { error: String },
    /// The file could not be understood (broken TOML, or bytes that are not
    /// text). It was set aside under a name of its own, so an earlier one is
    /// never replaced, and the app starts as on a new install.
    SetAside { backup: PathBuf, error: String },
    /// The file could not be understood, nor set aside: left as it is,
    /// nothing saved over it until the app restarts, as for `Unreadable`.
    Unparsable { error: String },
}

impl LoadProblem {
    /// Whether saving is off for this session (the file is kept as it is).
    pub fn saving_off(&self) -> bool {
        !matches!(self, LoadProblem::SetAside { .. })
    }

    /// The one message for the user about it.
    pub fn message(&self) -> String {
        match self {
            LoadProblem::Unreadable { .. } => "AI Limits could not read its settings file (config.toml), so it is running with nothing switched on. The file is kept as it is: changes made now are not saved. Restart AI Limits to try again.".to_string(),
            LoadProblem::SetAside { backup, .. } => format!(
                "AI Limits could not understand its settings file (config.toml) and set it aside as {}. It starts over with nothing switched on; the old settings are still in that file.",
                backup
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| backup.display().to_string())
            ),
            LoadProblem::Unparsable { .. } => "AI Limits could not understand its settings file (config.toml), and could not set it aside, so it is running with nothing switched on. The file is kept as it is: changes made now are not saved. Fix or remove it, then restart AI Limits.".to_string(),
        }
    }
}

/// How long a read that fails is retried before the file counts as
/// unreadable: about two seconds in all. A scanner or a backup that holds
/// the file (often right after an installer replaced the exe) lets go
/// within that.
const READ_RETRIES: [Duration; 5] = [
    Duration::from_millis(100),
    Duration::from_millis(200),
    Duration::from_millis(300),
    Duration::from_millis(500),
    Duration::from_millis(900),
];

/// Load the config from its usual place.
pub async fn load() -> Loaded {
    load_from(&config_path()).await
}

/// Load the config at `path`; create it on a first start.
///
/// The widget is a GUI app: an error out of here would end it with nothing
/// on screen, so every failure becomes a config that reads nothing and a
/// `LoadProblem`. A read that fails is never a parse error: the file is not
/// renamed or replaced for it, and nothing is saved over it this session.
pub async fn load_from(path: &Path) -> Loaded {
    load_with(path, &READ_RETRIES).await
}

async fn load_with(path: &Path, retries: &[Duration]) -> Loaded {
    let bytes = match read_with_retries(path, retries).await {
        FileRead::Bytes(bytes) => bytes,
        FileRead::Missing => return first_start(path).await,
        FileRead::Failed(error) => {
            tracing::warn!("config read error, the file is kept as it is and not saved over this session: {error}");
            hold_saving(path);
            return Loaded {
                config: in_memory(),
                problem: Some(LoadProblem::Unreadable { error }),
            };
        }
    };
    match parse(&bytes) {
        Ok((config, origin)) => {
            let mut config = config;
            migrate(&mut config, origin);
            Loaded {
                config,
                problem: None,
            }
        }
        Err(error) => {
            tracing::warn!("config parse error: {error}");
            // Whatever the file allowed is unknown now, so nothing is read
            // until the user chooses again: the safe side of consent.
            match set_aside(path).await {
                Ok(backup) => {
                    tracing::warn!("set the unusable config aside as {}", backup.display());
                    let mut config = Config::default();
                    settle_reads(&mut config, Origin::FreshInstall);
                    Loaded {
                        config,
                        problem: Some(LoadProblem::SetAside { backup, error }),
                    }
                }
                Err(e) => {
                    tracing::warn!("could not set the unusable config aside ({e}); it is kept as it is and not saved over this session");
                    hold_saving(path);
                    Loaded {
                        config: in_memory(),
                        problem: Some(LoadProblem::Unparsable { error }),
                    }
                }
            }
        }
    }
}

/// A first start: the default config, nothing allowed, the welcome owed.
/// Writing it is BEST-EFFORT: an unwritable %APPDATA% (full disk, locked-down
/// or roaming profile, AV quarantine) must not stop the widget.
async fn first_start(path: &Path) -> Loaded {
    let mut default = Config::default();
    settle_reads(&mut default, Origin::FreshInstall);
    match save_to(&default, path).await {
        Ok(()) => tracing::info!("Created default config at {}", path.display()),
        Err(e) => tracing::warn!("could not write default config ({e}); running in-memory"),
    }
    Loaded {
        config: default,
        problem: None,
    }
}

/// The config a session runs on when the file could not be used and is
/// kept: nothing allowed, nothing switched on, and no welcome either (what
/// the user would choose there could not be saved).
fn in_memory() -> Config {
    let mut config = Config::default();
    settle_reads(&mut config, Origin::FreshInstall);
    config.reads.pending = PendingNotice::None;
    config
}

/// What reading the config file gave.
enum FileRead {
    Bytes(Vec<u8>),
    Missing,
    Failed(String),
}

/// Read the file, retrying a failed read after each delay in turn.
async fn read_with_retries(path: &Path, retries: &[Duration]) -> FileRead {
    let mut delays = retries.iter();
    loop {
        match tokio::fs::read(path).await {
            Ok(bytes) => return FileRead::Bytes(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return FileRead::Missing,
            Err(e) => match delays.next() {
                Some(delay) => tokio::time::sleep(*delay).await,
                None => return FileRead::Failed(e.to_string()),
            },
        }
    }
}

/// The config in the file's bytes, and where it came from. Text in UTF-8
/// (with or without a BOM) or UTF-16 with a BOM, as Notepad saves it.
fn parse(bytes: &[u8]) -> std::result::Result<(Config, Origin), String> {
    let content = decode(bytes)?;
    let config = toml::from_str::<Config>(&content).map_err(|e| e.to_string())?;
    Ok(match origin_of(&content) {
        Some(origin) => (config, origin),
        // Neither [reads] nor [[providers]]: an empty file, or one written by
        // something else before the first start. Every version that read
        // without asking wrote its providers, so this is no upgrade.
        None => (Config::default(), Origin::FreshInstall),
    })
}

/// Config file bytes as text.
fn decode(bytes: &[u8]) -> std::result::Result<String, String> {
    let utf16 = |be: bool, rest: &[u8]| -> std::result::Result<String, String> {
        let (pairs, odd) = rest.as_chunks::<2>();
        if !odd.is_empty() {
            return Err("UTF-16 text with an odd number of bytes".to_string());
        }
        let units = pairs.iter().map(|&pair| {
            if be {
                u16::from_be_bytes(pair)
            } else {
                u16::from_le_bytes(pair)
            }
        });
        char::decode_utf16(units)
            .collect::<std::result::Result<String, _>>()
            .map_err(|_| "UTF-16 text that does not decode".to_string())
    };
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        String::from_utf8(rest.to_vec()).map_err(|_| "text that is not UTF-8".to_string())
    } else if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        utf16(false, rest)
    } else if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        utf16(true, rest)
    } else {
        String::from_utf8(bytes.to_vec())
            .map_err(|_| "text that is not UTF-8 (saved in another encoding?)".to_string())
    }
}

/// Move an unusable config out of the way under a name no earlier one has:
/// `config.toml.<date>-<time>.corrupt` (UTC: the local clock would pull in
/// the time-zone APIs for a file name), with `-2`, `-3`... when that is
/// taken. Never replaces a file.
async fn set_aside(path: &Path) -> std::io::Result<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let base = path.as_os_str().to_owned();
    for n in 1..=100u32 {
        let mut name = base.clone();
        if n == 1 {
            name.push(format!(".{stamp}.corrupt"));
        } else {
            name.push(format!(".{stamp}-{n}.corrupt"));
        }
        let backup = PathBuf::from(name);
        if tokio::fs::try_exists(&backup).await.unwrap_or(true) {
            continue;
        }
        tokio::fs::rename(path, &backup).await?;
        return Ok(backup);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no free name to set the config aside under",
    ))
}

/// The config as it stands on disk, read without changing anything: no
/// file is created, renamed or written, and nothing is retried. For the
/// diagnostics of `ailimits-auth status`, and for its commands that change
/// the config (which then refuse a file they cannot read instead of
/// replacing it).
#[derive(Debug, Clone)]
pub enum Peek {
    /// No config yet: the widget creates it on its first start.
    Missing,
    /// The config, brought up to date in memory as the widget would.
    Found(Box<Config>),
    /// The file is there but could not be read.
    Unreadable(String),
    /// The file could not be understood.
    Unparsable(String),
}

/// Look at the config at `path` without changing anything.
pub fn peek(path: &Path) -> Peek {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Peek::Missing,
        Err(e) => return Peek::Unreadable(e.to_string()),
    };
    match parse(&bytes) {
        Ok((mut config, origin)) => {
            migrate(&mut config, origin);
            Peek::Found(Box::new(config))
        }
        Err(e) => Peek::Unparsable(e),
    }
}

/// Config files that must not be written this session: one that could not
/// be read, or not set aside, stays exactly as it is until the app restarts.
static HELD: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

fn hold_saving(path: &Path) {
    let mut held = HELD.lock().unwrap_or_else(|p| p.into_inner());
    if !held.iter().any(|p| p == path) {
        held.push(path.to_path_buf());
    }
}

/// Whether saving to `path` is off for this session (see `LoadProblem`).
pub fn saving_held(path: &Path) -> bool {
    HELD.lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .any(|p| p == path)
}

/// Bring a loaded config up to date: renamed providers, providers added
/// since it was written, and the `[reads]` state its origin calls for.
pub(crate) fn migrate(config: &mut Config, origin: Origin) {
    // Migration: the Gemini provider became Antigravity (2026-07) — rename
    // the config id in place so the entry keeps its settings and the
    // append-missing pass below does not create a duplicate.
    for p in config.providers.iter_mut() {
        if p.id == "gemini" {
            p.id = "antigravity".to_string();
        }
    }

    // Migration: ensure every default provider exists so newly-added ones
    // appear in the menu for users with an older config. A provider the
    // file does not have is one the version that wrote it did not know (it
    // writes its whole list back), so it never read it: it is appended
    // switched off, and ticking it asks. Existing entries are left untouched.
    let defaults = Config::default().providers;
    for mut d in defaults {
        if !config.providers.iter().any(|p| p.id == d.id) {
            d.enabled = false;
            config.providers.push(d);
        }
    }

    // After the provider passes above, so an upgrade allows exactly the
    // providers that version read: its enabled ones, none of the appended.
    settle_reads(config, origin);
}

/// Where a loaded config came from, as far as reading consent goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    /// No usable config: nothing has been allowed yet.
    FreshInstall,
    /// A config written before `[reads]` existed (0.7.0 and older). Those
    /// versions read every enabled provider's sign-in without asking.
    Upgrade,
    /// A config that already records what was allowed.
    Current,
}

/// Where config text came from: `[reads]` makes it current, `[[providers]]`
/// without it an older version's, neither of them nothing to go by (None).
/// Asked of text that already parsed as a `Config`, so it parses again.
fn origin_of(content: &str) -> Option<Origin> {
    let table = content.parse::<toml::Table>().ok()?;
    if table.contains_key("reads") {
        Some(Origin::Current)
    } else if table
        .get("providers")
        .and_then(|p| p.as_array())
        .is_some_and(|a| !a.is_empty())
    {
        Some(Origin::Upgrade)
    } else {
        None
    }
}

/// Put the `[reads]` section in the state the origin calls for.
///
/// - Fresh install: nothing allowed, the welcome is owed.
/// - Upgrade: the providers that were reading keep reading (every enabled
///   one, as the old version did), and the one-time notice is owed. Not
///   "the providers with cached data": a cache entry expires with its reset,
///   so someone back after a week would find providers silently gone.
/// - Current: left exactly as the user left it.
pub(crate) fn settle_reads(config: &mut Config, origin: Origin) {
    match origin {
        Origin::FreshInstall => {
            config.reads = Default::default();
            config.reads.pending = super::schema::PendingNotice::Welcome;
            // Nothing pasted before on a fresh install: nothing to settle.
            config.reads.sources_settled = true;
            // Switched off as well as not allowed: a version that knows no
            // [reads] (a downgrade to 0.7.0) then reads nothing either.
            // Allowing a provider switches it on.
            for p in config.providers.iter_mut() {
                p.enabled = false;
            }
        }
        Origin::Upgrade => {
            let enabled: Vec<String> = config
                .providers
                .iter()
                .filter(|p| p.enabled)
                .map(|p| p.id.clone())
                .collect();
            for id in enabled {
                config.allow_sign_in(&id);
            }
            config.reads.pending = super::schema::PendingNotice::UpdateNotice;
        }
        Origin::Current => {}
    }
}

/// Save the config to its default path.
pub async fn save(config: &Config) -> Result<()> {
    save_to(config, &config_path()).await
}

/// Save the config to a specific path — the seam the background saver and its
/// tests build on. Refused, with the file untouched, while saving to that
/// path is held for the session (`LoadProblem::saving_off`).
pub async fn save_to(config: &Config, path: &Path) -> Result<()> {
    if saving_held(path) {
        anyhow::bail!(
            "saving is off this session: {} could not be used when the app started and is kept as it is",
            path.display()
        );
    }
    // The directory may have vanished between runs.
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let content = toml::to_string_pretty(config)?;
    atomic_write(path, content.as_bytes()).await?;
    Ok(())
}

/// How long shutdown waits for the writer to finish its final save. A wedged
/// disk must delay exit, not prevent it.
const SHUTDOWN_FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// A message for the config writer task.
#[derive(Clone)]
pub enum SaveMsg {
    /// Persist this config and keep running.
    Write(Config),
    /// Persist this config, then stop. Always the writer's last action.
    Final(Config),
}

/// The single background config writer. Every write goes through it, so two
/// writes can never interleave or land out of order.
pub struct ConfigSaver {
    tx: std::sync::Arc<tokio::sync::watch::Sender<Option<SaveMsg>>>,
    task: tokio::task::JoinHandle<()>,
}

/// A cheap clone of the writer's sending end, for the app's hot paths.
#[derive(Clone)]
pub struct ConfigSaverHandle {
    tx: std::sync::Arc<tokio::sync::watch::Sender<Option<SaveMsg>>>,
}

impl ConfigSaverHandle {
    /// Queue a config for persistence. Fire-and-forget: a burst coalesces to
    /// the latest value on the one-slot channel.
    ///
    /// If a `Final` has already been queued (`shutdown` is under way), this
    /// is a no-op instead: `send_if_modified` checks-and-sets atomically
    /// under the channel's own lock, so a `Write` landing concurrently with
    /// `shutdown`'s `Final` can never overwrite it. Without this, a `Write`
    /// that slipped into the slot after `Final` but before the writer task
    /// picked it up would make the task loop back for more instead of
    /// breaking, and the `JoinHandle` `shutdown` awaits would never resolve.
    pub fn save(&self, config: Config) {
        self.tx.send_if_modified(|slot| match slot {
            Some(SaveMsg::Final(_)) => false,
            _ => {
                *slot = Some(SaveMsg::Write(config));
                true
            }
        });
    }
}

impl ConfigSaver {
    /// A sending handle for the app's save call sites.
    pub fn handle(&self) -> ConfigSaverHandle {
        ConfigSaverHandle {
            tx: self.tx.clone(),
        }
    }

    /// Persist `config` and BLOCK until the writer has finished and stopped.
    ///
    /// This is the shutdown path. It deliberately does not write the file
    /// itself: a direct write would be a second writer racing whatever the
    /// task already has in flight, and a slower in-flight write of an older
    /// snapshot could rename over the newest config. Handing the task a
    /// `Final` message instead keeps a single writer, and awaiting the task
    /// to completion proves the newest config reached disk before the
    /// process exits. That guarantee holds only when the wait succeeds
    /// within `SHUTDOWN_FLUSH_TIMEOUT`: on timeout the `JoinHandle` is
    /// simply dropped rather than cancelled, so a wedged write can still
    /// land on disk after this function has already returned.
    ///
    /// The wait happens on a freshly spawned OS thread rather than via
    /// `rt.block_on` on the calling thread directly: the caller is usually a
    /// plain thread with no Tokio context (the app's event loop), where
    /// `block_on` is fine, but it can also already be running inside that
    /// same runtime (as in tests that call `shutdown` from `rt.block_on`),
    /// where a direct `block_on` would panic with "Cannot start a runtime
    /// from within a runtime". A fresh thread never carries that context, so
    /// entering the runtime there is always sound; joining it back is a
    /// plain OS-level wait that works from either kind of caller.
    pub fn shutdown(self, config: Config, rt: &tokio::runtime::Handle) {
        let ConfigSaver { tx, task } = self;
        let _ = tx.send(Some(SaveMsg::Final(config)));
        let rt = rt.clone();
        let waited = std::thread::spawn(move || {
            // `tokio::time::timeout` must be constructed AFTER the runtime is
            // entered (it looks up the time driver on creation), so it lives
            // inside the async block that `block_on` drives rather than as a
            // pre-built future passed into `block_on`.
            rt.block_on(async move { tokio::time::timeout(SHUTDOWN_FLUSH_TIMEOUT, task).await })
        })
        .join();
        match waited {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(e))) => tracing::warn!("config writer task failed: {e}"),
            Ok(Err(_)) => tracing::warn!("final config save timed out"),
            Err(_) => {
                tracing::warn!("config writer thread panicked while waiting for the final save")
            }
        }
    }
}

/// Spawn the single background config-saver task. Sending a config overwrites a
/// one-slot `watch` channel, so a burst of changes coalesces to the LATEST (the
/// newest setting always wins) AND the queue is bounded to one entry — a
/// slow/stalled disk cannot grow it. This also serialises writes, closing the
/// race where two independently-spawned saves could atomic-rename out of order
/// and drop the newest config.
///
/// While saving to `path` is held for the session (the file could not be
/// used at startup and is kept as it is), the saver writes nothing; the
/// shutdown handshake still completes.
pub fn spawn_saver(handle: &tokio::runtime::Handle, path: PathBuf) -> ConfigSaver {
    let (tx, mut rx) = tokio::sync::watch::channel::<Option<SaveMsg>>(None);
    let task = handle.spawn(async move {
        let mut told = false;
        while rx.changed().await.is_ok() {
            let msg = rx.borrow_and_update().clone();
            let (cfg, last) = match msg {
                Some(SaveMsg::Write(cfg)) => (cfg, false),
                Some(SaveMsg::Final(cfg)) => (cfg, true),
                // The initial `None` seed carries no config.
                None => continue,
            };
            if saving_held(&path) {
                if !told {
                    tracing::info!(
                        "config not saved: the file is kept as it was found this session"
                    );
                    told = true;
                }
            } else if let Err(e) = save_to(&cfg, &path).await {
                tracing::warn!("config save failed: {e}");
            }
            if last {
                break;
            }
        }
    });
    ConfigSaver {
        tx: std::sync::Arc::new(tx),
        task,
    }
}

/// Write bytes to `path` atomically: write a uniquely-named sibling temp
/// file, then rename it over the target (std/tokio rename on Windows uses
/// MOVEFILE_REPLACE_EXISTING). A plain `fs::write` truncates first, so a
/// crash mid-write would leave an empty/corrupt file; with the rename the
/// old contents survive any interruption. Shared by the config and the
/// provider cache.
pub async fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    // Unique per write: two overlapping background saves must not share a
    // temp file (the second writer would garble the first one's rename).
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp{n}"));
    let tmp = PathBuf::from(tmp);
    tokio::fs::write(&tmp, bytes).await?;
    match tokio::fs::rename(&tmp, path).await {
        Ok(()) => Ok(()),
        Err(e) => {
            // Don't leave the temp file behind on a failed rename.
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{ModelPart, PendingNotice};

    /// What loading does with config text, minus the disk (the disk is
    /// tested below, through `load_with`).
    fn load_text(content: &str) -> Config {
        let (mut config, origin) = parse(content.as_bytes()).expect("parses");
        migrate(&mut config, origin);
        config
    }

    /// A fresh folder under %TEMP% with the config path in it, removed when
    /// dropped.
    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Dir {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "ailimits-config-{tag}-{}-{}",
                std::process::id(),
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }

        fn config(&self) -> PathBuf {
            self.0.join("config.toml")
        }

        /// Every file name in the folder, sorted.
        fn names(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(&self.0)
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const QUICK: [Duration; 3] = [
        Duration::from_millis(20),
        Duration::from_millis(20),
        Duration::from_millis(20),
    ];

    fn block<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f)
    }

    /// A current config, as a running 0.7.1 writes it.
    fn current_text() -> String {
        format!(
            "{CONFIG_0_7_0}\n[reads]\nsign_in = [\"claude\", \"codex\"]\ntool_settings = true\n"
        )
    }

    #[test]
    fn a_missing_config_is_created_as_a_fresh_install() {
        let dir = Dir::new("missing");
        let loaded = block(load_with(&dir.config(), &QUICK));
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.config, Config::fresh_install());
        assert_eq!(dir.names(), ["config.toml"]);
        // What was written reads back as the same, now as a current config.
        let again = block(load_with(&dir.config(), &QUICK));
        assert_eq!(again.problem, None);
        assert_eq!(again.config, loaded.config);
    }

    #[test]
    fn a_readable_config_loads_as_it_was_left() {
        let dir = Dir::new("current");
        std::fs::write(dir.config(), current_text()).unwrap();
        let loaded = block(load_with(&dir.config(), &QUICK));
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.config.reads.sign_in, ["claude", "codex"]);
        assert!(loaded.config.reads.tool_settings);
        assert_eq!(loaded.config.window.pos_x, 1200);
        assert_eq!(dir.names(), ["config.toml"]);
        assert!(!saving_held(&dir.config()));
    }

    /// The bug: a config another program held for longer than one quarter
    /// second was set aside as unusable, the app came up "reset" (welcome,
    /// nothing on), and the save on exit replaced the real file. Now it is
    /// retried, then kept exactly as it is, and nothing is saved over it.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_config_that_cannot_be_read_is_kept_and_never_saved_over() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = Dir::new("locked");
        let text = current_text();
        std::fs::write(dir.config(), &text).unwrap();
        let lock = std::fs::File::options()
            .read(true)
            .share_mode(0)
            .open(dir.config())
            .unwrap();
        let loaded = block(load_with(&dir.config(), &QUICK));
        let Some(LoadProblem::Unreadable { .. }) = loaded.problem else {
            panic!("{:?}", loaded.problem);
        };
        assert!(loaded.problem.as_ref().unwrap().saving_off());
        // Nothing allowed or switched on, and no welcome to answer in vain.
        assert!(loaded.config.reads.sign_in.is_empty());
        assert!(!loaded.config.providers.iter().any(|p| p.enabled));
        assert_eq!(loaded.config.reads.pending, PendingNotice::None);
        // Not renamed, nothing set aside.
        assert_eq!(dir.names(), ["config.toml"]);
        drop(lock);
        // Every way of saving leaves the file as it was.
        assert!(saving_held(&dir.config()));
        assert!(block(save_to(&loaded.config, &dir.config())).is_err());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let saver = spawn_saver(rt.handle(), dir.config());
        saver.handle().save(loaded.config.clone());
        saver.shutdown(loaded.config.clone(), rt.handle());
        assert_eq!(std::fs::read_to_string(dir.config()).unwrap(), text);
        assert_eq!(dir.names(), ["config.toml"]);
    }

    /// A lock let go while the read is retried: the config loads as usual.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_lock_let_go_within_the_retries_loads_normally() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = Dir::new("briefly");
        std::fs::write(dir.config(), current_text()).unwrap();
        let lock = std::fs::File::options()
            .read(true)
            .share_mode(0)
            .open(dir.config())
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(lock);
        });
        let retries = [Duration::from_millis(100); 10];
        let loaded = block(load_with(&dir.config(), &retries));
        release.join().unwrap();
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.config.reads.sign_in, ["claude", "codex"]);
        assert!(!saving_held(&dir.config()));
    }

    /// Each config that cannot be understood is set aside under a name of
    /// its own: a second one never replaces the first.
    #[test]
    fn unparsable_configs_are_set_aside_under_names_of_their_own() {
        let dir = Dir::new("broken");
        std::fs::write(dir.config(), "[window\npos_x = 1").unwrap();
        let first = block(load_with(&dir.config(), &QUICK));
        let Some(LoadProblem::SetAside { backup: one, .. }) = first.problem.clone() else {
            panic!("{:?}", first.problem);
        };
        assert!(!first.problem.as_ref().unwrap().saving_off());
        assert_eq!(first.config, Config::fresh_install());
        assert!(!dir.config().exists());
        std::fs::write(dir.config(), "pos_x = = 2").unwrap();
        let second = block(load_with(&dir.config(), &QUICK));
        let Some(LoadProblem::SetAside { backup: two, .. }) = second.problem.clone() else {
            panic!("{:?}", second.problem);
        };
        assert_ne!(one, two);
        assert_eq!(std::fs::read_to_string(&one).unwrap(), "[window\npos_x = 1");
        assert_eq!(std::fs::read_to_string(&two).unwrap(), "pos_x = = 2");
        for backup in [&one, &two] {
            let name = backup.file_name().unwrap().to_string_lossy().into_owned();
            assert!(
                name.starts_with("config.toml.") && name.ends_with(".corrupt"),
                "{name}"
            );
        }
        // A fresh config may be saved again in its place.
        assert!(!saving_held(&dir.config()));
        block(save_to(&second.config, &dir.config())).unwrap();
        assert_eq!(dir.names().len(), 3);
        let message = second.problem.unwrap().message();
        assert!(
            message.contains(&*two.file_name().unwrap().to_string_lossy()),
            "{message}"
        );
    }

    /// A config that cannot be understood nor set aside (another program
    /// holds it open without letting it be moved) is kept as it is, and
    /// nothing is saved over it.
    #[cfg(target_os = "windows")]
    #[test]
    fn an_unparsable_config_that_cannot_be_set_aside_is_kept() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        let dir = Dir::new("pinned");
        std::fs::write(dir.config(), "[broken").unwrap();
        let held = std::fs::File::options()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(dir.config())
            .unwrap();
        let loaded = block(load_with(&dir.config(), &QUICK));
        drop(held);
        let Some(LoadProblem::Unparsable { .. }) = loaded.problem else {
            panic!("{:?}", loaded.problem);
        };
        assert_eq!(loaded.config.reads.pending, PendingNotice::None);
        assert_eq!(dir.names(), ["config.toml"]);
        assert!(block(save_to(&loaded.config, &dir.config())).is_err());
        assert_eq!(std::fs::read_to_string(dir.config()).unwrap(), "[broken");
    }

    /// Notepad's encodings: UTF-8 with a BOM and UTF-16 with a BOM read as
    /// the same config; text in another encoding is a file not understood.
    #[test]
    fn a_config_saved_in_notepads_encodings_reads() {
        let text = current_text();
        let want = load_text(&text);
        let mut bom = b"\xEF\xBB\xBF".to_vec();
        bom.extend_from_slice(text.as_bytes());
        let mut le = vec![0xFF, 0xFE];
        let mut be = vec![0xFE, 0xFF];
        for unit in text.encode_utf16() {
            le.extend_from_slice(&unit.to_le_bytes());
            be.extend_from_slice(&unit.to_be_bytes());
        }
        for bytes in [bom, le, be] {
            let (mut config, origin) = parse(&bytes).expect("reads");
            migrate(&mut config, origin);
            assert_eq!(config, want);
        }
        // Windows-1252 "é" alone is not UTF-8.
        assert!(parse(b"[hooks]\non_reset = \"caf\xE9\"\n").is_err());
        assert!(parse(b"\xFF\xFE\x00").is_err());
    }

    /// `peek` (ailimits-auth status) creates, renames and writes nothing.
    #[test]
    fn a_peek_changes_nothing() {
        let dir = Dir::new("peek");
        assert!(matches!(peek(&dir.config()), Peek::Missing));
        assert!(dir.names().is_empty());
        std::fs::write(dir.config(), "[broken").unwrap();
        assert!(matches!(peek(&dir.config()), Peek::Unparsable(_)));
        assert_eq!(dir.names(), ["config.toml"]);
        std::fs::write(dir.config(), CONFIG_0_7_0).unwrap();
        let Peek::Found(config) = peek(&dir.config()) else {
            panic!("not found");
        };
        // Brought up to date in memory only: the file still has no [reads].
        assert_eq!(config.reads.sign_in, ["claude", "codex", "antigravity"]);
        assert_eq!(std::fs::read_to_string(dir.config()).unwrap(), CONFIG_0_7_0);
        assert!(!saving_held(&dir.config()));
    }

    /// A config as 0.7.0 writes it (toml::to_string_pretty of its Config):
    /// its four providers, Copilot switched off, the rest on. Grok and
    /// Cursor came after 0.7.0.
    const CONFIG_0_7_0: &str = r#"
[general]
update_interval_secs = 60
indicator = "panel_rows"
auto_update = true
panel_offset_x = 0
panel_offset_y = 0
panel_display = "primary"

[window]
pos_x = 1200
pos_y = 80
opacity = 0.45
pinned = true
locked = false

[ui]
palette = "ocean"
saturation = 55
brightness = 100
monochrome = false
layout = "vertical"
detail = "medium"
width_scale = "full"
column_flow = "row"
show_forecast = false

[[providers]]
id = "claude"
enabled = true
auth_method = "subscription"
credential_label = ""
alert_threshold = 80

[[providers]]
id = "codex"
enabled = true
auth_method = "subscription"
credential_label = ""
alert_threshold = 80

[[providers]]
id = "copilot"
enabled = false
auth_method = "subscription"
credential_label = ""
alert_threshold = 85

[[providers]]
id = "antigravity"
enabled = true
auth_method = "subscription"
credential_label = ""
alert_threshold = 80

[notifications]
enabled = true
cooldown_minutes = 15

[hooks]
on_threshold = ""
on_reset = ""
on_startup = ""
"#;

    #[test]
    fn an_upgrade_keeps_every_enabled_provider_running() {
        let config = load_text(CONFIG_0_7_0);
        assert_eq!(config.reads.sign_in, ["claude", "codex", "antigravity"]);
        // Providers 0.7.0 did not have are added, but off and not allowed:
        // that version never read them.
        for id in ["grok", "cursor"] {
            let p = config.providers.iter().find(|p| p.id == id).unwrap();
            assert!(!p.enabled && !config.provider_active(p), "{id}");
        }
        assert_eq!(config.reads.pending, PendingNotice::UpdateNotice);
        assert!(!config.reads.tool_settings && !config.reads.last_session);
        for p in &config.providers {
            assert_eq!(config.provider_active(p), p.enabled, "{}", p.id);
        }
        // Everything else is as the user left it.
        assert_eq!(config.window.pos_x, 1200);
        assert!(config.window.pinned);
        assert_eq!(config.ui.detail, crate::config::schema::DetailLevel::Medium);
        assert_eq!(config.ui.rows, Default::default());
        assert_eq!(config.ui.model, Default::default());
    }

    #[test]
    fn an_older_config_allows_only_the_providers_it_had_switched_on() {
        // An old config: Gemini under its old id, Claude off, nothing else.
        // The version that wrote it read Gemini alone; every provider it
        // did not have is added off and not allowed.
        let config = load_text(
            "[[providers]]\nid = \"gemini\"\nenabled = true\n\n[[providers]]\nid = \"claude\"\nenabled = false\n",
        );
        assert!(config.sign_in_allowed("antigravity"));
        assert!(config.sign_in_allowed("gemini"));
        assert_eq!(config.reads.sign_in, ["antigravity"]);
        let active: Vec<&str> = config
            .providers
            .iter()
            .filter(|p| config.provider_active(p))
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(active, ["antigravity"]);
        for id in ["codex", "copilot", "grok", "cursor"] {
            let p = config.providers.iter().find(|p| p.id == id).unwrap();
            assert!(!p.enabled, "{id} appended switched on");
        }
    }

    #[test]
    fn a_current_config_is_left_as_the_user_left_it() {
        let text = format!(
            "{CONFIG_0_7_0}\n[reads]\nsign_in = [\"codex\"]\ntool_settings = true\nlast_session = false\npending = \"none\"\n"
        );
        let config = load_text(&text);
        assert_eq!(config.reads.sign_in, ["codex"]);
        assert!(config.reads.tool_settings);
        assert_eq!(config.reads.pending, PendingNotice::None);
        let active: Vec<&str> = config
            .providers
            .iter()
            .filter(|p| config.provider_active(p))
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(active, ["codex"]);
    }

    #[test]
    fn an_empty_reads_table_still_counts_as_answered() {
        // The user unticked everything: that is a choice, not an upgrade.
        let text = format!("{CONFIG_0_7_0}\n[reads]\n");
        let config = load_text(&text);
        assert!(config.reads.sign_in.is_empty());
        assert_eq!(config.reads.pending, PendingNotice::None);
    }

    #[test]
    fn a_fresh_install_reads_nothing_and_owes_the_welcome() {
        let mut config = Config::default();
        settle_reads(&mut config, Origin::FreshInstall);
        assert!(config.reads.sign_in.is_empty());
        assert_eq!(config.reads.pending, PendingNotice::Welcome);
        assert!(!config.providers.iter().any(|p| config.provider_active(p)));
        // Switched off too, so a version that ignores [reads] reads nothing.
        assert!(config.providers.iter().all(|p| !p.enabled));
    }

    #[test]
    fn an_empty_or_providerless_config_is_no_upgrade() {
        for text in [
            "",
            "
",
            "[window]
pos_x = 10
",
            "[ui]
detail = \"medium\"
",
        ] {
            let config = load_text(text);
            assert!(config.reads.sign_in.is_empty(), "{text:?}");
            assert_eq!(config.reads.pending, PendingNotice::Welcome, "{text:?}");
        }
    }

    /// 0.7.1 fresh install, Codex allowed, then 0.7.0 run once (it drops
    /// [reads] on save), then 0.7.1 again: only Codex was switched on, so
    /// only Codex is read before and after.
    #[test]
    fn a_downgrade_and_back_keeps_what_was_allowed() {
        let mut config = Config::default();
        settle_reads(&mut config, Origin::FreshInstall);
        config.allow_sign_in("codex");
        config
            .providers
            .iter_mut()
            .find(|p| p.id == "codex")
            .unwrap()
            .enabled = true;
        let mut text = toml::to_string_pretty(&config).unwrap();
        // What 0.7.0 would write back: no [reads].
        let at = text.find("[reads]").unwrap();
        text.truncate(at);
        let enabled_in_070: Vec<String> = toml::from_str::<Config>(&text)
            .unwrap()
            .providers
            .into_iter()
            .filter(|p| p.enabled)
            .map(|p| p.id)
            .collect();
        assert_eq!(enabled_in_070, ["codex"]);
        let back = load_text(&text);
        assert_eq!(back.reads.sign_in, ["codex"]);
    }

    #[test]
    fn the_default_config_writes_no_new_ui_tables() {
        // Someone who never opens the picker keeps a config.toml without
        // [ui.rows] or [ui.model]; only [reads] is new.
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(!text.contains("[ui.rows]"), "{text}");
        assert!(!text.contains("[ui.model"), "{text}");
        assert!(text.contains("[reads]"), "{text}");
    }

    #[test]
    fn rows_and_model_round_trip() {
        let mut config = load_text(CONFIG_0_7_0);
        config.ui.rows.widget = Some(vec!["claude".into(), "claude.opus".into()]);
        config.ui.model.tooltip = vec![ModelPart::Model, ModelPart::Effort];
        config.ui.model.source = crate::config::schema::ModelSource::LastUsed;
        config
            .ui
            .model
            .names
            .insert("claude-opus-5-5".into(), "Opus \"5\"".into());
        let text = toml::to_string_pretty(&config).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.ui.rows, config.ui.rows);
        assert_eq!(back.ui.model, config.ui.model);
        assert_eq!(back.reads, config.reads);
        assert_eq!(origin_of(&text), Some(Origin::Current));
    }

    #[test]
    fn bad_model_values_fall_back_without_breaking_the_config() {
        let text =
            format!("{CONFIG_0_7_0}\n[reads]\nsign_in = [\"claude\"]\npending = \"later\"\n");
        let text = text.replace(
            "show_forecast = false\n",
            "show_forecast = false\n\n[ui.model]\nwidget = [\"model\", \"colour\", 3, \"model\", \"effort\"]\nsource = \"psychic\"\n",
        );
        let config = load_text(&text);
        assert_eq!(
            config.ui.model.widget,
            [ModelPart::Model, ModelPart::Effort]
        );
        assert_eq!(config.ui.model.source, Default::default());
        assert_eq!(config.reads.pending, PendingNotice::None);
        assert_eq!(config.window.pos_x, 1200);
    }
}
