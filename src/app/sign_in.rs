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
        cursor::session::{
            self as cursor_session, Approval, OwnSession, SignInOutcome, StoreError,
        },
        key_label_for, usage_token_label_for, ProviderId, TokenCheck,
    },
    ui::picker::{CursorState, PickerAction, SourceId},
};
use tracing::warn;

/// The one task of a kind whose answer the event loop still waits for: the
/// check of a pasted usage token, or Cursor's sign-in. Each task is started
/// under a ticket of its own and sends it back with its answer. A task
/// does not store what it brings back: the event loop does, and only while
/// the answer's ticket is the one awaited. An answer from a task that was
/// cancelled, removed or replaced since is dropped, so a secret can never
/// be stored after the user took it back - not even by a task already past
/// its last await point when it was aborted.
pub(super) struct Awaited<H> {
    /// The last ticket handed out.
    issued: u64,
    /// The task awaited and its ticket.
    current: Option<(u64, H)>,
}

impl<H> Default for Awaited<H> {
    fn default() -> Self {
        Self {
            issued: 0,
            current: None,
        }
    }
}

impl<H> Awaited<H> {
    /// Start a task under a new ticket (`spawn` is handed it, to send back
    /// with the answer) and await it alone: the task awaited until now, if
    /// any, is returned to be aborted, and its answer no longer counts.
    pub(super) fn start(&mut self, spawn: impl FnOnce(u64) -> H) -> Option<H> {
        self.issued += 1;
        let ticket = self.issued;
        self.current
            .replace((ticket, spawn(ticket)))
            .map(|(_, h)| h)
    }

    /// Await nothing any more (Cancel, a removal, a sign-out): the task, to
    /// be aborted. Its answer, should one still come, no longer counts.
    pub(super) fn cancel(&mut self) -> Option<H> {
        self.current.take().map(|(_, h)| h)
    }

    /// Whether a task is awaited.
    pub(super) fn is_waiting(&self) -> bool {
        self.current.is_some()
    }

    /// An answer came under `ticket`: whether it is the one awaited. If it
    /// is, nothing is awaited any more.
    pub(super) fn answered(&mut self, ticket: u64) -> bool {
        let current = self.current.as_ref().is_some_and(|(t, _)| *t == ticket);
        if current {
            self.current = None;
        }
        current
    }
}

/// A pasted usage token on its way back from its check to the event loop,
/// which stores it there (`settle_token`). No debug print shows it.
pub struct PastedToken(String);

impl std::fmt::Debug for PastedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PastedToken(..)")
    }
}

/// What a checked paste's answer comes to: None, and nothing stored, when
/// the paste is no longer the one awaited (a later paste or a removal came
/// since); otherwise its outcome, the token stored with `store` unless the
/// check refused it.
fn settle_token<H>(
    awaited: &mut Awaited<H>,
    ticket: u64,
    check: TokenCheck,
    store: impl FnOnce() -> keyring::Result<()>,
) -> Option<TokenOutcome> {
    awaited
        .answered(ticket)
        .then(|| TokenOutcome::of(check, store))
}

/// What the end of Cursor's sign-in comes to: None, and nothing kept, when
/// it is no longer the sign-in awaited (cancelled, signed out of, or
/// started over since); otherwise its outcome, an approved session kept
/// with `store`.
fn settle_sign_in<H>(
    awaited: &mut Awaited<H>,
    ticket: u64,
    approval: Approval,
    store: impl FnOnce(&OwnSession) -> Result<(), StoreError>,
) -> Option<SignInOutcome> {
    awaited.answered(ticket).then(|| approval.keep_with(store))
}

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

