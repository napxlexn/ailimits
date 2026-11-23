// config/schema.rs — configuration structs.
// Full format documentation: docs/en/CONFIG.md.

use serde::{Deserialize, Serialize};

/// Deserialize an enum field tolerantly: an UNKNOWN value (a typo, or a value
/// renamed in a newer version) falls back to that field's own default instead
/// of failing the whole document. Without this, one bad enum string in a
/// hand-edited config.toml would make the ENTIRE config fail to parse and
/// silently reset every other setting (window position, palette, providers,
/// hooks…). This mirrors the tolerance already applied to unknown fields
/// (ignored) and unknown providers (skipped) — see docs: "old configs never
/// break parsing".
fn de_enum_or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    // Config is always TOML, so toml::Value is the right intermediate.
    let raw = toml::Value::deserialize(deserializer)?;
    Ok(T::deserialize(raw).unwrap_or_default())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub window: WindowConfig,
    #[serde(default)]
    pub ui: UIConfig,
    // If config.toml has no [[providers]] sections, fall back to the defaults.
    #[serde(default = "default_providers")]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
    /// What the user allowed the app to read of other tools' files. Nothing
    /// of theirs is read before it is allowed here.
    #[serde(default)]
    pub reads: Reads,
    /// Fine-tuning that lives only in this file (`config::tune`).
    #[serde(default, skip_serializing_if = "crate::config::tune::Tune::is_default")]
    pub tune: crate::config::tune::Tune,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            window: WindowConfig::default(),
            ui: UIConfig::default(),
            // IMPORTANT: the default config MUST contain every provider,
            // otherwise the menu has nothing to offer on first launch.
            providers: default_providers(),
            notifications: NotificationConfig::default(),
            hooks: HooksConfig::default(),
            reads: Reads::default(),
            tune: Default::default(),
        }
    }
}

impl Config {
    /// The config of a first start, as `storage` settles it: every provider
    /// listed but switched off, nothing allowed, the welcome owed. What a
    /// new install really runs on (`Config::default()` is the raw template
    /// before that, every provider on).
    pub fn fresh_install() -> Self {
        let mut config = Config::default();
        super::storage::settle_reads(&mut config, super::storage::Origin::FreshInstall);
        config
    }

    /// Whether the user allowed reading this provider's sign-in (the other
    /// tool's token file, its Credential Manager entry, `gh auth token`).
    pub fn sign_in_allowed(&self, provider_id: &str) -> bool {
        let id = canonical_provider_id(provider_id);
        self.reads
            .sign_in
            .iter()
            .any(|p| canonical_provider_id(p) == id)
    }

    /// Record that the user allowed reading this provider's sign-in, and
    /// the terms it was allowed under (`consent::terms_of`).
    pub fn allow_sign_in(&mut self, provider_id: &str) {
        let id = canonical_provider_id(provider_id).to_string();
        if !self.sign_in_allowed(&id) {
            self.reads.sign_in.push(id.clone());
        }
        self.reads
            .sign_in_terms
            .insert(id.clone(), crate::consent::terms_of(&id));
    }

    /// Whether the provider was set up with a sign-in of the app's own: a
    /// key or token the user pasted, the app's own Cursor session. That
    /// allows those alone, never the other tool's files.
    pub fn app_sign_in_allowed(&self, provider_id: &str) -> bool {
        let id = canonical_provider_id(provider_id);
        self.reads
            .app_sign_in
            .iter()
            .any(|p| canonical_provider_id(p) == id)
    }

    /// Record that the provider was set up with the app's own sign-in.
    pub fn allow_app_sign_in(&mut self, provider_id: &str) {
        if !self.app_sign_in_allowed(provider_id) {
            self.reads
                .app_sign_in
                .push(canonical_provider_id(provider_id).to_string());
        }
    }

    /// Stop a provider: nothing of it is read any more, of the tool's files
    /// or of the app's own, until the user allows it again.
    pub fn revoke(&mut self, provider_id: &str) {
        let id = canonical_provider_id(provider_id).to_string();
        self.reads
            .sign_in
            .retain(|p| canonical_provider_id(p) != id);
        self.reads
            .app_sign_in
            .retain(|p| canonical_provider_id(p) != id);
        for p in self.providers.iter_mut() {
            if canonical_provider_id(&p.id) == id {
                p.enabled = false;
            }
        }
    }

