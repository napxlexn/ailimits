// app/config_watch.rs — config.toml edited by hand, applied while the app
// runs, without a restart.
//
// No change notification of Windows is used (that would be a new import): the
// file's stamp (`storage::FileStamp`, one metadata call) is compared on timers
// the app has anyway:
//   - every 2 seconds while the user is at the PC (input within 5 minutes),
//     every 30 while away, and every 2 for 30 minutes after the file was
//     opened from Rows and models: a deadline of the event loop
//     (`ConfigWatch`), never a thread;
//   - once per polling cycle, off the event loop (`ConfigFile::tell_if_changed`
//     in the scheduler's cycle hook);
//   - right before a menu or picker action and the exit, and before each of
//     the app's own saves (a drag's end saves the position: the writer finds
//     the file changed and tells), so a change made in the app lands on top
//     of the file as it is now.
// The app's own writes are recorded by the writer (`ConfigFile::write`), so
// they never count as edits.
//
// An edit is merged with what the app changed meanwhile (`merge`) and applied
// part by part (`Part`, `Effects`): only what changed is redone.

use super::{feedback, App};
use crate::config::schema::Config;
use crate::config::storage::{self, Change, ConfigFile};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// How long the file is looked at often after it was opened from the window,
/// whether or not the user is at the PC.
pub(super) const FAST_FOR: Duration = Duration::from_secs(30 * 60);
/// How often the file is looked at while the user is at the PC (or in the
/// half hour after it was opened from the window): an edit shows within
/// about two seconds of its save. One metadata call each time.
pub(super) const FAST_EVERY: Duration = Duration::from_secs(2);
/// How often while nobody is at the PC (no input for `AWAY_AFTER_SECS`).
pub(super) const SLOW_EVERY: Duration = Duration::from_secs(30);
/// No input for this long: the user is away (the scheduler pauses polling
/// at the same mark).
pub(super) const AWAY_AFTER_SECS: u64 = 300;
/// A look that found the file busy (being written) comes again this soon,
/// at most `RETRIES` times in a row.
const RETRY_IN: Duration = Duration::from_millis(300);
const RETRIES: u8 = 5;

/// One part of the config as far as applying it goes: each is compared,
/// merged and applied whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Part {
    Interval,
    Indicator,
    AutoUpdate,
    PanelOffset,
    PanelDisplay,
    Position,
    Opacity,
    Pinned,
    Locked,
    Colours,
    Layout,
    Forecast,
    Rows,
    Model,
    Tooltip,
    WidgetNumbers,
    PanelNumbers,
    TrayStyle,
    Providers,
    Notifications,
    Hooks,
    Reads,
    Tune,
}

impl Part {
    pub(super) const ALL: [Part; 23] = [
        Part::Interval,
        Part::Indicator,
        Part::AutoUpdate,
        Part::PanelOffset,
        Part::PanelDisplay,
        Part::Position,
        Part::Opacity,
        Part::Pinned,
        Part::Locked,
        Part::Colours,
        Part::Layout,
        Part::Forecast,
        Part::Rows,
        Part::Model,
        Part::Tooltip,
        Part::WidgetNumbers,
        Part::PanelNumbers,
        Part::TrayStyle,
        Part::Providers,
        Part::Notifications,
        Part::Hooks,
        Part::Reads,
        Part::Tune,
    ];

