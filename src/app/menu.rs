// app/menu.rs — the context menu's actions (the widget's, the panel's and
// the tray icon's menu are one), and the Cursor sign-in it starts.

use super::{
    clipboard::{read_clipboard_secret, SecretKind},
    feedback, App, AppCommand, UserEvent,
};
use crate::{
    config::schema::{AuthMethod, Config},
    providers::{
        self,
        cursor::session::{self as cursor_session, SignInOutcome},
        key_label_for, usage_token_label_for, TokenCheck,
    },
    ui::context_menu::MenuAction,
};
use std::sync::atomic::{AtomicU64, Ordering};
use tao::event_loop::{ControlFlow, EventLoopWindowTarget};
use tracing::warn;

impl App {
    /// A menu item was clicked.
    pub(super) fn on_menu(
        &mut self,
        id: muda::MenuId,
        target: &EventLoopWindowTarget<UserEvent>,
        control_flow: &mut ControlFlow,
    ) {
        let Some(action) = self.menu.action_for(&id) else {
            return;
        };
        match action {
            MenuAction::OpenPicker => self.open_picker(target, None, true),
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
                self.config.ui.show_forecast = !self.config.ui.show_forecast;
                self.menu.sync(&self.config, self.win_state.pinned);
                self.save_config();
                self.window.request_redraw();
            }
            MenuAction::ToggleProvider(pid) => self.toggle_provider(pid, target),
            MenuAction::SetAuthMethod(pid, method) => {
                if let Some(pc) = self.config.providers.iter_mut().find(|p| p.id == pid) {
                    pc.auth_method = method.clone();
                    match method {
                        // api_key needs a key label.
                        AuthMethod::ApiKey => {
                            pc.credential_label =
                                key_label_for(&pid).unwrap_or_default().to_string();
                        }
                        // The subscription reads local files, no key needed.
                        AuthMethod::Subscription => pc.credential_label.clear(),
                    }
                }
                self.sync_providers(Some(pid.as_str()));
                self.menu.sync(&self.config, self.win_state.pinned);
                self.save_config();
                self.window.request_redraw();
            }
            MenuAction::PasteKey(pid) => self.paste_key(pid),
            MenuAction::RemoveKey(pid) => self.remove_key(pid),
            MenuAction::PasteUsageToken(pid) => self.paste_usage_token(pid),
            MenuAction::RemoveUsageToken(pid) => self.remove_usage_token(pid),
            MenuAction::SignInCursor => self.sign_in_cursor(),
            MenuAction::SignOutCursor => self.sign_out_cursor(),
            MenuAction::SetDetail(d) => {
                self.config.ui.detail = d;
                self.relayout();
                self.save_config();
            }
            MenuAction::SetLayout(l) => {
                self.config.ui.layout = l;
                self.relayout();
                self.save_config();
            }
            MenuAction::SetWidthScale(w) => {
                self.config.ui.width_scale = w;
                self.relayout();
                self.save_config();
            }
            MenuAction::SetColumnFlow(f) => {
                self.config.ui.column_flow = f;
                self.relayout();
                self.save_config();
            }
            MenuAction::SetPalette(p) => {
                // Picking a palette disables monochrome.
                self.config.ui.palette = p;
                self.config.ui.monochrome = false;
                self.colours_changed();
            }
            MenuAction::ToggleMonochrome => {
                self.config.ui.monochrome = !self.config.ui.monochrome;
                self.colours_changed();
            }
            MenuAction::SetOpacity(pct) => {
                self.config.window.opacity = (pct as f32 / 100.0).clamp(0.10, 0.85);
                self.menu.sync(&self.config, self.win_state.pinned);
                self.save_config();
                self.window.request_redraw();
            }
            MenuAction::SetBrightness(v) => {
                self.config.ui.brightness = v;
                self.colours_changed();
            }
            MenuAction::SetSaturation(v) => {
                self.config.ui.saturation = v;
                self.colours_changed();
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
                // The updater task reads this before each check.
                self.auto_update_enabled
                    .store(self.config.general.auto_update, Ordering::Relaxed);
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

    fn toggle_provider(&mut self, pid: String, target: &EventLoopWindowTarget<UserEvent>) {
        if flip_provider(&mut self.config, &pid) == Flip::Ask {
            // Never allowed to read its sign-in: ask first, in the picker,
            // naming the file and the host. The tick stays off until the
            // answer.
            self.menu.sync(&self.config, self.win_state.pinned);
            self.menu_asked = Some(pid.clone());
            self.open_picker(target, Some(pid), true);
            return;
        }
        // Switched off: its cached numbers go with it; either way it starts
        // over and is fetched.
        self.sync_providers(Some(pid.as_str()));
        self.refresh_models();
        // A disabled provider disappears immediately → resize.
        self.relayout();
        self.sync_tray();
        self.panel.update(&self.visible(), true);
        self.save_config();
    }

    // ─── Keys and tokens of the app's own ────────────────────────

    fn paste_key(&mut self, pid: String) {
        // A key the menu has no label for is never stored under an empty one.
        let pasted = key_label_for(&pid)
            .zip(SecretKind::key_for(&pid))
            .ok_or_else(|| anyhow::anyhow!("{pid} takes no key"))
            .and_then(|(label, kind)| Ok((label, read_clipboard_secret(kind)?)));
        let (label, key) = match pasted {
            Ok(pasted) => pasted,
            Err(e) => return feedback("AI Limits", &format!("{e}")),
        };
        match keyring::Entry::new("ailimits", label).and_then(|e| e.set_password(&key)) {
            Ok(()) => {
                if let Some(pc) = self.config.providers.iter_mut().find(|p| p.id == pid) {
                    pc.enabled = true;
                    pc.credential_label = label.to_string();
                    // Claude: a key implies the api_key method. Copilot: the
                    // PAT is just a token source for the subscription.
                    if pid == "claude" {
                        pc.auth_method = AuthMethod::ApiKey;
                    }
                }
                // Pasting a key is the user setting the provider up with the
                // app's own key: that allows the key, not the tool's files.
                self.set_up_with_app_sign_in(&pid);
                feedback("AI Limits", &format!("Key for {pid} stored"));
            }
            Err(e) => {
                warn!("keyring store failed: {e}");
                feedback("AI Limits", "Failed to store the key");
            }
        }
    }

    fn remove_key(&mut self, pid: String) {
        let label = key_label_for(&pid).unwrap_or_default();
        match keyring::Entry::new("ailimits", label).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => {
                if let Some(pc) = self.config.providers.iter_mut().find(|p| p.id == pid) {
                    // Without a key everything runs on the subscription
                    // method (Claude Code files / the gh CLI token).
                    pc.credential_label.clear();
                    pc.auth_method = AuthMethod::Subscription;
                }
                self.sync_providers(Some(pid.as_str()));
                self.menu.sync(&self.config, self.win_state.pinned);
                self.save_config();
                self.window.request_redraw();
                feedback("AI Limits", &format!("Key for {pid} removed"));
            }
            Err(e) => {
                warn!("keyring delete failed: {e}");
                feedback("AI Limits", "Failed to remove the key");
            }
        }
    }

    /// A usage token from the clipboard: its shape first, so nothing that
    /// is not one is ever sent; then one check against the provider's own
    /// usage endpoint (`providers::check_usage_token`) and, unless refused,
    /// Credential Manager - both in a task on the runtime, which the token
    /// never leaves. The event loop hears only the outcome
    /// (`AppCommand::UsageToken`, `on_usage_token`).
    fn paste_usage_token(&mut self, pid: String) {
        let pasted = usage_token_label_for(&pid)
            .zip(SecretKind::usage_token_for(&pid))
            .ok_or_else(|| anyhow::anyhow!("{pid} takes no usage token"))
            .and_then(|(label, kind)| Ok((label, read_clipboard_secret(kind)?)));
        let (label, token) = match pasted {
            Ok(pasted) => pasted,
            Err(e) => return feedback("AI Limits", &format!("{e}")),
        };
        if let Some(problem) = providers::usage_token_problem(&pid, &token) {
            return feedback(
                "AI Limits",
                &format!("Usage token for {pid} not stored: {problem}"),
            );
        }
        // A second paste while one is checked: the newer one counts.
        if let Some((_, task)) = self.usage_token_check.take() {
            task.abort();
        }
        let ticket = TOKEN_CHECKS.fetch_add(1, Ordering::Relaxed);
        let tx = self.cmd_tx.clone();
        let task = self.runtime.spawn(async move {
            let check = providers::check_usage_token(&pid, &token).await;
            let outcome = TokenOutcome::of(check, || {
                keyring::Entry::new("ailimits", label).and_then(|e| e.set_password(&token))
            });
            let done = AppCommand::UsageToken {
                pid,
                outcome,
                ticket,
            };
            let _ = tx.send(done).await;
        });
        self.usage_token_check = Some((ticket, task));
    }

    /// A pasted usage token was checked (`paste_usage_token`). The answer
    /// to a paste that a later paste or a removal replaced is not acted on.
    pub(super) fn on_usage_token(&mut self, pid: String, outcome: TokenOutcome, ticket: u64) {
        match self.usage_token_check.take() {
            Some((current, _)) if current == ticket => {}
            other => {
                self.usage_token_check = other;
                return;
            }
        }
        match outcome {
            TokenOutcome::Stored | TokenOutcome::StoredUnchecked(_) => {
                // The usage token is the app's own source, tried before the
                // tool's files: storing it sets the provider up the way a key
                // does.
                if let Some(pc) = self.config.providers.iter_mut().find(|p| p.id == pid) {
                    pc.enabled = true;
                }
                self.set_up_with_app_sign_in(&pid);
                let body = match outcome {
                    TokenOutcome::StoredUnchecked(why) => format!(
                        "Usage token for {pid} stored, but not checked: {why}. If the row shows an error, paste it again."
                    ),
                    _ => format!("Usage token for {pid} stored"),
                };
                feedback("AI Limits", &body);
            }
            TokenOutcome::Refused => feedback(
                "AI Limits",
                &format!("The usage token for {pid} was refused, so it was not stored"),
            ),
            TokenOutcome::StoreFailed => {
                feedback("AI Limits", "Failed to store the usage token");
            }
        }
    }

    fn remove_usage_token(&mut self, pid: String) {
        // A paste still being checked would store its token after this:
        // the removal is the later wish.
        if let Some((_, task)) = self.usage_token_check.take() {
            task.abort();
        }
        let label = usage_token_label_for(&pid).unwrap_or_default();
        match keyring::Entry::new("ailimits", label).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => {
                // The token's account may not be the tool's: its numbers go,
                // and a fetch still in flight with it is dropped.
                self.sync_providers(Some(pid.as_str()));
                self.window.request_redraw();
                feedback("AI Limits", &format!("Usage token for {pid} removed"));
            }
            Err(e) => {
                warn!("keyring delete failed: {e}");
                feedback("AI Limits", "Failed to remove the usage token");
            }
        }
    }

