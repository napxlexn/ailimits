// config/storage.rs — reading and writing config.toml.
// Format documentation: docs/en/CONFIG.md.

use super::schema::{Config, PendingNotice};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
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
    /// The file as it was found or created: its stamp (taken before it was
    /// read) and the config it holds, brought up to date. None where no
    /// usable file is there. What the writer and the watch for edits made
    /// elsewhere start from (`ConfigFile`).
    pub on_disk: Option<(FileStamp, Config)>,
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
    /// The file was used, but these lines (1-based) did not read (a value
    /// of the wrong type, a typo) and were left out: their defaults apply.
    /// A mistake costs that line, not the whole file.
    LeftOut {
        lines: Vec<usize>,
        /// A copy of the file as it was, kept beside it before the app saves
        /// over it (`config.toml.<stamp>.left-out`); None if it could not be
        /// made.
        copy: Option<PathBuf>,
    },
}

/// What is said when lines of config.toml were left out: where, never what.
pub fn left_out_message(lines: &[usize]) -> String {
    let list = lines
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let which = if lines.len() == 1 {
        format!("line {list} does not read and is left out")
    } else {
        format!("lines {list} do not read and are left out")
    };
    format!(
        "In config.toml, {which}: the default is used there, and everything else is in use. Fix it in the file."
    )
}

