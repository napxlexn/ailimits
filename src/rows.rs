// rows.rs — which rows the widget and the tooltip show, and the model label
// each row carries. See docs/en/CONFIG.md, `[ui.rows]` and `[ui.model]`.
//
// One row per provider by default: exactly the rows there were before rows
// could be chosen. Extra rows exist only where a provider meters a part on
// its own (Claude's Opus, Sonnet and Fable weeks and the week of any model
// Claude adds later, Antigravity's second pool, Grok Build, Cursor's Grok
// Bot). The taskbar panel and the tray icon keep one entry per provider and
// do not use this.

use crate::config::schema::{Config, ModelPart, Surface};
use crate::models::{self, ModelInfo, Models};
use crate::providers::{Metric, ProviderData, ProviderId, ProviderStatus};
use std::collections::BTreeMap;

/// An extra row: one metric of a provider, shown as a row of its own.
#[derive(Debug, Clone)]
pub struct SubRow {
    /// Row id in config.toml: provider.part.
    pub id: &'static str,
    pub provider: ProviderId,
    /// The label of the metric this row shows, exactly as the provider
    /// reports it (src/providers).
    pub metric: &'static str,
    /// The row's name, longest first; a narrow place takes a shorter one.
    pub names: &'static [&'static str],
    /// What the row is, for the picker.
    pub note: &'static str,
    /// What the row counts, shown when the pointer rests on it.
    pub hint: &'static str,
    /// Its numbers come from a request of their own, made only while the
    /// row is chosen on a surface (`row_wanted`): until then the provider
    /// has not been asked, so the row is not missing from its answer.
    pub on_request: bool,
}

/// Every extra row, in display order within its provider.
pub const SUB_ROWS: &[SubRow] = &[
    SubRow {
        id: "claude.opus",
        provider: ProviderId::Claude,
        metric: "Opus",
        names: &["Claude · Opus", "Cl · Opus", "Opus"],
        note: "Weekly, a limit of its own",
        hint: "Claude's separate weekly limit for Opus models, on plans that have one. It fills apart from the weekly limit.",
        on_request: false,
    },
    SubRow {
        id: "claude.sonnet",
        provider: ProviderId::Claude,
        metric: "Sonnet",
        names: &["Claude · Sonnet", "Cl · Sonnet", "Sonnet"],
        note: "Weekly, a limit of its own",
        hint: "Claude's separate weekly limit for Sonnet models, on plans that have one. It fills apart from the weekly limit.",
        on_request: false,
    },
    SubRow {
        id: "claude.fable",
        provider: ProviderId::Claude,
        metric: "Fable",
        names: &["Claude · Fable", "Cl · Fable", "Fable"],
        note: "Weekly, a limit of its own",
        hint: "Fable's own weekly allowance, a share of the weekly limit: Current week (Fable) in Claude Code's /usage.",
        on_request: false,
    },
    SubRow {
        id: "antigravity.cg",
        provider: ProviderId::Antigravity,
        metric: "Claude/GPT",
        names: &[
            "Antigravity · Claude and GPT",
            "Antigravity · Claude+GPT",
            "AG · Claude+GPT",
            "Claude+GPT",
            "C+GPT",
        ],
        note: "A weekly pool of its own",
        hint: "Antigravity's weekly pool for its Claude and GPT models, counted apart from the Gemini models.",
        on_request: false,
    },
    SubRow {
        id: "grok.build",
        provider: ProviderId::Grok,
        metric: "Build",
        names: &["Grok · Build", "Build"],
        note: "Its share of the weekly limit",
        hint: "How much of the shared weekly limit Grok Build has used.",
        on_request: false,
    },
    SubRow {
        id: "cursor.bot",
        provider: ProviderId::Cursor,
        metric: "Grok Bot",
        names: &["Cursor · Grok Bot", "Grok Bot", "Bot"],
        note: "Its share of the same limit",
        hint: "How much of the limit Cursor and Grok share Grok Bot has used.",
        on_request: true,
    },
];

/// The extra rows of the catalogue a provider can offer.
pub fn sub_rows(provider: &ProviderId) -> impl Iterator<Item = &'static SubRow> + '_ {
    SUB_ROWS.iter().filter(move |s| s.provider == *provider)
}

/// The extra rows a provider offers with `data`: the catalogue's, then, for
/// Claude, one for each week of a single model its reply reports that the
/// catalogue does not name (`model_row`), in the reply's order. A model
/// Claude adds later so gets a row of its own without an update.
pub fn offered_rows(provider: &ProviderId, data: Option<&ProviderData>) -> Vec<&'static SubRow> {
    let mut out: Vec<&'static SubRow> = sub_rows(provider).collect();
    let Some(data) = data.filter(|_| *provider == ProviderId::Claude) else {
        return out;
    };
    for m in data.metrics.iter().filter(|m| m.extra_only) {
        if out.iter().any(|s| s.metric.eq_ignore_ascii_case(&m.label)) {
            continue;
        }
        if let Some(row) = model_row(&m.label) {
            if !out.iter().any(|s| s.id == row.id) {
                out.push(row);
            }
        }
    }
    out
}