    /// Whether `a` and `b` differ in this part.
    pub(super) fn differs(self, a: &Config, b: &Config) -> bool {
        let (ga, gb) = (&a.general, &b.general);
        let (wa, wb) = (&a.window, &b.window);
        let (ua, ub) = (&a.ui, &b.ui);
        match self {
            Part::Interval => ga.update_interval_secs != gb.update_interval_secs,
            Part::Indicator => ga.indicator != gb.indicator,
            Part::AutoUpdate => ga.auto_update != gb.auto_update,
            Part::PanelOffset => {
                (ga.panel_offset_x, ga.panel_offset_y) != (gb.panel_offset_x, gb.panel_offset_y)
            }
            Part::PanelDisplay => ga.panel_display != gb.panel_display,
            Part::Position => (wa.pos_x, wa.pos_y) != (wb.pos_x, wb.pos_y),
            // By its bits: a NaN typed in is one value, not a change forever.
            Part::Opacity => wa.opacity.to_bits() != wb.opacity.to_bits(),
            Part::Pinned => wa.pinned != wb.pinned,
            Part::Locked => wa.locked != wb.locked,
            Part::Colours => {
                ua.palette != ub.palette
                    || ua.saturation != ub.saturation
                    || ua.brightness != ub.brightness
                    || ua.monochrome != ub.monochrome
            }
            Part::Layout => {
                ua.layout != ub.layout
                    || ua.detail != ub.detail
                    || ua.width_scale != ub.width_scale
                    || ua.column_flow != ub.column_flow
            }
            Part::Forecast => ua.show_forecast != ub.show_forecast,
            Part::Rows => ua.rows != ub.rows,
            Part::Model => ua.model != ub.model,
            Part::Tooltip => ua.tooltip != ub.tooltip,
            Part::WidgetNumbers => ua.widget != ub.widget,
            Part::PanelNumbers => ua.panel != ub.panel,
            Part::TrayStyle => ua.tray != ub.tray,
            Part::Providers => a.providers != b.providers,
            Part::Notifications => a.notifications != b.notifications,
            Part::Hooks => a.hooks != b.hooks,
            Part::Reads => {
                let (ra, rb) = (&a.reads, &b.reads);
                ra.sign_in != rb.sign_in
                    || ra.app_sign_in != rb.app_sign_in
                    || ra.tool_settings != rb.tool_settings
                    || ra.last_session != rb.last_session
            }
            Part::Tune => a.tune != b.tune,
        }
    }

    /// This part of `from`, put into `to`.
    pub(super) fn copy(self, from: &Config, to: &mut Config) {
        let (g, w, u) = (&from.general, &from.window, &from.ui);
        match self {
            Part::Interval => to.general.update_interval_secs = g.update_interval_secs,
            Part::Indicator => to.general.indicator = g.indicator,
            Part::AutoUpdate => to.general.auto_update = g.auto_update,
            Part::PanelOffset => {
                to.general.panel_offset_x = g.panel_offset_x;
                to.general.panel_offset_y = g.panel_offset_y;
            }
            Part::PanelDisplay => to.general.panel_display = g.panel_display,
            Part::Position => {
                to.window.pos_x = w.pos_x;
                to.window.pos_y = w.pos_y;
            }
            Part::Opacity => to.window.opacity = w.opacity,
            Part::Pinned => to.window.pinned = w.pinned,
            Part::Locked => to.window.locked = w.locked,
            Part::Colours => {
                to.ui.palette = u.palette.clone();
                to.ui.saturation = u.saturation;
                to.ui.brightness = u.brightness;
                to.ui.monochrome = u.monochrome;
            }
            Part::Layout => {
                to.ui.layout = u.layout.clone();
                to.ui.detail = u.detail.clone();
                to.ui.width_scale = u.width_scale;
                to.ui.column_flow = u.column_flow;
            }
            Part::Forecast => to.ui.show_forecast = u.show_forecast,
            Part::Rows => to.ui.rows = u.rows.clone(),
            Part::Model => to.ui.model = u.model.clone(),
            Part::Tooltip => to.ui.tooltip = u.tooltip,
            Part::WidgetNumbers => to.ui.widget = u.widget,
            Part::PanelNumbers => to.ui.panel = u.panel,
            Part::TrayStyle => to.ui.tray = u.tray,
            Part::Providers => to.providers = from.providers.clone(),
            Part::Notifications => to.notifications = from.notifications.clone(),
            Part::Hooks => to.hooks = from.hooks.clone(),
            Part::Reads => {
                to.reads.sign_in = from.reads.sign_in.clone();
                to.reads.app_sign_in = from.reads.app_sign_in.clone();
                to.reads.tool_settings = from.reads.tool_settings;
                to.reads.last_session = from.reads.last_session;
            }
            Part::Tune => to.tune = from.tune.clone(),
        }
    }

    /// The parts in which `a` and `b` differ.
    pub(super) fn changed(a: &Config, b: &Config) -> Vec<Part> {
        Part::ALL.into_iter().filter(|p| p.differs(a, b)).collect()
    }
}

