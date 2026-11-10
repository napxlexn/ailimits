// providers/cursor.rs — Cursor provider (the Cursor subscription).
// See docs/en/PROVIDERS.md, section "Cursor".
//
// Token: the widget's own session first — signed in from the menu or with
// `ailimits-auth login cursor`, kept in Credential Manager (session.rs).
// Without one, the editor's session token from its VS Code state database:
// READ-ONLY, never refreshed, and only while the editor keeps it as plain
// text. A value the editor encrypted is left alone, never decrypted.
// Usage: the dashboard service the editor itself calls — the current
// period's usage on every cycle, the plan at most hourly, and Grok Bot's
// status as an extra metric.

use super::{Metric, MetricUnit, MetricWindow, Provider, ProviderData, ProviderId, ProviderStatus};
use crate::config::schema::ProviderConfig;
use crate::platform::sqlite::{self, SqliteError};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use session::{OwnSession, RefreshAnswer, StoreError};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

pub mod session;

/// The dashboard service the Cursor editor calls (Connect protocol, JSON).
/// Undocumented; each method below verified 2026-11-10, HTTP 200.
const DASHBOARD_URL: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService";
/// The current period's included usage — the headline.
const CURRENT_PERIOD_USAGE: &str = "GetCurrentPeriodUsage";
/// The plan's name and the period its usage is counted in.
const PLAN_INFO: &str = "GetPlanInfo";
/// Grok Bot's usage — an extra metric.
const SAND_USAGE_STATUS: &str = "GetSandUsageStatus";

/// How long a plan answer is trusted: plans rarely change.
const PLAN_TTL: Duration = Duration::from_secs(3600);
/// When to ask again after a plan request that brought nothing.
const PLAN_RETRY: Duration = Duration::from_secs(600);

/// The editor's own session token in its state database.
const ACCESS_TOKEN_KEY: &str = "cursorAuth/accessToken";
/// VS Code keeps global state in one key/value table.
const ITEM_QUERY: &str = "SELECT value FROM ItemTable WHERE key = ?1";
/// How the editor marks a value it encrypted at rest. Such a value is never
/// decrypted: the widget signs in by itself instead.
const ENCRYPTED_PREFIX: &str = "cursor-safe-storage:";

/// The row's message once the widget's own session is over.
const SIGN_IN_AGAIN: &str = "sign in to Cursor again";

pub struct CursorProvider {
    config: ProviderConfig,
    http: reqwest::Client,
    /// The last plan answer, for the account it was asked for.
    plan: Mutex<Option<PlanCache>>,
}

/// One cached plan answer.
struct PlanCache {
    /// Whose plan: the token's `sub`.
    subject: String,
    /// None while no request has brought one.
    plan: Option<PlanInfo>,
    /// When to ask again.
    next_at: Instant,
}

/// Which token a request was made with: the answers to a refused one
/// differ.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Source {
    /// The widget's own session.
    Own,
    /// The editor's token, read-only.
    Editor,
}

impl CursorProvider {
    pub fn new(config: ProviderConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            // Never follow a 3xx — keeps the bearer token from leaking to a
            // redirected host. The service answers 200 directly.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build HTTP client");
        Self {
            config,
            http,
            plan: Mutex::new(None),
        }
    }

    fn data(&self, status: ProviderStatus, metrics: Vec<Metric>) -> ProviderData {
        ProviderData {
            id: self.id(),
            status,
            metrics,
            updated_at: Utc::now(),
            received_at: Some(Instant::now()),
        }
    }

    /// An honest error row (the message fits the row: 32 characters at most).
    fn error(&self, msg: &str) -> ProviderData {
        self.data(ProviderStatus::NetworkError(msg.to_string()), vec![])
    }

    /// The own session is over: only a new sign-in mends it. The last data
    /// stays on screen, greyed, and the row says what to do.
    fn sign_in_again(&self) -> ProviderData {
        self.data(ProviderStatus::AuthError(SIGN_IN_AGAIN.to_string()), vec![])
    }

