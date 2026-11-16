// notifications/alerts.rs — when a threshold alert (the toast and the
// `on_threshold` hook) is due.
//
// An alert says one thing: "this limit window has reached your threshold".
// It is said once per window, not again every cooldown while usage stays
// up, and not again after a restart (the book is kept in `alerts.json` next
// to the config). It is said again when the window resets and the new one
// reaches the threshold, or when usage fell well below the threshold and
// climbed back. The cooldown stays as the shortest time between two alerts
// of one provider.

use crate::providers::ProviderId;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Usage has to fall this many points under the threshold before the same
/// window may alert again: a value wobbling around the threshold is one
/// crossing, not one per wobble.
pub const REARM_BELOW_POINTS: f32 = 5.0;

/// Two reset times this close apart name the same window. Some providers
/// send the time left rather than the reset time, so the reset computed from
/// it moves by a few seconds from one fetch to the next; real windows are
/// hours apart.
const SAME_WINDOW_SLACK_MINUTES: i64 = 10;

/// Windows remembered per provider: the ones alerted lately. A headline
/// that switches between two windows (a spent week, then the session again)
/// alerts once for each, not on every switch.
const WINDOWS_PER_PROVIDER: usize = 4;

/// How long an entry is kept after its window reset or its last alert:
/// long enough for the cooldown, short enough that the file stays small.
const KEEP_DAYS: i64 = 1;

/// One provider's alerts: the windows alerted (each by its reset time, when
/// the provider gives one) and when the last alert was.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct Entry {
    #[serde(default)]
    windows: Vec<Option<DateTime<Utc>>>,
    #[serde(default)]
    last: Option<DateTime<Utc>>,
}

/// The alerts given, by provider (its config id, so a provider a newer
/// version added survives a round trip through this one).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AlertBook {
    #[serde(default)]
    providers: HashMap<String, Entry>,
}

/// What one reading decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    /// Alert now: show the toast and run the hook.
    pub fire: bool,
    /// The book changed and is to be saved.
    pub changed: bool,
}

fn key(id: &ProviderId) -> String {
    serde_json::to_value(id)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn same_window(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => (a - b).num_minutes().abs() <= SAME_WINDOW_SLACK_MINUTES,
        _ => false,
    }
}

impl AlertBook {
    /// The book saved in `alerts.json`, or an empty one when the file is
    /// missing or unreadable (the worst that does is one alert again).
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    /// The file's bytes, or None when there is nothing to keep.
    pub fn to_json(&self) -> Option<Vec<u8>> {
        if self.providers.is_empty() {
            return None;
        }
        serde_json::to_vec_pretty(self).ok()
    }

    /// A fresh reading of a provider's headline: `pct` of its window that
    /// resets at `reset_at`, against the provider's `threshold`.
    pub fn observe(
        &mut self,
        id: &ProviderId,
        pct: f32,
        threshold: f32,
        reset_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
        cooldown: Duration,
    ) -> Seen {
        let k = key(id);
        let before = self.providers.get(&k).cloned();
        let mut entry = before.clone().unwrap_or_default();
        let keep = Duration::days(KEEP_DAYS);
        entry
            .windows
            .retain(|w| w.is_none_or(|reset| reset + keep > now));
        // Well under the threshold again: the next crossing is news.
        if pct < threshold - REARM_BELOW_POINTS {
            entry.windows.clear();
        }
        let mut fire = false;
        if pct >= threshold {
            let told = entry.windows.iter().any(|w| same_window(*w, reset_at));
            // A wall clock set back must not silence alerts until it catches up.
            let cooled = entry
                .last
                .is_none_or(|last| now < last || now - last >= cooldown);
            if !told && cooled {
                entry.windows.push(reset_at);
                if entry.windows.len() > WINDOWS_PER_PROVIDER {
                    entry.windows.remove(0);
                }
                entry.last = Some(now);
                fire = true;
            }
        }
        let recent = entry.last.is_some_and(|last| now < last + keep);
        let after = (!entry.windows.is_empty() || recent).then_some(entry);
        let changed = after != before;
        match after {
            Some(entry) => {
                self.providers.insert(k, entry);
            }
            None => {
                self.providers.remove(&k);
            }
        }
        Seen { fire, changed }
    }

    /// Forget a provider: switched off or signed out, its next reading is
    /// news. Returns whether anything was forgotten.
    pub fn forget(&mut self, id: &ProviderId) -> bool {
        self.providers.remove(&key(id)).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COOLDOWN: i64 = 15;

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-16T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
            + Duration::minutes(minutes)
    }