/// The config to run on after an edit of the file, and the parts kept from
/// the app's own.
///
/// The file's (`theirs`), except for the parts the app changed since the
/// file last held `base` and the edit left as they were: a change made in
/// the app meanwhile (while the file did not read, or in the moment before
/// it was saved) is not lost. Where both changed a part, the file's wins:
/// it is what the user wrote last. With no `base` (the file's content was
/// not known), the file's wins whole. The app's own record - the window
/// still owed (`pending`) and the sources settled once (`sources_settled`) -
/// is never taken from the file: an edit does not bring the welcome back,
/// nor settle the sources again.
pub(super) fn merge(base: Option<&Config>, ours: &Config, theirs: &Config) -> (Config, Vec<Part>) {
    let mut out = theirs.clone();
    let mut kept = Vec::new();
    if let Some(base) = base {
        for p in Part::ALL {
            if p.differs(base, ours) && !p.differs(base, theirs) {
                p.copy(ours, &mut out);
                kept.push(p);
            }
        }
    }
    out.reads.pending = ours.reads.pending;
    out.reads.sources_settled = ours.reads.sources_settled;
    (out, kept)
}

/// What the app redoes for the parts that changed: only what they need.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Effects {
    /// The provider instances rebuilt where their config changed (and
    /// fetched), the display in config order.
    pub providers: bool,
    /// The model readers pointed at what the config asks for now.
    pub models: bool,
    /// The widget's shape, layout and theme anew, and its size.
    pub relayout: bool,
    /// The tray icon and the panel repainted with the new look.
    pub repaint: bool,
    /// The tray icon follows the providers shown.
    pub tray_rows: bool,
    /// `[tune.panel]` and `[tune.tray]` handed to the panel and the tray
    /// icon (`[tune.widget]` and `[tune.format]` reach the widget through
    /// the tuned layout and theme of `relayout`).
    pub tune: bool,
    pub indicator: bool,
    pub panel_numbers: bool,
    pub tray_style: bool,
    pub panel_offset: bool,
    pub panel_display: bool,
    pub position: bool,
    pub pinned: bool,
    pub locked: bool,
    pub interval: bool,
    pub auto_update: bool,
}

impl Effects {
    pub(super) fn of(parts: &[Part]) -> Self {
        let mut e = Effects::default();
        for p in parts {
            match p {
                Part::Interval => e.interval = true,
                Part::Indicator => e.indicator = true,
                Part::AutoUpdate => e.auto_update = true,
                Part::PanelOffset => e.panel_offset = true,
                Part::PanelDisplay => e.panel_display = true,
                Part::Position => e.position = true,
                Part::Pinned => e.pinned = true,
                Part::Locked => e.locked = true,
                // Read at the next paint; the menu's ticks follow below.
                Part::Opacity | Part::Forecast => {}
                Part::Colours => {
                    e.relayout = true;
                    e.repaint = true;
                }
                // Fine-tuning reaches sizes and colours of every surface.
                Part::Tune => {
                    e.relayout = true;
                    e.repaint = true;
                    e.tune = true;
                }
                Part::Layout | Part::WidgetNumbers => e.relayout = true,
                // The Grok Bot row decides whether Cursor asks for it.
                Part::Rows => {
                    e.providers = true;
                    e.relayout = true;
                    e.tray_rows = true;
                    e.repaint = true;
                }
                Part::Model => {
                    e.models = true;
                    e.relayout = true;
                }
                Part::PanelNumbers => e.panel_numbers = true,
                Part::TrayStyle => e.tray_style = true,
                Part::Providers | Part::Reads => {
                    e.providers = true;
                    e.models = true;
                    e.relayout = true;
                    e.tray_rows = true;
                    e.repaint = true;
                }
                // Read when the tooltip shows, or an alert or a hook is due.
                Part::Tooltip | Part::Notifications | Part::Hooks => {}
            }
        }
        e
    }
}

/// When the file is looked at besides the polling cycle and the actions.
pub(super) struct ConfigWatch {
    pub(super) file: Arc<ConfigFile>,
    /// Looked at often until then: the file was opened from the window.
    fast_until: Option<Instant>,
    /// The next look, a deadline of the event loop.
    next_look: Option<Instant>,
    /// Looks in a row that found the file busy.
    retries: u8,
    /// The parse error last told, while the file stays broken.
    broken: Option<String>,
}

impl ConfigWatch {
    pub(super) fn new(file: Arc<ConfigFile>) -> Self {
        Self {
            file,
            fast_until: None,
            next_look: Some(Instant::now() + FAST_EVERY),
            retries: 0,
            broken: None,
        }
    }

