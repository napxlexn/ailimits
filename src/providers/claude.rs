// providers/claude.rs — Claude provider (API key and subscription modes).
// See docs/en/PROVIDERS.md, section "Claude".

use super::{
    keyring_password, keyring_secret, pasted_key, read_text, status_words, transport_error, Metric,
    MetricUnit, MetricWindow, Net, Provider, ProviderData, ProviderId, ProviderStatus, Refusals,
    SecretSource, MAX_ANSWER_BYTES,
};
use crate::config::{
    paths::ToolPaths,
    schema::{AuthMethod, ProviderConfig},
};
use crate::utc::{self, Timestamp};
use anyhow::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

const CLAUDE_MODELS_URL: &str = "https://api.anthropic.com/v1/models";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Undocumented OAuth subscription usage endpoint — verified 2026-06-10, HTTP 200.
const OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";
/// Credential Manager label of the manual usage token.
const USAGE_TOKEN_LABEL: &str = "claude_usage_token";

/// Anthropic API rate limit headers.
const HEADER_TOKENS_REMAINING: &str = "anthropic-ratelimit-tokens-remaining";
const HEADER_TOKENS_LIMIT: &str = "anthropic-ratelimit-tokens-limit";
const HEADER_TOKENS_RESET: &str = "anthropic-ratelimit-tokens-reset";
const HEADER_REQUESTS_REMAINING: &str = "anthropic-ratelimit-requests-remaining";
const HEADER_REQUESTS_LIMIT: &str = "anthropic-ratelimit-requests-limit";
const HEADER_REQUESTS_RESET: &str = "anthropic-ratelimit-requests-reset";

pub struct ClaudeProvider {
    config: ProviderConfig,
    /// The client and where it sends (`Net::new` in the app). It never
    /// follows a redirect: reqwest strips only Authorization on one, never
    /// x-api-key.
    net: Net,
    /// The API key the user pasted (Credential Manager), read each cycle
    /// under Claude's own label only (`providers::own_key_label`).
    api_key: SecretSource,
    /// The app's own usage token (Credential Manager), read each cycle; it
    /// does not touch Claude Code's credentials.
    usage_token: SecretSource,
    /// Claude Code's sign-in, `.credentials.json` (config/paths.rs), read
    /// when allowed. None without an absolute home folder.
    credentials: Option<PathBuf>,
    /// A stored usage token the endpoint refused: not sent again for a
    /// while, so it costs no second request each cycle.
    refused: Refusals,
}

