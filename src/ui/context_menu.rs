// ui/context_menu.rs — the right-click context menu.
//
// A NATIVE menu via muda — no custom popup window. Provider authorization
// lives in the "Providers" submenu: keys and tokens are pasted FROM THE
// CLIPBOARD (a native menu has no text input).

use crate::config::schema::{
    AuthMethod, ColumnFlow, Config, DetailLevel, IndicatorKind, Layout, Palette, PanelDisplay,
    WidthScale,
};
use anyhow::Result;
use muda::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

/// Background opacity steps, %.
const OPACITY_STEPS: &[u8] = &[15, 25, 35, 50, 70, 85];
/// Text and bar brightness steps, %.
const BRIGHTNESS_STEPS: &[u8] = &[40, 60, 80, 100];
/// Palette saturation steps, %.
const SATURATION_STEPS: &[u8] = &[0, 25, 50, 75, 100];
/// Update interval steps, seconds: 1 / 5 / 15 / 30 min.
const INTERVAL_STEPS: &[u64] = &[60, 300, 900, 1800];

/// Menu actions.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuAction {
    SetDetail(DetailLevel),
    SetLayout(Layout),
    /// Pick the widget width step (horizontal bars only).
    SetWidthScale(WidthScale),
    /// Arrange the provider columns in a row or in a column (vertical bars only).
    SetColumnFlow(ColumnFlow),
    /// Lock the widget position (disables dragging).
    ToggleLock,
    /// Always on top.
    TogglePin,
    /// Pick the secondary indicator: tray icon / taskbar bar / off.
    SetIndicator(IndicatorKind),
    /// Move the panel indicator to another taskbar.
    SetPanelDisplay(PanelDisplay),
    /// Show/hide the burn-rate forecast.
    ToggleForecast,
    /// Enable/disable silent background auto-updates.
    ToggleAutoUpdate,
    /// Pick a color palette — disables monochrome.
    SetPalette(Palette),
    ToggleMonochrome,
    /// Background opacity, %.
    SetOpacity(u8),
    /// Text and bar brightness, %.
    SetBrightness(u8),
    /// Palette saturation, %.
    SetSaturation(u8),
    /// Update interval, seconds.
    SetUpdateInterval(u64),
    /// Enable/disable a provider.
    ToggleProvider(String),
    /// Provider auth method.
    SetAuthMethod(String, AuthMethod),
    /// Paste an API key / PAT from the clipboard.
    PasteKey(String),
    /// Remove the API key / PAT.
    RemoveKey(String),
    /// Paste a manual usage token from the clipboard (claude, codex).
    PasteUsageToken(String),
    /// Remove the manual usage token.
    RemoveUsageToken(String),
    Quit,
}

/// The step closest to a value.
fn closest_step(steps: &[u8], value: u8) -> u8 {
    steps
        .iter()
        .copied()
        .min_by_key(|s| (*s as i16 - value as i16).abs())
        .unwrap_or(value)
}

/// The interval step closest to a value.
fn closest_interval(value: u64) -> u64 {
    INTERVAL_STEPS
        .iter()
        .copied()
        .min_by_key(|s| s.abs_diff(value))
        .unwrap_or(value)
}

/// Whether a config indicator value selects the given menu item. PanelGrid
/// is a config-only alias of PanelRows (the overlay renders them the same),
/// so either checks the single "Taskbar panel" item.
fn indicator_matches(config_kind: IndicatorKind, item_kind: IndicatorKind) -> bool {
    let canon = |k: IndicatorKind| match k {
        IndicatorKind::PanelGrid => IndicatorKind::PanelRows,
        other => other,
    };
    canon(config_kind) == canon(item_kind)
}

/// The full context menu: view, behavior, appearance, providers, actions.
pub struct ContextMenu {
    pub menu: Menu,
    detail_items: Vec<(CheckMenuItem, DetailLevel)>,
    layout_items: Vec<(CheckMenuItem, Layout)>,
    width_items: Vec<(CheckMenuItem, WidthScale)>,
    flow_items: Vec<(CheckMenuItem, ColumnFlow)>,
    lock_item: CheckMenuItem,
    pin_item: CheckMenuItem,
    indicator_items: Vec<(CheckMenuItem, IndicatorKind)>,
    /// Which taskbar the mini panel attaches to. Empty on a single-display
    /// machine — see the "Only worth showing..." comment where it's built.
    display_items: Vec<(CheckMenuItem, PanelDisplay)>,
    forecast_item: CheckMenuItem,
    auto_update_item: CheckMenuItem,
    monochrome_item: CheckMenuItem,
    palette_items: Vec<(CheckMenuItem, Palette)>,
    opacity_items: Vec<(CheckMenuItem, u8)>,
    brightness_items: Vec<(CheckMenuItem, u8)>,
    saturation_items: Vec<(CheckMenuItem, u8)>,
    interval_items: Vec<(CheckMenuItem, u64)>,
    /// Per-provider enable toggles.
    provider_toggles: Vec<(CheckMenuItem, String)>,
    /// Auth method choice: only Claude has options.
    auth_method_items: Vec<(CheckMenuItem, String, AuthMethod)>,
    paste_key_items: Vec<(MenuItem, String)>,
    remove_key_items: Vec<(MenuItem, String)>,
    paste_usage_items: Vec<(MenuItem, String)>,
    remove_usage_items: Vec<(MenuItem, String)>,
    quit_item: MenuItem,
}

