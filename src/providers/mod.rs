// providers/mod.rs — the Provider trait and shared data types.

pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod grok;
pub(crate) mod proxy;
#[cfg(test)]
mod test_server;

pub use proxy::follow_windows_proxy;

use crate::utc::{self, SignedDuration, Timestamp};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};
use tracing::warn;

/// Provider identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Claude,
    Codex,
    Copilot,
    /// Google's Antigravity (the Gemini successor for individual accounts).
    /// The old cache/history files carry "gemini" — kept readable via the
    /// alias; new writes serialize as "antigravity".
    #[serde(alias = "gemini")]
    Antigravity,
    /// xAI's Grok (the SuperGrok subscription, read through the Grok CLI).
    Grok,
    /// The Cursor editor's subscription.
    Cursor,
}

impl ProviderId {
    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderId::Claude => "Claude",
            ProviderId::Codex => "Codex",
            ProviderId::Copilot => "Copilot",
            ProviderId::Antigravity => "Antigravity",
            ProviderId::Grok => "Grok",
            ProviderId::Cursor => "Cursor",
        }
    }

    /// The id this provider has in config.toml.
    pub fn config_id(&self) -> &'static str {
        match self {
            ProviderId::Claude => "claude",
            ProviderId::Codex => "codex",
            ProviderId::Copilot => "copilot",
            ProviderId::Antigravity => "antigravity",
            ProviderId::Grok => "grok",
            ProviderId::Cursor => "cursor",
        }
    }

    /// From a config id string.
    pub fn from_config_id(id: &str) -> Option<Self> {
        match id {
            "claude" => Some(ProviderId::Claude),
            "codex" => Some(ProviderId::Codex),
            "copilot" => Some(ProviderId::Copilot),
            // "gemini" is the legacy config id, migrated on load; accepted
            // here so nothing breaks mid-migration.
            "antigravity" | "gemini" => Some(ProviderId::Antigravity),
            "grok" => Some(ProviderId::Grok),
            "cursor" => Some(ProviderId::Cursor),
            _ => None,
        }
    }
}

// ─── Shared request helpers ──────────────────────────────────────

/// The HTTP client a provider sends with, and where its requests go: to the
/// provider's own hosts, or in tests to a local server standing in for them.
#[derive(Clone)]
pub(crate) struct Net {
    pub(crate) http: reqwest::Client,
    /// "http://127.0.0.1:port" in tests: every URL's scheme and host are
    /// swapped for it. None in the app.
    base: Option<String>,
}

impl Net {
    /// The app's client: a 10-second timeout and no redirects. These
    /// endpoints answer directly; refusing 3xx keeps a bearer token or an
    /// x-api-key header from ever being forwarded to another host.
    /// The proxy set in Windows is used once the app asked for it
    /// (`follow_windows_proxy`), unless an HTTP(S)_PROXY variable is set.
    pub(crate) fn new() -> Self {
        Self {
            http: proxy::with_windows_proxy(reqwest::Client::builder())
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("Failed to build HTTP client"),
            base: None,
        }
    }

    /// A client for a local test server: no proxy, a short timeout.
    #[cfg(test)]
    pub(crate) fn local(base: &str) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .build()
                .expect("test client"),
            base: Some(base.trim_end_matches('/').to_string()),
        }
    }

    /// `url` as this client sends it: unchanged in the app; in tests with
    /// its scheme and host replaced by the test server's.
    pub(crate) fn url(&self, url: &str) -> String {
        let Some(base) = &self.base else {
            return url.to_string();
        };
        let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
        let path = after_scheme
            .find('/')
            .map_or("/", |slash| &after_scheme[slash..]);
        format!("{base}{path}")
    }
}

/// The row's words for a request that got no HTTP answer: offline, DNS, a
/// timeout, a refused certificate, a body cut off. Never the raw error,
/// which quotes the URL. At most 32 characters, so the widget shows it as it
/// is, and ending in ", retrying", so the hover adds no action: the next
/// cycle simply tries again, and no tool needs to run.
pub(crate) fn transport_error(e: &reqwest::Error) -> &'static str {
    if e.is_timeout() {
        "timed out, retrying"
    } else if certificate_refused(e) {
        // A proxy that inspects TLS with its own root, say: the machine is
        // online, and calling it offline would send the user looking in the
        // wrong place.
        "untrusted certificate, retrying"
    } else {
        "offline, retrying"
    }
}

