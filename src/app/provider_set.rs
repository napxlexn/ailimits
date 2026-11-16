// app/provider_set.rs — the provider instances the scheduler polls, built
// from the config and rebuilt only where the config changed.

use super::AppCommand;
use crate::{
    config::schema::{Config, ProviderConfig, Surface},
    monitor::scheduler::{ProviderSlot, SharedProviders},
    providers::{
        antigravity::AntigravityProvider, claude::ClaudeProvider, codex::CodexProvider,
        copilot::CopilotProvider, cursor::CursorProvider, grok::GrokProvider, Provider, ProviderId,
    },
    rows,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// The extra row that shows Cursor's Grok Bot. Its numbers come from a
/// request of their own, which is made only while this row is chosen.
pub(super) const GROK_BOT_ROW: &str = "cursor.bot";

/// Whether the Grok Bot row is chosen on any surface: only then does the
/// Cursor provider ask for it.
pub(super) fn grok_bot_wanted(config: &Config) -> bool {
    rows::row_chosen(config, Surface::Widget, GROK_BOT_ROW)
        || rows::row_chosen(config, Surface::Tooltip, GROK_BOT_ROW)
}

/// Everything a provider instance is built from. Equal keys build the same
/// provider, so its instance - and whatever it keeps between cycles (the gh
/// token, Cursor's plan, Antigravity's project) - is kept.
#[derive(Debug, Clone, PartialEq)]
struct BuildKey {
    /// The provider's config as built: `enabled` already means "may run".
    config: serde_json::Value,
    /// `ProviderConfig::tool_files`, which the config does not save.
    tool_files: bool,
    /// Cursor only: whether Grok Bot is asked for.
    grok_bot: bool,
}

impl BuildKey {
    /// Whether the provider now draws on another source or account (and
    /// what it showed so far is no longer its own), rather than only asking
    /// for one row more or less.
    fn source_differs(&self, other: &BuildKey) -> bool {
        self.config != other.config || self.tool_files != other.tool_files
    }
}

/// The config a provider is built with. A provider the user has allowed
/// nothing for is built switched off: its fetch returns `NotConfigured`
/// before it touches any file, the keyring, `gh` or the network. One set up
/// only with the app's own sign-in (a pasted key, the menu's Cursor sign-in)
/// runs without `tool_files`: it uses that alone and never falls back to the
/// tool's sign-in.
fn built_config(config: &Config, pc: &ProviderConfig) -> ProviderConfig {
    ProviderConfig {
        enabled: config.provider_active(pc),
        tool_files: config.sign_in_allowed(&pc.id),
        ..pc.clone()
    }
}

fn build_key(config: &Config, id: &ProviderId, pc: &ProviderConfig) -> BuildKey {
    BuildKey {
        config: serde_json::to_value(pc).unwrap_or(serde_json::Value::Null),
        tool_files: pc.tool_files,
        grok_bot: *id == ProviderId::Cursor && grok_bot_wanted(config),
    }
}

fn build_one(id: &ProviderId, pc: ProviderConfig) -> Arc<dyn Provider> {
    match id {
        ProviderId::Claude => Arc::new(ClaudeProvider::new(pc)),
        ProviderId::Codex => Arc::new(CodexProvider::new(pc)),
        ProviderId::Copilot => Arc::new(CopilotProvider::new(pc)),
        ProviderId::Antigravity => Arc::new(AntigravityProvider::new(pc)),
        ProviderId::Grok => Arc::new(GrokProvider::new(pc)),
        // Grok Bot is asked for only while its row is chosen (the key's
        // `grok_bot`, which rebuilds Cursor when the choice changes).
        ProviderId::Cursor => Arc::new(CursorProvider::new(pc)),
    }
}

/// A provider rebuilt by `ProviderSet::sync`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Rebuilt {
    pub id: ProviderId,
    /// Its source or account changed: what it showed goes (Loading until
    /// the new instance answers), and so does its cached data.
    pub reload: bool,
}

/// The providers the scheduler polls (shared with it), each under the
/// serial of the instance that is current.
pub(super) struct ProviderSet {
    shared: SharedProviders,
    keys: HashMap<ProviderId, BuildKey>,
    thresholds: HashMap<ProviderId, u8>,
    next_serial: u64,
}