    /// Pause a provider: switched off, so nothing of it is read or sent,
    /// while its permission stays. A tick switches it on again without
    /// asking.
    pub fn pause(&mut self, provider_id: &str) {
        let id = canonical_provider_id(provider_id).to_string();
        for p in self.providers.iter_mut() {
            if canonical_provider_id(&p.id) == id {
                p.enabled = false;
            }
        }
    }

    /// Take back the permission to read the provider's tool sign-in, and
    /// switch it off. A sign-in of the app's own (a pasted key) stays.
    pub fn forget_sign_in(&mut self, provider_id: &str) {
        let id = canonical_provider_id(provider_id).to_string();
        self.reads
            .sign_in
            .retain(|p| canonical_provider_id(p) != id);
        self.reads.sign_in_terms.remove(&id);
        self.pause(&id);
    }

    /// A provider that may fetch: switched on AND allowed, either to read
    /// the tool's sign-in or to use the app's own. Everything that decides
    /// whether a provider runs or shows goes through this one test; which
    /// sources it may then use is `ProviderConfig::tool_files`.
    pub fn provider_active(&self, pc: &ProviderConfig) -> bool {
        pc.enabled && (self.sign_in_allowed(&pc.id) || self.app_sign_in_allowed(&pc.id))
    }
}

/// The current id for a config provider id ("gemini" became "antigravity").
fn canonical_provider_id(id: &str) -> &str {
    match id {
        "gemini" => "antigravity",
        other => other,
    }
}

/// Reads of other tools' files, each allowed by the user before it happens
/// (asked once, naming the file). See docs/en/CONFIG.md, `[reads]`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Reads {
    /// Providers whose sign-in may be read, by config id.
    #[serde(default)]
    pub sign_in: Vec<String>,
    /// Providers set up with the app's own sign-in only (a pasted key or
    /// token, the menu's Cursor sign-in): they use that, and the tool's own
    /// sign-in is never read for them. For the model label their tool's
    /// files are still read as for every running provider: the settings
    /// file when `tool_settings` allows it, and the newest session when
    /// `last_session` does (Claude Code, Codex CLI).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_sign_in: Vec<String>,
    /// The tools' own settings files, for the model and effort they are set
    /// to (Claude Code settings.json, Codex config.toml, Cursor's state
    /// database...), for every running provider, however it signs in.
    #[serde(default)]
    pub tool_settings: bool,
    /// The end of the newest session file, for the model last used (Claude
    /// Code, Codex CLI).
    #[serde(default)]
    pub last_session: bool,
    /// A one-time window still owed: the welcome on a fresh install, the
    /// notice after the update that introduced `[reads]`.
    #[serde(default, deserialize_with = "de_enum_or_default")]
    pub pending: PendingNotice,
    /// The sign-in sources were settled, once: a key or token pasted before
    /// the source could be chosen keeps the lead it had (app/sign_in.rs).
    /// Never again after that, so a source chosen in the window stays.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sources_settled: bool,
    /// The terms each provider's sign-in was allowed under
    /// (`consent::ReadInfo::terms`). A version that reads another file for
    /// it or sends its token elsewhere has higher terms: the permission is
    /// asked for again (`choices::settle_terms`). One allowed before terms
    /// were kept gets today's.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub sign_in_terms: std::collections::BTreeMap<String, u32>,
    /// The terms the model label's reads were allowed under
    /// (`consent::MODEL_TERMS`); 0 when never allowed.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub model_terms: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// The one-time window the app still has to show.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PendingNotice {
    #[default]
    None,
    /// Fresh install: the picker opens by itself, titled as a welcome.
    Welcome,
    /// Upgraded from a version that read without asking: one window lists
    /// every file read and every host contacted. Reading goes on meanwhile.
    UpdateNotice,
}

/// User-configured shell commands run on usage events.
///
/// Empty strings disable the hook (the default). This is intentionally
/// config.toml-only — there is no menu/clipboard path, so a malicious
/// clipboard string can never become an auto-run command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    /// Run when a provider crosses its alert threshold: once per limit
    /// window, like the toast (notifications/alerts.rs).
    #[serde(default)]
    pub on_threshold: String,
    /// Run when a provider's limit window resets (usage drops from near-limit).
    #[serde(default)]
    pub on_reset: String,
    /// Run once after the first successful fetch of any provider.
    #[serde(default)]
    pub on_startup: String,
}

