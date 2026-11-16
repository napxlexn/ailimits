// providers/copilot.rs — GitHub Copilot provider (subscription).
// See docs/en/PROVIDERS.md, section "GitHub Copilot".
//
// Single method: the internal quota endpoint copilot_internal/user (the same
// data VS Code shows). Token: a PAT from Credential Manager, or the gh CLI.

use super::{
    generation_of, status_words, transport_error, unless_superseded, Metric, MetricUnit,
    MetricWindow, Net, Provider, ProviderData, ProviderId, ProviderStatus, SecretSource,
};
use crate::config::schema::ProviderConfig;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Internal Copilot subscription quota endpoint — verified 2026-06-10, HTTP 200.
const COPILOT_INTERNAL_URL: &str = "https://api.github.com/copilot_internal/user";

/// The host whose token `gh` is asked for: the one the endpoint lives on.
/// Without it gh answers for `GH_HOST` or its first configured host — a
/// GitHub Enterprise token would then be sent to github.com.
const GH_HOST: &str = "github.com";

/// How long gh's answer stands: a token, or that it has none; and how long a
/// token the endpoint refused waits before gh is asked again. Spawning
/// gh.exe is the heaviest periodic operation: at most once per this, also
/// when it keeps failing.
const GH_TTL: Duration = Duration::from_secs(15 * 60);

/// What `gh auth token --hostname github.com` gave.
#[derive(Clone)]
pub(crate) enum GhToken {
    Token(String),
    /// gh could not be started: not installed, or not on PATH.
    NotInstalled,
    /// gh ran but gave no token: nobody signed in to github.com.
    NotSignedIn,
    /// gh did not finish in time and was killed.
    TimedOut,
}

// Written by hand so that no debug print can ever carry the token.
impl std::fmt::Debug for GhToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GhToken::Token(_) => f.write_str("Token(..)"),
            GhToken::NotInstalled => f.write_str("NotInstalled"),
            GhToken::NotSignedIn => f.write_str("NotSignedIn"),
            GhToken::TimedOut => f.write_str("TimedOut"),
        }
    }
}

/// Runs gh for a token — the real gh.exe, or a test's stand-in.
type GhRunner = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = GhToken> + Send>> + Send + Sync>;

/// Which token a request was made with: the answers to a refused one differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    /// A personal access token the user pasted.
    Pat,
    /// The token GitHub CLI gives.
    Gh,
}

/// gh's last answer and when it came.
struct GhState {
    at: Instant,
    outcome: GhOutcome,
}

#[derive(Clone)]
enum GhOutcome {
    /// A token to send.
    Token(String),
    /// Nothing to send until `GH_TTL` has passed: gh had no token, or the
    /// endpoint refused the one it gave. The row says why.
    Held(ProviderStatus),
}

pub struct CopilotProvider {
    config: ProviderConfig,
    /// Never follows a 3xx — keeps the token from leaking to a redirected
    /// host. The internal endpoint returns 200 directly.
    net: Net,
    /// The pasted PAT (Credential Manager, the provider's label).
    pat: SecretSource,
    gh: GhRunner,
    /// gh's last answer, for `GH_TTL`.
    gh_state: Mutex<Option<GhState>>,
    generation: &'static AtomicU64,
}

impl CopilotProvider {
    pub fn new(config: ProviderConfig) -> Self {
        let label = config.credential_label.clone();
        Self {
            config,
            net: Net::new(),
            pat: Box::new(move || {
                if label.is_empty() {
                    return None;
                }
                keyring::Entry::new("ailimits", &label)
                    .and_then(|e| e.get_password())
                    .ok()
            }),
            gh: Arc::new(|| Box::pin(gh_cli_token())),
            gh_state: Mutex::new(None),
            generation: generation_of(&ProviderId::Copilot),
        }
    }

    /// A provider that sends to a local test server, with a stand-in for
    /// the PAT and for gh.
    #[cfg(test)]
    fn for_test(config: ProviderConfig, server: &str, pat: Option<&str>, gh: GhRunner) -> Self {
        let pat = pat.map(str::to_string);
        Self {
            config,
            net: Net::local(server),
            pat: Box::new(move || pat.clone()),
            gh,
            gh_state: Mutex::new(None),
            generation: Box::leak(Box::new(AtomicU64::new(0))),
        }
    }

