// app/sign_in.rs — each provider's sign-in, from the picker's Sign-in block
// under its rows (it used to be the context menu's Providers submenu): the
// source it takes, a key or usage token of the user's own pasted from the
// clipboard (checked before it is stored, where the provider can be asked)
// or removed, and Cursor's sign-in of the app's own in the browser. What an
// action came to is said in the window; a toast says it only while the
// window is closed.

use super::{
    clipboard::{read_clipboard_secret, SecretKind},
    feedback, App, AppCommand,
};
use crate::{
    choices::{method_of, secret_label, sign_in_sources, source_of, SignInState},
    config::schema::AuthMethod,
    providers::{
        self,
        cursor::session::{self as cursor_session, SignInOutcome},
        key_label_for, usage_token_label_for, ProviderId, TokenCheck,
    },
    ui::picker::{CursorState, PickerAction, SourceId},
};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::warn;

/// Numbers the usage-token checks, so an answer can be matched to its paste.
static TOKEN_CHECKS: AtomicU64 = AtomicU64::new(0);

/// Every Credential Manager label of the app's own keys and tokens.
const SECRET_LABELS: [&str; 4] = [
    "claude_api_key",
    "copilot_pat",
    "claude_usage_token",
    "codex_usage_token",
];

/// What became of a pasted usage token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenOutcome {
    /// The endpoint took it (usage, or its rate limit); stored.
    Stored,
    /// No answer said either way (why, in words); stored all the same, as
    /// the owner chose: a provider that is down should not keep a working
    /// token out.
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

/// Which of the app's own keys and tokens are stored, asked of Credential
/// Manager by label (never enumerated).
fn stored_secrets() -> Vec<&'static str> {
    SECRET_LABELS
        .into_iter()
        .filter(|label| {
            keyring::Entry::new("ailimits", label)
                .and_then(|e| e.get_password())
                .is_ok()
        })
        .collect()
}

/// A key or token pasted before the source could be chosen took the lead
/// over the tool's own sign-in; the source now says so, so nothing changes
/// for the user. Returns whether the config changed.
pub(super) fn settle_sources(
    config: &mut crate::config::schema::Config,
    stored: &[&'static str],
) -> bool {
    let mut changed = false;
    for pc in config.providers.iter_mut() {
        if pc.auth_method != AuthMethod::Subscription {
            continue;
        }
        let pasted = match pc.id.as_str() {
            "claude" | "codex" => usage_token_label_for(&pc.id)
                .filter(|l| stored.contains(l))
                .map(|_| AuthMethod::UsageToken),
            "copilot" => key_label_for(&pc.id)
                .filter(|l| stored.contains(l))
                .map(|_| AuthMethod::ApiKey),
            _ => None,
        };
        if let Some(method) = pasted {
            if method == AuthMethod::ApiKey {
                pc.credential_label = key_label_for(&pc.id).unwrap_or_default().to_string();
            }
            pc.auth_method = method;
            changed = true;
        }
    }
    changed
}

/// A provider's name for the toasts.
fn name_of(pid: &str) -> &'static str {
    ProviderId::from_config_id(pid).map_or("AI Limits", |p| p.display_name())
}

impl App {
    /// What the sign-ins stand at, once at start: the stored keys and
    /// tokens, Cursor's session; a source left to the old order settled.
    pub(super) fn init_sign_in_state(&mut self) {
        let stored = stored_secrets();
        if settle_sources(&mut self.config, &stored) {
            self.save_config();
        }
        self.sign_in = SignInState {
            stored,
            cursor: Some(if cursor_session::is_stored() {
                CursorState::SignedIn
            } else {
                CursorState::SignedOut
            }),
            notes: Default::default(),
        };
    }

    /// The sign-in state the picker shows: Cursor waiting while its sign-in
    /// task runs.
    pub(super) fn sign_in_view(&self) -> SignInState {
        let mut state = self.sign_in.clone();
        if self.cursor_sign_in.is_some() {
            state.cursor = Some(CursorState::Waiting);
        }
        state
    }

    /// Say what a sign-in action came to: in the window, or as a toast
    /// while the window is closed.
    fn tell(&mut self, pid: &'static str, text: impl Into<String>) {
        let text = text.into();
        if self.picker.is_none() {
            feedback("AI Limits", &format!("{}: {text}", name_of(pid)));
        }
        self.sign_in.notes.insert(pid, text);
        self.refresh_picker();
    }

