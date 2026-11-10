// providers/antigravity.rs — Antigravity provider (Google's Gemini successor)
// for Code Assist quota. See docs/en/PROVIDERS.md, section "Antigravity".
//
// Mechanism: read the Antigravity OAuth token from Windows Credential Manager, then fall back to the legacy Gemini CLI file.

use super::{Metric, MetricUnit, MetricWindow, Provider, ProviderData, ProviderId, ProviderStatus};
use crate::config::schema::ProviderConfig;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tracing::warn;

/// Onboarding endpoint that reports the account's Code Assist project id.
/// Quota queries need it — every quota endpoint answers a misleading
/// "everything full" default view without a project id (verified 2026-07-09).
const LOAD_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";
/// The endpoint Antigravity's own quota manager uses (verified 2026-07-09):
/// per-model `quotaInfo` with the live remainingFraction/resetTime for the
/// pools Antigravity actually consumes — retrieveUserQuota does NOT track
/// those. Needs the project id in the body and an identified client (the
/// User-Agent / X-Goog-Api-Client headers below), otherwise 403.
const MODELS_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels";
/// The shared quota pools Antigravity meters today ("Gemini Models",
/// "Claude and GPT models"), verified live 2026-08-01. Needs the project id in
/// the body and an identified client; without the project it answers a single
/// synthetic "All Models" group with everything full.
const SUMMARY_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary";
/// The Windows Credential Manager target Antigravity keeps its token under.
const ANTIGRAVITY_CREDENTIAL_TARGET: &str = "gemini:antigravity";

pub struct AntigravityProvider {
    config: ProviderConfig,
    http: reqwest::Client,
    /// Code Assist project id from loadCodeAssist, cached after the first
    /// success — it is stable for the account, and quota queries need it.
    /// Forgotten when the token is rejected (see `token_rejected`).
    project: std::sync::Mutex<Option<String>>,
}

/// Outcome of one quota request (retrieveUserQuotaSummary or
/// fetchAvailableModels).
enum QuotaOutcome {
    /// Usable metrics.
    Metrics(Vec<Metric>),
    /// The endpoint answered, but with nothing we can trust (non-200 other
    /// than 401/403, an unparseable body, or a recognized-but-empty shape).
    Unusable,
    /// The token was rejected (401/403). No other source will do better —
    /// callers must stop the fallback chain and report this directly.
    TokenRejected,
}

/// What the Antigravity token sources hold for the widget.
pub enum AntigravityLogin {
    /// No token anywhere: Antigravity (or the Gemini CLI) was never signed
    /// in here, or was signed out.
    Missing,
    /// A token exists, but none is usable: every one is past its expiry (or
    /// states none). Only Antigravity may refresh it, so the widget waits.
    Expired,
    /// A usable access token.
    Token(String),
}

// Written by hand so that no debug print can ever carry the token.
impl std::fmt::Debug for AntigravityLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AntigravityLogin::Missing => f.write_str("Missing"),
            AntigravityLogin::Expired => f.write_str("Expired"),
            AntigravityLogin::Token(_) => f.write_str("Token(..)"),
        }
    }
}

/// Outcome of resolving the Code Assist project id.
enum ProjectResolution {
    /// Resolved project id (from cache or a fresh loadCodeAssist call).
    Project(String),
    /// The endpoint answered, but with nothing we can use.
    Unusable,
    /// The token was rejected (401/403).
    TokenRejected,
}

/// Identify like Antigravity's own client. Google treats anonymous callers
/// differently on every Code Assist endpoint: `loadCodeAssist` answers 200 but
/// silently omits `cloudaicompanionProject`, `fetchAvailableModels` and
/// `retrieveUserQuotaSummary` answer 403 (verified live 2026-08-01).
fn identified(req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    req.header("User-Agent", "antigravity/1.0")
        .header("X-Goog-Api-Client", "gl-go antigravity")
}

impl AntigravityProvider {
    pub fn new(config: ProviderConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            // Never follow a 3xx — keeps the bearer token from leaking to a
            // redirected host. The quota endpoint returns 200 directly.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build HTTP client");
        Self {
            config,
            http,
            project: std::sync::Mutex::new(None),
        }
    }

