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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TunePanel {}

impl TunePanel {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// `[tune.tray]`: the tray icon's rings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TuneTray {}

impl TuneTray {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
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
}
