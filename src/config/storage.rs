// config/storage.rs — reading and writing config.toml.
// Format documentation: docs/en/CONFIG.md.

use super::schema::Config;
use anyhow::Result;
use std::path::{Path, PathBuf};

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

/// Load the config, or create the default one on first launch.
pub async fn load_or_default() -> Result<Config> {
    let path = config_path();

    if !path.exists() {
        let mut default = Config::default();
        settle_reads(&mut default, Origin::FreshInstall);
        // First-run persistence is BEST-EFFORT: an unwritable %APPDATA%
        // (full disk, locked-down/roaming profile, AV quarantine) must not
        // stop the widget — it runs fine in-memory on defaults and shows
        // live data.
        if let Some(parent) = path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        match save(&default).await {
            Ok(()) => tracing::info!("Created default config at {}", path.display()),
            Err(e) => tracing::warn!("could not write default config ({e}); running in-memory"),
        }
        return Ok(default);
    }

    // A file that exists but cannot be read (permissions, a lock held by a
    // scanner, bytes that are not UTF-8) is handled like one that cannot be
    // parsed: the widget is a GUI app, so an error out of here would end it
    // with nothing on screen and nothing in the log. A lock is usually gone
    // within moments, so the read gets one retry; after that the file is set
    // aside for repair, logged, and the widget runs on defaults — never crash.
    let mut content = tokio::fs::read_to_string(&path).await;
    if content.is_err() {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        content = tokio::fs::read_to_string(&path).await;
    }
    let parsed = match content {
        Ok(content) => toml::from_str::<Config>(&content)
            .map(|cfg| (cfg, has_reads_table(&content)))
            .map_err(|e| ("parse", e.to_string())),
        Err(e) => Err(("read", e.to_string())),
    };
    let (mut config, origin) = match parsed {
        Ok((cfg, true)) => (cfg, Origin::Current),
        Ok((cfg, false)) => (cfg, Origin::Upgrade),
        Err((what, e)) => {
            tracing::warn!("Config {what} error: {e}, using defaults");
            // Preserve the file: a single bad line must not be silently
            // overwritten with defaults by the save on exit.
            let backup = path.with_extension("toml.corrupt");
            if let Err(re) = tokio::fs::rename(&path, &backup).await {
                tracing::warn!(
                    "could not set the unusable config aside ({re}); the save on exit will replace it"
                );
            } else {
                tracing::warn!("set the unusable config aside as {}", backup.display());
            }
            // Whatever the file allowed is unknown now, so nothing is read
            // until the user chooses again: the safe side of consent.
            (Config::default(), Origin::FreshInstall)
        }
    };

    migrate(&mut config, origin);

    Ok(config)
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
    // appear in the menu for users with an older config. Missing providers
    // are appended with their default (opt-in) state; existing entries are
    // left untouched.
    let defaults = Config::default().providers;
    for d in defaults {
        if !config.providers.iter().any(|p| p.id == d.id) {
            config.providers.push(d);
        }
    }

    // After the provider passes above, so an upgrade allows exactly the
    // providers that version would have read.
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

/// Whether the TOML text has a top-level `[reads]` table. Asked of text that
/// already parsed as a `Config`, so it parses again.
fn has_reads_table(content: &str) -> bool {
    content
        .parse::<toml::Table>()
        .is_ok_and(|t| t.contains_key("reads"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{ModelPart, PendingNotice};

    /// What `load_or_default` does with config text, minus the disk.
    fn load_text(content: &str) -> Config {
        let mut config: Config = toml::from_str(content).expect("parses");
        let origin = if has_reads_table(content) {
            Origin::Current
        } else {
            Origin::Upgrade
        };
        migrate(&mut config, origin);
        config
    }

    /// A config as 0.7.0 writes it (toml::to_string_pretty of its Config):
    /// Copilot switched off, the rest on.
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

[[providers]]
id = "grok"
enabled = true
auth_method = "subscription"
credential_label = ""
alert_threshold = 80

[[providers]]
id = "cursor"
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
        assert_eq!(
            config.reads.sign_in,
            ["claude", "codex", "antigravity", "grok", "cursor"]
        );
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
    fn an_older_config_gets_its_new_providers_allowed_like_the_old_version_did() {
        // 0.6: Gemini under its old id, no Grok or Cursor yet. 0.7.0 would
        // have appended both switched on and read them.
        let config = load_text(
            "[[providers]]\nid = \"gemini\"\nenabled = true\n\n[[providers]]\nid = \"claude\"\nenabled = false\n",
        );
        assert!(config.sign_in_allowed("antigravity"));
        assert!(config.sign_in_allowed("gemini"));
        assert!(!config.sign_in_allowed("claude"));
        assert!(config.sign_in_allowed("grok"));
        assert!(config.sign_in_allowed("cursor"));
        assert!(config.sign_in_allowed("codex"));
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
        // The providers stay switched on: ticking one only needs the read.
        assert!(config.providers.iter().all(|p| p.enabled));
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
        assert!(has_reads_table(&text));
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