    /// One of the Sign-in block's actions (`PickerAction`). False for any
    /// other action.
    pub(super) fn on_sign_in_action(&mut self, action: &PickerAction) -> bool {
        match action {
            PickerAction::SetSignInSource { provider, source } => {
                self.set_sign_in_source(provider, *source)
            }
            PickerAction::PasteSecret(pid) => self.paste_secret(pid),
            PickerAction::RemoveSecret(pid) => self.remove_secret(pid),
            PickerAction::CursorSignIn => self.sign_in_cursor(),
            PickerAction::CursorCancel => self.cancel_cursor_sign_in(),
            PickerAction::CursorSignOut => self.sign_out_cursor(),
            _ => return false,
        }
        true
    }

    fn set_sign_in_source(&mut self, pid: &str, source: SourceId) {
        let Some(id) = ProviderId::from_config_id(pid).map(|p| p.config_id()) else {
            return;
        };
        if !sign_in_sources(id).contains(&source) {
            return;
        }
        if let Some(pc) = self.config.providers.iter_mut().find(|p| p.id == id) {
            pc.auth_method = method_of(source);
            pc.credential_label = match source {
                SourceId::Key => key_label_for(id).unwrap_or_default().to_string(),
                _ => String::new(),
            };
        }
        // A key or token of the user's own already stored: choosing it sets
        // the provider up with it, as its paste did.
        if secret_label(id, source).is_some_and(|l| self.sign_in.stored.contains(&l)) {
            self.config.allow_app_sign_in(id);
        }
        self.sign_in.notes.remove(id);
        // Another source may be another account: its numbers start over.
        self.sync_providers(Some(id));
        self.refresh_models();
        self.relayout();
        self.sync_tray();
        self.panel.update(&self.visible(), true);
        self.save_config();
        self.refresh_picker();
    }

    /// Paste the chosen source's key or token from the clipboard.
    fn paste_secret(&mut self, pid: &str) {
        let Some(pc) = self.config.providers.iter().find(|p| p.id == pid) else {
            return;
        };
        let Some(id) = ProviderId::from_config_id(pid).map(|p| p.config_id()) else {
            return;
        };
        match source_of(&pc.auth_method) {
            SourceId::UsageToken => self.paste_usage_token(id),
            SourceId::Key => self.paste_key(id),
            SourceId::Tool => {}
        }
    }

    fn remove_secret(&mut self, pid: &str) {
        let Some(pc) = self.config.providers.iter().find(|p| p.id == pid) else {
            return;
        };
        let Some(id) = ProviderId::from_config_id(pid).map(|p| p.config_id()) else {
            return;
        };
        match source_of(&pc.auth_method) {
            SourceId::UsageToken => self.remove_usage_token(id),
            SourceId::Key => self.remove_key(id),
            SourceId::Tool => {}
        }
    }

    // ─── Keys ────────────────────────────────────────────────────

    fn paste_key(&mut self, pid: &'static str) {
        // A key the app has no label for is never stored under an empty one.
        let pasted = key_label_for(pid)
            .zip(SecretKind::key_for(pid))
            .ok_or_else(|| anyhow::anyhow!("{pid} takes no key"))
            .and_then(|(label, kind)| Ok((label, read_clipboard_secret(kind)?)));
        let (label, key) = match pasted {
            Ok(pasted) => pasted,
            Err(e) => return self.tell(pid, format!("Not stored: {e}.")),
        };
        match keyring::Entry::new("ailimits", label).and_then(|e| e.set_password(&key)) {
            Ok(()) => {
                if let Some(pc) = self.config.providers.iter_mut().find(|p| p.id == pid) {
                    pc.enabled = true;
                    pc.credential_label = label.to_string();
                    pc.auth_method = AuthMethod::ApiKey;
                }
                if !self.sign_in.stored.contains(&label) {
                    self.sign_in.stored.push(label);
                }
                // A key of the user's own: that allows the key, not the
                // tool's files.
                self.set_up_with_app_sign_in(pid);
                self.tell(
                    pid,
                    "Stored in Windows Credential Manager. The row says whether it works.",
                );
            }
            Err(e) => {
                warn!("keyring store failed: {e}");
                self.tell(pid, "Windows Credential Manager did not take it.");
            }
        }
    }