/// Default provider set for the first launch.
fn default_providers() -> Vec<ProviderConfig> {
    // Base template to avoid field duplication.
    let base =
        |id: &str, enabled: bool, auth: AuthMethod, label: &str, threshold: u8| ProviderConfig {
            id: id.to_string(),
            enabled,
            auth_method: auth,
            credential_label: label.to_string(),
            tool_files: false,
            alert_threshold: threshold,
        };

    vec![
        // Claude — subscription: the Claude Code OAuth token, no key needed.
        base("claude", true, AuthMethod::Subscription, "", 80),
        // Codex — ChatGPT subscription via the Codex CLI auth.json.
        base("codex", true, AuthMethod::Subscription, "", 80),
        // Copilot — subscription via the gh CLI token or a PAT.
        base("copilot", true, AuthMethod::Subscription, "", 85),
        // Antigravity (the Gemini successor) — its keyring token.
        base("antigravity", true, AuthMethod::Subscription, "", 80),
        // Grok — SuperGrok subscription via the Grok CLI's auth.json.
        base("grok", true, AuthMethod::Subscription, "", 80),
        // Cursor — the widget's own session only (its Sign-in in Rows and
        // models, or ailimits-auth login cursor). Hidden until it exists.
        base("cursor", true, AuthMethod::Subscription, "", 80),
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_update_interval")]
    pub update_interval_secs: u64,
    /// Secondary usage indicator besides the widget. Supersedes the legacy
    /// `show_tray_icon` flag (old configs: the unknown field is ignored and
    /// this defaults to Tray, which matches the old default).
    #[serde(default, deserialize_with = "de_enum_or_default")]
    pub indicator: IndicatorKind,
    /// Silently install newer releases in the background. On by default; the
    /// context menu exposes a toggle. Old configs without the field default to
    /// enabled, matching the shipped behaviour.
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    /// Manual nudge for the taskbar panel, in pixels, applied after the
    /// computed position. Config-only: a repair tool for shell replacements
    /// and unusual taskbars, not a preference. Clamped to +/-200.
    #[serde(default)]
    pub panel_offset_x: i32,
    #[serde(default)]
    pub panel_offset_y: i32,
    /// Taskbar the panel indicator attaches to. Falls back to the primary
    /// when the chosen display is gone.
    #[serde(default, deserialize_with = "de_enum_or_default")]
    pub panel_display: PanelDisplay,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            update_interval_secs: default_update_interval(),
            indicator: IndicatorKind::default(),
            auto_update: default_auto_update(),
            panel_offset_x: 0,
            panel_offset_y: 0,
            panel_display: PanelDisplay::Primary,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowConfig {
    #[serde(default = "default_pos_x")]
    pub pos_x: i32,
    #[serde(default = "default_pos_y")]
    pub pos_y: i32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    /// Always on top.
    #[serde(default)]
    pub pinned: bool,
    /// Locked position — dragging disabled; INDEPENDENT of `pinned`.
    #[serde(default)]
    pub locked: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            pos_x: default_pos_x(),
            pos_y: default_pos_y(),
            opacity: default_opacity(),
            pinned: false,
            locked: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UIConfig {
    #[serde(default, deserialize_with = "de_enum_or_default")]
    pub palette: Palette,
    #[serde(default = "default_saturation")]
    pub saturation: u8,
    /// Text and bar brightness, 20–100%.
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    #[serde(default)]
    pub monochrome: bool,
    #[serde(default, deserialize_with = "de_enum_or_default")]
    pub layout: Layout,
    #[serde(default, deserialize_with = "de_enum_or_default")]
    pub detail: DetailLevel,
    /// Width step for the rows layout; the columns layout ignores it.
    #[serde(default, deserialize_with = "de_enum_or_default")]
    pub width_scale: WidthScale,
    /// Column arrangement for the vertical-bars layout; the rows layout
    /// ignores it. The two settings are the same choice on opposite axes.
    #[serde(default, deserialize_with = "de_enum_or_default")]
    pub column_flow: ColumnFlow,
    /// Show a burn-rate forecast ("~Xh to limit") when usage is climbing.
    /// It never replaces the reset countdown — it only fills the slot when no
    /// future reset is known. Off by default: the reset time is the fact
    /// users plan around, the projection is optional extra.
    #[serde(default)]
    pub show_forecast: bool,
    /// Which rows the widget and the tooltip show. Absent: one row per
    /// provider, as before rows could be chosen.
    #[serde(default, skip_serializing_if = "UiRows::is_default")]
    pub rows: UiRows,
    /// The model, version and effort labels. Absent: none shown.
    #[serde(default, skip_serializing_if = "UiModel::is_default")]
    pub model: UiModel,
    /// What each widget row shows: its percentage, the time left until its
    /// reset, its progress bar, in any mix but never none. Absent: all
    /// three, the widget as it always was.
    #[serde(
        default,
        deserialize_with = "de_widget_numbers",
        skip_serializing_if = "UiNumbers::is_default"
    )]
    pub widget: UiNumbers,
    /// What each row of the panel's tooltip shows: its percentage, the time
    /// left until its reset, or both. Absent: both.
    #[serde(default, skip_serializing_if = "UiNumbers::is_default")]
    pub tooltip: UiNumbers,
    /// What each row of the taskbar panel shows: the percentage (with its
    /// bar), the time left until the reset, or both. Absent: the percentage
    /// alone, the panel of 0.7.0; a key left out of a hand-written table
    /// keeps that default too (`[ui.panel] bar = false` is the percentage
    /// alone, without its bar).
    #[serde(
        default = "UiNumbers::panel_default",
        deserialize_with = "de_panel_numbers",
        skip_serializing_if = "UiNumbers::is_panel_default"
    )]
    pub panel: UiNumbers,
    /// The tray icon's look. Absent: the two rings.
    #[serde(default, skip_serializing_if = "UiTray::is_default")]
    pub tray: UiTray,
}

