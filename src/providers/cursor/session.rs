// providers/cursor/session.rs — the widget's own Cursor session.
// See docs/en/PROVIDERS.md, section "Cursor".
//
// AI Limits signs in to Cursor by itself, the way the editor does: a login
// link the user approves in the browser (Cursor's own sign-in page, for the
// editor's OAuth client — Cursor offers no other), then a poll that hands
// the session over. PKCE binds the two: the link carries only the challenge,
// and only the holder of the verifier can collect the tokens. The session is
// kept in Credential Manager and belongs to the widget, so it is the one
// Cursor token the widget ever refreshes. Nothing of the editor's own
// session is read.

use super::{base64url_encode, parse_session_claims};
use crate::providers::{read_text, BodyError, Net, MAX_ANSWER_BYTES};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration as StdDuration;
use tracing::{debug, warn};

/// Credential Manager label (service `ailimits`) the session is kept under.
pub const SESSION_LABEL: &str = "cursor_session";
const KEYRING_SERVICE: &str = "ailimits";

/// The page the user approves a sign-in on — verified 2026-11-10.
const LOGIN_URL: &str = "https://cursor.com/loginDeepControl";
/// Answers 404 until the sign-in is approved, then 200 with the session —
/// verified 2026-11-10.
const POLL_URL: &str = "https://api2.cursor.sh/auth/poll";
/// Trades the refresh token for a new access token — verified 2026-11-10.
const REFRESH_URL: &str = "https://api2.cursor.sh/oauth/token";
/// The OAuth client a refresh is made for (the Cursor editor's own).
const CLIENT_ID: &str = "KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB";

/// How long a sign-in waits for the user to approve it in the browser.
pub const SIGN_IN_TIMEOUT: StdDuration = StdDuration::from_secs(300);
/// The pause before the first poll; each next pause is 1.2 times longer, up
/// to `POLL_MAX`.
const POLL_FIRST: StdDuration = StdDuration::from_secs(1);
const POLL_MAX: StdDuration = StdDuration::from_secs(8);

/// A session this close to its expiry is refreshed.
const REFRESH_WITHIN_DAYS: i64 = 7;
/// At most one refresh attempt per this many hours. A refresh that does not
/// move the expiry (one right after a sign-in did not) would otherwise
/// repeat on every update cycle.
const REFRESH_EVERY_HOURS: i64 = 24;

// ─── Sign-in by link ─────────────────────────────────────────────

/// One sign-in attempt. The verifier stays here; only the poll request
/// carries it, and the link the user opens carries its challenge instead.
pub struct LoginRequest {
    verifier: String,
    challenge: String,
    uuid: String,
}

// Written by hand so that no debug print can ever carry the verifier.
impl std::fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginRequest")
            .field("uuid", &self.uuid)
            .finish_non_exhaustive()
    }
}

impl LoginRequest {
    /// A fresh attempt: 32 random bytes for the verifier, a random v4 uuid.
    pub fn new() -> anyhow::Result<Self> {
        use ring::rand::{SecureRandom, SystemRandom};
        let rng = SystemRandom::new();
        let mut verifier = [0u8; 32];
        let mut uuid = [0u8; 16];
        rng.fill(&mut verifier)
            .and_then(|()| rng.fill(&mut uuid))
            .map_err(|_| anyhow::anyhow!("no system randomness"))?;
        Ok(Self::from_bytes(&verifier, uuid))
    }

    fn from_bytes(verifier: &[u8], uuid: [u8; 16]) -> Self {
        let verifier = base64url_encode(verifier);
        Self {
            challenge: pkce_challenge(&verifier),
            verifier,
            uuid: uuid_v4(uuid),
        }
    }

    /// The link the user opens to approve the sign-in.
    pub fn url(&self) -> String {
        format!(
            "{LOGIN_URL}?challenge={}&uuid={}&mode=login",
            self.challenge, self.uuid
        )
    }

    /// The poll for this attempt. It carries the verifier, so it is never
    /// logged.
    fn poll_url(&self) -> String {
        format!("{POLL_URL}?uuid={}&verifier={}", self.uuid, self.verifier)
    }
}

/// The PKCE S256 challenge: unpadded base64url of the SHA-256 of the
/// verifier's ASCII (RFC 7636).
pub fn pkce_challenge(verifier: &str) -> String {
    base64url_encode(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()).as_ref())
}

/// 16 random bytes as a version-4 UUID: the version and variant bits set,
/// lowercase hex in 8-4-4-4-12 groups.
fn uuid_v4(mut b: [u8; 16]) -> String {
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// The tokens an approved sign-in hands over.
pub struct Tokens {
    pub access: String,
    pub refresh: String,
}

// Written by hand so that no debug print can ever carry a token.
impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Tokens { .. }")
    }
}

