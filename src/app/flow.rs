// app/flow.rs — what several kinds of events share: keeping the providers
// and every surface in line with the config, the picker, and persisting
// state. These were macros inside the event loop before; each one is a
// method of `App` now, so a new action reuses them instead of repeating
// their steps by hand.

use super::{
    auto_open, model_job, open_picker_window, picker_anchor, picker_mode_for,
    provider_set::Rebuilt, surface_rows, visible_data, widget::apply_ui_change, widget_shape, App,
    AutoOpen, UserEvent,
};
use crate::{
    config::schema::Surface,
    providers::ProviderData,
    rows,
    ui::picker::{PickerAction, PickerInput, PickerMode},
};
use tao::event_loop::EventLoopWindowTarget;
use tracing::info;

use super::cache::FileJob;

impl App {
    /// Queue the config for the background writer.
    pub(super) fn save_config(&self) {
        #[cfg(target_os = "windows")]
        self.config_saves.save(self.config_to_save());
        #[cfg(not(target_os = "windows"))]
        self.config_saves.save(self.config.clone());
    }

    /// The providers the surfaces show now (`visible_data`).
    pub(super) fn visible(&self) -> Vec<ProviderData> {
        visible_data(&self.config, &self.display)
    }

    /// Hand the provider cache to its writer when the file is behind
    /// (cache.rs `CacheLedger`).
    pub(super) fn persist_cache(&mut self) {
        if let Some(bytes) = self
            .cache_ledger
            .due(&self.provider_cache, std::time::Instant::now())
        {
            if let Some(w) = self.cache_writer.as_ref() {
                w.send(FileJob::from(bytes));
            }
        }
    }

    /// Hand the alert book to its writer.
    pub(super) fn persist_alerts(&self) {
        if let Some(w) = self.alert_writer.as_ref() {
            w.send(FileJob::from(self.alert_book.to_json()));
        }
    }

    /// What Settings answered to losing the focus (closing, unless
    /// a name is being typed), applied now: once the messages of that
    /// moment are handled, and before a menu opens. Hidden while the focus
    /// was still on its way to the window clicked, the picker took that
    /// click with it: a right-click on the widget opened no menu until a
    /// second one (E2E 2026-11-22).
    pub(super) fn apply_deferred_picker_actions(&mut self) {
        if self.picker_deferred.is_empty() {
            return;
        }
        let actions = std::mem::take(&mut self.picker_deferred);
        self.apply_picker_actions(actions);
    }

    /// Bring the provider instances in line with the config
    /// (provider_set.rs), `force` rebuilt whatever its config. A rebuilt
    /// provider is fetched at once. One whose source or account changed
    /// starts over: Loading until it answers, and nothing of the old one
    /// stays - not its cached numbers, its last error, its trend or the
    /// alerts it gave.
    pub(super) fn sync_providers(&mut self, force: Option<&str>) {
        let mut cache_changed = false;
        for Rebuilt { id, reload } in self.providers.sync(&self.config, force) {
            if reload {
                if let Some(slot) = self.display.iter_mut().find(|d| d.id == id) {
                    *slot = super::loading_data(id.clone());
                }
                cache_changed |= self.provider_cache.remove(&id).is_some();
                self.last_errors.remove(&id);
                self.last_pct.remove(&id);
                self.trends.remove(&id);
                self.forecasts.remove(&id);
                if self.alert_book.forget(&id) {
                    self.persist_alerts();
                }
            }
            self.providers
                .fetch(&id, &self.cmd_tx, self.runtime.handle());
        }
        if cache_changed {
            self.persist_cache();
        }
    }

    /// Point the model readers at what the config now asks for
    /// (`model_job::refresh_models`).
    pub(super) fn refresh_models(&mut self) {
        model_job::refresh_models(
            &self.model_job,
            &self.config,
            &mut self.models,
            &self.cmd_tx,
            self.runtime.handle(),
        );
    }