/// The rows `model_row` made, kept for the life of the process so a row's
/// id and names are borrowed like the catalogue's.
static MADE: std::sync::Mutex<Vec<&'static SubRow>> = std::sync::Mutex::new(Vec::new());
/// The most rows `model_row` makes in one run: a reply naming more models
/// than that gets rows for the first ones.
const MADE_MAX: usize = 16;
/// The longest model name a made row shows; a longer one ends in "…".
const MADE_NAME_MAX: usize = 20;
/// The shortest form of a made row's name: the model's first word, at most
/// this many characters ("Sonnet", the catalogue's widest shortest form).
const MADE_SHORT_MAX: usize = 6;
/// The longest model name a made row's line under the bar shows; a longer
/// one shows its shortest form there ("Claude/GPT", the catalogue's longest
/// label, is 10).
const MADE_LABEL_MAX: usize = 10;

/// Whether `sub` is a row `model_row` made rather than the catalogue's.
fn is_made(sub: &SubRow) -> bool {
    !SUB_ROWS.iter().any(|s| s.id == sub.id)
}

/// The row of Claude's week of the model `label` names (its `display_name`,
/// `providers::claude::model_weeks`), made once per model: id
/// "claude.<name in lowercase, words joined by '-'>", names as the
/// catalogue's ("Claude · Haiku", "Cl · Haiku", "Haiku"). None for a name
/// with no letter or digit, or past `MADE_MAX`.
fn model_row(label: &str) -> Option<&'static SubRow> {
    let name = label.trim();
    let mut slug = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        return None;
    }
    let id = format!("claude.{slug}");
    let mut made = MADE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(row) = made.iter().find(|r| r.id == id) {
        return Some(*row);
    }
    if made.len() >= MADE_MAX {
        return None;
    }
    fn keep(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }
    let cut = |s: &str, max: usize, end: &str| {
        if s.chars().count() <= max {
            s.to_string()
        } else {
            let head: String = s.chars().take(max - end.chars().count()).collect();
            format!("{}{end}", head.trim_end())
        }
    };
    let shown = cut(name, MADE_NAME_MAX, "…");
    let mut names = vec![
        keep(format!("Claude · {shown}")),
        keep(format!("Cl · {shown}")),
        keep(shown.clone()),
    ];
    let first_word = name.split_whitespace().next().unwrap_or(name);
    let short = cut(first_word, MADE_SHORT_MAX, "");
    if short != shown {
        names.push(keep(short));
    }
    let row: &'static SubRow = Box::leak(Box::new(SubRow {
        id: keep(id),
        provider: ProviderId::Claude,
        metric: keep(name.to_string()),
        names: Box::leak(names.into_boxed_slice()),
        note: "Weekly, a limit of its own",
        hint: keep(format!(
            "{shown}'s own weekly allowance, a share of the weekly limit: Current week ({shown}) in Claude Code's /usage."
        )),
        on_request: false,
    }));
    made.push(row);
    Some(row)
}

/// Whether any row of `provider` is chosen for a surface: its own, one of
/// the catalogue's, or one `offered_rows` makes from a reply (known by its
/// id, "claude.haiku", before any reply names it).
pub fn provider_row_chosen(config: &Config, surface: Surface, provider: &str) -> bool {
    let chosen = match surface {
        Surface::Widget => &config.ui.rows.widget,
        Surface::Tooltip => &config.ui.rows.tooltip,
    };
    match chosen {
        Some(list) => list.iter().any(|r| r.split('.').next() == Some(provider)),
        None => true,
    }
}

/// A provider's own row name, as a one-step list.
fn base_names(provider: &ProviderId) -> &'static [&'static str] {
    match provider {
        ProviderId::Claude => &["Claude"],
        ProviderId::Codex => &["Codex"],
        ProviderId::Copilot => &["Copilot"],
        ProviderId::Antigravity => &["Antigravity"],
        ProviderId::Grok => &["Grok"],
        ProviderId::Cursor => &["Cursor"],
    }
}

/// A model label in parts, so a narrow place can drop them one at a time:
/// effort first, then the version, then the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelLabel {
    /// "Opus", when the model part is on.
    pub model: Option<String>,
    /// "Opus 5.5", when the version part is on too and the id has a version.
    pub with_version: Option<String>,
    /// "xhigh", when the effort part is on and the tool reports one.
    pub effort: Option<String>,
}

impl ModelLabel {
    /// Every form of the label, longest first, each one part shorter:
    /// "Opus 5.5 · xhigh", "Opus 5.5", "Opus". Effort alone: "xhigh".
    pub fn steps(&self) -> Vec<String> {
        let head = self.with_version.as_ref().or(self.model.as_ref());
        let mut out: Vec<String> = Vec::new();
        if let Some(effort) = &self.effort {
            out.push(match head {
                Some(h) => format!("{h} · {effort}"),
                None => effort.clone(),
            });
        }
        for part in [&self.with_version, &self.model].into_iter().flatten() {
            if out.last() != Some(part) {
                out.push(part.clone());
            }
        }
        out
    }

    /// The whole label.
    pub fn full(&self) -> Option<String> {
        self.steps().into_iter().next()
    }
}