/// What one poll answer means for the sign-in.
#[derive(Debug)]
pub enum PollAnswer {
    /// Not approved yet (404), or an answer that may pass: poll again.
    Pending,
    /// Approved: the session.
    Approved(Tokens),
    /// A 200 this version cannot read; polling on would not change it.
    Unrecognized,
}

/// Map one poll answer. Anything but a 200 is waited out, the deadline
/// bounds it: the poll answers 404 until the user approves.
pub fn interpret_poll(code: u16, body: &str) -> PollAnswer {
    if code != 200 {
        return PollAnswer::Pending;
    }
    match parse_tokens(body) {
        Some(tokens) => PollAnswer::Approved(tokens),
        None => PollAnswer::Unrecognized,
    }
}

/// The approved poll's body, verified 2026-11-10:
/// {"accessToken": "...", "refreshToken": "...", "authId": "...",
///  "challenge": "...", "uuid": "..."}; the two tokens were the same JWT.
/// A missing refresh token falls back to the access token, which is what
/// it was.
fn parse_tokens(body: &str) -> Option<Tokens> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let access = value
        .get("accessToken")?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let refresh = value
        .get("refreshToken")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(access);
    Some(Tokens {
        access: access.to_string(),
        refresh: refresh.to_string(),
    })
}

/// The pause before the next poll: 1.2 times the last one, at most
/// `POLL_MAX`.
fn next_poll_delay(prev: StdDuration) -> StdDuration {
    prev.mul_f64(1.2).min(POLL_MAX)
}

/// How a sign-in ended.
#[derive(Debug, Clone, PartialEq)]
pub enum SignInOutcome {
    /// Approved and stored; the session's expiry when the token states one.
    SignedIn(Option<DateTime<Utc>>),
    /// Nobody approved the link within `SIGN_IN_TIMEOUT`.
    TimedOut,
    /// Approved but unusable, or not stored. The text names what failed,
    /// never a token.
    Failed(String),
}

/// How waiting for the browser ended. An approved session is not kept yet:
/// whoever waited keeps it (`Approval::keep`). The app does so on its event
/// loop, and only while the sign-in is still the one it waits for, so a
/// Cancel or a sign-out made meanwhile is never undone by a session stored
/// after it.
#[derive(Debug)]
pub enum Approval {
    /// Approved: the session, to keep.
    Approved(Tokens),
    /// Nobody approved the link within `SIGN_IN_TIMEOUT`.
    TimedOut,
    /// Approved but unusable, or no client to ask with. The text names
    /// what failed, never a token.
    Failed(String),
}

impl Approval {
    /// Keep an approved session in Credential Manager: the sign-in's
    /// outcome.
    pub fn keep(self) -> SignInOutcome {
        self.keep_with(store)
    }

    /// `keep`, with `store` keeping the session.
    pub fn keep_with(
        self,
        store: impl FnOnce(&OwnSession) -> Result<(), StoreError>,
    ) -> SignInOutcome {
        match self {
            Approval::Approved(tokens) => {
                let session = OwnSession::from_tokens(tokens);
                match store(&session) {
                    Ok(()) => SignInOutcome::SignedIn(session.expires_at),
                    Err(e) => {
                        warn!("cursor session not stored: {e}");
                        SignInOutcome::Failed("could not store the session".to_string())
                    }
                }
            }
            Approval::TimedOut => SignInOutcome::TimedOut,
            Approval::Failed(why) => SignInOutcome::Failed(why),
        }
    }
}

/// Wait for the user to approve `request` in the browser, then keep the
/// session in Credential Manager: the whole sign-in, for `ailimits-auth`,
/// which nothing can cancel halfway.
pub async fn complete_sign_in(request: LoginRequest) -> SignInOutcome {
    wait_for_approval(request).await.keep()
}

/// Wait for the user to approve `request` in the browser. Polls with a
/// growing pause and gives up after `SIGN_IN_TIMEOUT`. Nothing is stored
/// here: the session comes back in the `Approval`.
pub async fn wait_for_approval(request: LoginRequest) -> Approval {
    let http = match http_client() {
        Ok(http) => http,
        Err(e) => return Approval::Failed(format!("no HTTP client: {e}")),
    };
    let deadline = tokio::time::Instant::now() + SIGN_IN_TIMEOUT;
    let mut delay = POLL_FIRST;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Approval::TimedOut;
        }
        tokio::time::sleep(delay.min(deadline - now)).await;
        match poll_once(&http, &request).await {
            PollAnswer::Pending => {}
            PollAnswer::Approved(tokens) => return Approval::Approved(tokens),
            PollAnswer::Unrecognized => {
                warn!("cursor sign-in answered an unknown schema");
                return Approval::Failed("unrecognized sign-in answer".to_string());
            }
        }
        delay = next_poll_delay(delay);
    }
}