impl LoadProblem {
    /// Whether saving is off for this session (the file is kept as it is).
    pub fn saving_off(&self) -> bool {
        !matches!(
            self,
            LoadProblem::SetAside { .. } | LoadProblem::LeftOut { .. }
        )
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
            LoadProblem::LeftOut { lines, copy } => match copy.as_ref().and_then(|c| c.file_name()) {
                Some(name) => format!(
                    "{} The file as it was is kept as {}.",
                    left_out_message(lines),
                    name.to_string_lossy()
                ),
                None => left_out_message(lines),
            },
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
    // Taken before the read: an edit that lands while the file is read
    // leaves a newer stamp, so the watch reads it again.
    let stamp = stamp_of(path).ok().flatten();
    let bytes = match read_with_retries(path, retries).await {
        FileRead::Bytes(bytes) => bytes,
        FileRead::Missing => return first_start(path).await,
        FileRead::Failed(error) => {
            tracing::warn!("config read error, the file is kept as it is and not saved over this session: {error}");
            hold_saving(path);
            return Loaded {
                config: in_memory(),
                problem: Some(LoadProblem::Unreadable { error }),
                on_disk: None,
            };
        }
    };
    match parse_keeping(&bytes) {
        Ok((config, origin, left_out)) => {
            let mut config = config;
            migrate(&mut config, origin);
            // Lines left out: the file as it was is kept beside it, since the
            // next save writes the config without them.
            let problem = if left_out.is_empty() {
                None
            } else {
                let copy = keep_copy(path, "left-out").await.ok();
                Some(LoadProblem::LeftOut {
                    lines: left_out,
                    copy,
                })
            };
            Loaded {
                on_disk: stamp.map(|s| (s, config.clone())),
                config,
                problem,
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
                        on_disk: None,
                    }
                }
                Err(e) => {
                    tracing::warn!("could not set the unusable config aside ({e}); it is kept as it is and not saved over this session");
                    hold_saving(path);
                    Loaded {
                        config: in_memory(),
                        problem: Some(LoadProblem::Unparsable { error }),
                        on_disk: None,
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
    let on_disk = match save_to(&default, path).await {
        Ok(()) => {
            tracing::info!("Created default config at {}", path.display());
            stamp_of(path).ok().flatten().map(|s| (s, default.clone()))
        }
        Err(e) => {
            tracing::warn!("could not write default config ({e}); running in-memory");
            None
        }
    };
    Loaded {
        config: default,
        problem: None,
        on_disk,
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

/// The config in the file's bytes, and where it came from (`parse_keeping`
/// without the lines left out).
fn parse(bytes: &[u8]) -> std::result::Result<(Config, Origin), String> {
    parse_keeping(bytes).map(|(config, origin, _)| (config, origin))
}

/// At most this many lines are left out of one file before it counts as
/// not understood.
const MAX_LEFT_OUT: usize = 8;

/// The config in the file's bytes, where it came from, and the lines left
/// out. Text in UTF-8 (with or without a BOM) or UTF-16 with a BOM, as
/// Notepad saves it. A `key = value` line whose value does not read (a
/// wrong type, a typo, a missing value) is left out and its default used,
/// so a mistake costs that line, not the whole file; a broken table header
/// or more than `MAX_LEFT_OUT` such lines still make it not understood.
fn parse_keeping(bytes: &[u8]) -> std::result::Result<(Config, Origin, Vec<usize>), String> {
    let content = decode(bytes)?;
    let mut text = content.clone();
    let mut left_out = Vec::new();
    let config = loop {
        match toml::from_str::<Config>(&text) {
            Ok(config) => break config,
            Err(e) => {
                let first = toml_error_position(&e, &text);
                let Some(span) = e.span() else {
                    return Err(first);
                };
                let (line, _) = line_and_column(&text, span.start);
                match without_line(&text, line) {
                    Some(next) if left_out.len() < MAX_LEFT_OUT => {
                        left_out.push(line);
                        text = next;
                    }
                    _ => return Err(first),
                }
            }
        }
    };
    left_out.sort_unstable();
    if !left_out.is_empty() {
        tracing::warn!("config.toml: left out line(s) {left_out:?}");
    }
    // Judged on the text that read: the lines left out are not in it.
    Ok(match origin_of(&text) {
        Some(origin) => (config, origin, left_out),
        // Neither [reads] nor [[providers]]: an empty file, or one written by
        // something else before the first start. Every version that read
        // without asking wrote its providers, so this is no upgrade.
        None => (Config::default(), Origin::FreshInstall, left_out),
    })
}

/// `text` with its `line` (1-based) commented out, when that line is a
/// `key = value` line; None for a table header, a comment, a blank line or
/// the inside of a multi-line value. Line numbers stay as they were.
fn without_line(text: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let target = lines.get(line.checked_sub(1)?)?;
    let t = target.trim_start();
    if t.is_empty() || t.starts_with('[') || t.starts_with('#') || !t.contains('=') {
        return None;
    }
    let mut out = String::with_capacity(text.len() + 1);
    for (i, l) in lines.iter().enumerate() {
        if i + 1 == line {
            out.push('#');
        }
        out.push_str(l);
    }
    Some(out)
}

/// Where in the config a TOML error is, never what: toml's own message
/// quotes the line, and serde's can quote a value, and either may be the
/// user's. Only the line and the column are logged and kept.
fn toml_error_position(e: &toml::de::Error, text: &str) -> String {
    match e.span() {
        Some(span) => {
            let (line, column) = line_and_column(text, span.start);
            format!("not a usable config at line {line}, column {column}")
        }
        None => "not a usable config".to_string(),
    }
}

/// The 1-based line and column (in characters) of byte `offset` in `text`.
fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let before = &text[..end];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |last| last.chars().count())
        + 1;
    (line, column)
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
/// A copy of the config beside it, under a name of its own
/// (`config.toml.<UTC stamp>.<what>`, never replacing an earlier one).
async fn keep_copy(path: &Path, what: &str) -> std::io::Result<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    for n in 1..=100u32 {
        let mut name = path.as_os_str().to_owned();
        if n == 1 {
            name.push(format!(".{stamp}.{what}"));
        } else {
            name.push(format!(".{stamp}-{n}.{what}"));
        }
        let copy = PathBuf::from(name);
        if tokio::fs::try_exists(&copy).await.unwrap_or(true) {
            continue;
        }
        // Read and written, not copied: a file copy would add CopyFileExW
        // to the exe's imports (the least-antivirus-risk rule).
        let bytes = tokio::fs::read(path).await?;
        tokio::fs::write(&copy, bytes).await?;
        return Ok(copy);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no free name for a copy of the config",
    ))
}

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

/// Save the config to a specific path at once: a first start, the commands
/// of `ailimits-auth`, tests. The app's own saves go through the background
/// writer instead (`spawn_saver_for`, `ConfigFile::write`), which also
/// keeps an edit made elsewhere from being written over. Refused, with the
/// file untouched, while saving to that path is held for the session
/// (`LoadProblem::saving_off`).
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

/// A message for the config writer task, with the generation of the file
/// it was made against (`ConfigFile::generation`): one made before an edit
/// that was read since is dropped.
#[derive(Clone)]
pub enum SaveMsg {
    /// Persist this config and keep running.
    Write(Config, u64),
    /// Persist this config, then stop. Always the writer's last action.
    Final(Config, u64),
}

/// The single background config writer. Every write goes through it, so two
/// writes can never interleave or land out of order.
pub struct ConfigSaver {
    tx: Arc<tokio::sync::watch::Sender<Option<SaveMsg>>>,
    task: tokio::task::JoinHandle<()>,
    file: Arc<ConfigFile>,
}

/// A cheap clone of the writer's sending end, for the app's hot paths.
#[derive(Clone)]
pub struct ConfigSaverHandle {
    tx: Arc<tokio::sync::watch::Sender<Option<SaveMsg>>>,
    file: Arc<ConfigFile>,
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
        let generation = self.file.generation();
        self.tx.send_if_modified(|slot| match slot {
            Some(SaveMsg::Final(..)) => false,
            _ => {
                *slot = Some(SaveMsg::Write(config, generation));
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
            file: self.file.clone(),
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
        let ConfigSaver { tx, task, file } = self;
        let _ = tx.send(Some(SaveMsg::Final(config, file.generation())));
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
///
/// The file found at `path` now counts as known: the first save replaces
/// it, as it always did. The app starts its writer from what it loaded
/// instead (`spawn_saver_for`).
pub fn spawn_saver(handle: &tokio::runtime::Handle, path: PathBuf) -> ConfigSaver {
    spawn_saver_for(handle, Arc::new(ConfigFile::new(path, None)))
}

/// `spawn_saver` for a file whose state the app shares with its watch for
/// edits made elsewhere (`ConfigFile`): each write is checked against it,
/// and recorded in it.
pub fn spawn_saver_for(handle: &tokio::runtime::Handle, file: Arc<ConfigFile>) -> ConfigSaver {
    let (tx, mut rx) = tokio::sync::watch::channel::<Option<SaveMsg>>(None);
    let writer = file.clone();
    let task = handle.spawn(async move {
        let mut told = false;
        while rx.changed().await.is_ok() {
            let msg = rx.borrow_and_update().clone();
            let (cfg, generation, last) = match msg {
                Some(SaveMsg::Write(cfg, g)) => (cfg, g, false),
                Some(SaveMsg::Final(cfg, g)) => (cfg, g, true),
                // The initial `None` seed carries no config.
                None => continue,
            };
            // One blocking step: the stamp checked, the file written and its
            // new stamp recorded under the lock the watch takes, so the
            // watch never sees a write of the app's own half done.
            let file = writer.clone();
            let outcome = tokio::task::spawn_blocking(move || file.write(&cfg, generation))
                .await
                .unwrap_or_else(|e| Written::Failed(e.to_string()));
            match outcome {
                Written::Done | Written::Same => {}
                Written::Held => {
                    if !told {
                        tracing::info!(
                            "config not saved: the file is kept as it was found this session"
                        );
                        told = true;
                    }
                }
                Written::Stale => {
                    tracing::debug!("config save dropped: made before an edit of the file")
                }
                Written::Broken => tracing::info!(
                    "config not saved: config.toml was edited and does not read; it is kept as it is"
                ),
                Written::Unseen => tracing::info!(
                    "config not saved: config.toml was edited elsewhere; the edit is read first"
                ),
                Written::Failed(e) => tracing::warn!("config save failed: {e}"),
            }
            if last {
                break;
            }
        }
    });
    ConfigSaver {
        tx: Arc::new(tx),
        task,
        file,
    }
}

// ─── The file on disk, shared by the writer and the watch ────────────

/// A file's length and last write time: what tells that it changed on disk,
/// for one metadata call and no read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub modified: Option<std::time::SystemTime>,
    pub len: u64,
}

/// The stamp of the file at `path` now; None when there is no file.
pub fn stamp_of(path: &Path) -> std::io::Result<Option<FileStamp>> {
    match std::fs::metadata(path) {
        Ok(m) => Ok(Some(FileStamp {
            modified: m.modified().ok(),
            len: m.len(),
        })),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// What this process knows of its config file, shared by the background
/// writer and the watch for edits made elsewhere (app/config_watch.rs).
///
/// Each write of the app's own records the stamp it left (under the lock
/// the watch takes), so the watch never takes it for an edit; each edit the
/// watch reads is recorded the same way. The writer replaces only the file
/// it knows: one changed elsewhere and not read yet, or read and found
/// broken, is never written over.
pub struct ConfigFile {
    path: PathBuf,
    state: Mutex<OnDisk>,
    /// Bumped by every edit read from disk. A save queued before it carries
    /// an older number and is dropped: it was made from the config the edit
    /// replaced (the app merges the two and saves again).
    generation: AtomicU64,
    /// Told when a change is found off the event loop (the polling cycle, a
    /// write refused for an edit not read yet), so the edit is read at once.
    notify: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

#[derive(Debug, Default)]
struct OnDisk {
    /// The file's stamp as last written or read here; None: no file.
    stamp: Option<FileStamp>,
    /// The config that file holds, brought up to date; None while unknown
    /// (removed, emptied, never read).
    config: Option<Config>,
    /// Changed elsewhere and does not read: never written over while so.
    broken: bool,
}

/// What the background writer did with a config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    Done,
    /// The file already holds this config: left as it is, so its comments
    /// and layout stay.
    Same,
    /// Saving is held for the session (`LoadProblem::saving_off`).
    Held,
    /// Made before an edit that was read since.
    Stale,
    /// The file was edited and does not read: kept as it is.
    Broken,
    /// The file was changed elsewhere and not read yet: kept, and the watch
    /// told.
    Unseen,
    Failed(String),
}

/// What a look at the file found (`ConfigFile::check`).
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Nothing new on disk.
    None,
    /// The file cannot be looked at now (the writer has it, or it is being
    /// written): look again in a moment.
    Retry,
    /// Edited elsewhere and understood: the config it holds now, brought up
    /// to date as a start would (renamed and added providers, `[reads]` as
    /// its origin calls for), and what the file held before as far as this
    /// process knows (None: unknown).
    Edited {
        config: Box<Config>,
        base: Option<Box<Config>>,
        /// Lines that did not read and were left out (`parse_keeping`).
        left_out: Vec<usize>,
    },
    /// Edited elsewhere and not understood: where (line and column, never
    /// the text). Nothing is written over it while it stays so.
    Broken(String),
    /// Removed or emptied. Not applied: the settings in memory are written
    /// back with the next save.
    Gone,
}

impl ConfigFile {
    /// The file at `path` as `on_disk` found it (`Loaded::on_disk`); without
    /// it, the file there now counts as known, its content unknown.
    pub fn new(path: PathBuf, on_disk: Option<(FileStamp, Config)>) -> Self {
        let state = match on_disk {
            Some((stamp, config)) => OnDisk {
                stamp: Some(stamp),
                config: Some(config),
                broken: false,
            },
            None => OnDisk {
                stamp: stamp_of(&path).ok().flatten(),
                config: None,
                broken: false,
            },
        };
        Self {
            path,
            state: Mutex::new(state),
            generation: AtomicU64::new(0),
            notify: OnceLock::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What to call when a change is found off the event loop (once).
    pub fn set_notify(&self, f: impl Fn() + Send + Sync + 'static) {
        let _ = self.notify.set(Box::new(f));
    }

    fn tell(&self) {
        if let Some(f) = self.notify.get() {
            f();
        }
    }

    /// Bumped by every edit read; a save carries the number it was made at.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    fn lock(&self) -> MutexGuard<'_, OnDisk> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn try_lock(&self) -> Option<MutexGuard<'_, OnDisk>> {
        match self.state.try_lock() {
            Ok(g) => Some(g),
            Err(std::sync::TryLockError::Poisoned(p)) => Some(p.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }

    /// Whether the file changed since it was last written or read here: one
    /// metadata call, nothing read, never waiting for the writer. For the
    /// polling cycle, off the event loop; `check` reads the change.
    pub fn changed(&self) -> bool {
        if saving_held(&self.path) {
            return false;
        }
        let Some(disk) = self.try_lock() else {
            return false;
        };
        stamp_of(&self.path).is_ok_and(|now| now != disk.stamp)
    }

    /// `changed`, and the change told (`set_notify`) when there is one.
    pub fn tell_if_changed(&self) {
        if self.changed() {
            self.tell();
        }
    }

    /// Look at the file: one metadata call while nothing changed. A change
    /// is read, parsed and brought up to date as a start would, and
    /// recorded, so it is reported once. A file that could not be used at
    /// startup is not looked at (`LoadProblem::saving_off`: restart).
    pub fn check(&self) -> Change {
        if saving_held(&self.path) {
            return Change::None;
        }
        let Some(mut disk) = self.try_lock() else {
            return Change::Retry;
        };
        let Ok(now) = stamp_of(&self.path) else {
            return Change::None;
        };
        if now == disk.stamp {
            return Change::None;
        }
        let Some(stamp) = now else {
            *disk = OnDisk::default();
            return Change::Gone;
        };
        let Ok(bytes) = std::fs::read(&self.path) else {
            return Change::Retry;
        };
        // Still being written (an editor that writes in place): read again
        // in a moment rather than take half a file.
        if stamp_of(&self.path).ok().flatten() != Some(stamp) {
            return Change::Retry;
        }
        disk.stamp = Some(stamp);
        if is_blank(&bytes) {
            disk.config = None;
            disk.broken = false;
            return Change::Gone;
        }
        match parse_keeping(&bytes) {
            Ok((mut config, origin, left_out)) => {
                migrate(&mut config, origin);
                let base = disk.config.replace(config.clone());
                disk.broken = false;
                self.generation.fetch_add(1, Ordering::SeqCst);
                Change::Edited {
                    config: Box::new(config),
                    base: base.map(Box::new),
                    left_out,
                }
            }
            Err(error) => {
                disk.broken = true;
                Change::Broken(error)
            }
        }
    }

    /// Write `config`, made at `generation`, unless the file is not the
    /// one known here: an edit made elsewhere and not read yet, or read and
    /// broken, is kept (`Written`). Nothing is written when the file already
    /// holds this config, so a hand edit's comments stay until the app has
    /// something new to say. Blocking: the writer task runs it off the
    /// runtime's threads.
    pub fn write(&self, config: &Config, generation: u64) -> Written {
        if saving_held(&self.path) {
            return Written::Held;
        }
        let mut disk = self.lock();
        if generation < self.generation() {
            return Written::Stale;
        }
        if disk.broken {
            return Written::Broken;
        }
        if let Ok(now) = stamp_of(&self.path) {
            if now != disk.stamp {
                drop(disk);
                self.tell();
                return Written::Unseen;
            }
        }
        if disk.config.as_ref() == Some(config) {
            return Written::Same;
        }
        let written = toml::to_string_pretty(config)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                // The directory may have vanished between runs.
                if let Some(parent) = self.path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                write_atomically(&self.path, text.as_bytes()).map_err(|e| e.to_string())
            });
        match written {
            Ok(()) => {
                disk.stamp = stamp_of(&self.path).ok().flatten();
                disk.config = Some(config.clone());
                Written::Done
            }
            Err(e) => Written::Failed(e),
        }
    }
}

/// Empty, or only white space (with or without a BOM): what an editor
/// leaves for a moment while it saves, or a file cleared by hand.
fn is_blank(bytes: &[u8]) -> bool {
    decode(bytes).is_ok_and(|text| text.trim().is_empty())
}

/// A unique sibling temp file for an atomic write: two overlapping writes
/// must not share one (the second would garble the first one's rename).
fn temp_beside(path: &Path) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp{n}"));
    PathBuf::from(tmp)
}

/// `atomic_write` for a caller that may block (the config writer, under its
/// lock).
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = temp_beside(path);
    std::fs::write(&tmp, bytes)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Write bytes to `path` atomically: write a uniquely-named sibling temp
/// file, then rename it over the target (std/tokio rename on Windows uses
/// MOVEFILE_REPLACE_EXISTING). A plain `fs::write` truncates first, so a
/// crash mid-write would leave an empty/corrupt file; with the rename the
/// old contents survive any interruption. Shared by the config and the
/// provider cache.
pub async fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = temp_beside(path);
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

    /// A value of the wrong type, a typo or a missing value costs its line:
    /// the rest of the file is used, the line is named. A broken header
    /// still makes the file not understood.
    #[test]
    fn a_mistyped_value_costs_only_its_line() {
        let text = "[reads]\nsign_in = [\"claude\"]\n\n[ui]\nbrightness = 50.5\npalette = \"ocean\"\n\n[tune.panel]\ntray_gap = \"wide\"\n\n[tune]\nformat = \"compact\"\n\n[window]\nopacity =\n";
        let (config, _, left_out) = parse_keeping(text.as_bytes()).expect("used");
        assert_eq!(left_out, [5, 12, 15]);
        assert_eq!(config.ui.palette, crate::config::schema::Palette::Ocean);
        assert_eq!(
            config.ui.brightness,
            crate::config::schema::UIConfig::default().brightness
        );
        assert!(config.sign_in_allowed("claude"));
        // tray_gap of the wrong type was already read as unset by [tune].
        assert!(config.tune.panel.is_default());
        let message = left_out_message(&left_out);
        assert!(
            message.contains("lines 5, 12, 15") && !message.contains("50.5"),
            "{message}"
        );
        assert!(parse_keeping(b"[ui\nbrightness = 50\n").is_err());
    }

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
        std::fs::write(
            dir.config(),
            "[[providers
pos_x = 2",
        )
        .unwrap();
        let second = block(load_with(&dir.config(), &QUICK));
        let Some(LoadProblem::SetAside { backup: two, .. }) = second.problem.clone() else {
            panic!("{:?}", second.problem);
        };
        assert_ne!(one, two);
        assert_eq!(std::fs::read_to_string(&one).unwrap(), "[window\npos_x = 1");
        assert_eq!(
            std::fs::read_to_string(&two).unwrap(),
            "[[providers
pos_x = 2"
        );
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
        // Not watched either: such a session runs on a config that reads
        // nothing until a restart, as its message says.
        let file = ConfigFile::new(dir.config(), loaded.on_disk);
        std::fs::write(dir.config(), current_text()).unwrap();
        assert!(!file.changed());
        assert_eq!(file.check(), Change::None);
        assert_eq!(file.write(&loaded.config, 0), Written::Held);
    }

    // ─── Edits made elsewhere, and the app's own writes ──────────

    /// The file loaded, and its writer's state as the app starts it.
    fn watched(dir: &Dir, text: &str) -> (Loaded, ConfigFile) {
        std::fs::write(dir.config(), text).unwrap();
        let loaded = block(load_with(&dir.config(), &QUICK));
        assert_eq!(loaded.problem, None);
        let file = ConfigFile::new(dir.config(), loaded.on_disk.clone());
        (loaded, file)
    }

    /// The app's own writes are recorded as they land: the watch never
    /// takes one for an edit, whether written directly or by the writer.
    #[test]
    fn the_apps_own_writes_are_not_edits() {
        let dir = Dir::new("own");
        let (loaded, file) = watched(&dir, &current_text());
        // The file as loaded is known.
        assert!(!file.changed());
        assert_eq!(file.check(), Change::None);
        let mut config = loaded.config.clone();
        config.window.pos_x = 4321;
        assert_eq!(file.write(&config, file.generation()), Written::Done);
        assert!(!file.changed());
        assert_eq!(file.check(), Change::None);
        // Through the background writer, as the app saves.
        let file = Arc::new(file);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let saver = spawn_saver_for(rt.handle(), file.clone());
        config.window.pos_y = 98765;
        saver.handle().save(config.clone());
        config.ui.saturation = 7;
        saver.shutdown(config.clone(), rt.handle());
        assert!(!file.changed());
        assert_eq!(file.check(), Change::None);
        let on_disk: Config =
            toml::from_str(&std::fs::read_to_string(dir.config()).unwrap()).unwrap();
        assert_eq!(on_disk.window.pos_y, 98765);
        assert_eq!(on_disk.ui.saturation, 7);
    }

    /// An edit made elsewhere is read once, brought up to date as a start
    /// would, with what the file held before as the base of the merge.
    #[test]
    fn an_edit_made_elsewhere_is_read_once() {
        let dir = Dir::new("edited");
        let (loaded, file) = watched(&dir, &current_text());
        let before = file.generation();
        std::fs::write(
            dir.config(),
            current_text().replace("pos_x = 1200", "pos_x = 33"),
        )
        .unwrap();
        assert!(file.changed());
        let Change::Edited { config, base, .. } = file.check() else {
            panic!("no edit read");
        };
        assert_eq!(config.window.pos_x, 33);
        // Brought up to date: the providers 0.7.0 did not have are there.
        assert!(config.providers.iter().any(|p| p.id == "cursor"));
        assert_eq!(base.as_deref(), Some(&loaded.config));
        assert_eq!(file.generation(), before + 1);
        // Once: the next look finds nothing new.
        assert!(!file.changed());
        assert_eq!(file.check(), Change::None);
    }

    /// The bug this guards against: an edit saved a moment before the app
    /// saved a change of its own was written over. The writer keeps a file
    /// it does not know, tells the watch, and a save made before the edit
    /// was read is dropped; once read, the app's next save goes through.
    #[test]
    fn an_edit_not_read_yet_is_never_written_over() {
        let dir = Dir::new("unseen");
        let (loaded, file) = watched(&dir, &current_text());
        let told = Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let told = told.clone();
            file.set_notify(move || told.store(true, Ordering::SeqCst));
        }
        let edit = current_text().replace("pos_x = 1200", "pos_x = 5");
        std::fs::write(dir.config(), &edit).unwrap();
        let queued_at = file.generation();
        let mut ours = loaded.config.clone();
        ours.ui.saturation = 1;
        assert_eq!(file.write(&ours, queued_at), Written::Unseen);
        assert!(told.load(Ordering::SeqCst), "the watch was not told");
        assert_eq!(std::fs::read_to_string(dir.config()).unwrap(), edit);
        // Read: the save queued before it is dropped, a new one is written.
        assert!(matches!(file.check(), Change::Edited { .. }));
        assert_eq!(file.write(&ours, queued_at), Written::Stale);
        assert_eq!(std::fs::read_to_string(dir.config()).unwrap(), edit);
        assert_eq!(file.write(&ours, file.generation()), Written::Done);
    }

    /// An edit that does not parse is said by its place only, kept exactly
    /// as it is, and never saved over - directly, by the writer, or at the
    /// exit - until it reads again; then the last good config is the base.
    #[test]
    fn a_broken_edit_is_kept_and_never_saved_over() {
        let dir = Dir::new("broken-edit");
        let (loaded, file) = watched(&dir, &current_text());
        let broken = format!(
            "{}\n[hooks]\non_reset = \"sk-ant-SECRET\" junk\n",
            current_text()
        );
        std::fs::write(dir.config(), &broken).unwrap();
        let Change::Broken(error) = file.check() else {
            panic!("not broken");
        };
        assert!(error.contains("at line "), "{error}");
        assert!(!error.contains("SECRET"), "{error}");
        // Told once: the next look finds nothing new.
        assert_eq!(file.check(), Change::None);
        let mut ours = loaded.config.clone();
        ours.window.locked = true;
        assert_eq!(file.write(&ours, file.generation()), Written::Broken);
        let file = Arc::new(file);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let saver = spawn_saver_for(rt.handle(), file.clone());
        saver.handle().save(ours.clone());
        saver.shutdown(ours.clone(), rt.handle());
        assert_eq!(std::fs::read_to_string(dir.config()).unwrap(), broken);
        assert_eq!(dir.names(), ["config.toml"]);
        // Fixed: read, with the last config that read as the base.
        let fixed = current_text().replace("pos_y = 80", "pos_y = 81");
        std::fs::write(dir.config(), &fixed).unwrap();
        let Change::Edited { config, base, .. } = file.check() else {
            panic!("the fix was not read");
        };
        assert_eq!(config.window.pos_y, 81);
        assert_eq!(base.as_deref(), Some(&loaded.config));
        assert_eq!(file.write(&ours, file.generation()), Written::Done);
    }

    /// A config the file already holds is not written again: a hand edit's
    /// comments and layout stay until the app has something new to say.
    #[test]
    fn the_same_config_is_not_written_again() {
        let dir = Dir::new("same");
        let text = format!("# my notes\n{}", current_text());
        let (loaded, file) = watched(&dir, &text);
        assert_eq!(file.write(&loaded.config, 0), Written::Same);
        assert_eq!(std::fs::read_to_string(dir.config()).unwrap(), text);
        // An edited file read is known the same way.
        let edited = text.replace("# my notes", "# my notes, longer now");
        std::fs::write(dir.config(), &edited).unwrap();
        let Change::Edited { config, .. } = file.check() else {
            panic!("no edit read");
        };
        assert_eq!(file.write(&config, file.generation()), Written::Same);
        assert_eq!(std::fs::read_to_string(dir.config()).unwrap(), edited);
    }

    /// A file emptied (or caught empty while an editor saves it) or
    /// removed is not applied - it would read as a fresh install - and the
    /// next save writes the settings back.
    #[test]
    fn an_emptied_or_removed_file_is_not_applied() {
        let dir = Dir::new("emptied");
        let (loaded, file) = watched(&dir, &current_text());
        std::fs::write(dir.config(), "  \r\n").unwrap();
        assert_eq!(file.check(), Change::Gone);
        assert_eq!(file.check(), Change::None);
        assert_eq!(file.write(&loaded.config, 0), Written::Done);
        std::fs::remove_file(dir.config()).unwrap();
        assert_eq!(file.check(), Change::Gone);
        assert_eq!(file.write(&loaded.config, 0), Written::Done);
        assert_eq!(
            block(load_with(&dir.config(), &QUICK)).config,
            loaded.config
        );
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

    /// A config that does not parse is logged and kept by where the error
    /// is, never by what is there: the broken line or a value in it may be
    /// the user's.
    #[test]
    fn a_parse_error_names_the_place_never_the_content() {
        for (text, line) in [
            // Broken syntax on line 4.
            ("[ui]\n\n[hooks]\non_reset = \"sk-ant-SECRET\" junk\n", 4),
            // A value of the wrong type: serde would quote it.
            (
                "[[providers]]\nid = \"claude\"\nenabled = \"sk-ant-SECRET\"\n",
                3,
            ),
            // An unclosed string after non-ASCII text.
            ("[hooks]\non_reset = \"Вася sk-ant-SECRET\n", 2),
        ] {
            let Err(e) = toml::from_str::<Config>(text) else {
                panic!("{text:?} parsed");
            };
            let error = toml_error_position(&e, text);
            assert!(!error.contains("SECRET"), "{error}");
            assert!(!error.contains("Вася"), "{error}");
            assert!(
                error.contains(&format!("at line {line}, column ")),
                "{error}"
            );
        }
        assert_eq!(line_and_column("ab\ncd", 0), (1, 1));
        assert_eq!(line_and_column("ab\ncd", 4), (2, 2));
        assert_eq!(line_and_column("я\nюя", 5), (2, 2));
        // Inside a character, and past the end.
        assert_eq!(line_and_column("я", 1), (1, 1));
        assert_eq!(line_and_column("ab", 99), (1, 3));
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
