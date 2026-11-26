// config/tune.rs — `[tune]`: fine-tuning that lives only in config.toml.
//
// Knobs too fine for the context menu or Settings: sizes, spacing,
// colours, thresholds and formats of the widget, the taskbar panel and the
// tray icon. Every field is optional; its default is exactly the look the
// app has without it (the pinned visual baselines hold that), and a table
// equal to its default is never written. Values out of range are clamped
// where they are read, never rejected: a typo cannot break the config.
// The user's guide is docs/en/TUNING.md.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The `[tune]` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Tune {
    #[serde(skip_serializing_if = "TuneWidget::is_default")]
    pub widget: TuneWidget,
    #[serde(skip_serializing_if = "TunePanel::is_default")]
    pub panel: TunePanel,
    #[serde(skip_serializing_if = "TuneTray::is_default")]
    pub tray: TuneTray,
    #[serde(skip_serializing_if = "TuneFormat::is_default")]
    pub format: TuneFormat,
}

impl Tune {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// `[tune.widget]`: the floating widget's sizes, spacing and colours.
///
/// Every key is optional and kept as written (an unset key is not written
/// back). A number out of its range is clamped, a value of the wrong kind or
/// a colour that is not `"#rrggbb"` is ignored: the key then reads as its
/// default. The accessors below are where the values are read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TuneWidget {
    /// Text size, as a factor of today's sizes: every text in the widget,
    /// and with it the room the text takes (row heights, the name and
    /// percent columns, the widget's width). Default 1.0, range 0.8–1.5.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub text_scale: Option<f32>,
    /// Extra space between two rows (rows layout) or two columns (columns
    /// layout), in pixels. Default 0, range 0–24.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub row_gap: Option<f32>,
    /// Bar thickness, as a factor of today's: 3 px (Compact rows), 4 px
    /// (Medium), 5 px (Expanded); 7, 8.5 and 11 px wide in the columns
    /// layout. A thicker bar makes a Medium or Expanded row taller by what it
    /// adds. Default 1.0, range 0.5–3.0.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub bar_scale: Option<f32>,
    /// Inner padding between the widget's edge and its rows or columns, as
    /// a factor of today's: 11 px at the sides and 4 px above and below the
    /// rows; 7 px at the sides, 4 px above and 6 px below the columns. The
    /// widget grows by what it adds. Default 1.0, range 0.0–3.0; under about
    /// 0.5 the round corners clip the text in them (`corners = "square"`
    /// does not).
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub padding_scale: Option<f32>,
    /// The widget's corners, as Windows 11 rounds a window: "round"
    /// (default, about 8 px), "small" (about 4 px) or "square". Windows 10
    /// draws every window square.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub corners: Option<Corners>,
    /// The colour of a low usage (below `[tune.format] mid_at`): its bar and
    /// its percent, as "#rrggbb", in place of the palette's. The menu's
    /// brightness still applies; Monochrome still turns it grey. The tray
    /// icon keeps the palette. Default: the palette's colour.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub low: Option<String>,
    /// The colour of a middle usage (from `mid_at` to below `high_at`), as
    /// "#rrggbb"; as `low`. Default: the palette's colour.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub mid: Option<String>,
    /// The colour of a high usage (from `high_at`), as "#rrggbb"; as `low`.
    /// The forecast ("~1h 30min") takes it too. Default: the palette's
    /// colour.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub high: Option<String>,
    /// An accent per provider: its rows' names drawn in that colour, as
    /// "#rrggbb", keyed by the provider's config id ("claude", "codex",
    /// "copilot", "antigravity", "grok", "cursor"), e.g.
    /// `provider_colors = { claude = "#d97757" }`. An unknown provider or an
    /// invalid colour is ignored. Default: none, every name in the widget's
    /// grey. The menu's brightness applies; Monochrome leaves them grey.
    #[serde(
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "de_string_map"
    )]
    pub provider_colors: BTreeMap<String, String>,
}

/// How Windows 11 rounds the widget's corners
/// (`DWMWA_WINDOW_CORNER_PREFERENCE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Corners {
    /// Windows' standard radius, about 8 px: the widget as it always was.
    #[default]
    Round,
    /// Windows' small radius, about 4 px.
    Small,
    /// No rounding.
    Square,
}

impl TuneWidget {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// `text_scale`, clamped: 0.8–1.5, default 1.0.
    pub fn text_scale(&self) -> f32 {
        clamped(self.text_scale, 1.0, 0.8, 1.5)
    }

    /// `row_gap` in pixels, clamped: 0–24, default 0.
    pub fn row_gap(&self) -> f32 {
        clamped(self.row_gap, 0.0, 0.0, 24.0)
    }

    /// `bar_scale`, clamped: 0.5–3.0, default 1.0.
    pub fn bar_scale(&self) -> f32 {
        clamped(self.bar_scale, 1.0, 0.5, 3.0)
    }