/// Whether an error, or anything behind it, is about the server's
/// certificate (rustls words every such failure with "certificate").
fn certificate_refused(e: &(dyn std::error::Error + 'static)) -> bool {
    let mut cause = Some(e);
    while let Some(err) = cause {
        if err.to_string().to_ascii_lowercase().contains("certificate") {
            return true;
        }
        cause = err.source();
    }
    false
}

/// The most a provider's answer is read to: each one is a few kilobytes of
/// JSON, so anything near this is not an answer at all.
pub(crate) const MAX_ANSWER_BYTES: usize = 1 << 20;

/// The row's words for an answer past its cap. It passes by itself as far as
/// the row can tell: the next cycle asks again.
pub(crate) const ANSWER_TOO_LARGE: &str = "answer too large, retrying";

/// Why an answer's body was not read.
#[derive(Debug)]
pub(crate) enum BodyError {
    /// The connection failed partway: a timeout, a body cut off.
    Transport(reqwest::Error),
    /// Longer than the cap: said so by its Content-Length, or found while
    /// reading it. Nothing past the cap is kept.
    TooLarge,
}

impl BodyError {
    /// The row's words for it: at most 32 characters, ending in
    /// ", retrying".
    pub(crate) fn words(&self) -> &'static str {
        match self {
            BodyError::Transport(e) => transport_error(e),
            BodyError::TooLarge => ANSWER_TOO_LARGE,
        }
    }
}

impl std::fmt::Display for BodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyError::Transport(e) => e.fmt(f),
            BodyError::TooLarge => f.write_str("the answer is larger than any it should be"),
        }
    }
}

impl std::error::Error for BodyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            // Transparent: the transport error's own causes, not itself again.
            BodyError::Transport(e) => e.source(),
            BodyError::TooLarge => None,
        }
    }
}

