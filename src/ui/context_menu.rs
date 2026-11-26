// ui/context_menu.rs — the right-click context menu.
//
// A NATIVE menu via muda — no custom popup window. The look, the indicator
// and the window's options; each provider's sign-in is in the picker
// (Settings, its Sign-in).

use crate::config::schema::{
    ColumnFlow, Config, DetailLevel, IndicatorKind, Layout, Palette, PanelDisplay, WidthScale,
};
use anyhow::Result;
use muda::{CheckMenuItem, IconMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

/// Background opacity steps, %.
const OPACITY_STEPS: &[u8] = &[15, 25, 35, 50, 70, 85];
/// Text and bar brightness steps, %.
const BRIGHTNESS_STEPS: &[u8] = &[40, 60, 80, 100];
/// Palette saturation steps, %.
const SATURATION_STEPS: &[u8] = &[0, 25, 50, 75, 100];
/// Update interval steps, seconds: 1 / 5 / 15 / 30 min.
const INTERVAL_STEPS: &[u64] = &[60, 300, 900, 1800];

/// The submenus whose items change only how the widget, the panel and the
/// tray icon look: the item the pointer rests on is shown before a click
/// (`ContextMenu::look_action`). Their titles name them.
const WIDTH_TITLE: &str = "Width";
const FLOW_TITLE: &str = "Column layout";
const PALETTE_TITLE: &str = "Palette";
const OPACITY_TITLE: &str = "Background opacity";
const BRIGHTNESS_TITLE: &str = "Brightness";
const SATURATION_TITLE: &str = "Saturation";
/// The submenus that group the rest: the detail level, the layout (with
/// Width and Column layout inside), the look (Palette, Monochrome and the
/// three steps), and what the taskbar shows. Detail, Layout and Appearance
/// hold look items of their own.
const DETAIL_TITLE: &str = "Detail";
const LAYOUT_TITLE: &str = "Layout";
const APPEARANCE_TITLE: &str = "Appearance";
const TASKBAR_TITLE: &str = "Taskbar";
const PANEL_DISPLAY_TITLE: &str = "Panel display";
const INTERVAL_TITLE: &str = "Refresh interval";

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
    /// Open the project's GitHub page in the browser.
    OpenProjectPage,
    /// Open the Settings window (the picker).
    OpenPicker,
    Quit,
}

/// The project page the version line opens.
pub const PROJECT_URL: &str = "https://github.com/napxlexn/ailimits";

/// The GitHub mark for the version line: Octicons' `mark-github` (MIT),
/// rasterised to 16 px (32 px for a 200% menu) as straight RGBA, white ink,
/// and quietened to 40% here so it sits back from the text. None if the
/// bytes do not make an icon, in which case the line simply has no mark.
fn github_mark() -> Option<muda::Icon> {
    const MARK_16: &[u8] = include_bytes!("../../assets/github-mark-16.rgba");
    const MARK_32: &[u8] = include_bytes!("../../assets/github-mark-32.rgba");
    const INK: u32 = 102; // 40% of 255
    let (bytes, size) = if crate::platform::menu_scale_is_200() {
        (MARK_32, 32)
    } else {
        (MARK_16, 16)
    };
    let quiet: Vec<u8> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|px| [px[0], px[1], px[2], (px[3] as u32 * INK / 255) as u8])
        .collect();
    muda::Icon::from_rgba(quiet, size, size).ok()
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

/// Whether an action changes only how the widget, the panel and the tray
/// icon look: nothing is read, started or moved, and nothing but the look
/// is saved. Such an item shows itself while the pointer rests on it.
pub fn is_look(action: &MenuAction) -> bool {
    matches!(
        action,
        MenuAction::SetDetail(_)
            | MenuAction::SetLayout(_)
            | MenuAction::SetWidthScale(_)
            | MenuAction::SetColumnFlow(_)
            | MenuAction::ToggleForecast
            | MenuAction::SetPalette(_)
            | MenuAction::ToggleMonochrome
            | MenuAction::SetOpacity(_)
            | MenuAction::SetBrightness(_)
            | MenuAction::SetSaturation(_)
    )
}