/// The label for `info` with the parts on for a surface, or None when none
/// of them has anything to show. Version needs the model; effort can go alone.
pub fn model_label(
    info: &ModelInfo,
    provider: &ProviderId,
    parts: &[ModelPart],
    names: &BTreeMap<String, String>,
) -> Option<ModelLabel> {
    let on = |p: ModelPart| parts.contains(&p);
    // An empty id: the tool stores an effort but no model; no model to name.
    // The user's name is looked up as the picker keeps it: under the id, or
    // under "provider:id" for a tool's own router ("copilot:auto").
    let n = (!info.id.is_empty()).then(|| {
        let rename = crate::choices::model_name(names, provider, &info.id);
        models::name_parts(&info.id, provider, rename)
    });
    let model = n
        .as_ref()
        .filter(|_| on(ModelPart::Model))
        .map(|n| n.model.clone());
    let with_version = n
        .as_ref()
        .filter(|n| on(ModelPart::Model) && on(ModelPart::Version) && n.with_version != n.model)
        .map(|n| n.with_version.clone());
    let effort = on(ModelPart::Effort)
        .then(|| info.effort.clone())
        .flatten()
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty());
    (model.is_some() || effort.is_some()).then_some(ModelLabel {
        model,
        with_version,
        effort,
    })
}

/// One row of the widget or the tooltip.
#[derive(Debug, Clone)]
pub struct RowView {
    /// Row id: "claude", or "claude.opus" for an extra row.
    pub id: &'static str,
    pub provider: ProviderId,
    /// The row's name, longest first. A provider's own row has one form.
    pub names: &'static [&'static str],
    /// The data the row draws. A provider's own row: the provider's data,
    /// untouched. An extra row: the same data narrowed to its one metric.
    pub data: ProviderData,
    /// The model label, on a provider's own row only (an extra row's name
    /// already says the model).
    pub model: Option<ModelLabel>,
}

impl RowView {
    /// The longest form of the row's name.
    pub fn name(&self) -> &'static str {
        self.names[0]
    }

    /// Whether this is an extra row rather than a provider's own.
    pub fn is_extra(&self) -> bool {
        self.id.contains('.')
    }
}

/// Whether a row is chosen on the widget or the tooltip.
pub fn row_wanted(config: &Config, id: &str) -> bool {
    row_chosen(config, Surface::Widget, id) || row_chosen(config, Surface::Tooltip, id)
}

/// Whether a row id is chosen for a surface. With nothing chosen yet, a
/// provider's own row is and an extra row is not.
pub fn row_chosen(config: &Config, surface: Surface, id: &str) -> bool {
    let chosen = match surface {
        Surface::Widget => &config.ui.rows.widget,
        Surface::Tooltip => &config.ui.rows.tooltip,
    };
    match chosen {
        Some(list) => list.iter().any(|r| r == id),
        None => !id.contains('.'),
    }
}

/// The rows a surface shows, in order: each provider's own row, then its
/// extra rows, providers in the order of `visible` (the providers the widget
/// shows today, see app.rs `visible_data`).
///
/// An extra row needs its metric in the provider's data: a plan without an
/// Opus week has no Opus row, and while a provider has no data yet its extra
/// rows wait for it rather than show an empty bar.
///
/// Each row is aged for display on its own (`ProviderData::aged_for_display`):
/// a provider's own row by its own windows, an extra row by its one metric,
/// so a reset in one never makes another an estimate. `visible` is best the
/// data as the providers sent it; data already aged gives the same own rows
/// and nearly the same extra rows (see `extra_row_data`).
///
/// With the default config this is exactly one row per entry of `visible`,
/// carrying that entry aged for display, without the metrics only an extra
/// row shows.
pub fn visible_rows(
    config: &Config,
    visible: &[ProviderData],
    surface: Surface,
    models: &Models,
) -> Vec<RowView> {
    let parts = config.ui.model.parts(surface);
    let mut rows = Vec::with_capacity(visible.len());
    for data in visible {
        let provider = data.id.clone();
        if row_chosen(config, surface, provider.config_id()) {
            let model = if parts.is_empty() {
                None
            } else {
                models
                    .get(&provider)
                    .and_then(|info| model_label(info, &provider, parts, &config.ui.model.names))
            };
            rows.push(RowView {
                id: provider.config_id(),
                provider: provider.clone(),
                names: base_names(&provider),
                data: own_row_data(&data.aged_for_display()),
                model,
            });
        }
        for sub in offered_rows(&provider, Some(data)) {
            if !row_chosen(config, surface, sub.id) {
                continue;
            }
            let Some(metric) = data
                .metrics
                .iter()
                .find(|m| m.label.eq_ignore_ascii_case(sub.metric))
            else {
                continue;
            };
            let mut row_data = extra_row_data(data, metric);
            // A made row's long model name is too wide for the line under
            // its bar ("Mythos Preview Extended: 9%"): its shortest form
            // goes there instead.
            if is_made(sub) && metric.label.chars().count() > MADE_LABEL_MAX {
                if let (Some(m), Some(short)) = (row_data.metrics.first_mut(), sub.names.last()) {
                    m.label = short.to_string();
                }
            }
            rows.push(RowView {
                id: sub.id,
                provider: provider.clone(),
                names: sub.names,
                data: row_data,
                model: None,
            });
        }
    }
    rows
}

