// choices.rs — what the picker shows (`PickerInput`) and what its actions do
// to the config. Pure: no window, no files; app.rs applies the result to the
// surfaces and saves the config.

use crate::config::schema::{Config, ModelPart, PendingNotice, Surface};
use crate::models::{self, Models};
use crate::providers::{ProviderData, ProviderId, ProviderStatus};
use crate::rows::{self, SUB_ROWS};
use crate::ui::picker::{ModelChoice, PickerAction, PickerInput, PickerMode, ReadKind, RowChoice};

/// Every row id there is, in display order: each provider of the config in
/// its order, its own row first, then its extra rows.
pub fn catalogue(config: &Config) -> Vec<&'static str> {
    let mut out = Vec::new();
    for pc in &config.providers {
        let Some(pid) = ProviderId::from_config_id(&pc.id) else {
            continue;
        };
        out.push(pid.config_id());
        out.extend(rows::sub_rows(&pid).map(|s| s.id));
    }
    out
}

/// The provider a row id belongs to ("claude.opus" -> "claude").
fn provider_of(row: &str) -> &str {
    row.split('.').next().unwrap_or(row)
}

/// Whether a provider runs (switched on and allowed), by config id.
fn active(config: &Config, provider: &str) -> bool {
    config
        .providers
        .iter()
        .any(|p| p.id == provider && config.provider_active(p))
}

/// The rows a surface shows, spelled out: chosen AND its provider runs. On
/// a fresh install that is nothing; with the default rows it is every
/// running provider's own row.
pub fn resolved_rows(config: &Config, surface: Surface) -> Vec<String> {
    catalogue(config)
        .into_iter()
        .filter(|id| active(config, provider_of(id)) && rows::row_chosen(config, surface, id))
        .map(String::from)
        .collect()
}

fn rows_slot(config: &mut Config, surface: Surface) -> &mut Option<Vec<String>> {
    match surface {
        Surface::Widget => &mut config.ui.rows.widget,
        Surface::Tooltip => &mut config.ui.rows.tooltip,
    }
}

/// Put a row in or out of a surface's list, spelling the default out the
/// first time. Unknown ids already in the list are kept.
fn set_row(config: &mut Config, surface: Surface, row: &str, on: bool) {
    let order = catalogue(config);
    let mut list = match rows_slot(config, surface).clone() {
        Some(list) => list,
        None => resolved_rows(config, surface),
    };
    list.retain(|r| r != row);
    if on {
        list.push(row.to_string());
    }
    // Catalogue order first, then whatever the catalogue does not know.
    let pos = |r: &String| order.iter().position(|o| o == r).unwrap_or(usize::MAX);
    list.sort_by_key(pos);
    *rows_slot(config, surface) = Some(list);
}

/// Whether any row of a provider is chosen on either surface.
fn has_any_row(config: &Config, provider: &str) -> bool {
    let Some(pid) = ProviderId::from_config_id(provider) else {
        return false;
    };
    let ids: Vec<&str> = std::iter::once(pid.config_id())
        .chain(rows::sub_rows(&pid).map(|s| s.id))
        .collect();
    [Surface::Widget, Surface::Tooltip]
        .iter()
        .any(|&surface| ids.iter().any(|id| rows::row_chosen(config, surface, id)))
}

/// A provider switched on from outside the picker (the Providers menu, a
/// pasted key, a Cursor sign-in) also gets its own row back on every
/// surface whose rows were chosen.
pub fn show_provider_rows(config: &mut Config, provider: &str) {
    let Some(pid) = ProviderId::from_config_id(provider) else {
        return;
    };
    for surface in [Surface::Widget, Surface::Tooltip] {
        let chosen = rows_slot(config, surface)
            .as_ref()
            .is_some_and(|l| !l.iter().any(|r| r == pid.config_id()));
        if chosen {
            set_row(config, surface, pid.config_id(), true);
        }
    }
}