    fn remove_key(&mut self, pid: &'static str) {
        let label = key_label_for(pid).unwrap_or_default();
        match keyring::Entry::new("ailimits", label).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => {
                self.sign_in.stored.retain(|l| *l != label);
                self.sync_providers(Some(pid));
                self.save_config();
                self.window.request_redraw();
                self.tell(pid, "Removed from Windows Credential Manager.");
            }
            Err(e) => {
                warn!("keyring delete failed: {e}");
                self.tell(pid, "Windows Credential Manager did not remove it.");
            }
        }
    }

    // ─── Usage tokens ────────────────────────────────────────────

    /// A usage token from the clipboard: its shape first, so nothing that
    /// is not one is ever sent; then one check against the provider's own
    /// usage endpoint (`providers::check_usage_token`) and, unless refused,
    /// Credential Manager - both in a task on the runtime, which the token
    /// never leaves. The event loop hears only the outcome
    /// (`AppCommand::UsageToken`, `on_usage_token`).
    fn paste_usage_token(&mut self, pid: &'static str) {
        let pasted = usage_token_label_for(pid)
            .zip(SecretKind::usage_token_for(pid))
            .ok_or_else(|| anyhow::anyhow!("{pid} takes no usage token"))
            .and_then(|(label, kind)| Ok((label, read_clipboard_secret(kind)?)));
        let (label, token) = match pasted {
            Ok(pasted) => pasted,
            Err(e) => return self.tell(pid, format!("Not stored: {e}.")),
        };
        if let Some(problem) = providers::usage_token_problem(pid, &token) {
            return self.tell(pid, format!("Not stored: {problem}."));
        }
        // A second paste while one is checked: the newer one counts.
        if let Some((_, task)) = self.usage_token_check.take() {
            task.abort();
        }
        let ticket = TOKEN_CHECKS.fetch_add(1, Ordering::Relaxed);
        let tx = self.cmd_tx.clone();
        let owned = pid.to_string();
        let task = self.runtime.spawn(async move {
            let check = providers::check_usage_token(&owned, &token).await;
            let outcome = TokenOutcome::of(check, || {
                keyring::Entry::new("ailimits", label).and_then(|e| e.set_password(&token))
            });
            let done = AppCommand::UsageToken {
                pid: owned,
                outcome,
                ticket,
            };
            let _ = tx.send(done).await;
        });
        self.usage_token_check = Some((ticket, task));
        self.tell(pid, format!("Checking it with {}...", name_of(pid)));
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
        let Some(id) = ProviderId::from_config_id(&pid).map(|p| p.config_id()) else {
            return;
        };
        match outcome {
            TokenOutcome::Stored | TokenOutcome::StoredUnchecked(_) => {
                if let Some(pc) = self.config.providers.iter_mut().find(|p| p.id == id) {
                    pc.enabled = true;
                    pc.auth_method = AuthMethod::UsageToken;
                    pc.credential_label.clear();
                }
                if let Some(label) = usage_token_label_for(id) {
                    if !self.sign_in.stored.contains(&label) {
                        self.sign_in.stored.push(label);
                    }
                }
                self.set_up_with_app_sign_in(id);
                let text = match outcome {
                    TokenOutcome::StoredUnchecked(why) => format!(
                        "Stored, but not checked: {why}. If the row shows an error, paste it again."
                    ),
                    _ => "It works. Stored in Windows Credential Manager.".to_string(),
                };
                self.tell(id, text);
            }
            TokenOutcome::Refused => {
                self.tell(
                    id,
                    format!("{} refused it, so it was not stored.", name_of(id)),
                );
            }
            TokenOutcome::StoreFailed => {
                self.tell(id, "Windows Credential Manager did not take it.");
            }
        }
    }

    fn remove_usage_token(&mut self, pid: &'static str) {
        // A paste still being checked would store its token after this:
        // the removal is the later wish.
        if let Some((_, task)) = self.usage_token_check.take() {
            task.abort();
        }
        let label = usage_token_label_for(pid).unwrap_or_default();
        match keyring::Entry::new("ailimits", label).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => {
                self.sign_in.stored.retain(|l| *l != label);
                // The token's account may not be the tool's: its numbers go,
                // and a fetch still in flight with it is dropped.
                self.sync_providers(Some(pid));
                self.window.request_redraw();
                self.tell(pid, "Removed from Windows Credential Manager.");
            }
            Err(e) => {
                warn!("keyring delete failed: {e}");
                self.tell(pid, "Windows Credential Manager did not remove it.");
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
                crate::platform::open_url(&request.url());
                let tx = self.cmd_tx.clone();
                self.cursor_sign_in = Some(self.runtime.spawn(async move {
                    let outcome = cursor_session::complete_sign_in(request).await;
                    let _ = tx.send(AppCommand::CursorSignIn(outcome)).await;
                }));
                self.sign_in.notes.remove("cursor");
                self.refresh_picker();
            }
            Err(e) => {
                warn!("cursor sign-in: {e}");
                self.tell("cursor", "The sign-in could not start. Try again.");
            }
        }
    }

    fn cancel_cursor_sign_in(&mut self) {
        if let Some(task) = self.cursor_sign_in.take() {
            task.abort();
        }
        self.tell("cursor", "Cancelled.");
    }

    /// The sign-in started from the window has ended.
    pub(super) fn on_cursor_sign_in(&mut self, outcome: SignInOutcome) {
        self.cursor_sign_in = None;
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
                self.sign_in.cursor = Some(CursorState::SignedIn);
                // The new session may be another account: no old numbers
                // stand in for it, and nothing still in flight for the old
                // one lands after it.
                self.sync_providers(Some(pid));
                self.refresh_models();
                self.relayout();
                self.sync_tray();
                self.panel.update(&self.visible(), true);
                self.save_config();
                self.tell(pid, "Signed in.");
            }
            SignInOutcome::TimedOut => {
                self.tell(
                    "cursor",
                    "The sign-in timed out after 5 minutes. Try again.",
                );
            }
            SignInOutcome::Failed(e) => {
                warn!("cursor sign-in failed: {e}");
                self.tell("cursor", "The sign-in did not work. Try again.");
            }
        }
    }

    fn sign_out_cursor(&mut self) {
        if let Some(task) = self.cursor_sign_in.take() {
            task.abort();
        }
        match cursor_session::delete() {
            Ok(()) => {
                self.sign_in.cursor = Some(CursorState::SignedOut);
                // Nothing of the session stays on screen, and nothing still
                // in flight for it lands later.
                self.sync_providers(Some("cursor"));
                self.relayout_if_reshaped();
                self.sync_tray();
                self.panel.update(&self.visible(), true);
                self.window.request_redraw();
                self.tell("cursor", "Signed out.");
            }
            Err(e) => {
                warn!("cursor sign-out failed: {e}");
                self.tell(
                    "cursor",
                    "Windows Credential Manager did not remove the session.",
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::Config;

    #[test]
    fn a_pasted_token_is_stored_unless_refused() {
        let mut stored = false;
        assert_eq!(
            TokenOutcome::of(TokenCheck::Accepted, || {
                stored = true;
                Ok(())
            }),
            TokenOutcome::Stored
        );
        assert!(stored);
        let mut stored = false;
        assert_eq!(
            TokenOutcome::of(TokenCheck::Refused, || {
                stored = true;
                Ok(())
            }),
            TokenOutcome::Refused
        );
        assert!(!stored, "a refused token is never stored");
        assert_eq!(
            TokenOutcome::of(TokenCheck::Unchecked("no answer".into()), || Ok(())),
            TokenOutcome::StoredUnchecked("no answer".into())
        );
        assert_eq!(
            TokenOutcome::of(TokenCheck::Accepted, || Err(keyring::Error::NoEntry)),
            TokenOutcome::StoreFailed
        );
    }

    /// A key or token pasted before the source could be chosen kept the
    /// lead it had: the source says so after the update; the tool's own
    /// sign-in stays where nothing was pasted.
    #[test]
    fn a_pasted_secret_keeps_its_lead_after_the_update() {
        let mut config = Config::default();
        assert!(!settle_sources(&mut config, &[]));
        let stored = ["claude_usage_token", "copilot_pat"];
        assert!(settle_sources(&mut config, &stored));
        let method = |id: &str| {
            config
                .providers
                .iter()
                .find(|p| p.id == id)
                .unwrap()
                .auth_method
                .clone()
        };
        assert_eq!(method("claude"), AuthMethod::UsageToken);
        assert_eq!(method("copilot"), AuthMethod::ApiKey);
        assert_eq!(method("codex"), AuthMethod::Subscription);
        // Once settled, nothing more changes.
        assert!(!settle_sources(&mut config, &stored));
        // A key chosen already stays the key.
        let mut config = Config::default();
        config.providers[0].auth_method = AuthMethod::ApiKey;
        assert!(!settle_sources(&mut config, &["claude_usage_token"]));
    }
}