impl ClaudeProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            api_key: pasted_key(
                ProviderId::Claude,
                &config.credential_label,
                keyring_password,
            ),
            config,
            net: Net::new(),
            usage_token: keyring_secret(USAGE_TOKEN_LABEL),
            credentials: ToolPaths::from_env().map(|p| p.claude_credentials()),
            refused: Refusals::default(),
        }
    }

    /// A provider that sends to a local test server and finds its sources
    /// where the test put them: `home` stands for Claude Code's folder.
    #[cfg(test)]
    fn for_test(
        config: ProviderConfig,
        server: &str,
        usage_token: Option<&str>,
        home: Option<PathBuf>,
    ) -> Self {
        let token = usage_token.map(str::to_string);
        Self {
            // No test reads Credential Manager: a key is handed in by the
            // test that needs one.
            api_key: Box::new(|| None),
            config,
            net: Net::local(server),
            usage_token: Box::new(move || token.clone()),
            credentials: home.map(|h| h.join(".credentials.json")),
            refused: Refusals::default(),
        }
    }

    /// API key from Windows Credential Manager, under Claude's own label.
    fn get_api_key(&self) -> Option<String> {
        (self.api_key)()
    }

    /// Method A: API key → rate limit headers.
    async fn fetch_via_api_key(&self, api_key: &str) -> Result<ProviderData> {
        let resp = match self
            .net
            .http
            .get(self.net.url(CLAUDE_MODELS_URL))
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("claude models request failed: {e}");
                let words = transport_error(&e).to_string();
                return Ok(self.data(ProviderStatus::NetworkError(words), vec![]));
            }
        };

        // 401 → invalid key; 429 → limit exhausted but the headers are still present.
        let code = resp.status().as_u16();
        if code == 401 {
            return Ok(self.data(
                ProviderStatus::AuthError("invalid API key".to_string()),
                vec![],
            ));
        }

        // Each header is an Option: /v1/models may lack the token headers.
        let mut metrics = Vec::new();

        if let (Some(limit), Some(remaining)) = (
            header_u64(&resp, HEADER_REQUESTS_LIMIT),
            header_u64(&resp, HEADER_REQUESTS_REMAINING),
        ) {
            metrics.push(Metric {
                label: "Requests".to_string(),
                used: limit.saturating_sub(remaining),
                limit: Some(limit),
                unit: MetricUnit::Requests,
                reset_at: header_datetime(&resp, HEADER_REQUESTS_RESET),
                window: MetricWindow::Session,
                unified_pool: false,
                extra_only: false,
            });
        }

        if let (Some(limit), Some(remaining)) = (
            header_u64(&resp, HEADER_TOKENS_LIMIT),
            header_u64(&resp, HEADER_TOKENS_REMAINING),
        ) {
            metrics.push(Metric {
                label: "Tokens".to_string(),
                used: limit.saturating_sub(remaining),
                limit: Some(limit),
                unit: MetricUnit::Tokens,
                reset_at: header_datetime(&resp, HEADER_TOKENS_RESET),
                window: MetricWindow::Session,
                unified_pool: false,
                extra_only: false,
            });
        }

        if metrics.is_empty() {
            // A 200 without the headers is a changed answer; any other status
            // is named for what it is.
            let words = if code == 200 {
                "no ratelimit headers in response".to_string()
            } else {
                status_words(code)
            };
            return Ok(self.data(ProviderStatus::NetworkError(words), vec![]));
        }

        Ok(self.data(ProviderStatus::Ok, metrics))
    }

    /// Method B: the usage token the user pasted (`use_usage_token`), or
    /// Claude Code's own sign-in: the source chosen, and only that one.
    ///
    /// - the usage token: /api/oauth/usage; refused, it reads "usage token
    ///   rejected" and is left out for `REFUSED_TOKEN_PAUSE`;
    /// - Claude Code's token: /api/oauth/usage, live server-side %;
    /// - an honest NetworkError when the source exists but has no fresh
    ///   data; NotConfigured when there is none ("run Claude Code").
    ///
    /// One usage request per cycle. A rate limit, a server error or no
    /// answer ends the cycle on its honest status: none of them says the
    /// token is bad.
    ///
    /// (A statusline.jsonl snapshot used to sit between 1 and 2. Claude Code
    /// stopped writing that file in 2026; the reader went with 0.7.0.)
    async fn fetch_via_subscription(&self, use_usage_token: bool) -> Result<ProviderData> {
        let credentials_path = self.credentials.clone();
        // The source the user chose, and only that one: the usage token they
        // pasted, or Claude Code's own sign-in.
        let usage_token = if use_usage_token {
            (self.usage_token)()
        } else {
            None
        };
        // The token sources this machine has; stats-cache holds no limits and
        // does not count.
        // Claude Code's file only when the user allowed reading it and chose
        // it; the stored usage token is the app's own.
        let tool_files = self.config.tool_files && !use_usage_token;
        let mut chain = ChainOutcome {
            usage_token: usage_token.is_some(),
            credentials_file: tool_files && credentials_path.as_ref().is_some_and(|p| p.exists()),
            ..ChainOutcome::default()
        };

        // 0. The usage token the user pasted, when it is the source chosen:
        // a minimal read-only usage request. A refusal is said as one; a
        // token refused lately is not sent again (`REFUSED_TOKEN_PAUSE`).
        if let Some(token) = usage_token {
            let refused = if self.refused.paused(&token) {
                debug!("the stored Claude usage token was refused lately; not sent");
                true
            } else {
                match self.fetch_via_oauth(&token, &mut chain).await {
                    Ok(data) => return Ok(data),
                    Err(Miss::Refused) => {
                        self.refused.note(&token);
                        warn!("the stored Claude usage token was refused");
                        true
                    }
                    Err(Miss::Other) => return Ok(self.data(chain.status(), vec![])),
                }
            };
            if refused {
                return Ok(self.data(
                    ProviderStatus::AuthError("usage token rejected".to_string()),
                    vec![],
                ));
            }
        }

        // 1. OAuth: the Claude Code token is READ-ONLY — never refresh it,
        // refresh-token rotation would log Claude Code out.
        if let Some(path) = credentials_path.filter(|_| tool_files) {
            match read_oauth_token(&path).await {
                Ok(Some(token)) => {
                    // Rejected, unanswered or unreadable — fall back.
                    if let Ok(data) = self.fetch_via_oauth(&token, &mut chain).await {
                        return Ok(data);
                    }
                }
                Ok(None) => { /* no file, or an expired token — fall back */ }
                Err(e) => warn!(".credentials.json parse error: {e}"),
            }
        }

        // 2./3. No source produced data: an honest error, or a hidden row
        // when there is no source at all.
        Ok(self.data(chain.status(), vec![]))
    }

    /// One usage request. Err = no data from this token, fall back; why is
    /// noted in `chain`, so the caller can word its error honestly.
    async fn fetch_via_oauth(
        &self,
        token: &str,
        chain: &mut ChainOutcome,
    ) -> std::result::Result<ProviderData, Miss> {
        let resp = match self
            .net
            .http
            .get(self.net.url(OAUTH_USAGE_URL))
            .bearer_auth(token)
            .header("anthropic-beta", OAUTH_BETA_HEADER)
            .send()
            .await
        {
            Ok(r) => r,
            // No answer at all, which says nothing about the token.
            Err(e) => {
                warn!("oauth usage request failed: {e}");
                chain.no_answer = Some(transport_error(&e));
                return Err(Miss::Other);
            }
        };

        match resp.status().as_u16() {
            200 => {
                let body = match read_text(resp, MAX_ANSWER_BYTES).await {
                    Ok(body) => body,
                    Err(e) => {
                        warn!("oauth usage body read failed: {e}");
                        chain.no_answer = Some(e.words());
                        return Err(Miss::Other);
                    }
                };
                match parse_oauth_usage(&body) {
                    Ok(metrics) if !metrics.is_empty() => {
                        Ok(self.data(ProviderStatus::Ok, metrics))
                    }
                    // Schema changed — an honest error beats invented numbers.
                    parsed => {
                        if let Err(e) = parsed {
                            // serde names a position in the body, never its content.
                            warn!("oauth usage body is not JSON: {e}");
                        }
                        chain.unrecognized = true;
                        Err(Miss::Other)
                    }
                }
            }
            // 401/403 → a stale token.
            401 | 403 => {
                warn!("oauth usage endpoint refused the token, falling back");
                Err(Miss::Refused)
            }
            // The endpoint is undocumented, so any other non-200 falls back
            // quietly, noted for the row: a rate limit, a server error.
            status => {
                if status == 429 {
                    chain.rate_limited = true;
                } else {
                    chain.other_status = Some(status);
                }
                warn!("oauth usage endpoint returned HTTP {status}, falling back");
                Err(Miss::Other)
            }
        }
    }

    fn data(&self, status: ProviderStatus, metrics: Vec<Metric>) -> ProviderData {
        ProviderData {
            id: self.id(),
            status,
            metrics,
            updated_at: utc::now(),
            // Live fetch — monotonically anchored against wall-clock jumps.
            received_at: Some(std::time::Instant::now()),
        }
    }
}