    /// The own session first; without one, the editor's plain token.
    async fn fetch_via_subscription(&self) -> ProviderData {
        // A synchronous read, like the other providers' keyring reads.
        match session::load() {
            Ok(Some(own)) => self.fetch_with_own_session(own).await,
            Ok(None) => self.fetch_with_editor_token().await,
            // A record this version cannot read: a new sign-in replaces it.
            Err(StoreError::Corrupt) => {
                warn!("cursor: the stored session is unreadable");
                self.sign_in_again()
            }
            Err(e @ StoreError::Unavailable(_)) => {
                warn!("cursor: {e}");
                self.error("Credential Manager unavailable")
            }
        }
    }

    /// Usage with the widget's own session, refreshed near its expiry.
    async fn fetch_with_own_session(&self, mut own: OwnSession) -> ProviderData {
        if own.ended {
            return self.sign_in_again();
        }
        let mut renewed = false;
        if own.needs_refresh(Utc::now()) {
            (own, renewed) = self.refresh(own).await;
        }
        // An expired token is never sent; a refresh could not renew it.
        if own.ended || own.is_expired(Utc::now()) {
            return self.sign_in_again();
        }

        let mut answer = self.call(CURRENT_PERIOD_USAGE, &own.access_token).await;
        if matches!(answer, Ok((code, _)) if is_rejected(code)) {
            // Refused. Near its expiry one refresh may still save the
            // session; otherwise it is over.
            if !renewed && own.near_expiry(Utc::now()) {
                (own, renewed) = self.refresh(own).await;
                if renewed {
                    answer = self.call(CURRENT_PERIOD_USAGE, &own.access_token).await;
                }
            }
            if !renewed || matches!(answer, Ok((code, _)) if is_rejected(code)) {
                let ended = OwnSession {
                    ended: true,
                    ..own.clone()
                };
                keep(&own.access_token, &ended);
                return self.sign_in_again();
            }
        }
        self.finish(&own.access_token, answer, Source::Own).await
    }

    /// One refresh of the widget's own session — never of the editor's
    /// token. The result is kept; `true` when a new token came.
    async fn refresh(&self, own: OwnSession) -> (OwnSession, bool) {
        let Some(answer) = session::request_refresh(&self.http, &own).await else {
            // No answer at all: nothing learnt, a later cycle tries again.
            return (own, false);
        };
        let next = own.after_refresh(&answer, Utc::now());
        keep(&own.access_token, &next);
        (next, matches!(answer, RefreshAnswer::Renewed(_)))
    }

    /// Usage with the editor's token, while it is a plain one. Read-only:
    /// never refreshed, and never sent past its `exp`.
    async fn fetch_with_editor_token(&self) -> ProviderData {
        let path = state_db_path();
        if !path.exists() {
            // Cursor is not installed (or has never run) for this user.
            return self.data(ProviderStatus::NotConfigured, vec![]);
        }

        // One indexed lookup, about a millisecond, bounded by the busy timeout.
        let token = match read_editor_token(&path) {
            Ok(EditorToken::Plain(token)) => token,
            // Nobody signed in to the editor, or it keeps the token
            // encrypted: either way there is no session to read, and the
            // row stays hidden until the user signs in from the menu.
            Ok(EditorToken::Absent | EditorToken::Encrypted) => {
                return self.data(ProviderStatus::NotConfigured, vec![]);
            }
            // The editor is writing right now — transient, the next cycle retries.
            Err(SqliteError::Busy) => return self.error("state database busy, retrying"),
            Err(e @ SqliteError::Unavailable(_)) => {
                warn!("cursor state database: {e}");
                return self.error("winsqlite3.dll unavailable");
            }
            Err(e) => {
                warn!("cursor state database: {e}");
                return self.error("state database unreadable");
            }
        };

        let Some(claims) = parse_session_claims(&token) else {
            return self.error("unrecognized session token");
        };
        if claims.is_expired(Utc::now()) {
            return self.error("token expired");
        }
        let answer = self.call(CURRENT_PERIOD_USAGE, &token).await;
        self.finish(&token, answer, Source::Editor).await
    }