async fn poll_once(http: &reqwest::Client, request: &LoginRequest) -> PollAnswer {
    let resp = match http.get(request.poll_url()).send().await {
        Ok(resp) => resp,
        Err(e) => {
            // The error would quote the URL, and the URL carries the verifier.
            debug!("cursor sign-in poll failed: {}", e.without_url());
            return PollAnswer::Pending;
        }
    };
    let code = resp.status().as_u16();
    let body = if code == 200 {
        match read_text(resp, MAX_ANSWER_BYTES).await {
            Ok(body) => body,
            // Cut off mid-transfer: the next poll asks again.
            Err(BodyError::Transport(_)) => return PollAnswer::Pending,
            // No session is that large: asking again would bring the same.
            Err(BodyError::TooLarge) => return PollAnswer::Unrecognized,
        }
    } else {
        String::new()
    };
    interpret_poll(code, &body)
}

/// The client for the sign-in. It never follows a redirect, so neither the
/// verifier nor a token can reach another host. The proxy set in Windows is
/// used once the app asked for it (`providers::follow_windows_proxy`).
pub fn http_client() -> reqwest::Result<reqwest::Client> {
    crate::providers::proxy::with_windows_proxy(reqwest::Client::builder())
        .timeout(StdDuration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

// ─── The kept session ────────────────────────────────────────────

/// The widget's own session, as kept in Credential Manager.
#[derive(Clone, PartialEq)]
pub struct OwnSession {
    pub access_token: String,
    /// What a refresh sends; so far always the access token itself.
    pub refresh_token: String,
    /// The access token's `exp`, when it states one.
    pub expires_at: Option<DateTime<Utc>>,
    /// The last refresh attempt that got an answer.
    pub refreshed_at: Option<DateTime<Utc>>,
    /// The server ended the session: a refresh answered `shouldLogout`, or
    /// the usage call refused it several fetches in a row. A new sign-in is
    /// needed. Kept instead of deleted, so the row can say so rather than
    /// disappear.
    pub ended: bool,
}

// Written by hand so that no debug print can ever carry a token.
impl std::fmt::Debug for OwnSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnSession")
            .field("expires_at", &self.expires_at)
            .field("refreshed_at", &self.refreshed_at)
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

/// The record's JSON: epoch seconds for the instants, the refresh token
/// only when it differs from the access token. Stored as UTF-8 bytes — the
/// credential blob holds 2560 bytes, which a UTF-16 password would halve.
#[derive(Serialize, Deserialize)]
struct Record {
    access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refreshed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    ended: bool,
}

impl OwnSession {
    /// A new session from a sign-in; its expiry is the access token's `exp`.
    pub fn from_tokens(tokens: Tokens) -> Self {
        Self {
            expires_at: expiry_of(&tokens.access),
            access_token: tokens.access,
            refresh_token: tokens.refresh,
            refreshed_at: None,
            ended: false,
        }
    }

    /// Past its expiry: never sent again.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|exp| exp <= now)
    }

    /// Within `REFRESH_WITHIN_DAYS` of its expiry, and not past it: an
    /// expired session is not renewed (the row asks to sign in again), so
    /// no request goes out for it day after day.
    pub fn near_expiry(&self, now: DateTime<Utc>) -> bool {
        self.expires_at
            .is_some_and(|exp| exp > now && exp - now <= Duration::days(REFRESH_WITHIN_DAYS))
    }

    /// Near its expiry, and no refresh attempt in the last
    /// `REFRESH_EVERY_HOURS`. A token without an expiry is left to the
    /// server to judge.
    pub fn needs_refresh(&self, now: DateTime<Utc>) -> bool {
        self.near_expiry(now) && attempt_due(self.refreshed_at, now)
    }

    /// The session after a refresh answer received at `now`. A new token
    /// replaces both tokens (the refresh token was the access token);
    /// a logout ends the session; anything else only notes the attempt.
    pub fn after_refresh(&self, answer: &RefreshAnswer, now: DateTime<Utc>) -> Self {
        match answer {
            RefreshAnswer::Renewed(token) => Self {
                access_token: token.clone(),
                refresh_token: token.clone(),
                expires_at: expiry_of(token),
                refreshed_at: Some(now),
                ended: false,
            },
            RefreshAnswer::LoggedOut => Self {
                ended: true,
                refreshed_at: Some(now),
                ..self.clone()
            },
            RefreshAnswer::Unrecognized | RefreshAnswer::Status(_) => Self {
                refreshed_at: Some(now),
                ..self.clone()
            },
        }
    }

    /// The record kept in Credential Manager.
    pub fn encode(&self) -> String {
        let record = Record {
            access_token: self.access_token.clone(),
            refresh_token: (self.refresh_token != self.access_token)
                .then(|| self.refresh_token.clone()),
            expires_at: self.expires_at.map(|t| t.timestamp()),
            refreshed_at: self.refreshed_at.map(|t| t.timestamp()),
            ended: self.ended,
        };
        // A struct of strings and numbers always serialises.
        serde_json::to_string(&record).unwrap_or_default()
    }

    /// Read a kept record back. None for anything that is not one.
    pub fn decode(text: &str) -> Option<Self> {
        let record: Record = serde_json::from_str(text).ok()?;
        let access = record.access_token.trim();
        if access.is_empty() {
            return None;
        }
        let instant = |secs: Option<i64>| secs.and_then(|s| DateTime::<Utc>::from_timestamp(s, 0));
        Some(Self {
            refresh_token: record
                .refresh_token
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty())
                .unwrap_or_else(|| access.to_string()),
            access_token: access.to_string(),
            expires_at: instant(record.expires_at),
            refreshed_at: instant(record.refreshed_at),
            ended: record.ended,
        })
    }
}