/// Apply a look action (`is_look`) to the config: what a click does to it,
/// and what the item shows while the pointer rests on it. Returns false,
/// changing nothing, for any other action.
pub fn look(config: &mut Config, action: &MenuAction) -> bool {
    match action {
        MenuAction::SetDetail(d) => config.ui.detail = d.clone(),
        MenuAction::SetLayout(l) => config.ui.layout = l.clone(),
        MenuAction::SetWidthScale(w) => config.ui.width_scale = *w,
        MenuAction::SetColumnFlow(f) => config.ui.column_flow = *f,
        MenuAction::ToggleForecast => config.ui.show_forecast = !config.ui.show_forecast,
        MenuAction::SetPalette(p) => {
            // Picking a palette leaves monochrome.
            config.ui.palette = p.clone();
            config.ui.monochrome = false;
        }
        MenuAction::ToggleMonochrome => config.ui.monochrome = !config.ui.monochrome,
        MenuAction::SetOpacity(pct) => {
            config.window.opacity = (*pct as f32 / 100.0).clamp(0.10, 0.85);
        }
        MenuAction::SetBrightness(v) => config.ui.brightness = *v,
        MenuAction::SetSaturation(v) => config.ui.saturation = *v,
        _ => return false,
    }
    true
}

/// The id of the item of `items` labelled `text`.
fn labelled<T>(items: &[(CheckMenuItem, T)], text: &str) -> Option<muda::MenuId> {
    items
        .iter()
        .find(|(item, _)| item.text() == text)
        .map(|(item, _)| item.id().clone())
}

/// The full context menu: view, behavior, appearance, providers, actions.
pub struct ContextMenu {
    pub menu: Menu,
    detail_items: Vec<(CheckMenuItem, DetailLevel)>,
    layout_items: Vec<(CheckMenuItem, Layout)>,
    width_items: Vec<(CheckMenuItem, WidthScale)>,
    flow_items: Vec<(CheckMenuItem, ColumnFlow)>,
    /// Width and Column layout, greyed while the other layout is chosen.
    width_submenu: Submenu,
    flow_submenu: Submenu,
    lock_item: CheckMenuItem,
    pin_item: CheckMenuItem,
    indicator_items: Vec<(CheckMenuItem, IndicatorKind)>,
    /// Which taskbar the mini panel attaches to. Empty on a single-display
    /// machine.
    display_items: Vec<(CheckMenuItem, PanelDisplay)>,
    forecast_item: CheckMenuItem,
    auto_update_item: CheckMenuItem,
    monochrome_item: CheckMenuItem,
    palette_items: Vec<(CheckMenuItem, Palette)>,
    opacity_items: Vec<(CheckMenuItem, u8)>,
    brightness_items: Vec<(CheckMenuItem, u8)>,
    saturation_items: Vec<(CheckMenuItem, u8)>,
    interval_items: Vec<(CheckMenuItem, u64)>,
    picker_item: MenuItem,
    quit_item: MenuItem,
    /// The version line; opens the project page.
    version_item: IconMenuItem,
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

        // Layouts, named by what the providers make: rows (Layout::Vertical,
        // providers stacked, horizontal bars) or columns (Layout::Horizontal,
        // side by side, vertical bars). The config keeps its values.
        let layout_items: Vec<(CheckMenuItem, Layout)> =
            [("Rows", Layout::Vertical), ("Columns", Layout::Horizontal)]
                .into_iter()
                .map(|(label, l)| (CheckMenuItem::new(label, true, ui.layout == l, None), l))
                .collect();