    /// Turn the usage answer into the row: the headline, labelled with the
    /// plan, then Grok Bot when its status reads.
    async fn finish(
        &self,
        token: &str,
        answer: std::result::Result<(u16, String), String>,
        source: Source,
    ) -> ProviderData {
        let (code, body) = match answer {
            Ok(answer) => answer,
            Err(e) => {
                warn!("cursor usage request failed: {e}");
                return self.error("offline, retrying");
            }
        };
        let usage = match interpret_period_usage(code, &body) {
            Ok(usage) => usage,
            Err(e) => {
                let (status, _) = usage_error_status(&e, source == Source::Own);
                return self.data(status, vec![]);
            }
        };
        let plan = self.plan(token).await;
        let mut metrics = vec![usage_metric(&usage, plan.as_ref())];
        if let Some(sand) = self.sand_usage(token).await {
            metrics.push(sand_metric(&sand));
        }
        self.data(ProviderStatus::Ok, metrics)
    }

    /// The plan, from the cache while it is fresh: one request an hour at
    /// most. A request that brings nothing keeps the last known plan and is
    /// tried again in ten minutes.
    async fn plan(&self, token: &str) -> Option<PlanInfo> {
        let subject = parse_session_claims(token)
            .map(|claims| claims.subject)
            .unwrap_or_default();
        let now = Instant::now();
        let known = {
            let cache = self.plan.lock().unwrap_or_else(PoisonError::into_inner);
            match cache.as_ref().filter(|c| c.subject == subject) {
                Some(c) if now < c.next_at => return c.plan.clone(),
                Some(c) => c.plan.clone(),
                None => None,
            }
        };

        let fresh = match self.call(PLAN_INFO, token).await {
            Ok((200, body)) => {
                let plan = parse_plan_info(&body);
                if plan.is_none() {
                    debug!("cursor plan info: unknown schema");
                }
                plan
            }
            Ok((code, _)) => {
                debug!("cursor plan info returned HTTP {code}");
                None
            }
            Err(e) => {
                debug!("cursor plan info request failed: {e}");
                None
            }
        };
        let (plan, next_at) = match fresh {
            Some(plan) => (Some(plan), now + PLAN_TTL),
            None => (known, now + PLAN_RETRY),
        };
        *self.plan.lock().unwrap_or_else(PoisonError::into_inner) = Some(PlanCache {
            subject,
            plan: plan.clone(),
            next_at,
        });
        plan
    }

    /// Grok Bot's usage. An extra: whatever goes wrong only leaves it out.
    async fn sand_usage(&self, token: &str) -> Option<SandUsage> {
        match self.call(SAND_USAGE_STATUS, token).await {
            Ok((200, body)) => {
                let sand = parse_sand_usage(&body);
                if sand.is_none() {
                    debug!("cursor grok bot status: unknown schema");
                }
                sand
            }
            Ok((code, _)) => {
                debug!("cursor grok bot status returned HTTP {code}");
                None
            }
            Err(e) => {
                debug!("cursor grok bot status request failed: {e}");
                None
            }
        }
    }

    /// One dashboard call: POST `{}` with the bearer token, as the editor
    /// sends it. The body is read only from a 200; Err is a request that got
    /// no answer.
    async fn call(&self, method: &str, token: &str) -> std::result::Result<(u16, String), String> {
        let resp = self
            .http
            .post(format!("{DASHBOARD_URL}/{method}"))
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("Connect-Protocol-Version", "1")
            .body("{}")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let code = resp.status().as_u16();
        let body = if code == 200 {
            resp.text().await.map_err(|e| e.to_string())?
        } else {
            String::new()
        };
        Ok((code, body))
    }
}

#[async_trait]
impl Provider for CursorProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Cursor
    }

    async fn fetch(&self) -> Result<ProviderData> {
        if !self.config.enabled {
            return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
        }
        Ok(self.fetch_via_subscription().await)
    }
}

/// Keep the session after a change, in place of the one this fetch started
/// with — unless the user signed out or in again meanwhile. A failure is
/// logged and survived: the session in hand still works for this cycle.
fn keep(previous_token: &str, next: &OwnSession) {
    match session::replace(previous_token, next) {
        Ok(true) => {}
        Ok(false) => debug!("cursor: the session changed meanwhile; not overwritten"),
        Err(e) => warn!("cursor: could not keep the session: {e}"),
    }
}

/// The service refused the token.
fn is_rejected(code: u16) -> bool {
    matches!(code, 401 | 403)
}

