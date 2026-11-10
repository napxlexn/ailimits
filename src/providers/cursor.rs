// providers/cursor.rs — Cursor provider (the Cursor editor's subscription).
// See docs/en/PROVIDERS.md, section "Cursor".
//
// Mechanism: read the session token the editor keeps in its VS Code state
// database (READ-ONLY, never refreshed — the editor refreshes it itself), then
// one request to the usage summary the cursor.com dashboard shows.

use super::{Metric, MetricUnit, MetricWindow, Provider, ProviderData, ProviderId, ProviderStatus};
use crate::config::schema::ProviderConfig;
use crate::platform::sqlite::{self, SqliteError};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use tracing::warn;

/// Undocumented dashboard endpoint — response schema verified 2026-11-10, HTTP 200.
const USAGE_SUMMARY_URL: &str = "https://cursor.com/api/usage-summary";

/// The editor's own session token in its state database.
const ACCESS_TOKEN_KEY: &str = "cursorAuth/accessToken";
/// VS Code keeps global state in one key/value table.
const ITEM_QUERY: &str = "SELECT value FROM ItemTable WHERE key = ?1";

pub struct CursorProvider {
    config: ProviderConfig,
    http: reqwest::Client,
}

impl CursorProvider {
    pub fn new(config: ProviderConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            // Never follow a 3xx — keeps the session cookie from leaking to a
            // redirected host. The endpoint returns 200 directly.
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

    /// An honest error row (the message fits the row: 32 characters at most).
    fn error(&self, msg: &str) -> ProviderData {
        self.data(ProviderStatus::NetworkError(msg.to_string()), vec![])
    }

    /// Cursor subscription usage. The editor's token is read-only: Cursor
    /// refreshes it while it runs, the widget must never rotate it.
    async fn fetch_via_subscription(&self) -> Result<ProviderData> {
        let path = state_db_path();
        if !path.exists() {
            // Cursor is not installed (or has never run) for this user.
            return Ok(self.data(ProviderStatus::NotConfigured, vec![]));
        }

        // A synchronous read, like the keyring reads of the other providers:
        // one indexed lookup, about a millisecond, bounded by the busy timeout.
        let token = match read_access_token(&path) {
            Ok(Some(token)) => token,
            // Installed, but nobody is signed in.
            Ok(None) => return Ok(self.data(ProviderStatus::NotConfigured, vec![])),
            // The editor is writing right now — transient, the next cycle retries.
            Err(SqliteError::Busy) => return Ok(self.error("state database busy, retrying")),
            Err(e @ SqliteError::Unavailable(_)) => {
                warn!("cursor state database: {e}");
                return Ok(self.error("winsqlite3.dll unavailable"));
            }
            Err(e) => {
                warn!("cursor state database: {e}");
                return Ok(self.error("state database unreadable"));
            }
        };

        let Some(claims) = parse_session_claims(&token) else {
            return Ok(self.error("unrecognized session token"));
        };
        // An expired token is never sent, and never refreshed here: opening
        // Cursor refreshes it, the last data stays on screen greyed until then.
        if claims.is_expired(Utc::now()) {
            return Ok(self.error("token expired"));
        }

        let Ok(mut cookie) =
            reqwest::header::HeaderValue::from_str(&session_cookie(&claims.subject, &token))
        else {
            return Ok(self.error("unrecognized session token"));
        };
        // Keeps the value out of any Debug output of the request.
        cookie.set_sensitive(true);

        let resp = match self
            .http
            .get(USAGE_SUMMARY_URL)
            .header(reqwest::header::COOKIE, cookie)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, "ailimits-widget")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return Ok(self.data(ProviderStatus::NetworkError(e.to_string()), vec![])),
        };

        let status = resp.status().as_u16();
        let body = if status == 200 {
            match resp.text().await {
                Ok(body) => body,
                Err(e) => return Ok(self.data(ProviderStatus::NetworkError(e.to_string()), vec![])),
            }
        } else {
            String::new()
        };
        let (status, metrics) = interpret_usage_summary(status, &body);
        Ok(self.data(status, metrics))
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
        self.fetch_via_subscription().await
    }
}

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