    fn data(&self, status: ProviderStatus, metrics: Vec<Metric>) -> ProviderData {
        ProviderData {
            id: self.id(),
            status,
            metrics,
            updated_at: Utc::now(),
            received_at: Some(std::time::Instant::now()),
        }
    }

    /// Usage with gh's token. gh is asked at most once per `GH_TTL`, its
    /// failures included, and a token the endpoint refused is not sent
    /// again until then: the row keeps the reason meanwhile.
    async fn fetch_with_gh(&self) -> ProviderData {
        let now = Instant::now();
        let standing = {
            let state = self.gh_state.lock().unwrap_or_else(PoisonError::into_inner);
            state
                .as_ref()
                .filter(|s| now.saturating_duration_since(s.at) < GH_TTL)
                .map(|s| s.outcome.clone())
        };
        let token = match standing {
            Some(GhOutcome::Token(token)) => token,
            Some(GhOutcome::Held(status)) => {
                debug!("copilot: gh's last answer stands; nothing sent");
                return self.data(status, vec![]);
            }
            None => match (self.gh)().await {
                GhToken::Token(token) => {
                    self.keep_gh(now, GhOutcome::Token(token.clone()));
                    token
                }
                other => {
                    let status = gh_failure_status(&other);
                    self.keep_gh(now, GhOutcome::Held(status.clone()));
                    return self.data(status, vec![]);
                }
            },
        };
        let (code, data) = self.fetch_via_internal(&token, TokenSource::Gh).await;
        if code.is_some_and(is_refusal) {
            // Refused: no new gh run and no request for `GH_TTL`.
            self.keep_gh(Instant::now(), GhOutcome::Held(data.status.clone()));
        }
        data
    }

    fn keep_gh(&self, at: Instant, outcome: GhOutcome) {
        *self.gh_state.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(GhState { at, outcome });
    }

    /// Internal subscription quota endpoint, and the HTTP status it answered
    /// (None when no answer came).
    async fn fetch_via_internal(
        &self,
        token: &str,
        source: TokenSource,
    ) -> (Option<u16>, ProviderData) {
        let resp = match self
            .net
            .http
            .get(self.net.url(COPILOT_INTERNAL_URL))
            // "token …" format, the same the Copilot extensions use.
            .header("Authorization", format!("token {token}"))
            .header("User-Agent", "ailimits-widget")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("copilot request failed: {e}");
                let words = transport_error(&e).to_string();
                return (None, self.data(ProviderStatus::NetworkError(words), vec![]));
            }
        };

        let code = resp.status().as_u16();
        // Only a 200 carries a body worth reading. One cut off mid-transfer
        // is an honest error row, never an Err the scheduler would only log.
        let body = if code == 200 {
            match resp.text().await {
                Ok(body) => body,
                Err(e) => {
                    warn!("copilot body read failed: {e}");
                    let words = transport_error(&e).to_string();
                    return (None, self.data(ProviderStatus::NetworkError(words), vec![]));
                }
            }
        } else {
            String::new()
        };
        let (status, metrics) = quota_outcome(code, &body, source);
        (Some(code), self.data(status, metrics))
    }
}

/// The endpoint refused the token or the account (401/403/404).
fn is_refusal(code: u16) -> bool {
    matches!(code, 401 | 403 | 404)
}

/// The row for a gh run that gave no token. Not installed or nobody signed
/// in: no sign-in here, so the row stays hidden like any provider's without
/// one; a hung gh is an honest, passing error.
fn gh_failure_status(gh: &GhToken) -> ProviderStatus {
    match gh {
        GhToken::TimedOut => ProviderStatus::NetworkError("gh timed out, retrying".to_string()),
        _ => ProviderStatus::NotConfigured,
    }
}