    /// Recompute the layout and theme for the current shape and resize the
    /// widget (`widget::apply_ui_change`).
    pub(super) fn apply_ui(&mut self) {
        apply_ui_change(
            &self.config,
            &mut self.win_layout,
            &mut self.theme,
            &mut self.pixmap,
            &mut self.surface,
            &self.window,
            &mut self.win_state,
            &self.menu,
            &self.shape,
        );
    }

    /// The widget's shape anew, then `apply_ui`.
    pub(super) fn relayout(&mut self) {
        self.shape = widget_shape(&self.config, &self.display, &self.models, &self.renderer);
        self.apply_ui();
    }

    /// `relayout`, only when the shape changed. Returns whether it did.
    pub(super) fn relayout_if_reshaped(&mut self) -> bool {
        let shape = widget_shape(&self.config, &self.display, &self.models, &self.renderer);
        if shape == self.shape {
            return false;
        }
        self.shape = shape;
        self.apply_ui();
        true
    }

    /// The tray icon's tooltip: the rows chosen in the Tooltip column, as
    /// the panel's tooltip shows them (`tray::tray_tip_text`).
    pub(super) fn sync_tray_tip(&mut self) {
        let rows = surface_rows(&self.config, &self.display, Surface::Tooltip, &self.models);
        let text = crate::ui::tray::tray_tip_text(
            &rows,
            rows::nothing_chosen(&self.config),
            &self.config.ui.tooltip,
        );
        self.tray.set_tip(text);
    }

    /// The tray icon follows the providers shown, and whether any is chosen.
    pub(super) fn sync_tray(&mut self) {
        self.sync_tray_tip();
        let visible = self.visible();
        self.tray
            .set_first_install(rows::nothing_chosen(&self.config), &visible);
        self.tray
            .sync_providers(&self.menu.menu, &visible, &self.theme);
    }

    // ─── The picker ──────────────────────────────────────────────

    pub(super) fn picker_input(&self) -> PickerInput {
        crate::choices::picker_input(
            &self.config,
            &self.display,
            &self.models,
            self.picker_mode,
            &self.found_tools,
            &self.sign_in_view(),
        )
    }

    /// Show the picker the current state, if it is open.
    pub(super) fn refresh_picker(&mut self) {
        if self.picker.is_some() {
            let input = self.picker_input();
            if let Some(p) = self.picker.as_mut() {
                p.set_input(input);
            }
        }
    }

    /// Open the picker (or bring it forward). `activate`: the user opened
    /// it (the menu, a click), so it takes the keyboard focus; the welcome
    /// or the update notice opening by itself does not
    /// (`PickerWindow::open_quietly`).
    pub(super) fn open_picker(
        &mut self,
        target: &EventLoopWindowTarget<UserEvent>,
        activate: bool,
    ) {
        // Opened now, by whatever path: an owed window that waited has had
        // its turn.
        self.auto_open_waiting = false;
        if self.picker.is_none() {
            self.picker_mode = picker_mode_for(&self.config);
            self.found_tools = crate::consent::found_tools();
            let input = self.picker_input();
            let anchor =
                picker_anchor(&self.window, self.window_visible, self.widget_grown(&input));
            self.picker = open_picker_window(
                target,
                &self.renderer,
                input,
                anchor,
                self.picker_at,
                activate,
            );
        } else if let Some(p) = self.picker.as_ref().filter(|_| activate) {
            p.focus();
        }
    }

    /// The widget's size once every row the window offers is ticked for it:
    /// how far it may grow while the window is open, which the window keeps
    /// clear of (`picker_anchor`). None when no row is left to tick.
    fn widget_grown(&self, input: &PickerInput) -> Option<(i32, i32)> {
        let more = input
            .rows
            .iter()
            .filter(|r| !input.widget_rows.iter().any(|w| w == r.id))
            .count();
        if more == 0 {
            return None;
        }
        let mut shape = self.shape.clone();
        shape.choose = false;
        shape.unshown = false;
        shape.model_lines.extend(std::iter::repeat_n(false, more));
        let grown = crate::ui::layout::compute_tuned(
            &self.config.ui.layout,
            &self.config.ui.detail,
            &self.config.ui.width_scale,
            &self.config.ui.column_flow,
            &shape,
            &crate::ui::layout::WidgetMetrics::from_tune(&self.config.tune),
        );
        Some((grown.width.ceil() as i32, grown.height.ceil() as i32))
    }

