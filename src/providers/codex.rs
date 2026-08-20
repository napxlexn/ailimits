// providers/codex.rs — OpenAI Codex provider (ChatGPT subscription).
// See docs/en/PROVIDERS.md, section "OpenAI Codex".
//
// Token sources, in order:
//   1. manual usage token from Credential Manager ("codex_usage_token")
//   2. the Codex CLI token from auth.json (READ-ONLY, never refreshed)
// Both are used for a single undocumented endpoint: wham/usage.

use super::{Metric, MetricUnit, MetricWindow, Provider, ProviderData, ProviderId, ProviderStatus};
use crate::config::schema::ProviderConfig;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tracing::warn;

/// Undocumented ChatGPT subscription usage endpoint — verified 2026-06-10, HTTP 200.
const WHAM_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// Credential Manager label of the manual usage token.
const USAGE_TOKEN_LABEL: &str = "codex_usage_token";

pub struct CodexProvider {
    config: ProviderConfig,
    http: reqwest::Client,
}

impl CodexProvider {
    pub fn new(config: ProviderConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            // Never follow a 3xx — keeps the bearer token from leaking to a
            // redirected host. These endpoints return 200 directly.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build HTTP client");
        Self { config, http }
    }

    /// Manual usage token; tried before the Codex CLI token.
    fn get_usage_token(&self) -> Option<String> {
        keyring::Entry::new("ailimits", USAGE_TOKEN_LABEL)
            .and_then(|e| e.get_password())
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
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
    async fn fetch_via_subscription(&self) -> Result<ProviderData> {
        // 1. Manual usage token, if stored. Rejected → fall through to auth.json.
        if let Some(token) = self.get_usage_token() {
            match self.fetch_usage(&token).await {
                Ok(Some(data)) => return Ok(data),
                Ok(None) => warn!("stored Codex usage token was rejected, falling back"),
                Err(e) => warn!("stored Codex usage token fetch failed: {e}"),
            }
        }

        // 2. Codex CLI token.
        let token = match read_codex_token().await {
            Ok(Some(t)) => t,
            Ok(None) => {
                // No auth.json — hint to log into Codex CLI.
                return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
            }
            Err(e) => {
                warn!("codex auth.json parse error: {e}");
                return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
            }
        };

        match self.fetch_usage(&token).await {
            Ok(Some(data)) => Ok(data),
            Ok(None) => Ok(self.data(
                ProviderStatus::AuthError("token expired — run Codex CLI once".to_string()),
                vec![],
            )),
            Err(e) => Ok(self.data(ProviderStatus::NetworkError(e.to_string()), vec![])),
        }
    }

    /// One usage request. Ok(None) = the token was rejected (401/403).
    async fn fetch_usage(&self, token: &str) -> Result<Option<ProviderData>> {
        let resp = self
            .http
            .get(WHAM_USAGE_URL)
            .bearer_auth(token)
            .header("User-Agent", "ailimits-widget")
            .send()
            .await?;

        match resp.status().as_u16() {
            200 => {
                let body = resp.text().await?;
                let metrics = parse_wham_usage(&body)?;
                if metrics.is_empty() {
                    // Undocumented endpoint: an unknown schema is an honest error,
                    // not invented numbers.
                    return Ok(Some(self.data(
                        ProviderStatus::NetworkError("unrecognized wham/usage schema".to_string()),
                        vec![],
                    )));
                }
                Ok(Some(self.data(ProviderStatus::Ok, metrics)))
            }
            401 | 403 => Ok(None),
            other => Ok(Some(self.data(
                ProviderStatus::NetworkError(format!("HTTP {other}")),
                vec![],
            ))),
        }
    }
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
        self.fetch_via_subscription().await
    }
}

/// Read the Codex CLI access token: %USERPROFILE%\.codex\auth.json → tokens.access_token.
/// Ok(None) if the file or the token is missing.
///
/// Structure verified 2026-06-10: {"auth_mode": ..., "tokens": {"access_token": ...}, ...}
async fn read_codex_token() -> Result<Option<String>> {
    let path = dirs::home_dir()
        .unwrap_or_default()
        .join(".codex")
        .join("auth.json");
    if !path.exists() {
        return Ok(None);
    }
    let content = tokio::fs::read_to_string(&path).await?;
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
            });
        }
    };

    // Order matters: the first metric drives the progress bar.
    push_window("primary_window", "Session", MetricWindow::Session);
    push_window("secondary_window", "Weekly", MetricWindow::Long);

    Ok(metrics)
}