/// One read-only usage request to check a pasted usage token before it is
/// stored (`providers::check_usage_token`).
pub(crate) async fn check_usage_token(net: &Net, token: &str) -> super::TokenCheck {
    super::token_check(
        net.http
            .get(net.url(OAUTH_USAGE_URL))
            .bearer_auth(token)
            .header("anthropic-beta", OAUTH_BETA_HEADER)
            .send()
            .await,
    )
}

/// Why a usage request brought no data.
enum Miss {
    /// 401/403: the token is not accepted.
    Refused,
    /// Anything else, noted in the `ChainOutcome`.
    Other,
}

/// What the subscription source chain learned in a cycle that produced no
/// data — enough to word the row honestly.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ChainOutcome {
    /// A manual usage token is stored in Credential Manager.
    pub usage_token: bool,
    /// Claude Code's `.credentials.json` exists.
    pub credentials_file: bool,
    /// A usage request bounced off the endpoint's rate limiter (HTTP 429).
    pub rate_limited: bool,
    /// A usage request got no answer (offline, DNS, a timeout, a refused
    /// certificate, a body cut off): the row's words for it.
    pub no_answer: Option<&'static str>,
    /// Any other status that says nothing about the token: a server error
    /// (5xx, 529 "overloaded"), or one the endpoint never gave before.
    pub other_status: Option<u16>,
    /// A 200 carried a body the parser does not know.
    pub unrecognized: bool,
}