/// A provider's data as its own row draws it: without the metrics only an
/// extra row shows (`Metric::extra_only`), so its own row looks the same
/// whether the provider reports them or not. Its status is the provider's,
/// which only its own windows decide (`aged_for_display`).
fn own_row_data(data: &ProviderData) -> ProviderData {
    if !data.metrics.iter().any(|m| m.extra_only) {
        return data.clone();
    }
    ProviderData {
        metrics: data
            .metrics
            .iter()
            .filter(|m| !m.extra_only)
            .cloned()
            .collect(),
        ..data.clone()
    }
}

/// An extra row's data: the provider's data narrowed to `metric`, the row's
/// headline, aged by that metric alone. Its own reset passed: an estimate,
/// "≈0%"; another window's reset leaves it as it is.
///
/// From data already aged the provider-wide status is set aside — it may
/// come from another window. A metric only an extra row shows is never
/// aged there, so its row still ages exactly. One of the provider's own
/// windows was zeroed there if its reset passed: read back as an estimate
/// when it is 0 with no reset left in an estimate, the one guess this makes.
fn extra_row_data(data: &ProviderData, metric: &Metric) -> ProviderData {
    let estimated = matches!(data.status, ProviderStatus::Estimated);
    let rolled_over_upstream =
        estimated && !metric.extra_only && metric.used == 0 && metric.reset_at.is_none();
    let one = ProviderData {
        status: if estimated {
            ProviderStatus::Ok
        } else {
            data.status.clone()
        },
        // On its own row the metric is that row's headline.
        metrics: vec![Metric {
            extra_only: false,
            ..metric.clone()
        }],
        ..data.clone()
    };
    let mut aged = one.aged_for_display();
    if rolled_over_upstream && matches!(aged.status, ProviderStatus::Ok) {
        aged.status = ProviderStatus::Estimated;
    }
    aged
}

/// The most providers the taskbar panel draws, and so the most the
/// picker's Taskbar column lets the user tick.
pub const TASKBAR_MAX: usize = 2;

/// The providers ticked in the picker's Taskbar column, by config id, in
/// the widget's order: those chosen (`ui.rows.taskbar`) that run, at most
/// `TASKBAR_MAX`. Untouched, or when none of those chosen runs, the first
/// two that run: what the panel draws by itself.
pub fn taskbar_ticked(config: &Config) -> Vec<&'static str> {
    let running: Vec<&'static str> = config
        .providers
        .iter()
        .filter(|p| config.provider_active(p))
        .filter_map(|p| ProviderId::from_config_id(&p.id).map(|id| id.config_id()))
        .collect();
    let first = || running.iter().copied().take(TASKBAR_MAX).collect();
    match &config.ui.rows.taskbar {
        None => first(),
        Some(list) => {
            let chosen: Vec<&'static str> = running
                .iter()
                .copied()
                .filter(|id| list.iter().any(|p| p == id))
                .take(TASKBAR_MAX)
                .collect();
            if chosen.is_empty() {
                first()
            } else {
                chosen
            }
        }
    }
}

/// Whether the user chose the providers of the taskbar panel and the tray
/// icon, and `provider` is one of them: it is still in use there, so its
/// sign-in stays allowed though no widget or tooltip row of it is ticked.
pub fn on_taskbar(config: &Config, provider: &str) -> bool {
    config
        .ui
        .rows
        .taskbar
        .as_ref()
        .is_some_and(|list| list.iter().any(|p| p == provider))
}

/// Whether no provider may run yet: a fresh install before the first tick,
/// or everything switched off. The widget and the tray then say "Choose what
/// to show" instead of looking empty.
pub fn nothing_chosen(config: &Config) -> bool {
    !config.providers.iter().any(|p| config.provider_active(p))
}

/// The narrowest places the widget draws a model name, as (width, font
/// size) in the renderer's pixels, one per size it draws a model at. A
/// renamed model must fit every one, so it shows in full in every size. The
/// renderer owns these places: any change to where a model name is drawn
/// updates this list (`renderer::tests::every_model_name_place_holds_its_slot`
/// holds them to the geometry). Medium at half width beside the longest
/// reset (10.5 px), the Compact, Medium and Expanded columns (9, 10, 11 px).
pub const NAME_SLOTS: &[(f32, f32)] = &[(56.5, 10.5), (51.0, 9.0), (59.9, 10.0), (68.8, 11.0)];

/// Whether a model name fits every place a model name is drawn.
/// `measure(text, size)` is `Renderer::text_width`.
pub fn name_fits(name: &str, measure: impl Fn(&str, f32) -> f32) -> bool {
    let name = name.trim();
    NAME_SLOTS
        .iter()
        .all(|&(width, size)| measure(name, size) <= width)
}