/// The body of `resp`, at most `cap` bytes: a Content-Length past it is
/// refused before anything is read, and a body without one is read in
/// chunks and given up on as soon as it passes the cap.
pub(crate) async fn read_body(
    mut resp: reqwest::Response,
    cap: usize,
) -> std::result::Result<Vec<u8>, BodyError> {
    let announced = resp.content_length();
    if announced.is_some_and(|n| n > cap as u64) {
        return Err(BodyError::TooLarge);
    }
    let mut body = Vec::with_capacity(announced.map_or(0, |n| n as usize));
    while let Some(chunk) = resp.chunk().await.map_err(BodyError::Transport)? {
        // `body` never holds more than `cap`, so this cannot underflow.
        if chunk.len() > cap - body.len() {
            return Err(BodyError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// `read_body` as text, as reqwest's `text()` reads it here: UTF-8, any
/// invalid sequence replaced.
pub(crate) async fn read_text(
    resp: reqwest::Response,
    cap: usize,
) -> std::result::Result<String, BodyError> {
    let body = read_body(resp, cap).await?;
    Ok(String::from_utf8(body)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
}

/// The row's words for an HTTP status that says nothing about the token: a
/// rate limit or a server error (5xx, Anthropic's 529 "overloaded" among
/// them) passes by itself and says so, so the hover adds no action that
/// would not help; anything else is named as it is.
pub(crate) fn status_words(code: u16) -> String {
    match code {
        429 => "rate-limited, retrying".to_string(),
        500..=599 => format!("HTTP {code}, retrying"),
        other => format!("HTTP {other}"),
    }
}

/// A name a server sent, fit for a row: control characters dropped, trimmed,
/// at most 32 characters. None when nothing is left.
pub(crate) fn clean_label(name: &str) -> Option<String> {
    let kept: String = name.chars().filter(|c| !c.is_control()).collect();
    let cut: String = kept.trim().chars().take(32).collect();
    let cut = cut.trim_end();
    (!cut.is_empty()).then(|| cut.to_string())
}

/// How long a stored token the server refused is not sent again.
pub const REFUSED_TOKEN_PAUSE: Duration = Duration::from_secs(30 * 60);

/// The last token a server refused, by a hash of it (never the token): it
/// is not sent again for `REFUSED_TOKEN_PAUSE`, so a stale stored token
/// costs no request each cycle. A different token, pasted anew, is tried at
/// once.
#[derive(Default)]
pub(crate) struct Refusals {
    last: Mutex<Option<(u64, Instant)>>,
}

impl Refusals {
    fn hash(token: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        token.hash(&mut h);
        h.finish()
    }

    /// Whether `token` was refused less than `REFUSED_TOKEN_PAUSE` before
    /// `now`.
    pub(crate) fn paused_at(&self, token: &str, now: Instant) -> bool {
        let last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        last.is_some_and(|(hash, at)| {
            hash == Self::hash(token) && now.saturating_duration_since(at) < REFUSED_TOKEN_PAUSE
        })
    }

    pub(crate) fn paused(&self, token: &str) -> bool {
        self.paused_at(token, Instant::now())
    }

    /// The server refused `token` at `now`.
    pub(crate) fn note_at(&self, token: &str, now: Instant) {
        *self.last.lock().unwrap_or_else(PoisonError::into_inner) = Some((Self::hash(token), now));
    }

    pub(crate) fn note(&self, token: &str) {
        self.note_at(token, Instant::now());
    }
}

// ─── Pasted usage tokens ─────────────────────────────────────────

/// The longest usage token taken. ChatGPT's access tokens are JWTs well over
/// a thousand characters; nothing a provider issues comes near this.
pub const MAX_USAGE_TOKEN_LEN: usize = 8192;

/// Why `token` cannot be a usage token for the provider `provider_id`,
/// judged before anything is sent; None when it may be one. Claude's are
/// OAuth tokens starting "sk-ant-"; ChatGPT's (Codex) are JWTs, three
/// dot-separated base64url parts. The words suit a toast or the CLI.
pub fn usage_token_problem(provider_id: &str, token: &str) -> Option<&'static str> {
    if token.is_empty() {
        return Some("nothing to store");
    }
    if token.len() > MAX_USAGE_TOKEN_LEN {
        return Some("too long for a usage token");
    }
    if token.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Some("a usage token has no spaces or line breaks");
    }
    let jwt = || {
        let parts: Vec<&str> = token.split('.').collect();
        parts.len() == 3
            && parts.iter().all(|p| {
                !p.is_empty()
                    && p.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'=')
            })
    };
    match provider_id {
        "claude" if !token.starts_with("sk-ant-") => {
            Some("not a Claude usage token: those start with sk-ant-")
        }
        "codex" if !jwt() => Some("not a ChatGPT usage token: those have three parts"),
        "claude" | "codex" => None,
        _ => Some("this provider takes no usage token"),
    }
}

/// What one check of a pasted usage token found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenCheck {
    /// The endpoint took it: usage (200), or its rate limit (429, a bucket
    /// shared with the tool's own polling, which only answers a token it
    /// knows).
    Accepted,
    /// The endpoint refused it (401/403).
    Refused,
    /// No answer that says either: why, in words for a toast or the CLI.
    Unchecked(String),
}

/// One read-only request to the usage endpoint of `provider_id` ("claude"
/// or "codex") with a pasted `token`, to check it before it is stored. The
/// token goes only to the host the provider sends it to anyway.
pub async fn check_usage_token(provider_id: &str, token: &str) -> TokenCheck {
    let net = Net::new();
    match provider_id {
        "claude" => claude::check_usage_token(&net, token).await,
        "codex" => codex::check_usage_token(&net, token).await,
        _ => TokenCheck::Unchecked("this provider takes no usage token".to_string()),
    }
}

/// A check's answer.
pub(crate) fn token_check(answer: reqwest::Result<reqwest::Response>) -> TokenCheck {
    match answer.map(|r| r.status().as_u16()) {
        Ok(200 | 429) => TokenCheck::Accepted,
        Ok(401 | 403) => TokenCheck::Refused,
        Ok(code) => TokenCheck::Unchecked(format!("the usage endpoint answered HTTP {code}")),
        Err(e) => {
            warn!("usage token check: no answer: {e}");
            TokenCheck::Unchecked("the usage endpoint did not answer".to_string())
        }
    }
}

/// A secret the app keeps in Credential Manager (service `ailimits`), read
/// fresh on every call — or, in tests, whatever the test hands in.
pub(crate) type SecretSource = Box<dyn Fn() -> Option<String> + Send + Sync>;

/// The app's own secret under `label`, trimmed; None when there is none.
pub(crate) fn keyring_secret(label: &'static str) -> SecretSource {
    Box::new(move || {
        crate::secrets::read_password(label)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

/// Credential Manager key label per provider id (Codex has no key —
/// it is subscription-only and uses a usage token instead).
pub fn key_label_for(id: &str) -> Option<&'static str> {
    match id {
        "claude" => Some("claude_api_key"),
        "copilot" => Some("copilot_pat"),
        _ => None,
    }
}

/// The label a provider's pasted key is read under: its own fixed label
/// (`key_label_for`), and only when the config names exactly that one. The
/// config's `credential_label` only says whether a key is set up (configs
/// from before 0.7.1 name the label this way); any other value in it -
/// another provider's key, a usage token's label - is ignored and reads
/// nothing, so no secret is ever sent to a host it was not pasted for.
pub(crate) fn own_key_label(provider: ProviderId, configured: &str) -> Option<&'static str> {
    key_label_for(provider.config_id()).filter(|own| configured == *own)
}

/// Reads the app's own secret under a label: Credential Manager in the app,
/// a stand-in in tests.
pub(crate) type SecretReader = fn(&'static str) -> Option<String>;

/// The password kept under `label` (service `ailimits`), as it is stored.
pub(crate) fn keyring_password(label: &'static str) -> Option<String> {
    crate::secrets::read_password(label).ok()
}

/// A provider's pasted key, read with `read` each time it is asked for, and
/// only under the provider's own label (`own_key_label`). Nothing at all
/// when the config names no key of the provider's own.
pub(crate) fn pasted_key(
    provider: ProviderId,
    configured: &str,
    read: SecretReader,
) -> SecretSource {
    match own_key_label(provider, configured) {
        Some(label) => Box::new(move || read(label)),
        None => Box::new(|| None),
    }
}

/// Credential Manager label of the manual usage token (tried before the
/// CLI-owned token files; see docs/en/PROVIDERS.md).
pub fn usage_token_label_for(id: &str) -> Option<&'static str> {
    match id {
        "claude" => Some("claude_usage_token"),
        "codex" => Some("codex_usage_token"),
        _ => None,
    }
}

/// Metric units.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricUnit {
    Requests,
    Tokens,
    Percent,
}

/// Which limit window a metric measures. Serialised into the provider cache,
/// so it must default: entries written before this field existed read back as
/// `Session`, which is what every pre-existing first metric was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MetricWindow {
    /// A short rolling window (the Claude / Codex 5-hour session).
    #[default]
    Session,
    /// A long window that gates the short one — a spent long limit blocks new
    /// sessions no matter what the session gauge reads.
    Long,
}