#[async_trait]
impl Provider for CopilotProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Copilot
    }

    async fn fetch(&self) -> Result<ProviderData> {
        if !self.config.enabled {
            return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
        }
        let started = self.generation.load(Ordering::SeqCst);
        // Token: PAT from Credential Manager, or gh's token, which is asked
        // of `gh` only when the user allowed it.
        let data = match (self.pat)() {
            Some(pat) => self.fetch_via_internal(&pat, TokenSource::Pat).await.1,
            None if self.config.tool_files => self.fetch_with_gh().await,
            None => self.data(ProviderStatus::NotConfigured, vec![]),
        };
        unless_superseded(data, started, self.generation)
    }
}

/// gh CLI token via `gh auth token --hostname github.com`, without a
/// console window flash.
async fn gh_cli_token() -> GhToken {
    let mut cmd = tokio::process::Command::new("gh");
    cmd.args(["auth", "token", "--hostname", GH_HOST]);
    #[cfg(target_os = "windows")]
    {
        // CREATE_NO_WINDOW: no console flash from a GUI app.
        cmd.creation_flags(0x0800_0000);
        // "gh" is resolved by std, not by CreateProcess: since Rust 1.58 the
        // lookup covers the widget's own directory, System32, the Windows
        // directory and PATH — never the working directory — so a "gh.exe"
        // planted in a launch folder cannot receive the live token on stdout.
        // The working directory is still pinned to the system directory so
        // gh itself (config discovery, its own child processes) never runs
        // relative to wherever the widget happened to be started from.
        if let Some(root) = std::env::var_os("SystemRoot") {
            cmd.current_dir(std::path::Path::new(&root).join("System32"));
        }
    }
    // Bound the wait: a hung gh.exe must not stall the fetch task, which the
    // scheduler joins before its next cycle — one hang would otherwise freeze
    // every provider's updates. kill_on_drop reaps the process on timeout.
    cmd.kill_on_drop(true);
    let output = match tokio::time::timeout(Duration::from_secs(15), cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            debug!("gh could not be started: {e}");
            return GhToken::NotInstalled;
        }
        Err(_) => {
            warn!("gh auth token timed out after 15s (killed)");
            return GhToken::TimedOut;
        }
    };
    if !output.status.success() {
        warn!("gh auth token failed (not logged in to {GH_HOST})");
        return GhToken::NotSignedIn;
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        GhToken::NotSignedIn
    } else {
        GhToken::Token(token)
    }
}

/// One copilot_internal answer → status + metrics. Free of I/O so every
/// branch is testable: any non-200 or unknown body is an honest error, never
/// numbers.
///
/// A refusal reads by where the token came from. A pasted PAT: "check the
/// key", the user's own key is the thing to look at. gh's token: a short
/// message the row shows as it is (the hover adds "check gh auth") — the
/// user gave no key. A 404 is an account without Copilot, whatever the
/// token: GitHub answers 404 for an account the endpoint knows nothing of.
pub fn quota_outcome(code: u16, body: &str, source: TokenSource) -> (ProviderStatus, Vec<Metric>) {
    // The internal API schema changed — an honest signal.
    let unrecognized = || {
        (
            ProviderStatus::NetworkError("unrecognized copilot_internal schema".to_string()),
            vec![],
        )
    };
    let words = |w: &str| (ProviderStatus::NetworkError(w.to_string()), vec![]);
    match (code, source) {
        (200, _) => match parse_copilot_quotas(body) {
            Ok(metrics) if !metrics.is_empty() => (ProviderStatus::Ok, metrics),
            Ok(_) => unrecognized(),
            Err(e) => {
                // serde names a position in the body, never its content.
                warn!("copilot_internal body is not JSON: {e}");
                unrecognized()
            }
        },
        (404, TokenSource::Gh) => words("no Copilot on this account"),
        (401, TokenSource::Gh) => words("gh token rejected"),
        (403, TokenSource::Gh) => words("no Copilot access"),
        (401 | 403 | 404, TokenSource::Pat) => (
            ProviderStatus::AuthError("token has no Copilot access".to_string()),
            vec![],
        ),
        (other, _) => {
            warn!("copilot_internal returned HTTP {other}");
            (ProviderStatus::NetworkError(status_words(other)), vec![])
        }
    }
}