// ─── The editor's token (fallback, read-only) ────────────────────

/// The editor's global state database:
/// %APPDATA%\Cursor\User\globalStorage\state.vscdb.
pub fn state_db_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_default()
        .join("Cursor")
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

/// What the editor's state database holds for the widget.
#[derive(Clone, PartialEq)]
pub enum EditorToken {
    /// No session token: nobody is signed in to the editor.
    Absent,
    /// The editor encrypted the token at rest. It is never decrypted.
    Encrypted,
    /// A plain token, readable as the JWT it is.
    Plain(String),
}

// Written by hand so that no debug print can ever carry the token.
impl std::fmt::Debug for EditorToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditorToken::Absent => f.write_str("Absent"),
            EditorToken::Encrypted => f.write_str("Encrypted"),
            EditorToken::Plain(_) => f.write_str("Plain(..)"),
        }
    }
}

/// Classify the stored value. An empty one is what signing out leaves in
/// some versions; a JSON-quoted one is how VS Code stores some keys.
pub fn classify_editor_value(value: Option<&str>) -> EditorToken {
    let Some(value) = value
        .map(|v| v.trim().trim_matches('"').trim())
        .filter(|v| !v.is_empty())
    else {
        return EditorToken::Absent;
    };
    if value.starts_with(ENCRYPTED_PREFIX) {
        EditorToken::Encrypted
    } else {
        EditorToken::Plain(value.to_string())
    }
}

/// Read the editor's session token, strictly read-only.
fn read_editor_token(path: &Path) -> std::result::Result<EditorToken, SqliteError> {
    let values = sqlite::lookup_texts(path, ITEM_QUERY, &[ACCESS_TOKEN_KEY])?;
    Ok(classify_editor_value(
        values.into_iter().next().flatten().as_deref(),
    ))
}

/// What the widget needs from a session token's claims. Holds no secret.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionClaims {
    /// The token's `sub`, whole: like "github|user_01…" (the sign-in
    /// provider, then the account id). It tells accounts apart.
    pub subject: String,
    /// `exp`, when the token states one.
    pub expires_at: Option<DateTime<Utc>>,
}

impl SessionClaims {
    /// Whether the token is past its `exp`. A token without one is left for
    /// the server to judge.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|exp| exp <= now)
    }
}

/// Decode the claims of a Cursor session token (a JWT; the signature is the
/// server's business, only the payload is read). None when the token is not
/// a JWT or carries no `sub`.
pub fn parse_session_claims(token: &str) -> Option<SessionClaims> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64url_decode(payload)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let sub = value.get("sub")?.as_str()?.trim();
    if sub.is_empty() {
        return None;
    }
    let expires_at = value
        .get("exp")
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
        .and_then(|secs| DateTime::<Utc>::from_timestamp(secs, 0));
    Some(SessionClaims {
        subject: sub.to_string(),
        expires_at,
    })
}

/// Unpadded base64url (the JWT segment alphabet); standard-alphabet input
/// and trailing padding are tolerated. None on any other character.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in input.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        // At most 13 live bits; the mask keeps the accumulator bounded.
        acc = ((acc << 6) | u32::from(v)) & 0x3fff;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Unpadded base64url (RFC 4648 §5), the alphabet of JWT segments and of
/// the PKCE verifier and challenge.
fn base64url_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        // Up to three bytes into the top of a 24-bit group, then one
        // character per six bits the chunk actually covers.
        let group = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, &b)| acc | (u32::from(b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            let sextet = (group >> (18 - 6 * i)) & 0x3f;
            out.push(char::from(ALPHABET[sextet as usize]));
        }
    }
    out
}

// ─── Answers ─────────────────────────────────────────────────────

/// The current period's usage, as GetCurrentPeriodUsage answers it.
#[derive(Debug, Clone, PartialEq)]
pub struct PeriodUsage {
    /// `planUsage.totalPercentUsed`, rounded and clamped to 0–100.
    pub used: u64,
    /// `billingCycleStart`.
    pub start: Option<DateTime<Utc>>,
    /// `billingCycleEnd`: the reset.
    pub end: Option<DateTime<Utc>>,
}