    /// One fetchAvailableModels request. 401/403 → TokenRejected; any other
    /// non-200 or an unparseable/empty body → Unusable.
    async fn fetch_models_quota(&self, token: &str, project: &str) -> Result<QuotaOutcome> {
        let resp = identified(self.http.post(MODELS_URL))
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .body(serde_json::json!({ "project": project }).to_string())
            .send()
            .await?;
        match resp.status().as_u16() {
            200 => {}
            401 | 403 => return Ok(QuotaOutcome::TokenRejected),
            other => {
                warn!("fetchAvailableModels returned HTTP {other}");
                return Ok(QuotaOutcome::Unusable);
            }
        }
        let body = resp.text().await?;
        let metrics = parse_available_models_quota(&body)?;
        if metrics.is_empty() {
            Ok(QuotaOutcome::Unusable)
        } else {
            Ok(QuotaOutcome::Metrics(metrics))
        }
    }

    /// One retrieveUserQuotaSummary request. 401/403 → TokenRejected; any
    /// other non-200 or an unparseable/empty body → Unusable.
    async fn fetch_quota_summary(&self, token: &str, project: &str) -> Result<QuotaOutcome> {
        let resp = identified(self.http.post(SUMMARY_URL))
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .body(serde_json::json!({ "project": project }).to_string())
            .send()
            .await?;
        match resp.status().as_u16() {
            200 => {}
            401 | 403 => return Ok(QuotaOutcome::TokenRejected),
            other => {
                warn!("retrieveUserQuotaSummary returned HTTP {other}");
                return Ok(QuotaOutcome::Unusable);
            }
        }
        let body = resp.text().await?;
        let metrics = parse_quota_summary(&body)?;
        if metrics.is_empty() {
            Ok(QuotaOutcome::Unusable)
        } else {
            Ok(QuotaOutcome::Metrics(metrics))
        }
    }

    /// Resolve the Code Assist project id (cached). Unusable/TokenRejected on
    /// failure — callers must treat both as fatal.
    async fn resolve_project(&self, token: &str) -> ProjectResolution {
        if let Some(p) = self.project.lock().ok().and_then(|g| g.clone()) {
            return ProjectResolution::Project(p);
        }
        let resp = match identified(self.http.post(LOAD_URL))
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .body("{}")
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                warn!("loadCodeAssist request failed: {e}");
                return ProjectResolution::Unusable;
            }
        };
        match resp.status().as_u16() {
            200 => {}
            401 | 403 => return ProjectResolution::TokenRejected,
            other => {
                warn!("loadCodeAssist returned HTTP {other}");
                return ProjectResolution::Unusable;
            }
        }
        let body = match resp.text().await {
            Ok(body) => body,
            Err(e) => {
                warn!("loadCodeAssist body read failed: {e}");
                return ProjectResolution::Unusable;
            }
        };
        let Some(project) = parse_load_code_assist_project(&body) else {
            warn!("loadCodeAssist returned no cloudaicompanionProject");
            return ProjectResolution::Unusable;
        };
        if let Ok(mut guard) = self.project.lock() {
            *guard = Some(project.clone());
        }
        ProjectResolution::Project(project)
    }

    /// The single message produced when Antigravity rejects the token — used
    /// at every request site so the wording never drifts.
    ///
    /// Also forgets the cached project id: after a switch to another Google
    /// account the old account's project is what gets refused, and only a
    /// fresh loadCodeAssist with the new token names the right one. Otherwise
    /// every cycle would read "token rejected" until the widget restarts.
    fn token_rejected(&self) -> ProviderData {
        if let Ok(mut guard) = self.project.lock() {
            *guard = None;
        }
        self.data(
            ProviderStatus::AuthError(
                "Antigravity token rejected — run Antigravity CLI once".to_string(),
            ),
            vec![],
        )
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

    async fn fetch_via_subscription(&self) -> Result<ProviderData> {
        let token = match read_antigravity_login().await {
            Ok(AntigravityLogin::Token(t)) => t,
            // Never signed in here — the row stays hidden.
            Ok(AntigravityLogin::Missing) => {
                return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
            }
            // A login that has lapsed is an honest error, not a missing one:
            // app.rs keeps the last real data on screen, greyed with its age.
            Ok(AntigravityLogin::Expired) => {
                return Ok(self.data(
                    ProviderStatus::NetworkError("token expired".to_string()),
                    vec![],
                ));
            }
            // A credential that exists but cannot be read: honest, never
            // "not set up". The message never quotes the secret.
            Err(e) => {
                warn!("antigravity credentials unreadable: {e}");
                return Ok(self.data(
                    ProviderStatus::NetworkError("credentials unreadable".to_string()),
                    vec![],
                ));
            }
        };

        // The project id is REQUIRED — report an honest error instead of
        // rendering numbers we know may be wrong.
        let project = match self.resolve_project(&token).await {
            ProjectResolution::Project(p) => p,
            ProjectResolution::TokenRejected => return Ok(self.token_rejected()),
            ProjectResolution::Unusable => {
                return Ok(self.data(
                    ProviderStatus::NetworkError(
                        "no Code Assist project id — open Antigravity once".to_string(),
                    ),
                    vec![],
                ));
            }
        };

        // Primary: the shared pools Antigravity itself shows.
        match self.fetch_quota_summary(&token, &project).await {
            Ok(QuotaOutcome::Metrics(metrics)) => {
                return Ok(self.data(ProviderStatus::Ok, metrics));
            }
            Ok(QuotaOutcome::TokenRejected) => return Ok(self.token_rejected()),
            Ok(QuotaOutcome::Unusable) => {
                warn!("retrieveUserQuotaSummary returned no usable pools, falling back")
            }
            Err(e) => warn!("retrieveUserQuotaSummary failed, falling back: {e}"),
        }

        // Per-model quota (the pools Antigravity itself meters).
        match self.fetch_models_quota(&token, &project).await {
            Ok(QuotaOutcome::Metrics(metrics)) => {
                return Ok(self.data(ProviderStatus::Ok, metrics));
            }
            Ok(QuotaOutcome::TokenRejected) => return Ok(self.token_rejected()),
            Ok(QuotaOutcome::Unusable) => warn!("fetchAvailableModels returned no usable quota"),
            Err(e) => warn!("fetchAvailableModels failed: {e}"),
        }

        // Both real sources failed or returned nothing usable. There is no
        // further fallback: retrieveUserQuota answers the Code Assist bucket
        // view, which does NOT track Antigravity consumption — an
        // Antigravity-only account reads remainingFraction=1 there
        // regardless of actual usage. Showing that would be ProviderStatus::Ok
        // at 0% used while the real pool may be fully spent, which is worse
        // than showing nothing.
        Ok(self.data(
            ProviderStatus::NetworkError("Antigravity quota unavailable".to_string()),
            vec![],
        ))
    }
}