    // ─── Cursor's sign-in ────────────────────────────────────────

    fn sign_in_cursor(&mut self) {
        match cursor_session::LoginRequest::new() {
            Ok(request) => {
                // A second click starts over: the first link may have been
                // closed unseen.
                if let Some(task) = self.cursor_sign_in.take() {
                    task.abort();
                }
                // Opened from this thread, like the project page.
                crate::platform::open_url(&request.url());
                let tx = self.cmd_tx.clone();
                self.cursor_sign_in = Some(self.runtime.spawn(async move {
                    let outcome = cursor_session::complete_sign_in(request).await;
                    let _ = tx.send(AppCommand::CursorSignIn(outcome)).await;
                }));
            }
            Err(e) => {
                warn!("cursor sign-in: {e}");
                feedback("AI Limits", "Cursor sign-in failed");
            }
        }
    }

    /// The sign-in started from the menu has ended.
    pub(super) fn on_cursor_sign_in(&mut self, outcome: SignInOutcome) {
        match outcome {
            SignInOutcome::SignedIn(_) => {
                let pid = "cursor";
                // Signing in says the user wants the row, and the session is
                // the app's own: nothing else of Cursor's is read for it.
                if let Some(pc) = self.config.providers.iter_mut().find(|p| p.id == pid) {
                    pc.enabled = true;
                }
                self.config.allow_app_sign_in(pid);
                crate::choices::show_provider_rows(&mut self.config, pid);
                // The new session may be another account: no old numbers
                // stand in for it, and nothing still in flight for the old
                // one lands after it.
                self.sync_providers(Some(pid));
                self.refresh_models();
                self.relayout();
                self.sync_tray();
                self.panel.update(&self.visible(), true);
                self.menu.set_cursor_signed_in(true);
                self.save_config();
                feedback("AI Limits", "Signed in to Cursor");
            }
            SignInOutcome::TimedOut => {
                feedback("AI Limits", "Cursor sign-in timed out");
            }
            SignInOutcome::Failed(e) => {
                warn!("cursor sign-in failed: {e}");
                feedback("AI Limits", "Cursor sign-in failed");
            }
        }
    }

