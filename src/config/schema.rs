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
    // Buffer the value, then try to interpret it as T; unknown → default.
    // Config is always TOML, so toml::Value is the right intermediate.
    let raw = toml::Value::deserialize(deserializer)?;
    Ok(T::deserialize(raw).unwrap_or_default())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            window: WindowConfig::default(),
            ui: UIConfig::default(),
            // IMPORTANT: the default config MUST contain every provider,
            // otherwise the widget is empty on first launch.
            providers: default_providers(),
            notifications: NotificationConfig::default(),
            hooks: HooksConfig::default(),
        }
    }
}

/// User-configured shell commands run on usage events.
///
/// Empty strings disable the hook (the default). This is intentionally
/// config.toml-only — there is no menu/clipboard path, so a malicious
/// clipboard string can never become an auto-run command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    /// Run when a provider crosses its alert threshold (shares the toast cooldown).
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
            alert_threshold: threshold,
        };

    vec![
        // Claude — subscription: the Claude Code OAuth token, no key needed.
        base("claude", true, AuthMethod::Subscription, "", 80),
        // Codex — ChatGPT subscription via the Codex CLI auth.json.
        base("codex", true, AuthMethod::Subscription, "", 80),
        // Copilot — subscription via the gh CLI token or a PAT.
        base("copilot", true, AuthMethod::Subscription, "", 85),
        // Antigravity (the Gemini successor) — keyring token first, then the
        // legacy Gemini CLI file.
        base("antigravity", true, AuthMethod::Subscription, "", 80),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            update_interval_secs: default_update_interval(),
            indicator: IndicatorKind::default(),
            auto_update: default_auto_update(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Show a burn-rate forecast ("~Xh to limit") when usage is climbing.
    /// It never replaces the reset countdown — it only fills the slot when no
    /// future reset is known. Off by default: the reset time is the fact
    /// users plan around, the projection is optional extra.
    #[serde(default)]
    pub show_forecast: bool,
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
            show_forecast: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Per-provider toast cooldown, minutes.
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DetailLevel {
    #[default]
    Compact,
    Medium,
    Expanded,
}

/// The taskbar usage indicator: a tray pie icon, a 16px tray icon with
/// stacked bars (legacy, config-only), a transparent overlay painted over
/// the taskbar next to the tray (monochrome, system-theme-following; the
/// two busiest providers as clock-sized "percent + bar" rows), or none.
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    #[default]
    Subscription,
    ApiKey,
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