        // Width steps. Only the rows layout can be narrowed: the columns
        // layouts size themselves from the provider count.
        // Width and Column layout apply to one layout each: the other one's
        // is greyed (`sync` keeps it so).
        let rows_layout = matches!(ui.layout, Layout::Vertical | Layout::Grid);
        let width_submenu = Submenu::new(WIDTH_TITLE, rows_layout);
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
        let flow_submenu = Submenu::new(FLOW_TITLE, !rows_layout);
        let flow_items: Vec<(CheckMenuItem, ColumnFlow)> = [
            ("Side by side", ColumnFlow::Row),
            ("Stacked", ColumnFlow::Column),
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
        // "Taskbar" submenu — radio-style choice of what the taskbar shows.
        let indicator_submenu = Submenu::new(TASKBAR_TITLE, true);
        // The legacy 16px "bars" tray icon stays config-only (too small to
        // read). A single "Taskbar panel" entry: the grid layout was
        // superseded by the top-2 design.
        let indicator_items: Vec<(CheckMenuItem, IndicatorKind)> = [
            ("Tray icon", IndicatorKind::Tray),
            ("Taskbar panel", IndicatorKind::PanelRows),
            ("None", IndicatorKind::Off),
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
        #[cfg(target_os = "windows")]
        let bars = crate::platform::secondary_taskbars();
        // No secondary taskbars to offer on other platforms.
        #[cfg(not(target_os = "windows"))]
        let bars: Vec<isize> = Vec::new();
        let mut display_items: Vec<(CheckMenuItem, PanelDisplay)> = Vec::new();
        tracing::debug!(
            "menu: {} secondary taskbar(s) found, Display submenu {}",
            bars.len(),
            if bars.is_empty() { "OMITTED" } else { "shown" }
        );
        if !bars.is_empty() {
            let display_submenu = Submenu::new(PANEL_DISPLAY_TITLE, true);
            let mut entries = vec![("Main display".to_string(), PanelDisplay::Primary)];
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
            indicator_submenu.append(&PredefinedMenuItem::separator())?;
            indicator_submenu.append(&display_submenu)?;
        }

        let forecast_item =
            CheckMenuItem::new("Usage forecast", true, config.ui.show_forecast, None);
        let auto_update_item =
            CheckMenuItem::new("Automatic updates", true, config.general.auto_update, None);

        // "Palette" submenu: monochrome + 8 colored palettes.
        let palette_submenu = Submenu::new(PALETTE_TITLE, true);
        let monochrome_item = CheckMenuItem::new("Monochrome", true, ui.monochrome, None);
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
            step_submenu(OPACITY_TITLE, OPACITY_STEPS, opacity_now)?;
        let (brightness_submenu, brightness_items) =
            step_submenu(BRIGHTNESS_TITLE, BRIGHTNESS_STEPS, ui.brightness)?;
        let (saturation_submenu, saturation_items) =
            step_submenu(SATURATION_TITLE, SATURATION_STEPS, ui.saturation)?;

        // Update interval submenu: 1 / 5 / 15 / 30 min.
        let interval_submenu = Submenu::new(INTERVAL_TITLE, true);
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

        let picker_item = MenuItem::new("Settings\u{2026}", true, None);
        let quit_item = MenuItem::new("Quit", true, None);
        // The version line doubles as the way to the project: its GitHub
        // mark says where it leads, so the words are the version alone.
        let version_item = IconMenuItem::new(
            format!("AI Limits {}", env!("CARGO_PKG_VERSION")),
            true,
            github_mark(),
            None,
        );

        // Order: the window; what the widget is (detail, layout, look,
        // taskbar); how it behaves; the data and the app's own updates; the
        // project and Quit, last as in Windows.
        let detail_submenu = Submenu::new(DETAIL_TITLE, true);
        for (item, _) in &detail_items {
            detail_submenu.append(item)?;
        }
        let layout_submenu = Submenu::new(LAYOUT_TITLE, true);
        for (item, _) in &layout_items {
            layout_submenu.append(item)?;
        }
        layout_submenu.append(&PredefinedMenuItem::separator())?;
        layout_submenu.append(&width_submenu)?;
        layout_submenu.append(&flow_submenu)?;
        let appearance_submenu = Submenu::new(APPEARANCE_TITLE, true);
        appearance_submenu.append(&palette_submenu)?;
        appearance_submenu.append(&monochrome_item)?;
        appearance_submenu.append(&PredefinedMenuItem::separator())?;
        appearance_submenu.append(&opacity_submenu)?;
        appearance_submenu.append(&brightness_submenu)?;
        appearance_submenu.append(&saturation_submenu)?;

        menu.append(&picker_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&detail_submenu)?;
        menu.append(&layout_submenu)?;
        menu.append(&appearance_submenu)?;
        menu.append(&indicator_submenu)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&pin_item)?;
        menu.append(&lock_item)?;
        menu.append(&forecast_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&interval_submenu)?;
        menu.append(&auto_update_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&version_item)?;
        menu.append(&quit_item)?;

        Ok(Self {
            menu,
            detail_items,
            layout_items,
            width_items,
            flow_items,
            width_submenu,
            flow_submenu,
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
            picker_item,
            quit_item,
            version_item,
        })
    }

    /// Take "Automatic updates" out of the menu: for a copy that updates
    /// through its own channel (the Store, Scoop, the portable zip, a build
    /// from source), where it would do nothing (`updater::channel`).
    pub fn remove_auto_update(&self) {
        if let Err(e) = self.menu.remove(&self.auto_update_item) {
            tracing::warn!("the Automatic updates item stayed in the menu: {e}");
        }
    }