/// Read the session token from the state database, strictly read-only.
/// Ok(None) when no one is signed in (no row, or an empty value).
fn read_access_token(path: &Path) -> std::result::Result<Option<String>, SqliteError> {
    let values = sqlite::lookup_texts(path, ITEM_QUERY, &[ACCESS_TOKEN_KEY])?;
    Ok(values
        .into_iter()
        .next()
        .flatten()
        // Tolerate a JSON-quoted value: VS Code stores some keys that way.
        .map(|v| v.trim().trim_matches('"').to_string())
        .filter(|v| !v.is_empty()))
}

/// What the widget needs from the session token's claims. Holds no secret.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionClaims {
    /// The token's `sub`, whole: the session cookie carries it in front of
    /// the token.
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

/// Decode the claims of the editor's session token (a JWT; the signature is
/// the server's business, only the payload is read). None when the token is
/// not a JWT or carries no `sub`.
///
/// `sub` looks like "github|user_01…" (the sign-in provider, then the
/// account id); the cookie carries it whole, the form verified live.
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

/// The cursor.com session cookie: `<sub>%3A%3A<token>` (a literal, already
/// URL-encoded "::"), with the token's `sub` whole - verified 2026-11-10,
/// HTTP 200 with a freshly issued token.
fn session_cookie(subject: &str, token: &str) -> String {
    format!("WorkosCursorSessionToken={subject}%3A%3A{token}")
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

/// Map one usage-summary answer to what the widget shows. Pure, so every
/// branch is testable without a network.
///
/// 200 with a known shape → Ok; 200 with anything else, or any non-200, is an
/// honest error — never a guessed number.
pub fn interpret_usage_summary(status: u16, body: &str) -> (ProviderStatus, Vec<Metric>) {
    match status {
        200 => match parse_usage_summary(body) {
            Ok(metrics) if !metrics.is_empty() => (ProviderStatus::Ok, metrics),
            // Malformed JSON or an unknown schema.
            _ => (
                ProviderStatus::NetworkError("unrecognized usage schema".to_string()),
                vec![],
            ),
        },
        // The server no longer accepts the editor's session.
        401 | 403 => (
            ProviderStatus::NetworkError("token rejected".to_string()),
            vec![],
        ),
        other => (
            ProviderStatus::NetworkError(format!("HTTP {other}")),
            vec![],
        ),
    }
}

/// Parse the usage-summary response into metrics.
///
/// Schema verified by a manual request on 2026-11-10 (free plan):
/// {"billingCycleStart": "RFC3339", "billingCycleEnd": "RFC3339",
///  "membershipType": "free", "limitType": "user", "isUnlimited": false,
///  "individualUsage": {"plan": {"enabled": true, "used": 0, "limit": 0,
///    "remaining": 0, "breakdown": {...}, "autoPercentUsed": 0,
///    "apiPercentUsed": 0, "totalPercentUsed": 0},
///   "onDemand": {"enabled": false, "used": 0, "limit": null, ...}},
///  "teamUsage": {}}
/// One monthly window: totalPercentUsed (0–100), reset at billingCycleEnd,
/// labelled with the plan. The auto/API split and on-demand spend have no
/// place in the metric model yet and are not shown. The parser is tolerant.
pub fn parse_usage_summary(body: &str) -> Result<Vec<Metric>> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    if !value.is_object() {
        return Ok(vec![]);
    }

    // An unlimited plan has nothing to chart: an honest "Unlimited" row, not a
    // 0% bar. No reset either — the hint line would quote "0%" against it.
    if value.get("isUnlimited").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(vec![Metric {
            label: "Unlimited".to_string(),
            used: 0,
            limit: None,
            unit: MetricUnit::Percent,
            reset_at: None,
            window: MetricWindow::Session,
            unified_pool: false,
        }]);
    }

    let plan = match value.get("individualUsage").and_then(|u| u.get("plan")) {
        Some(p) if p.is_object() => p,
        _ => return Ok(vec![]),
    };
    // A disabled individual plan (e.g. limits kept at the team level, a shape
    // not seen yet) says nothing about this account's usage.
    if plan.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
        return Ok(vec![]);
    }
    // The server's own percentage; failing that, used/limit when both are
    // there (amounts in cents).
    let pct = plan
        .get("totalPercentUsed")
        .and_then(|v| v.as_f64())
        .or_else(|| {
            let used = plan.get("used").and_then(|v| v.as_f64())?;
            let limit = plan.get("limit").and_then(|v| v.as_f64())?;
            (limit > 0.0).then(|| used / limit * 100.0)
        })
        .filter(|p| p.is_finite());
    let Some(pct) = pct else {
        return Ok(vec![]);
    };

    let reset_at = value.get("billingCycleEnd").and_then(parse_instant);
    let label = value
        .get("membershipType")
        .and_then(|v| v.as_str())
        .map(plan_label)
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "Monthly".to_string());

    Ok(vec![Metric {
        label,
        used: pct.round().clamp(0.0, 100.0) as u64,
        limit: Some(100),
        unit: MetricUnit::Percent,
        reset_at,
        // One monthly allowance, the same kind of window as Copilot's.
        window: MetricWindow::Session,
        unified_pool: false,
    }])
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