#[async_trait]
impl Provider for AntigravityProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Antigravity
    }

    async fn fetch(&self) -> Result<ProviderData> {
        if !self.config.enabled {
            return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
        }
        self.fetch_via_subscription().await
    }
}

/// Read the Antigravity login: the keyring first, then the legacy Gemini CLI
/// file (only read when the keyring holds no usable token).
async fn read_antigravity_login() -> Result<AntigravityLogin> {
    let keyring = read_antigravity_keyring_login();
    if let Ok(AntigravityLogin::Token(token)) = keyring {
        return Ok(AntigravityLogin::Token(token));
    }
    combine_logins(keyring, read_legacy_gemini_login().await)
}

/// One login from the two sources. A usable token wins; otherwise an expired
/// one says the account was signed in here (an honest "token expired", the
/// last data kept); an unreadable source is an error unless the other one
/// says more; only with neither is the login missing.
pub fn combine_logins(
    keyring: Result<AntigravityLogin>,
    legacy: Result<AntigravityLogin>,
) -> Result<AntigravityLogin> {
    use AntigravityLogin::{Expired, Missing, Token};
    match (keyring, legacy) {
        (Ok(Token(token)), _) | (_, Ok(Token(token))) => Ok(Token(token)),
        (Ok(Expired), _) | (_, Ok(Expired)) => Ok(Expired),
        (Err(e), _) | (_, Err(e)) => Err(e),
        (Ok(Missing), Ok(Missing)) => Ok(Missing),
    }
}

/// Read the legacy Gemini CLI login. A missing file is `Missing`.
async fn read_legacy_gemini_login() -> Result<AntigravityLogin> {
    let path = dirs::home_dir()
        .unwrap_or_default()
        .join(".gemini")
        .join("oauth_creds.json");
    if !path.exists() {
        return Ok(AntigravityLogin::Missing);
    }
    let content = tokio::fs::read_to_string(&path).await?;
    parse_gemini_oauth_creds(&content, Utc::now())
}

/// Parse the legacy Gemini CLI `oauth_creds.json`:
/// {"access_token": "...", "expiry_date": <ms epoch>, ...}.
pub fn parse_gemini_oauth_creds(body: &str, now: DateTime<Utc>) -> Result<AntigravityLogin> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let Some(token) = value
        .get("access_token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    else {
        return Ok(AntigravityLogin::Missing);
    };
    let expiry_ms = value
        .get("expiry_date")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    if expiry_ms <= now.timestamp_millis() {
        return Ok(AntigravityLogin::Expired);
    }
    Ok(AntigravityLogin::Token(token.to_string()))
}

