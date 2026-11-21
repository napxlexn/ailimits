// choices.rs — what the picker shows (`PickerInput`) and what its actions do
// to the config. Pure: no window, no files; app.rs applies the result to the
// surfaces and saves the config.

use crate::config::schema::{Config, ModelPart, ModelSource, NumbersOn, PendingNotice, Surface};
use crate::models::{self, Models};
use crate::providers::{ProviderData, ProviderId, ProviderStatus};
use crate::rows;
use crate::ui::picker::{
    CursorState, ModelChoice, PickerAction, PickerInput, PickerMode, ReadKind, RowChoice,
    SignInChoice, SignInSource, SourceId,
};
use std::collections::BTreeMap;

/// Every row id there is, in display order: each provider of the config in
/// its order, its own row first, then its extra rows.
pub fn catalogue(config: &Config) -> Vec<&'static str> {
    catalogue_with(config, &[])
}

/// `catalogue`, with the rows `display` adds: the week of a model Claude
/// reports that the catalogue does not name (`rows::offered_rows`).
fn catalogue_with(config: &Config, display: &[ProviderData]) -> Vec<&'static str> {
    let mut out = Vec::new();
    for pc in &config.providers {
        let Some(pid) = ProviderId::from_config_id(&pc.id) else {
            continue;
        };
        out.push(pid.config_id());
        let data = display.iter().find(|d| d.id == pid);
        out.extend(rows::offered_rows(&pid, data).into_iter().map(|s| s.id));
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
    resolved_rows_with(config, surface, &[])
}

/// `resolved_rows`, with the rows `display` adds (`catalogue_with`).
fn resolved_rows_with(config: &Config, surface: Surface, display: &[ProviderData]) -> Vec<String> {
    catalogue_with(config, display)
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

/// Whether any row of a provider is chosen on either surface, a model's
/// week Claude reported among them (`rows::provider_row_chosen`).
fn has_any_row(config: &Config, provider: &str) -> bool {
    let Some(pid) = ProviderId::from_config_id(provider) else {
        return false;
    };
    [Surface::Widget, Surface::Tooltip]
        .iter()
        .any(|&surface| rows::provider_row_chosen(config, surface, pid.config_id()))
        || rows::on_taskbar(config, pid.config_id())
}

/// Put a provider on the taskbar panel and the tray icon or take it off,
/// spelling the default (the first two that run) out the first time. The
/// list keeps the widget's order and at most `rows::TASKBAR_MAX`; the last
/// one is never taken off (nothing left would mean the default again, ticks
/// jumping back), and a provider that would make one too many is refused.
/// Whether anything changed.
fn set_taskbar(config: &mut Config, provider: &str, on: bool) -> bool {
    let mut list: Vec<String> = rows::taskbar_ticked(config)
        .into_iter()
        .map(String::from)
        .collect();
    let had = list.iter().any(|p| p == provider);
    if had == on {
        return false;
    }
    if on {
        if list.len() >= rows::TASKBAR_MAX {
            return false;
        }
        list.push(provider.to_string());
    } else {
        if list.len() <= 1 {
            return false;
        }
        list.retain(|p| p != provider);
    }
    let order = catalogue(config);
    list.sort_by_key(|p| order.iter().position(|o| o == p).unwrap_or(usize::MAX));
    config.ui.rows.taskbar = Some(list);
    true
}

/// A provider set up with the app's own sign-in (a pasted key or usage
/// token, a Cursor sign-in) also gets its own row back on every surface
/// whose rows were chosen.
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

// ─── Model names ─────────────────────────────────────────────────

/// Whether a model id is a tool's own router rather than a model: "auto"
/// in Copilot and in Cursor are two different things, so each tool names
/// its own.
fn tool_specific(id: &str) -> bool {
    id.eq_ignore_ascii_case("auto") || id.eq_ignore_ascii_case("default")
}

/// The key a model's name is kept under in `[ui.model] names`: the id the
/// tool stores ("gpt-6.1-sol", one name for every tool that offers it), or
/// for a tool's own router "provider:id" ("copilot:auto").
pub fn model_key(provider: &ProviderId, id: &str) -> String {
    if tool_specific(id) {
        format!("{}:{id}", provider.config_id())
    } else {
        id.to_string()
    }
}

/// The user's name for a tool's model: under its key, or for a router the
/// name an earlier version kept under the bare id. The widget's labels
/// (`rows::model_label`) must look names up through this as well, so the
/// picker and the widget agree.
pub fn model_name<'a>(
    names: &'a BTreeMap<String, String>,
    provider: &ProviderId,
    id: &str,
) -> Option<&'a str> {
    names
        .get(&model_key(provider, id))
        .or_else(|| tool_specific(id).then(|| names.get(id)).flatten())
        .map(String::as_str)
}

/// Rename a model by its key. A router's name kept under the bare id by an
/// earlier version applied to every tool: it is split first, so each tool
/// keeps it until renamed on its own.
fn rename(config: &mut Config, key: &str, name: Option<&str>) {
    let names = &mut config.ui.model.names;
    if let Some((_, id)) = key.split_once(':').filter(|(_, id)| tool_specific(id)) {
        if let Some(old) = names.remove(id) {
            for pc in &config.providers {
                let Some(pid) = ProviderId::from_config_id(&pc.id) else {
                    continue;
                };
                if models::catalog::known(&pid).contains(&id) {
                    names
                        .entry(model_key(&pid, id))
                        .or_insert_with(|| old.clone());
                }
            }
        }
    }
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => {
            names.insert(key.to_string(), n.to_string());
        }
        None => {
            names.remove(key);
        }
    }
}