    fn see(book: &mut AlertBook, pct: f32, reset: Option<DateTime<Utc>>, minute: i64) -> bool {
        book.observe(
            &ProviderId::Claude,
            pct,
            80.0,
            reset,
            at(minute),
            Duration::minutes(COOLDOWN),
        )
        .fire
    }

    #[test]
    fn three_hours_above_the_threshold_alert_once() {
        // Before: one toast every 15 minutes, twelve in three hours.
        let mut book = AlertBook::default();
        let reset = Some(at(300));
        let fired = (0..180).filter(|m| see(&mut book, 85.0, reset, *m)).count();
        assert_eq!(fired, 1);
    }

    #[test]
    fn a_new_window_alerts_again() {
        let mut book = AlertBook::default();
        assert!(see(&mut book, 85.0, Some(at(60)), 0));
        assert!(!see(&mut book, 90.0, Some(at(60)), 30));
        // The window reset at minute 60; the next one reaches 80% too.
        assert!(see(&mut book, 82.0, Some(at(360)), 120));
    }

    #[test]
    fn a_reset_time_that_drifts_by_seconds_is_the_same_window() {
        let mut book = AlertBook::default();
        assert!(see(&mut book, 85.0, Some(at(300)), 0));
        let drifted = Some(at(300) + Duration::seconds(7));
        assert!(!see(&mut book, 86.0, drifted, 20));
    }

    #[test]
    fn falling_well_below_rearms_and_wobbling_does_not() {
        let mut book = AlertBook::default();
        assert!(see(&mut book, 81.0, None, 0));
        // A wobble around the threshold is the same crossing.
        assert!(!see(&mut book, 78.0, None, 20));
        assert!(!see(&mut book, 81.0, None, 40));
        // Well below, then back up: a new crossing.
        assert!(!see(&mut book, 60.0, None, 60));
        assert!(see(&mut book, 81.0, None, 80));
    }

    #[test]
    fn the_cooldown_stays_the_shortest_gap() {
        let mut book = AlertBook::default();
        assert!(see(&mut book, 85.0, Some(at(5)), 0));
        // A new window minutes later: held back until the cooldown ends.
        assert!(!see(&mut book, 85.0, Some(at(300)), 6));
        assert!(see(&mut book, 85.0, Some(at(300)), COOLDOWN));
        // Re-armed by a drop, a quick climb still waits out the cooldown.
        assert!(!see(&mut book, 10.0, Some(at(300)), COOLDOWN + 1));
        assert!(!see(&mut book, 85.0, Some(at(300)), COOLDOWN + 2));
        assert!(see(&mut book, 85.0, Some(at(300)), 2 * COOLDOWN));
    }

    #[test]
    fn the_book_survives_a_restart() {
        let mut book = AlertBook::default();
        assert!(see(&mut book, 85.0, Some(at(300)), 0));
        let saved = book.to_json().unwrap();
        let mut again = AlertBook::from_json(std::str::from_utf8(&saved).unwrap());
        // The relaunched widget does not alert for the same window again.
        assert!(!see(&mut again, 85.0, Some(at(300)), 30));
    }

    #[test]
    fn unknown_providers_and_a_broken_file_are_harmless() {
        let book = AlertBook::from_json(
            r#"{"providers":{"someday":{"windows":[null],"last":"2026-11-16T10:00:00Z"}}}"#,
        );
        assert_eq!(book.providers.len(), 1);
        assert_eq!(AlertBook::from_json("not json"), AlertBook::default());
        assert_eq!(AlertBook::default().to_json(), None);
    }

    #[test]
    fn forgetting_a_provider_makes_its_next_crossing_news() {
        let mut book = AlertBook::default();
        assert!(see(&mut book, 85.0, Some(at(300)), 0));
        assert!(book.forget(&ProviderId::Claude));
        assert!(see(&mut book, 85.0, Some(at(300)), 30));
    }

    #[test]
    fn nothing_changes_while_below_the_threshold() {
        let mut book = AlertBook::default();
        let seen = book.observe(
            &ProviderId::Codex,
            40.0,
            80.0,
            None,
            at(0),
            Duration::minutes(COOLDOWN),
        );
        assert_eq!(
            seen,
            Seen {
                fire: false,
                changed: false
            }
        );
        assert_eq!(book.to_json(), None);
    }

    #[test]
    fn the_book_forgets_old_windows_by_itself() {
        let mut book = AlertBook::default();
        assert!(see(&mut book, 85.0, Some(at(60)), 0));
        // Two days on, below the threshold: nothing is worth keeping.
        assert!(!see(&mut book, 20.0, None, 2 * 24 * 60));
        assert_eq!(book.to_json(), None);
    }
}
