// providers/codex.rs — OpenAI Codex provider (ChatGPT subscription).
// See docs/en/PROVIDERS.md, section "OpenAI Codex".
//
// Token sources, in order:
//   1. manual usage token from Credential Manager ("codex_usage_token")
//   2. the Codex CLI token from auth.json in the CLI's home (READ-ONLY,
//      never refreshed): CODEX_HOME when it is set, otherwise
//      %USERPROFILE%\.codex (config/paths.rs; a CODEX_HOME that is not an
//      absolute path means no file is read at all)
// Both are used for a single undocumented endpoint: wham/usage.

use super::{
    keyring_secret, status_words, transport_error, Metric, MetricUnit, MetricWindow, Net, Provider,
    ProviderData, ProviderId, ProviderStatus, Refusals, SecretSource,
};
use crate::config::{
    paths::ToolPaths,
    schema::{AuthMethod, ProviderConfig},
};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Undocumented ChatGPT subscription usage endpoint — verified 2026-06-10, HTTP 200.
const WHAM_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// Credential Manager label of the manual usage token.
const USAGE_TOKEN_LABEL: &str = "codex_usage_token";

pub struct CodexProvider {
    config: ProviderConfig,
    /// Never follows a 3xx — keeps the bearer token from leaking to a
    /// redirected host. The endpoint returns 200 directly.
    net: Net,
    /// The app's own usage token (Credential Manager), read each cycle.
    usage_token: SecretSource,
    /// The Codex CLI's sign-in (`ToolPaths::codex_auth`: auth.json in
    /// CODEX_HOME when set, else in %USERPROFILE%\.codex; None for a
    /// relative CODEX_HOME), read only when allowed.
    auth_path: Option<PathBuf>,
    /// A stored usage token the endpoint refused: not sent again for a
    /// while, so it costs no second request each cycle.
    refused: Refusals,
}

impl CodexProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            net: Net::new(),
            usage_token: keyring_secret(USAGE_TOKEN_LABEL),
            auth_path: ToolPaths::from_env().and_then(|p| p.codex_auth()),
            refused: Refusals::default(),
        }
    }

    /// A provider that sends to a local test server and finds its sources
    /// where the test put them.
    #[cfg(test)]
    fn for_test(
        config: ProviderConfig,
        server: &str,
        usage_token: Option<&str>,
        auth_path: Option<PathBuf>,
    ) -> Self {
        let token = usage_token.map(str::to_string);
        Self {
            config,
            net: Net::local(server),
            usage_token: Box::new(move || token.clone()),
            auth_path,
            refused: Refusals::default(),
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

    /// ChatGPT subscription usage. The CLI token is read-only: Codex CLI
    /// refreshes it itself, the widget must never rotate it.
    async fn fetch_via_subscription(&self, use_usage_token: bool) -> ProviderData {
        // The source chosen, and only that one. 1. The usage token the user
        // pasted: refused, it reads "usage token rejected" and is not sent
        // again for `REFUSED_TOKEN_PAUSE`; any other outcome ends the cycle.
        // 2. Codex CLI's auth.json. One usage request per cycle.
        let mut usage_token_failure = None;
        if let Some(token) = (self.usage_token)().filter(|_| use_usage_token) {
            let refused = ProviderStatus::AuthError("usage token rejected".to_string());
            if self.refused.paused(&token) {
                debug!("the stored Codex usage token was refused lately; not sent");
                usage_token_failure = Some(refused);
            } else {
                match self.fetch_usage(&token).await {
                    Usage::Answer(data) => return data,
                    Usage::Refused => {
                        warn!("stored Codex usage token was rejected, falling back");
                        self.refused.note(&token);
                        usage_token_failure = Some(refused);
                    }
                    Usage::NoAnswer(words) => {
                        return self.data(ProviderStatus::NetworkError(words.to_string()), vec![]);
                    }
                }
            }
        }

        // The usage token alone was chosen: Codex CLI's file is not read.
        if use_usage_token {
            return self.data(status_without_cli_token(usage_token_failure, false), vec![]);
        }

        // 2. Codex CLI token, only when the user allowed reading it.
        let Some(path) = self.auth_path.as_deref().filter(|_| self.config.tool_files) else {
            return self.data(status_without_cli_token(usage_token_failure, false), vec![]);
        };
        let token = match read_codex_token(path).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                return self.data(status_without_cli_token(usage_token_failure, false), vec![]);
            }
            Err(e) => {
                warn!("codex auth.json unreadable: {e}");
                return self.data(status_without_cli_token(usage_token_failure, true), vec![]);
            }
        };

        match self.fetch_usage(&token).await {
            Usage::Answer(data) => data,
            // Codex CLI refreshes its token when it runs; the hover adds
            // "run Codex CLI". Not "check the key": no key was given.
            Usage::Refused => self.data(
                ProviderStatus::NetworkError("token expired".to_string()),
                vec![],
            ),
            Usage::NoAnswer(words) => {
                self.data(ProviderStatus::NetworkError(words.to_string()), vec![])
            }
        }
    }

    /// One usage request.
    async fn fetch_usage(&self, token: &str) -> Usage {
        let resp = match self
            .net
            .http
            .get(self.net.url(WHAM_USAGE_URL))
            .bearer_auth(token)
            .header("User-Agent", "ailimits-widget")
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                warn!("codex usage request failed: {e}");
                return Usage::NoAnswer(transport_error(&e));
            }
        };

        match resp.status().as_u16() {
            200 => {
                let body = match resp.text().await {
                    Ok(body) => body,
                    Err(e) => {
                        warn!("codex usage body read failed: {e}");
                        return Usage::NoAnswer(transport_error(&e));
                    }
                };
                let metrics = match parse_wham_usage(&body) {
                    Ok(metrics) => metrics,
                    Err(e) => {
                        // serde names a position in the body, never its content.
                        warn!("wham/usage body is not JSON: {e}");
                        Vec::new()
                    }
                };
                if metrics.is_empty() {
                    // Undocumented endpoint: an unknown schema is an honest error,
                    // not invented numbers.
                    return Usage::Answer(self.data(
                        ProviderStatus::NetworkError("unrecognized wham/usage schema".to_string()),
                        vec![],
                    ));
                }
                Usage::Answer(self.data(ProviderStatus::Ok, metrics))
            }
            401 | 403 => Usage::Refused,
            other => {
                warn!("wham/usage returned HTTP {other}");
                Usage::Answer(self.data(ProviderStatus::NetworkError(status_words(other)), vec![]))
            }
        }
    }
}