/// Why a usage answer brought no usage.
#[derive(Debug, Clone, PartialEq)]
pub enum UsageError {
    /// 401/403: the token is not accepted. Verified 2026-11-10 without one:
    /// 401 {"code": "unauthenticated", ..., "debug": {"error":
    /// "ERROR_NOT_LOGGED_IN", ...}}.
    Rejected,
    /// A 200 this version cannot read.
    Unrecognized,
    /// Any other status.
    Status(u16),
}

/// Map one usage answer. Free of I/O, so every branch is testable: any
/// non-200 or unknown body is an honest error, never a guessed number.
pub fn interpret_period_usage(
    code: u16,
    body: &str,
) -> std::result::Result<PeriodUsage, UsageError> {
    match code {
        200 => parse_period_usage(body).ok_or(UsageError::Unrecognized),
        401 | 403 => Err(UsageError::Rejected),
        other => Err(UsageError::Status(other)),
    }
}

/// The row a usage error leaves: a refused own session asks for a new
/// sign-in; a refused editor token is only rejected (the editor owns it).
/// Every error keeps the last data greyed.
pub fn usage_error_status(error: &UsageError, own_session: bool) -> (ProviderStatus, Vec<Metric>) {
    let status = match error {
        UsageError::Rejected if own_session => ProviderStatus::AuthError(SIGN_IN_AGAIN.to_string()),
        UsageError::Rejected => ProviderStatus::NetworkError("token rejected".to_string()),
        UsageError::Unrecognized => {
            ProviderStatus::NetworkError("unrecognized usage schema".to_string())
        }
        UsageError::Status(code) => {
            warn!("cursor usage returned HTTP {code}");
            ProviderStatus::NetworkError(format!("HTTP {code}"))
        }
    };
    (status, vec![])
}

/// Parse GetCurrentPeriodUsage. Schema verified 2026-11-10 (Premium plan):
/// {"billingCycleStart": "1794265288407", "billingCycleEnd": "1794870088407",
///  "planUsage": {"remainingBonus": false, "bonusTooltip": "...",
///    "autoPercentUsed": 1.19, "totalPercentUsed": 1.19},
///  "spendLimitUsage": {"limitType": "user"}, "displayThreshold": 200,
///  "enabled": true, "displayMessage": "You've used 1% of your included usage",
///  "autoModelSelectedDisplayMessage": "...",
///  "namedModelSelectedDisplayMessage": "...", "autoBucketModels": [...]}
///
/// The cycle bounds are millisecond epochs written as strings. The body is
/// protobuf JSON, which leaves zero values out, so a `planUsage` without
/// `totalPercentUsed` beside valid bounds is 0% — and only then. Without a
/// `planUsage` object it is an unknown schema (None).
pub fn parse_period_usage(body: &str) -> Option<PeriodUsage> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let plan = value.get("planUsage").filter(|p| p.is_object())?;
    let start = value.get("billingCycleStart").and_then(parse_instant);
    let end = value.get("billingCycleEnd").and_then(parse_instant);
    let bounds_valid = matches!((start, end), (Some(s), Some(e)) if e > s);
    let used = match plan.get("totalPercentUsed") {
        Some(v) => percent(number(v)?),
        // The encoder's omitted zero — trusted only beside a real cycle.
        None if bounds_valid => 0,
        None => return None,
    };
    Some(PeriodUsage { used, start, end })
}

/// The headline metric: the period's included usage, reset at the period's
/// end, labelled with the plan.
pub fn usage_metric(usage: &PeriodUsage, plan: Option<&PlanInfo>) -> Metric {
    Metric {
        label: usage_label(usage, plan),
        used: usage.used,
        limit: Some(100),
        unit: MetricUnit::Percent,
        reset_at: usage.end,
        // One included allowance per period, the same kind of window as
        // Copilot's monthly one.
        window: MetricWindow::Session,
        unified_pool: false,
    }
}

/// The plan's name ("Premium"); without one, the period the usage is
/// counted in — the plan's word for it, else the cycle's length; else
/// "Included".
fn usage_label(usage: &PeriodUsage, plan: Option<&PlanInfo>) -> String {
    if let Some(name) = plan.and_then(|p| p.name.as_deref()) {
        return name.to_string();
    }
    if let Some(period) = plan.and_then(|p| p.period) {
        return period.label().to_string();
    }
    let label = match (usage.start, usage.end) {
        (Some(start), Some(end)) => match (end - start).num_days() {
            6..=8 => "Weekly",
            27..=32 => "Monthly",
            _ => "Included",
        },
        _ => "Included",
    };
    label.to_string()
}

