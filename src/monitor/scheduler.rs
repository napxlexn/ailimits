// monitor/scheduler.rs — background update scheduler for providers.
//
// The provider list is SHARED (Arc<RwLock>): the event loop rebuilds it
// live when auth settings change in the context menu.

use crate::{
    app::AppCommand,
    platform,
    providers::{Provider, ProviderStatus},
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Minimum update interval, seconds.
const MIN_INTERVAL_SECS: u64 = 60;
/// Pause polling once the user has been idle (or the workstation locked) for this long.
const IDLE_PAUSE_SECS: u64 = 300;
/// While paused, re-check this often whether the user is back.
const PAUSE_RECHECK_SECS: u64 = 20;
/// Upper bound for the error backoff: if nothing succeeds, never wait longer than this.
const MAX_BACKOFF_SECS: u64 = 300;

/// Shared provider list.
pub type SharedProviders = Arc<RwLock<Vec<Arc<dyn Provider>>>>;

/// Shared update interval — changed live from the context menu.
pub type SharedInterval = Arc<AtomicU64>;

pub struct Scheduler {
    providers: SharedProviders,
    /// Interval in seconds — shared, read before every cycle.
    interval_secs: SharedInterval,
    cmd_tx: mpsc::Sender<AppCommand>,
}

impl Scheduler {
    pub fn new(
        providers: SharedProviders,
        interval_secs: SharedInterval,
        cmd_tx: mpsc::Sender<AppCommand>,
    ) -> Self {
        Self {
            providers,
            interval_secs,
            cmd_tx,
        }
    }

    /// Run the scheduler (spawned as a background task).
    ///
    /// Adaptive, but conservative — it only ever polls LESS than the user's
    /// interval, never more (matching the project rule for undocumented
    /// endpoints: "when unsure, poll less"):
    ///   - Pause entirely when the user is idle / the workstation is locked,
    ///     resuming immediately on activity.
    ///   - Back off (up to MAX_BACKOFF_SECS) when a whole cycle yields no
    ///     success, resetting to the baseline the moment anything succeeds.
    pub async fn run(self) {
        info!(
            "Scheduler started, interval {}s",
            self.interval_secs.load(Ordering::Relaxed)
        );

        // The current error backoff in seconds (>= the baseline interval).
        let mut backoff = self
            .interval_secs
            .load(Ordering::Relaxed)
            .max(MIN_INTERVAL_SECS);
        let mut was_paused = false;
        // The very first fetch always runs, even if the user is already idle
        // (e.g. the widget autostarts on boot) — otherwise it would show an
        // empty window until the user returns and moves the mouse.
        let mut first_run = true;

        loop {
            // Pause: skip fetching while idle/locked; re-check periodically so
            // we resume promptly once the user is back.
            if !first_run && platform::user_idle_secs() >= IDLE_PAUSE_SECS {
                if !was_paused {
                    debug!("idle/locked — pausing polling");
                    was_paused = true;
                }
                tokio::time::sleep(std::time::Duration::from_secs(PAUSE_RECHECK_SECS)).await;
                continue;
            }
            if was_paused {
                debug!("activity resumed — polling immediately");
                was_paused = false;
            }
            first_run = false;

            let any_ok = self.fetch_all().await;

            let base = self
                .interval_secs
                .load(Ordering::Relaxed)
                .max(MIN_INTERVAL_SECS);
            let sleep = if any_ok {
                backoff = base;
                base
            } else {
                // Nothing worked — wait longer before retrying, capped.
                backoff = next_backoff(backoff, base);
                debug!("no provider succeeded — backing off to {backoff}s");
                backoff
            };
            tokio::time::sleep(std::time::Duration::from_secs(sleep)).await;
        }
    }

    /// Fetch every provider once. Returns true if at least one provider
    /// produced an `Ok` result this cycle (used for error backoff). The UI is
    /// still updated incrementally as each provider responds.
    ///
    /// (next_backoff lives below as a free function so it is unit-testable.)
    async fn fetch_all(&self) -> bool {
        // A short read under the lock, no await held.
        let providers: Vec<Arc<dyn Provider>> = match self.providers.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };

        let mut handles = Vec::with_capacity(providers.len());
        for provider in providers {
            let tx = self.cmd_tx.clone();
            // Each provider gets its own tokio task; the JoinHandle yields
            // whether it succeeded so the loop can decide on backoff.
            handles.push(tokio::spawn(async move {
                match provider.fetch().await {
                    Ok(data) => {
                        let ok = matches!(data.status, ProviderStatus::Ok);
                        debug!(
                            "provider {:?} fetch status {:?}, metrics {}, primary {:?}",
                            data.id,
                            data.status,
                            data.metrics.len(),
                            data.primary_percentage()
                        );
                        // The event loop may be gone — that is not an error.
                        if tx.send(AppCommand::UpdateProvider(data)).await.is_err() {
                            error!("command channel closed, dropping provider update");
                        }
                        ok
                    }
                    Err(e) => {
                        // Log and keep the scheduler running.
                        error!("provider {:?} fetch failed: {e}", provider.id());
                        false
                    }
                }
            }));
        }

        let mut any_ok = false;
        for h in handles {
            if let Ok(true) = h.await {
                any_ok = true;
            }
        }
        any_ok
    }
}

/// Double the error backoff, bounded to `[base, max(base, MAX_BACKOFF_SECS)]`:
/// never retry FASTER than the user's interval, and never wait absurdly
/// longer either. The upper bound must track `base` — a plain
/// `clamp(base, MAX_BACKOFF_SECS)` panics when base > MAX (min > max), which
/// a 15/30-minute menu interval plus one fully-failed cycle would hit.
fn next_backoff(backoff: u64, base: u64) -> u64 {
    backoff
        .saturating_mul(2)
        .clamp(base, MAX_BACKOFF_SECS.max(base))
}

#[cfg(test)]
mod tests {
    use super::next_backoff;

    #[test]
    fn backoff_doubles_up_to_the_cap_for_short_intervals() {
        // base 60: 60 → 120 → 240 → 300 (cap) → 300.
        assert_eq!(next_backoff(60, 60), 120);
        assert_eq!(next_backoff(120, 60), 240);
        assert_eq!(next_backoff(240, 60), 300);
        assert_eq!(next_backoff(300, 60), 300);
    }

    #[test]
    fn long_user_intervals_do_not_panic_and_stay_at_base() {
        // Regression: base 900/1800 (15/30 min menu items) used to panic in
        // clamp(min > max) after one fully-failed cycle.
        assert_eq!(next_backoff(900, 900), 900);
        assert_eq!(next_backoff(1800, 1800), 1800);
    }

    #[test]
    fn backoff_never_drops_below_the_user_interval() {
        // A stale small backoff (after a live interval change) is lifted.
        assert_eq!(next_backoff(60, 900), 900);
    }
}
