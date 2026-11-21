// providers/grok.rs — Grok provider (xAI SuperGrok subscription).
// See docs/en/PROVIDERS.md, section "Grok".
//
// Single source: the Grok CLI's login, auth.json in the CLI's home
// (READ-ONLY, never refreshed), used for one undocumented endpoint: the CLI
// proxy's billing call in its credits format.

use super::{
    clean_label, read_text, status_words, transport_error, Metric, MetricUnit, MetricWindow, Net,
    Provider, ProviderData, ProviderId, ProviderStatus, MAX_ANSWER_BYTES,
};
use crate::config::{paths::ToolPaths, schema::ProviderConfig};
use crate::utc::{self, Timestamp};
use anyhow::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Undocumented SuperGrok usage endpoint the Grok CLI itself calls — verified
/// 2026-11-10, HTTP 200. `format=credits` is what selects the subscription
/// limit: the bare path answers the monthly API spend instead.
const BILLING_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
/// The proxy's marker for a Grok CLI session token, sent beside the bearer.
const TOKEN_AUTH_HEADER: &str = "x-xai-token-auth";
const TOKEN_AUTH_VALUE: &str = "xai-grok-cli";

pub struct GrokProvider {
    config: ProviderConfig,
    /// Never follows a 3xx — keeps the bearer token from leaking to a
    /// redirected host. The billing endpoint returns 200 directly.
    net: Net,
    /// The Grok CLI's login file (`ToolPaths::grok_auth`: auth.json in
    /// GROK_HOME when set, else in %USERPROFILE%\.grok; None for a relative
    /// GROK_HOME), read only when allowed.
    login_path: Option<PathBuf>,
}

/// What the Grok CLI's login file holds for the widget.
pub enum GrokLogin {
    /// No login file, or one without a session: the CLI was never signed in
    /// here, or was signed out.
    Missing,
    /// Sessions exist, but every access token is past its expiry. Only the
    /// CLI may refresh them, so the widget waits for the next `grok` run.
    Expired,
    /// A usable access token, with its expiry when the file states one.
    Token {
        key: String,
        expires_at: Option<Timestamp>,
    },
}

// Written by hand so that no debug print can ever carry the token.
impl std::fmt::Debug for GrokLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrokLogin::Missing => f.write_str("Missing"),
            GrokLogin::Expired => f.write_str("Expired"),
            GrokLogin::Token { expires_at, .. } => f
                .debug_struct("Token")
                .field("expires_at", expires_at)
                .finish_non_exhaustive(),
        }
    }
}