/// Parse quota_snapshots into metrics.
///
/// Schema verified by a manual request on 2026-06-10 (free_limited_copilot plan):
/// {"copilot_plan": "individual", "quota_reset_date": "...", "quota_snapshots": {
///   "premium_interactions": {"entitlement": N, "remaining": N, "unlimited": bool, ...},
///   "chat": {...}, "completions": {...}}}
/// `remaining` can be negative (overage). Unlimited quotas are skipped.
pub fn parse_copilot_quotas(body: &str) -> Result<Vec<Metric>> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let snapshots = match value.get("quota_snapshots") {
        Some(s) if s.is_object() => s,
        _ => return Ok(vec![]),
    };

    // Quota reset date "YYYY-MM-DD", midnight UTC.
    let reset_at = value
        .get("quota_reset_date")
        .and_then(|v| v.as_str())
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));

    let mut metrics = Vec::new();
    let mut push_quota = |key: &str, label: &str| {
        let snap = match snapshots.get(key) {
            Some(s) if s.is_object() => s,
            _ => return,
        };
        // Unlimited — nothing to chart.
        if snap
            .get("unlimited")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return;
        }
        let entitlement = snap.get("entitlement").and_then(|v| v.as_u64());
        let remaining = snap.get("remaining").and_then(|v| v.as_i64());
        if let (Some(limit), Some(remaining)) = (entitlement, remaining) {
            if limit == 0 {
                return;
            }
            // remaining < 0 = overage → `used` clamps to the limit in the UI.
            let used = (limit as i64 - remaining).max(0) as u64;
            metrics.push(Metric {
                label: label.to_string(),
                used,
                limit: Some(limit),
                unit: MetricUnit::Requests,
                reset_at,
                window: MetricWindow::Session,
                unified_pool: false,
                extra_only: false,
            });
        }
    };

    // Order matters: the first metric drives the progress bar.
    push_quota("premium_interactions", "Premium");
    push_quota("chat", "Chat");
    push_quota("completions", "Completions");

    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::test_server::{Reply, TestServer};
    use std::sync::atomic::AtomicUsize;

    fn config(tool_files: bool) -> ProviderConfig {
        ProviderConfig {
            id: "copilot".to_string(),
            enabled: true,
            auth_method: crate::config::schema::AuthMethod::Subscription,
            credential_label: String::new(),
            alert_threshold: 80,
            tool_files,
        }
    }

    const QUOTA_BODY: &str = r#"{"quota_reset_date": "2099-01-01", "quota_snapshots": {
        "premium_interactions": {"entitlement": 50, "remaining": 10, "unlimited": false}}}"#;

    /// A stand-in for gh that counts its runs and always answers `answer`.
    fn counting_gh(answer: GhToken) -> (GhRunner, Arc<AtomicUsize>) {
        let runs = Arc::new(AtomicUsize::new(0));
        let counter = runs.clone();
        let gh: GhRunner = Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            let answer = answer.clone();
            Box::pin(async move { answer })
        });
        (gh, runs)
    }

    fn status_of(data: &ProviderData) -> String {
        format!("{:?}", data.status)
    }

    /// No pasted token and `gh` not allowed: nothing is run or sent.
    #[tokio::test]
    async fn without_tool_files_gh_is_not_asked() {
        let server = TestServer::start();
        let (gh, runs) = counting_gh(GhToken::Token("gh-token".into()));
        let p = CopilotProvider::for_test(config(false), &server.base, None, gh);
        let data = p.fetch().await.unwrap();
        assert!(matches!(data.status, ProviderStatus::NotConfigured));
        assert_eq!(runs.load(Ordering::SeqCst), 0);
        assert!(server.seen().is_empty());
        assert!(p.gh_state.lock().unwrap().is_none());
    }

    /// gh that gives no token is asked once per 15 minutes, not every cycle.
    #[tokio::test]
    async fn a_gh_without_a_token_is_asked_once_per_fifteen_minutes() {
        for answer in [
            GhToken::NotInstalled,
            GhToken::NotSignedIn,
            GhToken::TimedOut,
        ] {
            let server = TestServer::start();
            let (gh, runs) = counting_gh(answer.clone());
            let p = CopilotProvider::for_test(config(true), &server.base, None, gh);
            let first = status_of(&p.fetch().await.unwrap());
            for _ in 0..5 {
                assert_eq!(status_of(&p.fetch().await.unwrap()), first, "{answer:?}");
            }
            assert_eq!(runs.load(Ordering::SeqCst), 1, "{answer:?}");
            assert!(server.seen().is_empty());
        }
    }

    /// A token the endpoint refuses (an account without Copilot) is neither
    /// sent again nor asked of gh again for 15 minutes; the row says why.
    #[tokio::test]
    async fn a_refused_gh_token_waits_fifteen_minutes() {
        for (code, words) in [
            (404, "no Copilot on this account"),
            (401, "gh token rejected"),
            (403, "no Copilot access"),
        ] {
            let server = TestServer::start();
            server.route("/copilot_internal/user", vec![Reply::status(code)]);
            let (gh, runs) = counting_gh(GhToken::Token("gh-token".into()));
            let p = CopilotProvider::for_test(config(true), &server.base, None, gh);
            for _ in 0..4 {
                let data = p.fetch().await.unwrap();
                assert!(
                    matches!(&data.status, ProviderStatus::NetworkError(m) if m == words),
                    "{code}: {:?}",
                    data.status
                );
            }
            assert_eq!(runs.load(Ordering::SeqCst), 1);
            assert_eq!(server.count("/copilot_internal/user"), 1);
            // Once the 15 minutes are over, gh is asked again. (Skipped on a
            // machine up for less than that: the clock cannot go back.)
            let earlier = {
                let mut state = p.gh_state.lock().unwrap();
                let state = state.as_mut().unwrap();
                state.at.checked_sub(GH_TTL).inspect(|t| state.at = *t)
            };
            if earlier.is_some() {
                let _ = p.fetch().await.unwrap();
                assert_eq!(runs.load(Ordering::SeqCst), 2);
            }
        }
    }

    /// A working token: gh once, then one request per cycle.
    #[tokio::test]
    async fn a_working_gh_token_is_kept_and_sent_each_cycle() {
        let server = TestServer::start();
        server.route("/copilot_internal/user", vec![Reply::ok(QUOTA_BODY)]);
        let (gh, runs) = counting_gh(GhToken::Token("gh-token".into()));
        let p = CopilotProvider::for_test(config(true), &server.base, None, gh);
        for _ in 0..3 {
            assert!(matches!(
                p.fetch().await.unwrap().status,
                ProviderStatus::Ok
            ));
        }
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        let seen = server.seen();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[0].auth.as_deref(), Some("token gh-token"));
    }

    /// A pasted PAT goes first, and gh is never run for it.
    #[tokio::test]
    async fn a_pasted_pat_needs_no_gh() {
        let server = TestServer::start();
        server.route("/copilot_internal/user", vec![Reply::status(401)]);
        let (gh, runs) = counting_gh(GhToken::Token("gh-token".into()));
        let p = CopilotProvider::for_test(config(true), &server.base, Some("pat"), gh);
        let data = p.fetch().await.unwrap();
        assert!(matches!(&data.status, ProviderStatus::AuthError(_)));
        assert_eq!(runs.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn no_answer_reads_offline_and_keeps_the_token() {
        let server = TestServer::start();
        server.route(
            "/copilot_internal/user",
            vec![Reply::Hangup, Reply::ok(QUOTA_BODY)],
        );
        let (gh, runs) = counting_gh(GhToken::Token("gh-token".into()));
        let p = CopilotProvider::for_test(config(true), &server.base, None, gh);
        let data = p.fetch().await.unwrap();
        assert!(
            matches!(&data.status, ProviderStatus::NetworkError(m) if m == "offline, retrying")
        );
        assert!(matches!(
            p.fetch().await.unwrap().status,
            ProviderStatus::Ok
        ));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn gh_is_asked_for_github_com_only() {
        // The provider's own argument list, as the hosts test reads it.
        let src = include_str!("copilot.rs");
        assert!(src.contains(r#"cmd.args(["auth", "token", "--hostname", GH_HOST]);"#));
        assert_eq!(GH_HOST, "github.com");
        assert!(!format!("{:?}", GhToken::Token("secret".into())).contains("secret"));
    }
}
