// providers/claude.rs — Claude provider (API key and subscription modes).
// See docs/en/PROVIDERS.md, section "Claude".

use super::{Metric, MetricUnit, Provider, ProviderData, ProviderId, ProviderStatus};
use crate::config::schema::{AuthMethod, ProviderConfig};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tracing::warn;

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
    http: reqwest::Client,
}

impl ClaudeProvider {
    pub fn new(config: ProviderConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            // These usage endpoints never legitimately redirect; refusing 3xx
            // removes any chance of forwarding a bearer / x-api-key header to a
            // different host (reqwest only strips Authorization, not x-api-key).
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build HTTP client");

        Self { config, http }
    }

    /// API key from Windows Credential Manager.
    fn get_api_key(&self) -> Option<String> {
        if self.config.credential_label.is_empty() {
            return None;
        }
        keyring::Entry::new("ailimits", &self.config.credential_label)
            .and_then(|e| e.get_password())
            .ok()
    }

    /// Manual usage token (usage endpoint only); does not touch Claude Code credentials.
    fn get_usage_token(&self) -> Option<String> {
        keyring::Entry::new("ailimits", USAGE_TOKEN_LABEL)
            .and_then(|e| e.get_password())
            .ok()
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty())
    }

    /// Method A: API key → rate limit headers.
    async fn fetch_via_api_key(&self, api_key: &str) -> Result<ProviderData> {
        let resp = match self
            .http
            .get(CLAUDE_MODELS_URL)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(self.data(ProviderStatus::NetworkError(e.to_string()), vec![]));
            }
        };

        // 401 → invalid key; 429 → limit exhausted but the headers are still present.
        if resp.status().as_u16() == 401 {
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
            });
        }

        if metrics.is_empty() {
            return Ok(self.data(
                ProviderStatus::NetworkError("no ratelimit headers in response".to_string()),
                vec![],
            ));
        }

        Ok(self.data(ProviderStatus::Ok, metrics))
    }

    /// Method B: subscription — a chain of sources.
    ///
    /// 0. manual usage token from Credential Manager
    /// 1. OAuth /api/oauth/usage with the Claude Code token — live server-side %
    /// 2. statusline.jsonl — Claude Code status bar snapshots
    /// 3. an honest NetworkError when a source exists but has no fresh data
    /// 4. NotConfigured — "run Claude Code at least once"
    async fn fetch_via_subscription(&self) -> Result<ProviderData> {
        let claude_dir = claude_dir();
        let credentials_path = claude_dir.join(".credentials.json");
        let statusline_path = claude_dir.join("statusline.jsonl");
        // Real limit sources; stats-cache holds no limits and does not count.
        let has_source = credentials_path.exists() || statusline_path.exists();

        // Whether a usage request bounced off the endpoint's rate limiter
        // this cycle — only to word the error honestly below (a 429 is not
        // an expired token).
        let mut rate_limited = false;

        // 0. Manual usage token: a minimal read-only usage request.
        // Rejected → quietly fall back to the Claude Code sources.
        if let Some(token) = self.get_usage_token() {
            match self.fetch_via_oauth(&token, &mut rate_limited).await {
                Ok(Some(data)) => return Ok(data),
                Ok(None) => warn!("stored Claude usage token was rejected, falling back"),
                Err(e) => warn!("stored Claude usage token fetch failed: {e}"),
            }
        }

        // 1. OAuth: the Claude Code token is READ-ONLY — never refresh it,
        // refresh-token rotation would log Claude Code out.
        match read_oauth_token(&credentials_path).await {
            Ok(Some(token)) => match self.fetch_via_oauth(&token, &mut rate_limited).await {
                Ok(Some(data)) => return Ok(data),
                Ok(None) => { /* expired or 401 — fall back */ }
                Err(e) => warn!("oauth usage fetch error: {e}"),
            },
            Ok(None) => { /* no file or token — fall back */ }
            Err(e) => warn!(".credentials.json parse error: {e}"),
        }

        // 2. statusline.jsonl — server-side % from snapshots.
        match read_statusline(&statusline_path).await {
            Ok(Some(data)) => return Ok(self.finish_statusline(data)),
            Ok(None) => { /* file missing — fall back */ }
            Err(e) => warn!("statusline.jsonl parse error: {e}"),
        }

        // 3. A source exists but there is no fresh data right now — an HONEST
        // error instead of invented numbers. app.rs keeps the last real data
        // on screen; the renderer greys it out with its age.
        // On HTTP 429 (the usage endpoint's per-token bucket is shared with
        // Claude Code's own polling; verified 2026-07-09, `Retry-After: 0`)
        // the next cycle usually gets through — say so instead of falsely
        // blaming the token.
        if has_source {
            // Short message — the renderer shows it instead of a generic "offline".
            let msg = if rate_limited {
                "rate-limited, retrying"
            } else {
                "token expired"
            };
            return Ok(self.data(ProviderStatus::NetworkError(msg.to_string()), vec![]));
        }

        // 4. No sources at all — hint to run Claude Code.
        Ok(self.data(ProviderStatus::NotConfigured, vec![]))
    }

    /// One usage request; Ok(None) = the token did not work, fall back.
    /// Sets `rate_limited` on HTTP 429 so the caller can word its error honestly.
    async fn fetch_via_oauth(
        &self,
        token: &str,
        rate_limited: &mut bool,
    ) -> Result<Option<ProviderData>> {
        let resp = self
            .http
            .get(OAUTH_USAGE_URL)
            .bearer_auth(token)
            .header("anthropic-beta", OAUTH_BETA_HEADER)
            .send()
            .await?;

        match resp.status().as_u16() {
            200 => {
                let body = resp.text().await?;
                let metrics = parse_oauth_usage(&body)?;
                if metrics.is_empty() {
                    // Schema changed — falling back to statusline is more honest.
                    return Ok(None);
                }
                Ok(Some(self.data(ProviderStatus::Ok, metrics)))
            }
            // 401/403 → stale token; 429 → the shared per-token bucket is
            // momentarily empty (see fetch_via_subscription). The endpoint is
            // undocumented, so any non-200 falls back quietly.
            status => {
                if status == 429 {
                    *rate_limited = true;
                }
                warn!("oauth usage endpoint returned HTTP {status}, falling back");
                Ok(None)
            }
        }
    }

    fn data(&self, status: ProviderStatus, metrics: Vec<Metric>) -> ProviderData {
        ProviderData {
            id: self.id(),
            status,
            metrics,
            updated_at: Utc::now(),
            // Live fetch — monotonically anchored against wall-clock jumps.
            received_at: Some(std::time::Instant::now()),
        }
    }

    /// Finalize statusline data: keep the snapshot time so the UI can show its age.
    fn finish_statusline(&self, parsed: StatuslineSnapshot) -> ProviderData {
        ProviderData {
            id: self.id(),
            status: ProviderStatus::Ok,
            metrics: parsed.metrics,
            updated_at: parsed.taken_at.unwrap_or_else(Utc::now),
            // A snapshot read from a file: its staleness is the snapshot's own
            // wall-clock age, not the read time — leave received_at None.
            received_at: None,
        }
    }
}