impl GrokProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            net: Net::new(),
            login_path: ToolPaths::from_env().and_then(|p| p.grok_auth()),
        }
    }

    /// A provider that sends to a local test server and reads the login
    /// file the test wrote.
    #[cfg(test)]
    fn for_test(config: ProviderConfig, server: &str, login_path: Option<PathBuf>) -> Self {
        Self {
            config,
            net: Net::local(server),
            login_path,
        }
    }

    fn data(&self, status: ProviderStatus, metrics: Vec<Metric>) -> ProviderData {
        ProviderData {
            id: self.id(),
            status,
            metrics,
            updated_at: utc::now(),
            received_at: Some(std::time::Instant::now()),
        }
    }

    /// SuperGrok usage. The CLI token is read-only: the Grok CLI refreshes it
    /// whenever it runs, the widget must never rotate it.
    async fn fetch_via_subscription(&self) -> Result<ProviderData> {
        // The Grok CLI's login is its own: only when the user allowed it.
        let Some(path) = self
            .login_path
            .as_deref()
            .filter(|_| self.config.tool_files)
        else {
            return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
        };
        let token = match read_grok_login(path).await {
            Ok(GrokLogin::Token { key, .. }) => key,
            // No session at all — the row stays hidden until `grok` signs in.
            Ok(GrokLogin::Missing) => {
                return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
            }
            // A login that has lapsed is an honest error, not a missing one:
            // app.rs keeps the last real data on screen, greyed with its age.
            Ok(GrokLogin::Expired) => {
                return Ok(self.data(
                    ProviderStatus::NetworkError("token expired".to_string()),
                    vec![],
                ));
            }
            // Possibly caught mid-rewrite by the CLI; the next cycle reads it
            // again. The message never quotes the file.
            Err(e) => {
                warn!("grok auth.json unreadable: {e}");
                return Ok(self.data(
                    ProviderStatus::NetworkError("auth.json unreadable".to_string()),
                    vec![],
                ));
            }
        };

        let resp = match self
            .net
            .http
            .get(self.net.url(BILLING_URL))
            .bearer_auth(&token)
            .header(TOKEN_AUTH_HEADER, TOKEN_AUTH_VALUE)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("grok billing request failed: {e}");
                let words = transport_error(&e).to_string();
                return Ok(self.data(ProviderStatus::NetworkError(words), vec![]));
            }
        };
        let code = resp.status().as_u16();
        // Only a 200 carries a body worth reading. One cut off mid-transfer
        // is an honest error row, never an Err the scheduler would only log.
        let body = if code == 200 {
            match read_text(resp, MAX_ANSWER_BYTES).await {
                Ok(body) => body,
                Err(e) => {
                    warn!("grok billing body read failed: {e}");
                    let words = e.words().to_string();
                    return Ok(self.data(ProviderStatus::NetworkError(words), vec![]));
                }
            }
        } else {
            String::new()
        };
        let (status, metrics) = billing_outcome(code, &body);
        Ok(self.data(status, metrics))
    }
}

#[async_trait]
impl Provider for GrokProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Grok
    }

    async fn fetch(&self) -> Result<ProviderData> {
        if !self.config.enabled {
            return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
        }
        self.fetch_via_subscription().await
    }
}

/// Read the Grok CLI login. A missing file is `Missing`, not an error.
async fn read_grok_login(path: &Path) -> Result<GrokLogin> {
    let content = match tokio::fs::read_to_string(path).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(GrokLogin::Missing),
        Err(e) => return Err(e.into()),
    };
    parse_grok_auth(&content, utc::now())
}

/// Pick a usable session from the Grok CLI's auth.json.
///
/// Structure verified 2026-11-10 against the real file: an object keyed
/// `"<issuer>::<client-id>"`, each value a session record
/// {"key": "<access token, a JWT>", "refresh_token": "...",
///  "expires_at": "RFC3339", "auth_mode": "oidc", "oidc_issuer": "...", ...}.
/// Expired sessions are skipped — the CLI refreshes them itself. Of the rest,
/// the one valid longest wins; a session with no stated expiry (the CLI gives
/// those a 30-day lifetime of its own) comes last and is left to the server
/// to judge. A session another issuer gave (not xAI's) is never sent to
/// xAI: it is passed over as if it were not there.
pub fn parse_grok_auth(body: &str, now: Timestamp) -> Result<GrokLogin> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let Some(sessions) = value.as_object() else {
        return Ok(GrokLogin::Missing);
    };

    let mut any_session = false;
    let mut best: Option<(Option<Timestamp>, &str)> = None;
    for (name, record) in sessions {
        let issuer = record
            .get("oidc_issuer")
            .and_then(|v| v.as_str())
            .or_else(|| name.split_once("::").map(|(issuer, _)| issuer));
        if issuer.is_some_and(|i| !is_xai_issuer(i)) {
            continue;
        }
        let Some(key) = record
            .get("key")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        any_session = true;
        let expires_at = record.get("expires_at").and_then(rfc3339);
        if expires_at.is_some_and(|t| t <= now) {
            continue;
        }
        // None orders below every Some: a later stated expiry beats an
        // earlier one, and any stated expiry beats none.
        if best.is_none_or(|(b, _)| expires_at > b) {
            best = Some((expires_at, key));
        }
    }

    Ok(match best {
        Some((expires_at, key)) => GrokLogin::Token {
            key: key.to_string(),
            expires_at,
        },
        None if any_session => GrokLogin::Expired,
        None => GrokLogin::Missing,
    })
}