/// The longest start of `name` that fits, cut at a character and with no
/// trailing space: what a paste keeps.
pub fn fit_name(name: &str, measure: impl Fn(&str, f32) -> f32) -> String {
    let name = name.trim();
    let mut end = 0;
    for (i, c) in name.char_indices() {
        let next = i + c.len_utf8();
        if !name_fits(&name[..next], &measure) {
            break;
        }
        end = next;
    }
    name[..end].trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Metric, MetricUnit, MetricWindow, ProviderStatus};
    use crate::utc::{self, SignedDuration};

    fn metric(label: &str, used: u64, window: MetricWindow) -> Metric {
        Metric {
            label: label.to_string(),
            used,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: Some(utc::now() + SignedDuration::from_hours(3)),
            window,
            unified_pool: false,
            extra_only: false,
        }
    }

    fn data(id: ProviderId, metrics: Vec<Metric>) -> ProviderData {
        ProviderData {
            id,
            status: ProviderStatus::Ok,
            metrics,
            updated_at: utc::now(),
            received_at: None,
        }
    }

    fn sample() -> Vec<ProviderData> {
        vec![
            data(
                ProviderId::Claude,
                vec![
                    metric("Session", 25, MetricWindow::Session),
                    metric("Weekly", 34, MetricWindow::Long),
                    metric("Opus", 61, MetricWindow::Long),
                    metric("Sonnet", 12, MetricWindow::Long),
                ],
            ),
            data(
                ProviderId::Codex,
                vec![metric("Session", 48, MetricWindow::Session)],
            ),
            data(
                ProviderId::Antigravity,
                vec![
                    metric("Gemini", 30, MetricWindow::Long),
                    metric("Claude/GPT", 64, MetricWindow::Long),
                ],
            ),
        ]
    }

    fn ids(rows: &[RowView]) -> Vec<&str> {
        rows.iter().map(|r| r.id).collect()
    }

    /// The guarantee the visual baseline rests on: the default config gives
    /// one row per provider, in order, with its data untouched and no label.
    #[test]
    fn default_config_is_one_untouched_row_per_provider() {
        let config = Config::default();
        let visible = sample();
        for surface in [Surface::Widget, Surface::Tooltip] {
            let rows = visible_rows(&config, &visible, surface, &Models::new());
            assert_eq!(ids(&rows), ["claude", "codex", "antigravity"]);
            for (row, d) in rows.iter().zip(&visible) {
                assert_eq!(row.data.id, d.id);
                assert_eq!(row.data.metrics.len(), d.metrics.len());
                assert_eq!(row.name(), d.id.display_name());
                assert!(row.model.is_none());
                assert!(!row.is_extra());
            }
        }
    }

    #[test]
    fn extra_rows_follow_their_provider_and_carry_one_metric() {
        let mut config = Config::default();
        config.ui.rows.widget = Some(
            ["codex", "claude.sonnet", "antigravity.cg", "claude.opus"]
                .map(String::from)
                .to_vec(),
        );
        let rows = visible_rows(&config, &sample(), Surface::Widget, &Models::new());
        assert_eq!(
            ids(&rows),
            ["claude.opus", "claude.sonnet", "codex", "antigravity.cg"]
        );
        assert_eq!(rows[0].data.metrics.len(), 1);
        assert_eq!(rows[0].data.metrics[0].label, "Opus");
        assert_eq!(rows[3].data.metrics[0].used, 64);
        assert!(rows[0].is_extra());
        // The tooltip has its own choice, still the default.
        let tip = visible_rows(&config, &sample(), Surface::Tooltip, &Models::new());
        assert_eq!(ids(&tip), ["claude", "codex", "antigravity"]);
    }

    /// Claude's Fable week, spent and resetting first: the Claude row still
    /// shows exactly what it showed before the week was reported, and the
    /// Fable row shows only once chosen.
    #[test]
    fn a_metric_only_an_extra_row_shows_leaves_the_own_row_as_it_was() {
        let before = sample();
        let mut after = before.clone();
        let mut fable = metric("Fable", 100, MetricWindow::Long);
        fable.reset_at = Some(utc::now() + SignedDuration::from_mins(5));
        fable.extra_only = true;
        after[0].metrics.push(fable);
        let config = Config::default();
        for surface in [Surface::Widget, Surface::Tooltip] {
            let old = visible_rows(&config, &before, surface, &Models::new());
            let new = visible_rows(&config, &after, surface, &Models::new());
            assert_eq!(ids(&new), ids(&old));
            let (o, n) = (&old[0].data, &new[0].data);
            let labels = |d: &ProviderData| {
                d.metrics
                    .iter()
                    .map(|m| m.label.clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(labels(n), labels(o));
            assert_eq!(n.primary_percentage(), o.primary_percentage());
            assert_eq!(n.headline_reset(), o.headline_reset());
        }
        // The provider's data as a whole keeps its headline too (the tray
        // icon, the taskbar panel and the alerts read it there).
        assert_eq!(
            after[0].primary_percentage(),
            before[0].primary_percentage()
        );
        assert_eq!(after[0].next_reset(), before[0].next_reset());
        // Chosen, the Fable row carries the week alone.
        let mut config = Config::default();
        config.ui.rows.widget = Some(vec!["claude".into(), "claude.fable".into()]);
        let rows = visible_rows(&config, &after, Surface::Widget, &Models::new());
        assert_eq!(ids(&rows), ["claude", "claude.fable"]);
        assert_eq!(rows[1].data.metrics.len(), 1);
        assert_eq!(rows[1].data.primary_percentage(), Some(100.0));
        assert_eq!(rows[1].data.headline_reset(), after[0].metrics[4].reset_at);
        assert_eq!(rows[1].name(), "Claude · Fable");
    }

    /// Claude with a Fable week whose reset is `fable_reset` from now and a
    /// session whose reset is `session_reset` from now.
    fn claude_with_fable(
        session_reset: SignedDuration,
        fable_reset: SignedDuration,
    ) -> ProviderData {
        let mut d = sample().remove(0);
        d.metrics[0].reset_at = Some(utc::now() + session_reset);
        let mut fable = metric("Fable", 40, MetricWindow::Long);
        fable.reset_at = Some(utc::now() + fable_reset);
        fable.extra_only = true;
        d.metrics.push(fable);
        d
    }

    fn is_estimate(d: &ProviderData) -> bool {
        matches!(d.status, ProviderStatus::Estimated)
    }

    /// Fable's week reset five minutes ago, nothing chosen: the Claude row,
    /// the tray and the panel read exactly as before — live, not "≈".
    #[test]
    fn a_passed_fable_reset_leaves_the_claude_row_live() {
        let raw = claude_with_fable(SignedDuration::from_hours(2), -SignedDuration::from_mins(5));
        // What the tray and the panel get (app.rs `visible_data`).
        let shown = raw.aged_for_display();
        assert!(matches!(shown.status, ProviderStatus::Ok));
        assert!(!shown.is_aged());
        assert_eq!(shown.primary_percentage(), Some(25.0));
        // The widget and the tooltip, from the raw data and from aged data.
        for input in [raw.clone(), shown] {
            for surface in [Surface::Widget, Surface::Tooltip] {
                let rows = visible_rows(
                    &Config::default(),
                    std::slice::from_ref(&input),
                    surface,
                    &Models::new(),
                );
                assert_eq!(ids(&rows), ["claude"]);
                let row = &rows[0].data;
                assert!(matches!(row.status, ProviderStatus::Ok), "{:?}", row.status);
                let used: Vec<u64> = row.metrics.iter().map(|m| m.used).collect();
                assert_eq!(used, [25, 34, 61, 12], "the same metrics, untouched");
            }
        }
    }

    /// The Fable row ages by its own week only: a passed session reset
    /// leaves it live, its own passed reset makes it "≈0%".
    #[test]
    fn an_extra_row_ages_by_its_own_metric() {
        let mut config = Config::default();
        config.ui.rows.widget = Some(vec!["claude".into(), "claude.fable".into()]);
        let rows_of = |d: &ProviderData| {
            let raw = visible_rows(
                &config,
                std::slice::from_ref(d),
                Surface::Widget,
                &Models::new(),
            );
            let aged = visible_rows(
                &config,
                &[d.aged_for_display()],
                Surface::Widget,
                &Models::new(),
            );
            (raw, aged)
        };

        // The session rolled over, Fable's week did not.
        let d = claude_with_fable(
            -SignedDuration::from_mins(5),
            SignedDuration::from_hours(3 * 24),
        );
        for rows in <[_; 2]>::from(rows_of(&d)) {
            assert_eq!(ids(&rows), ["claude", "claude.fable"]);
            assert!(is_estimate(&rows[0].data), "the session reset: ≈0%");
            assert!(!is_estimate(&rows[1].data), "not Fable's");
            assert_eq!(rows[1].data.metrics[0].used, 40);
        }

        // Fable's week rolled over, the session did not.
        let d = claude_with_fable(SignedDuration::from_hours(2), -SignedDuration::from_mins(5));
        for rows in <[_; 2]>::from(rows_of(&d)) {
            assert!(!is_estimate(&rows[0].data));
            assert!(is_estimate(&rows[1].data));
            assert_eq!(rows[1].data.metrics[0].used, 0);
            assert_eq!(rows[1].data.metrics[0].reset_at, None);
        }
    }

    /// The same for an extra row of one of the provider's own windows: the
    /// Opus row is an estimate for Opus's reset, never for the session's.
    #[test]
    fn an_own_window_row_ages_by_its_own_metric() {
        let mut config = Config::default();
        config.ui.rows.widget = Some(vec!["claude".into(), "claude.opus".into()]);
        let rows = |d: &ProviderData| {
            visible_rows(
                &config,
                std::slice::from_ref(d),
                Surface::Widget,
                &Models::new(),
            )
        };

        let mut session_reset = sample().remove(0);
        session_reset.metrics[0].reset_at = Some(utc::now() - SignedDuration::from_mins(5));
        for input in [session_reset.clone(), session_reset.aged_for_display()] {
            let r = rows(&input);
            assert!(is_estimate(&r[0].data));
            assert!(!is_estimate(&r[1].data), "Opus did not reset");
            assert_eq!(r[1].data.metrics[0].used, 61);
        }

        let mut opus_reset = sample().remove(0);
        opus_reset.metrics[2].reset_at = Some(utc::now() - SignedDuration::from_mins(5));
        for input in [opus_reset.clone(), opus_reset.aged_for_display()] {
            let r = rows(&input);
            assert!(is_estimate(&r[1].data));
            assert_eq!(r[1].data.metrics[0].used, 0);
        }
    }

    #[test]
    fn an_extra_row_without_its_metric_is_left_out() {
        let mut config = Config::default();
        config.ui.rows.widget = Some(vec!["claude".into(), "grok.build".into()]);
        let mut visible = sample();
        visible.push(data(
            ProviderId::Grok,
            vec![metric("Weekly", 12, MetricWindow::Long)],
        ));
        let rows = visible_rows(&config, &visible, Surface::Widget, &Models::new());
        assert_eq!(ids(&rows), ["claude"]);
    }

    #[test]
    fn unknown_row_ids_are_ignored() {
        let mut config = Config::default();
        config.ui.rows.widget = Some(vec!["claude".into(), "claude.haiku".into(), "x".into()]);
        let rows = visible_rows(&config, &sample(), Surface::Widget, &Models::new());
        assert_eq!(ids(&rows), ["claude"]);
    }

    /// Claude with a week of `label` only an extra row shows, as
    /// `providers::claude::model_weeks` reports a model's week.
    fn claude_with_week(label: &str, used: u64) -> ProviderData {
        let mut d = sample().remove(0);
        let mut week = metric(label, used, MetricWindow::Long);
        week.extra_only = true;
        d.metrics.push(week);
        d
    }

    fn row_ids(rows: &[&'static SubRow]) -> Vec<&'static str> {
        rows.iter().map(|s| s.id).collect()
    }

    /// A model Claude adds later gets a row of its own from the reply: not
    /// shown until chosen, the Claude row as it was, named as the
    /// catalogue's rows are.
    #[test]
    fn a_model_claude_adds_gets_a_row_of_its_own() {
        let d = claude_with_week("Haiku", 44);
        let one = std::slice::from_ref(&d);
        let rows = visible_rows(&Config::default(), one, Surface::Widget, &Models::new());
        assert_eq!(ids(&rows), ["claude"]);
        let used: Vec<u64> = rows[0].data.metrics.iter().map(|m| m.used).collect();
        assert_eq!(used, [25, 34, 61, 12], "the week is not the Claude row's");
        let mut config = Config::default();
        config.ui.rows.widget = Some(vec!["claude".into(), "claude.haiku".into()]);
        let rows = visible_rows(&config, one, Surface::Widget, &Models::new());
        assert_eq!(ids(&rows), ["claude", "claude.haiku"]);
        assert_eq!(rows[1].names, ["Claude · Haiku", "Cl · Haiku", "Haiku"]);
        assert_eq!(rows[1].data.metrics.len(), 1);
        assert_eq!(rows[1].data.metrics[0].used, 44);
        assert!(rows[1].model.is_none());
        // A reply without the week: no row; the id stays in the file.
        let rows = visible_rows(&config, &sample()[..1], Surface::Widget, &Models::new());
        assert_eq!(ids(&rows), ["claude"]);
    }

    #[test]
    fn made_rows_are_made_once_named_longest_first_and_only_for_claude() {
        let long = claude_with_week("Mythos Preview Extended", 1);
        let a = offered_rows(&ProviderId::Claude, Some(&long));
        assert_eq!(
            row_ids(&a),
            [
                "claude.opus",
                "claude.sonnet",
                "claude.fable",
                "claude.mythos-preview-extended"
            ]
        );
        let made = a[3];
        assert_eq!(
            made.names,
            [
                "Claude · Mythos Preview Exte…",
                "Cl · Mythos Preview Exte…",
                "Mythos Preview Exte…",
                "Mythos"
            ]
        );
        assert_eq!(made.metric, "Mythos Preview Extended");
        assert!(!made.on_request);
        assert!(made
            .hint
            .starts_with("Mythos Preview Exte…'s own weekly allowance"));
        // Asked again: the same row, not another one.
        let b = offered_rows(&ProviderId::Claude, Some(&long));
        assert!(std::ptr::eq(a[3], b[3]));
        // The line under its bar names it by its shortest form.
        let mut config = Config::default();
        config.ui.rows.widget = Some(vec!["claude.mythos-preview-extended".into()]);
        let shown = visible_rows(
            &config,
            std::slice::from_ref(&long),
            Surface::Widget,
            &Models::new(),
        );
        assert_eq!(shown[0].data.metrics[0].label, "Mythos");
        // A week the catalogue names keeps the catalogue's row.
        let fable = claude_with_week("fable", 1);
        assert_eq!(
            row_ids(&offered_rows(&ProviderId::Claude, Some(&fable))),
            ["claude.opus", "claude.sonnet", "claude.fable"]
        );
        // A name with no letter or digit: no row.
        let blank = claude_with_week(" · ", 1);
        assert_eq!(offered_rows(&ProviderId::Claude, Some(&blank)).len(), 3);
        // Another provider's week of its own never makes one.
        let mut grok = data(
            ProviderId::Grok,
            vec![metric("Weekly", 1, MetricWindow::Long)],
        );
        let mut heavy = metric("Heavy", 1, MetricWindow::Long);
        heavy.extra_only = true;
        grok.metrics.push(heavy);
        assert_eq!(
            row_ids(&offered_rows(&ProviderId::Grok, Some(&grok))),
            ["grok.build"]
        );
    }

    #[test]
    fn every_sub_row_belongs_to_a_known_provider_and_has_names() {
        for s in SUB_ROWS {
            assert!(s.id.starts_with(s.provider.config_id()));
            assert!(s.id.contains('.'));
            assert!(!s.names.is_empty());
            // Longest first.
            for w in s.names.windows(2) {
                assert!(w[0].chars().count() >= w[1].chars().count(), "{}", s.id);
            }
        }
    }

    fn info(id: &str, effort: Option<&str>) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            effort: effort.map(String::from),
        }
    }

    #[test]
    fn label_parts_follow_the_ticks() {
        let names = BTreeMap::new();
        let p = ProviderId::Claude;
        let i = info("Opus 5.5", Some("xhigh"));
        let none = model_label(&i, &p, &[], &names);
        assert!(none.is_none());
        // Version without the model shows nothing of the name.
        let v = model_label(&i, &p, &[ModelPart::Version], &names);
        assert!(v.is_none());
        let e = model_label(&i, &p, &[ModelPart::Effort], &names).unwrap();
        assert_eq!(e.steps(), ["xhigh"]);
        let me = model_label(&i, &p, &[ModelPart::Model, ModelPart::Effort], &names).unwrap();
        assert_eq!(me.full().as_deref(), Some("Opus · xhigh"));
        // An empty effort is no effort.
        let blank = model_label(&info("x", Some("  ")), &p, &[ModelPart::Effort], &names);
        assert!(blank.is_none());
        // No model written (an empty id): the effort alone, whatever is ticked.
        let all = [ModelPart::Model, ModelPart::Version, ModelPart::Effort];
        let bare = model_label(&info("", Some("xhigh")), &p, &all, &names).unwrap();
        assert_eq!(bare.steps(), ["xhigh"]);
        let model_only = model_label(&info("", Some("xhigh")), &p, &[ModelPart::Model], &names);
        assert!(model_only.is_none());
    }

    #[test]
    fn steps_drop_effort_then_version_then_model() {
        let label = ModelLabel {
            model: Some("Opus".into()),
            with_version: Some("Opus 5.5".into()),
            effort: Some("xhigh".into()),
        };
        assert_eq!(label.steps(), ["Opus 5.5 · xhigh", "Opus 5.5", "Opus"]);
        let no_version = ModelLabel {
            with_version: None,
            ..label.clone()
        };
        assert_eq!(no_version.steps(), ["Opus · xhigh", "Opus"]);
    }

    #[test]
    fn a_rename_replaces_the_automatic_name() {
        let mut names = BTreeMap::new();
        names.insert("claude-opus-5-5".to_string(), "My Opus".to_string());
        let label = model_label(
            &info("claude-opus-5-5", None),
            &ProviderId::Claude,
            &[ModelPart::Model],
            &names,
        )
        .unwrap();
        assert_eq!(label.full().as_deref(), Some("My Opus"));
    }

    /// A tool's own router is named per tool, as the picker keeps it
    /// ("copilot:auto"); a name an earlier version kept under the bare id
    /// still shows for both until one is renamed.
    #[test]
    fn a_router_renamed_in_the_picker_shows_under_its_tool() {
        let label = |names: &BTreeMap<String, String>, p: ProviderId| {
            model_label(&info("auto", None), &p, &[ModelPart::Model], names).and_then(|l| l.full())
        };
        let mut names = BTreeMap::new();
        names.insert("copilot:auto".to_string(), "Cop".to_string());
        assert_eq!(label(&names, ProviderId::Copilot).as_deref(), Some("Cop"));
        assert_ne!(label(&names, ProviderId::Cursor).as_deref(), Some("Cop"));
        let mut old = BTreeMap::new();
        old.insert("auto".to_string(), "Router".to_string());
        assert_eq!(label(&old, ProviderId::Copilot).as_deref(), Some("Router"));
        assert_eq!(label(&old, ProviderId::Cursor).as_deref(), Some("Router"));
    }

    #[test]
    fn model_labels_go_on_a_providers_own_row_only() {
        let mut config = Config::default();
        config.ui.rows.widget = Some(vec!["claude".into(), "claude.opus".into()]);
        config.ui.model.widget = vec![ModelPart::Model];
        let mut models = Models::new();
        models.insert(ProviderId::Claude, info("Opus", None));
        let rows = visible_rows(&config, &sample(), Surface::Widget, &models);
        assert!(rows[0].model.is_some());
        assert!(rows[1].model.is_none());
        // The tooltip has no parts on.
        let tip = visible_rows(&config, &sample(), Surface::Tooltip, &models);
        assert!(tip[0].model.is_none());
    }

    #[test]
    fn nothing_chosen_until_a_provider_is_allowed() {
        let mut config = Config::default();
        assert!(nothing_chosen(&config));
        config.allow_sign_in("codex");
        assert!(!nothing_chosen(&config));
        for p in config.providers.iter_mut() {
            p.enabled = false;
        }
        assert!(nothing_chosen(&config));
    }

    /// A fixed-advance stand-in for the font: 5 px per character at 10 px.
    fn mono(text: &str, size: f32) -> f32 {
        text.chars().count() as f32 * size * 0.5
    }

    #[test]
    fn names_are_held_to_the_narrowest_place() {
        // 56.5 px at 10.5 px is the narrower one here: 10 characters of
        // 5.25 px fit (52.5), 11 do not (57.75).
        assert!(name_fits("abcdefghij", mono));
        assert!(!name_fits("abcdefghijk", mono));
        assert_eq!(fit_name("abcdefghij klmnop", mono), "abcdefghij");
        assert_eq!(fit_name("  ab  ", mono), "ab");
        // Never splits a character.
        assert_eq!(fit_name("ééééééééééééé", mono), "éééééééééé");
    }
}