/// Claude Code directory: %USERPROFILE%\.claude.
fn claude_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude")
}

/// Read a valid OAuth token from .credentials.json; Ok(None) if missing or expired.
///
/// Structure verified against the real file:
/// {"claudeAiOauth": {"accessToken": "...", "expiresAt": <ms epoch>, ...}}
async fn read_oauth_token(path: &PathBuf) -> Result<Option<String>> {
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
    if expires_at_ms <= Utc::now().timestamp_millis() {
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

    let mut push_window = |key: &str, label: &str| {
        let window = match value.get(key) {
            Some(w) if w.is_object() => w,
            _ => return,
        };
        let pct = window
            .get("utilization")
            .and_then(|v| v.as_f64())
            .map(|f| f.round().clamp(0.0, 100.0) as u64);
        if let Some(pct) = pct {
            let reset_at = window
                .get("resets_at")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc));
            metrics.push(pct_metric(label, pct, reset_at));
        }
    };

    // Order matters: the first metric drives the progress bar.
    push_window("five_hour", "Session");
    push_window("seven_day", "Weekly");
    push_window("seven_day_opus", "Opus");
    push_window("seven_day_sonnet", "Sonnet");

    Ok(metrics)
}

/// Parsed statusline snapshot.
struct StatuslineSnapshot {
    metrics: Vec<Metric>,
    taken_at: Option<DateTime<Utc>>,
}