impl ContextMenu {
    pub fn new(config: &Config, pinned: bool) -> Result<Self> {
        let ui = &config.ui;
        let menu = Menu::new();

        // Detail levels.
        let detail_items: Vec<(CheckMenuItem, DetailLevel)> = [
            ("Compact", DetailLevel::Compact),
            ("Medium", DetailLevel::Medium),
            ("Expanded", DetailLevel::Expanded),
        ]
        .into_iter()
        .map(|(label, d)| (CheckMenuItem::new(label, true, ui.detail == d, None), d))
        .collect();

        // Layouts. The label names the PROGRESS-BAR orientation the user sees,
        // which is the opposite axis of the provider stacking: Layout::Vertical
        // stacks providers in rows with horizontal bars, while Layout::Horizontal
        // lays them out in columns with vertical bars. (Enum values are unchanged
        // so existing configs keep their orientation — only the labels are fixed.)
        let layout_items: Vec<(CheckMenuItem, Layout)> = [
            ("Vertical", Layout::Horizontal),
            ("Horizontal", Layout::Vertical),
        ]
        .into_iter()
        .map(|(label, l)| (CheckMenuItem::new(label, true, ui.layout == l, None), l))
        .collect();

        // Width steps. Only the rows layout can be narrowed: the columns
        // layouts size themselves from the provider count, so their items are
        // greyed rather than hidden - a missing menu entry reads as a bug.
        let width_submenu = Submenu::new("Width", true);
        let width_items: Vec<(CheckMenuItem, WidthScale)> = [
            ("100%", WidthScale::Full),
            ("75%", WidthScale::ThreeQuarters),
            ("50%", WidthScale::Half),
        ]
        .into_iter()
        .map(|(label, w)| {
            (
                CheckMenuItem::new(label, true, ui.width_scale == w, None),
                w,
            )
        })
        .collect();
        for (item, _) in &width_items {
            width_submenu.append(item)?;
        }

        // Column arrangement. The mirror image of Width: it only means
        // something when the bars are vertical, and it flips the widget's own
        // orientation rather than its size.
        let flow_submenu = Submenu::new("Arrangement", true);
        let flow_items: Vec<(CheckMenuItem, ColumnFlow)> = [
            ("In a row", ColumnFlow::Row),
            ("In a column", ColumnFlow::Column),
        ]
        .into_iter()
        .map(|(label, f)| {
            (
                CheckMenuItem::new(label, true, ui.column_flow == f, None),
                f,
            )
        })
        .collect();
        for (item, _) in &flow_items {
            flow_submenu.append(item)?;
        }

        // Two INDEPENDENT options: position lock and always-on-top.
        let lock_item = CheckMenuItem::new("Lock position", true, config.window.locked, None);
        let pin_item = CheckMenuItem::new("Always on top", true, pinned, None);
        // "Indicator" submenu — radio-style choice of the secondary indicator.
        let indicator_submenu = Submenu::new("Indicator", true);
        // The legacy 16px "bars" tray icon stays config-only (too small to
        // read). A single "Taskbar panel" entry: the transparent overlay
        // renders identically for panel_rows and panel_grid (the grid layout
        // was superseded by the top-2 design), so panel_grid in an old config
        // is just an alias that checks this same item.
        let indicator_items: Vec<(CheckMenuItem, IndicatorKind)> = [
            ("Tray pie icon", IndicatorKind::Tray),
            ("Taskbar panel", IndicatorKind::PanelRows),
            ("Off", IndicatorKind::Off),
        ]
        .into_iter()
        .map(|(label, k)| {
            (
                CheckMenuItem::new(
                    label,
                    true,
                    indicator_matches(config.general.indicator, k),
                    None,
                ),
                k,
            )
        })
        .collect();
        for (item, _) in &indicator_items {
            indicator_submenu.append(item)?;
        }

        // "Display" submenu — which taskbar the mini panel attaches to.
        // Only worth showing when there IS another taskbar: a one-entry
        // "Display" submenu on a single-monitor machine is pure noise.
        let ui_display = config.general.panel_display;
        let bars = crate::platform::secondary_taskbars();
        let mut display_items: Vec<(CheckMenuItem, PanelDisplay)> = Vec::new();
        if !bars.is_empty() {
            let display_submenu = Submenu::new("Display", true);
            let mut entries = vec![("Primary".to_string(), PanelDisplay::Primary)];
            for i in 0..bars.len() {
                entries.push((
                    format!("Display {}", i + 2),
                    PanelDisplay::Secondary(i as u8),
                ));
            }
            for (label, target) in entries {
                let item = CheckMenuItem::new(label, true, ui_display == target, None);
                display_submenu.append(&item)?;
                display_items.push((item, target));
            }
            indicator_submenu.append(&display_submenu)?;
        }

        let forecast_item = CheckMenuItem::new("Forecast", true, config.ui.show_forecast, None);
        let auto_update_item =
            CheckMenuItem::new("Automatic updates", true, config.general.auto_update, None);

        // "Palette" submenu: monochrome + 8 colored palettes.
        let palette_submenu = Submenu::new("Palette", true);
        let monochrome_item = CheckMenuItem::new("Monochrome", true, ui.monochrome, None);
        palette_submenu.append(&monochrome_item)?;
        palette_submenu.append(&PredefinedMenuItem::separator())?;
        let palette_items: Vec<(CheckMenuItem, Palette)> = [
            ("Default", Palette::Default),
            ("Ocean", Palette::Ocean),
            ("Sunset", Palette::Sunset),
            ("Forest", Palette::Forest),
            ("Neon", Palette::Neon),
            ("Ice", Palette::Ice),
            ("Rose", Palette::Rose),
            ("Slate", Palette::Slate),
        ]
        .into_iter()
        .map(|(label, p)| {
            (
                CheckMenuItem::new(label, true, !ui.monochrome && ui.palette == p, None),
                p,
            )
        })
        .collect();
        for (item, _) in &palette_items {
            palette_submenu.append(item)?;
        }

        // Numeric step submenus.
        let step_submenu = |title: &str,
                            steps: &[u8],
                            current: u8|
         -> Result<(Submenu, Vec<(CheckMenuItem, u8)>)> {
            let sub = Submenu::new(title, true);
            let marked = closest_step(steps, current);
            let mut items = Vec::new();
            for &step in steps {
                let item = CheckMenuItem::new(format!("{step}%"), true, step == marked, None);
                sub.append(&item)?;
                items.push((item, step));
            }
            Ok((sub, items))
        };
        let opacity_now = (config.window.opacity * 100.0).round() as u8;
        let (opacity_submenu, opacity_items) =
            step_submenu("Background opacity", OPACITY_STEPS, opacity_now)?;
        let (brightness_submenu, brightness_items) =
            step_submenu("Brightness", BRIGHTNESS_STEPS, ui.brightness)?;
        let (saturation_submenu, saturation_items) =
            step_submenu("Saturation", SATURATION_STEPS, ui.saturation)?;

        // Update interval submenu: 1 / 5 / 15 / 30 min.
        let interval_submenu = Submenu::new("Update interval", true);
        let interval_marked = closest_interval(config.general.update_interval_secs);
        let mut interval_items = Vec::new();
        for &secs in INTERVAL_STEPS {
            let item = CheckMenuItem::new(
                format!("{} min", secs / 60),
                true,
                secs == interval_marked,
                None,
            );
            interval_submenu.append(&item)?;
            interval_items.push((item, secs));
        }

        // "Providers" submenu with authorization.
        let providers_submenu = Submenu::new("Providers", true);
        let mut provider_toggles = Vec::new();
        let mut auth_method_items = Vec::new();
        let mut paste_key_items = Vec::new();
        let mut remove_key_items = Vec::new();
        let mut paste_usage_items = Vec::new();
        let mut remove_usage_items = Vec::new();

        for pc in &config.providers {
            let display = crate::providers::ProviderId::from_config_id(&pc.id)
                .map(|p| p.display_name())
                .unwrap_or("?");
            let sub = Submenu::new(display, true);

            let toggle = CheckMenuItem::new("Enabled", true, pc.enabled, None);
            sub.append(&toggle)?;
            provider_toggles.push((toggle, pc.id.clone()));

            // Method choice — Claude only: subscription or API key.
            // Codex and Copilot are subscription-only.
            if pc.id == "claude" {
                sub.append(&PredefinedMenuItem::separator())?;
                let sub_item = CheckMenuItem::new(
                    "Claude Code subscription",
                    true,
                    pc.auth_method == AuthMethod::Subscription,
                    None,
                );
                let key_item =
                    CheckMenuItem::new("API key", true, pc.auth_method == AuthMethod::ApiKey, None);
                sub.append(&sub_item)?;
                sub.append(&key_item)?;
                auth_method_items.push((sub_item, pc.id.clone(), AuthMethod::Subscription));
                auth_method_items.push((key_item, pc.id.clone(), AuthMethod::ApiKey));
            }

            // Keys — only where they exist: Claude (API key), Copilot (PAT).
            if crate::providers::key_label_for(&pc.id).is_some() {
                sub.append(&PredefinedMenuItem::separator())?;
                let paste_label = if pc.id == "copilot" {
                    // The PAT is optional: without it the gh CLI token is used.
                    "Paste PAT from clipboard (optional)"
                } else {
                    "Paste API key from clipboard"
                };
                let paste = MenuItem::new(paste_label, true, None);
                let remove = MenuItem::new("Remove key", true, None);
                sub.append(&paste)?;
                sub.append(&remove)?;
                paste_key_items.push((paste, pc.id.clone()));
                remove_key_items.push((remove, pc.id.clone()));
            }

            // Manual usage tokens — Claude and Codex: an alternative to the
            // auto-detected CLI tokens.
            if crate::providers::usage_token_label_for(&pc.id).is_some() {
                sub.append(&PredefinedMenuItem::separator())?;
                let paste = MenuItem::new("Paste usage token from clipboard", true, None);
                let remove = MenuItem::new("Remove usage token", true, None);
                sub.append(&paste)?;
                sub.append(&remove)?;
                paste_usage_items.push((paste, pc.id.clone()));
                remove_usage_items.push((remove, pc.id.clone()));
            }

            providers_submenu.append(&sub)?;
        }

        let quit_item = MenuItem::new("Quit", true, None);

        // Order: detail / layout / behavior / appearance / providers / actions.
        for (item, _) in &detail_items {
            menu.append(item)?;
        }
        menu.append(&PredefinedMenuItem::separator())?;
        for (item, _) in &layout_items {
            menu.append(item)?;
        }
        menu.append(&width_submenu)?;
        menu.append(&flow_submenu)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&lock_item)?;
        menu.append(&pin_item)?;
        menu.append(&indicator_submenu)?;
        menu.append(&forecast_item)?;
        menu.append(&auto_update_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&palette_submenu)?;
        menu.append(&opacity_submenu)?;
        menu.append(&brightness_submenu)?;
        menu.append(&saturation_submenu)?;
        menu.append(&interval_submenu)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&providers_submenu)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit_item)?;
        // Version — a disabled info line.
        menu.append(&MenuItem::new(
            format!("AI Limits v{}", env!("CARGO_PKG_VERSION")),
            false,
            None,
        ))?;

        Ok(Self {
            menu,
            detail_items,
            layout_items,
            width_items,
            flow_items,
            lock_item,
            pin_item,
            indicator_items,
            display_items,
            forecast_item,
            auto_update_item,
            monochrome_item,
            palette_items,
            opacity_items,
            brightness_items,
            saturation_items,
            interval_items,
            provider_toggles,
            auth_method_items,
            paste_key_items,
            remove_key_items,
            paste_usage_items,
            remove_usage_items,
            quit_item,
        })
    }

    /// Map a menu event id to an action.
    pub fn action_for(&self, event_id: &muda::MenuId) -> Option<MenuAction> {
        for (item, d) in &self.detail_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetDetail(d.clone()));
            }
        }
        for (item, l) in &self.layout_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetLayout(l.clone()));
            }
        }
        for (item, w) in &self.width_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetWidthScale(*w));
            }
        }
        for (item, f) in &self.flow_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetColumnFlow(*f));
            }
        }
        for (item, p) in &self.palette_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetPalette(p.clone()));
            }
        }
        for (item, v) in &self.opacity_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetOpacity(*v));
            }
        }
        for (item, v) in &self.brightness_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetBrightness(*v));
            }
        }
        for (item, v) in &self.saturation_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetSaturation(*v));
            }
        }
        for (item, v) in &self.interval_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetUpdateInterval(*v));
            }
        }
        if *event_id == self.monochrome_item.id() {
            return Some(MenuAction::ToggleMonochrome);
        }
        if *event_id == self.lock_item.id() {
            return Some(MenuAction::ToggleLock);
        }
        for (item, k) in &self.indicator_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetIndicator(*k));
            }
        }
        for (item, target) in &self.display_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetPanelDisplay(*target));
            }
        }
        if *event_id == self.forecast_item.id() {
            return Some(MenuAction::ToggleForecast);
        }
        if *event_id == self.auto_update_item.id() {
            return Some(MenuAction::ToggleAutoUpdate);
        }
        for (item, id) in &self.provider_toggles {
            if *event_id == item.id() {
                return Some(MenuAction::ToggleProvider(id.clone()));
            }
        }
        for (item, id, method) in &self.auth_method_items {
            if *event_id == item.id() {
                return Some(MenuAction::SetAuthMethod(id.clone(), method.clone()));
            }
        }
        for (item, id) in &self.paste_key_items {
            if *event_id == item.id() {
                return Some(MenuAction::PasteKey(id.clone()));
            }
        }
        for (item, id) in &self.remove_key_items {
            if *event_id == item.id() {
                return Some(MenuAction::RemoveKey(id.clone()));
            }
        }
        for (item, id) in &self.paste_usage_items {
            if *event_id == item.id() {
                return Some(MenuAction::PasteUsageToken(id.clone()));
            }
        }
        for (item, id) in &self.remove_usage_items {
            if *event_id == item.id() {
                return Some(MenuAction::RemoveUsageToken(id.clone()));
            }
        }
        if *event_id == self.pin_item.id() {
            Some(MenuAction::TogglePin)
        } else if *event_id == self.quit_item.id() {
            Some(MenuAction::Quit)
        } else {
            None
        }
    }

    /// Sync the checkmarks with the config.
    pub fn sync(&self, config: &Config, pinned: bool) {
        let ui = &config.ui;
        for (item, d) in &self.detail_items {
            item.set_checked(ui.detail == *d);
        }
        for (item, l) in &self.layout_items {
            item.set_checked(ui.layout == *l);
        }
        // Width and Arrangement are the same choice on opposite axes, so
        // exactly one of them applies at a time. The inapplicable one is
        // greyed rather than hidden - a menu that changes shape reads as a bug.
        let rows_layout = matches!(ui.layout, Layout::Vertical | Layout::Grid);
        for (item, w) in &self.width_items {
            item.set_checked(ui.width_scale == *w);
            item.set_enabled(rows_layout);
        }
        for (item, f) in &self.flow_items {
            item.set_checked(ui.column_flow == *f);
            item.set_enabled(!rows_layout);
        }
        self.monochrome_item.set_checked(ui.monochrome);
        for (item, p) in &self.palette_items {
            item.set_checked(!ui.monochrome && ui.palette == *p);
        }
        let opacity_now = (config.window.opacity * 100.0).round() as u8;
        let opacity_marked = closest_step(OPACITY_STEPS, opacity_now);
        for (item, v) in &self.opacity_items {
            item.set_checked(*v == opacity_marked);
        }
        let bright_marked = closest_step(BRIGHTNESS_STEPS, ui.brightness);
        for (item, v) in &self.brightness_items {
            item.set_checked(*v == bright_marked);
        }
        let sat_marked = closest_step(SATURATION_STEPS, ui.saturation);
        for (item, v) in &self.saturation_items {
            item.set_checked(*v == sat_marked);
        }
        let interval_marked = closest_interval(config.general.update_interval_secs);
        for (item, v) in &self.interval_items {
            item.set_checked(*v == interval_marked);
        }
        self.lock_item.set_checked(config.window.locked);
        for (item, k) in &self.indicator_items {
            item.set_checked(indicator_matches(config.general.indicator, *k));
        }
        for (item, target) in &self.display_items {
            item.set_checked(config.general.panel_display == *target);
        }
        self.forecast_item.set_checked(config.ui.show_forecast);
        self.auto_update_item
            .set_checked(config.general.auto_update);
        for (item, id) in &self.provider_toggles {
            let enabled = config
                .providers
                .iter()
                .find(|p| p.id == *id)
                .is_some_and(|p| p.enabled);
            item.set_checked(enabled);
        }
        for (item, id, method) in &self.auth_method_items {
            let active = config
                .providers
                .iter()
                .find(|p| p.id == *id)
                .is_some_and(|p| p.auth_method == *method);
            item.set_checked(active);
        }
        self.pin_item.set_checked(pinned);
    }
}