    /// The next look, if one is set: the loop sleeps until then at most.
    pub(super) fn next_wake(&self) -> Option<Instant> {
        self.next_look
    }

    /// The file was opened from the window: looked at every `FAST_EVERY`
    /// for `FAST_FOR`.
    pub(super) fn look_often(&mut self, now: Instant) {
        self.fast_until = Some(now + FAST_FOR);
        self.next_look = Some(now + FAST_EVERY);
    }

    /// Whether a look is due at `now`; the next one is set: in
    /// `FAST_EVERY` while the user is at the PC (`idle_secs`, asked only
    /// when a look is due) or the file was opened from the window lately,
    /// else in `SLOW_EVERY`.
    pub(super) fn due(&mut self, now: Instant, idle_secs: impl FnOnce() -> u64) -> bool {
        match self.next_look {
            Some(t) if now >= t => {
                let opened = self.fast_until.is_some_and(|end| now < end);
                if !opened {
                    self.fast_until = None;
                }
                let every = if opened || idle_secs() < AWAY_AFTER_SECS {
                    FAST_EVERY
                } else {
                    SLOW_EVERY
                };
                self.next_look = Some(now + every);
                true
            }
            _ => false,
        }
    }

    /// The file was busy: looked at again shortly, a few times in a row.
    fn retry(&mut self, now: Instant) {
        if self.retries >= RETRIES {
            return;
        }
        self.retries += 1;
        let soon = now + RETRY_IN;
        self.next_look = Some(self.next_look.map_or(soon, |t| t.min(soon)));
    }
}

/// What a broken file is told with: where, never what.
fn broken_message(error: &str) -> String {
    format!(
        "config.toml was saved, but it does not read: {error}. AI Limits keeps running on the settings it had and does not save over the file until it reads again."
    )
}

const READS_AGAIN: &str = "config.toml reads again, and its settings apply now.";

/// `%SystemRoot%\System32\notepad.exe`: the full path, so no notepad.exe
/// elsewhere on the search path is started.
fn system_notepad() -> std::path::PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    std::path::Path::new(&root)
        .join("System32")
        .join("notepad.exe")
}

impl App {
    /// Look at config.toml and apply an edit made elsewhere. Skipped while a
    /// menu shows a look under the pointer: the look in memory is not the
    /// config, and the menu's click comes with a look of its own.
    pub(super) fn check_config_file(&mut self) {
        #[cfg(target_os = "windows")]
        if self.menu_preview.is_some() || self.look_base.is_some() {
            return;
        }
        let change = self.config_watch.file.check();
        if change != Change::Retry {
            self.config_watch.retries = 0;
        }
        match change {
            Change::None => {}
            Change::Retry => self.config_watch.retry(Instant::now()),
            Change::Gone => {
                info!("config.toml was removed or emptied: kept in memory, written back with the next save")
            }
            Change::Broken(error) => {
                warn!("config.toml was edited and does not read: {error}");
                if self.config_watch.broken.as_deref() != Some(error.as_str()) {
                    feedback("AI Limits", &broken_message(&error));
                    self.config_watch.broken = Some(error);
                }
            }
            Change::Edited {
                config,
                base,
                left_out,
            } => {
                let was_broken = self.config_watch.broken.take().is_some();
                self.on_config_edited(*config, base.map(|b| *b));
                if !left_out.is_empty() {
                    feedback("AI Limits", &storage::left_out_message(&left_out));
                } else if was_broken {
                    feedback("AI Limits", READS_AGAIN);
                }
            }
        }
    }

    /// An edit of the file, read: merged with what the app changed since
    /// (`merge`), brought on screen as a start would (a position on no
    /// monitor comes back), applied, and saved when the app had something
    /// the file did not.
    fn on_config_edited(&mut self, theirs: Config, base: Option<Config>) {
        let (merged, kept) = merge(base.as_ref(), &self.config, &theirs);
        if !kept.is_empty() {
            info!("config.toml edited; kept from the app: {kept:?}");
        }
        self.apply_reloaded_config(merged);
        if !kept.is_empty() {
            self.save_config();
        }
    }

