// providers/grok.rs — Grok provider (xAI SuperGrok subscription).
// See docs/en/PROVIDERS.md, section "Grok".
//
// Single source: the Grok CLI's login, auth.json in the CLI's home
// (READ-ONLY, never refreshed), used for one undocumented endpoint: the CLI
// proxy's billing call in its credits format.

use super::{Metric, MetricUnit, MetricWindow, Provider, ProviderData, ProviderId, ProviderStatus};
use crate::config::schema::ProviderConfig;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
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
    http: reqwest::Client,
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
        expires_at: Option<DateTime<Utc>>,
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
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            // Never follow a 3xx — keeps the bearer token from leaking to a
            // redirected host. The billing endpoint returns 200 directly.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build HTTP client");
        Self { config, http }
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

    /// SuperGrok usage. The CLI token is read-only: the Grok CLI refreshes it
    /// whenever it runs, the widget must never rotate it.
    async fn fetch_via_subscription(&self) -> Result<ProviderData> {
        // The Grok CLI's login is its own: only when the user allowed it.
        if !self.config.tool_files {
            return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
        }
        let token = match read_grok_login().await {
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
            .http
            .get(BILLING_URL)
            .bearer_auth(&token)
            .header(TOKEN_AUTH_HEADER, TOKEN_AUTH_VALUE)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return Ok(self.data(ProviderStatus::NetworkError(e.to_string()), vec![])),
        };
        let code = resp.status().as_u16();
        // Only a 200 carries a body worth reading. One cut off mid-transfer
        // is an honest error row, never an Err the scheduler would only log.
        let body = if code == 200 {
            match resp.text().await {
                Ok(body) => body,
                Err(e) => return Ok(self.data(ProviderStatus::NetworkError(e.to_string()), vec![])),
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

/// The Grok CLI's home: `GROK_HOME` when set, otherwise `%USERPROFILE%\.grok`
/// — the CLI's own rule (its docs, user-guide/05-configuration.md).
fn grok_home() -> PathBuf {
    match std::env::var_os("GROK_HOME").filter(|v| !v.is_empty()) {
        Some(home) => PathBuf::from(home),
        None => dirs::home_dir().unwrap_or_default().join(".grok"),
    }
}

/// Where the Grok CLI keeps its login.
pub fn auth_path() -> PathBuf {
    grok_home().join("auth.json")
}

/// Read the Grok CLI login. A missing file is `Missing`, not an error.
async fn read_grok_login() -> Result<GrokLogin> {
    let content = match tokio::fs::read_to_string(auth_path()).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(GrokLogin::Missing),
        Err(e) => return Err(e.into()),
    };
    parse_grok_auth(&content, Utc::now())
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
/// to judge.
pub fn parse_grok_auth(body: &str, now: DateTime<Utc>) -> Result<GrokLogin> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let Some(sessions) = value.as_object() else {
        return Ok(GrokLogin::Missing);
    };

    let mut any_session = false;
    let mut best: Option<(Option<DateTime<Utc>>, &str)> = None;
    for record in sessions.values() {
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
            (
                ProviderStatus::NetworkError(format!("HTTP {other}")),
                vec![],
            )
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
/// it follows as its own long-window metric, the place Claude's Opus and
/// Sonnet pools take; nothing draws them yet.
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

    let used = match config.get("creditUsagePercent").and_then(number) {
        Some(pct) => percent(pct),
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
    }];

    if let Some(products) = config.get("productUsage").and_then(|v| v.as_array()) {
        for entry in products {
            let Some(name) = entry
                .get("product")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
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
                label: product_label(name),
                used: share,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at,
                window: MetricWindow::Long,
                unified_pool: false,
            });
        }
    }

    Ok(metrics)
}

/// An RFC 3339 timestamp field.
fn rfc3339(v: &serde_json::Value) -> Option<DateTime<Utc>> {
    v.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
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
/// The row already says Grok; any other name passes through unchanged.
fn product_label(product: &str) -> String {
    match product.strip_prefix("Grok") {
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ => product.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One live request with the real Grok CLI login. Run explicitly:
    /// `cargo test grok_live_fetch -- --ignored --nocapture`. Prints the
    /// parsed numbers only; the token never leaves the provider.
    #[tokio::test]
    #[ignore]
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
