// providers/cursor.rs — Cursor provider (the Cursor subscription).
// See docs/en/PROVIDERS.md, section "Cursor".
//
// Token: only the widget's own session — signed in from the menu or with
// `ailimits-auth login cursor`, kept in Credential Manager (session.rs).
// Nothing of the Cursor editor is read for it: not its state database, not
// its token.
// Usage: the dashboard service the editor itself calls — the current
// period's usage on every cycle, the plan at most hourly, and Grok Bot's
// status only while its row ("cursor.bot") is chosen.

use super::{
    clean_label, read_text, status_words, transport_error, Metric, MetricUnit, MetricWindow, Net,
    Provider, ProviderData, ProviderId, ProviderStatus, MAX_ANSWER_BYTES,
};
use crate::config::schema::ProviderConfig;
use crate::utc::{self, Timestamp};
use anyhow::Result;
use async_trait::async_trait;
use session::{OwnSession, RefreshAnswer, StoreError};
use std::sync::{Arc, Mutex, PoisonError};
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
/// Grok Bot's usage — for its extra row only.
const SAND_USAGE_STATUS: &str = "GetSandUsageStatus";

/// How long a plan answer is trusted: plans rarely change.
const PLAN_TTL: Duration = Duration::from_secs(3600);
/// When to ask again after a plan request that brought nothing.
const PLAN_RETRY: Duration = Duration::from_secs(600);

/// How many fetches in a row the service must refuse the own session
/// before it counts as over. One refusal can be a glitch on the service's
/// side or a proxy's; a session the user really lost reads "sign in again"
/// a couple of cycles later.
const END_AFTER_REFUSALS: u32 = 3;

/// The row's message once the widget's own session is over.
const SIGN_IN_AGAIN: &str = "sign in to Cursor again";
/// The row's message while refusals are counted towards ending it.
const REFUSED_RETRYING: &str = "token rejected, retrying";

/// Where the widget's own session is kept: Credential Manager in the app,
/// memory in tests.
pub(crate) trait SessionStore: Send + Sync {
    fn load(&self) -> std::result::Result<Option<OwnSession>, StoreError>;
    /// Keep `next` in place of the session whose token was `previous`, but
    /// only while that one is still kept (see `session::replace`).
    fn replace(&self, previous: &str, next: &OwnSession) -> std::result::Result<bool, StoreError>;
}

/// Credential Manager, label `cursor_session`.
struct CredentialManager;

impl SessionStore for CredentialManager {
    fn load(&self) -> std::result::Result<Option<OwnSession>, StoreError> {
        session::load()
    }

    fn replace(&self, previous: &str, next: &OwnSession) -> std::result::Result<bool, StoreError> {
        session::replace(previous, next)
    }
}

pub struct CursorProvider {
    config: ProviderConfig,
    /// Never follows a 3xx — keeps the bearer token from leaking to a
    /// redirected host. The service answers 200 directly.
    net: Net,
    store: Arc<dyn SessionStore>,
    /// Whether Grok Bot's usage is asked for: only while its row is chosen.
    grok_bot: bool,
    /// The last plan answer, for the account it was asked for.
    plan: Mutex<Option<PlanCache>>,
    /// Refusals of the own session in a row: (a hash of its token, count).
    refusals: Mutex<Option<(u64, u32)>>,
    /// When this instance last tried a refresh, answered or not. The kept
    /// session notes the attempt too (`OwnSession::refreshed_at`), but only
    /// once it is saved: should Credential Manager not take it, this still
    /// holds the next attempt off for a day.
    last_refresh: Mutex<Option<Timestamp>>,
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

impl CursorProvider {
    /// The provider without Grok Bot: one usage request per cycle (and the
    /// plan at most hourly, the session's refresh near its expiry).
    pub fn new(config: ProviderConfig) -> Self {
        Self::with_parts(config, Net::new(), Arc::new(CredentialManager))
    }

    fn with_parts(config: ProviderConfig, net: Net, store: Arc<dyn SessionStore>) -> Self {
        Self {
            config,
            net,
            store,
            grok_bot: false,
            plan: Mutex::new(None),
            refusals: Mutex::new(None),
            last_refresh: Mutex::new(None),
        }
    }

