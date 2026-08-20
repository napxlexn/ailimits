// providers/copilot.rs — GitHub Copilot provider (subscription).
// See docs/en/PROVIDERS.md, section "GitHub Copilot".
//
// Single method: the internal quota endpoint copilot_internal/user (the same
// data VS Code shows). Token: a PAT from Credential Manager, or the gh CLI.

use super::{Metric, MetricUnit, MetricWindow, Provider, ProviderData, ProviderId, ProviderStatus};
use crate::config::schema::ProviderConfig;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use tracing::warn;

/// Internal Copilot subscription quota endpoint — verified 2026-06-10, HTTP 200.
const COPILOT_INTERNAL_URL: &str = "https://api.github.com/copilot_internal/user";

/// gh token cache TTL: spawning gh.exe every cycle is the heaviest periodic op.
const GH_TOKEN_TTL: std::time::Duration = std::time::Duration::from_secs(15 * 60);

pub struct CopilotProvider {
    config: ProviderConfig,
    http: reqwest::Client,
    /// gh CLI token cache: (token, fetched_at).
    gh_token_cache: std::sync::Mutex<Option<(String, std::time::Instant)>>,
}

impl CopilotProvider {
    pub fn new(config: ProviderConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            // Never follow a 3xx — keeps the token from leaking to a
            // redirected host. The internal endpoint returns 200 directly.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build HTTP client");
        Self {
            config,
            http,
            gh_token_cache: std::sync::Mutex::new(None),
        }
    }

    /// PAT from Credential Manager.
    fn get_token(&self) -> Option<String> {
        if self.config.credential_label.is_empty() {
            return None;
        }
        keyring::Entry::new("ailimits", &self.config.credential_label)
            .and_then(|e| e.get_password())
            .ok()
    }

    /// gh CLI token with a 15-minute cache; a 401 invalidates it.
    async fn gh_token_cached(&self) -> Option<String> {
        if let Ok(guard) = self.gh_token_cache.lock() {
            if let Some((token, fetched_at)) = guard.as_ref() {
                if fetched_at.elapsed() < GH_TOKEN_TTL {
                    return Some(token.clone());
                }
            }
        }
        let token = gh_cli_token().await?;
        if let Ok(mut guard) = self.gh_token_cache.lock() {
            *guard = Some((token.clone(), std::time::Instant::now()));
        }
        Some(token)
    }

    /// Drop the cache — the server rejected the token.
    fn invalidate_gh_cache(&self) {
        if let Ok(mut guard) = self.gh_token_cache.lock() {
            *guard = None;
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

    /// Internal subscription quota endpoint.
    async fn fetch_via_internal(&self, token: &str) -> Result<ProviderData> {
        let resp = match self
            .http
            .get(COPILOT_INTERNAL_URL)
            // "token …" format, the same the Copilot extensions use.
            .header("Authorization", format!("token {token}"))
            .header("User-Agent", "ailimits-widget")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return Ok(self.data(ProviderStatus::NetworkError(e.to_string()), vec![])),
        };

        match resp.status().as_u16() {
            200 => {
                let body = resp.text().await?;
                let metrics = parse_copilot_quotas(&body)?;
                if metrics.is_empty() {
                    // The internal API schema changed — an honest signal.
                    return Ok(self.data(
                        ProviderStatus::NetworkError(
                            "unrecognized copilot_internal schema".to_string(),
                        ),
                        vec![],
                    ));
                }
                Ok(self.data(ProviderStatus::Ok, metrics))
            }
            401 | 403 | 404 => Ok(self.data(
                ProviderStatus::AuthError("token has no Copilot access".to_string()),
                vec![],
            )),
            other => Ok(self.data(
                ProviderStatus::NetworkError(format!("HTTP {other}")),
                vec![],
            )),
        }
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
        // Token: PAT from Credential Manager, or the cached gh CLI token.
        let (token, from_gh) = match self.get_token() {
            Some(t) => (Some(t), false),
            None => (self.gh_token_cached().await, true),
        };
        match token {
            Some(t) => {
                let result = self.fetch_via_internal(&t).await;
                // A rejected cached token → invalidate; the next fetch grabs a fresh one.
                if from_gh {
                    if let Ok(data) = &result {
                        if matches!(data.status, ProviderStatus::AuthError(_)) {
                            self.invalidate_gh_cache();
                        }
                    }
                }
                result
            }
            None => Ok(self.data(
                ProviderStatus::AuthError(
                    "no token: install gh CLI (gh auth login) or add a PAT".to_string(),
                ),
                vec![],
            )),
        }
    }
}

/// gh CLI token via `gh auth token`, without a console window flash.
async fn gh_cli_token() -> Option<String> {
    let mut cmd = tokio::process::Command::new("gh");
    cmd.args(["auth", "token"]);
    #[cfg(target_os = "windows")]
    {
        // CREATE_NO_WINDOW: no console flash from a GUI app.
        cmd.creation_flags(0x0800_0000);
        // Run from the system dir so a "gh.exe" planted in an attacker-
        // controlled working directory cannot be invoked instead (CreateProcess
        // searches the current directory before PATH) — it would receive the
        // live gh token on stdout.
        if let Some(root) = std::env::var_os("SystemRoot") {
            cmd.current_dir(std::path::Path::new(&root).join("System32"));
        }
    }
    // Bound the wait: a hung gh.exe must not stall the fetch task, which the
    // scheduler joins before its next cycle — one hang would otherwise freeze
    // every provider's updates. kill_on_drop reaps the process on timeout.
    cmd.kill_on_drop(true);
    let output = match tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output()).await
    {
        Ok(Ok(output)) => output,
        Ok(Err(_)) => return None,
        Err(_) => {
            warn!("gh auth token timed out after 15s (killed)");
            return None;
        }
    };
    if !output.status.success() {
        warn!("gh auth token failed (gh not installed or not logged in)");
        return None;
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
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
            });
        }
    };

    // Order matters: the first metric drives the progress bar.
    push_quota("premium_interactions", "Premium");
    push_quota("chat", "Chat");
    push_quota("completions", "Completions");

    Ok(metrics)
}