/// `[ui.tray]`: how the tray icon draws its two providers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct UiTray {
    #[serde(default, deserialize_with = "de_enum_or_default")]
    pub style: TrayStyle,
}

impl UiTray {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// The tray icon's look (ui/tray.rs): what it draws of the two providers
/// the panel shows. Every style is monochrome in the taskbar's ink.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrayStyle {
    /// Two concentric rings, the busier outside.
    #[default]
    Rings,
    /// Two upright bars side by side, filling from the bottom.
    Bars,
    /// The busier one's percentage as digits.
    Number,
    /// A filled sector of the busier one, like a clock face.
    Sector,
}

impl Default for UIConfig {
    fn default() -> Self {
        Self {
            palette: Palette::Default,
            saturation: default_saturation(),
            brightness: default_brightness(),
            monochrome: false,
            layout: Layout::Vertical,
            detail: DetailLevel::Compact,
            width_scale: WidthScale::Full,
            column_flow: ColumnFlow::Row,
            show_forecast: false,
            rows: UiRows::default(),
            model: UiModel::default(),
            tooltip: UiNumbers::default(),
            widget: UiNumbers::default(),
            panel: UiNumbers::panel_default(),
            tray: UiTray::default(),
        }
    }
}

/// The rows chosen for each surface, by row id: a provider's own row is its
/// config id ("claude"), an extra row is provider.part ("claude.opus"; the
/// catalogue is `crate::rows::SUB_ROWS`). `None` keeps the default: every
/// provider's own row, no extra rows. Unknown ids are kept and ignored, so a
/// newer version's rows survive a round trip through an older one.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct UiRows {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<Vec<String>>,
    /// The providers the taskbar panel and the tray icon show, by config id,
    /// at most `rows::TASKBAR_MAX`: the picker's Taskbar column. `None`:
    /// both show the first two running providers, in the widget's order
    /// (the tray icon inherits the panel's), and the file gets no
    /// `taskbar` line until the user chooses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taskbar: Option<Vec<String>>,
}

impl UiRows {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Model labels: which parts each surface shows, where the model comes from,
/// and the names the user gave models.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct UiModel {
    #[serde(default, deserialize_with = "de_parts")]
    pub widget: Vec<ModelPart>,
    #[serde(default, deserialize_with = "de_parts")]
    pub tooltip: Vec<ModelPart>,
    #[serde(default, deserialize_with = "de_enum_or_default")]
    pub source: ModelSource,
    /// Model id as the tool stores it -> the name to show instead of the
    /// automatic one. Limited to what fits the narrowest place the widget
    /// draws a model name (`crate::rows::name_fits`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub names: std::collections::BTreeMap<String, String>,
}

impl UiModel {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The parts on for one surface.
    pub fn parts(&self, surface: Surface) -> &[ModelPart] {
        match surface {
            Surface::Widget => &self.widget,
            Surface::Tooltip => &self.tooltip,
        }
    }
}

/// The two surfaces whose rows and labels are chosen separately. The taskbar
/// panel and the tray icon keep one entry per provider and no labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Surface {
    Widget,
    Tooltip,
}