    fn sign_out_cursor(&mut self) {
        if let Some(task) = self.cursor_sign_in.take() {
            task.abort();
        }
        match cursor_session::delete() {
            Ok(()) => {
                // Nothing of the session stays on screen, and nothing still
                // in flight for it lands later: the row reloads from whatever
                // else it may use, or hides.
                self.sync_providers(Some("cursor"));
                self.relayout_if_reshaped();
                self.sync_tray();
                self.panel.update(&self.visible(), true);
                self.menu.set_cursor_signed_in(false);
                self.window.request_redraw();
                feedback("AI Limits", "Signed out of Cursor");
            }
            Err(e) => {
                warn!("cursor sign-out failed: {e}");
                feedback("AI Limits", "Failed to sign out of Cursor");
            }
        }
    }
}

/// Numbers the usage-token checks, so an answer can be matched to its paste.
static TOKEN_CHECKS: AtomicU64 = AtomicU64::new(0);

/// What became of a pasted usage token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenOutcome {
    /// The endpoint took it (usage, or its rate limit); stored.
    Stored,
    /// No answer said either way (why, in words for the toast); stored all
    /// the same, as the owner chose: a provider that is down should not
    /// keep a working token out.
    StoredUnchecked(String),
    /// The endpoint refused it (401/403): not stored.
    Refused,
    /// Credential Manager did not take it.
    StoreFailed,
}