    /// `padding_scale`, clamped: 0.0–3.0, default 1.0.
    pub fn padding_scale(&self) -> f32 {
        clamped(self.padding_scale, 1.0, 0.0, 3.0)
    }

    /// `corners`, default round.
    pub fn corners(&self) -> Corners {
        self.corners.unwrap_or_default()
    }
}

/// `[tune.panel]`: the taskbar panel's sizes, spacing and ink.
///
/// Every knob is a multiple of today's look (1.0) unless its unit says
/// otherwise. A key left out, or holding the wrong kind of value, reads as
/// its default; a number outside its range reads as the nearest end of it.
/// The panel's size follows the knobs, and so does the room it asks of the
/// taskbar: a bigger panel that no longer fits beside the tray stands down
/// for the tray icon, as it does today when the bar is full.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TunePanel {
    /// The digits' height, as a multiple of today's: 0.225 of the taskbar's
    /// thickness, the system clock's digit size (about 11 px of ink on a
    /// 48 px taskbar). The number's column widens with it, so a wider number
    /// is not squeezed. Range 0.5 to 1.5, default 1.0. The row gap and the
    /// bar's thickness are measured in digit heights, so they grow with it.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub digit_size: Option<f64>,
    /// The usage bar's length, as a multiple of today's (about half the
    /// panel's width on a horizontal taskbar). Lying down the panel grows or
    /// shrinks with the bar; standing up (a taskbar on the left or right) the
    /// panel is as wide as the taskbar, so the bar stays centred and at most
    /// that wide (less a pixel at each end). Range 0.25 to 3.0, default 1.0.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub bar_length: Option<f64>,
    /// The usage bar's thickness, as a multiple of today's: half a digit's
    /// height. Its ends stay round. Range 0.25 to 2.0, default 1.0.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub bar_thickness: Option<f64>,
    /// The space between the two providers' rows, as a multiple of today's:
    /// 0.4 of a digit's height lying down, 0.6 standing up. 0 puts the rows
    /// edge to edge. Range 0.0 to 2.0, default 1.0.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub row_gap: Option<f64>,
    /// The space between the panel and the notification area (the tray, the
    /// clock), in screen pixels along the taskbar, not scaled with the
    /// display scaling (as today). It moves the panel AND changes the room
    /// it needs: the panel only shows where the panel plus this gap fit
    /// between the last app button and the tray. Where the tray's edge
    /// could not be measured, 10 more pixels are kept, as today. Whole
    /// pixels, range 0 to 200, default 10.
    ///
    /// `[general] panel_offset_x` / `panel_offset_y` are different: they
    /// nudge the panel after it is placed, in either direction and across the
    /// bar too (up to 200 px), without changing the room it asks for.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub tray_gap: Option<i64>,
    /// The ink on a DARK taskbar, `"#rrggbb"`, in place of the system's
    /// near-white (`#ececec`). The bar's groove and the dimmer ink of a stale
    /// or estimated row are derived from it as they are from the default
    /// (the same colour at the groove's and the halfway transparency).
    /// Anything but `#` and six hex digits is ignored. Default: none.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub ink_dark: Option<String>,
    /// The ink on a LIGHT taskbar, `"#rrggbb"`, in place of the system's
    /// near-black (`#242424`); derived the same way as `ink_dark`. The panel
    /// follows the system theme (Settings > Personalization > Colors,
    /// "Choose your default Windows mode"), so each theme has its own ink.
    /// Default: none.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub ink_light: Option<String>,
    /// The size of the panel's hover tooltip (text, padding and corners
    /// together), as a multiple of the shell's own tooltip (12 px text at
    /// 100% display scaling). Whatever the value, the tooltip is never drawn
    /// below 85% of the 100% size or above three times it. The tray icon's
    /// own tooltip is the system's and keeps the system's size. Range 0.85
    /// to 2.0, default 1.0.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub tooltip_size: Option<f64>,
}

impl TunePanel {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// `digit_size`, clamped.
    pub fn digit_size(&self) -> f32 {
        scale_knob(self.digit_size, 0.5, 1.5)
    }

    /// `bar_length`, clamped.
    pub fn bar_length(&self) -> f32 {
        scale_knob(self.bar_length, 0.25, 3.0)
    }

    /// `bar_thickness`, clamped.
    pub fn bar_thickness(&self) -> f32 {
        scale_knob(self.bar_thickness, 0.25, 2.0)
    }

    /// `row_gap`, clamped.
    pub fn row_gap(&self) -> f32 {
        scale_knob(self.row_gap, 0.0, 2.0)
    }

    /// `tray_gap` in pixels, clamped.
    pub fn tray_gap(&self) -> i32 {
        self.tray_gap.map_or(10, |v| v.clamp(0, 200) as i32)
    }