impl ChainOutcome {
    /// The status the cycle ends on.
    ///
    /// Any source — the stored usage token as much as the credentials file —
    /// makes it an HONEST error instead of invented numbers: app.rs keeps the
    /// last real data on screen and the renderer greys it with its age. Only
    /// with no source at all does the row stay hidden (`NotConfigured`).
    /// On HTTP 429 (the usage endpoint's per-token bucket is shared with
    /// Claude Code's own polling; verified 2026-07-09, `Retry-After: 0`) the
    /// next cycle usually gets through — say so instead of falsely blaming
    /// the token. The same for a machine that is offline and for a server
    /// error: neither says anything about the token, and running Claude Code
    /// would not help. Only a refusal (401/403) or no usable token at all
    /// reads "token expired".
    pub fn status(self) -> ProviderStatus {
        if !self.usage_token && !self.credentials_file {
            return ProviderStatus::NotConfigured;
        }
        // Short messages: the row's hover shows them, and one that ends in
        // ", retrying" gets no "run Claude Code" appended.
        let msg = if self.rate_limited {
            "rate-limited, retrying".to_string()
        } else if let Some(words) = self.no_answer {
            words.to_string()
        } else if let Some(code) = self.other_status {
            status_words(code)
        } else if self.unrecognized {
            "unrecognized usage schema".to_string()
        } else {
            "token expired".to_string()
        };
        ProviderStatus::NetworkError(msg)
    }
}

/// Read a valid OAuth token from .credentials.json; Ok(None) if missing or expired.
///
/// Structure verified against the real file:
/// {"claudeAiOauth": {"accessToken": "...", "expiresAt": <ms epoch>, ...}}
async fn read_oauth_token(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = tokio::fs::read_to_string(path).await?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let oauth = match value.get("claudeAiOauth") {
        Some(o) => o,
        None => return Ok(None),
    };

    // Never use an expired token — Claude Code refreshes it itself.
    let expires_at_ms = oauth.get("expiresAt").and_then(|v| v.as_i64()).unwrap_or(0);
    if expires_at_ms <= utc::now().as_millisecond() {
        return Ok(None);
    }

    Ok(oauth
        .get("accessToken")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string()))
}

/// Parse the /api/oauth/usage response into metrics.
///
/// Schema verified by a manual request on 2026-06-10:
/// {"five_hour": {"utilization": 83.0, "resets_at": "RFC3339"}, "seven_day": {...},
///  "seven_day_opus": null, "seven_day_sonnet": {...}, ...}
/// Unknown/null fields are ignored — the parser is tolerant.
pub fn parse_oauth_usage(body: &str) -> Result<Vec<Metric>> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let mut metrics = Vec::new();

    let mut push_window = |key: &str, label: &str, window: MetricWindow| {
        let field = match value.get(key) {
            Some(w) if w.is_object() => w,
            _ => return,
        };
        let pct = field
            .get("utilization")
            .and_then(|v| v.as_f64())
            .map(|f| f.round().clamp(0.0, 100.0) as u64);
        if let Some(pct) = pct {
            let reset_at = field
                .get("resets_at")
                .and_then(|v| v.as_str())
                .and_then(utc::parse_rfc3339);
            metrics.push(pct_metric(label, pct, reset_at, window));
        }
    };

    // Order matters: the first metric drives the progress bar.
    push_window("five_hour", "Session", MetricWindow::Session);
    push_window("seven_day", "Weekly", MetricWindow::Long);
    push_window("seven_day_opus", "Opus", MetricWindow::Long);
    push_window("seven_day_sonnet", "Sonnet", MetricWindow::Long);

    // A model's own week ("Current week (Fable)" in Claude Code's /usage),
    // only beside the main windows: alone it is no reading of the account.
    if !metrics.is_empty() {
        for scoped in model_weeks(&value) {
            if !metrics
                .iter()
                .any(|m| m.label.eq_ignore_ascii_case(&scoped.label))
            {
                metrics.push(scoped);
            }
        }
    }

    Ok(metrics)
}

