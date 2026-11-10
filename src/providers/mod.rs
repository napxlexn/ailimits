// providers/mod.rs — the Provider trait and shared data types.

pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod grok;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

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

/// Credential Manager key label per provider id (Codex has no key —
/// it is subscription-only and uses a usage token instead).
pub fn key_label_for(id: &str) -> Option<&'static str> {
    match id {
        "claude" => Some("claude_api_key"),
        "copilot" => Some("copilot_pat"),
        _ => None,
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
    pub reset_at: Option<DateTime<Utc>>,
    /// Which limit window this measures. Providers classify their own windows;
    /// never infer it from the label.
    #[serde(default)]
    pub window: MetricWindow,
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
    pub updated_at: DateTime<Utc>,
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
    /// The metric that represents this provider right now.
    ///
    /// A spent long window outranks the session gauge: once the weekly cap is
    /// gone no new session can start, so a session reading "20% used" would
    /// claim headroom the account does not have. Every surface — widget, tray,
    /// taskbar panel, tooltips, notifications — must agree, so the rule lives
    /// here rather than inside one renderer.
    pub fn headline_metric(&self) -> Option<&Metric> {
        let spent = self.metrics.iter().find(|m| {
            matches!(m.window, MetricWindow::Long)
                && m.percentage().is_some_and(|p| p >= EXHAUSTED_PCT)
        });
        // Otherwise the long-standing contract: the provider's first metric.
        spent.or_else(|| self.metrics.first())
    }

    /// Primary percentage for display: the headline metric's percentage.
    pub fn primary_percentage(&self) -> Option<f32> {
        self.headline_metric().and_then(|m| m.percentage())
    }

    /// Time of the nearest FUTURE metric reset. A reset timestamp already in
    /// the past (stale data whose window rolled over) is not "next": it would
    /// read as "resets in 0" in notifications and make the burn-rate forecast
    /// suppress itself against a moment that is already gone.
    pub fn next_reset(&self) -> Option<DateTime<Utc>> {
        let now = Utc::now();
        self.metrics
            .iter()
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
    pub fn headline_reset(&self) -> Option<DateTime<Utc>> {
        let now = Utc::now();
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
        let wall_age = Utc::now()
            .signed_duration_since(self.updated_at)
            .num_seconds()
            .max(0);
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
    pub fn aged_for_display(&self) -> ProviderData {
        if !matches!(self.status, ProviderStatus::Ok) {
            return self.clone();
        }

        // A reset only counts as provably-passed once it is past by more than
        // the grace window — so a small wall-clock skew can't manufacture an
        // ≈0% estimate. A reset in the (now-GRACE, now] band stays grey-stale.
        let cutoff = Utc::now() - chrono::Duration::seconds(RESET_GRACE_SECS);
        let mut any_reset_passed = false;
        let metrics: Vec<Metric> = self
            .metrics
            .iter()
            .map(|m| match m.reset_at {
                // Reset passed → the window rolled over; the next reset is unknown.
                Some(reset) if reset <= cutoff => {
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