/// Whether an OIDC issuer is xAI's own (verified: `https://auth.x.ai`): an
/// https address on x.ai or grok.com, or a host under them.
fn is_xai_issuer(issuer: &str) -> bool {
    let Some(rest) = issuer.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => authority,
    }
    .to_ascii_lowercase();
    ["x.ai", "grok.com"]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// One billing answer → status + metrics. Free of I/O so every branch is
/// testable: any non-200 or unknown body is an honest error, never numbers.
pub fn billing_outcome(code: u16, body: &str) -> (ProviderStatus, Vec<Metric>) {
    let unrecognized = || {
        (
            ProviderStatus::NetworkError("unrecognized billing schema".to_string()),
            vec![],
        )
    };
    match code {
        200 => match parse_billing_credits(body) {
            Ok(metrics) if !metrics.is_empty() => (ProviderStatus::Ok, metrics),
            Ok(_) => unrecognized(),
            Err(e) => {
                // serde names a position in the body, never its content.
                warn!("grok billing body is not JSON: {e}");
                unrecognized()
            }
        },
        // The CLI may have rotated the token since the file was read; the
        // next cycle reads the file again.
        401 | 403 => (
            ProviderStatus::NetworkError("token rejected".to_string()),
            vec![],
        ),
        other => {
            warn!("grok billing returned HTTP {other}");
            (ProviderStatus::NetworkError(status_words(other)), vec![])
        }
    }
}

/// Parse the billing response (`?format=credits`) into metrics.
///
/// Schema verified by a manual request on 2026-11-10:
/// {"config": {"currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY",
///   "start": "RFC3339", "end": "RFC3339"},
///  "creditUsagePercent": 1.0,
///  "productUsage": [{"product": "GrokBuild", "usagePercent": 1.0}],
///  "billingPeriodStart": "RFC3339", "billingPeriodEnd": "RFC3339", ...}}
///
/// The body is protobuf JSON, which leaves zero values out: before any use
/// the same call answered with no `creditUsagePercent` and no `productUsage`
/// at all. A missing percent beside a well-formed period is therefore 0% —
/// and only then; without the period it is an unknown schema (empty list).
///
/// The first metric is the whole subscription limit. Each product's share of
/// it follows as its own long-window metric, shown only by its extra row
/// (`Metric::extra_only`; Grok Build's is "grok.build"): the Grok row looks
/// the same whether the server lists products or not.
pub fn parse_billing_credits(body: &str) -> Result<Vec<Metric>> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let Some(config) = value.get("config").filter(|c| c.is_object()) else {
        return Ok(Vec::new());
    };

    let period = config.get("currentPeriod").filter(|p| p.is_object());
    let period_end = period.and_then(|p| p.get("end")).and_then(rfc3339);
    let period_known = period_end.is_some()
        || period
            .and_then(|p| p.get("start"))
            .and_then(rfc3339)
            .is_some();
    let reset_at = period_end.or_else(|| config.get("billingPeriodEnd").and_then(rfc3339));

    let used = match config.get("creditUsagePercent") {
        Some(v) => match number(v) {
            Some(pct) => percent(pct),
            // There, but not a number this parser knows: no number is made
            // up for it (the row says the schema is unrecognized).
            None => return Ok(Vec::new()),
        },
        // The encoder's omitted zero — trusted only beside a real period.
        None if period_known => 0,
        None => return Ok(Vec::new()),
    };
    let label = period_label(
        period
            .and_then(|p| p.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
    );

    let mut metrics = vec![Metric {
        label: label.to_string(),
        used,
        limit: Some(100),
        unit: MetricUnit::Percent,
        reset_at,
        // The subscription's only window, and its general limit: a spent
        // one must drive every surface.
        window: MetricWindow::Long,
        unified_pool: false,
        extra_only: false,
    }];

    if let Some(products) = config.get("productUsage").and_then(|v| v.as_array()) {
        for entry in products {
            let Some(name) = entry
                .get("product")
                .and_then(|v| v.as_str())
                .and_then(product_label)
            else {
                continue;
            };
            let share = match entry.get("usagePercent") {
                // Another omitted zero.
                None => 0,
                Some(v) => match number(v) {
                    Some(pct) => percent(pct),
                    None => continue,
                },
            };
            metrics.push(Metric {
                label: name,
                used: share,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at,
                window: MetricWindow::Long,
                unified_pool: false,
                extra_only: true,
            });
        }
    }

    Ok(metrics)
}