/// Read Antigravity's keyring login. No secret is `Missing`.
fn read_antigravity_keyring_login() -> Result<AntigravityLogin> {
    let Some(secret) = read_antigravity_keyring_secret()? else {
        return Ok(AntigravityLogin::Missing);
    };
    parse_antigravity_keyring_token(&secret)
}

/// Read the raw Antigravity keyring JSON.
#[cfg(target_os = "windows")]
fn read_antigravity_keyring_secret() -> Result<Option<String>> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target: Vec<u16> = ANTIGRAVITY_CREDENTIAL_TARGET
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut credential: *mut CREDENTIALW = null_mut();
    let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(1168) {
            return Ok(None);
        }
        return Err(err.into());
    }

    struct CredentialGuard(*mut CREDENTIALW);
    impl Drop for CredentialGuard {
        fn drop(&mut self) {
            unsafe { CredFree(self.0.cast()) };
        }
    }
    let _guard = CredentialGuard(credential);

    let credential = unsafe { &*credential };
    if credential.CredentialBlob.is_null() || credential.CredentialBlobSize == 0 {
        return Ok(None);
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(
            credential.CredentialBlob,
            credential.CredentialBlobSize as usize,
        )
    };
    Ok(Some(String::from_utf8(bytes.to_vec())?))
}

/// Non-Windows builds have no Antigravity Credential Manager target.
#[cfg(not(target_os = "windows"))]
fn read_antigravity_keyring_secret() -> Result<Option<String>> {
    Ok(None)
}

/// Whether the Antigravity Credential Manager target holds a usable (unexpired)
/// access token — for the `ailimits-auth status` diagnostic, which must not
/// duplicate the platform read.
pub fn keyring_token_present() -> bool {
    match read_antigravity_keyring_secret() {
        Ok(Some(secret)) => matches!(
            parse_antigravity_keyring_token(&secret),
            Ok(AntigravityLogin::Token(_))
        ),
        _ => false,
    }
}

/// Parse Antigravity keyring JSON into a login:
/// {"token": {"access_token": "...", "expiry": "RFC3339", ...}, ...}.
/// A token past its expiry, or with none the parser can read, is `Expired`:
/// it is never sent.
pub fn parse_antigravity_keyring_token(body: &str) -> Result<AntigravityLogin> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let Some(token) = value.get("token").and_then(|v| v.as_object()) else {
        return Ok(AntigravityLogin::Missing);
    };
    let Some(access_token) = token
        .get("access_token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    else {
        return Ok(AntigravityLogin::Missing);
    };

    let expires_at = token
        .get("expiry")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    Ok(match expires_at {
        Some(expiry) if expiry > Utc::now() => AntigravityLogin::Token(access_token.to_string()),
        _ => AntigravityLogin::Expired,
    })
}

/// Parse the retrieveUserQuotaSummary response. Verified live 2026-08-01:
/// `{"groups": [{"displayName": "Gemini Models", "buckets": [{"bucketId": "…",
///   "displayName": "Weekly Limit", "window": "…", "resetTime": "RFC3339",
///   "remainingFraction": 0}]}, …], "description": "…"}`.
pub fn parse_quota_summary(body: &str) -> Result<Vec<Metric>> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let Some(groups) = value.get("groups").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };

    let mut metrics = Vec::new();
    for group in groups {
        let name = group
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        // The project-less default view: one catch-all group, everything full.
        // Treat it as no data rather than as a full quota.
        if name.eq_ignore_ascii_case("All Models") {
            return Ok(Vec::new());
        }
        let Some(buckets) = group.get("buckets").and_then(|v| v.as_array()) else {
            continue;
        };
        for bucket in buckets {
            let reset_at = bucket
                .get("resetTime")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .filter(|dt| dt.timestamp() > 0);
            // A spent pool drops remainingFraction and keeps only the reset —
            // that is zero remaining, not an unknown quota.
            let frac = match bucket.get("remainingFraction").and_then(|v| v.as_f64()) {
                Some(f) => f,
                None if reset_at.is_some() => 0.0,
                None => continue,
            };
            metrics.push(Metric {
                label: short_pool_label(name),
                used: ((1.0 - frac.clamp(0.0, 1.0)) * 100.0).round() as u64,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at,
                // These pools ARE Antigravity's general limits; it has no
                // session window, so a spent pool must drive every surface.
                window: MetricWindow::Long,
                unified_pool: false,
            });
        }
    }
    Ok(metrics)
}

/// Compact pool name for the widget's narrow rows.
fn short_pool_label(display_name: &str) -> String {
    let lower = display_name.to_ascii_lowercase();
    if lower.contains("gemini") {
        "Gemini".to_string()
    } else if lower.contains("claude") || lower.contains("gpt") {
        "Claude/GPT".to_string()
    } else {
        display_name.to_string()
    }
}