/// A single provider metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    /// Label, e.g. "Session", "Weekly".
    pub label: String,
    /// Used amount.
    pub used: u64,
    /// Limit, when known.
    pub limit: Option<u64>,
    pub unit: MetricUnit,
    /// Reset time, when known.
    #[serde(default, with = "crate::utc::as_text::option")]
    pub reset_at: Option<Timestamp>,
    /// Which limit window this measures. Providers classify their own windows;
    /// never infer it from the label.
    #[serde(default)]
    pub window: MetricWindow,
    /// This window and the provider's headline window are one pool, which
    /// another provider meters as well: Cursor's Grok Bot on a unified
    /// Grok/Cursor plan, where Cursor, Grok and Grok Bot draw on one weekly
    /// allowance. Carried so the display can decide how to show a shared
    /// pool; nothing reads it yet. Left out of the cache when false, so
    /// every other metric serialises as before.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unified_pool: bool,
    /// A part the provider meters on its own beside its main windows, shown
    /// only by an extra row the user chose (Claude's Fable week, Cursor's
    /// Grok Bot, a Grok product's share). Never the provider's headline, its
    /// reset, a part of its own row or a reason to call that row an
    /// estimate: a provider that starts to report one looks exactly as
    /// before until that row is chosen. Left out of the cache when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub extra_only: bool,
}

impl Metric {
    /// Usage percentage 0–100.
    pub fn percentage(&self) -> Option<f32> {
        self.limit.map(|lim| {
            if lim == 0 {
                0.0
            } else {
                (self.used as f32 / lim as f32 * 100.0).min(100.0)
            }
        })
    }

    /// Display text, e.g. "84 / 200".
    pub fn display_text(&self) -> String {
        match self.unit {
            MetricUnit::Tokens => {
                let used = format_tokens(self.used);
                match self.limit {
                    Some(lim) => format!("{} / {}", used, format_tokens(lim)),
                    None => used,
                }
            }
            MetricUnit::Requests => match self.limit {
                Some(lim) => format!("{} / {}", self.used, lim),
                None => self.used.to_string(),
            },
            MetricUnit::Percent => format!("{}%", self.used),
        }
    }
}

/// Short token format: 1200000 → "1.2M".
fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f32 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}k", n / 1_000)
    } else {
        n.to_string()
    }
}

/// Provider status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderStatus {
    /// Data received.
    Ok,
    /// Estimate: the source is down, but a window has reset since the last
    /// data — an extrapolated state, rendered with an "≈" marker.
    Estimated,
    Loading,
    AuthError(String),
    NetworkError(String),
    NotConfigured,
}

/// Data older than this is considered stale (grey bar + age display) — the
/// floor; a longer update interval raises it (see `stale_after_secs`).
pub const STALE_AFTER_SECS: i64 = 300;

/// Headroom over the update interval before data counts as stale. The
/// scheduler starts the next cycle only after every provider of this one has
/// answered, so a slow provider (request timeouts, a gh.exe spawn) pushes the
/// next refresh past the bare interval.
pub const STALE_SLACK_SECS: i64 = 60;

/// The update interval the scheduler sleeps on, as it last reported it (0
/// before its first cycle — the floor then applies).
static UPDATE_INTERVAL_SECS: AtomicU64 = AtomicU64::new(0);

/// Tell the staleness rule which update interval the scheduler runs on.
pub fn set_update_interval_secs(secs: u64) {
    UPDATE_INTERVAL_SECS.store(secs, Ordering::Relaxed);
}

/// How old data may get before it is stale, under a given update interval.
/// A value only stops updating once it has missed its next scheduled refresh:
/// on a 15- or 30-minute interval healthy data is minutes old between fetches
/// and must not grey out.
pub fn stale_after_secs(update_interval_secs: u64) -> i64 {
    let interval = i64::try_from(update_interval_secs).unwrap_or(i64::MAX);
    STALE_AFTER_SECS.max(interval.saturating_add(STALE_SLACK_SECS))
}

/// The scheduler's idle pause, as the staleness rule sees it. Polling stops
/// while the user is away (idle or locked), and the time spent there does not
/// age live data: a value only goes stale once it has missed refreshes the
/// scheduler actually tried to make. Without this every row greyed in the
/// seconds between the user coming back and the resume fetch landing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PollingPause {
    /// Since when polling is paused; `None` while it runs.
    pub paused_since: Option<Instant>,
    /// The most recent pause that has ended, as (start, end).
    pub last: Option<(Instant, Instant)>,
}

impl PollingPause {
    /// Record polling pausing (`true`) or resuming (`false`) at `now`. A
    /// repeat of the current state changes nothing.
    pub fn mark(&mut self, paused: bool, now: Instant) {
        match (paused, self.paused_since) {
            (true, None) => self.paused_since = Some(now),
            (false, Some(since)) => {
                self.last = Some((since, now));
                self.paused_since = None;
            }
            _ => {}
        }
    }

