// monitor/scheduler.rs — background update scheduler for providers.
//
// The provider list is SHARED (Arc<RwLock>): the event loop rebuilds it
// live when auth settings change in the context menu.

use crate::{
    app::AppCommand,
    platform,
    providers::{self, Provider, ProviderStatus},
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
/// The one early retry after a wake (from sleep or from the idle pause)
/// whose first cycle got nothing at all: long enough for Wi-Fi to come back,
/// short enough that the rows are not left grey for minutes.
const WAKE_RETRY_SECS: u64 = 20;
/// A sleep that took this much longer on the wall clock than it was set for
/// slept through a machine sleep (or a clock jump): the next cycle counts as
/// a wake.
const WAKE_SLACK_SECS: u64 = 90;

/// One provider of the shared list, with the serial it was built under.
///
/// Every update a fetch sends carries the serial of the instance that made
/// it; the event loop drops an update whose serial is no longer the
/// provider's current one. A provider is rebuilt (a new serial) when the
/// account behind it may have changed - a sign-in or sign-out, a key pasted
/// or removed, a consent given or taken back - so a request that was already
/// in flight cannot bring the old account's numbers back on screen.
#[derive(Clone)]
pub struct ProviderSlot {
    pub serial: u64,
    pub provider: Arc<dyn Provider>,
}

/// Shared provider list.
pub type SharedProviders = Arc<RwLock<Vec<ProviderSlot>>>;

/// Shared update interval — changed live from the context menu.
pub type SharedInterval = Arc<AtomicU64>;

/// Work that rides on every polling cycle instead of a timer of its own (the
/// model labels are re-read here). Called after the cycle's fetches, never
/// while polling is paused; it must only hand work off, not block.
pub type CycleHook = Arc<dyn Fn() + Send + Sync>;

pub struct Scheduler {
    providers: SharedProviders,
    /// Interval in seconds — shared, read before every cycle.
    interval_secs: SharedInterval,
    cmd_tx: mpsc::Sender<AppCommand>,
    on_cycle: Option<CycleHook>,
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
            on_cycle: None,
        }
    }

    /// Run `hook` after every cycle's fetches.
    pub fn with_cycle_hook(mut self, hook: CycleHook) -> Self {
        self.on_cycle = Some(hook);
        self
    }

    /// Run the scheduler (spawned as a background task).
    ///
    /// Adaptive, but conservative — it only ever polls LESS than the user's
    /// interval, never more (matching the project rule for undocumented
    /// endpoints: "when unsure, poll less"). The one exception is a single
    /// early retry right after a wake whose first cycle got nothing at all
    /// (`plan_sleep`): the network is usually still coming up then.
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
        // The last sleep: when it began on the wall clock and how long it was
        // set for. A machine sleep shows up as a wall-clock gap.
        let mut last_sleep: Option<(std::time::SystemTime, u64)> = None;

        loop {
            // Pause: skip fetching while idle/locked; re-check periodically so
            // we resume promptly once the user is back.
            if !first_run && platform::user_idle_secs() >= IDLE_PAUSE_SECS {
                if !was_paused {
                    debug!("idle/locked — pausing polling");
                    was_paused = true;
                    // The pause must not age what is on screen: the staleness
                    // rule leaves this time out.
                    providers::set_polling_paused(true);
                }
                tokio::time::sleep(std::time::Duration::from_secs(PAUSE_RECHECK_SECS)).await;
                continue;
            }
            let mut woke = last_sleep.is_some_and(|(began, secs)| slept_through(began, secs));
            if was_paused {
                debug!("activity resumed — polling immediately");
                was_paused = false;
                providers::set_polling_paused(false);
                woke = true;
            }
            first_run = false;

            let any_ok = self.fetch_all().await;
            if let Some(hook) = &self.on_cycle {
                hook();
            }

            let base = self
                .interval_secs
                .load(Ordering::Relaxed)
                .max(MIN_INTERVAL_SECS);
            // Staleness is measured against the interval this sleep runs on —
            // published here, not from the menu, because a menu change only
            // takes effect once the current sleep has ended.
            providers::set_update_interval_secs(base);
            let (sleep, next) = plan_sleep(any_ok, woke, backoff, base);
            if !any_ok {
                if woke {
                    debug!("nothing answered straight after a wake — one retry in {sleep}s");
                } else {
                    debug!("no provider succeeded — backing off to {sleep}s");
                }
            }
            backoff = next;
            last_sleep = Some((std::time::SystemTime::now(), sleep));
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
        let slots: Vec<ProviderSlot> = match self.providers.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };

        let mut handles = Vec::with_capacity(slots.len());
        for slot in slots {
            let tx = self.cmd_tx.clone();
            // Each provider gets its own tokio task.
            handles.push(tokio::spawn(async move {
                match slot.provider.fetch().await {
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
                        let update = AppCommand::UpdateProvider {
                            data,
                            serial: slot.serial,
                        };
                        if tx.send(update).await.is_err() {
                            error!("command channel closed, dropping provider update");
                        }
                        ok
                    }
                    Err(e) => {
                        // Log and keep the scheduler running.
                        error!("provider {:?} fetch failed: {e}", slot.provider.id());
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

/// Whether a sleep that began at `began` and was set for `secs` seconds took
/// so much longer on the wall clock that the machine slept meanwhile.
fn slept_through(began: std::time::SystemTime, secs: u64) -> bool {
    began
        .elapsed()
        .is_ok_and(|took| took.as_secs() > secs.saturating_add(WAKE_SLACK_SECS))
}

/// How long to sleep after a cycle, and the backoff the next one starts
/// from: `(sleep, backoff)`.
///
/// - Something answered: the user's interval, and the backoff starts over.
/// - Nothing answered right after a wake: one early retry. Waking from sleep
///   the first cycle usually runs before Wi-Fi is back, and the full backoff
///   would leave every row grey for two minutes. The backoff is left as it
///   was, so a retry that fails too backs off as usual.
/// - Nothing answered otherwise: the doubled backoff.
fn plan_sleep(any_ok: bool, woke: bool, backoff: u64, base: u64) -> (u64, u64) {
    if any_ok {
        (base, base)
    } else if woke {
        (WAKE_RETRY_SECS.min(base), backoff.max(base))
    } else {
        let next = next_backoff(backoff, base);
        (next, next)
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
    use super::{next_backoff, plan_sleep, slept_through, WAKE_RETRY_SECS};

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

    #[test]
    fn a_failed_first_cycle_after_a_wake_retries_once_soon() {
        // Woke from sleep, Wi-Fi not back yet: retry in 20 s, not 120.
        assert_eq!(plan_sleep(false, true, 60, 60), (WAKE_RETRY_SECS, 60));
        // The retry fails too (no longer a wake): the usual doubling.
        assert_eq!(plan_sleep(false, false, 60, 60), (120, 120));
        // Never sooner than the user's own interval would be.
        assert_eq!(plan_sleep(false, true, 60, 10), (10, 60));
    }

    #[test]
    fn a_working_cycle_resets_the_backoff_wake_or_not() {
        assert_eq!(plan_sleep(true, true, 240, 60), (60, 60));
        assert_eq!(plan_sleep(true, false, 240, 60), (60, 60));
    }

    #[test]
    fn a_wall_clock_gap_far_past_the_sleep_reads_as_a_wake() {
        let now = std::time::SystemTime::now();
        let hour_ago = now - std::time::Duration::from_secs(3600);
        assert!(slept_through(hour_ago, 60));
        let just_now = now - std::time::Duration::from_secs(61);
        assert!(!slept_through(just_now, 60));
    }
}