    /// The ink override for the taskbar's theme, when it is a valid colour.
    pub fn ink(&self, light: bool) -> Option<(u8, u8, u8)> {
        let raw = if light {
            &self.ink_light
        } else {
            &self.ink_dark
        };
        raw.as_deref().and_then(parse_hex_rgb)
    }

    /// `tooltip_size`, clamped.
    pub fn tooltip_size(&self) -> f32 {
        scale_knob(self.tooltip_size, 0.85, 2.0)
    }
}

/// `[tune.tray]`: the tray icon's rings (the Tray indicator, and the icon the
/// panel puts up while it cannot be seen).
///
/// The icon is drawn on a 32 x 32 canvas and Windows scales it to the tray's
/// size: 16 px at 100% display scaling, 20 at 125%, 24 at 150%, 32 at 200%.
/// So the pixels here are canvas pixels: 2 of them are 1 on screen at 100%.
/// The outer ring keeps its place 1 px inside the canvas; the knobs move the
/// inner ring and the hole in the middle. A key left out, or holding the
/// wrong kind of value, reads as its default; a number outside its range
/// reads as the nearest end of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TuneTray {
    /// Each ring's stroke width, in canvas pixels. Range 2.0 to 6.0, default
    /// 5.0. A thicker ring leaves less room for short arcs: under a few
    /// percent the inner ring shows a dot.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub ring_width: Option<f64>,
    /// The bare space between the outer and the inner ring, in canvas
    /// pixels. Range 1.0 to 6.0, default 2.0 (1 px on screen at 100%; below
    /// about 1.5 the two rings start to read as one thick band at 16 px).
    /// The inner ring always keeps a hole in the middle of at least 1 px: a
    /// gap that would close it is cut down to fit (twice the ring width plus
    /// the gap stays at most 14).
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub ring_gap: Option<f64>,
}

impl TuneTray {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// `ring_width` in canvas pixels, clamped.
    pub fn ring_width(&self) -> f32 {
        knob(self.ring_width, 5.0, 2.0, 6.0)
    }

    /// `ring_gap` in canvas pixels, clamped to its range only; the tray
    /// fits it to the icon with the ring width (`ui::tray`).
    pub fn ring_gap(&self) -> f32 {
        knob(self.ring_gap, 2.0, 1.0, 6.0)
    }
}

/// A number knob: unset or not a finite number reads as `default`, the rest
/// clamped to `lo..=hi`.
fn knob(value: Option<f64>, default: f32, lo: f32, hi: f32) -> f32 {
    match value {
        Some(v) if v.is_finite() => (v as f32).clamp(lo, hi),
        _ => default,
    }
}

/// A knob that is a multiple of today's look: 1.0 by default.
fn scale_knob(value: Option<f64>, lo: f32, hi: f32) -> f32 {
    knob(value, 1.0, lo, hi)
}

/// `"#rrggbb"` (either case, spaces around ignored) as its three channels;
/// anything else is None.
pub fn parse_hex_rgb(text: &str) -> Option<(u8, u8, u8)> {
    let hex = text.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some((channel(0)?, channel(2)?, channel(4)?))
}

/// `[tune.format]`: usage thresholds and how times read.
///
/// As `[tune.widget]`: every key optional, a number clamped, a value of the
/// wrong kind ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TuneFormat {
    /// The usage, in percent, from which a row reads as middle (the mid
    /// colour) rather than low: in the widget and in the tray icon's bars.
    /// Default 60, range 0–100.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub mid_at: Option<f32>,
    /// The usage, in percent, from which a row reads as high (the high
    /// colour). Default 80, range 0–100, and never below `mid_at` (a lower
    /// value is raised to it, so the middle level is skipped).
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub high_at: Option<f32>,
    /// How the widget writes a time left (the reset countdown, the
    /// forecast, the age of data that stopped updating, the Compact hint
    /// line): "standard" (default: "2d 5h", "1h 12min", "12min", "45s"),
    /// "compact" ("2d5h", "1h12m", "12m", "45s") or "long" (a day-long time
    /// keeps its minutes: "2d 5h 12min"; shorter ones as standard; where a
    /// long time does not fit, such as an Expanded column, the standard one
    /// shows). The tray's tooltip and the notifications keep the standard
    /// style.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_lenient"
    )]
    pub countdown: Option<Countdown>,
}

/// How a time left is written (`[tune.format] countdown`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Countdown {
    /// "2d 5h", "1h 12min", "12min", "45s": the widget as it always was.
    #[default]
    Standard,
    /// "2d5h", "1h12m", "12m", "45s".
    Compact,
    /// "2d 5h 12min"; under a day as `Standard`.
    Long,
}

impl TuneFormat {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// (`mid_at`, `high_at`), clamped: each 0–100, `high_at` at least
    /// `mid_at`; default (60, 80).
    pub fn thresholds(&self) -> (f32, f32) {
        let mid = clamped(self.mid_at, 60.0, 0.0, 100.0);
        let high = clamped(self.high_at, 80.0, 0.0, 100.0).max(mid);
        (mid, high)
    }