/// What GetPlanInfo says about the plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanInfo {
    /// `planName`, e.g. "Premium".
    pub name: Option<String>,
    /// `includedUsagePeriod`: what the included usage is counted per.
    pub period: Option<UsagePeriod>,
}

/// The period the included usage is counted per.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UsagePeriod {
    Daily,
    Weekly,
    Monthly,
}

impl UsagePeriod {
    /// "INCLUDED_USAGE_PERIOD_WEEKLY" → Weekly. None for a kind this
    /// version does not know (an unspecified one included).
    fn parse(kind: &str) -> Option<Self> {
        if kind.contains("WEEK") {
            Some(UsagePeriod::Weekly)
        } else if kind.contains("MONTH") {
            Some(UsagePeriod::Monthly)
        } else if kind.contains("DAY") || kind.contains("DAILY") {
            Some(UsagePeriod::Daily)
        } else {
            None
        }
    }

    fn label(self) -> &'static str {
        match self {
            UsagePeriod::Daily => "Daily",
            UsagePeriod::Weekly => "Weekly",
            UsagePeriod::Monthly => "Monthly",
        }
    }
}

/// Parse GetPlanInfo. Schema verified 2026-11-10:
/// {"planInfo": {"planName": "Premium", "price": "",
///   "billingCycleEnd": "1796857287995",
///   "includedUsagePeriod": "INCLUDED_USAGE_PERIOD_WEEKLY",
///   "includedUsageResetsAt": "1794870088407"}, ...}
/// None without a `planInfo` object.
pub fn parse_plan_info(body: &str) -> Option<PlanInfo> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let info = value.get("planInfo").filter(|p| p.is_object())?;
    let name = info
        .get("planName")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let period = info
        .get("includedUsagePeriod")
        .and_then(|v| v.as_str())
        .and_then(UsagePeriod::parse);
    Some(PlanInfo { name, period })
}

/// Grok Bot's usage, as GetSandUsageStatus answers it.
#[derive(Debug, Clone, PartialEq)]
pub struct SandUsage {
    /// `usagePercent`, rounded and clamped to 0–100.
    pub used: u64,
    /// `nextResetTimestampUtc`.
    pub reset_at: Option<DateTime<Utc>>,
    /// `isUnified`: Grok, Cursor and Grok Bot draw on one pool.
    pub unified: bool,
}

/// Parse GetSandUsageStatus. Schema verified 2026-11-10:
/// {"currentPeriodStart": "2026-11-09T23:01:28.407Z",
///  "nextResetTimestampUtc": "2026-11-16T23:01:28.407Z",
///  "usagePercent": 1.19, "hasAvailableUsage": true,
///  "grokPlanLabel": "Grok Bot Plan", "cursorPlanName": "Premium",
///  "isUnified": true, "billingBrand": "SAND_BILLING_BRAND_GROK", ...}
///
/// Protobuf JSON again: no `usagePercent` is 0% only beside a known period
/// and `hasAvailableUsage: true`. A body with every field left out is an
/// account without Grok Bot, and gives nothing.
pub fn parse_sand_usage(body: &str) -> Option<SandUsage> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if !value.is_object() {
        return None;
    }
    let reset_at = value.get("nextResetTimestampUtc").and_then(parse_instant);
    let period_known = reset_at.is_some()
        || value
            .get("currentPeriodStart")
            .and_then(parse_instant)
            .is_some();
    let available = value.get("hasAvailableUsage").and_then(|v| v.as_bool()) == Some(true);
    let used = match value.get("usagePercent") {
        Some(v) => percent(number(v)?),
        None if period_known && available => 0,
        None => return None,
    };
    Some(SandUsage {
        used,
        reset_at,
        unified: value.get("isUnified").and_then(|v| v.as_bool()) == Some(true),
    })
}