/// What an applied action leaves for the app to do.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Applied {
    /// A provider was switched on or allowed: rebuild the providers and
    /// fetch it.
    pub provider: Option<String>,
    /// The model request may have changed: re-read the models.
    pub models: bool,
    /// The config changed: save it, re-lay the widget, repaint.
    pub changed: bool,
    /// The picker is to close.
    pub close: bool,
}

/// Apply one picker action to the config.
pub fn apply(config: &mut Config, action: &PickerAction) -> Applied {
    let mut out = Applied {
        changed: true,
        ..Applied::default()
    };
    match action {
        PickerAction::SetRow { surface, row, on } => {
            let provider = provider_of(row).to_string();
            if *on {
                if let Some(pc) = config.providers.iter_mut().find(|p| p.id == provider) {
                    if !pc.enabled {
                        pc.enabled = true;
                        out.provider = Some(provider.clone());
                    }
                }
            }
            set_row(config, *surface, row, *on);
            // No row of the provider left on either surface: the user is
            // done with it, so nothing of it is read any more ("Untick its
            // rows to stop"). Ticking a row again asks again.
            let allowed =
                config.sign_in_allowed(&provider) || config.app_sign_in_allowed(&provider);
            if !*on && allowed && !has_any_row(config, &provider) {
                config.revoke(&provider);
                out.provider = Some(provider);
            }
            out.models = out.provider.is_some();
        }
        PickerAction::SetPart { surface, part, on } => {
            let parts = match surface {
                Surface::Widget => &mut config.ui.model.widget,
                Surface::Tooltip => &mut config.ui.model.tooltip,
            };
            parts.retain(|p| p != part);
            if *on {
                parts.push(*part);
                // A fixed order keeps the saved file stable.
                parts.sort_by_key(|p| match p {
                    ModelPart::Model => 0,
                    ModelPart::Version => 1,
                    ModelPart::Effort => 2,
                });
            }
            out.models = true;
        }
        PickerAction::SetSource(source) => {
            config.ui.model.source = *source;
            out.models = true;
        }
        PickerAction::Rename { model_id, name } => {
            match name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                Some(n) => {
                    config
                        .ui
                        .model
                        .names
                        .insert(model_id.clone(), n.to_string());
                }
                None => {
                    config.ui.model.names.remove(model_id);
                }
            }
        }
        PickerAction::Allow(kind) => match kind {
            ReadKind::SignIn(provider) => {
                config.allow_sign_in(provider);
                if let Some(pc) = config.providers.iter_mut().find(|p| p.id == *provider) {
                    pc.enabled = true;
                }
                out.provider = Some(provider.clone());
                out.models = true;
            }
            // The picker sends one SignIn per tool found instead (each
            // provider then starts like a ticked one); allowed together
            // here all the same.
            ReadKind::FoundTools(ids) => {
                for provider in ids {
                    config.allow_sign_in(provider);
                    if let Some(pc) = config.providers.iter_mut().find(|p| p.id == *provider) {
                        pc.enabled = true;
                    }
                }
                out.models = true;
            }
            ReadKind::ToolSettings => {
                config.reads.tool_settings = true;
                out.models = true;
            }
            ReadKind::LastSession => {
                config.reads.last_session = true;
                out.models = true;
            }
        },
        PickerAction::NoticeDone => {
            config.reads.pending = PendingNotice::None;
        }
        PickerAction::Close => {
            out.changed = false;
            out.close = true;
        }
    }
    out
}

/// The note under a provider's own row in the picker: in an error, the
/// widget's own words for it (`renderer::error_words`), so the two never
/// disagree; before any data, "No data yet".
fn base_note(data: Option<&ProviderData>) -> std::borrow::Cow<'static, str> {
    match data.map(|d| &d.status) {
        Some(ProviderStatus::Ok) => "Today's row".into(),
        Some(status) => crate::ui::renderer::error_words(status)
            .filter(|w| !w.trim().is_empty())
            .unwrap_or_else(|| "No data yet".into()),
        None => "No data yet".into(),
    }
}