/// Whether a refresh may be tried at `now`, the last attempt having been
/// made at `last` (None: never): one attempt per `REFRESH_EVERY_HOURS`.
pub fn attempt_due(last: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    last.is_none_or(|t| now - t >= Duration::hours(REFRESH_EVERY_HOURS))
}

/// A token's `exp`, when it is a JWT that states one.
fn expiry_of(token: &str) -> Option<DateTime<Utc>> {
    parse_session_claims(token).and_then(|claims| claims.expires_at)
}

// ─── Credential Manager ──────────────────────────────────────────

/// Credential Manager could not be asked, or holds a record this version
/// cannot read. Neither ever quotes the record.
#[derive(Debug, Clone, PartialEq)]
pub enum StoreError {
    Unavailable(String),
    Corrupt,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Unavailable(e) => write!(f, "Credential Manager unavailable: {e}"),
            StoreError::Corrupt => f.write_str("the stored Cursor session is unreadable"),
        }
    }
}

impl std::error::Error for StoreError {}

/// The kept session; Ok(None) when there is none.
pub fn load() -> Result<Option<OwnSession>, StoreError> {
    load_at(SESSION_LABEL)
}

/// Keep `session`, replacing any earlier one: a sign-in. The app then
/// rebuilds the Cursor provider (app/provider_set.rs), so an answer still in
/// flight for the session from before is dropped.
pub fn store(session: &OwnSession) -> Result<(), StoreError> {
    store_at(SESSION_LABEL, session)
}

/// Keep `next` in place of the session whose access token was
/// `previous_token`, but only while that session is still the one kept:
/// a sign-out or a new sign-in made meanwhile wins over a fetch that began
/// with the old session. `Ok(false)` when it was not the one kept.
pub fn replace(previous_token: &str, next: &OwnSession) -> Result<bool, StoreError> {
    replace_at(SESSION_LABEL, previous_token, next)
}

/// Forget the session: a sign-out. Nothing kept is not an error. The app
/// then rebuilds the Cursor provider, so an answer still in flight with the
/// session is dropped and nothing of the account comes back on screen.
pub fn delete() -> Result<(), StoreError> {
    delete_at(SESSION_LABEL)
}

/// Whether a record is kept, readable or not — there is something to sign
/// out of.
pub fn is_stored() -> bool {
    matches!(load(), Ok(Some(_)) | Err(StoreError::Corrupt))
}

fn entry(label: &str) -> Result<keyring::Entry, StoreError> {
    keyring::Entry::new(KEYRING_SERVICE, label).map_err(|e| StoreError::Unavailable(e.to_string()))
}

fn load_at(label: &str) -> Result<Option<OwnSession>, StoreError> {
    match entry(label)?.get_secret() {
        Ok(bytes) => String::from_utf8(bytes)
            .ok()
            .and_then(|text| OwnSession::decode(&text))
            .map(Some)
            .ok_or(StoreError::Corrupt),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(StoreError::Unavailable(e.to_string())),
    }
}

fn store_at(label: &str, session: &OwnSession) -> Result<(), StoreError> {
    entry(label)?
        .set_secret(session.encode().as_bytes())
        .map_err(|e| StoreError::Unavailable(e.to_string()))
}