    /// An owed window that waited for a fullscreen app opens once the app
    /// is no longer in front. `fullscreen` is asked only while one waits.
    pub(super) fn try_auto_open(
        &mut self,
        target: &EventLoopWindowTarget<UserEvent>,
        fullscreen: impl FnOnce() -> bool,
    ) {
        if !self.auto_open_waiting {
            return;
        }
        match auto_open(self.config.reads.pending, self.picker.is_some(), fullscreen) {
            AutoOpen::Now => {
                info!("the fullscreen app is gone: the owed window opens");
                self.open_picker(target, false);
            }
            AutoOpen::No => self.auto_open_waiting = false,
            AutoOpen::Later => {}
        }
    }

    /// A provider set up with the app's own sign-in (a pasted key or usage
    /// token): allowed that alone, not the tool's sign-in, then every
    /// surface follows at once.
    pub(super) fn set_up_with_app_sign_in(&mut self, pid: &str) {
        self.config.allow_app_sign_in(pid);
        crate::choices::show_provider_rows(&mut self.config, pid);
        // The key or token is new: so may be the account.
        self.sync_providers(Some(pid));
        self.refresh_models();
        // A first provider on a fresh install: the widget leaves "Choose
        // what to show" now, not at the first data.
        self.relayout();
        self.sync_tray();
        self.menu.sync(&self.config, self.win_state.pinned);
        self.save_config();
        self.window.request_redraw();
        self.refresh_picker();
    }

    /// Apply what the picker asks for to the config and every surface at
    /// once.
    pub(super) fn apply_picker_actions(&mut self, actions: Vec<PickerAction>) {
        // An edit of config.toml made meanwhile first: the actions apply on
        // top of the file as it is now (app/config_watch.rs).
        self.check_config_file();
        let mut changed = false;
        for action in actions {
            // The Sign-in block's actions reach the Credential Manager, the
            // clipboard and the browser (app/sign_in.rs).
            if self.on_sign_in_action(&action) {
                continue;
            }
            // The settings file opens in Notepad (app/config_watch.rs).
            if action == PickerAction::OpenSettingsFile {
                self.open_settings_file();
                continue;
            }
            let done = crate::choices::apply(&mut self.config, &action);
            if action == PickerAction::NoticeDone {
                self.picker_mode = PickerMode::Normal;
            }
            if done.close {
                if let Some(p) = self.picker.take() {
                    if let Some(at) = p.close() {
                        self.picker_at = Some(at);
                    }
                }
            }
            if let Some(pid) = &done.provider {
                self.sync_providers(Some(pid.as_str()));
            }
            if done.models {
                self.refresh_models();
            }
            changed |= done.changed;
        }
        // Whatever else the actions changed for the providers: several tools
        // allowed at once, the Grok Bot row chosen or not.
        self.sync_providers(None);
        if changed {
            self.save_config();
            self.relayout();
            self.sync_tray();
            let visible = self.visible();
            self.panel.set_numbers(self.config.ui.panel, &visible);
            self.panel.set_tune(&self.config.tune.panel, &visible);
            self.tray
                .set_tune(&self.config.tune.tray, &visible, &self.theme);
            self.tray
                .set_style(self.config.ui.tray.style, &visible, &self.theme);
            self.panel.update(&self.visible(), true);
            self.menu.sync(&self.config, self.win_state.pinned);
            self.window.request_redraw();
        }
        self.refresh_picker();
    }
}