    /// `countdown`, default standard.
    pub fn countdown(&self) -> Countdown {
        self.countdown.unwrap_or_default()
    }
}

/// `value` held to `lo..=hi`; `default` when unset or not a number (NaN, an
/// infinity).
fn clamped(value: Option<f32>, default: f32, lo: f32, hi: f32) -> f32 {
    value
        .filter(|v| v.is_finite())
        .map_or(default, |v| v.clamp(lo, hi))
}

/// A `[tune]` key of the wrong kind (`text_scale = "big"`, `corners =
/// "rounded"`, a word where a number belongs) reads as unset instead of
/// failing the whole config. An integer is taken where a decimal is asked
/// for.
fn de_lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    let raw = toml::Value::deserialize(deserializer)?;
    Ok(T::deserialize(raw).ok())
}

/// A table of strings; an entry that is not a string is dropped, and a
/// value that is not a table reads as empty.
fn de_string_map<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = toml::Value::deserialize(deserializer)?;
    Ok(match raw {
        toml::Value::Table(table) => table
            .into_iter()
            .filter_map(|(k, v)| match v {
                toml::Value::String(s) => Some((k, s)),
                _ => None,
            })
            .collect(),
        _ => BTreeMap::new(),
    })
}

/// One `[tune]` key as config.toml explains it (`config::annotate`): a key
/// not set is written commented out, `# key = shown`, with `note` after it,
/// so taking the `# ` away is all it takes to use it. Tests hold the names
/// to the struct fields, `shown` to the defaults and `range` to the clamps
/// of the accessors above.
#[derive(Debug, Clone, Copy)]
pub struct KeyDoc {
    /// The table: "widget", "panel", "tray" or "format".
    pub table: &'static str,
    pub key: &'static str,
    /// The value shown, as TOML: the default, written as the app writes it,
    /// or an example where the default is not a value (the palette's
    /// colour, no colour).
    pub shown: &'static str,
    /// Whether `shown` is the default (false: an example; `note` then says
    /// what the default is).
    pub default: bool,
    /// The lowest and the highest value that count, as TOML, for a number.
    pub range: Option<(&'static str, &'static str)>,
    /// What follows the value: its range or its words, and what it changes.
    pub note: &'static str,
}

const fn doc(
    table: &'static str,
    key: &'static str,
    shown: &'static str,
    range: Option<(&'static str, &'static str)>,
    note: &'static str,
) -> KeyDoc {
    KeyDoc {
        table,
        key,
        shown,
        default: true,
        range,
        note,
    }
}

const fn example(
    table: &'static str,
    key: &'static str,
    shown: &'static str,
    note: &'static str,
) -> KeyDoc {
    KeyDoc {
        table,
        key,
        shown,
        default: false,
        range: None,
        note,
    }
}

/// Every `[tune]` key, table by table, in the order of the struct fields.
pub const KEY_DOCS: &[KeyDoc] = &[
    doc(
        "widget",
        "text_scale",
        "1.0",
        Some(("0.8", "1.5")),
        "0.8 to 1.5: size of all text (factor)",
    ),
    doc(
        "widget",
        "row_gap",
        "0.0",
        Some(("0.0", "24.0")),
        "0 to 24 px: extra space between rows or columns",
    ),
    doc(
        "widget",
        "bar_scale",
        "1.0",
        Some(("0.5", "3.0")),
        "0.5 to 3.0: bar thickness (factor)",
    ),
    doc(
        "widget",
        "padding_scale",
        "1.0",
        Some(("0.0", "3.0")),
        "0.0 to 3.0: space inside the edge (factor)",
    ),
    doc(
        "widget",
        "corners",
        "\"round\"",
        None,
        "\"round\", \"small\" or \"square\"",
    ),
    example(
        "widget",
        "low",
        "\"#4caf50\"",
        "colour under mid_at; default: the palette's",
    ),
    example(
        "widget",
        "mid",
        "\"#ffb300\"",
        "colour from mid_at; default: the palette's",
    ),
    example(
        "widget",
        "high",
        "\"#e53935\"",
        "colour from high_at; default: the palette's",
    ),
    example(
        "widget",
        "provider_colors",
        "{ claude = \"#d97757\" }",
        "name colour per id; default: grey",
    ),
    doc(
        "panel",
        "digit_size",
        "1.0",
        Some(("0.5", "1.5")),
        "0.5 to 1.5: digit height (factor)",
    ),
    doc(
        "panel",
        "bar_length",
        "1.0",
        Some(("0.25", "3.0")),
        "0.25 to 3.0: bar length (factor)",
    ),
    doc(
        "panel",
        "bar_thickness",
        "1.0",
        Some(("0.25", "2.0")),
        "0.25 to 2.0: bar thickness (factor)",
    ),
    doc(
        "panel",
        "row_gap",
        "1.0",
        Some(("0.0", "2.0")),
        "0.0 to 2.0: space between the rows (factor)",
    ),
    doc(
        "panel",
        "tray_gap",
        "10",
        Some(("0", "200")),
        "0 to 200 whole px: space before the tray",
    ),
    doc(
        "panel",
        "ink_dark",
        "\"#ececec\"",
        None,
        "\"#rrggbb\": digits and bar, dark taskbar",
    ),
    doc(
        "panel",
        "ink_light",
        "\"#242424\"",
        None,
        "\"#rrggbb\": digits and bar, light taskbar",
    ),
    doc(
        "panel",
        "tooltip_size",
        "1.0",
        Some(("0.85", "2.0")),
        "0.85 to 2.0: its tooltip's size (factor)",
    ),
    doc(
        "tray",
        "ring_width",
        "5.0",
        Some(("2.0", "6.0")),
        "2.0 to 6.0 px: each ring's thickness",
    ),
    doc(
        "tray",
        "ring_gap",
        "2.0",
        Some(("1.0", "6.0")),
        "1.0 to 6.0 px: space between the rings",
    ),
    doc(
        "format",
        "mid_at",
        "60.0",
        Some(("0.0", "100.0")),
        "0 to 100 %: the middle colour from here",
    ),
    doc(
        "format",
        "high_at",
        "80.0",
        Some(("0.0", "100.0")),
        "0 to 100 %, not below mid_at: high colour",
    ),
    doc(
        "format",
        "countdown",
        "\"standard\"",
        None,
        "\"standard\", \"compact\" (2d5h) or \"long\"",
    ),
];