/// An RFC 3339 timestamp field.
fn rfc3339(v: &serde_json::Value) -> Option<Timestamp> {
    v.as_str().and_then(utc::parse_rfc3339)
}

/// A protobuf-JSON number: a JSON number as a rule, though the encoder may
/// write some values as strings. Anything non-finite is not usage.
fn number(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        .filter(|f: &f64| f.is_finite())
}

/// A 0–100 percentage, rounded and clamped.
fn percent(pct: f64) -> u64 {
    pct.round().clamp(0.0, 100.0) as u64
}

/// Row label for the period type: "USAGE_PERIOD_TYPE_WEEKLY" → "Weekly". A
/// type it does not know is labelled for what it is — a limit.
fn period_label(kind: &str) -> &'static str {
    if kind.contains("WEEK") {
        "Weekly"
    } else if kind.contains("DAY") || kind.contains("DAILY") {
        "Daily"
    } else if kind.contains("MONTH") {
        "Monthly"
    } else {
        "Limit"
    }
}

/// Compact product name for the widget's narrow rows: "GrokBuild" → "Build".
/// The row already says Grok; any other name passes through, cleaned (no
/// control characters, 32 characters at most). None for one with nothing
/// left to show.
fn product_label(product: &str) -> Option<String> {
    let name = clean_label(product)?;
    match name.strip_prefix("Grok") {
        Some(rest) if !rest.trim().is_empty() => Some(rest.trim().to_string()),
        _ => Some(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Switched on but not allowed to read the tool's own sign-in: the fetch
    /// ends before any file, keyring entry or request.
    #[tokio::test]
    async fn without_tool_files_nothing_is_read() {
        let p = GrokProvider::new(ProviderConfig {
            id: "grok".to_string(),
            enabled: true,
            auth_method: crate::config::schema::AuthMethod::Subscription,
            credential_label: String::new(),
            alert_threshold: 80,
            tool_files: false,
        });
        let data = p.fetch().await.unwrap();
        assert!(matches!(data.status, ProviderStatus::NotConfigured));
    }

    use crate::providers::test_server::{Reply, TempHome, TestServer};

    fn config(tool_files: bool) -> ProviderConfig {
        ProviderConfig {
            id: "grok".to_string(),
            enabled: true,
            auth_method: Default::default(),
            credential_label: String::new(),
            tool_files,
            alert_threshold: 80,
        }
    }

    /// A Grok CLI home with a made-up session valid until 2100.
    fn grok_home() -> TempHome {
        let home = TempHome::new("grok");
        home.write(
            "auth.json",
            r#"{"https://auth.x.ai::client": {"key": "grok-token", "expires_at": "2100-01-01T00:00:00Z"}}"#,
        );
        home
    }

    const BILLING_BODY: &str = r#"{"config": {"currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY", "start": "2099-01-01T00:00:00Z", "end": "2099-01-08T00:00:00Z"}, "creditUsagePercent": 12.0, "productUsage": [{"product": "GrokBuild", "usagePercent": 3.0}]}}"#;

    fn message(data: &ProviderData) -> String {
        match &data.status {
            ProviderStatus::NetworkError(m) | ProviderStatus::AuthError(m) => m.clone(),
            other => format!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn one_request_per_cycle_and_honest_failures() {
        let home = grok_home();
        let path = Some(home.path().join("auth.json"));
        let server = TestServer::start();
        server.route("/v1/billing", vec![Reply::ok(BILLING_BODY)]);
        let p = GrokProvider::for_test(config(true), &server.base, path.clone());
        let data = p.fetch().await.unwrap();
        assert!(matches!(data.status, ProviderStatus::Ok));
        assert_eq!(server.count("/v1/billing"), 1);
        assert_eq!(server.seen()[0].auth.as_deref(), Some("Bearer grok-token"));
        // The product's share is for its extra row only.
        assert!(!data.metrics[0].extra_only);
        assert!(data.metrics[1].extra_only);
        assert_eq!(data.metrics[1].label, "Build");

        for (reply, words) in [
            (Reply::Hangup, "offline, retrying"),
            (Reply::status(502), "HTTP 502, retrying"),
            (Reply::status(429), "rate-limited, retrying"),
            (Reply::status(401), "token rejected"),
        ] {
            let server = TestServer::start();
            server.route("/v1/billing", vec![reply]);
            let p = GrokProvider::for_test(config(true), &server.base, path.clone());
            assert_eq!(message(&p.fetch().await.unwrap()), words);
        }
    }

    #[tokio::test]
    async fn without_tool_files_the_login_is_not_read_or_sent() {
        let home = grok_home();
        let server = TestServer::start();
        server.route("/v1/billing", vec![Reply::ok(BILLING_BODY)]);
        let p = GrokProvider::for_test(
            config(false),
            &server.base,
            Some(home.path().join("auth.json")),
        );
        assert!(matches!(
            p.fetch().await.unwrap().status,
            ProviderStatus::NotConfigured
        ));
        assert!(server.seen().is_empty());
    }

    /// The login is read from the CLI's home as config/paths.rs resolves
    /// it: an absolute GROK_HOME, else %USERPROFILE%\.grok; never a path
    /// relative to the working directory.
    #[test]
    fn the_login_lives_in_the_clis_home_and_never_a_relative_one() {
        let p = GrokProvider::new(config(false));
        let paths = ToolPaths::from_env().expect("a home folder");
        assert_eq!(p.login_path, paths.grok_auth());
        if let Some(path) = &p.login_path {
            assert!(path.is_absolute());
        }
    }

    #[test]
    fn product_names_are_cleaned() {
        assert_eq!(product_label("GrokBuild").as_deref(), Some("Build"));
        assert_eq!(product_label("Grok").as_deref(), Some("Grok"));
        assert_eq!(
            product_label("Imagine\u{1b}[31m").as_deref(),
            Some("Imagine[31m")
        );
        assert_eq!(product_label(" \u{7} "), None);
        assert_eq!(product_label(&"P".repeat(70)).map(|s| s.len()), Some(32));
    }

    /// One live request with the real Grok CLI login. Run explicitly:
    /// `cargo test grok_live_fetch -- --ignored --nocapture`. Prints the
    /// parsed numbers only; the token never leaves the provider.
    #[tokio::test]
    #[ignore = "one live request with this PC's Grok CLI sign-in"]
    async fn grok_live_fetch() {
        let provider = GrokProvider::new(ProviderConfig {
            id: "grok".to_string(),
            enabled: true,
            auth_method: Default::default(),
            credential_label: String::new(),
            tool_files: true,
            alert_threshold: 80,
        });
        let data = provider.fetch().await.expect("fetch");
        println!("status: {:?}", data.status);
        for m in &data.metrics {
            println!(
                "{}: {}% ({:?} window), resets {:?}",
                m.label, m.used, m.window, m.reset_at
            );
        }
        assert!(
            matches!(data.status, ProviderStatus::Ok),
            "a live login should read Ok"
        );
    }
}