/// Grok Bot as an extra metric: a session window, so it never becomes the
/// headline, and no surface draws it yet. It carries whether its pool is
/// the headline's.
pub fn sand_metric(sand: &SandUsage) -> Metric {
    Metric {
        label: "Grok Bot".to_string(),
        used: sand.used,
        limit: Some(100),
        unit: MetricUnit::Percent,
        reset_at: sand.reset_at,
        window: MetricWindow::Session,
        unified_pool: sand.unified,
    }
}

/// RFC3339 text, or epoch seconds / milliseconds (as a number or digits).
fn parse_instant(v: &serde_json::Value) -> Option<DateTime<Utc>> {
    let from_epoch = |n: i64| {
        if n.abs() >= 100_000_000_000 {
            DateTime::<Utc>::from_timestamp_millis(n)
        } else {
            DateTime::<Utc>::from_timestamp(n, 0)
        }
    };
    let dt = match v {
        serde_json::Value::String(s) => DateTime::parse_from_rfc3339(s.trim())
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|| s.trim().parse::<i64>().ok().and_then(from_epoch)),
        serde_json::Value::Number(n) => n.as_i64().and_then(from_epoch),
        _ => None,
    };
    dt.filter(|dt| dt.timestamp() > 0)
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

// ─── Status (no network) ─────────────────────────────────────────

/// The editor's token as `ailimits-auth status` reports it.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorTokenState {
    /// No state database: Cursor is not installed for this user.
    NotInstalled,
    /// The database holds no session token.
    Absent,
    /// The editor keeps it encrypted; the widget does not read it.
    Encrypted,
    /// A plain token, with its expiry when stated.
    Plain(Option<DateTime<Utc>>),
    /// A plain token past its expiry.
    Expired(DateTime<Utc>),
    /// The database or the token could not be read.
    Unreadable(String),
}

