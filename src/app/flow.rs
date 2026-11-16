// app/flow.rs — what several kinds of events share: keeping the providers
// and every surface in line with the config, the picker, and persisting
// state. These were macros inside the event loop before; each one is a
// method of `App` now, so a new action reuses them instead of repeating
// their steps by hand.

use super::{
    auto_open, model_job, open_picker_window, picker_anchor, picker_mode_for,
    provider_set::Rebuilt, visible_data, widget::apply_ui_change, widget_shape, App, AutoOpen,
    UserEvent,
};
use crate::{
    providers::ProviderData,
    rows,
    ui::picker::{PickerAction, PickerInput, PickerMode, ReadKind},
};
use tao::event_loop::EventLoopWindowTarget;
use tracing::info;

use super::cache::FileJob;

impl App {
    /// Queue the config for the background writer.
    pub(super) fn save_config(&self) {
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

    /// The tray icon follows the providers shown, and whether any is chosen.
    pub(super) fn sync_tray(&mut self) {
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

    /// Open the picker (or bring it forward), asking about `ask`'s sign-in
    /// at once when given.
    pub(super) fn open_picker(
        &mut self,
        target: &EventLoopWindowTarget<UserEvent>,
        ask: Option<String>,
    ) {
        // Opened now, by whatever path: an owed window that waited has had
        // its turn.
        self.auto_open_waiting = false;
        if self.picker.is_none() {
            self.picker_mode = picker_mode_for(&self.config);
            self.found_tools = crate::consent::found_tools();
            self.picker = open_picker_window(
                target,
                &self.renderer,
                self.picker_input(),
                picker_anchor(&self.window, self.window_visible),
            );
        } else if let Some(p) = self.picker.as_ref() {
            p.focus();
        }
        if let (Some(p), Some(id)) = (self.picker.as_mut(), ask) {
            p.ask_sign_in(&id);
        }
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
                self.open_picker(target, None);
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
        // The menu's question sends its Allow alone; a row's Allow comes
        // with the row it was asked for.
        let lone = actions.len() == 1;
        let mut changed = false;
        for action in actions {
            let done = crate::choices::apply(&mut self.config, &action);
            match &action {
                PickerAction::NoticeDone => self.picker_mode = PickerMode::Normal,
                PickerAction::Allow(ReadKind::SignIn(p))
                    if lone && self.menu_asked.as_deref() == Some(p.as_str()) =>
                {
                    crate::choices::show_provider_rows(&mut self.config, p);
                    self.menu_asked = None;
                }
                _ => {}
            }
            if done.close {
                if let Some(p) = self.picker.take() {
                    p.close();
                }
                self.menu_asked = None;
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
        // The menu's question is answered in the first actions after it
        // (Cancel sends none): whatever comes next is not it.
        self.menu_asked = None;
        if changed {
            self.save_config();
            self.relayout();
            self.sync_tray();
            self.panel.update(&self.visible(), true);
            self.menu.sync(&self.config, self.win_state.pinned);
            self.window.request_redraw();
        }
        self.refresh_picker();
    }
}