impl ProviderSet {
    /// Every provider of the config, built - including those switched off,
    /// whose fetch returns at once.
    pub(super) fn new(config: &Config) -> Self {
        let mut set = Self {
            shared: Arc::new(RwLock::new(Vec::new())),
            keys: HashMap::new(),
            thresholds: HashMap::new(),
            next_serial: 0,
        };
        set.sync(config, None);
        set
    }

    /// The list the scheduler polls.
    pub(super) fn shared(&self) -> SharedProviders {
        self.shared.clone()
    }

    fn slots(&self) -> Vec<ProviderSlot> {
        match self.shared.read() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// The providers, in config order.
    pub(super) fn ids(&self) -> Vec<ProviderId> {
        self.slots().iter().map(|s| s.provider.id()).collect()
    }

    /// The provider's alert threshold, %.
    pub(super) fn threshold(&self, id: &ProviderId) -> u8 {
        self.thresholds.get(id).copied().unwrap_or(80)
    }

    /// Whether an update made under `serial` came from the provider's
    /// current instance. One that does not was in flight while the provider
    /// was rebuilt - signed out, signed in as someone else, its key or its
    /// consent changed - and its numbers are not to be shown or cached.
    pub(super) fn is_current(&self, id: &ProviderId, serial: u64) -> bool {
        self.slots()
            .iter()
            .any(|s| s.serial == serial && s.provider.id() == *id)
    }

    /// Bring the instances in line with the config: a provider whose build
    /// key changed is rebuilt, `force` is rebuilt whatever its key (its
    /// account changed outside the config: a sign-in, a key in Credential
    /// Manager), every other instance is kept. Returns what was rebuilt.
    pub(super) fn sync(&mut self, config: &Config, force: Option<&str>) -> Vec<Rebuilt> {
        let forced = force.and_then(ProviderId::from_config_id);
        let old = self.slots();
        let mut slots = Vec::with_capacity(config.providers.len());
        let mut keys = HashMap::new();
        let mut thresholds = HashMap::new();
        let mut rebuilt = Vec::new();
        for pc in &config.providers {
            let Some(id) = ProviderId::from_config_id(&pc.id) else {
                warn!("unknown provider '{}' in config, skipping", pc.id);
                continue;
            };
            if keys.contains_key(&id) {
                warn!("provider '{}' is in the config twice, skipping", pc.id);
                continue;
            }
            let built = built_config(config, pc);
            let key = build_key(config, &id, &built);
            thresholds.insert(id.clone(), built.alert_threshold);
            let was = self.keys.get(&id);
            let current = old.iter().find(|s| s.provider.id() == id);
            match current {
                Some(slot) if forced.as_ref() != Some(&id) && was == Some(&key) => {
                    slots.push(slot.clone());
                }
                _ => {
                    self.next_serial += 1;
                    let reload = forced.as_ref() == Some(&id)
                        || was.is_some_and(|was| was.source_differs(&key));
                    debug!(
                        "provider {id:?} built as #{}{}",
                        self.next_serial,
                        if reload { ", reloading" } else { "" }
                    );
                    if current.is_some() {
                        rebuilt.push(Rebuilt {
                            id: id.clone(),
                            reload,
                        });
                    }
                    slots.push(ProviderSlot {
                        serial: self.next_serial,
                        provider: build_one(&id, built),
                    });
                }
            }
            keys.insert(id, key);
        }
        self.keys = keys;
        self.thresholds = thresholds;
        match self.shared.write() {
            Ok(mut guard) => *guard = slots,
            Err(poisoned) => *poisoned.into_inner() = slots,
        }
        rebuilt
    }

    /// Out-of-band fetch of a single provider.
    pub(super) fn fetch(
        &self,
        id: &ProviderId,
        cmd_tx: &mpsc::Sender<AppCommand>,
        runtime: &tokio::runtime::Handle,
    ) {
        for slot in self.slots() {
            if slot.provider.id() != *id {
                continue;
            }
            let tx = cmd_tx.clone();
            runtime.spawn(async move {
                if let Ok(data) = slot.provider.fetch().await {
                    let update = AppCommand::UpdateProvider {
                        data,
                        serial: slot.serial,
                    };
                    let _ = tx.send(update).await;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serial_of(set: &ProviderSet, id: &ProviderId) -> u64 {
        set.slots()
            .iter()
            .find(|s| s.provider.id() == *id)
            .unwrap()
            .serial
    }

    #[test]
    fn grok_bot_is_asked_for_only_while_its_row_is_chosen() {
        assert!(rows::SUB_ROWS.iter().any(|r| r.id == GROK_BOT_ROW));
        let mut config = Config::default();
        assert!(!grok_bot_wanted(&config), "not by default");
        config.ui.rows.widget = Some(vec!["cursor".into(), GROK_BOT_ROW.into()]);
        assert!(grok_bot_wanted(&config));
        config.ui.rows.widget = Some(vec!["cursor".into()]);
        config.ui.rows.tooltip = Some(vec![GROK_BOT_ROW.into()]);
        assert!(grok_bot_wanted(&config), "the tooltip counts too");
    }

    #[test]
    fn choosing_the_grok_bot_row_rebuilds_cursor_alone_and_keeps_its_data() {
        let mut config = Config::default();
        config.allow_app_sign_in("cursor");
        config.allow_sign_in("claude");
        let mut set = ProviderSet::new(&config);
        let claude = serial_of(&set, &ProviderId::Claude);
        config.ui.rows.widget = Some(vec!["cursor".into(), GROK_BOT_ROW.into()]);
        let rebuilt = set.sync(&config, None);
        assert_eq!(
            rebuilt,
            [Rebuilt {
                id: ProviderId::Cursor,
                reload: false
            }]
        );
        assert_eq!(serial_of(&set, &ProviderId::Claude), claude);
    }

    #[test]
    fn an_unchanged_config_keeps_every_instance() {
        let mut config = Config::default();
        config.allow_sign_in("claude");
        config.allow_sign_in("codex");
        let mut set = ProviderSet::new(&config);
        let before: Vec<u64> = set.slots().iter().map(|s| s.serial).collect();
        assert!(set.sync(&config, None).is_empty());
        let after: Vec<u64> = set.slots().iter().map(|s| s.serial).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn an_update_from_before_a_sign_out_is_not_current() {
        let mut config = Config::default();
        config.allow_app_sign_in("cursor");
        let mut set = ProviderSet::new(&config);
        let in_flight = serial_of(&set, &ProviderId::Cursor);
        assert!(set.is_current(&ProviderId::Cursor, in_flight));
        // Signed out (or in as someone else): the account changed outside
        // the config, so the sign-out forces the rebuild.
        let rebuilt = set.sync(&config, Some("cursor"));
        assert_eq!(
            rebuilt,
            [Rebuilt {
                id: ProviderId::Cursor,
                reload: true
            }]
        );
        assert!(!set.is_current(&ProviderId::Cursor, in_flight));
        // Another provider's update is not confused with it.
        assert!(!set.is_current(&ProviderId::Claude, in_flight));
    }

    #[test]
    fn a_consent_taken_back_rebuilds_and_reloads_that_provider() {
        let mut config = Config::default();
        config.allow_sign_in("claude");
        config.allow_sign_in("codex");
        let mut set = ProviderSet::new(&config);
        let codex = serial_of(&set, &ProviderId::Codex);
        config.revoke("claude");
        let rebuilt = set.sync(&config, None);
        assert_eq!(
            rebuilt,
            [Rebuilt {
                id: ProviderId::Claude,
                reload: true
            }]
        );
        assert_eq!(serial_of(&set, &ProviderId::Codex), codex);
    }

    #[test]
    fn a_tool_found_and_allowed_with_others_is_built_too() {
        // "Allow" for several tools at once changes several providers in one
        // go; each of them is rebuilt and fetched, not only the one named.
        let mut config = Config::default();
        let mut set = ProviderSet::new(&config);
        config.allow_sign_in("claude");
        config.allow_sign_in("codex");
        let ids: Vec<ProviderId> = set.sync(&config, None).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, [ProviderId::Claude, ProviderId::Codex]);
    }

    #[test]
    fn the_providers_keep_the_config_order_and_thresholds() {
        let config = Config::default();
        let set = ProviderSet::new(&config);
        let ids = set.ids();
        let expected: Vec<ProviderId> = config
            .providers
            .iter()
            .filter_map(|p| ProviderId::from_config_id(&p.id))
            .collect();
        assert_eq!(ids, expected);
        for pc in &config.providers {
            let id = ProviderId::from_config_id(&pc.id).unwrap();
            assert_eq!(set.threshold(&id), pc.alert_threshold);
        }
    }
}