    /// Run on `new` from now on: what differs from the config in use is
    /// applied to every surface, and nothing else is redone.
    pub(super) fn apply_reloaded_config(&mut self, new: Config) {
        let parts = Part::changed(&self.config, &new);
        self.config = new;
        if parts.is_empty() {
            return;
        }
        info!("config.toml edited: {parts:?} applied");
        let e = Effects::of(&parts);
        if e.interval {
            // As from the menu: the scheduler takes it after its wait.
            self.update_interval
                .store(self.config.general.update_interval_secs, Ordering::Relaxed);
        }
        if e.auto_update {
            self.auto_update_enabled
                .set(self.config.general.auto_update);
        }
        if e.pinned {
            self.win_state.pinned = self.config.window.pinned;
            self.window.set_always_on_top(self.win_state.pinned);
        }
        if e.locked {
            self.win_state.drag_pending = false;
            self.win_state.end_drag();
        }
        if e.position {
            // On screen as a start would put it; the position written stays
            // as written, so its display coming back brings the widget there.
            let (x, y) = (self.config.window.pos_x, self.config.window.pos_y);
            #[cfg(target_os = "windows")]
            let (x, y) = crate::platform::ensure_on_screen(x, y);
            self.window
                .set_outer_position(tao::dpi::PhysicalPosition::new(x, y));
            self.win_state.pos = (x as f64, y as f64);
            // The accent is lost on a move to another monitor.
            #[cfg(target_os = "windows")]
            super::widget::apply_windows_glass(&self.window);
        }
        if e.providers {
            self.sync_providers(None);
            self.follow_provider_order();
        }
        if e.models {
            self.refresh_models();
        }
        if e.relayout {
            self.relayout();
        }
        let visible = self.visible();
        if e.indicator {
            let kind = self.config.general.indicator;
            self.tray
                .set_mode(kind, &self.menu.menu, &visible, &self.theme);
            self.panel.set_mode(kind, &visible);
        }
        if e.panel_display {
            let display = self.config.general.panel_display;
            self.panel.set_display(display);
            #[cfg(target_os = "windows")]
            crate::platform::watch_taskbar(display);
            // As from the menu: a restart, not a reposition, and a look a
            // beat later in case the new bar is under a fullscreen app.
            self.panel.restart(&visible);
            #[cfg(target_os = "windows")]
            {
                self.indicator.recheck_at = Some(Instant::now() + super::indicator::SETTLE);
            }
        }
        if e.panel_offset {
            self.panel.set_offset(
                self.config.general.panel_offset_x,
                self.config.general.panel_offset_y,
            );
            self.panel.on_taskbar_moved(&visible);
        }
        if e.panel_numbers {
            self.panel.set_numbers(self.config.ui.panel, &visible);
        }
        if e.tray_style {
            self.tray
                .set_style(self.config.ui.tray.style, &visible, &self.theme);
        }
        if e.tune {
            // After `relayout`: the tray's rings take the new theme.
            self.panel.set_tune(&self.config.tune.panel, &visible);
            self.tray
                .set_tune(&self.config.tune.tray, &visible, &self.theme);
        }
        if e.tray_rows {
            self.sync_tray();
        }
        self.sync_tray_tip();
        if e.repaint {
            self.tray.update(&visible, &self.theme, true);
            self.panel.update(&visible, true);
        }
        self.menu.sync(&self.config, self.win_state.pinned);
        self.window.request_redraw();
        self.refresh_picker();
    }

    /// The display follows the providers' order in the config (an edit may
    /// move them): each keeps what it showed.
    fn follow_provider_order(&mut self) {
        let ids = self.providers.ids();
        if self.display.iter().map(|d| &d.id).eq(ids.iter()) {
            return;
        }
        let mut old = std::mem::take(&mut self.display);
        self.display = ids
            .into_iter()
            .map(|id| match old.iter().position(|d| d.id == id) {
                Some(i) => old.swap_remove(i),
                None => super::loading_data(id),
            })
            .collect();
    }