/// What each row shows besides its name, on the panel's tooltip
/// (`UIConfig::tooltip`) or on the taskbar panel itself (`UIConfig::panel`):
/// the percentage, the time left until it resets, or both; never neither
/// (the picker keeps one on). The tray icon's tooltip says what the panel's
/// does (`tray::tray_tip_text`); the widget always shows both. `bar` is the taskbar panel's
/// progress bar, on unless switched off to take less of the taskbar; the
/// tooltip has none and ignores it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiNumbers {
    #[serde(default = "default_true")]
    pub percent: bool,
    #[serde(default = "default_true")]
    pub reset: bool,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub bar: bool,
}

/// Both, the owner's choice (2026-11-20): each row says how much is used
/// and when it resets.
impl Default for UiNumbers {
    fn default() -> Self {
        Self {
            percent: true,
            reset: true,
            bar: true,
        }
    }
}

impl UiNumbers {
    /// The percentage alone (with the panel's bar): the tooltip's one line
    /// and the panel of 0.7.0.
    pub const PERCENT_ONLY: Self = Self {
        percent: true,
        reset: false,
        bar: true,
    };
}

impl UiNumbers {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The panel's default: the percentage alone, as in 0.7.0.
    pub fn panel_default() -> Self {
        Self::PERCENT_ONLY
    }

    fn is_panel_default(&self) -> bool {
        *self == Self::PERCENT_ONLY
    }

    /// Whether `part` is on.
    pub fn shows(&self, part: TipPart) -> bool {
        match part {
            TipPart::Percent => self.percent,
            TipPart::Reset => self.reset,
            TipPart::Bar => self.bar,
        }
    }

    /// Turn `part` on or off where it shows; the last thing on there stays
    /// on, so nothing is ever empty: of the percentage and the reset time
    /// on the tooltip (it has no bar), of those and the bar on the widget
    /// and the panel.
    pub fn set(&mut self, part: TipPart, on: bool, place: NumbersOn) {
        let mut next = *self;
        match part {
            TipPart::Percent => next.percent = on,
            TipPart::Reset => next.reset = on,
            TipPart::Bar => next.bar = on,
        }
        if next.shows_any(place) {
            *self = next;
        }
    }

    /// Whether anything is left to show at `place`.
    pub fn shows_any(&self, place: NumbersOn) -> bool {
        match place {
            NumbersOn::Tooltip => self.percent || self.reset,
            NumbersOn::Widget | NumbersOn::Panel => self.percent || self.reset || self.bar,
        }
    }
}

/// One thing a tooltip or panel row can show (`UiNumbers`). The bar is the
/// taskbar panel's only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipPart {
    Percent,
    Reset,
    Bar,
}

/// Where a `UiNumbers` choice applies: the widget, the tooltip of the panel
/// and the tray icon, or the taskbar panel itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumbersOn {
    Widget,
    Tooltip,
    Panel,
}

/// One part of a model label: "Opus" (model), "5.5" (version, shown only
/// with the model), "xhigh" (effort, may be shown alone).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelPart {
    Model,
    Version,
    Effort,
}

/// Where the model comes from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelSource {
    /// The model set in the tool's settings file.
    #[default]
    Configured,
    /// The model of the newest session (Claude Code, Codex CLI); the others
    /// keep showing the configured one.
    LastUsed,
}

/// A list of model parts where an unknown entry is skipped instead of failing
/// the whole config; duplicates collapse.
fn de_parts<'de, D>(deserializer: D) -> Result<Vec<ModelPart>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = toml::Value::deserialize(deserializer)?;
    let mut out = Vec::new();
    if let toml::Value::Array(items) = raw {
        for item in items {
            if let Ok(part) = ModelPart::deserialize(item) {
                if !out.contains(&part) {
                    out.push(part);
                }
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, deserialize_with = "de_enum_or_default")]
    pub auth_method: AuthMethod,
    /// Credential Manager key label; empty for the subscription method.
    #[serde(default)]
    pub credential_label: String,
    /// Notification threshold, %.
    #[serde(default = "default_threshold")]
    pub alert_threshold: u8,
    /// Whether the provider may read the other tool's own sign-in: its token
    /// file, its Credential Manager entry, `gh auth token`. Set from
    /// `[reads] sign_in` when the provider is built (see
    /// app/provider_set.rs); never saved. Without it a provider uses
    /// only what is the app's own, and otherwise reports nothing.
    #[serde(skip)]
    pub tool_files: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// The shortest time between two alerts of one provider, minutes. An
    /// alert is said once per limit window, not again every cooldown.
    #[serde(default = "default_cooldown")]
    pub cooldown_minutes: u32,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cooldown_minutes: default_cooldown(),
        }
    }
}

