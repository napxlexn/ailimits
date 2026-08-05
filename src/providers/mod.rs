// providers/mod.rs — the Provider trait and shared data types.

pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod copilot;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;

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
}

impl ProviderId {
    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderId::Claude => "Claude",
            ProviderId::Codex => "Codex",
            ProviderId::Copilot => "Copilot",
            ProviderId::Antigravity => "Antigravity",
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

/// Data older than this is considered stale (grey bar + age display).
pub const STALE_AFTER_SECS: i64 = 300;

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
    /// wall-clock jump (NTP, VM resume, DST) cannot falsely age fresh data;
    /// when `None` (a statusline snapshot or a disk-cache entry, which carry
    /// their own intrinsic `updated_at` age) staleness falls back to wall time.
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

    /// Data age in seconds, if stale. The staleness DECISION is monotonic for
    /// live data (immune to wall-clock jumps); data with no monotonic anchor
    /// (a statusline snapshot or a disk-cache entry, `received_at == None`)
    /// falls back to its intrinsic wall-clock age. The returned NUMBER is always
    /// the wall-clock age, for an honest "x ago" label.
    pub fn stale_age_secs(&self) -> Option<i64> {
        let wall_age = Utc::now()
            .signed_duration_since(self.updated_at)
            .num_seconds()
            .max(0);
        let is_stale = match self.received_at {
            Some(inst) => inst.elapsed().as_secs() as i64 > STALE_AFTER_SECS,
            None => wall_age > STALE_AFTER_SECS,
        };
        is_stale.then_some(wall_age)
    }

    /// Extrapolate stale data for display.
    ///
    /// When the source is down and a window's reset time has passed, that
    /// window has rolled over: show used=0 with the Estimated status ("≈0%").
    /// Windows that have not reset yet keep their last value — actual usage
    /// is at least the last known one.
    pub fn aged_for_display(&self) -> ProviderData {
        // Extrapolate only stale Ok data.
        if self.stale_age_secs().is_none() || !matches!(self.status, ProviderStatus::Ok) {
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