/// One read-only usage request to check a pasted usage token before it is
/// stored (`providers::check_usage_token`).
pub(crate) async fn check_usage_token(net: &Net, token: &str) -> super::TokenCheck {
    super::token_check(
        net.http
            .get(net.url(WHAM_USAGE_URL))
            .bearer_auth(token)
            .header("User-Agent", "ailimits-widget")
            .send()
            .await,
    )
}

/// What one usage request came to.
enum Usage {
    /// The row: numbers, or an honest error the server's answer explains.
    Answer(ProviderData),
    /// 401/403: the token is not accepted.
    Refused,
    /// No HTTP answer: the row's words for it.
    NoAnswer(&'static str),
}

#[async_trait]
impl Provider for CodexProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Codex
    }

    async fn fetch(&self) -> Result<ProviderData> {
        if !self.config.enabled {
            return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
        }
        // The source the user chose, and only that one.
        let use_usage_token = self.config.auth_method == AuthMethod::UsageToken;
        Ok(self.fetch_via_subscription(use_usage_token).await)
    }
}

/// The status of a cycle in which the Codex CLI token could not be used.
///
/// An unreadable auth.json is an honest error — possibly caught mid-rewrite
/// by Codex CLI, the next cycle reads it again; the message never quotes the
/// file. Without auth.json, a stored usage token that failed still owns the
/// row: app.rs keeps its last data on screen, greyed with its age. Only with
/// no source at all does the row stay hidden (`NotConfigured`).
pub fn status_without_cli_token(
    usage_token_failure: Option<ProviderStatus>,
    auth_json_unreadable: bool,
) -> ProviderStatus {
    if auth_json_unreadable {
        return ProviderStatus::NetworkError("auth.json unreadable".to_string());
    }
    usage_token_failure.unwrap_or(ProviderStatus::NotConfigured)
}