    /// Whether live data received at `received` is stale at `now`: older than
    /// `stale_after` seconds once the paused time since it arrived is taken
    /// out. Never stale while paused — nothing is being missed then, and the
    /// resume fetch is about to replace it.
    ///
    /// Only the most recent pause is remembered. Between two pauses the
    /// scheduler always runs a fetch cycle, so live data predates an older
    /// pause only when that fetch failed for it; counting the older pause as
    /// active time then greys it sooner, which is the honest direction.
    pub fn is_stale(&self, received: Instant, now: Instant, stale_after: i64) -> bool {
        if self.paused_since.is_some() {
            return false;
        }
        let paused = self.last.map_or(Duration::ZERO, |(start, end)| {
            end.min(now).saturating_duration_since(start.max(received))
        });
        let active = now
            .saturating_duration_since(received)
            .saturating_sub(paused);
        i64::try_from(active.as_secs()).unwrap_or(i64::MAX) > stale_after
    }
}

/// The scheduler's pause state, published from its loop.
static POLLING_PAUSE: Mutex<PollingPause> = Mutex::new(PollingPause {
    paused_since: None,
    last: None,
});

/// Tell the staleness rule that polling has paused (`true`) or resumed.
pub fn set_polling_paused(paused: bool) {
    let now = Instant::now();
    POLLING_PAUSE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .mark(paused, now);
}

fn polling_pause() -> PollingPause {
    *POLLING_PAUSE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A reset is only treated as "provably passed" (→ the `≈0%` estimate) once it
/// is more than this far in the past. Small relative to STALE_AFTER_SECS and to
/// real reset windows (5h / 7d), so it cannot mask a genuine rollover, but it
/// stops a small wall-clock skew from fabricating a reset that never happened.
pub const RESET_GRACE_SECS: i64 = 60;

/// A window at or above this percentage is spent: the account is blocked on it
/// regardless of what any other window reads.
pub const EXHAUSTED_PCT: f32 = 100.0;

/// Provider data for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderData {
    pub id: ProviderId,
    pub status: ProviderStatus,
    pub metrics: Vec<Metric>,
    /// Last update time (wall clock) — used for the displayed "x ago" age and
    /// the reset countdowns.
    #[serde(with = "crate::utc::as_text")]
    pub updated_at: Timestamp,
    /// Monotonic stamp of when this data was RECEIVED LIVE this session.
    /// SESSION-LOCAL: it survives in-process moves (the mpsc channel) but is
    /// intentionally NOT serialized (`#[serde(skip)]` ⇒ `None` after a disk
    /// round-trip). When `Some`, staleness is gated on this monotonic clock so a
    /// wall-clock jump (NTP, VM resume, DST) cannot falsely age fresh data,
    /// and the scheduler's idle pause is taken out of it (`PollingPause`);
    /// when `None` (a disk-cache entry, which carries its own intrinsic
    /// `updated_at` age) staleness falls back to wall time.
    #[serde(skip)]
    pub received_at: Option<Instant>,
}

impl ProviderData {
    /// The metric that represents this provider right now (never one only
    /// an extra row shows, `Metric::extra_only`).
    ///
    /// A spent long window outranks the session gauge: once the weekly cap is
    /// gone no new session can start, so a session reading "20% used" would
    /// claim headroom the account does not have. Every surface — widget, tray,
    /// taskbar panel, tooltips, notifications — must agree, so the rule lives
    /// here rather than inside one renderer.
    pub fn headline_metric(&self) -> Option<&Metric> {
        let mut own = self.metrics.iter().filter(|m| !m.extra_only);
        let spent = own.clone().find(|m| {
            matches!(m.window, MetricWindow::Long)
                && m.percentage().is_some_and(|p| p >= EXHAUSTED_PCT)
        });
        // Otherwise the long-standing contract: the provider's first metric.
        spent.or_else(|| own.next())
    }

    /// Primary percentage for display: the headline metric's percentage.
    pub fn primary_percentage(&self) -> Option<f32> {
        self.headline_metric().and_then(|m| m.percentage())
    }

    /// Time of the nearest FUTURE metric reset. A reset timestamp already in
    /// the past (stale data whose window rolled over) is not "next": it would
    /// read as "resets in 0" in notifications and make the burn-rate forecast
    /// suppress itself against a moment that is already gone.
    pub fn next_reset(&self) -> Option<Timestamp> {
        let now = utc::now();
        self.metrics
            .iter()
            .filter(|m| !m.extra_only)
            .filter_map(|m| m.reset_at)
            .filter(|t| *t > now)
            .min()
    }

    /// The reset that belongs to the headline metric — the one whose
    /// percentage every surface is showing. `next_reset()` answers the nearest
    /// reset across ALL windows, which is the wrong one to quote once a spent
    /// long window has taken over the headline: the session window resets far
    /// sooner and quoting it tells the user they are unblocked when they are
    /// not. Falls back to the nearest future reset when the headline has none.
    pub fn headline_reset(&self) -> Option<Timestamp> {
        let now = utc::now();
        self.headline_metric()
            .and_then(|m| m.reset_at)
            .filter(|t| *t > now)
            .or_else(|| self.next_reset())
    }