/// Read the last valid line of statusline.jsonl; Ok(None) if the file is missing.
async fn read_statusline(path: &PathBuf) -> Result<Option<StatuslineSnapshot>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = tokio::fs::read_to_string(path).await?;

    // The last non-empty line that parses as JSON — lines may be truncated.
    let snapshot: serde_json::Value = content
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .find_map(|l| serde_json::from_str(l).ok())
        .ok_or_else(|| anyhow::anyhow!("no valid JSON line in statusline.jsonl"))?;

    // The schema is undocumented — search known field names tolerantly.
    let mut metrics = Vec::new();

    // 5-hour session percentage.
    if let Some(pct) = find_pct(
        &snapshot,
        &[
            "session_pct",
            "five_hour_pct",
            "session_percent",
            "sessionUsedPct",
        ],
    ) {
        metrics.push(pct_metric(
            "Session",
            pct,
            find_reset(
                &snapshot,
                &["session_reset", "session_reset_at", "sessionResetAt"],
            ),
        ));
    }
    // Weekly limit percentage.
    if let Some(pct) = find_pct(
        &snapshot,
        &["weekly_pct", "week_pct", "weekly_percent", "weeklyUsedPct"],
    ) {
        metrics.push(pct_metric(
            "Weekly",
            pct,
            find_reset(
                &snapshot,
                &["weekly_reset", "week_reset_at", "weeklyResetAt"],
            ),
        ));
    }

    if metrics.is_empty() {
        anyhow::bail!("statusline.jsonl has no recognizable limit fields");
    }

    let taken_at = find_reset(&snapshot, &["timestamp", "time", "taken_at", "ts"]);

    Ok(Some(StatuslineSnapshot { metrics, taken_at }))
}

/// Percentage metric.
fn pct_metric(label: &str, pct: u64, reset_at: Option<DateTime<Utc>>) -> Metric {
    Metric {
        label: label.to_string(),
        used: pct.min(100),
        limit: Some(100),
        unit: MetricUnit::Percent,
        reset_at,
    }
}

/// Find a numeric field by candidate names, recursing one nesting level.
fn find_pct(value: &serde_json::Value, names: &[&str]) -> Option<u64> {
    let obj = value.as_object()?;
    for name in names {
        if let Some(v) = obj.get(*name).and_then(json_as_u64) {
            return Some(v);
        }
    }
    // One level deep: {"limits": {...}} and the like.
    for nested in obj.values().filter(|v| v.is_object()) {
        for name in names {
            if let Some(v) = nested
                .as_object()
                .and_then(|o| o.get(*name))
                .and_then(json_as_u64)
            {
                return Some(v);
            }
        }
    }
    None
}

/// A number from JSON: int, float, or string ("83%" included).
fn json_as_u64(v: &serde_json::Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_f64().map(|f| f.round().max(0.0) as u64))
        .or_else(|| {
            v.as_str()
                .and_then(|s| s.trim_end_matches('%').parse().ok())
        })
}

/// Find an RFC3339 time by candidate names.
fn find_reset(value: &serde_json::Value, names: &[&str]) -> Option<DateTime<Utc>> {
    let obj = value.as_object()?;
    for name in names {
        if let Some(dt) = obj
            .get(*name)
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        {
            return Some(dt.with_timezone(&Utc));
        }
    }
    None
}

/// Header as Option<u64>.
fn header_u64(resp: &reqwest::Response, name: &str) -> Option<u64> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

/// Reset time header — RFC3339.
fn header_datetime(resp: &reqwest::Response, name: &str) -> Option<DateTime<Utc>> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
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
                updated_at: Utc::now(),
                received_at: Some(std::time::Instant::now()),
            });
        }

        match self.config.auth_method {
            AuthMethod::ApiKey => match self.get_api_key() {
                Some(key) => self.fetch_via_api_key(&key).await,
                None => Ok(ProviderData {
                    id: self.id(),
                    status: ProviderStatus::AuthError(
                        "API key not found in Credential Manager".to_string(),
                    ),
                    metrics: vec![],
                    updated_at: Utc::now(),
                    received_at: Some(std::time::Instant::now()),
                }),
            },
            AuthMethod::Subscription => self.fetch_via_subscription().await,
        }
    }
}