    /// The look action (`is_look`) of the item the pointer or the keyboard
    /// rests on: `group` is the title of the submenu it is in (None at the
    /// top level), `text` its label as the menu shows it. None for any
    /// other item.
    pub fn look_action(&self, group: Option<&str>, text: &str) -> Option<MenuAction> {
        let toggle = |item: &CheckMenuItem| (item.text() == text).then(|| item.id().clone());
        let id = match group {
            None => toggle(&self.forecast_item),
            Some(DETAIL_TITLE) => labelled(&self.detail_items, text),
            Some(LAYOUT_TITLE) => labelled(&self.layout_items, text),
            Some(APPEARANCE_TITLE) => toggle(&self.monochrome_item),
            Some(WIDTH_TITLE) => labelled(&self.width_items, text),
            Some(FLOW_TITLE) => labelled(&self.flow_items, text),
            Some(PALETTE_TITLE) => labelled(&self.palette_items, text),
            Some(OPACITY_TITLE) => labelled(&self.opacity_items, text),
            Some(BRIGHTNESS_TITLE) => labelled(&self.brightness_items, text),
            Some(SATURATION_TITLE) => labelled(&self.saturation_items, text),
            Some(_) => None,
        }?;
        self.action_for(&id).filter(is_look)
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
        if *event_id == self.pin_item.id() {
            Some(MenuAction::TogglePin)
        } else if *event_id == self.quit_item.id() {
            Some(MenuAction::Quit)
        } else if *event_id == self.picker_item.id() {
            Some(MenuAction::OpenPicker)
        } else if *event_id == self.version_item.id() {
            Some(MenuAction::OpenProjectPage)
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
        // Width and Column layout are the same choice on opposite axes, so
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
        self.width_submenu.set_enabled(rows_layout);
        self.flow_submenu.set_enabled(!rows_layout);
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
        self.pin_item.set_checked(pinned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each item that only changes the look is found by its submenu's title
    /// and its label, as the menu reports them; the same label under
    /// another title is another item; nothing else is a look item.
    #[test]
    fn the_look_items_are_found_by_title_and_label() {
        let menu = ContextMenu::new(&Config::default(), false).unwrap();
        let act = |group: Option<&str>, text: &str| menu.look_action(group, text);
        assert_eq!(
            act(Some(DETAIL_TITLE), "Compact"),
            Some(MenuAction::SetDetail(DetailLevel::Compact))
        );
        assert_eq!(
            act(Some(LAYOUT_TITLE), "Columns"),
            Some(MenuAction::SetLayout(Layout::Horizontal))
        );
        assert_eq!(
            act(None, "Usage forecast"),
            Some(MenuAction::ToggleForecast)
        );
        assert_eq!(
            act(Some(WIDTH_TITLE), "50%"),
            Some(MenuAction::SetWidthScale(WidthScale::Half))
        );
        assert_eq!(
            act(Some(OPACITY_TITLE), "50%"),
            Some(MenuAction::SetOpacity(50))
        );
        assert_eq!(
            act(Some(BRIGHTNESS_TITLE), "60%"),
            Some(MenuAction::SetBrightness(60))
        );
        assert_eq!(
            act(Some(SATURATION_TITLE), "25%"),
            Some(MenuAction::SetSaturation(25))
        );
        assert_eq!(
            act(Some(PALETTE_TITLE), "Ocean"),
            Some(MenuAction::SetPalette(Palette::Ocean))
        );
        assert_eq!(
            act(Some(APPEARANCE_TITLE), "Monochrome"),
            Some(MenuAction::ToggleMonochrome)
        );
        assert_eq!(
            act(Some(FLOW_TITLE), "Stacked"),
            Some(MenuAction::SetColumnFlow(ColumnFlow::Column))
        );
        // Not look items: they move, start or read something.
        assert_eq!(act(None, "Lock position"), None);
        assert_eq!(act(None, "Always on top"), None);
        assert_eq!(act(None, "Automatic updates"), None);
        assert_eq!(act(Some(TASKBAR_TITLE), "Tray icon"), None);
        assert_eq!(act(Some(INTERVAL_TITLE), "5 min"), None);
        // A label of one submenu under another's title.
        assert_eq!(act(Some(OPACITY_TITLE), "Ocean"), None);
        assert_eq!(act(None, "50%"), None);
    }

    /// A look action changes the config as its click does, and nothing
    /// else: every other action leaves it untouched.
    #[test]
    fn a_look_changes_only_the_look() {
        let mut config = Config::default();
        assert!(look(&mut config, &MenuAction::SetOpacity(50)));
        assert!((config.window.opacity - 0.50).abs() < 1e-6);
        assert!(look(&mut config, &MenuAction::SetOpacity(100)));
        assert!((config.window.opacity - 0.85).abs() < 1e-6);
        config.ui.monochrome = true;
        assert!(look(&mut config, &MenuAction::SetPalette(Palette::Rose)));
        assert!(!config.ui.monochrome && config.ui.palette == Palette::Rose);
        let before = config.clone();
        for action in [
            MenuAction::ToggleLock,
            MenuAction::TogglePin,
            MenuAction::SetIndicator(IndicatorKind::Off),
            MenuAction::SetUpdateInterval(300),
            MenuAction::ToggleAutoUpdate,
            MenuAction::OpenPicker,
            MenuAction::Quit,
        ] {
            assert!(!is_look(&action));
            assert!(!look(&mut config, &action));
        }
        assert_eq!(config, before);
    }
}