/// `[tune.widget]` and `[tune.format]`: parsing, clamping, defaults.
#[cfg(test)]
mod widget_format_tests {
    use super::*;

    fn tune(text: &str) -> Tune {
        toml::from_str(text).expect("a [tune] never fails to parse")
    }

    #[test]
    fn an_empty_table_reads_as_today() {
        let t = tune("[widget]\n[format]\n");
        assert!(t.is_default());
        let (w, f) = (&t.widget, &t.format);
        assert_eq!(
            (
                w.text_scale(),
                w.row_gap(),
                w.bar_scale(),
                w.padding_scale()
            ),
            (1.0, 0.0, 1.0, 1.0)
        );
        assert_eq!(w.corners(), Corners::Round);
        assert_eq!(f.thresholds(), (60.0, 80.0));
        assert_eq!(f.countdown(), Countdown::Standard);
    }

    #[test]
    fn every_key_parses() {
        let t = tune(
            r##"
            [widget]
            text_scale = 1.2
            row_gap = 4
            bar_scale = 2.0
            padding_scale = 0.5
            corners = "small"
            low = "#00ff00"
            mid = "#FFAA00"
            high = "#ff0000"
            provider_colors = { claude = "#d97757", codex = "#10a37f" }
            [format]
            mid_at = 50
            high_at = 90.5
            countdown = "compact"
            "##,
        );
        let (w, f) = (&t.widget, &t.format);
        assert_eq!(w.text_scale(), 1.2);
        assert_eq!(w.row_gap(), 4.0);
        assert_eq!(w.bar_scale(), 2.0);
        assert_eq!(w.padding_scale(), 0.5);
        assert_eq!(w.corners(), Corners::Small);
        assert_eq!(w.low.as_deref(), Some("#00ff00"));
        assert_eq!(w.mid.as_deref(), Some("#FFAA00"));
        assert_eq!(w.high.as_deref(), Some("#ff0000"));
        assert_eq!(w.provider_colors.len(), 2);
        assert_eq!(w.provider_colors["claude"], "#d97757");
        assert_eq!(f.thresholds(), (50.0, 90.5));
        assert_eq!(f.countdown(), Countdown::Compact);
    }

    #[test]
    fn out_of_range_values_are_clamped_where_read() {
        let t = tune(
            "[widget]\ntext_scale = 9\nrow_gap = -3\nbar_scale = 0.1\npadding_scale = 7\n\
             [format]\nmid_at = -5\nhigh_at = 400\n",
        );
        let (w, f) = (&t.widget, &t.format);
        assert_eq!(w.text_scale(), 1.5);
        assert_eq!(w.row_gap(), 0.0);
        assert_eq!(w.bar_scale(), 0.5);
        assert_eq!(w.padding_scale(), 3.0);
        assert_eq!(f.thresholds(), (0.0, 100.0));
        // Kept as written: clamping happens where it is read.
        assert_eq!(w.text_scale, Some(9.0));
        let low = tune("[widget]\ntext_scale = 0.1\n");
        assert_eq!(low.widget.text_scale(), 0.8);
        let nan = tune("[widget]\ntext_scale = nan\nrow_gap = inf\n");
        assert_eq!((nan.widget.text_scale(), nan.widget.row_gap()), (1.0, 0.0));
    }