// ─── Enum types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Palette {
    #[default]
    Default,
    Ocean,
    Sunset,
    Forest,
    Neon,
    Ice,
    Rose,
    Slate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    #[default]
    Vertical,
    Horizontal,
    /// Legacy value kept for old configs; renders as Vertical.
    Grid,
}

/// Widget width as a fraction of its natural width, for the rows layout.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WidthScale {
    #[default]
    Full,
    ThreeQuarters,
    Half,
}

/// How the provider columns are arranged when the bars are vertical.
/// Ignored by the rows layout, which has no columns to arrange.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ColumnFlow {
    /// Side by side: a wide, short widget.
    #[default]
    Row,
    /// Stacked downwards: a narrow, tall widget.
    Column,
}

impl WidthScale {
    /// The fraction of the natural width this step keeps.
    pub fn fraction(&self) -> f32 {
        match self {
            Self::Full => 1.0,
            Self::ThreeQuarters => 0.75,
            Self::Half => 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DetailLevel {
    #[default]
    Compact,
    Medium,
    Expanded,
}

/// The taskbar usage indicator: a tray ring icon, a 16px tray icon with
/// stacked bars (legacy, config-only), a transparent overlay painted over
/// the taskbar next to the tray (monochrome, system-theme-following; two
/// providers as clock-sized rows: the first two in the widget's order, or
/// the two ticked in the Taskbar column), or none.
/// `PanelGrid` is a legacy alias of `PanelRows` — the overlay renders both
/// identically; old configs keep parsing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorKind {
    #[default]
    Tray,
    Bars,
    PanelRows,
    PanelGrid,
    Off,
}

/// Which taskbar the mini panel attaches to. Secondary bars are numbered
/// left to right, starting at 0, across the taskbars that actually exist.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PanelDisplay {
    #[default]
    Primary,
    Secondary(u8),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// The tool's own sign-in (Claude Code, Codex CLI, GitHub CLI, ...).
    #[default]
    Subscription,
    /// A key of the user's own: Claude's API key, Copilot's token.
    ApiKey,
    /// A usage token the user pasted (Claude, Codex). An older version
    /// reads it as Subscription.
    UsageToken,
}

// ─── Default values ───────────────────────────────────────────────
fn default_update_interval() -> u64 {
    60
}
fn default_auto_update() -> bool {
    true
}
fn default_opacity() -> f32 {
    0.45
}
fn default_pos_x() -> i32 {
    50
}
fn default_pos_y() -> i32 {
    50
}
fn default_brightness() -> u8 {
    100
}
fn default_saturation() -> u8 {
    55
}
fn default_threshold() -> u8 {
    80
}
fn default_cooldown() -> u32 {
    15
}
fn default_true() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

/// `[ui.panel]`: each key left out keeps the panel's own default (the
/// percentage with its bar), not the tooltip's.
fn de_panel_numbers<'de, D: serde::Deserializer<'de>>(d: D) -> Result<UiNumbers, D::Error> {
    #[derive(Deserialize)]
    struct Partial {
        percent: Option<bool>,
        reset: Option<bool>,
        bar: Option<bool>,
    }
    let p = Partial::deserialize(d)?;
    let base = UiNumbers::panel_default();
    let n = UiNumbers {
        percent: p.percent.unwrap_or(base.percent),
        reset: p.reset.unwrap_or(base.reset),
        bar: p.bar.unwrap_or(base.bar),
    };
    // Never nothing: as the picker keeps one on.
    Ok(if n.shows_any(NumbersOn::Panel) {
        n
    } else {
        base
    })
}

/// `[ui.widget]`: each key left out keeps the widget's default (shown);
/// nothing at all reads as the default.
fn de_widget_numbers<'de, D: serde::Deserializer<'de>>(d: D) -> Result<UiNumbers, D::Error> {
    #[derive(Deserialize)]
    struct Partial {
        percent: Option<bool>,
        reset: Option<bool>,
        bar: Option<bool>,
    }
    let p = Partial::deserialize(d)?;
    let n = UiNumbers {
        percent: p.percent.unwrap_or(true),
        reset: p.reset.unwrap_or(true),
        bar: p.bar.unwrap_or(true),
    };
    Ok(if n.shows_any(NumbersOn::Widget) {
        n
    } else {
        UiNumbers::default()
    })
}