fn replace_at(label: &str, previous_token: &str, next: &OwnSession) -> Result<bool, StoreError> {
    match load_at(label)? {
        Some(current) if current.access_token == previous_token => {
            store_at(label, next).map(|()| true)
        }
        _ => Ok(false),
    }
}

fn delete_at(label: &str) -> Result<(), StoreError> {
    match entry(label)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(StoreError::Unavailable(e.to_string())),
    }
}

// ─── Refresh (the own session only) ──────────────────────────────

/// What a refresh answer means.
pub enum RefreshAnswer {
    /// A new access token.
    Renewed(String),
    /// The server ended the session.
    LoggedOut,
    /// A 200 this version cannot read.
    Unrecognized,
    /// Any other status.
    Status(u16),
}

// Written by hand so that no debug print can ever carry a token.
impl std::fmt::Debug for RefreshAnswer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefreshAnswer::Renewed(_) => f.write_str("Renewed(..)"),
            RefreshAnswer::LoggedOut => f.write_str("LoggedOut"),
            RefreshAnswer::Unrecognized => f.write_str("Unrecognized"),
            RefreshAnswer::Status(code) => write!(f, "Status({code})"),
        }
    }
}

/// Map one refresh answer. Verified 2026-11-10 right after a sign-in: 200
/// {"access_token": "...", "id_token": "...", "shouldLogout": false}, with
/// a new token. Only `shouldLogout: true` ends the session. A refused
/// refresh (401/403, never seen, its body not read) is a status like any
/// other: the session goes on with its token until that expires, and the
/// usage call decides whether the service still takes it — so a proxy's
/// or a glitch's 403 cannot sign the user out.
pub fn interpret_refresh(code: u16, body: &str) -> RefreshAnswer {
    match code {
        200 => {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
                return RefreshAnswer::Unrecognized;
            };
            if value.get("shouldLogout").and_then(|v| v.as_bool()) == Some(true) {
                return RefreshAnswer::LoggedOut;
            }
            match value
                .get("access_token")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                Some(token) => RefreshAnswer::Renewed(token.to_string()),
                None => RefreshAnswer::Unrecognized,
            }
        }
        other => RefreshAnswer::Status(other),
    }
}