    /// Data age in seconds, if stale — measured against the update interval
    /// the app runs on (`stale_after_secs`).
    pub fn stale_age_secs(&self) -> Option<i64> {
        self.stale_age_secs_after(stale_after_secs(
            UPDATE_INTERVAL_SECS.load(Ordering::Relaxed),
        ))
    }

    /// Data age in seconds, if older than `stale_after` seconds. The
    /// staleness DECISION is monotonic for live data (immune to wall-clock
    /// jumps) and leaves out the time polling spent paused while the user was
    /// away (`PollingPause::is_stale`); data with no monotonic anchor (a
    /// disk-cache entry, `received_at == None`) falls back to its intrinsic
    /// wall-clock age. The returned NUMBER is always the wall-clock age, for
    /// an honest "x ago" label.
    pub fn stale_age_secs_after(&self, stale_after: i64) -> Option<i64> {
        let wall_age = (utc::now() - self.updated_at).as_secs().max(0);
        let is_stale = match self.received_at {
            Some(inst) => polling_pause().is_stale(inst, Instant::now(), stale_after),
            None => wall_age > stale_after,
        };
        is_stale.then_some(wall_age)
    }

    /// Whether this value is not live: stale, or an estimate. The widget
    /// greys such a row, the taskbar panel inks it dimmer and its hover
    /// explains why — all asking this one question.
    pub fn is_aged(&self) -> bool {
        matches!(self.status, ProviderStatus::Estimated) || self.stale_age_secs().is_some()
    }

    /// Extrapolate data for display.
    ///
    /// When a window's reset time has passed and no newer data has replaced
    /// it, that window has rolled over: show used=0 with the Estimated status
    /// ("≈0%"). This does not wait for the data to go stale - a reset that
    /// passed during the idle pause, which does not age data, is still a fact
    /// the row must show. Windows that have not reset yet keep their last
    /// value: actual usage is at least the last known one.
    ///
    /// Only the provider's own windows decide the status. A metric only an
    /// extra row shows (`Metric::extra_only`) is left as it came: its reset
    /// never makes the provider's row, the tray or the panel an estimate,
    /// and the extra row ages it on its own (rows.rs).
    pub fn aged_for_display(&self) -> ProviderData {
        if !matches!(self.status, ProviderStatus::Ok) {
            return self.clone();
        }

        // A reset only counts as provably-passed once it is past by more than
        // the grace window — so a small wall-clock skew can't manufacture an
        // ≈0% estimate. A reset in the (now-GRACE, now] band stays grey-stale.
        let cutoff = utc::earlier(utc::now(), SignedDuration::from_secs(RESET_GRACE_SECS));
        let mut any_reset_passed = false;
        let metrics: Vec<Metric> = self
            .metrics
            .iter()
            .map(|m| match m.reset_at {
                // Reset passed → the window rolled over; the next reset is unknown.
                Some(reset) if reset <= cutoff && !m.extra_only => {
                    any_reset_passed = true;
                    Metric {
                        used: 0,
                        reset_at: None,
                        ..m.clone()
                    }
                }
                _ => m.clone(),
            })
            .collect();

        ProviderData {
            id: self.id.clone(),
            status: if any_reset_passed {
                ProviderStatus::Estimated
            } else {
                ProviderStatus::Ok
            },
            metrics,
            updated_at: self.updated_at,
            // Preserve the monotonic gate across the transform.
            received_at: self.received_at,
        }
    }
}