/// The weeks of single models in `limits[]`, shown only by an extra row
/// (`Metric::extra_only`). Schema verified by a manual request on
/// 2026-11-16:
/// {"limits": [{"group": "weekly", "kind": "weekly_scoped", "percent": 0,
///   "resets_at": "RFC3339", "is_active": false, "severity": "normal",
///   "scope": {"model": {"display_name": "Fable", "id": null},
///   "surface": null}}, ...]}
/// The other entries ("session", "weekly_all") repeat five_hour and
/// seven_day. `percent` is of the model's own allowance.
fn model_weeks(value: &serde_json::Value) -> Vec<Metric> {
    let Some(limits) = value.get("limits").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    limits
        .iter()
        .filter(|l| l.get("kind").and_then(|v| v.as_str()) == Some("weekly_scoped"))
        .filter_map(|l| {
            let name = l
                .get("scope")?
                .get("model")?
                .get("display_name")?
                .as_str()?
                .trim();
            let ok = !name.is_empty()
                && name.chars().count() <= 32
                && !name.chars().any(char::is_control);
            if !ok {
                return None;
            }
            let pct = l.get("percent")?.as_f64()?.round().clamp(0.0, 100.0) as u64;
            let reset_at = l
                .get("resets_at")
                .and_then(|v| v.as_str())
                .and_then(utc::parse_rfc3339);
            Some(Metric {
                extra_only: true,
                ..pct_metric(name, pct, reset_at, MetricWindow::Long)
            })
        })
        .collect()
}

/// Percentage metric.
fn pct_metric(label: &str, pct: u64, reset_at: Option<Timestamp>, window: MetricWindow) -> Metric {
    Metric {
        label: label.to_string(),
        used: pct.min(100),
        limit: Some(100),
        unit: MetricUnit::Percent,
        reset_at,
        window,
        unified_pool: false,
        extra_only: false,
    }
}

/// Header as Option<u64>.
fn header_u64(resp: &reqwest::Response, name: &str) -> Option<u64> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

/// Reset time header — RFC3339.
fn header_datetime(resp: &reqwest::Response, name: &str) -> Option<Timestamp> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(utc::parse_rfc3339)
}