    #[test]
    fn a_high_threshold_below_the_mid_one_is_raised_to_it() {
        let t = tune("[format]\nmid_at = 70\nhigh_at = 40\n");
        assert_eq!(t.format.thresholds(), (70.0, 70.0));
        let only_mid = tune("[format]\nmid_at = 85\n");
        assert_eq!(only_mid.format.thresholds(), (85.0, 85.0));
    }

    #[test]
    fn a_value_of_the_wrong_kind_reads_as_unset() {
        let t = tune(
            r##"
            [widget]
            text_scale = "big"
            row_gap = true
            corners = "rounded"
            low = 12
            provider_colors = { claude = 5, codex = "#10a37f" }
            [format]
            mid_at = "sixty"
            countdown = "verbose"
            "##,
        );
        assert_eq!(t.widget.text_scale, None);
        assert_eq!(t.widget.row_gap, None);
        assert_eq!(t.widget.corners(), Corners::Round);
        assert_eq!(t.widget.low, None);
        assert_eq!(t.widget.provider_colors.len(), 1);
        assert_eq!(t.format.thresholds(), (60.0, 80.0));
        assert_eq!(t.format.countdown(), Countdown::Standard);
        let not_a_table = tune("[widget]\nprovider_colors = \"#ffffff\"\n");
        assert!(not_a_table.widget.provider_colors.is_empty());
        assert!(not_a_table.is_default());
    }

    #[test]
    fn only_the_keys_set_are_written_back() {
        let mut config = crate::config::schema::Config::default();
        config.tune.widget.bar_scale = Some(2.0);
        config.tune.format.countdown = Some(Countdown::Long);
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("[tune.widget]\nbar_scale = 2.0\n"), "{text}");
        assert!(
            text.contains("[tune.format]\ncountdown = \"long\"\n"),
            "{text}"
        );
        assert!(!text.contains("text_scale") && !text.contains("provider_colors"));
        let back: crate::config::schema::Config = toml::from_str(&text).unwrap();
        assert_eq!(back.tune, config.tune);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A default `[tune]` is never written, and an empty or unknown one
    /// reads as the default.
    #[test]
    fn the_default_is_not_written() {
        let config = crate::config::schema::Config::default();
        assert!(!toml::to_string(&config).unwrap().contains("[tune"));
        let t: Tune = toml::from_str("[widget]\nunknown = 1\n").unwrap();
        assert!(t.is_default());
    }

    /// The panel's and the tray's knobs left out read as today's look.
    #[test]
    fn panel_and_tray_defaults_are_todays_look() {
        let p = TunePanel::default();
        assert!(p.is_default());
        assert_eq!(
            (
                p.digit_size(),
                p.bar_length(),
                p.bar_thickness(),
                p.row_gap()
            ),
            (1.0, 1.0, 1.0, 1.0)
        );
        assert_eq!(p.tray_gap(), 10);
        assert_eq!((p.ink(false), p.ink(true)), (None, None));
        assert_eq!(p.tooltip_size(), 1.0);
        let t = TuneTray::default();
        assert!(t.is_default());
        assert_eq!((t.ring_width(), t.ring_gap()), (5.0, 2.0));
        // An empty table, or one with only unknown keys, is the default too.
        let tune: Tune = toml::from_str("[panel]\n[tray]\nsize = 3\n").unwrap();
        assert!(tune.is_default());
    }

    /// Every knob is read from the file; an integer stands for a decimal.
    #[test]
    fn panel_and_tray_knobs_are_read() {
        let text = r##"
[panel]
digit_size = 1.2
bar_length = 2
bar_thickness = 0.5
row_gap = 0.0
tray_gap = 24
ink_dark = "#7FD4ff"
ink_light = "#0b5394"
tooltip_size = 1.25

[tray]
ring_width = 4
ring_gap = 3.5
"##;
        let tune: Tune = toml::from_str(text).unwrap();
        let p = &tune.panel;
        assert_eq!(
            (
                p.digit_size(),
                p.bar_length(),
                p.bar_thickness(),
                p.row_gap()
            ),
            (1.2, 2.0, 0.5, 0.0)
        );
        assert_eq!(p.tray_gap(), 24);
        assert_eq!(p.ink(false), Some((0x7f, 0xd4, 0xff)));
        assert_eq!(p.ink(true), Some((0x0b, 0x53, 0x94)));
        assert_eq!(p.tooltip_size(), 1.25);
        assert_eq!((tune.tray.ring_width(), tune.tray.ring_gap()), (4.0, 3.5));
        assert!(tune.widget.is_default() && tune.format.is_default());
    }

