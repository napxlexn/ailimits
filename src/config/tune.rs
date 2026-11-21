// config/tune.rs — `[tune]`: fine-tuning that lives only in config.toml.
//
// Knobs too fine for the context menu or Rows and models: sizes, spacing,
// colours, thresholds and formats of the widget, the taskbar panel and the
// tray icon. Every field is optional; its default is exactly the look the
// app has without it (the pinned visual baselines hold that), and a table
// equal to its default is never written. Values out of range are clamped
// where they are read, never rejected: a typo cannot break the config.
// The user's guide is docs/en/TUNING.md (and docs/uk/TUNING.md).

use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TuneWidget {}

impl TuneWidget {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
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
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "de_knob")]
    pub digit_size: Option<f64>,
    /// The usage bar's length, as a multiple of today's (about half the
    /// panel's width on a horizontal taskbar). Lying down the panel grows or
    /// shrinks with the bar; standing up (a taskbar on the left or right) the
    /// panel is as wide as the taskbar, so the bar stays centred and at most
    /// that wide (less a pixel at each end). Range 0.25 to 3.0, default 1.0.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "de_knob")]
    pub bar_length: Option<f64>,
    /// The usage bar's thickness, as a multiple of today's: half a digit's
    /// height. Its ends stay round. Range 0.25 to 2.0, default 1.0.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "de_knob")]
    pub bar_thickness: Option<f64>,
    /// The space between the two providers' rows, as a multiple of today's:
    /// 0.4 of a digit's height lying down, 0.6 standing up. 0 puts the rows
    /// edge to edge. Range 0.0 to 2.0, default 1.0.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "de_knob")]
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
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "de_knob")]
    pub tray_gap: Option<i64>,
    /// The ink on a DARK taskbar, `"#rrggbb"`, in place of the system's
    /// near-white (`#ececec`). The bar's groove and the dimmer ink of a stale
    /// or estimated row are derived from it as they are from the default
    /// (the same colour at the groove's and the halfway transparency).
    /// Anything but `#` and six hex digits is ignored. Default: none.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "de_knob")]
    pub ink_dark: Option<String>,
    /// The ink on a LIGHT taskbar, `"#rrggbb"`, in place of the system's
    /// near-black (`#242424`); derived the same way as `ink_dark`. The panel
    /// follows the system theme (Settings > Personalization > Colors,
    /// "Choose your default Windows mode"), so each theme has its own ink.
    /// Default: none.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "de_knob")]
    pub ink_light: Option<String>,
    /// The size of the panel's hover tooltip (text, padding and corners
    /// together), as a multiple of the shell's own tooltip (12 px text at
    /// 100% display scaling). Whatever the value, the tooltip is never drawn
    /// below 85% of the 100% size or above three times it. The tray icon's
    /// own tooltip is the system's and keeps the system's size. Range 0.85
    /// to 2.0, default 1.0.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "de_knob")]
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
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "de_knob")]
    pub ring_width: Option<f64>,
    /// The bare space between the outer and the inner ring, in canvas
    /// pixels. Range 1.0 to 6.0, default 2.0 (1 px on screen at 100%; below
    /// about 1.5 the two rings start to read as one thick band at 16 px).
    /// The inner ring always keeps a hole in the middle of at least 1 px: a
    /// gap that would close it is cut down to fit (twice the ring width plus
    /// the gap stays at most 14).
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "de_knob")]
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

/// A `[tune]` value that reads as unset when it holds the wrong kind of
/// value (a word where a number belongs), so a typo cannot cost the whole
/// config. An integer is taken where a decimal is asked for.
fn de_knob<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    let raw = toml::Value::deserialize(deserializer)?;
    Ok(T::deserialize(raw).ok())
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TuneFormat {}

impl TuneFormat {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
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