/// Once, for a config from before the source could be chosen: a key or
/// token pasted then took the lead over the tool's own sign-in, and the
/// source now says so, so nothing changes for the user. Never again after
/// that (`Reads::sources_settled`): a source chosen in the window stays,
/// though a token is still stored. Returns whether the config changed.
pub(super) fn settle_sources_once(
    config: &mut crate::config::schema::Config,
    stored: &[&'static str],
) -> bool {
    if config.reads.sources_settled {
        return false;
    }
    settle_sources(config, stored);
    config.reads.sources_settled = true;
    true
}

/// A key or token pasted before the source could be chosen took the lead
/// over the tool's own sign-in; the source now says so. Returns whether a
/// source changed.
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
        if settle_sources_once(&mut self.config, &stored) {
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
        if self.cursor_sign_in.is_waiting() {
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
    /// usage endpoint (`providers::check_usage_token`), in a task on the
    /// runtime. The task stores nothing: it hands the token and the check
    /// back to the event loop (`AppCommand::UsageToken`, `on_usage_token`),
    /// which stores it there unless refused - and only if this paste is
    /// still the one awaited.
    fn paste_usage_token(&mut self, pid: &'static str) {
        let pasted = usage_token_label_for(pid)
            .zip(SecretKind::usage_token_for(pid))
            .ok_or_else(|| anyhow::anyhow!("{pid} takes no usage token"))
            .and_then(|(_, kind)| read_clipboard_secret(kind));
        let token = match pasted {
            Ok(token) => token,
            Err(e) => return self.tell(pid, format!("Not stored: {e}.")),
        };
        if let Some(problem) = providers::usage_token_problem(pid, &token) {
            return self.tell(pid, format!("Not stored: {problem}."));
        }
        let tx = self.cmd_tx.clone();
        let owned = pid.to_string();
        let runtime = &self.runtime;
        let replaced = self.usage_token_check.start(|ticket| {
            runtime.spawn(async move {
                let check = providers::check_usage_token(&owned, &token).await;
                let done = AppCommand::UsageToken {
                    pid: owned,
                    check,
                    token: PastedToken(token),
                    ticket,
                };
                let _ = tx.send(done).await;
            })
        });
        // A second paste while one is checked: the newer one counts.
        if let Some(task) = replaced {
            task.abort();
        }
        self.tell(pid, format!("Checking it with {}...", name_of(pid)));
    }

    /// A pasted usage token was checked (`paste_usage_token`): stored here
    /// unless refused. The answer to a paste that a later paste or a
    /// removal replaced is dropped, and its token with it.
    pub(super) fn on_usage_token(
        &mut self,
        pid: String,
        check: TokenCheck,
        token: PastedToken,
        ticket: u64,
    ) {
        let Some(id) = ProviderId::from_config_id(&pid).map(|p| p.config_id()) else {
            return;
        };
        let Some(label) = usage_token_label_for(id) else {
            return;
        };
        let store =
            || keyring::Entry::new("ailimits", label).and_then(|e| e.set_password(&token.0));
        let Some(outcome) = settle_token(&mut self.usage_token_check, ticket, check, store) else {
            return;
        };
        match outcome {
            TokenOutcome::Stored | TokenOutcome::StoredUnchecked(_) => {
                if let Some(pc) = self.config.providers.iter_mut().find(|p| p.id == id) {
                    pc.enabled = true;
                    pc.auth_method = AuthMethod::UsageToken;
                    pc.credential_label.clear();
                }
                if !self.sign_in.stored.contains(&label) {
                    self.sign_in.stored.push(label);
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
        // A paste still being checked is not stored after this, even if
        // its answer is already on its way: the removal is the later wish.
        if let Some(task) = self.usage_token_check.cancel() {
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
                crate::platform::open_url(&request.url());
                let tx = self.cmd_tx.clone();
                let runtime = &self.runtime;
                // The task only waits for the browser; the session it brings
                // back is kept by `on_cursor_sign_in`.
                let replaced = self.cursor_sign_in.start(|ticket| {
                    runtime.spawn(async move {
                        let approval = cursor_session::wait_for_approval(request).await;
                        let _ = tx.send(AppCommand::CursorSignIn { approval, ticket }).await;
                    })
                });
                // A second click starts over: the first link may have been
                // closed unseen.
                if let Some(task) = replaced {
                    task.abort();
                }
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
        if let Some(task) = self.cursor_sign_in.cancel() {
            task.abort();
        }
        self.tell("cursor", "Cancelled.");
    }

    /// The sign-in started from the window stopped waiting for the browser:
    /// an approved session is kept here. One cancelled, signed out of or
    /// started over since keeps nothing and says nothing.
    pub(super) fn on_cursor_sign_in(&mut self, approval: Approval, ticket: u64) {
        let Some(outcome) = settle_sign_in(
            &mut self.cursor_sign_in,
            ticket,
            approval,
            cursor_session::store,
        ) else {
            return;
        };
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
        // A sign-in still waiting keeps nothing after this, even if its
        // answer is already on its way.
        if let Some(task) = self.cursor_sign_in.cancel() {
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

    /// Only the task awaited counts: a later start replaces it, a cancel
    /// ends it, and each answer is taken once.
    #[test]
    fn only_the_awaited_task_is_answered() {
        let mut awaited = Awaited::default();
        assert!(!awaited.is_waiting());
        let mut first = 0;
        assert_eq!(awaited.start(|t| first = t), None);
        assert!(awaited.is_waiting());
        let mut second = 0;
        assert_eq!(awaited.start(|t| second = t), Some(()), "replaced");
        assert_ne!(first, second);
        assert!(!awaited.answered(first), "replaced since");
        assert!(awaited.is_waiting());
        assert!(awaited.answered(second));
        assert!(!awaited.is_waiting());
        assert!(!awaited.answered(second), "taken once");

        let mut third = 0;
        awaited.start(|t| third = t);
        assert_eq!(awaited.cancel(), Some(()));
        assert!(!awaited.is_waiting());
        assert!(!awaited.answered(third), "cancelled");
        assert_eq!(awaited.cancel(), None);
    }

    /// A checked token is stored by the event loop, unless refused, and
    /// only while its paste is the one awaited: a removal or a later paste
    /// made while it was checked leaves it unstored.
    #[test]
    fn a_checked_token_is_stored_only_while_its_paste_is_awaited() {
        let mut awaited = Awaited::default();
        let mut stored = false;
        let mut ticket = 0;
        awaited.start(|t| ticket = t);
        assert_eq!(
            settle_token(&mut awaited, ticket, TokenCheck::Accepted, || {
                stored = true;
                Ok(())
            }),
            Some(TokenOutcome::Stored)
        );
        assert!(stored);

        // Removed while it was checked.
        let mut stored = false;
        awaited.start(|t| ticket = t);
        let _ = awaited.cancel();
        assert_eq!(
            settle_token(&mut awaited, ticket, TokenCheck::Accepted, || {
                stored = true;
                Ok(())
            }),
            None
        );
        assert!(!stored, "a removal is the later wish");

        // Pasted again while it was checked: only the newer one is stored.
        let (mut older, mut newer) = (0, 0);
        awaited.start(|t| older = t);
        awaited.start(|t| newer = t);
        let mut stored = Vec::new();
        assert_eq!(
            settle_token(&mut awaited, older, TokenCheck::Accepted, || {
                stored.push("older");
                Ok(())
            }),
            None
        );
        assert_eq!(
            settle_token(
                &mut awaited,
                newer,
                TokenCheck::Unchecked("x".into()),
                || {
                    stored.push("newer");
                    Ok(())
                }
            ),
            Some(TokenOutcome::StoredUnchecked("x".into()))
        );
        assert_eq!(stored, ["newer"]);

        // Refused: awaited, and still not stored.
        let mut stored = false;
        awaited.start(|t| ticket = t);
        assert_eq!(
            settle_token(&mut awaited, ticket, TokenCheck::Refused, || {
                stored = true;
                Ok(())
            }),
            Some(TokenOutcome::Refused)
        );
        assert!(!stored);
        assert!(!format!("{:?}", PastedToken("sk-ant-secret".into())).contains("secret"));
    }

    /// Cursor's session is kept by the event loop only while its sign-in is
    /// the one awaited: Cancel, a sign-out or a new sign-in made while the
    /// browser was waited for keeps nothing, approved or not.
    #[test]
    fn a_cursor_session_is_kept_only_while_its_sign_in_is_awaited() {
        let approved = || {
            Approval::Approved(cursor_session::Tokens {
                access: "a.b.c".into(),
                refresh: "a.b.c".into(),
            })
        };
        let mut awaited = Awaited::default();
        let mut kept = Vec::new();
        let mut ticket = 0;

        awaited.start(|t| ticket = t);
        let outcome = settle_sign_in(&mut awaited, ticket, approved(), |s| {
            kept.push(s.access_token.clone());
            Ok(())
        });
        assert_eq!(outcome, Some(SignInOutcome::SignedIn(None)));
        assert_eq!(kept.len(), 1);
        assert!(!awaited.is_waiting(), "no longer waiting");

        // Cancelled, or signed out of, while the browser was waited for.
        awaited.start(|t| ticket = t);
        let _ = awaited.cancel();
        let outcome = settle_sign_in(&mut awaited, ticket, approved(), |s| {
            kept.push(s.access_token.clone());
            Ok(())
        });
        assert_eq!(outcome, None);
        assert_eq!(kept.len(), 1, "nothing kept after a Cancel");

        // Started over: the first link's answer keeps nothing.
        let (mut first, mut second) = (0, 0);
        awaited.start(|t| first = t);
        awaited.start(|t| second = t);
        let outcome = settle_sign_in(&mut awaited, first, approved(), |s| {
            kept.push(s.access_token.clone());
            Ok(())
        });
        assert_eq!(outcome, None);
        assert!(awaited.is_waiting(), "still waiting for the second");
        let outcome = settle_sign_in(&mut awaited, second, Approval::TimedOut, |_| {
            panic!("nothing to keep")
        });
        assert_eq!(outcome, Some(SignInOutcome::TimedOut));

        // Credential Manager refusing it: an honest failure.
        awaited.start(|t| ticket = t);
        let outcome = settle_sign_in(&mut awaited, ticket, approved(), |_| {
            Err(StoreError::Unavailable("refused".into()))
        });
        assert!(matches!(outcome, Some(SignInOutcome::Failed(_))));
        assert_eq!(kept.len(), 1);
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

    /// Settled once, never again: the tool's sign-in chosen in the window
    /// stays, though the old token is still stored.
    #[test]
    fn the_sources_are_settled_once() {
        let mut config = Config::default();
        let stored = ["claude_usage_token"];
        assert!(settle_sources_once(&mut config, &stored));
        assert!(config.reads.sources_settled);
        config.providers[0].auth_method = AuthMethod::Subscription;
        assert!(!settle_sources_once(&mut config, &stored));
        assert_eq!(config.providers[0].auth_method, AuthMethod::Subscription);
        // A fresh install has nothing to settle.
        assert!(Config::fresh_install().reads.sources_settled);
    }
}