/// Everything the picker shows.
pub fn picker_input(
    config: &Config,
    display: &[ProviderData],
    models: &Models,
    mode: PickerMode,
    found: &[&'static str],
) -> PickerInput {
    let mut rows_out = Vec::new();
    for pc in &config.providers {
        let Some(pid) = ProviderId::from_config_id(&pc.id) else {
            continue;
        };
        let data = display.iter().find(|d| d.id == pid);
        rows_out.push(RowChoice {
            id: pid.config_id(),
            provider: pid.config_id(),
            name: pid.display_name(),
            note: base_note(data),
            hint: "",
            has_data: data.is_some_and(|d| matches!(d.status, ProviderStatus::Ok)),
        });
        for sub in SUB_ROWS.iter().filter(|s| s.provider == pid) {
            let has = data.is_some_and(|d| {
                d.metrics
                    .iter()
                    .any(|m| m.label.eq_ignore_ascii_case(sub.metric))
            });
            rows_out.push(RowChoice {
                id: sub.id,
                provider: pid.config_id(),
                name: sub.names[0],
                note: sub.note.into(),
                hint: sub.hint,
                has_data: has,
            });
        }
    }
    // Every model each running tool offers (models::catalog), the one it
    // uses now first. Two tools on the same model id share one name: one
    // field, under the first. An effort with no model (an empty id) has no
    // name to change.
    let mut model_choices: Vec<ModelChoice> = Vec::new();
    for pc in config
        .providers
        .iter()
        .filter(|p| config.provider_active(p))
    {
        let Some(pid) = ProviderId::from_config_id(&pc.id) else {
            continue;
        };
        let current = models
            .get(&pid)
            .map(|info| info.id.as_str())
            .filter(|id| !id.is_empty());
        let ids = current
            .into_iter()
            .chain(models::catalog::known(&pid).iter().copied());
        for id in ids {
            if model_choices.iter().any(|c| c.model_id == id) {
                continue;
            }
            model_choices.push(ModelChoice {
                provider: pid.config_id(),
                model_id: id.to_string(),
                automatic: models::name_parts(id, &pid, None).with_version,
                renamed: config.ui.model.names.get(id).cloned(),
                current: current == Some(id),
            });
        }
    }
    PickerInput {
        mode,
        rows: rows_out,
        widget_rows: resolved_rows(config, Surface::Widget),
        tooltip_rows: resolved_rows(config, Surface::Tooltip),
        model: config.ui.model.clone(),
        reads: config.reads.clone(),
        enabled: config
            .providers
            .iter()
            .filter(|p| p.enabled)
            .filter_map(|p| ProviderId::from_config_id(&p.id).map(|i| i.config_id()))
            .collect(),
        found: found.to_vec(),
        models: model_choices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::ModelSource;
    use crate::models::ModelInfo;

    fn row(surface: Surface, row: &str, on: bool) -> PickerAction {
        PickerAction::SetRow {
            surface,
            row: row.to_string(),
            on,
        }
    }

    #[test]
    fn a_fresh_install_shows_nothing_ticked() {
        let config = Config::default();
        assert!(resolved_rows(&config, Surface::Widget).is_empty());
        assert!(resolved_rows(&config, Surface::Tooltip).is_empty());
    }

    #[test]
    fn the_first_tick_spells_out_only_what_runs() {
        let mut config = Config::default();
        apply(
            &mut config,
            &PickerAction::Allow(ReadKind::SignIn("claude".into())),
        );
        apply(&mut config, &row(Surface::Widget, "claude", true));
        assert_eq!(
            config.ui.rows.widget.as_deref(),
            Some(&["claude".to_string()][..])
        );
        // The tooltip was not touched: still the default, so Claude shows
        // there too, being allowed now.
        assert!(config.ui.rows.tooltip.is_none());
        assert_eq!(resolved_rows(&config, Surface::Tooltip), ["claude"]);
        // A second provider allowed from the tooltip column stays out of
        // the widget, whose rows were chosen.
        apply(
            &mut config,
            &PickerAction::Allow(ReadKind::SignIn("codex".into())),
        );
        apply(&mut config, &row(Surface::Tooltip, "codex", true));
        assert_eq!(resolved_rows(&config, Surface::Widget), ["claude"]);
        assert_eq!(
            resolved_rows(&config, Surface::Tooltip),
            ["claude", "codex"]
        );
    }

    #[test]
    fn rows_keep_catalogue_order_and_unknown_ids() {
        let mut config = Config::default();
        for p in ["claude", "codex"] {
            config.allow_sign_in(p);
        }
        config.ui.rows.widget = Some(vec!["codex".into(), "future.row".into()]);
        apply(&mut config, &row(Surface::Widget, "claude.opus", true));
        apply(&mut config, &row(Surface::Widget, "claude", true));
        assert_eq!(
            config.ui.rows.widget.unwrap(),
            ["claude", "claude.opus", "codex", "future.row"]
        );
    }

    #[test]
    fn ticking_a_row_of_a_switched_off_provider_switches_it_on() {
        let mut config = Config::default();
        config.allow_sign_in("grok");
        config
            .providers
            .iter_mut()
            .find(|p| p.id == "grok")
            .unwrap()
            .enabled = false;
        let done = apply(&mut config, &row(Surface::Widget, "grok.build", true));
        assert_eq!(done.provider.as_deref(), Some("grok"));
        assert!(config.providers.iter().any(|p| p.id == "grok" && p.enabled));
        // Unticking never switches a provider off: the panel and the tray
        // still show it.
        let done = apply(&mut config, &row(Surface::Widget, "grok.build", false));
        assert!(done.provider.is_none());
        assert!(config.providers.iter().any(|p| p.id == "grok" && p.enabled));
    }

    #[test]
    fn parts_source_names_and_notice() {
        let mut config = Config::default();
        let on = |part| PickerAction::SetPart {
            surface: Surface::Tooltip,
            part,
            on: true,
        };
        apply(&mut config, &on(ModelPart::Effort));
        apply(&mut config, &on(ModelPart::Model));
        apply(&mut config, &on(ModelPart::Model));
        assert_eq!(
            config.ui.model.tooltip,
            [ModelPart::Model, ModelPart::Effort]
        );
        assert!(apply(&mut config, &PickerAction::SetSource(ModelSource::LastUsed)).models);
        assert_eq!(config.ui.model.source, ModelSource::LastUsed);
        let rename = |name: Option<&str>| PickerAction::Rename {
            model_id: "gpt-6.1-sol".into(),
            name: name.map(String::from),
        };
        apply(&mut config, &rename(Some(" Sol ")));
        assert_eq!(config.ui.model.names["gpt-6.1-sol"], "Sol");
        apply(&mut config, &rename(Some("   ")));
        assert!(config.ui.model.names.is_empty());
        config.reads.pending = PendingNotice::UpdateNotice;
        apply(&mut config, &PickerAction::NoticeDone);
        assert_eq!(config.reads.pending, PendingNotice::None);
        let close = apply(&mut config, &PickerAction::Close);
        assert!(close.close && !close.changed);
    }

    #[test]
    fn unticking_every_row_of_a_provider_stops_reading_it() {
        let mut config = Config::default();
        apply(
            &mut config,
            &PickerAction::Allow(ReadKind::SignIn("claude".into())),
        );
        apply(&mut config, &row(Surface::Widget, "claude.opus", true));
        // Off in the widget; the tooltip still shows it by default.
        let done = apply(&mut config, &row(Surface::Widget, "claude", false));
        assert!(done.provider.is_none());
        assert!(config.sign_in_allowed("claude"));
        let done = apply(&mut config, &row(Surface::Tooltip, "claude", false));
        assert!(done.provider.is_none(), "the Opus row is still on");
        let done = apply(&mut config, &row(Surface::Widget, "claude.opus", false));
        assert_eq!(done.provider.as_deref(), Some("claude"));
        assert!(!config.sign_in_allowed("claude"));
        let pc = config.providers.iter().find(|p| p.id == "claude").unwrap();
        assert!(!pc.enabled && !config.provider_active(pc));
        // The app's own sign-in goes too. Both surfaces are spelled out by
        // now and neither has Cursor's row, so one untick ends it.
        config.allow_app_sign_in("cursor");
        config
            .providers
            .iter_mut()
            .find(|p| p.id == "cursor")
            .unwrap()
            .enabled = true;
        let done = apply(&mut config, &row(Surface::Widget, "cursor", false));
        assert_eq!(done.provider.as_deref(), Some("cursor"));
        assert!(!config.app_sign_in_allowed("cursor"));
    }

    #[test]
    fn one_name_field_per_model_id() {
        let mut config = Config::default();
        config.allow_sign_in("codex");
        config.allow_sign_in("copilot");
        let mut models = Models::new();
        for p in [ProviderId::Codex, ProviderId::Copilot] {
            models.insert(
                p,
                ModelInfo {
                    id: "gpt-5".into(),
                    effort: None,
                },
            );
        }
        let input = picker_input(&config, &[], &models, PickerMode::Normal, &[]);
        let fields = |id: &str| input.models.iter().filter(|m| m.model_id == id).count();
        assert_eq!(fields("gpt-5"), 1);
        assert_eq!(input.models[0].model_id, "gpt-5");
        assert_eq!(input.models[0].provider, "codex");
        // A model both tools offer is one field too, under the first.
        assert_eq!(fields("gpt-5.6-sol"), 1);
        let sol = input
            .models
            .iter()
            .find(|m| m.model_id == "gpt-5.6-sol")
            .unwrap();
        assert_eq!(sol.provider, "codex");
        // Copilot keeps the rest of its list.
        assert!(input
            .models
            .iter()
            .any(|m| m.provider == "copilot" && m.model_id == "auto"));
    }

    #[test]
    fn allowing_reads_asks_for_models() {
        let mut config = Config::default();
        assert!(apply(&mut config, &PickerAction::Allow(ReadKind::ToolSettings)).models);
        assert!(config.reads.tool_settings);
        assert!(apply(&mut config, &PickerAction::Allow(ReadKind::LastSession)).models);
        assert!(config.reads.last_session);
    }

    #[test]
    fn a_provider_switched_on_elsewhere_gets_its_row_back() {
        let mut config = Config::default();
        config.allow_sign_in("claude");
        config.ui.rows.widget = Some(vec!["claude.opus".into()]);
        show_provider_rows(&mut config, "claude");
        assert_eq!(config.ui.rows.widget.unwrap(), ["claude", "claude.opus"]);
        // A surface still on the default is left alone.
        assert!(config.ui.rows.tooltip.is_none());
    }

    #[test]
    fn the_input_lists_every_row_and_the_known_models() {
        let mut config = Config::default();
        config.allow_sign_in("codex");
        let mut models = Models::new();
        models.insert(
            ProviderId::Codex,
            ModelInfo {
                id: "gpt-6.1-sol".into(),
                effort: Some("high".into()),
            },
        );
        let input = picker_input(&config, &[], &models, PickerMode::Normal, &["codex"]);
        let ids: Vec<&str> = input.rows.iter().map(|r| r.id).collect();
        assert_eq!(&ids[..3], ["claude", "claude.opus", "claude.sonnet"]);
        assert_eq!(ids.len(), 6 + SUB_ROWS.len());
        assert_eq!(input.widget_rows, ["codex"]);
        // Every model Codex offers, the one in use first and marked.
        let known = crate::models::catalog::known(&ProviderId::Codex);
        assert_eq!(input.models.len(), known.len());
        assert_eq!(input.models[0].model_id, "gpt-6.1-sol");
        assert_eq!(input.models[0].automatic, "GPT-6.1 Sol");
        assert!(input.models[0].current);
        assert!(input.models[1..]
            .iter()
            .all(|m| !m.current && m.provider == "codex"));
        // A tool not running offers nothing to rename.
        assert!(input.models.iter().all(|m| m.provider == "codex"));
        assert_eq!(input.found, ["codex"]);
    }
}