impl TokenOutcome {
    /// The outcome of `check`, storing the token with `store` unless it was
    /// refused.
    fn of(check: TokenCheck, store: impl FnOnce() -> keyring::Result<()>) -> Self {
        if check == TokenCheck::Refused {
            return TokenOutcome::Refused;
        }
        if let Err(e) = store() {
            warn!("keyring store failed: {e}");
            return TokenOutcome::StoreFailed;
        }
        match check {
            TokenCheck::Unchecked(why) => TokenOutcome::StoredUnchecked(why),
            _ => TokenOutcome::Stored,
        }
    }
}

/// What a click on a provider's tick in the Providers menu did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flip {
    /// Never allowed to read anything: the click is a question to ask, and
    /// nothing changed.
    Ask,
    /// It runs now, and its own row is back.
    On,
    /// It no longer runs; its consent stays.
    Off,
    /// No such provider in the config.
    Missing,
}

/// The tick shows whether the provider runs (switched on and allowed), so a
/// click flips exactly that.
fn flip_provider(config: &mut Config, pid: &str) -> Flip {
    if !config.sign_in_allowed(pid) && !config.app_sign_in_allowed(pid) {
        return Flip::Ask;
    }
    let was_active = config
        .providers
        .iter()
        .find(|p| p.id == pid)
        .map(|p| config.provider_active(p));
    let Some(pc) = config.providers.iter_mut().find(|p| p.id == pid) else {
        return Flip::Missing;
    };
    pc.enabled = was_active != Some(true);
    if pc.enabled {
        crate::choices::show_provider_rows(config, pid);
        Flip::On
    } else {
        Flip::Off
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_provider_never_allowed_is_asked_about_not_switched_on() {
        let mut config = Config::default();
        let before: Vec<bool> = config.providers.iter().map(|p| p.enabled).collect();
        assert_eq!(flip_provider(&mut config, "codex"), Flip::Ask);
        let after: Vec<bool> = config.providers.iter().map(|p| p.enabled).collect();
        assert_eq!(before, after);
        assert!(!config.sign_in_allowed("codex"));
    }

    #[test]
    fn the_tick_flips_whether_an_allowed_provider_runs() {
        let mut config = Config::default();
        config.allow_sign_in("codex");
        assert_eq!(flip_provider(&mut config, "codex"), Flip::Off);
        assert_eq!(flip_provider(&mut config, "codex"), Flip::On);
        // The consent stays through both clicks.
        assert!(config.sign_in_allowed("codex"));
    }

    #[test]
    fn switched_back_on_its_own_row_returns() {
        let mut config = Config::default();
        config.allow_app_sign_in("cursor");
        config.ui.rows.widget = Some(vec!["claude".into()]);
        assert_eq!(flip_provider(&mut config, "cursor"), Flip::Off);
        assert_eq!(flip_provider(&mut config, "cursor"), Flip::On);
        assert!(crate::rows::row_chosen(
            &config,
            crate::config::schema::Surface::Widget,
            "cursor"
        ));
    }

    /// A refused token is never stored; any other check stores it, saying
    /// when it could not be checked; a store that fails says so.
    #[test]
    fn a_pasted_token_is_stored_unless_refused() {
        let stored = std::cell::Cell::new(0);
        let store = || {
            stored.set(stored.get() + 1);
            Ok(())
        };
        assert_eq!(
            TokenOutcome::of(TokenCheck::Refused, store),
            TokenOutcome::Refused
        );
        assert_eq!(stored.get(), 0);
        assert_eq!(
            TokenOutcome::of(TokenCheck::Accepted, store),
            TokenOutcome::Stored
        );
        let why = "the usage endpoint answered HTTP 503".to_string();
        assert_eq!(
            TokenOutcome::of(TokenCheck::Unchecked(why.clone()), store),
            TokenOutcome::StoredUnchecked(why)
        );
        assert_eq!(stored.get(), 2);
        let failing = || Err(keyring::Error::NoEntry);
        assert_eq!(
            TokenOutcome::of(TokenCheck::Accepted, failing),
            TokenOutcome::StoreFailed
        );
    }

    #[test]
    fn a_provider_missing_from_the_config_changes_nothing() {
        let mut config = Config::default();
        config.allow_sign_in("someday");
        assert_eq!(flip_provider(&mut config, "someday"), Flip::Missing);
    }
}