// ─── Applying an action ──────────────────────────────────────────

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
        PickerAction::SetTaskbar { provider, on } => {
            if *on {
                if let Some(pc) = config.providers.iter_mut().find(|p| p.id == *provider) {
                    if !pc.enabled {
                        pc.enabled = true;
                        out.provider = Some(provider.clone());
                    }
                }
            }
            if !set_taskbar(config, provider, *on) && out.provider.is_none() {
                out.changed = false;
                return out;
            }
            // Off the taskbar with no row of it left anywhere: done with it,
            // as an unticked row (`SetRow`).
            let allowed = config.sign_in_allowed(provider) || config.app_sign_in_allowed(provider);
            if !*on && allowed && !has_any_row(config, provider) {
                config.revoke(provider);
                out.provider = Some(provider.clone());
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
            // No part left on either surface: nothing of the tools' files is
            // read for the label any more, and the next tick asks again
            // ("Untick Model, Version and Effort to stop").
            if config.ui.model.widget.is_empty() && config.ui.model.tooltip.is_empty() {
                config.reads.tool_settings = false;
                config.reads.last_session = false;
            }
            out.models = true;
        }
        PickerAction::SetTip { part, place, on } => {
            let numbers = match place {
                NumbersOn::Tooltip => &mut config.ui.tooltip,
                NumbersOn::Panel => &mut config.ui.panel,
            };
            let before = *numbers;
            numbers.set(*part, *on);
            out.changed = *numbers != before;
        }
        PickerAction::SetSource(source) => {
            config.ui.model.source = *source;
            // Back to the tools' settings: the sessions are not read any
            // more, and Last used asks again ("Switch back to As set in the
            // tool to stop").
            if *source == ModelSource::Configured {
                config.reads.last_session = false;
            }
            out.models = true;
        }
        PickerAction::Rename { key, name } => rename(config, key, name.as_deref()),
        PickerAction::Allow(kind) => match kind {
            ReadKind::SignIn(provider) => {
                config.allow_sign_in(provider);
                if let Some(pc) = config.providers.iter_mut().find(|p| p.id == *provider) {
                    pc.enabled = true;
                }
                out.provider = Some(provider.clone());
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
        // Allowed, not switched on: nothing runs until a row is ticked.
        PickerAction::Consent(provider) => config.allow_sign_in(provider),
        // The sign-in's actions reach the Credential Manager, the clipboard
        // and the browser: the app carries them out (app/flow.rs).
        PickerAction::SetSignInSource { .. }
        | PickerAction::PasteSecret(_)
        | PickerAction::RemoveSecret(_)
        | PickerAction::CursorSignIn
        | PickerAction::CursorCancel
        | PickerAction::CursorSignOut
        // The settings file opens in Notepad (app/config_watch.rs).
        | PickerAction::OpenSettingsFile => out.changed = false,
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

// ─── What the picker shows ───────────────────────────────────────

/// The note under a provider's own row in the picker: in an error, the
/// widget's own words for it (`renderer::error_words`), so the two never
/// disagree; a running provider with nothing to sign in with says what is
/// missing; before any data, "No data yet".
fn base_note(
    data: Option<&ProviderData>,
    pid: &ProviderId,
    runs: bool,
) -> std::borrow::Cow<'static, str> {
    match data.map(|d| &d.status) {
        Some(ProviderStatus::Ok) => "Today's row".into(),
        Some(ProviderStatus::NotConfigured) if runs => match pid {
            ProviderId::Cursor => "Not signed in: open its Sign-in below".into(),
            _ => "No sign-in found on this PC".into(),
        },
        Some(status) => crate::ui::renderer::error_words(status)
            .filter(|w| !w.trim().is_empty())
            .unwrap_or_else(|| "No data yet".into()),
        None => "No data yet".into(),
    }
}

/// The note under an extra row whose numbers are asked for only while it is
/// chosen (`SubRow::on_request`), while it is not.
pub const ON_REQUEST_NOTE: &str = "Its share of the same limit, asked for once this row is ticked";

/// Everything the picker shows.
pub fn picker_input(
    config: &Config,
    display: &[ProviderData],
    models: &Models,
    mode: PickerMode,
    found: &[&'static str],
    sign_in: &SignInState,
) -> PickerInput {
    let mut input = picker_input_with(
        config,
        display,
        models,
        mode,
        found,
        crate::consent::found_surely,
    );
    input.sign_in = sign_in_choices(config, display, sign_in);
    input
}

/// `picker_input`, with whether a tool found was found surely (by a folder
/// of its own: `consent::found_surely`) given, for tests.
pub(crate) fn picker_input_with(
    config: &Config,
    display: &[ProviderData],
    models: &Models,
    mode: PickerMode,
    found: &[&'static str],
    surely: impl Fn(&str) -> bool,
) -> PickerInput {
    let mut rows_out = Vec::new();
    for pc in &config.providers {
        let Some(pid) = ProviderId::from_config_id(&pc.id) else {
            continue;
        };
        let data = display.iter().find(|d| d.id == pid);
        let ok = data.is_some_and(|d| matches!(d.status, ProviderStatus::Ok));
        rows_out.push(RowChoice {
            id: pid.config_id(),
            provider: pid.config_id(),
            name: pid.display_name(),
            note: base_note(data, &pid, config.provider_active(pc)),
            hint: "",
            has_data: ok,
            provider_ok: ok,
        });
        for sub in rows::offered_rows(&pid, data) {
            let has = data.is_some_and(|d| {
                d.metrics
                    .iter()
                    .any(|m| m.label.eq_ignore_ascii_case(sub.metric))
            });
            // A row asked for only while it is chosen (Grok Bot) is not
            // missing from an answer that never asked for it: it says when
            // it is asked for instead.
            let unasked = sub.on_request && !crate::rows::row_wanted(config, sub.id);
            // A limit the provider, having answered, does not report is one
            // this plan does not have (Claude's Opus and Sonnet weeks exist
            // only on some plans): its row is not offered. One chosen
            // earlier stays, so it can be unticked; one asked for only
            // while chosen cannot be reported before it is. Before the first
            // answer every row is offered: what the plan has is not known.
            let chosen = [Surface::Widget, Surface::Tooltip]
                .iter()
                .any(|&s| crate::rows::row_chosen(config, s, sub.id));
            if ok && !has && !sub.on_request && !chosen {
                continue;
            }
            rows_out.push(RowChoice {
                id: sub.id,
                provider: pid.config_id(),
                name: sub.names[0],
                note: if unasked {
                    ON_REQUEST_NOTE.into()
                } else {
                    sub.note.into()
                },
                hint: sub.hint,
                has_data: has,
                provider_ok: ok && !unasked,
            });
        }
    }
    PickerInput {
        mode,
        rows: rows_out,
        widget_rows: resolved_rows_with(config, Surface::Widget, display),
        tooltip_rows: resolved_rows_with(config, Surface::Tooltip, display),
        taskbar_rows: rows::taskbar_ticked(config)
            .into_iter()
            .map(String::from)
            .collect(),
        model: config.ui.model.clone(),
        tip: config.ui.tooltip,
        panel: config.ui.panel,
        reads: config.reads.clone(),
        enabled: config
            .providers
            .iter()
            .filter(|p| p.enabled)
            .filter_map(|p| ProviderId::from_config_id(&p.id).map(|i| i.config_id()))
            .collect(),
        found: found.to_vec(),
        found_weak: found.iter().copied().filter(|id| !surely(id)).collect(),
        models: model_choices(config, models),
        sign_in: Vec::new(),
    }
}

/// What the app knows of the sign-ins besides the config: which of its own
/// keys and tokens are stored, where Cursor's sign-in stands, and what the
/// last sign-in action of each provider came to.
#[derive(Debug, Clone, Default)]
pub struct SignInState {
    /// Credential Manager labels of the app's own keys and tokens that are
    /// stored (`providers::key_label_for`, `usage_token_label_for`).
    pub stored: Vec<&'static str>,
    pub cursor: Option<CursorState>,
    /// By config id.
    pub notes: BTreeMap<&'static str, String>,
}

/// The sources a provider's sign-in can come from, in the window's order.
pub fn sign_in_sources(provider: &str) -> &'static [SourceId] {
    match provider {
        "claude" => &[SourceId::Tool, SourceId::UsageToken, SourceId::Key],
        "codex" => &[SourceId::Tool, SourceId::UsageToken],
        "copilot" => &[SourceId::Tool, SourceId::Key],
        _ => &[],
    }
}

/// The source a provider's config says it uses.
pub fn source_of(method: &crate::config::schema::AuthMethod) -> SourceId {
    use crate::config::schema::AuthMethod;
    match method {
        AuthMethod::Subscription => SourceId::Tool,
        AuthMethod::ApiKey => SourceId::Key,
        AuthMethod::UsageToken => SourceId::UsageToken,
    }
}

/// The config's method for a source.
pub fn method_of(source: SourceId) -> crate::config::schema::AuthMethod {
    use crate::config::schema::AuthMethod;
    match source {
        SourceId::Tool => AuthMethod::Subscription,
        SourceId::Key => AuthMethod::ApiKey,
        SourceId::UsageToken => AuthMethod::UsageToken,
    }
}

/// The Credential Manager label a pasted secret of `source` is kept under.
pub fn secret_label(provider: &str, source: SourceId) -> Option<&'static str> {
    match source {
        SourceId::Tool => None,
        SourceId::Key => crate::providers::key_label_for(provider),
        SourceId::UsageToken => crate::providers::usage_token_label_for(provider),
    }
}

/// A source's name in the window: its radio, and the closed line.
fn source_names(provider: &str, source: SourceId) -> (&'static str, &'static str) {
    match (provider, source) {
        ("claude", SourceId::Tool) => ("Claude Code's sign-in", "Claude Code"),
        ("codex", SourceId::Tool) => ("Codex CLI's sign-in", "Codex CLI"),
        ("copilot", SourceId::Tool) => ("GitHub CLI's sign-in", "GitHub CLI"),
        ("antigravity", _) => ("Antigravity CLI's sign-in", "Antigravity CLI"),
        ("grok", _) => ("Grok CLI's sign-in", "Grok CLI"),
        (_, SourceId::UsageToken) => ("A usage token you paste", "Your usage token"),
        ("copilot", SourceId::Key) => ("A fine-grained token you paste", "Your token"),
        (_, SourceId::Key) => ("An API key you paste", "Your API key"),
        (_, SourceId::Tool) => ("The tool's sign-in", "The tool"),
    }
}

/// How a provider's last answer went, in a few words for the closed line.
fn how_it_went(data: Option<&ProviderData>) -> std::borrow::Cow<'static, str> {
    match data.map(|d| &d.status) {
        Some(ProviderStatus::Ok) | Some(ProviderStatus::Estimated) => "works".into(),
        Some(ProviderStatus::NotConfigured) => "no sign-in found".into(),
        Some(ProviderStatus::Loading) | None => "checking".into(),
        Some(status) => crate::ui::renderer::error_words(status)
            .filter(|w| !w.trim().is_empty())
            .unwrap_or_else(|| "not working".into()),
    }
}

/// Each provider's sign-in, as the window shows it under its rows: where it
/// comes from, the sources to choose from, what is read and where its token
/// goes (the consent texts, `consent::READ_INFO`), and the actions.
pub fn sign_in_choices(
    config: &Config,
    display: &[ProviderData],
    state: &SignInState,
) -> Vec<SignInChoice> {
    let mut out = Vec::new();
    for pc in &config.providers {
        let Some(pid) = ProviderId::from_config_id(&pc.id) else {
            continue;
        };
        let id = pid.config_id();
        let Some(info) = crate::consent::read_info(id) else {
            continue;
        };
        let data = display.iter().find(|d| d.id == pid);
        let hosts = info.hosts.join(" and ");
        let allowed = config.sign_in_allowed(id) || config.app_sign_in_allowed(id);
        let runs = config.provider_active(pc);
        let note = state.notes.get(id).cloned();
        if id == "cursor" {
            let cursor = state.cursor.unwrap_or(CursorState::SignedOut);
            let summary = match cursor {
                CursorState::Waiting => "Waiting for the browser".to_string(),
                CursorState::SignedOut => "Not signed in".to_string(),
                CursorState::SignedIn if runs => format!("Signed in \u{b7} {}", how_it_went(data)),
                CursorState::SignedIn => "Signed in".to_string(),
            };
            out.push(SignInChoice {
                provider: id,
                summary,
                sources: Vec::new(),
                detail: format!(
                    "Signs in to Cursor on its own page ({}) in your browser, as the Cursor editor does, and keeps that session in Windows Credential Manager; it goes only to {hosts}. Nothing of the editor's own sign-in is read.",
                    info.pages.join(", ")
                ),
                paste: None,
                stored: false,
                note,
                cursor: Some(cursor),
            });
            continue;
        }
        let choices = sign_in_sources(id);
        let chosen = if choices.is_empty() {
            SourceId::Tool
        } else {
            let s = source_of(&pc.auth_method);
            if choices.contains(&s) {
                s
            } else {
                SourceId::Tool
            }
        };
        let label = secret_label(id, chosen);
        let stored = label.is_some_and(|l| state.stored.contains(&l));
        let (_, short) = source_names(id, chosen);
        let summary = if !allowed {
            "Not allowed yet".to_string()
        } else if label.is_some() && !stored {
            format!("{short} \u{b7} none stored")
        } else if !runs {
            format!("{short} \u{b7} off")
        } else {
            format!("{short} \u{b7} {}", how_it_went(data))
        };
        let detail = match (chosen, label) {
            (SourceId::Tool, _) if id == "codex" => format!(
                "Reads {}; its {} goes only to {hosts}. Codex CLI keeps it there unless its cli_auth_credentials_store is \"keyring\" or \"auto\": then paste a usage token instead.",
                info.sign_in.join("; "),
                info.secret()
            ),
            (SourceId::Tool, _) | (_, None) => format!(
                "Reads {}; its {} goes only to {hosts}.",
                info.sign_in.join("; "),
                info.secret()
            ),
            (SourceId::Key, Some(l)) if id == "copilot" => format!(
                "Kept in Windows Credential Manager ({l}); it goes only to {hosts}. A fine-grained token needs the Copilot Requests permission."
            ),
            (_, Some(l)) => format!(
                "Kept in Windows Credential Manager ({l}); it goes only to {hosts}."
            ),
        };
        let paste = match chosen {
            SourceId::Tool => None,
            SourceId::UsageToken => Some("Paste token from clipboard"),
            SourceId::Key if id == "copilot" => Some("Paste token from clipboard"),
            SourceId::Key => Some("Paste key from clipboard"),
        };
        out.push(SignInChoice {
            provider: id,
            summary,
            sources: choices
                .iter()
                .map(|&s| SignInSource {
                    id: s,
                    label: source_names(id, s).0,
                    on: s == chosen,
                })
                .collect(),
            detail,
            paste,
            stored,
            note,
            cursor: None,
        });
    }
    out
}

/// The models each running tool is known to offer (models::catalog), the
/// one it uses now first, listed under every tool that offers it: a model
/// two tools share is one name, its field under each of them.
fn model_choices(config: &Config, models: &Models) -> Vec<ModelChoice> {
    let mut lists: Vec<(ProviderId, Option<String>, Vec<String>)> = Vec::new();
    for pc in config
        .providers
        .iter()
        .filter(|p| config.provider_active(p))
    {
        let Some(pid) = ProviderId::from_config_id(&pc.id) else {
            continue;
        };
        // An effort with no model (an empty id) has no name to change.
        let current = models
            .get(&pid)
            .map(|info| info.id.clone())
            .filter(|id| !id.is_empty());
        let mut ids: Vec<String> = Vec::new();
        for id in current
            .iter()
            .cloned()
            .chain(models::catalog::known(&pid).iter().map(|s| s.to_string()))
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        lists.push((pid, current, ids));
    }
    let names = &config.ui.model.names;
    let mut out = Vec::new();
    for (pid, current, ids) in &lists {
        for id in ids {
            let key = model_key(pid, id);
            let shared_with = lists
                .iter()
                .filter(|(other, _, other_ids)| {
                    other != pid && other_ids.iter().any(|o| model_key(other, o) == key)
                })
                .map(|(other, ..)| other.display_name())
                .collect();
            out.push(ModelChoice {
                provider: pid.config_id(),
                model_id: id.clone(),
                key,
                automatic: models::name_parts(id, pid, None).with_version,
                renamed: model_name(names, pid, id).map(String::from),
                current: current.as_deref() == Some(id.as_str()),
                shared_with,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ModelInfo;

    fn row(surface: Surface, row: &str, on: bool) -> PickerAction {
        PickerAction::SetRow {
            surface,
            row: row.to_string(),
            on,
        }
    }

    fn input(config: &Config, models: &Models, found: &[&'static str]) -> PickerInput {
        picker_input_with(config, &[], models, PickerMode::Normal, found, |_| true)
    }

    fn taskbar(id: &str, on: bool) -> PickerAction {
        PickerAction::SetTaskbar {
            provider: id.to_string(),
            on,
        }
    }

    /// The Taskbar column: untouched, no `taskbar` line in the file and
    /// the first two that run ticked. A change spells the choice out in
    /// the widget's order, at most two, never none. A provider left only
    /// on the taskbar keeps its sign-in; taken off there too, it stops.
    #[test]
    fn the_taskbar_choice_is_spelled_out_and_held_to_two() {
        let mut config = Config::default();
        for id in ["claude", "codex", "grok"] {
            config.allow_sign_in(id);
        }
        assert_eq!(rows::taskbar_ticked(&config), ["claude", "codex"]);
        assert!(!toml::to_string(&config).unwrap().contains("taskbar"));
        // Two ticked: a third is refused, nothing written.
        assert!(!apply(&mut config, &taskbar("grok", true)).changed);
        assert!(config.ui.rows.taskbar.is_none());
        apply(&mut config, &taskbar("codex", false));
        assert_eq!(config.ui.rows.taskbar, Some(vec!["claude".to_string()]));
        // Codex keeps running: its widget and tooltip rows are still on.
        assert!(config.sign_in_allowed("codex"));
        apply(&mut config, &taskbar("grok", true));
        apply(&mut config, &taskbar("claude", false));
        assert_eq!(rows::taskbar_ticked(&config), ["grok"]);
        // The last one stays.
        assert!(!apply(&mut config, &taskbar("grok", false)).changed);
        apply(&mut config, &taskbar("claude", true));
        // The widget's order, whatever the order of the clicks.
        assert_eq!(
            config.ui.rows.taskbar,
            Some(vec!["claude".to_string(), "grok".to_string()])
        );
        assert!(toml::to_string(&config).unwrap().contains("taskbar"));
        // Grok only on the taskbar: it keeps running.
        apply(&mut config, &row(Surface::Widget, "grok", false));
        apply(&mut config, &row(Surface::Tooltip, "grok", false));
        assert!(config.sign_in_allowed("grok"));
        // Off the taskbar too: done with it.
        apply(&mut config, &taskbar("grok", false));
        assert!(!config.sign_in_allowed("grok"));
        assert_eq!(rows::taskbar_ticked(&config), ["claude"]);
    }

    /// A tool switched off in the menu but allowed: a Taskbar tick
    /// switches it back on, as a row's tick does.
    #[test]
    fn a_taskbar_tick_switches_an_allowed_tool_back_on() {
        let mut config = Config::default();
        config.allow_sign_in("claude");
        config.allow_sign_in("grok");
        config
            .providers
            .iter_mut()
            .find(|p| p.id == "grok")
            .unwrap()
            .enabled = false;
        assert_eq!(rows::taskbar_ticked(&config), ["claude"]);
        let a = apply(&mut config, &taskbar("grok", true));
        assert_eq!(a.provider.as_deref(), Some("grok"));
        assert_eq!(rows::taskbar_ticked(&config), ["claude", "grok"]);
    }

    /// An extra row is offered only while its provider, having answered,
    /// reports it: a plan without an Opus or Sonnet week lists Fable alone.
    /// A row chosen earlier stays (to be unticked), a row asked for only
    /// while chosen stays, and before the first answer every row is there.
    #[test]
    fn only_the_limits_a_plan_reports_are_offered() {
        use crate::providers::{Metric, MetricUnit, MetricWindow};
        let metric = |label: &str| Metric {
            label: label.to_string(),
            used: 10,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: None,
            window: MetricWindow::Long,
            unified_pool: false,
            extra_only: false,
        };
        let mut config = Config::default();
        config.allow_sign_in("claude");
        config.allow_app_sign_in("cursor");
        let data = |id: ProviderId, labels: &[&str]| ProviderData {
            id,
            status: ProviderStatus::Ok,
            metrics: labels.iter().map(|l| metric(l)).collect(),
            updated_at: crate::utc::now(),
            received_at: None,
        };
        let display = [
            data(ProviderId::Claude, &["Session", "Weekly", "Fable"]),
            data(ProviderId::Cursor, &["Included"]),
        ];
        let offered = |config: &Config, display: &[ProviderData]| -> Vec<&'static str> {
            picker_input_with(
                config,
                display,
                &Models::new(),
                PickerMode::Normal,
                &[],
                |_| true,
            )
            .rows
            .iter()
            .map(|r| r.id)
            .filter(|id| id.starts_with("claude") || id.starts_with("cursor"))
            .collect()
        };
        assert_eq!(
            offered(&config, &display),
            ["claude", "claude.fable", "cursor", "cursor.bot"]
        );
        // Chosen before: still there, to be unticked.
        config.ui.rows.tooltip = Some(vec!["claude".into(), "claude.opus".into()]);
        assert_eq!(
            offered(&config, &display),
            [
                "claude",
                "claude.opus",
                "claude.fable",
                "cursor",
                "cursor.bot"
            ]
        );
        // No answer yet: what the plan has is not known.
        config.ui.rows.tooltip = None;
        assert_eq!(
            offered(&config, &[]),
            [
                "claude",
                "claude.opus",
                "claude.sonnet",
                "claude.fable",
                "cursor",
                "cursor.bot"
            ]
        );
    }

    /// The week of a model the catalogue does not name (one Claude adds
    /// later) is offered as a row of its own and ticks like any other; alone
    /// it keeps Claude's sign-in when Claude's own row is unticked.
    #[test]
    fn a_model_claude_adds_is_offered_and_ticks_like_any_row() {
        use crate::providers::{Metric, MetricUnit, MetricWindow};
        let metric = |label: &str, extra_only: bool| Metric {
            label: label.to_string(),
            used: 10,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: None,
            window: MetricWindow::Long,
            unified_pool: false,
            extra_only,
        };
        let mut config = Config::default();
        config.allow_sign_in("claude");
        let display = [ProviderData {
            id: ProviderId::Claude,
            status: ProviderStatus::Ok,
            metrics: vec![
                metric("Session", false),
                metric("Weekly", false),
                metric("Fable", true),
                metric("Haiku", true),
            ],
            updated_at: crate::utc::now(),
            received_at: None,
        }];
        let input = |config: &Config| {
            picker_input_with(
                config,
                &display,
                &Models::new(),
                PickerMode::Normal,
                &[],
                |_| true,
            )
        };
        let claude: Vec<&str> = input(&config)
            .rows
            .iter()
            .map(|r| r.id)
            .filter(|id| id.starts_with("claude"))
            .collect();
        assert_eq!(claude, ["claude", "claude.fable", "claude.haiku"]);
        let haiku = input(&config)
            .rows
            .into_iter()
            .find(|r| r.id == "claude.haiku")
            .unwrap();
        assert_eq!(haiku.name, "Claude · Haiku");
        assert!(haiku.has_data && haiku.provider_ok);
        apply(&mut config, &row(Surface::Widget, "claude.haiku", true));
        assert_eq!(input(&config).widget_rows, ["claude", "claude.haiku"]);
        // Claude's own row off both surfaces: the Haiku row keeps it running.
        apply(&mut config, &row(Surface::Widget, "claude", false));
        apply(&mut config, &row(Surface::Tooltip, "claude", false));
        assert!(config.sign_in_allowed("claude"));
        assert_eq!(input(&config).widget_rows, ["claude.haiku"]);
        // Its last row off: done with Claude.
        apply(&mut config, &row(Surface::Widget, "claude.haiku", false));
        assert!(!config.sign_in_allowed("claude"));
    }

    /// The tooltip's Percent and Reset time: untouched, both and nothing
    /// written; either can go, never both.
    #[test]
    fn the_tooltip_shows_the_percent_the_reset_time_or_both() {
        use crate::config::schema::{TipPart, UiNumbers};
        let tip = |part, on| PickerAction::SetTip {
            part,
            place: NumbersOn::Tooltip,
            on,
        };
        let mut config = Config::default();
        assert!(config.ui.tooltip.percent && config.ui.tooltip.reset);
        assert!(!toml::to_string(&config).unwrap().contains("[ui.tooltip]"));
        assert!(apply(&mut config, &tip(TipPart::Reset, false)).changed);
        assert_eq!(config.ui.tooltip, UiNumbers::PERCENT_ONLY);
        // The percentage alone: it cannot go.
        assert!(!apply(&mut config, &tip(TipPart::Percent, false)).changed);
        assert!(apply(&mut config, &tip(TipPart::Reset, true)).changed);
        assert!(apply(&mut config, &tip(TipPart::Percent, false)).changed);
        // The reset time alone: it cannot go.
        assert!(!apply(&mut config, &tip(TipPart::Reset, false)).changed);
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("[ui.tooltip]"), "{text}");
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.ui.tooltip, config.ui.tooltip);
        // A file without the section, as 0.7.0 wrote it, reads as both.
        let old: Config = toml::from_str("[ui]\nmonochrome = true\n").unwrap();
        assert_eq!(old.ui.tooltip, UiNumbers::default());
        assert_eq!(old.ui.panel, UiNumbers::PERCENT_ONLY);
    }

    /// The taskbar panel's own choice: untouched, the percentage alone (the
    /// panel of 0.7.0) and nothing written; the reset time added, or alone.
    #[test]
    fn the_panel_shows_the_percent_the_reset_time_or_both() {
        use crate::config::schema::{TipPart, UiNumbers};
        let set = |part, on| PickerAction::SetTip {
            part,
            place: NumbersOn::Panel,
            on,
        };
        let mut config = Config::default();
        assert_eq!(config.ui.panel, UiNumbers::PERCENT_ONLY);
        assert!(!toml::to_string(&config).unwrap().contains("[ui.panel]"));
        assert!(!apply(&mut config, &set(TipPart::Percent, false)).changed);
        assert!(apply(&mut config, &set(TipPart::Reset, true)).changed);
        assert_eq!(config.ui.panel, UiNumbers::default());
        // The tooltip's choice is its own.
        assert_eq!(config.ui.tooltip, UiNumbers::default());
        assert!(apply(&mut config, &set(TipPart::Percent, false)).changed);
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("[ui.panel]"), "{text}");
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.ui.panel, config.ui.panel);
        assert!(!back.ui.panel.percent && back.ui.panel.reset);
    }

    /// Allow all's other tools: allowed, not switched on, no row; the first
    /// tick of one switches it on with nothing more to allow.
    #[test]
    fn allow_all_allows_without_ticking() {
        let mut config = Config::fresh_install();
        let grok = |c: &Config| c.providers.iter().find(|p| p.id == "grok").unwrap().clone();
        apply(&mut config, &PickerAction::Consent("grok".into()));
        assert!(config.sign_in_allowed("grok"));
        assert!(!grok(&config).enabled);
        assert!(!config.provider_active(&grok(&config)));
        apply(
            &mut config,
            &PickerAction::SetRow {
                surface: Surface::Widget,
                row: "grok".into(),
                on: true,
            },
        );
        assert!(config.provider_active(&grok(&config)));
    }

    /// The panel's progress bar: on by default and not written; it can go
    /// whatever numbers are shown, and the last number still stays.
    #[test]
    fn the_panels_bar_can_be_switched_off() {
        use crate::config::schema::{TipPart, UiNumbers};
        let set = |part, on| PickerAction::SetTip {
            part,
            place: NumbersOn::Panel,
            on,
        };
        let mut config = Config::default();
        assert!(config.ui.panel.bar);
        assert!(apply(&mut config, &set(TipPart::Bar, false)).changed);
        assert!(!config.ui.panel.bar && config.ui.panel.percent);
        // The percentage alone without its bar: still the last number.
        assert!(!apply(&mut config, &set(TipPart::Percent, false)).changed);
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("bar = false"), "{text}");
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.ui.panel, config.ui.panel);
        // A hand-written table: a key left out keeps the panel's default.
        let only_bar: Config = toml::from_str(
            "[ui.panel]
bar = false
",
        )
        .unwrap();
        assert_eq!(
            only_bar.ui.panel,
            UiNumbers {
                bar: false,
                ..UiNumbers::PERCENT_ONLY
            }
        );
        let neither: Config = toml::from_str(
            "[ui.panel]
percent = false
reset = false
",
        )
        .unwrap();
        assert_eq!(neither.ui.panel, UiNumbers::PERCENT_ONLY);
        // An older [ui.panel] without the key keeps its bar.
        let old: UiNumbers = toml::from_str("percent = true\nreset = true").unwrap();
        assert!(old.bar);
        // Back on: the default again, and nothing written.
        assert!(apply(&mut config, &set(TipPart::Bar, true)).changed);
        assert!(!toml::to_string(&config).unwrap().contains("[ui.panel]"));
    }

    /// The sign-in under each provider: the source the config names, what
    /// it reads (the consent text) and where its token goes; a pasted
    /// source says whether anything is stored and offers its actions.
    #[test]
    fn each_provider_says_where_its_sign_in_comes_from() {
        use crate::config::schema::AuthMethod;
        let mut config = Config::default();
        config.allow_sign_in("claude");
        config.allow_sign_in("codex");
        config.allow_app_sign_in("copilot");
        if let Some(pc) = config.providers.iter_mut().find(|p| p.id == "copilot") {
            pc.auth_method = AuthMethod::ApiKey;
        }
        let ok = |id| ProviderData {
            id,
            status: ProviderStatus::Ok,
            metrics: Vec::new(),
            updated_at: crate::utc::now(),
            received_at: None,
        };
        let display = [ok(ProviderId::Claude), ok(ProviderId::Copilot)];
        let mut state = SignInState {
            stored: vec!["copilot_pat"],
            ..SignInState::default()
        };
        let all = sign_in_choices(&config, &display, &state);
        let of = |id: &str| all.iter().find(|s| s.provider == id).unwrap().clone();
        let claude = of("claude");
        assert_eq!(claude.summary, "Claude Code \u{b7} works");
        assert_eq!(claude.sources.len(), 3);
        assert!(claude.sources[0].on && claude.sources[0].id == SourceId::Tool);
        assert!(
            claude.detail.contains(".credentials.json"),
            "{}",
            claude.detail
        );
        assert!(claude.detail.contains("api.anthropic.com"));
        assert_eq!(claude.paste, None);
        let codex = of("codex");
        assert_eq!(codex.summary, "Codex CLI \u{b7} checking");
        let copilot = of("copilot");
        assert_eq!(copilot.summary, "Your token \u{b7} works");
        assert!(copilot.stored);
        assert!(copilot.detail.contains("copilot_pat"));
        assert_eq!(copilot.paste, Some("Paste token from clipboard"));
        assert_eq!(of("grok").summary, "Not allowed yet");
        assert!(of("grok").sources.is_empty());
        assert_eq!(of("cursor").summary, "Not signed in");
        assert_eq!(of("cursor").cursor, Some(CursorState::SignedOut));
        // Nothing stored yet for the source chosen.
        state.stored.clear();
        state.notes.insert("copilot", "Removed.".into());
        let copilot = sign_in_choices(&config, &display, &state)
            .into_iter()
            .find(|s| s.provider == "copilot")
            .unwrap();
        assert_eq!(copilot.summary, "Your token \u{b7} none stored");
        assert!(!copilot.stored);
        assert_eq!(copilot.note.as_deref(), Some("Removed."));
        // Sources and methods map both ways.
        for s in [SourceId::Tool, SourceId::Key, SourceId::UsageToken] {
            assert_eq!(source_of(&method_of(s)), s);
        }
    }

    #[test]
    fn a_fresh_install_shows_nothing_ticked() {
        let config = Config::fresh_install();
        assert!(resolved_rows(&config, Surface::Widget).is_empty());
        assert!(resolved_rows(&config, Surface::Tooltip).is_empty());
    }

    #[test]
    fn the_first_tick_spells_out_only_what_runs() {
        let mut config = Config::fresh_install();
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
            key: "gpt-6.1-sol".into(),
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

    /// Unticking the last model part stops the reads of the tools' files
    /// for the label: the next tick asks again. Going back to "As set in
    /// the tool" stops the session reads alone.
    #[test]
    fn the_model_reads_can_be_taken_back() {
        let mut config = Config::default();
        apply(&mut config, &PickerAction::Allow(ReadKind::ToolSettings));
        apply(&mut config, &PickerAction::Allow(ReadKind::LastSession));
        let part = |surface, part, on| PickerAction::SetPart { surface, part, on };
        apply(&mut config, &part(Surface::Widget, ModelPart::Model, true));
        apply(
            &mut config,
            &part(Surface::Tooltip, ModelPart::Effort, true),
        );
        apply(&mut config, &PickerAction::SetSource(ModelSource::LastUsed));
        // Back to the settings: only the sessions stop.
        apply(
            &mut config,
            &PickerAction::SetSource(ModelSource::Configured),
        );
        assert!(config.reads.tool_settings && !config.reads.last_session);
        apply(&mut config, &PickerAction::Allow(ReadKind::LastSession));
        // One part still on: everything stays.
        apply(&mut config, &part(Surface::Widget, ModelPart::Model, false));
        assert!(config.reads.tool_settings && config.reads.last_session);
        // The last one off: both stop.
        let done = apply(
            &mut config,
            &part(Surface::Tooltip, ModelPart::Effort, false),
        );
        assert!(done.models);
        assert!(!config.reads.tool_settings && !config.reads.last_session);
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

    fn uses(models: &mut Models, p: ProviderId, id: &str) {
        models.insert(
            p,
            ModelInfo {
                id: id.into(),
                effort: None,
            },
        );
    }

    /// A model two tools share is one name: its field is listed under each
    /// tool, with the same key, marked in use for the tool that uses it and
    /// naming the other.
    #[test]
    fn a_shared_model_is_one_name_under_each_tool() {
        let mut config = Config::default();
        config.allow_sign_in("codex");
        config.allow_sign_in("copilot");
        let mut models = Models::new();
        uses(&mut models, ProviderId::Codex, "gpt-5");
        uses(&mut models, ProviderId::Copilot, "gpt-5.6-sol");
        let input = input(&config, &models, &[]);
        let find = |provider: &str, id: &str| {
            input
                .models
                .iter()
                .find(|m| m.provider == provider && m.model_id == id)
                .unwrap_or_else(|| panic!("{provider} {id}"))
                .clone()
        };
        // Each tool's list starts with the model it uses now.
        assert_eq!(input.models[0].model_id, "gpt-5");
        assert!(input.models[0].current);
        // gpt-5.6-sol: in both lists, one key, in use under Copilot only.
        let (codex, copilot) = (find("codex", "gpt-5.6-sol"), find("copilot", "gpt-5.6-sol"));
        assert_eq!(codex.key, copilot.key);
        assert!(!codex.current && copilot.current);
        assert_eq!(codex.shared_with, ["Copilot"]);
        assert_eq!(copilot.shared_with, ["Codex"]);
        // gpt-5 only Codex uses: listed once, shared with nobody.
        assert_eq!(
            input
                .models
                .iter()
                .filter(|m| m.model_id == "gpt-5")
                .count(),
            1
        );
        assert!(find("codex", "gpt-5").shared_with.is_empty());
    }

    /// "auto" is each tool's own router: Copilot's and Cursor's are named
    /// apart, under "copilot:auto" and "cursor:auto". A name an earlier
    /// version kept under the bare id still shows, for both, and is split
    /// between them at the first rename.
    #[test]
    fn a_router_is_named_per_tool() {
        let mut config = Config::default();
        config.allow_sign_in("copilot");
        config.allow_sign_in("cursor");
        let input_now = |config: &Config| input(config, &Models::new(), &[]);
        let auto = |input: &PickerInput, p: &str| {
            input
                .models
                .iter()
                .find(|m| m.provider == p && m.model_id == "auto")
                .unwrap()
                .clone()
        };
        let i = input_now(&config);
        assert_eq!(auto(&i, "copilot").key, "copilot:auto");
        assert_eq!(auto(&i, "cursor").key, "cursor:auto");
        assert!(auto(&i, "copilot").shared_with.is_empty());
        // An earlier version's shared name.
        config.ui.model.names.insert("auto".into(), "Router".into());
        let i = input_now(&config);
        assert_eq!(auto(&i, "copilot").renamed.as_deref(), Some("Router"));
        assert_eq!(auto(&i, "cursor").renamed.as_deref(), Some("Router"));
        apply(
            &mut config,
            &PickerAction::Rename {
                key: "copilot:auto".into(),
                name: Some("Cop".into()),
            },
        );
        let names = &config.ui.model.names;
        assert!(!names.contains_key("auto"));
        assert_eq!(names["copilot:auto"], "Cop");
        assert_eq!(names["cursor:auto"], "Router");
        assert_eq!(
            model_name(names, &ProviderId::Cursor, "auto"),
            Some("Router")
        );
        assert_eq!(model_name(names, &ProviderId::Copilot, "AUTO"), None);
        // An ordinary model keeps one name for every tool.
        assert_eq!(model_key(&ProviderId::Copilot, "gpt-5.5"), "gpt-5.5");
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
        let input = input(&config, &models, &["codex"]);
        let ids: Vec<&str> = input.rows.iter().map(|r| r.id).collect();
        assert_eq!(&ids[..3], ["claude", "claude.opus", "claude.sonnet"]);
        assert_eq!(ids.len(), 6 + rows::SUB_ROWS.len());
        assert_eq!(input.widget_rows, ["codex"]);
        // Every model Codex is known to offer, the one in use first and
        // marked.
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
        assert!(input.found_weak.is_empty());
    }

    /// A tool found only by a related tool's folder is passed on as found
    /// weakly: the welcome marks it but does not tick it.
    #[test]
    fn weak_finds_are_passed_on() {
        let config = Config::default();
        let input = picker_input_with(
            &config,
            &[],
            &Models::new(),
            PickerMode::Welcome,
            &["claude", "copilot", "antigravity"],
            |id| id == "claude",
        );
        assert_eq!(input.found, ["claude", "copilot", "antigravity"]);
        assert_eq!(input.found_weak, ["copilot", "antigravity"]);
    }

    fn data(id: ProviderId, status: ProviderStatus, metrics: &[&str]) -> ProviderData {
        ProviderData {
            id,
            status,
            metrics: metrics
                .iter()
                .map(|label| crate::providers::Metric {
                    label: label.to_string(),
                    used: 1,
                    limit: Some(100),
                    unit: crate::providers::MetricUnit::Percent,
                    reset_at: None,
                    window: crate::providers::MetricWindow::Long,
                    unified_pool: false,
                    extra_only: false,
                })
                .collect(),
            updated_at: crate::utc::now(),
            received_at: None,
        }
    }

    /// The notes under a row: an extra row is said to be missing only once
    /// its provider answered (not while it loads); a running provider with
    /// no sign-in says what is missing, Cursor where to sign in.
    #[test]
    fn notes_wait_for_an_answer_and_say_what_is_missing() {
        let mut config = Config::default();
        for p in ["claude", "cursor", "codex"] {
            config.allow_sign_in(p);
        }
        let display = [
            data(ProviderId::Claude, ProviderStatus::Loading, &[]),
            data(ProviderId::Cursor, ProviderStatus::NotConfigured, &[]),
            data(ProviderId::Codex, ProviderStatus::NotConfigured, &[]),
            data(ProviderId::Grok, ProviderStatus::Ok, &["Weekly"]),
        ];
        let input = picker_input_with(
            &config,
            &display,
            &Models::new(),
            PickerMode::Normal,
            &[],
            |_| true,
        );
        let row = |id: &str| input.rows.iter().find(|r| r.id == id).unwrap().clone();
        assert!(!row("claude.opus").provider_ok);
        // Grok answered without its Build share: not offered...
        assert!(!input.rows.iter().any(|r| r.id == "grok.build"));
        // ...unless chosen earlier: then it is missing, and says so.
        let mut chosen = config.clone();
        chosen.ui.rows.widget = Some(vec!["grok".into(), "grok.build".into()]);
        let with_build = picker_input_with(
            &chosen,
            &display,
            &Models::new(),
            PickerMode::Normal,
            &[],
            |_| true,
        );
        let build = with_build
            .rows
            .iter()
            .find(|r| r.id == "grok.build")
            .unwrap();
        assert!(build.provider_ok && !build.has_data);
        assert_eq!(row("cursor").note, "Not signed in: open its Sign-in below");
        assert_eq!(row("codex").note, "No sign-in found on this PC");
        // Grok Bot is asked for only once its row is ticked: until then it
        // says so, and is not called missing.
        let bot = row("cursor.bot");
        assert_eq!(bot.note, ON_REQUEST_NOTE);
        assert!(!bot.provider_ok);
        assert_eq!(row("grok").note, "Today's row");
        // Not running: nothing to say yet.
        assert_eq!(row("copilot").note, "No data yet");
    }

    /// The Grok Bot row, with Cursor answering: not asked for while it is
    /// not ticked, so never "not reported"; once ticked, an ordinary extra
    /// row that is missing only when the answer lacks it.
    #[test]
    fn an_on_request_row_says_when_it_is_asked_for() {
        let mut config = Config::default();
        config.allow_app_sign_in("cursor");
        let display = [data(ProviderId::Cursor, ProviderStatus::Ok, &["Usage"])];
        let bot = |config: &Config| {
            picker_input_with(
                config,
                &display,
                &Models::new(),
                PickerMode::Normal,
                &[],
                |_| true,
            )
            .rows
            .into_iter()
            .find(|r| r.id == "cursor.bot")
            .unwrap()
        };
        let unticked = bot(&config);
        assert_eq!(unticked.note, ON_REQUEST_NOTE);
        assert!(!unticked.provider_ok && !unticked.has_data);
        config.ui.rows.tooltip = Some(vec!["cursor".into(), "cursor.bot".into()]);
        let ticked = bot(&config);
        assert_eq!(ticked.note, "Its share of the same limit");
        assert!(ticked.provider_ok && !ticked.has_data);
    }
}