    /// Ask for Grok Bot's usage too (`GetSandUsageStatus`, a second request
    /// each cycle) — for the "cursor.bot" row; pass whether that row is
    /// chosen on any surface. Off by default.
    pub fn with_grok_bot(mut self, on: bool) -> Self {
        self.grok_bot = on;
        self
    }

    fn data(&self, status: ProviderStatus, metrics: Vec<Metric>) -> ProviderData {
        ProviderData {
            id: self.id(),
            status,
            metrics,
            updated_at: utc::now(),
            received_at: Some(Instant::now()),
        }
    }

    /// An honest error row (the message fits the row: 32 characters at most).
    fn error(&self, msg: &str) -> ProviderData {
        self.data(ProviderStatus::NetworkError(msg.to_string()), vec![])
    }

    /// The own session is over: only a new sign-in mends it. The last data
    /// stays on screen, greyed, and the row says what to do — in its own
    /// words, not "key rejected": no key was given.
    fn sign_in_again(&self) -> ProviderData {
        self.error(SIGN_IN_AGAIN)
    }

    /// The own session, the only token there is.
    async fn fetch_via_subscription(&self) -> ProviderData {
        // A synchronous read, like the other providers' keyring reads.
        match self.store.load() {
            Ok(Some(own)) => self.fetch_with_own_session(own).await,
            // Not signed in (Settings, Cursor's Sign-in): hidden.
            Ok(None) => self.data(ProviderStatus::NotConfigured, vec![]),
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
        if self.refresh_due(&own, utc::now()) {
            (own, renewed) = self.refresh(own).await;
        }
        // An expired token is never sent; a refresh could not renew it.
        if own.ended || own.is_expired(utc::now()) {
            return self.sign_in_again();
        }

        let mut answer = self.call(CURRENT_PERIOD_USAGE, &own.access_token).await;
        if matches!(answer, Ok((code, _)) if is_rejected(code)) {
            // Refused. Near its expiry one refresh may still save the
            // session — at most one a day, like any refresh, so a run of
            // refusals costs no refresh each cycle.
            if !renewed && self.refresh_due(&own, utc::now()) {
                (own, renewed) = self.refresh(own).await;
                if renewed {
                    answer = self.call(CURRENT_PERIOD_USAGE, &own.access_token).await;
                }
            }
            if matches!(answer, Ok((code, _)) if is_rejected(code)) {
                // Over only once the service has refused it several fetches
                // in a row; until then the last data stays, greyed.
                if self.count_refusal(&own.access_token) < END_AFTER_REFUSALS {
                    return self.error(REFUSED_RETRYING);
                }
                let ended = OwnSession {
                    ended: true,
                    ..own.clone()
                };
                self.keep(&own.access_token, &ended);
                return self.sign_in_again();
            }
        }
        if answer.is_ok() {
            // Any answer but a refusal breaks the run of refusals.
            *self.refusals.lock().unwrap_or_else(PoisonError::into_inner) = None;
        }
        self.finish(&own.access_token, answer).await
    }

    /// One more refusal of the session holding `token`; the run's length.
    fn count_refusal(&self, token: &str) -> u32 {
        let hash = token_hash(token);
        let mut refusals = self.refusals.lock().unwrap_or_else(PoisonError::into_inner);
        let count = match *refusals {
            Some((h, n)) if h == hash => n + 1,
            _ => 1,
        };
        *refusals = Some((hash, count));
        count
    }

    /// Whether `own` is to be refreshed at `now`: near its expiry with no
    /// attempt noted in the kept session for a day (`needs_refresh`), and
    /// none made by this instance for a day either, kept or not.
    fn refresh_due(&self, own: &OwnSession, now: Timestamp) -> bool {
        own.needs_refresh(now)
            && session::attempt_due(
                *self
                    .last_refresh
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
                now,
            )
    }

    /// One refresh of the widget's own session — never of the editor's
    /// token. The result is kept; `true` when a new token came.
    async fn refresh(&self, own: OwnSession) -> (OwnSession, bool) {
        // Noted before the request: an attempt counts whatever comes of it,
        // and whether or not Credential Manager takes the result.
        *self
            .last_refresh
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(utc::now());
        let Some(answer) = session::request_refresh(&self.net, &own).await else {
            // No answer at all (offline, a timeout): nothing learnt, but the
            // attempt counts, so it waits a day like any other and a PC that
            // is offline does not try each cycle.
            let next = OwnSession {
                refreshed_at: Some(utc::now()),
                ..own.clone()
            };
            self.keep(&own.access_token, &next);
            return (next, false);
        };
        let next = own.after_refresh(&answer, utc::now());
        self.keep(&own.access_token, &next);
        (next, matches!(answer, RefreshAnswer::Renewed(_)))
    }

    /// Keep the session after a change, in place of the one this fetch
    /// started with — unless the user signed out or in again meanwhile. A
    /// failure is logged and survived: the session in hand still works for
    /// this cycle, and `last_refresh` keeps the refresh from repeating on
    /// the next.
    fn keep(&self, previous_token: &str, next: &OwnSession) {
        match self.store.replace(previous_token, next) {
            Ok(true) => {}
            Ok(false) => debug!("cursor: the session changed meanwhile; not overwritten"),
            Err(e) => warn!("cursor: could not keep the session: {e}"),
        }
    }

    /// Turn the usage answer into the row: the headline, labelled with the
    /// plan, then Grok Bot when its row is chosen and its status reads.
    async fn finish(
        &self,
        token: &str,
        answer: std::result::Result<(u16, String), &'static str>,
    ) -> ProviderData {
        let (code, body) = match answer {
            Ok(answer) => answer,
            Err(words) => return self.error(words),
        };
        let usage = match interpret_period_usage(code, &body) {
            Ok(usage) => usage,
            Err(e) => {
                let (status, _) = usage_error_status(&e);
                return self.data(status, vec![]);
            }
        };
        let plan = self.plan(token).await;
        let mut metrics = vec![usage_metric(&usage, plan.as_ref())];
        if self.grok_bot {
            if let Some(sand) = self.sand_usage(token).await {
                metrics.push(sand_metric(&sand));
            }
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
            Err(words) => {
                debug!("cursor plan info request failed: {words}");
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
            Err(words) => {
                debug!("cursor grok bot status request failed: {words}");
                None
            }
        }
    }

    /// One dashboard call: POST `{}` with the bearer token, as the editor
    /// sends it. The body is read only from a 200; Err is a request that got
    /// no answer, as the row words it.
    async fn call(
        &self,
        method: &str,
        token: &str,
    ) -> std::result::Result<(u16, String), &'static str> {
        let resp = self
            .net
            .http
            .post(self.net.url(&format!("{DASHBOARD_URL}/{method}")))
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("Connect-Protocol-Version", "1")
            .body("{}")
            .send()
            .await
            .map_err(|e| {
                warn!("cursor {method} request failed: {e}");
                transport_error(&e)
            })?;
        let code = resp.status().as_u16();
        let body = if code == 200 {
            read_text(resp, MAX_ANSWER_BYTES).await.map_err(|e| {
                warn!("cursor {method} body read failed: {e}");
                e.words()
            })?
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

/// The service refused the token.
fn is_rejected(code: u16) -> bool {
    matches!(code, 401 | 403)
}

/// A hash of a token, to tell sessions apart without keeping the token.
fn token_hash(token: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    token.hash(&mut h);
    h.finish()
}

// ─── Session tokens ──────────────────────────────────────────────

/// What the widget needs from a session token's claims. Holds no secret.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionClaims {
    /// The token's `sub`, whole: like "github|user_01…" (the sign-in
    /// provider, then the account id). It tells accounts apart.
    pub subject: String,
    /// `exp`, when the token states one.
    pub expires_at: Option<Timestamp>,
}

impl SessionClaims {
    /// Whether the token is past its `exp`. A token without one is left for
    /// the server to judge.
    pub fn is_expired(&self, now: Timestamp) -> bool {
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
        .and_then(utc::from_unix_secs);
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
    pub start: Option<Timestamp>,
    /// `billingCycleEnd`: the reset.
    pub end: Option<Timestamp>,
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

/// The row a usage error leaves. A refused session asks for a new sign-in
/// (the fetch first lets a few refusals pass, see `END_AFTER_REFUSALS`); a
/// rate limit or a server error says it retries; every error keeps the
/// last data greyed.
pub fn usage_error_status(error: &UsageError) -> (ProviderStatus, Vec<Metric>) {
    let status = match error {
        UsageError::Rejected => ProviderStatus::NetworkError(SIGN_IN_AGAIN.to_string()),
        UsageError::Unrecognized => {
            ProviderStatus::NetworkError("unrecognized usage schema".to_string())
        }
        UsageError::Status(code) => {
            warn!("cursor usage returned HTTP {code}");
            ProviderStatus::NetworkError(status_words(*code))
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
        extra_only: false,
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
        (Some(start), Some(end)) => match (end - start).as_secs() / 86_400 {
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
/// None without a `planInfo` object. The plan's name names the row, so it
/// is cleaned: no control characters, 32 characters at most.
pub fn parse_plan_info(body: &str) -> Option<PlanInfo> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let info = value.get("planInfo").filter(|p| p.is_object())?;
    let name = info
        .get("planName")
        .and_then(|v| v.as_str())
        .and_then(clean_label);
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
    pub reset_at: Option<Timestamp>,
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

/// Grok Bot as a metric only its extra row ("cursor.bot") shows: never the
/// headline, the Cursor row's reset or a reason to call that row an
/// estimate. It carries whether its pool is the headline's.
pub fn sand_metric(sand: &SandUsage) -> Metric {
    Metric {
        label: "Grok Bot".to_string(),
        used: sand.used,
        limit: Some(100),
        unit: MetricUnit::Percent,
        reset_at: sand.reset_at,
        window: MetricWindow::Session,
        unified_pool: sand.unified,
        extra_only: true,
    }
}

/// RFC3339 text, or epoch seconds / milliseconds (as a number or digits).
fn parse_instant(v: &serde_json::Value) -> Option<Timestamp> {
    let from_epoch = |n: i64| {
        if n.abs() >= 100_000_000_000 {
            utc::from_unix_millis(n)
        } else {
            utc::from_unix_secs(n)
        }
    };
    let dt = match v {
        serde_json::Value::String(s) => utc::parse_rfc3339(s.trim())
            .or_else(|| s.trim().parse::<i64>().ok().and_then(from_epoch)),
        serde_json::Value::Number(n) => n.as_i64().and_then(from_epoch),
        _ => None,
    };
    dt.filter(|dt| utc::unix_secs(*dt) > 0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::test_server::{Reply, TestServer};
    use crate::utc::SignedDuration;

    /// A fake token: a JWT-shaped string whose payload is
    /// {"sub":"github|user_01TESTFIXTURE","exp":4102444800,…} (year 2100).
    const FAKE_TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJnaXRodWJ8dXNlcl8wMVRFU1RGSVhUVVJFIiwiZXhwIjo0MTAyNDQ0ODAwLCJpc3MiOiJodHRwczovL2F1dGhlbnRpY2F0aW9uLmN1cnNvci5zaCIsImF1ZCI6Imh0dHBzOi8vY3Vyc29yLmNvbSJ9.c2lnbmF0dXJl";

    const USAGE_BODY: &str = r#"{"billingCycleStart":"1794265288407","billingCycleEnd":"4102444800000","planUsage":{"totalPercentUsed":37.0}}"#;
    const PLAN_BODY: &str = r#"{"planInfo":{"planName":"Premium","includedUsagePeriod":"INCLUDED_USAGE_PERIOD_WEEKLY"}}"#;
    const SAND_BODY: &str = r#"{"nextResetTimestampUtc":"2099-11-16T23:01:28.407Z","usagePercent":12,"hasAvailableUsage":true,"isUnified":true}"#;

    /// A session store in memory.
    #[derive(Default)]
    struct Memory {
        kept: Mutex<Option<OwnSession>>,
    }

    impl SessionStore for Memory {
        fn load(&self) -> std::result::Result<Option<OwnSession>, StoreError> {
            Ok(self.kept.lock().unwrap().clone())
        }

        fn replace(
            &self,
            previous: &str,
            next: &OwnSession,
        ) -> std::result::Result<bool, StoreError> {
            let mut kept = self.kept.lock().unwrap();
            match kept.as_ref() {
                Some(current) if current.access_token == previous => {
                    *kept = Some(next.clone());
                    Ok(true)
                }
                _ => Ok(false),
            }
        }
    }

    fn config() -> ProviderConfig {
        ProviderConfig {
            id: "cursor".to_string(),
            enabled: true,
            auth_method: Default::default(),
            credential_label: String::new(),
            tool_files: true,
            alert_threshold: 80,
        }
    }

    fn signed_in(
        server: &TestServer,
        session: Option<OwnSession>,
    ) -> (CursorProvider, Arc<Memory>) {
        let store = Arc::new(Memory::default());
        *store.kept.lock().unwrap() = session;
        let p = CursorProvider::with_parts(config(), Net::local(&server.base), store.clone());
        (p, store)
    }

    fn session() -> OwnSession {
        OwnSession {
            access_token: FAKE_TOKEN.to_string(),
            refresh_token: FAKE_TOKEN.to_string(),
            expires_at: parse_session_claims(FAKE_TOKEN).and_then(|c| c.expires_at),
            refreshed_at: None,
            ended: false,
        }
    }

    fn message(data: &ProviderData) -> String {
        match &data.status {
            ProviderStatus::NetworkError(m) | ProviderStatus::AuthError(m) => m.clone(),
            other => format!("{other:?}"),
        }
    }

    /// By default one usage request per cycle, the plan once: Grok Bot is
    /// never asked for while its row is not chosen.
    #[tokio::test]
    async fn grok_bot_is_asked_for_only_when_its_row_is_chosen() {
        let server = TestServer::start();
        server.route(CURRENT_PERIOD_USAGE, vec![Reply::ok(USAGE_BODY)]);
        server.route(PLAN_INFO, vec![Reply::ok(PLAN_BODY)]);
        server.route(SAND_USAGE_STATUS, vec![Reply::ok(SAND_BODY)]);
        let (p, _) = signed_in(&server, Some(session()));
        for _ in 0..3 {
            let data = p.fetch().await.unwrap();
            assert!(matches!(data.status, ProviderStatus::Ok));
            assert_eq!(data.metrics.len(), 1);
            assert_eq!(data.metrics[0].label, "Premium");
        }
        assert_eq!(server.count(CURRENT_PERIOD_USAGE), 3);
        assert_eq!(server.count(PLAN_INFO), 1, "the plan at most hourly");
        assert_eq!(server.count(SAND_USAGE_STATUS), 0);

        let server = TestServer::start();
        server.route(CURRENT_PERIOD_USAGE, vec![Reply::ok(USAGE_BODY)]);
        server.route(PLAN_INFO, vec![Reply::ok(PLAN_BODY)]);
        server.route(SAND_USAGE_STATUS, vec![Reply::ok(SAND_BODY)]);
        let (p, _) = signed_in(&server, Some(session()));
        let p = p.with_grok_bot(true);
        let data = p.fetch().await.unwrap();
        assert_eq!(data.metrics.len(), 2);
        let bot = &data.metrics[1];
        assert_eq!(bot.label, "Grok Bot");
        assert!(bot.extra_only, "for its own row only");
        assert_eq!(server.count(SAND_USAGE_STATUS), 1);
        // The Cursor row reads exactly as without it.
        assert_eq!(data.headline_metric().unwrap().label, "Premium");
    }

    /// One refusal does not end the session: only several in a row do,
    /// and any other answer breaks the run.
    #[tokio::test]
    async fn the_session_ends_only_after_refusals_in_a_row() {
        let server = TestServer::start();
        server.route(
            CURRENT_PERIOD_USAGE,
            vec![
                Reply::status(401),
                Reply::status(403),
                Reply::ok(USAGE_BODY),
                Reply::status(401),
                Reply::status(401),
                Reply::status(401),
            ],
        );
        server.route(PLAN_INFO, vec![Reply::ok(PLAN_BODY)]);
        let (p, store) = signed_in(&server, Some(session()));
        let words: Vec<String> = {
            let mut out = Vec::new();
            for _ in 0..6 {
                out.push(message(&p.fetch().await.unwrap()));
            }
            out
        };
        assert_eq!(
            words,
            [
                REFUSED_RETRYING,
                REFUSED_RETRYING,
                "Ok",
                REFUSED_RETRYING,
                REFUSED_RETRYING,
                SIGN_IN_AGAIN
            ]
        );
        assert!(store.kept.lock().unwrap().as_ref().unwrap().ended);
        // Ended: nothing is sent any more.
        let before = server.count(CURRENT_PERIOD_USAGE);
        assert_eq!(message(&p.fetch().await.unwrap()), SIGN_IN_AGAIN);
        assert_eq!(server.count(CURRENT_PERIOD_USAGE), before);
    }

    /// A refused refresh is not a sign-out: the session goes on with its
    /// token until it expires; the attempt is noted, so it waits a day.
    #[tokio::test]
    async fn a_refused_refresh_keeps_the_session() {
        let server = TestServer::start();
        server.route("/oauth/token", vec![Reply::status(401)]);
        server.route(CURRENT_PERIOD_USAGE, vec![Reply::ok(USAGE_BODY)]);
        server.route(PLAN_INFO, vec![Reply::ok(PLAN_BODY)]);
        // Six days left: inside the refresh window.
        let mut near = session();
        near.expires_at = Some(utc::now() + SignedDuration::from_hours(6 * 24));
        let (p, store) = signed_in(&server, Some(near));
        let data = p.fetch().await.unwrap();
        assert!(
            matches!(data.status, ProviderStatus::Ok),
            "{:?}",
            data.status
        );
        let kept = store.kept.lock().unwrap().clone().unwrap();
        assert!(!kept.ended);
        assert!(kept.refreshed_at.is_some());
        assert_eq!(server.count("/oauth/token"), 1);
        // The next cycle does not ask again.
        let _ = p.fetch().await.unwrap();
        assert_eq!(server.count("/oauth/token"), 1);
    }

    /// A refresh that gets no answer is an attempt too: the next cycle does
    /// not try again, so an offline PC sends nothing each cycle.
    #[tokio::test]
    async fn an_unanswered_refresh_waits_a_day() {
        let server = TestServer::start();
        server.route("/oauth/token", vec![Reply::Hangup]);
        server.route(CURRENT_PERIOD_USAGE, vec![Reply::ok(USAGE_BODY)]);
        server.route(PLAN_INFO, vec![Reply::ok(PLAN_BODY)]);
        let mut near = session();
        near.expires_at = Some(utc::now() + SignedDuration::from_hours(6 * 24));
        let (p, store) = signed_in(&server, Some(near));
        let _ = p.fetch().await.unwrap();
        let kept = store.kept.lock().unwrap().clone().unwrap();
        assert!(kept.refreshed_at.is_some());
        let tries = server.count("/oauth/token");
        let _ = p.fetch().await.unwrap();
        assert_eq!(
            server.count("/oauth/token"),
            tries,
            "no second try the same day"
        );
    }

    /// A session store that reads but never keeps anything: Credential
    /// Manager refusing every write.
    struct Unwritable(OwnSession);

    impl SessionStore for Unwritable {
        fn load(&self) -> std::result::Result<Option<OwnSession>, StoreError> {
            Ok(Some(self.0.clone()))
        }

        fn replace(&self, _: &str, _: &OwnSession) -> std::result::Result<bool, StoreError> {
            Err(StoreError::Unavailable("refused in the test".to_string()))
        }
    }

    /// A refresh whose result cannot be kept is still an attempt: each
    /// cycle reads the session as it was before, near its expiry and with
    /// no attempt noted, yet only one refresh goes out a day - answered,
    /// renewed, refused or unanswered.
    #[tokio::test]
    async fn a_refresh_that_cannot_be_kept_is_not_repeated_each_cycle() {
        let renewed = format!(r#"{{"access_token":"{FAKE_TOKEN}","shouldLogout":false}}"#);
        for (reply, usage) in [
            (Reply::ok(&renewed), Reply::ok(USAGE_BODY)),
            (Reply::status(500), Reply::ok(USAGE_BODY)),
            (Reply::status(401), Reply::ok(USAGE_BODY)),
            (Reply::Hangup, Reply::ok(USAGE_BODY)),
            // Refused usage near the expiry asks for a refresh too.
            (Reply::status(500), Reply::status(401)),
        ] {
            let server = TestServer::start();
            server.route("/oauth/token", vec![reply.clone()]);
            server.route(CURRENT_PERIOD_USAGE, vec![usage.clone()]);
            server.route(PLAN_INFO, vec![Reply::ok(PLAN_BODY)]);
            let mut near = session();
            near.expires_at = Some(utc::now() + SignedDuration::from_hours(6 * 24));
            let p = CursorProvider::with_parts(
                config(),
                Net::local(&server.base),
                Arc::new(Unwritable(near)),
            );
            for _ in 0..3 {
                let _ = p.fetch().await.unwrap();
            }
            assert_eq!(
                server.count("/oauth/token"),
                1,
                "{reply:?} / usage {usage:?}"
            );
            assert_eq!(server.count(CURRENT_PERIOD_USAGE), 3);
        }
    }

    /// Refused near its expiry: one refresh is tried, not one each cycle,
    /// and three refused fetches end the session.
    #[tokio::test]
    async fn a_run_of_refusals_near_expiry_costs_one_refresh() {
        let server = TestServer::start();
        server.route("/oauth/token", vec![Reply::status(500)]);
        server.route(CURRENT_PERIOD_USAGE, vec![Reply::status(401)]);
        let mut near = session();
        near.expires_at = Some(utc::now() + SignedDuration::from_hours(3 * 24));
        let (p, store) = signed_in(&server, Some(near));
        for _ in 0..3 {
            let _ = p.fetch().await.unwrap();
        }
        assert_eq!(server.count("/oauth/token"), 1);
        assert_eq!(server.count(CURRENT_PERIOD_USAGE), 3);
        assert!(store.kept.lock().unwrap().as_ref().unwrap().ended);
    }

    /// Without the app's own sign-in there is no token at all: nothing is
    /// read of the editor and nothing is sent.
    #[tokio::test]
    async fn without_its_own_sign_in_nothing_is_sent() {
        let server = TestServer::start();
        server.route(CURRENT_PERIOD_USAGE, vec![Reply::ok(USAGE_BODY)]);
        let (p, _) = signed_in(&server, None);
        let data = p.fetch().await.unwrap();
        assert!(matches!(data.status, ProviderStatus::NotConfigured));
        assert!(server.seen().is_empty());
    }

    #[tokio::test]
    async fn no_answer_and_server_errors_read_honestly() {
        for (reply, words) in [
            (Reply::Hangup, "offline, retrying"),
            (Reply::status(503), "HTTP 503, retrying"),
            (Reply::status(429), "rate-limited, retrying"),
        ] {
            let server = TestServer::start();
            server.route(CURRENT_PERIOD_USAGE, vec![reply]);
            let (p, _) = signed_in(&server, Some(session()));
            assert_eq!(message(&p.fetch().await.unwrap()), words);
        }
    }

    /// Nothing of the editor's sign-in is in the app: the key that names
    /// its token and the prefix of its encrypted form are gone from the
    /// code, and the status line reads nothing.
    #[test]
    fn the_editors_token_is_never_read() {
        let src = [include_str!("cursor.rs"), include_str!("cursor/session.rs")].concat();
        let key = ["cursorAuth", "/accessToken"].concat();
        let prefix = ["cursor-safe", "-storage:"].concat();
        let database = ["state", ".vscdb"].concat();
        assert!(!src.contains(&key));
        assert!(!src.contains(&prefix));
        assert!(!src.contains(&database));
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

    #[test]
    fn plan_names_are_cleaned() {
        let plan = parse_plan_info(r#"{"planInfo":{"planName":"Pro\u0007 Plus"}}"#).unwrap();
        assert_eq!(plan.name.as_deref(), Some("Pro Plus"));
        let long = format!(r#"{{"planInfo":{{"planName":"{}"}}}}"#, "x".repeat(90));
        assert_eq!(parse_plan_info(&long).unwrap().name.unwrap().len(), 32);
    }

    /// Without a plan the cycle's length names it, in whole days counted
    /// down: 6–8 a week, 27–32 a month.
    #[test]
    fn the_cycle_length_names_the_period_in_whole_days() {
        let label = |end: &str| {
            let usage = PeriodUsage {
                used: 1,
                start: crate::utc::parse_rfc3339("2026-11-01T00:00:00Z"),
                end: crate::utc::parse_rfc3339(end),
            };
            usage_label(&usage, None)
        };
        assert_eq!(label("2026-11-06T23:59:59.999999999Z"), "Included");
        assert_eq!(label("2026-11-07T00:00:00Z"), "Weekly");
        assert_eq!(label("2026-11-09T23:59:59.999999999Z"), "Weekly");
        assert_eq!(label("2026-11-10T00:00:00Z"), "Included");
        assert_eq!(label("2026-11-27T23:59:59Z"), "Included");
        assert_eq!(label("2026-11-28T00:00:00Z"), "Monthly");
        assert_eq!(label("2026-12-03T23:59:59.999Z"), "Monthly");
        assert_eq!(label("2026-12-04T00:00:00Z"), "Included");
        assert_eq!(label("2026-10-25T00:00:00Z"), "Included");
    }
}
