// config/storage.rs — reading and writing config.toml.
// Format documentation: docs/en/CONFIG.md.

use super::schema::Config;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Config path: %APPDATA%\AiLimits\config.toml.
pub fn config_path() -> PathBuf {
    dirs::config_dir()
        // Fall back to the current directory if %APPDATA% is unavailable.
        .unwrap_or_else(|| PathBuf::from("."))
        .join("AiLimits")
        .join("config.toml")
}

/// Load the config, or create the default one on first launch.
pub async fn load_or_default() -> Result<Config> {
    let path = config_path();

    if !path.exists() {
        let default = Config::default();
        // First-run persistence is BEST-EFFORT: an unwritable %APPDATA%
        // (full disk, locked-down/roaming profile, AV quarantine) must not
        // stop the widget — it runs fine in-memory on defaults and shows
        // live data. Persistence everywhere else already only warns on
        // failure; match that here instead of aborting the whole app.
        if let Some(parent) = path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        match save(&default).await {
            Ok(()) => tracing::info!("Created default config at {}", path.display()),
            Err(e) => tracing::warn!("could not write default config ({e}); running in-memory"),
        }
        return Ok(default);
    }

    let content = tokio::fs::read_to_string(&path).await?;

    // On a parse error: preserve the unparseable file for manual repair (a
    // single bad line shouldn't be silently overwritten with defaults on the
    // next save), log, and fall back to defaults — never crash.
    let mut config: Config = match toml::from_str(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::warn!("Config parse error: {e}, using defaults");
            let backup = path.with_extension("toml.corrupt");
            if let Err(re) = tokio::fs::rename(&path, &backup).await {
                tracing::warn!("could not back up the unparseable config: {re}");
            } else {
                tracing::warn!("backed up the unparseable config to {}", backup.display());
            }
            Config::default()
        }
    };

    // Migration: the Gemini provider became Antigravity (2026-07) — rename
    // the config id in place so the entry keeps its settings and the
    // append-missing pass below does not create a duplicate.
    for p in config.providers.iter_mut() {
        if p.id == "gemini" {
            p.id = "antigravity".to_string();
        }
    }

    // Migration: ensure every default provider exists so newly-added ones
    // appear in the menu for users with an older config. Missing providers
    // are appended with their default (opt-in) state; existing entries are
    // left untouched.
    let defaults = Config::default().providers;
    for d in defaults {
        if !config.providers.iter().any(|p| p.id == d.id) {
            config.providers.push(d);
        }
    }

    Ok(config)
}

/// Save the config to its default path.
pub async fn save(config: &Config) -> Result<()> {
    save_to(config, &config_path()).await
}

/// Save the config to a specific path — the seam the background saver and its
/// tests build on.
pub async fn save_to(config: &Config, path: &Path) -> Result<()> {
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

/// The single background config writer. Every write goes through one task, so
/// two writes can never interleave or land out of order.
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
    pub fn save(&self, config: Config) {
        let _ = self.tx.send(Some(SaveMsg::Write(config)));
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
    /// proves the newest config reached disk before the process exits.
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
            Ok(Ok(Err(_elapsed))) => tracing::warn!("final config save timed out"),
            Ok(Err(e)) => tracing::warn!("config writer task failed: {e}"),
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
pub fn spawn_saver(handle: &tokio::runtime::Handle, path: PathBuf) -> ConfigSaver {
    let (tx, mut rx) = tokio::sync::watch::channel::<Option<SaveMsg>>(None);
    let task = handle.spawn(async move {
        while rx.changed().await.is_ok() {
            let msg = rx.borrow_and_update().clone();
            let (cfg, last) = match msg {
                Some(SaveMsg::Write(cfg)) => (cfg, false),
                Some(SaveMsg::Final(cfg)) => (cfg, true),
                // The initial `None` seed carries no config.
                None => continue,
            };
            if let Err(e) = save_to(&cfg, &path).await {
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