/// "free" → "Free", "pro_plus" → "Pro Plus".
fn plan_label(membership: &str) -> String {
    membership
        .split(['_', '-', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What `ailimits-auth status` reports about the editor's session.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    /// No state database: Cursor is not installed for this user.
    NotInstalled,
    /// The database holds no session token.
    SignedOut,
    /// A token the widget can use, with its expiry when stated.
    Valid(Option<DateTime<Utc>>),
    /// A token past its expiry — opening Cursor refreshes it.
    Expired(DateTime<Utc>),
    /// The database or the token could not be read.
    Unreadable(String),
}

/// The session as the provider would find it right now — no network.
pub fn session_state() -> SessionState {
    let path = state_db_path();
    if !path.exists() {
        return SessionState::NotInstalled;
    }
    let token = match read_access_token(&path) {
        Ok(Some(token)) => token,
        Ok(None) => return SessionState::SignedOut,
        Err(e) => return SessionState::Unreadable(e.to_string()),
    };
    match parse_session_claims(&token) {
        Some(claims) => match claims.expires_at {
            Some(exp) if claims.is_expired(Utc::now()) => SessionState::Expired(exp),
            exp => SessionState::Valid(exp),
        },
        None => SessionState::Unreadable("unrecognized session token".to_string()),
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
        let token = read_access_token(&tmp.0)
            .expect("readable")
            .expect("a token");
        assert_eq!(token, FAKE_TOKEN);

        let claims = parse_session_claims(&token).expect("claims");
        assert_eq!(claims.subject, "github|user_01TESTFIXTURE");
        assert!(!claims.is_expired(Utc::now()));
        assert_eq!(
            session_cookie(&claims.subject, &token),
            format!("WorkosCursorSessionToken=github|user_01TESTFIXTURE%3A%3A{FAKE_TOKEN}")
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
        assert_eq!(read_access_token(&tmp.0).expect("readable"), None);
    }

    #[test]
    fn a_database_without_the_table_is_unreadable_not_signed_out() {
        let tmp = TempDb::new("cursor-no-table");
        {
            let db = TestDb::create(&tmp.0);
            db.exec("CREATE TABLE other (k TEXT);");
        }
        assert!(matches!(
            read_access_token(&tmp.0),
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
            read_access_token(&tmp.0).expect("readable").as_deref(),
            Some(FAKE_TOKEN)
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
    fn plan_labels_are_title_cased() {
        assert_eq!(plan_label("free"), "Free");
        assert_eq!(plan_label("pro_plus"), "Pro Plus");
        assert_eq!(plan_label("ultra"), "Ultra");
    }
}