/// Parse the fetchAvailableModels response. Verified live 2026-07-09:
/// `{"models": {"gemini-2.5-pro": {"displayName": "Gemini 2.5 Pro",
///   "quotaInfo": {"remainingFraction": 0.63, "resetTime": "RFC3339"}, …}, …}}`.
/// Every Gemini model shares one pool today (same fraction + reset), so pools
/// are deduped; non-Gemini models (Antigravity also fronts Claude / GPT-OSS)
/// belong to other providers' stories and are skipped.
pub fn parse_available_models_quota(body: &str) -> Result<Vec<Metric>> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let Some(models) = value.get("models").and_then(|v| v.as_object()) else {
        return Ok(Vec::new());
    };

    // (fraction milli-units, reset string) → best (rank, version, label).
    let mut pools: std::collections::HashMap<(u64, String), (u8, f64, String, Metric)> =
        std::collections::HashMap::new();
    for (model_id, info) in models {
        if !model_id.starts_with("gemini") {
            continue;
        }
        let Some(quota) = info.get("quotaInfo") else {
            continue;
        };
        let reset_str = quota
            .get("resetTime")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        // A spent pool drops remainingFraction and keeps only the reset — zero
        // remaining, not an unknown quota (verified live 2026-08-01).
        let frac = match quota.get("remainingFraction").and_then(|v| v.as_f64()) {
            Some(f) => f,
            None if !reset_str.is_empty() => 0.0,
            None => continue,
        };
        let reset_at = DateTime::parse_from_rfc3339(&reset_str)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
            .filter(|dt| dt.timestamp() > 0);
        let label = model_label(model_id);
        let metric = Metric {
            label: label.clone(),
            used: ((1.0 - frac.clamp(0.0, 1.0)) * 100.0).round() as u64,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at,
            window: MetricWindow::Session,
            unified_pool: false,
        };
        let key = ((frac.clamp(0.0, 1.0) * 1000.0).round() as u64, reset_str);
        let rank = model_rank(&label);
        let version = model_version(&label);
        // Represent each pool by its highest-ranked, newest member.
        match pools.get(&key) {
            Some((r, v, _, _)) if (*r, -*v) <= (rank, -version) => {}
            _ => {
                pools.insert(key, (rank, version, label, metric));
            }
        }
    }

    let mut metrics: Vec<Metric> = pools.into_values().map(|(_, _, _, m)| m).collect();
    metrics.sort_by(|a, b| {
        model_rank(&a.label)
            .cmp(&model_rank(&b.label))
            .then_with(|| {
                model_version(&b.label)
                    .partial_cmp(&model_version(&a.label))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    metrics.truncate(4);
    Ok(metrics)
}

/// Project id from the loadCodeAssist response, e.g.
/// `{"cloudaicompanionProject": "sacred-airline-12rdt", …}`.
pub fn parse_load_code_assist_project(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("cloudaicompanionProject")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// "gemini-2.5-pro" → "2.5 Pro", "gemini-2.5-flash-lite" → "2.5 Flash-Lite".
fn model_label(model: &str) -> String {
    let m = model.strip_prefix("gemini-").unwrap_or(model);
    match m.split_once('-') {
        Some((ver, rest)) => {
            let name = rest.split('-').map(title).collect::<Vec<_>>().join("-");
            format!("{ver} {name}")
        }
        None => title(m),
    }
}

fn title(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Leading version number of a label ("3.1 Pro-Preview" → 3.1) for
/// newest-first ordering inside a class; 0.0 when there is none.
fn model_version(label: &str) -> f64 {
    label
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// Sort key: Pro first, then plain Flash, then Flash-Lite, then others.
fn model_rank(label: &str) -> u8 {
    let l = label.to_lowercase();
    if l.contains("pro") {
        0
    } else if l.contains("flash-lite") {
        2
    } else if l.contains("flash") {
        1
    } else {
        3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rejected_token_forgets_the_cached_project() {
        let provider = AntigravityProvider::new(ProviderConfig {
            id: "antigravity".to_string(),
            enabled: true,
            auth_method: Default::default(),
            credential_label: String::new(),
            alert_threshold: 80,
        });
        // The project of the account signed in before the switch.
        *provider.project.lock().unwrap() = Some("old-account-project".to_string());

        let data = provider.token_rejected();
        assert!(matches!(data.status, ProviderStatus::AuthError(_)));
        assert_eq!(
            *provider.project.lock().unwrap(),
            None,
            "the next cycle must ask loadCodeAssist again with the current token"
        );
    }
}