#[async_trait]
impl Provider for ClaudeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Claude
    }

    async fn fetch(&self) -> Result<ProviderData> {
        if !self.config.enabled {
            return Ok(ProviderData {
                id: self.id(),
                status: ProviderStatus::NotConfigured,
                metrics: vec![],
                updated_at: utc::now(),
                received_at: Some(std::time::Instant::now()),
            });
        }

        let data = match self.config.auth_method {
            AuthMethod::ApiKey => match self.get_api_key() {
                Some(key) => self.fetch_via_api_key(&key).await?,
                // The API key chosen and none stored: nothing to read, and
                // no "key rejected" for a key never given. The Sign-in
                // block says none is stored.
                None => self.data(ProviderStatus::NotConfigured, vec![]),
            },
            AuthMethod::Subscription => self.fetch_via_subscription(false).await?,
            AuthMethod::UsageToken => self.fetch_via_subscription(true).await?,
        };
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::test_server::{Reply, TempHome, TestServer};
    use crate::providers::REFUSED_TOKEN_PAUSE;
    use std::time::Instant;

    fn config(tool_files: bool) -> ProviderConfig {
        ProviderConfig {
            id: "claude".to_string(),
            enabled: true,
            auth_method: AuthMethod::Subscription,
            credential_label: String::new(),
            tool_files,
            alert_threshold: 80,
        }
    }

    /// The usage token the user pasted as the source chosen.
    fn token_config(tool_files: bool) -> ProviderConfig {
        ProviderConfig {
            auth_method: AuthMethod::UsageToken,
            ..config(tool_files)
        }
    }

    const USAGE_BODY: &str = r#"{"five_hour": {"utilization": 25.0, "resets_at": "2099-01-01T00:00:00Z"}, "seven_day": {"utilization": 34.0, "resets_at": "2099-01-05T00:00:00Z"}}"#;

    /// A Claude Code folder holding a made-up, unexpired sign-in.
    fn claude_code_home() -> TempHome {
        let home = TempHome::new("claude");
        home.write(
            ".credentials.json",
            r#"{"claudeAiOauth": {"accessToken": "cc-token", "expiresAt": 4102444800000}}"#,
        );
        home
    }

    fn message(data: &ProviderData) -> String {
        match &data.status {
            ProviderStatus::NetworkError(m) | ProviderStatus::AuthError(m) => m.clone(),
            other => format!("{other:?}"),
        }
    }

    /// The source chosen, and only that one: a refused pasted token says
    /// so and is not sent again for half an hour; Claude Code's sign-in,
    /// though there, is not its stand-in. The tool's sign-in chosen, a
    /// stored token is never sent.
    #[tokio::test]
    async fn a_refused_usage_token_is_left_out_for_half_an_hour() {
        let server = TestServer::start();
        server.route(
            "/api/oauth/usage",
            vec![Reply::status(401), Reply::ok(USAGE_BODY)],
        );
        let home = claude_code_home();
        let p = ClaudeProvider::for_test(
            token_config(true),
            &server.base,
            Some("stored-token"),
            Some(home.path()),
        );
        for _ in 0..2 {
            let data = p.fetch().await.unwrap();
            assert_eq!(message(&data), "usage token rejected");
        }
        let auths: Vec<_> = server.seen().into_iter().filter_map(|s| s.auth).collect();
        assert_eq!(
            auths,
            ["Bearer stored-token"],
            "no stand-in, and the refused token is not sent on the second cycle"
        );
        // The tool's sign-in chosen: the stored token stays unsent.
        let server2 = TestServer::start();
        server2.route("/api/oauth/usage", vec![Reply::ok(USAGE_BODY)]);
        let p2 = ClaudeProvider::for_test(
            config(true),
            &server2.base,
            Some("stored-token"),
            Some(home.path()),
        );
        assert!(matches!(
            p2.fetch().await.unwrap().status,
            ProviderStatus::Ok
        ));
        let auths: Vec<_> = server2.seen().into_iter().filter_map(|s| s.auth).collect();
        assert_eq!(auths, ["Bearer cc-token"]);
        // After the pause it is tried again; another token at once.
        let later = Instant::now() + REFUSED_TOKEN_PAUSE;
        assert!(!p.refused.paused_at("stored-token", later));
        assert!(!p.refused.paused("pasted-anew"));
    }

    /// A plan without a five-hour window (`five_hour` null, absent or
    /// without a number): the week alone, read as an answer - never an
    /// error status, never a session number made up.
    #[tokio::test]
    async fn a_plan_without_a_session_window_shows_the_week_alone() {
        let home = claude_code_home();
        let week = r#""seven_day": {"utilization": 34.0, "resets_at": "2099-01-05T00:00:00Z"}"#;
        for session in [
            r#""five_hour": null, "#,
            "",
            r#""five_hour": {"utilization": null, "resets_at": null}, "#,
        ] {
            let body = format!("{{{session}{week}}}");
            let server = TestServer::start();
            server.route("/api/oauth/usage", vec![Reply::ok(&body)]);
            let p = ClaudeProvider::for_test(config(true), &server.base, None, Some(home.path()));
            let data = p.fetch().await.unwrap();
            assert!(
                matches!(data.status, ProviderStatus::Ok),
                "{body}: {:?}",
                data.status
            );
            let read: Vec<_> = data
                .metrics
                .iter()
                .map(|m| (m.label.as_str(), m.used, m.window))
                .collect();
            assert_eq!(read, [("Weekly", 34, MetricWindow::Long)], "{body}");
        }
    }

    /// A refused usage token keeps its row, saying so, without a request
    /// each cycle.
    #[tokio::test]
    async fn a_refused_usage_token_alone_keeps_its_row_without_requests() {
        let server = TestServer::start();
        server.route("/api/oauth/usage", vec![Reply::status(403)]);
        let p = ClaudeProvider::for_test(
            token_config(false),
            &server.base,
            Some("stored-token"),
            None,
        );
        for _ in 0..3 {
            let data = p.fetch().await.unwrap();
            assert_eq!(message(&data), "usage token rejected");
        }
        assert_eq!(server.count("/api/oauth/usage"), 1);
    }

    /// Server errors and an overloaded API are no expired token, and the
    /// hover adds no "run Claude Code" to them.
    #[tokio::test]
    async fn server_errors_are_not_an_expired_token() {
        for code in [500, 502, 503, 529] {
            let server = TestServer::start();
            server.route("/api/oauth/usage", vec![Reply::status(code)]);
            let p = ClaudeProvider::for_test(token_config(false), &server.base, Some("t"), None);
            let data = p.fetch().await.unwrap();
            assert_eq!(message(&data), format!("HTTP {code}, retrying"));
        }
        let server = TestServer::start();
        server.route("/api/oauth/usage", vec![Reply::status(429)]);
        let p = ClaudeProvider::for_test(token_config(false), &server.base, Some("t"), None);
        assert_eq!(message(&p.fetch().await.unwrap()), "rate-limited, retrying");
    }

    /// A request that got no answer reads offline, never the raw error (it
    /// quotes the URL) and never an expired token.
    #[tokio::test]
    async fn no_answer_reads_offline() {
        let server = TestServer::start();
        server.route("/api/oauth/usage", vec![Reply::Hangup]);
        let home = claude_code_home();
        let p = ClaudeProvider::for_test(config(true), &server.base, None, Some(home.path()));
        let data = p.fetch().await.unwrap();
        assert_eq!(message(&data), "offline, retrying");
    }

    /// The API key is read under Claude's own label only. A config naming
    /// another label (Copilot's token, a usage token) reads nothing, so that
    /// secret is never sent to Anthropic; the row shows no key instead.
    #[tokio::test]
    async fn the_api_key_is_read_only_under_claudes_own_label() {
        /// A Credential Manager holding a secret under every label.
        fn every_label(label: &'static str) -> Option<String> {
            Some(format!("secret of {label}"))
        }
        let with_label = |label: &str| ProviderConfig {
            auth_method: AuthMethod::ApiKey,
            credential_label: label.to_string(),
            ..config(false)
        };
        for foreign in [
            "copilot_pat",
            "claude_usage_token",
            "codex_usage_token",
            "cursor_session",
            "",
        ] {
            let server = TestServer::start();
            server.route("/v1/models", vec![Reply::ok("{}")]);
            let mut p = ClaudeProvider::for_test(with_label(foreign), &server.base, None, None);
            p.api_key = pasted_key(ProviderId::Claude, foreign, every_label);
            let data = p.fetch().await.unwrap();
            assert!(
                matches!(data.status, ProviderStatus::NotConfigured),
                "{foreign:?}: {:?}",
                data.status
            );
            assert!(server.seen().is_empty(), "{foreign:?}: nothing sent");
        }
        // Its own label: the key goes with the request.
        let server = TestServer::start();
        server.route("/v1/models", vec![Reply::ok("{}")]);
        let mut p =
            ClaudeProvider::for_test(with_label("claude_api_key"), &server.base, None, None);
        p.api_key = pasted_key(ProviderId::Claude, "claude_api_key", every_label);
        let _ = p.fetch().await.unwrap();
        assert_eq!(server.count("/v1/models"), 1);
    }

    /// Without the user's leave, Claude Code's file is not read and nothing
    /// is sent, even with a valid sign-in in it.
    #[tokio::test]
    async fn without_tool_files_nothing_is_read_or_sent() {
        let server = TestServer::start();
        server.route("/api/oauth/usage", vec![Reply::ok(USAGE_BODY)]);
        let home = claude_code_home();
        let p = ClaudeProvider::for_test(config(false), &server.base, None, Some(home.path()));
        let data = p.fetch().await.unwrap();
        assert!(matches!(data.status, ProviderStatus::NotConfigured));
        assert!(server.seen().is_empty());
    }
}

#[cfg(test)]
mod live_schema {
    use super::*;

    /// One manual request to the usage endpoint, printing the body (usage
    /// numbers only; the token never leaves the request header).
    #[tokio::test]
    #[ignore = "one live request with this PC's Claude Code sign-in"]
    async fn live_usage_schema() {
        let path = ToolPaths::from_env().unwrap().claude_credentials();
        let Some(token) = read_oauth_token(&path).await.unwrap() else {
            println!("no valid token");
            return;
        };
        let body = reqwest::Client::new()
            .get(OAUTH_USAGE_URL)
            .bearer_auth(&token)
            .header("anthropic-beta", OAUTH_BETA_HEADER)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        println!("SCHEMA {}", serde_json::to_string_pretty(&v).unwrap());
    }
}