    /// Out of range reads as the nearest end; not a finite number reads as
    /// the default.
    #[test]
    fn out_of_range_knobs_are_clamped_where_read() {
        let low = TunePanel {
            digit_size: Some(0.1),
            bar_length: Some(-4.0),
            bar_thickness: Some(0.0),
            row_gap: Some(-1.0),
            tray_gap: Some(-30),
            tooltip_size: Some(0.2),
            ..Default::default()
        };
        assert_eq!(
            (
                low.digit_size(),
                low.bar_length(),
                low.bar_thickness(),
                low.row_gap(),
                low.tray_gap(),
                low.tooltip_size()
            ),
            (0.5, 0.25, 0.25, 0.0, 0, 0.85)
        );
        let high = TunePanel {
            digit_size: Some(9.0),
            bar_length: Some(40.0),
            bar_thickness: Some(5.0),
            row_gap: Some(7.5),
            tray_gap: Some(i64::MAX),
            tooltip_size: Some(10.0),
            ..Default::default()
        };
        assert_eq!(
            (
                high.digit_size(),
                high.bar_length(),
                high.bar_thickness(),
                high.row_gap(),
                high.tray_gap(),
                high.tooltip_size()
            ),
            (1.5, 3.0, 2.0, 2.0, 200, 2.0)
        );
        let odd: Tune = toml::from_str(
            "[panel]\ndigit_size = nan\nbar_length = inf\n[tray]\nring_width = -inf\n",
        )
        .unwrap();
        assert_eq!((odd.panel.digit_size(), odd.panel.bar_length()), (1.0, 1.0));
        assert_eq!(odd.tray.ring_width(), 5.0);
        let tray = TuneTray {
            ring_width: Some(0.5),
            ring_gap: Some(99.0),
        };
        assert_eq!((tray.ring_width(), tray.ring_gap()), (2.0, 6.0));
    }

    /// A value of the wrong kind reads as unset, and the rest of the table
    /// (and of the config) still reads: a typo costs that one knob.
    #[test]
    fn a_wrong_kind_of_value_costs_only_that_knob() {
        let text = r##"
[panel]
digit_size = "big"
bar_length = 1.5
tray_gap = 12.5
ink_dark = 0xffffff
ink_light = ["#000000"]
row_gap = true
[tray]
ring_width = { px = 4 }
ring_gap = 3
"##;
        let tune: Tune = toml::from_str(text).unwrap();
        let p = &tune.panel;
        assert_eq!(p.digit_size, None);
        assert_eq!(p.bar_length(), 1.5);
        assert_eq!(p.tray_gap, None);
        assert_eq!(p.tray_gap(), 10);
        assert_eq!(
            (p.ink_dark.as_deref(), p.ink_light.as_deref()),
            (None, None)
        );
        assert_eq!(p.row_gap, None);
        assert_eq!(tune.tray.ring_width, None);
        assert_eq!(tune.tray.ring_gap(), 3.0);
        // Through the whole config, next to what it already holds.
        let config: crate::config::schema::Config =
            toml::from_str("[tune.panel]\ndigit_size = \"big\"\nrow_gap = 0.5\n").unwrap();
        assert_eq!(config.tune.panel.row_gap(), 0.5);
    }

    /// Only `#` and six hex digits make a colour; anything else is ignored.
    #[test]
    fn colours_are_hash_and_six_hex_digits() {
        assert_eq!(parse_hex_rgb("#000000"), Some((0, 0, 0)));
        assert_eq!(parse_hex_rgb(" #FfA0c3 "), Some((0xff, 0xa0, 0xc3)));
        for bad in [
            "", "#", "ffffff", "#fff", "#fffffff", "#gggggg", "red", "#12 456", "##123456",
            "#+12345",
        ] {
            assert_eq!(parse_hex_rgb(bad), None, "{bad:?}");
        }
        let p = TunePanel {
            ink_dark: Some("white".into()),
            ink_light: Some("#123456".into()),
            ..Default::default()
        };
        assert_eq!(
            (p.ink(false), p.ink(true)),
            (None, Some((0x12, 0x34, 0x56)))
        );
    }

    /// `KEY_DOCS` names every field of the four tables, in their order, and
    /// nothing else: a field added without its line in config.toml, or a
    /// line for a field that is gone, fails here.
    #[test]
    fn the_key_docs_name_every_field() {
        use crate::config::annotate::field_names;
        for (table, fields) in [
            ("widget", field_names::<TuneWidget>()),
            ("panel", field_names::<TunePanel>()),
            ("tray", field_names::<TuneTray>()),
            ("format", field_names::<TuneFormat>()),
        ] {
            let documented: Vec<&str> = KEY_DOCS
                .iter()
                .filter(|d| d.table == table)
                .map(|d| d.key)
                .collect();
            assert_eq!(documented, fields, "[tune.{table}]");
        }
        let tables = field_names::<Tune>();
        assert!(KEY_DOCS.iter().all(|d| tables.contains(&d.table)));
    }