/// One refresh request for the widget's own session — never for any other
/// app's token. None when no answer came at all (offline): nothing was
/// learnt, a later cycle may try again.
pub(crate) async fn request_refresh(net: &Net, session: &OwnSession) -> Option<RefreshAnswer> {
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "client_id": CLIENT_ID,
        "refresh_token": session.refresh_token,
    });
    let resp = match net.http.post(net.url(REFRESH_URL)).json(&body).send().await {
        Ok(resp) => resp,
        Err(e) => {
            debug!("cursor refresh failed: {e}");
            return None;
        }
    };
    let code = resp.status().as_u16();
    let text = if code == 200 {
        match read_text(resp, MAX_ANSWER_BYTES).await {
            Ok(text) => text,
            // Cut off, or past the cap: nothing learnt; the attempt counts.
            Err(e) => {
                debug!("cursor refresh answer not read: {e}");
                return None;
            }
        }
    } else {
        String::new()
    };
    let answer = interpret_refresh(code, &text);
    match &answer {
        RefreshAnswer::Unrecognized => warn!("cursor refresh answered an unknown schema"),
        RefreshAnswer::Status(code) => warn!("cursor refresh returned HTTP {code}"),
        _ => {}
    }
    Some(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// A fake JWT whose payload is
    /// {"sub":"github|user_01TESTFIXTURE","exp":4102444800,…} (year 2100).
    const FAKE_TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJnaXRodWJ8dXNlcl8wMVRFU1RGSVhUVVJFIiwiZXhwIjo0MTAyNDQ0ODAwLCJpc3MiOiJodHRwczovL2F1dGhlbnRpY2F0aW9uLmN1cnNvci5zaCIsImF1ZCI6Imh0dHBzOi8vY3Vyc29yLmNvbSJ9.c2lnbmF0dXJl";

    fn at(y: i32, mo: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, 12, 0, 0).unwrap()
    }

    fn session(expires_at: Option<DateTime<Utc>>) -> OwnSession {
        OwnSession {
            access_token: "a.b.c".to_string(),
            refresh_token: "a.b.c".to_string(),
            expires_at,
            refreshed_at: None,
            ended: false,
        }
    }

    #[test]
    fn the_pkce_pair_matches_the_rfc_7636_vector() {
        // RFC 7636, appendix B: the verifier's 32 octets, its base64url form
        // and its S256 challenge.
        let octets = [
            116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186, 22, 212,
            37, 77, 105, 214, 191, 240, 91, 88, 5, 88, 83, 132, 141, 121,
        ];
        let request = LoginRequest::from_bytes(&octets, [0; 16]);
        assert_eq!(
            request.verifier,
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        );
        assert_eq!(
            request.challenge,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn the_uuid_is_a_version_4_uuid() {
        let id = uuid_v4([0xff; 16]);
        assert_eq!(id, "ffffffff-ffff-4fff-bfff-ffffffffffff");
        let id = uuid_v4([0; 16]);
        assert_eq!(id, "00000000-0000-4000-8000-000000000000");
    }

    #[test]
    fn a_fresh_request_is_random_and_well_formed() {
        let a = LoginRequest::new().expect("randomness");
        let b = LoginRequest::new().expect("randomness");
        assert_ne!(a.verifier, b.verifier);
        assert_ne!(a.uuid, b.uuid);
        // 32 bytes → 43 unpadded base64url characters.
        assert_eq!(a.verifier.len(), 43);
        assert_eq!(a.challenge, pkce_challenge(&a.verifier));
        assert_eq!(a.uuid.len(), 36);
        assert_eq!(&a.uuid[14..15], "4");
    }

    #[test]
    fn the_link_carries_the_challenge_and_never_the_verifier() {
        let request = LoginRequest::new().expect("randomness");
        let url = request.url();
        assert!(url.starts_with("https://cursor.com/loginDeepControl?"));
        assert!(url.contains(&format!("challenge={}", request.challenge)));
        assert!(url.contains(&format!("uuid={}", request.uuid)));
        assert!(url.ends_with("&mode=login"));
        assert!(!url.contains(&request.verifier));

        let poll = request.poll_url();
        assert_eq!(
            poll,
            format!(
                "https://api2.cursor.sh/auth/poll?uuid={}&verifier={}",
                request.uuid, request.verifier
            )
        );
        assert!(!format!("{request:?}").contains(&request.verifier));
    }

    #[test]
    fn poll_answers_wait_until_the_session_arrives() {
        assert!(matches!(interpret_poll(404, ""), PollAnswer::Pending));
        // Anything else may pass too; the deadline bounds the wait.
        for code in [429, 500, 502] {
            assert!(matches!(interpret_poll(code, ""), PollAnswer::Pending));
        }

        let body = format!(
            r#"{{"accessToken":"{FAKE_TOKEN}","refreshToken":"{FAKE_TOKEN}","authId":"github|user_01TESTFIXTURE","challenge":"c","uuid":"u"}}"#
        );
        match interpret_poll(200, &body) {
            PollAnswer::Approved(tokens) => {
                assert_eq!(tokens.access, FAKE_TOKEN);
                assert_eq!(tokens.refresh, FAKE_TOKEN);
                assert!(!format!("{tokens:?}").contains(FAKE_TOKEN));
            }
            other => panic!("{other:?}"),
        }

        // No refresh token: the access token is what it would have been.
        match interpret_poll(200, r#"{"accessToken":"x.y.z"}"#) {
            PollAnswer::Approved(tokens) => assert_eq!(tokens.refresh, "x.y.z"),
            other => panic!("{other:?}"),
        }

        for body in ["", "<html>", r#"{"foo":1}"#, r#"{"accessToken":""}"#, "[]"] {
            assert!(
                matches!(interpret_poll(200, body), PollAnswer::Unrecognized),
                "{body}"
            );
        }
    }

    #[test]
    fn polling_backs_off_to_a_ceiling() {
        let mut delay = POLL_FIRST;
        let mut waited = StdDuration::ZERO;
        let mut polls = 0;
        while waited < SIGN_IN_TIMEOUT {
            waited += delay;
            polls += 1;
            let next = next_poll_delay(delay);
            assert!(next >= delay && next <= POLL_MAX);
            delay = next;
        }
        assert_eq!(delay, POLL_MAX);
        // About a poll every 8 s once settled: a few dozen in five minutes.
        assert!((30..60).contains(&polls), "{polls}");
        assert_eq!(
            next_poll_delay(StdDuration::from_secs(1)),
            StdDuration::from_millis(1200)
        );
    }

    #[test]
    fn a_session_takes_its_expiry_from_the_token() {
        let session = OwnSession::from_tokens(Tokens {
            access: FAKE_TOKEN.to_string(),
            refresh: FAKE_TOKEN.to_string(),
        });
        assert_eq!(
            session.expires_at,
            Some(Utc.with_ymd_and_hms(2100, 1, 1, 0, 0, 0).unwrap())
        );
        assert!(!session.ended);
        assert!(!format!("{session:?}").contains(FAKE_TOKEN));
    }

    #[test]
    fn refresh_waits_for_the_last_week_and_then_once_a_day() {
        let now = at(2026, 11, 10);
        // Far from its expiry: nothing to do.
        assert!(!session(Some(now + Duration::days(50))).needs_refresh(now));
        // Within a week of it; past it, never again.
        assert!(session(Some(now + Duration::days(6))).needs_refresh(now));
        assert!(!session(Some(now - Duration::days(1))).needs_refresh(now));
        assert!(!session(Some(now)).needs_refresh(now));
        // No expiry stated: the server judges it.
        assert!(!session(None).needs_refresh(now));

        let mut s = session(Some(now + Duration::days(3)));
        s.refreshed_at = Some(now - Duration::hours(2));
        assert!(!s.needs_refresh(now), "tried two hours ago");
        s.refreshed_at = Some(now - Duration::hours(25));
        assert!(s.needs_refresh(now), "a day has passed");

        assert!(session(Some(now)).is_expired(now));
        assert!(!session(Some(now + Duration::seconds(1))).is_expired(now));
        assert!(!session(None).is_expired(now));
    }

    #[test]
    fn refresh_answers_renew_end_or_note_the_session() {
        let now = at(2026, 11, 10);
        let old = session(Some(now + Duration::days(2)));

        // The verified shape: a new token, the session goes on.
        let body =
            format!(r#"{{"access_token":"{FAKE_TOKEN}","id_token":"x","shouldLogout":false}}"#);
        let answer = interpret_refresh(200, &body);
        assert!(matches!(&answer, RefreshAnswer::Renewed(t) if t == FAKE_TOKEN));
        assert!(!format!("{answer:?}").contains(FAKE_TOKEN));
        let renewed = old.after_refresh(&answer, now);
        assert_eq!(renewed.access_token, FAKE_TOKEN);
        assert_eq!(renewed.refresh_token, FAKE_TOKEN, "stored as both");
        assert_eq!(
            renewed.expires_at,
            Some(Utc.with_ymd_and_hms(2100, 1, 1, 0, 0, 0).unwrap())
        );
        assert_eq!(renewed.refreshed_at, Some(now));
        assert!(!renewed.ended);

        // shouldLogout: the session is over, whatever else the body says.
        let answer = interpret_refresh(200, r#"{"access_token":"x.y.z","shouldLogout":true}"#);
        assert!(matches!(answer, RefreshAnswer::LoggedOut));
        let ended = old.after_refresh(&answer, now);
        assert!(ended.ended);
        assert_eq!(ended.access_token, old.access_token);
        // A refused refresh is no sign-out: the session goes on, the try is
        // noted.
        for code in [401, 403] {
            let answer = interpret_refresh(code, "");
            assert!(matches!(answer, RefreshAnswer::Status(c) if c == code));
            let kept = old.after_refresh(&answer, now);
            assert!(!kept.ended);
            assert_eq!(kept.refreshed_at, Some(now));
        }

        // Unknown bodies and other statuses keep the session, noting the try.
        for body in [
            "",
            "nope",
            r#"{"shouldLogout":false}"#,
            r#"{"access_token":""}"#,
        ] {
            let answer = interpret_refresh(200, body);
            assert!(matches!(answer, RefreshAnswer::Unrecognized), "{body}");
            let kept = old.after_refresh(&answer, now);
            assert_eq!(kept.access_token, old.access_token);
            assert_eq!(kept.refreshed_at, Some(now));
            assert!(!kept.ended);
        }
        assert!(matches!(
            interpret_refresh(500, "{}"),
            RefreshAnswer::Status(500)
        ));
    }

    #[test]
    fn the_record_round_trips() {
        let now = at(2026, 11, 10);
        let mut s = session(Some(now + Duration::days(60)));
        s.refreshed_at = Some(now);
        let text = s.encode();
        assert!(
            !text.contains("refresh_token"),
            "the same token is stored once: {text}"
        );
        assert!(!text.contains("ended"));
        assert_eq!(OwnSession::decode(&text), Some(s.clone()));

        s.refresh_token = "r.e.f".to_string();
        s.ended = true;
        assert_eq!(OwnSession::decode(&s.encode()), Some(s));

        // A record from a later version with fields this one does not know.
        let later = r#"{"access_token":"a.b.c","expires_at":4102444800,"kind":"v2"}"#;
        let read = OwnSession::decode(later).expect("tolerated");
        assert_eq!(read.refresh_token, "a.b.c");

        for junk in ["", "{}", "[]", "a.b.c", r#"{"access_token":"  "}"#] {
            assert!(OwnSession::decode(junk).is_none(), "{junk}");
        }
    }

    /// A round trip through the real Credential Manager under a throwaway
    /// label, removed again even when an assertion fails.
    #[cfg(target_os = "windows")]
    #[test]
    fn the_session_round_trips_through_credential_manager() {
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = delete_at(&self.0);
            }
        }
        let label = format!(
            "cursor_session_test_{}_{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        let _cleanup = Cleanup(label.clone());

        assert_eq!(load_at(&label), Ok(None));
        let mut s = session(Some(at(2027, 1, 9)));
        // Room to spare: two different tokens of 900 characters each, more
        // than a UTF-16 password could hold.
        s.access_token = "x".repeat(900);
        s.refresh_token = "y".repeat(900);
        store_at(&label, &s).expect("stored");
        assert_eq!(load_at(&label), Ok(Some(s.clone())));

        // A fetch that began with this session may end it...
        let ended = OwnSession {
            ended: true,
            ..s.clone()
        };
        assert_eq!(replace_at(&label, &s.access_token, &ended), Ok(true));
        assert_eq!(load_at(&label), Ok(Some(ended.clone())));

        // ...but not once a new sign-in has replaced it.
        let newer = session(Some(at(2027, 2, 1)));
        store_at(&label, &newer).expect("signed in again");
        assert_eq!(replace_at(&label, &s.access_token, &ended), Ok(false));
        assert_eq!(load_at(&label), Ok(Some(newer.clone())));

        // Nor resurrect a session the user signed out of.
        delete_at(&label).expect("deleted");
        assert_eq!(replace_at(&label, &newer.access_token, &newer), Ok(false));
        assert_eq!(load_at(&label), Ok(None));

        store_at(&label, &s).expect("stored");
        delete_at(&label).expect("deleted");
        assert_eq!(load_at(&label), Ok(None));
        // Deleting what is not there is fine.
        delete_at(&label).expect("nothing to delete");
    }

    /// The record in Credential Manager holds epoch seconds; a record 0.7.0
    /// wrote reads and is written back to the byte.
    #[test]
    fn the_record_reads_and_is_written_as_before() {
        for record in [
            r#"{"access_token":"a.b.c","expires_at":4102444800,"refreshed_at":1794823200}"#,
            r#"{"access_token":"a.b.c","refresh_token":"r","expires_at":0,"refreshed_at":-1,"ended":true}"#,
            r#"{"access_token":"a.b.c","expires_at":-62135596800}"#,
            r#"{"access_token":"a.b.c"}"#,
        ] {
            assert_eq!(OwnSession::decode(record).unwrap().encode(), record);
        }
        let s = OwnSession::decode(r#"{"access_token":"t","expires_at":1794823200}"#).unwrap();
        assert_eq!(
            s.expires_at,
            crate::utc::parse_rfc3339("2026-11-16T10:00:00Z")
        );
        // A refresh noted mid-second is kept in whole seconds, rounded down.
        let noted = s.after_refresh(
            &RefreshAnswer::Unrecognized,
            crate::utc::parse_rfc3339("2026-11-16T10:00:00.999Z").unwrap(),
        );
        assert_eq!(
            noted.encode(),
            r#"{"access_token":"t","expires_at":1794823200,"refreshed_at":1794823200}"#
        );
        // An instant no calendar holds reads as none.
        let huge = OwnSession::decode(r#"{"access_token":"t","expires_at":9223372036854775807}"#);
        assert_eq!(huge.unwrap().expires_at, None);
    }

    /// Refreshed within 7 days of the expiry, to the nanosecond, never after
    /// it, and at most once in 24 hours.
    #[test]
    fn the_refresh_boundaries_hold_to_the_nanosecond() {
        let t = |text: &str| crate::utc::parse_rfc3339(text).unwrap();
        let now = t("2026-11-16T10:00:00Z");
        assert!(session(Some(t("2026-11-23T10:00:00Z"))).near_expiry(now));
        assert!(!session(Some(t("2026-11-23T10:00:00.000000001Z"))).near_expiry(now));
        assert!(session(Some(t("2026-11-16T10:00:00.000000001Z"))).near_expiry(now));
        assert!(!session(Some(now)).near_expiry(now));
        assert!(session(Some(now)).is_expired(now));
        assert!(!session(Some(t("2026-11-16T10:00:00.000000001Z"))).is_expired(now));
        assert!(attempt_due(None, now));
        assert!(attempt_due(Some(t("2026-11-15T10:00:00Z")), now));
        assert!(!attempt_due(Some(t("2026-11-15T10:00:00.000000001Z")), now));
        // A clock set back: the last attempt lies ahead, none is due.
        assert!(!attempt_due(Some(t("2026-11-17T10:00:00Z")), now));
    }
}