    /// Rows and models' "Settings file...": config.toml in Notepad, and the
    /// file looked at often while it may be edited there.
    pub(super) fn open_settings_file(&mut self) {
        let path = self.config_watch.file.path().to_path_buf();
        // Never opened on nothing: a file removed meanwhile is written first.
        if !path.exists() {
            #[cfg(target_os = "windows")]
            let config = self.config_to_save();
            #[cfg(not(target_os = "windows"))]
            let config = self.config.clone();
            let generation = self.config_watch.file.generation();
            let _ = self.config_watch.file.write(&config, generation);
        }
        // In the Store copy the file may be in the package's own folder,
        // which Notepad does not see at the %APPDATA% path.
        let path = storage::path_for_others(&path);
        // CreateProcessW with the path as the one argument: no shell verb,
        // nothing the shell would look up.
        match std::process::Command::new(system_notepad())
            .arg(&path)
            .spawn()
        {
            Ok(_) => {
                info!("config.toml opened in Notepad; looked at often for a while");
                self.config_watch.look_often(Instant::now());
            }
            Err(e) => {
                warn!("Notepad did not start: {e}");
                feedback(
                    "AI Limits",
                    &format!(
                        "Notepad did not start. The settings file is {}",
                        path.display()
                    ),
                );
            }
        }
    }
}

impl App {
    /// At exit: an edit not read yet is merged in first, so the last save
    /// does not write over it (and is not refused for it). Nothing is
    /// redrawn: the windows are going.
    pub(super) fn config_file_at_exit(&mut self) {
        #[cfg(target_os = "windows")]
        if self.menu_preview.is_some() || self.look_base.is_some() {
            return;
        }
        if let Change::Edited { config, base, .. } = self.config_watch.file.check() {
            let (merged, _) = merge(base.as_deref(), &self.config, &config);
            self.config = merged;
        }
    }
}