/// The editor's token as the provider would find it right now.
pub fn editor_token_state() -> EditorTokenState {
    let path = state_db_path();
    if !path.exists() {
        return EditorTokenState::NotInstalled;
    }
    let token = match read_editor_token(&path) {
        Ok(EditorToken::Plain(token)) => token,
        Ok(EditorToken::Absent) => return EditorTokenState::Absent,
        Ok(EditorToken::Encrypted) => return EditorTokenState::Encrypted,
        Err(e) => return EditorTokenState::Unreadable(e.to_string()),
    };
    match parse_session_claims(&token) {
        Some(claims) => match claims.expires_at {
            Some(exp) if claims.is_expired(Utc::now()) => EditorTokenState::Expired(exp),
            exp => EditorTokenState::Plain(exp),
        },
        None => EditorTokenState::Unreadable("unrecognized session token".to_string()),
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use crate::platform::sqlite::test_support::{TempDb, TestDb};

    /// A fake token: a JWT-shaped string whose payload is
    /// {"sub":"github|user_01TESTFIXTURE","exp":4102444800,…} (year 2100).
    const FAKE_TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJnaXRodWJ8dXNlcl8wMVRFU1RGSVhUVVJFIiwiZXhwIjo0MTAyNDQ0ODAwLCJpc3MiOiJodHRwczovL2F1dGhlbnRpY2F0aW9uLmN1cnNvci5zaCIsImF1ZCI6Imh0dHBzOi8vY3Vyc29yLmNvbSJ9.c2lnbmF0dXJl";

    /// The editor's table, as VS Code creates it.
    const SCHEMA: &str =
        "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);";

    #[test]
    fn the_token_is_read_back_from_a_state_database() {
        let tmp = TempDb::new("cursor-token");
        {
            let db = TestDb::create(&tmp.0);
            db.exec("PRAGMA journal_mode=WAL;");
            db.exec(SCHEMA);
            db.exec(&format!(
                "INSERT INTO ItemTable VALUES ('{ACCESS_TOKEN_KEY}', '{FAKE_TOKEN}');"
            ));
            db.exec("INSERT INTO ItemTable VALUES ('cursorAuth/stripeMembershipType', 'free');");
        }
        assert_eq!(
            read_editor_token(&tmp.0).expect("readable"),
            EditorToken::Plain(FAKE_TOKEN.to_string())
        );
        let claims = parse_session_claims(FAKE_TOKEN).expect("claims");
        assert_eq!(claims.subject, "github|user_01TESTFIXTURE");
        assert!(!claims.is_expired(Utc::now()));
    }

    #[test]
    fn an_encrypted_token_is_left_alone() {
        let tmp = TempDb::new("cursor-encrypted");
        {
            let db = TestDb::create(&tmp.0);
            db.exec(SCHEMA);
            // The editor's at-rest form: the prefix, then base64 of "v10"
            // and the AES-GCM ciphertext (made-up bytes here).
            db.exec(&format!(
                "INSERT INTO ItemTable VALUES ('{ACCESS_TOKEN_KEY}', '{ENCRYPTED_PREFIX}v1:djEwAAAAAAAAAAAAAAAAAAAAAAAA');"
            ));
        }
        assert_eq!(
            read_editor_token(&tmp.0).expect("readable"),
            EditorToken::Encrypted
        );
    }

    #[test]
    fn a_signed_out_database_has_no_token() {
        let tmp = TempDb::new("cursor-signed-out");
        {
            let db = TestDb::create(&tmp.0);
            db.exec(SCHEMA);
            db.exec("INSERT INTO ItemTable VALUES ('cursorAuth/cachedEmail', 'x');");
            // Signing out leaves an empty value behind in some versions.
            db.exec(&format!(
                "INSERT INTO ItemTable VALUES ('{ACCESS_TOKEN_KEY}', '');"
            ));
        }
        assert_eq!(
            read_editor_token(&tmp.0).expect("readable"),
            EditorToken::Absent
        );
    }

    #[test]
    fn a_database_without_the_table_is_unreadable_not_signed_out() {
        let tmp = TempDb::new("cursor-no-table");
        {
            let db = TestDb::create(&tmp.0);
            db.exec("CREATE TABLE other (k TEXT);");
        }
        assert!(matches!(
            read_editor_token(&tmp.0),
            Err(SqliteError::Failed(_))
        ));
    }

    #[test]
    fn a_json_quoted_value_is_unwrapped() {
        let tmp = TempDb::new("cursor-quoted");
        {
            let db = TestDb::create(&tmp.0);
            db.exec(SCHEMA);
            db.exec(&format!(
                "INSERT INTO ItemTable VALUES ('{ACCESS_TOKEN_KEY}', '\"{FAKE_TOKEN}\"');"
            ));
        }
        assert_eq!(
            read_editor_token(&tmp.0).expect("readable"),
            EditorToken::Plain(FAKE_TOKEN.to_string())
        );
    }

    #[test]
    fn base64url_decodes_with_and_without_padding() {
        assert_eq!(base64url_decode("aGk").as_deref(), Some(&b"hi"[..]));
        assert_eq!(base64url_decode("aGk=").as_deref(), Some(&b"hi"[..]));
        assert_eq!(base64url_decode("_-8").as_deref(), Some(&[0xff, 0xef][..]));
        assert_eq!(base64url_decode("a b"), None);
    }

    #[test]
    fn base64url_encodes_without_padding_and_round_trips() {
        // RFC 4648 §10 vectors, in the url-safe alphabet and unpadded.
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg"),
            ("fo", "Zm8"),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg"),
            ("fooba", "Zm9vYmE"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64url_encode(plain.as_bytes()), encoded);
        }
        assert_eq!(base64url_encode(&[0xff, 0xef]), "_-8");
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(
            base64url_decode(&base64url_encode(&bytes)).as_deref(),
            Some(&bytes[..])
        );
    }

    #[test]
    fn usage_periods_name_the_window() {
        assert_eq!(
            UsagePeriod::parse("INCLUDED_USAGE_PERIOD_WEEKLY"),
            Some(UsagePeriod::Weekly)
        );
        assert_eq!(
            UsagePeriod::parse("INCLUDED_USAGE_PERIOD_MONTHLY"),
            Some(UsagePeriod::Monthly)
        );
        assert_eq!(
            UsagePeriod::parse("INCLUDED_USAGE_PERIOD_DAILY"),
            Some(UsagePeriod::Daily)
        );
        assert_eq!(
            UsagePeriod::parse("INCLUDED_USAGE_PERIOD_UNSPECIFIED"),
            None
        );
    }
}