/// The trait every provider implements.
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;

    async fn fetch(&self) -> Result<ProviderData>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_client_sends_every_url_to_the_local_server() {
        let net = Net::local("http://127.0.0.1:9/");
        assert_eq!(
            net.url("https://api.anthropic.com/api/oauth/usage"),
            "http://127.0.0.1:9/api/oauth/usage"
        );
        assert_eq!(
            net.url("https://cli-chat-proxy.grok.com/v1/billing?format=credits"),
            "http://127.0.0.1:9/v1/billing?format=credits"
        );
        assert_eq!(net.url("https://host.invalid"), "http://127.0.0.1:9/");
        // The app's client leaves every URL as it is.
        let real = "https://api.github.com/copilot_internal/user";
        assert_eq!(Net::new().url(real), real);
    }

    #[test]
    fn statuses_that_pass_say_so() {
        assert_eq!(status_words(429), "rate-limited, retrying");
        assert_eq!(status_words(500), "HTTP 500, retrying");
        assert_eq!(status_words(529), "HTTP 529, retrying");
        assert_eq!(status_words(404), "HTTP 404");
        assert_eq!(status_words(302), "HTTP 302");
        for code in [429, 500, 503, 529, 599] {
            assert!(status_words(code).chars().count() <= 32);
        }
    }

    #[test]
    fn server_names_are_cleaned_for_a_row() {
        assert_eq!(clean_label("  Premium ").as_deref(), Some("Premium"));
        assert_eq!(
            clean_label("Pro\u{0}\u{1b}Plus").as_deref(),
            Some("ProPlus")
        );
        assert_eq!(clean_label("a\nb").as_deref(), Some("ab"));
        assert_eq!(
            clean_label(&"é".repeat(40)).map(|s| s.chars().count()),
            Some(32)
        );
        assert_eq!(
            clean_label(&format!("{} x", "a".repeat(31))).as_deref(),
            Some(&*"a".repeat(31))
        );
        assert_eq!(clean_label(" \u{7}\t "), None);
        assert_eq!(clean_label(""), None);
    }

    #[test]
    fn a_refused_token_waits_half_an_hour_and_another_goes_at_once() {
        let refusals = Refusals::default();
        let t0 = Instant::now();
        assert!(!refusals.paused_at("a", t0));
        refusals.note_at("a", t0);
        assert!(refusals.paused_at("a", t0));
        assert!(refusals.paused_at("a", t0 + REFUSED_TOKEN_PAUSE - Duration::from_secs(1)));
        assert!(!refusals.paused_at("a", t0 + REFUSED_TOKEN_PAUSE));
        assert!(!refusals.paused_at("b", t0), "a token pasted anew is tried");
    }

    #[derive(Debug)]
    struct Chain(&'static str, Option<Box<Chain>>);

    impl std::fmt::Display for Chain {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    impl std::error::Error for Chain {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.1.as_deref().map(|e| e as _)
        }
    }

    #[test]
    fn a_refused_certificate_is_found_anywhere_in_the_chain() {
        let tls = Chain(
            "error sending request",
            Some(Box::new(Chain(
                "client error (Connect)",
                Some(Box::new(Chain(
                    "invalid peer certificate: UnknownIssuer",
                    None,
                ))),
            ))),
        );
        assert!(certificate_refused(&tls));
        let refused = Chain(
            "error sending request",
            Some(Box::new(Chain("connection refused", None))),
        );
        assert!(!certificate_refused(&refused));
    }

    /// A port nobody listens on: offline, in words the row can show.
    #[tokio::test]
    async fn a_refused_connection_reads_offline() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let net = Net::local(&format!("http://127.0.0.1:{port}"));
        let e = net
            .http
            .get(net.url("https://host.invalid/x"))
            .send()
            .await
            .expect_err("nobody listens there");
        assert_eq!(transport_error(&e), "offline, retrying");
    }

    /// An answer is read up to its cap and no further: a Content-Length past
    /// it is refused before the body is read, a body without one as soon as
    /// it passes the cap; anything within it is read whole.
    #[tokio::test]
    async fn an_answer_is_read_only_up_to_its_cap() {
        use crate::providers::test_server::{Reply, TestServer};
        let big = MAX_ANSWER_BYTES;
        for (reply, cap, want) in [
            (Reply::ok(&"x".repeat(64)), 64, Ok(64)),
            (Reply::Unsized("x".repeat(64)), 64, Ok(64)),
            (Reply::ok(""), 64, Ok(0)),
            (Reply::ok(&"x".repeat(65)), 64, Err(ANSWER_TOO_LARGE)),
            (Reply::Unsized("x".repeat(65)), 64, Err(ANSWER_TOO_LARGE)),
            // Many chunks: counted across all of them.
            (Reply::Unsized("x".repeat(big)), big, Ok(big)),
            (
                Reply::Unsized("x".repeat(big + 1)),
                big,
                Err(ANSWER_TOO_LARGE),
            ),
            (
                Reply::Unsized("x".repeat(4 * big)),
                big,
                Err(ANSWER_TOO_LARGE),
            ),
        ] {
            let server = TestServer::start();
            server.route("/x", vec![reply]);
            let net = Net::local(&server.base);
            let resp = net
                .http
                .get(net.url("https://host.invalid/x"))
                .send()
                .await
                .expect("an answer");
            let got = read_text(resp, cap)
                .await
                .map(|text| text.len())
                .map_err(|e| e.words());
            assert_eq!(got, want, "cap {cap}");
        }
        assert!(ANSWER_TOO_LARGE.chars().count() <= 32);
        assert!(ANSWER_TOO_LARGE.ends_with(", retrying"));
    }

    /// A pasted key is read only under the provider's own label: a config
    /// naming another provider's label, or a usage token's, reads nothing.
    #[test]
    fn a_key_is_read_only_under_the_providers_own_label() {
        assert_eq!(
            own_key_label(ProviderId::Claude, "claude_api_key"),
            Some("claude_api_key")
        );
        assert_eq!(
            own_key_label(ProviderId::Copilot, "copilot_pat"),
            Some("copilot_pat")
        );
        let others = [
            "",
            "copilot_pat",
            "claude_api_key",
            "claude_usage_token",
            "codex_usage_token",
            "cursor_session",
            "CLAUDE_API_KEY",
            " claude_api_key",
        ];
        for label in others {
            if label != "claude_api_key" {
                assert_eq!(own_key_label(ProviderId::Claude, label), None, "{label:?}");
            }
            if label != "copilot_pat" {
                assert_eq!(own_key_label(ProviderId::Copilot, label), None, "{label:?}");
            }
            // Providers without a key of their own read none at all.
            for id in [ProviderId::Codex, ProviderId::Grok, ProviderId::Cursor] {
                assert_eq!(own_key_label(id.clone(), label), None, "{id:?} {label:?}");
            }
        }
        fn every_label(label: &'static str) -> Option<String> {
            Some(format!("secret of {label}"))
        }
        assert_eq!(
            pasted_key(ProviderId::Copilot, "copilot_pat", every_label)(),
            Some("secret of copilot_pat".to_string())
        );
        assert_eq!(
            pasted_key(ProviderId::Copilot, "claude_api_key", every_label)(),
            None
        );
    }

    #[test]
    fn a_pasted_text_must_look_like_the_providers_usage_token() {
        let jwt = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ4In0.c2lnbmF0dXJl";
        assert_eq!(usage_token_problem("claude", "sk-ant-oat01-abcdef"), None);
        assert_eq!(usage_token_problem("codex", jwt), None);
        // A long ChatGPT token fits; the old 500-character cap did not.
        let long = format!(
            "{}.{}.{}",
            "a".repeat(900),
            "b".repeat(900),
            "c".repeat(300)
        );
        assert_eq!(usage_token_problem("codex", &long), None);

        for (provider, text) in [
            ("claude", ""),
            ("claude", "hunter2-my-password"),
            ("claude", "sk-ant-oat01 with spaces"),
            ("claude", "sk-ant-oat01-a\nb"),
            ("codex", "sk-ant-oat01-abcdef"),
            ("codex", "a.b"),
            ("codex", "a..c"),
            ("codex", "a.b!.c"),
            ("copilot", jwt),
        ] {
            assert!(
                usage_token_problem(provider, text).is_some(),
                "{provider}: {text:?}"
            );
        }
        let too_long = format!("sk-ant-{}", "x".repeat(MAX_USAGE_TOKEN_LEN));
        assert!(usage_token_problem("claude", &too_long).is_some());
    }

    /// A check takes 200 and 429, refuses 401/403, and says what else came.
    #[tokio::test]
    async fn a_usage_token_check_reads_the_status() {
        use crate::providers::test_server::{Reply, TestServer};
        for (reply, want) in [
            (Reply::ok("{}"), TokenCheck::Accepted),
            (Reply::status(429), TokenCheck::Accepted),
            (Reply::status(401), TokenCheck::Refused),
            (Reply::status(403), TokenCheck::Refused),
            (
                Reply::status(503),
                TokenCheck::Unchecked("the usage endpoint answered HTTP 503".to_string()),
            ),
            (
                Reply::Hangup,
                TokenCheck::Unchecked("the usage endpoint did not answer".to_string()),
            ),
        ] {
            let server = TestServer::start();
            server.route("/api/oauth/usage", vec![reply.clone()]);
            server.route("/backend-api/wham/usage", vec![reply]);
            let net = Net::local(&server.base);
            assert_eq!(claude::check_usage_token(&net, "sk-ant-x").await, want);
            assert_eq!(codex::check_usage_token(&net, "a.b.c").await, want);
            let auths: Vec<_> = server.seen().into_iter().filter_map(|s| s.auth).collect();
            assert_eq!(auths, ["Bearer sk-ant-x", "Bearer a.b.c"]);
        }
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use crate::config::schema::ProviderConfig;

    /// One live request per provider, the way the widget makes it, with the
    /// tools' sign-ins allowed. Prints the status and the parsed numbers only;
    /// no token leaves a provider.
    /// `cargo test --lib live_fetch_every_provider -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "live requests with this PC's sign-ins"]
    async fn live_fetch_every_provider() {
        let config = |id: &str| ProviderConfig {
            id: id.to_string(),
            enabled: true,
            auth_method: Default::default(),
            credential_label: String::new(),
            tool_files: true,
            alert_threshold: 80,
        };
        let providers: Vec<Box<dyn Provider>> = vec![
            Box::new(claude::ClaudeProvider::new(config("claude"))),
            Box::new(codex::CodexProvider::new(config("codex"))),
            Box::new(antigravity::AntigravityProvider::new(config("antigravity"))),
            Box::new(copilot::CopilotProvider::new(config("copilot"))),
            Box::new(grok::GrokProvider::new(config("grok"))),
            Box::new(cursor::CursorProvider::new(config("cursor"))),
        ];
        println!("LIVE at {}", utc::to_rfc3339(utc::now()));
        for p in providers {
            match p.fetch().await {
                Ok(data) => {
                    println!("LIVE {:?}: {:?}", p.id(), data.status);
                    for m in &data.metrics {
                        println!(
                            "LIVE   {}: {}/{:?} {:?} ({:?}), resets {:?}",
                            m.label, m.used, m.limit, m.unit, m.window, m.reset_at
                        );
                    }
                }
                Err(e) => println!("LIVE {:?}: error {e}", p.id()),
            }
        }
    }
}
