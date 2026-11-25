// app/menu.rs — the context menu's actions (the widget's, the panel's and
// the tray icon's menu are one). Sign-in lives in the picker
// (app/sign_in.rs).

use super::{App, UserEvent};
use crate::ui::context_menu::MenuAction;
use std::sync::atomic::Ordering;
use tao::event_loop::{ControlFlow, EventLoopWindowTarget};

impl App {
    /// A menu item was clicked.
    pub(super) fn on_menu(
        &mut self,
        id: muda::MenuId,
        target: &EventLoopWindowTarget<UserEvent>,
        control_flow: &mut ControlFlow,
    ) {
        // A look shown while the menu was open was left on screen for its
        // click: the click applies to what was there before the menu.
        #[cfg(target_os = "windows")]
        self.take_look_base();
        let Some(action) = self.menu.action_for(&id) else {
            return;
        };
        // An edit of config.toml made meanwhile first: the click applies on
        // top of the file as it is now (app/config_watch.rs).
        self.check_config_file();
        // What a look item does to the config is one function, the one its
        // preview uses (`context_menu::look`); the arms below repaint and save.
        crate::ui::context_menu::look(&mut self.config, &action);
        match action {
            MenuAction::OpenPicker => self.open_picker_from_menu(target),
            MenuAction::Quit => *control_flow = ControlFlow::Exit,
            MenuAction::OpenProjectPage => {
                crate::platform::open_url(crate::ui::context_menu::PROJECT_URL);
            }
            MenuAction::TogglePin => {
                self.win_state.pinned = !self.win_state.pinned;
                self.window.set_always_on_top(self.win_state.pinned);
                self.menu.sync(&self.config, self.win_state.pinned);
                self.config.window.pinned = self.win_state.pinned;
                self.save_config();
            }
            MenuAction::SetIndicator(kind) => {
                self.config.general.indicator = kind;
                // Tray and panel each ignore the other's modes.
                let visible = self.visible();
                self.tray
                    .set_mode(kind, &self.menu.menu, &visible, &self.theme);
                self.panel.set_mode(kind, &visible);
                self.menu.sync(&self.config, self.win_state.pinned);
                self.save_config();
            }
            MenuAction::SetPanelDisplay(display) => {
                self.config.general.panel_display = display;
                self.panel.set_display(display);
                #[cfg(target_os = "windows")]
                crate::platform::watch_taskbar(display);
                // A full restart, not a reposition: `on_taskbar_moved`
                // returns early while the panel is suppressed, so a panel
                // parked by a fullscreen app could not be brought back by
                // switching displays at all.
                self.panel.restart(&self.visible());
                // The new display may be owned by a fullscreen app; the
                // reposition above cannot see that, so arm the same deferred
                // re-check TaskbarMoved uses to catch it a beat later instead
                // of painting straight over the game.
                #[cfg(target_os = "windows")]
                {
                    self.indicator.recheck_at =
                        Some(std::time::Instant::now() + super::indicator::SETTLE);
                }
                self.menu.sync(&self.config, self.win_state.pinned);
                self.save_config();
            }
            MenuAction::ToggleForecast => {
                self.menu.sync(&self.config, self.win_state.pinned);
                self.save_config();
                self.window.request_redraw();
            }
            MenuAction::SetDetail(_)
            | MenuAction::SetLayout(_)
            | MenuAction::SetWidthScale(_)
            | MenuAction::SetColumnFlow(_) => {
                self.relayout();
                self.save_config();
            }
            MenuAction::SetPalette(_)
            | MenuAction::ToggleMonochrome
            | MenuAction::SetBrightness(_)
            | MenuAction::SetSaturation(_) => self.colours_changed(),
            MenuAction::SetOpacity(_) => {
                self.menu.sync(&self.config, self.win_state.pinned);
                self.save_config();
                self.window.request_redraw();
            }
            MenuAction::SetUpdateInterval(secs) => {
                self.config.general.update_interval_secs = secs;
                // The scheduler picks the new value up on its next cycle.
                self.update_interval.store(secs, Ordering::Relaxed);
                self.menu.sync(&self.config, self.win_state.pinned);
                self.save_config();
            }
            MenuAction::ToggleAutoUpdate => {
                self.config.general.auto_update = !self.config.general.auto_update;
                // The updater task reads this before each check, and
                // checks soon after it is switched on.
                self.auto_update_enabled
                    .set(self.config.general.auto_update);
                self.menu.sync(&self.config, self.win_state.pinned);
                self.save_config();
            }
            MenuAction::ToggleLock => {
                self.config.window.locked = !self.config.window.locked;
                // Cancel any in-flight drag.
                self.win_state.drag_pending = false;
                self.win_state.end_drag();
                self.menu.sync(&self.config, self.win_state.pinned);
                self.save_config();
            }
        }
    }

    /// The palette, monochrome, brightness or saturation changed: every
    /// surface takes the new colours.
    fn colours_changed(&mut self) {
        self.apply_ui();
        let visible = self.visible();
        self.tray.update(&visible, &self.theme, true);
        self.panel.update(&visible, true);
        self.save_config();
    }
}