/// The writer and the watch, started from the file as it was loaded.
pub(super) fn config_file(on_disk: Option<(storage::FileStamp, Config)>) -> Arc<ConfigFile> {
    Arc::new(ConfigFile::new(storage::config_path(), on_disk))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{IndicatorKind, Palette, PanelDisplay, PendingNotice};

    /// A config with every part set otherwise than the default.
    fn every_part_changed() -> Config {
        let mut c = Config::default();
        c.general.update_interval_secs = 300;
        c.general.indicator = IndicatorKind::PanelRows;
        c.general.auto_update = false;
        c.general.panel_offset_x = 5;
        c.general.panel_offset_y = -3;
        c.general.panel_display = PanelDisplay::Secondary(1);
        c.window.pos_x = 900;
        c.window.pos_y = 40;
        c.window.opacity = 0.7;
        c.window.pinned = true;
        c.window.locked = true;
        c.ui.palette = Palette::Ocean;
        c.ui.saturation = 10;
        c.ui.brightness = 60;
        c.ui.monochrome = true;
        c.ui.layout = crate::config::schema::Layout::Horizontal;
        c.ui.detail = crate::config::schema::DetailLevel::Expanded;
        c.ui.width_scale = crate::config::schema::WidthScale::Half;
        c.ui.column_flow = crate::config::schema::ColumnFlow::Column;
        c.ui.show_forecast = true;
        c.ui.rows.widget = Some(vec!["claude".into(), "claude.opus".into()]);
        c.ui.model.widget = vec![crate::config::schema::ModelPart::Model];
        c.ui.tooltip.percent = false;
        c.ui.widget.bar = false;
        c.ui.panel.reset = true;
        c.ui.tray.style = crate::config::schema::TrayStyle::Bars;
        c.providers[0].alert_threshold = 50;
        c.notifications.cooldown_minutes = 60;
        c.hooks.on_reset = "echo reset".into();
        c.reads.sign_in = vec!["claude".into()];
        c.reads.tool_settings = true;
        c.tune.widget.text_scale = Some(1.2);
        c.tune.panel.digit_size = Some(14.0);
        c.tune.tray.ring_width = Some(3.0);
        c.tune.format.high_at = Some(90.0);
        c
    }

    /// Every field of the config belongs to a part (the app's own record
    /// aside): copying every part copies the whole config. A field added
    /// to no part would never be applied or merged.
    #[test]
    fn every_field_belongs_to_a_part() {
        let theirs = every_part_changed();
        let mut out = Config::default();
        for p in Part::ALL {
            assert!(
                p.differs(&Config::default(), &theirs),
                "{p:?} not changed by the sample"
            );
            p.copy(&theirs, &mut out);
        }
        assert_eq!(out, theirs);
        assert!(Part::changed(&out, &theirs).is_empty());
    }

    /// The parts are told apart: a change in one is a change in it alone.
    #[test]
    fn a_change_is_found_in_its_part_alone() {
        let base = Config::default();
        let mut c = base.clone();
        c.window.pos_x += 10;
        assert_eq!(Part::changed(&base, &c), [Part::Position]);
        let mut c = base.clone();
        c.ui.palette = Palette::Rose;
        c.reads.tool_settings = true;
        assert_eq!(Part::changed(&base, &c), [Part::Colours, Part::Reads]);
        // The app's own record is no part: an edit of it changes nothing.
        let mut c = base.clone();
        c.reads.pending = PendingNotice::Welcome;
        c.reads.sources_settled = !base.reads.sources_settled;
        assert!(Part::changed(&base, &c).is_empty());
        // A NaN opacity is one value.
        let mut a = base.clone();
        a.window.opacity = f32::NAN;
        assert!(Part::changed(&a, &a.clone()).is_empty());
    }

    /// A hand edit with nothing changed in the app meanwhile: the file's
    /// config, whole.
    #[test]
    fn an_edit_alone_is_taken_as_it_is() {
        let base = Config::default();
        let mut theirs = base.clone();
        theirs.ui.palette = Palette::Neon;
        theirs.hooks.on_threshold = "echo hi".into();
        let (merged, kept) = merge(Some(&base), &base, &theirs);
        assert_eq!(merged, theirs);
        assert!(kept.is_empty());
    }

    /// The widget dragged while the file did not read (or in the moment
    /// before it was saved): the drag stays, the edit applies, and the two
    /// are saved together.
    #[test]
    fn a_change_made_in_the_app_meanwhile_is_kept() {
        let base = Config::default();
        let mut ours = base.clone();
        ours.window.pos_x = 777;
        ours.ui.rows.widget = Some(vec!["codex".into()]);
        let mut theirs = base.clone();
        theirs.ui.palette = Palette::Forest;
        let (merged, kept) = merge(Some(&base), &ours, &theirs);
        assert_eq!(merged.window.pos_x, 777);
        assert_eq!(merged.ui.rows.widget, ours.ui.rows.widget);
        assert_eq!(merged.ui.palette, Palette::Forest);
        assert_eq!(kept, [Part::Position, Part::Rows]);
    }

    /// Both changed the same part: the file's wins, it was written last.
    /// Without a known base the file's wins whole.
    #[test]
    fn the_file_wins_where_both_changed() {
        let base = Config::default();
        let mut ours = base.clone();
        ours.ui.palette = Palette::Ice;
        let mut theirs = base.clone();
        theirs.ui.palette = Palette::Slate;
        let (merged, kept) = merge(Some(&base), &ours, &theirs);
        assert_eq!(merged.ui.palette, Palette::Slate);
        assert!(kept.is_empty());
        let (merged, kept) = merge(None, &ours, &theirs);
        assert_eq!(merged.ui.palette, Palette::Slate);
        assert!(kept.is_empty());
    }

    /// The welcome still owed and the sources settled once are the app's:
    /// an edit neither brings the welcome back nor settles again.
    #[test]
    fn the_apps_own_record_is_never_taken_from_the_file() {
        let mut ours = Config::default();
        ours.reads.pending = PendingNotice::None;
        ours.reads.sources_settled = true;
        let mut theirs = ours.clone();
        theirs.reads.pending = PendingNotice::Welcome;
        theirs.reads.sources_settled = false;
        let (merged, _) = merge(Some(&ours), &ours, &theirs);
        assert_eq!(merged.reads.pending, PendingNotice::None);
        assert!(merged.reads.sources_settled);
    }

    /// Only what a part needs is redone.
    #[test]
    fn each_part_redoes_only_what_it_needs() {
        assert_eq!(Effects::of(&[]), Effects::default());
        // Hooks, alerts and the tooltip are read when they are due.
        assert_eq!(
            Effects::of(&[Part::Hooks, Part::Notifications, Part::Tooltip]),
            Effects::default()
        );
        // The opacity and the forecast: the next paint.
        assert_eq!(
            Effects::of(&[Part::Opacity, Part::Forecast]),
            Effects::default()
        );
        let e = Effects::of(&[Part::Position]);
        assert_eq!(
            e,
            Effects {
                position: true,
                ..Default::default()
            }
        );
        // A palette: the theme anew on every surface, no provider touched.
        let e = Effects::of(&[Part::Colours]);
        assert!(e.relayout && e.repaint && !e.providers && !e.models && !e.tune);
        // A consent given in the file: the providers and the model readers
        // follow, and every surface.
        let e = Effects::of(&[Part::Reads]);
        assert!(e.providers && e.models && e.relayout && e.tray_rows && e.repaint);
        // The rows: Cursor's Grok Bot follows them, the models do not.
        let e = Effects::of(&[Part::Rows]);
        assert!(e.providers && !e.models && e.relayout);
        let e = Effects::of(&[Part::PanelDisplay, Part::PanelOffset, Part::PanelNumbers]);
        assert!(e.panel_display && e.panel_offset && e.panel_numbers && !e.relayout);
        let e = Effects::of(&[Part::Indicator, Part::Interval, Part::AutoUpdate]);
        assert!(e.indicator && e.interval && e.auto_update && !e.providers);
        let e = Effects::of(&[Part::Pinned, Part::Locked]);
        assert!(e.pinned && e.locked && !e.relayout);
        // Fine-tuning: the widget's tuned layout and theme, and the knobs
        // handed to the panel and the tray icon.
        let e = Effects::of(&[Part::Tune]);
        assert!(e.relayout && e.repaint && e.tune && !e.providers && !e.models);
    }

    /// Every 2 seconds while the user is at the PC, every 30 while away;
    /// opened from the window, every 2 seconds for 30 minutes either way.
    #[test]
    fn the_file_is_looked_at_often_while_the_user_is_there() {
        let file = Arc::new(ConfigFile::new(
            std::env::temp_dir().join("ailimits-watch-timing-none.toml"),
            None,
        ));
        let mut w = ConfigWatch::new(file);
        let t0 = Instant::now() + FAST_EVERY;
        // From the start: looked at.
        assert!(w.next_wake().is_some_and(|t| t <= t0));
        assert!(w.due(t0, || 0));
        assert_eq!(w.next_wake(), Some(t0 + FAST_EVERY));
        assert!(!w.due(t0 + Duration::from_secs(1), || 0));
        // Away: every 30 seconds.
        let t1 = t0 + FAST_EVERY;
        assert!(w.due(t1, || AWAY_AFTER_SECS));
        assert_eq!(w.next_wake(), Some(t1 + SLOW_EVERY));
        // Opened from the window: often though away, for half an hour.
        w.look_often(t1);
        assert_eq!(w.next_wake(), Some(t1 + FAST_EVERY));
        assert!(w.due(t1 + FAST_EVERY, || 9999));
        assert_eq!(w.next_wake(), Some(t1 + FAST_EVERY * 2));
        let late = t1 + FAST_FOR - Duration::from_secs(1);
        assert!(w.due(late, || 9999));
        assert_eq!(w.next_wake(), Some(late + FAST_EVERY));
        // Past it, still away: slow again.
        let past = t1 + FAST_FOR + Duration::from_secs(1);
        assert!(w.due(past, || 9999));
        assert_eq!(w.next_wake(), Some(past + SLOW_EVERY));
        // A busy file: a few quick looks, not forever.
        let t1 = Instant::now();
        for _ in 0..RETRIES {
            w.retry(t1);
            assert_eq!(w.next_wake(), Some(t1 + RETRY_IN));
            assert!(w.due(t1 + RETRY_IN, || 0));
        }
        // Past the retries: the next look is the usual one.
        w.retry(t1);
        assert_eq!(w.next_wake(), Some(t1 + RETRY_IN + FAST_EVERY));
    }

    /// One metadata call: what each look costs while nothing changed.
    #[test]
    fn a_look_at_an_unchanged_file_costs_one_metadata_call() {
        let dir = std::env::temp_dir().join(format!("ailimits-watch-cost-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, toml::to_string_pretty(&Config::default()).unwrap()).unwrap();
        let file = ConfigFile::new(path, None);
        let n = 200u32;
        let start = Instant::now();
        for _ in 0..n {
            assert_eq!(file.check(), Change::None);
        }
        let each = start.elapsed() / n;
        let _ = std::fs::remove_dir_all(&dir);
        println!("a look at an unchanged config.toml: {each:?}");
        // Generous: a local metadata call is tens of microseconds.
        assert!(each < Duration::from_millis(20), "{each:?}");
    }
}