    /// Everything the accessors give, the inks as drawn: two tunes that
    /// look the same give the same text.
    fn resolved(t: &Tune) -> String {
        let ink = |light: bool| {
            t.panel.ink(light).unwrap_or_else(|| {
                let (c, _) = crate::ui::theme::panel_ink(light);
                (c.r, c.g, c.b)
            })
        };
        let (w, p, r) = (&t.widget, &t.panel, &t.tray);
        format!(
            "{:?}",
            (
                (
                    w.text_scale(),
                    w.row_gap(),
                    w.bar_scale(),
                    w.padding_scale()
                ),
                w.corners(),
                (
                    p.digit_size(),
                    p.bar_length(),
                    p.bar_thickness(),
                    p.row_gap()
                ),
                (p.tray_gap(), ink(false), ink(true), p.tooltip_size()),
                (r.ring_width(), r.ring_gap()),
                (t.format.thresholds(), t.format.countdown()),
            )
        )
    }

    /// `key = value` (with `also` before it) under its table.
    fn with(d: &KeyDoc, also: &str, value: &str) -> Tune {
        let text = format!("[{}]\n{also}{} = {value}\n", d.table, d.key);
        toml::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"))
    }

    /// The value config.toml shows for a key not set is its real default:
    /// written in, it changes nothing that shows. An example (a colour
    /// where the default is the palette's) at least reads as a value.
    #[test]
    fn the_values_shown_are_the_defaults() {
        let today = resolved(&Tune::default());
        for d in KEY_DOCS {
            let set = with(d, "", d.shown);
            assert!(!set.is_default(), "{}: {} does not read", d.key, d.shown);
            if d.default {
                assert_eq!(resolved(&set), today, "{} = {}", d.key, d.shown);
            } else {
                // Every colour of an example is one that counts.
                let colours: Vec<&str> = d.shown.split('"').skip(1).step_by(2).collect();
                assert!(!colours.is_empty(), "{}", d.key);
                for c in colours {
                    assert!(parse_hex_rgb(c).is_some(), "{}: {c}", d.key);
                }
                assert!(d.note.contains("default:"), "{}: {}", d.key, d.note);
            }
        }
    }

    /// The ranges config.toml gives are the clamps: past either end reads
    /// as that end, and just inside it reads as itself.
    #[test]
    fn the_ranges_shown_are_the_clamps() {
        for d in KEY_DOCS {
            let Some((lo, hi)) = d.range else {
                continue;
            };
            // The note says the same range, ".0" or not.
            let spell = |x: &str| [x.to_string(), x.trim_end_matches(".0").to_string()];
            let said = spell(lo).iter().any(|a| {
                spell(hi)
                    .iter()
                    .any(|b| d.note.starts_with(&format!("{a} to {b}")))
            });
            assert!(said, "{}: {}", d.key, d.note);
            // high_at never reads below mid_at: put mid_at out of the way.
            let also = if d.key == "high_at" {
                "mid_at = 0\n"
            } else {
                ""
            };
            let whole = !lo.contains('.');
            let (l, h): (f64, f64) = (lo.parse().unwrap(), hi.parse().unwrap());
            let text = |v: f64| {
                if whole {
                    format!("{}", v as i64)
                } else {
                    format!("{v:?}")
                }
            };
            let at = |v: f64| resolved(&with(d, also, &text(v)));
            let step = if whole { 1.0 } else { (h - l) / 100.0 };
            assert_eq!(at(l - 1.0), at(l), "{} below {lo}", d.key);
            assert_eq!(at(h + 1.0), at(h), "{} above {hi}", d.key);
            assert_ne!(at(l + step), at(l), "{} just above {lo}", d.key);
            assert_ne!(at(h - step), at(h), "{} just below {hi}", d.key);
        }
    }

    /// Only the knobs set are written back, as they were set (an invalid
    /// colour included, so nothing of the user's is lost); none set writes
    /// no table.
    #[test]
    fn only_the_knobs_set_are_written() {
        let mut config = crate::config::schema::Config::default();
        config.tune.panel.digit_size = Some(1.2);
        config.tune.panel.ink_dark = Some("teal".into());
        config.tune.tray.ring_gap = Some(3.0);
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("[tune.panel]"), "{text}");
        assert!(text.contains("digit_size = 1.2"), "{text}");
        assert!(text.contains("ink_dark = \"teal\""), "{text}");
        assert!(!text.contains("bar_length"), "{text}");
        assert!(
            text.contains("[tune.tray]") && text.contains("ring_gap = 3.0"),
            "{text}"
        );
        assert!(
            !text.contains("ring_width") && !text.contains("[tune.widget]"),
            "{text}"
        );
        let back: crate::config::schema::Config = toml::from_str(&text).unwrap();
        assert_eq!(back.tune, config.tune);
    }
}