/// Read the Codex CLI access token: auth.json → tokens.access_token.
/// Ok(None) if the file or the token is missing.
///
/// Structure verified 2026-06-10: {"auth_mode": ..., "tokens": {"access_token": ...}, ...}
async fn read_codex_token(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = tokio::fs::read_to_string(path).await?;
    let value: serde_json::Value = serde_json::from_str(&content)?;
    Ok(value
        .get("tokens")
        .and_then(|t| t.get("access_token"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string()))
}

/// Parse wham/usage into metrics.
///
/// Schema verified by a manual request on 2026-06-10 (plus plan):
/// {"rate_limit": {"primary_window": {"used_percent": 1, "reset_at": 1781121633, ...},
///  "secondary_window": {"used_percent": 28, ...}}, "plan_type": "plus", ...}
/// primary = the 5-hour session, secondary = the week. The parser is tolerant.
pub fn parse_wham_usage(body: &str) -> Result<Vec<Metric>> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let rate_limit = match value.get("rate_limit") {
        Some(rl) if rl.is_object() => rl,
        _ => return Ok(vec![]),
    };

    let mut metrics = Vec::new();
    let mut push_window = |key: &str, label: &str, window: MetricWindow| {
        let field = match rate_limit.get(key) {
            Some(w) if w.is_object() => w,
            _ => return,
        };
        let pct = field
            .get("used_percent")
            .and_then(|v| v.as_f64())
            .map(|f| f.round().clamp(0.0, 100.0) as u64);
        if let Some(pct) = pct {
            // reset_at is unix epoch seconds.
            let reset_at = field
                .get("reset_at")
                .and_then(|v| v.as_i64())
                .and_then(|s| DateTime::<Utc>::from_timestamp(s, 0));
            metrics.push(Metric {
                label: label.to_string(),
                used: pct,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at,
                window,
                unified_pool: false,
                extra_only: false,
            });
        }
    };

    // Order matters: the first metric drives the progress bar.
    push_window("primary_window", "Session", MetricWindow::Session);
    push_window("secondary_window", "Weekly", MetricWindow::Long);

    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::test_server::{Reply, TempHome, TestServer};

    fn config(tool_files: bool) -> ProviderConfig {
        ProviderConfig {
            id: "codex".to_string(),
            enabled: true,
            auth_method: Default::default(),
            credential_label: String::new(),
            tool_files,
            alert_threshold: 80,
        }
    }

    const USAGE_BODY: &str = r#"{"rate_limit": {"primary_window": {"used_percent": 1, "reset_at": 4102444800}, "secondary_window": {"used_percent": 28, "reset_at": 4102444800}}}"#;

    fn codex_home() -> TempHome {
        let home = TempHome::new("codex");
        home.write(
            "auth.json",
            r#"{"auth_mode": "chatgpt", "tokens": {"access_token": "cli-token"}}"#,
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
    /// so and is not sent again for half an hour; Codex CLI's sign-in,
    /// though there, is not its stand-in. Codex CLI's chosen, a stored
    /// token is never sent.
    #[tokio::test]
    async fn a_refused_usage_token_is_left_out_for_half_an_hour() {
        let server = TestServer::start();
        server.route(
            "/backend-api/wham/usage",
            vec![Reply::status(401), Reply::ok(USAGE_BODY)],
        );
        let home = codex_home();
        let token_config = ProviderConfig {
            auth_method: AuthMethod::UsageToken,
            ..config(true)
        };
        let p = CodexProvider::for_test(
            token_config,
            &server.base,
            Some("stored-token"),
            Some(home.path().join("auth.json")),
        );
        for _ in 0..3 {
            assert_eq!(message(&p.fetch().await.unwrap()), "usage token rejected");
        }
        let auths: Vec<_> = server.seen().into_iter().filter_map(|s| s.auth).collect();
        assert_eq!(auths, ["Bearer stored-token"]);
        let server2 = TestServer::start();
        server2.route("/backend-api/wham/usage", vec![Reply::ok(USAGE_BODY)]);
        let p2 = CodexProvider::for_test(
            config(true),
            &server2.base,
            Some("stored-token"),
            Some(home.path().join("auth.json")),
        );
        assert!(matches!(
            p2.fetch().await.unwrap().status,
            ProviderStatus::Ok
        ));
        let auths: Vec<_> = server2.seen().into_iter().filter_map(|s| s.auth).collect();
        assert_eq!(auths, ["Bearer cli-token"]);
    }

    #[tokio::test]
    async fn a_refused_cli_token_says_expired_not_check_the_key() {
        let server = TestServer::start();
        server.route("/backend-api/wham/usage", vec![Reply::status(401)]);
        let home = codex_home();
        let p = CodexProvider::for_test(
            config(true),
            &server.base,
            None,
            Some(home.path().join("auth.json")),
        );
        let data = p.fetch().await.unwrap();
        assert!(
            matches!(&data.status, ProviderStatus::NetworkError(m) if m == "token expired"),
            "{:?}",
            data.status
        );
    }

    #[tokio::test]
    async fn no_answer_and_server_errors_read_honestly() {
        let home = codex_home();
        for (reply, words) in [
            (Reply::Hangup, "offline, retrying"),
            (Reply::status(503), "HTTP 503, retrying"),
            (Reply::status(429), "rate-limited, retrying"),
            (Reply::ok("<html>"), "unrecognized wham/usage schema"),
        ] {
            let server = TestServer::start();
            server.route("/backend-api/wham/usage", vec![reply]);
            let p = CodexProvider::for_test(
                config(true),
                &server.base,
                None,
                Some(home.path().join("auth.json")),
            );
            assert_eq!(message(&p.fetch().await.unwrap()), words);
        }
    }

    #[tokio::test]
    async fn without_tool_files_auth_json_is_not_read_or_sent() {
        let server = TestServer::start();
        server.route("/backend-api/wham/usage", vec![Reply::ok(USAGE_BODY)]);
        let home = codex_home();
        let p = CodexProvider::for_test(
            config(false),
            &server.base,
            None,
            Some(home.path().join("auth.json")),
        );
        assert!(matches!(
            p.fetch().await.unwrap().status,
            ProviderStatus::NotConfigured
        ));
        assert!(server.seen().is_empty());
    }

    /// The sign-in is read from the CLI's home as config/paths.rs resolves
    /// it, the folder the model label comes from too; never relative to
    /// the working directory.
    #[test]
    fn the_sign_in_lives_in_the_clis_home() {
        let p = CodexProvider::new(config(false));
        let paths = ToolPaths::from_env().expect("a home folder");
        assert_eq!(p.auth_path, paths.codex_auth());
        if let Some(path) = &p.auth_path {
            assert!(path.is_absolute());
            assert_eq!(path.file_name().and_then(|n| n.to_str()), Some("auth.json"));
        }
    }
}
