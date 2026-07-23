// monitor/trend.rs — usage burn-rate projection.
//
// We already get the true server-side usage %, so the forecast is a simple
// least-squares extrapolation over a short recent window — no token
// estimation. It is advisory: when the data is uncertain (too few samples,
// not rising, or the window resets before the projected hit), there is no
// forecast and the UI falls back to the normal reset countdown.

use chrono::{DateTime, Duration, Utc};
use std::collections::VecDeque;

/// Look only at samples from the last hour.
const WINDOW_SECS: i64 = 3600;
const MIN_SAMPLES: usize = 3;
/// Below this slope (≈0.36 %/hour) we treat usage as flat — no forecast.
const MIN_SLOPE_PER_SEC: f64 = 1e-4;
const MAX_SAMPLES: usize = 64;

/// A projected moment the metric reaches 100%.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Forecast {
    pub hit_at: DateTime<Utc>,
}

/// Per-provider ring buffer of `(timestamp, percent)` samples.
#[derive(Default)]
pub struct Trend {
    samples: VecDeque<(DateTime<Utc>, f32)>,
}

impl Trend {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::new(),
        }
    }

    /// Record a sample. A sharp drop means the window reset, so the old
    /// history is discarded (a trend across a reset boundary is meaningless).
    pub fn push(&mut self, ts: DateTime<Utc>, pct: f32) {
        if let Some(&(_, last)) = self.samples.back() {
            if pct + 30.0 < last {
                self.samples.clear();
            }
        }
        self.samples.push_back((ts, pct));
        while self.samples.len() > MAX_SAMPLES {
            self.samples.pop_front();
        }
        let cutoff = ts - Duration::seconds(WINDOW_SECS);
        while self.samples.front().is_some_and(|(t, _)| *t < cutoff) {
            self.samples.pop_front();
        }
    }

    pub fn forecast(&self, reset_at: Option<DateTime<Utc>>) -> Option<Forecast> {
        let v: Vec<(DateTime<Utc>, f32)> = self.samples.iter().copied().collect();
        project(&v, reset_at)
    }
}

/// Pure projection: least-squares slope of recent samples → time to reach 100%.
/// Returns None when too few samples, not meaningfully rising, already at the
/// limit, or the window resets before the projected hit.
pub fn project(
    samples: &[(DateTime<Utc>, f32)],
    reset_at: Option<DateTime<Utc>>,
) -> Option<Forecast> {
    if samples.len() < MIN_SAMPLES {
        return None;
    }
    let latest = *samples.last()?;
    let recent: Vec<(DateTime<Utc>, f32)> = samples
        .iter()
        .filter(|(t, _)| (latest.0 - *t).num_seconds() <= WINDOW_SECS)
        .copied()
        .collect();
    if recent.len() < MIN_SAMPLES {
        return None;
    }

    // x = seconds since the first recent sample; y = percent.
    let t0 = recent[0].0;
    let xs: Vec<f64> = recent
        .iter()
        .map(|(t, _)| (*t - t0).num_seconds() as f64)
        .collect();
    let ys: Vec<f64> = recent.iter().map(|(_, p)| *p as f64).collect();
    let n = xs.len() as f64;
    let sx: f64 = xs.iter().sum();
    let sy: f64 = ys.iter().sum();
    let sxx: f64 = xs.iter().map(|x| x * x).sum();
    let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| x * y).sum();
    let denom = n * sxx - sx * sx;
    if denom.abs() < 1e-6 {
        return None;
    }
    let slope = (n * sxy - sx * sy) / denom; // percent per second
    if slope <= MIN_SLOPE_PER_SEC {
        return None;
    }

    let latest_pct = latest.1 as f64;
    if latest_pct >= 100.0 {
        return None;
    }
    let secs = ((100.0 - latest_pct) / slope).round() as i64;
    if secs <= 0 {
        return None;
    }
    let hit_at = latest.0 + Duration::seconds(secs);

    // If the limit window resets before we'd hit it, there is nothing to warn about.
    if let Some(reset) = reset_at {
        if hit_at >= reset {
            return None;
        }
    }
    Some(Forecast { hit_at })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(points: &[(i64, f32)]) -> Vec<(DateTime<Utc>, f32)> {
        let base = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        points
            .iter()
            .map(|(off, p)| (base + Duration::seconds(*off), *p))
            .collect()
    }

    #[test]
    fn rising_series_projects_a_hit_before_reset() {
        // 50% → 80% over 30 min ⇒ ~1%/min ⇒ ~20 more min to 100%.
        let s = series(&[(0, 50.0), (900, 65.0), (1800, 80.0)]);
        let reset = DateTime::<Utc>::from_timestamp(1_700_000_000 + 7200, 0);
        let f = project(&s, reset).expect("should forecast");
        let secs = (f.hit_at - s.last().unwrap().0).num_seconds();
        assert!((1000..=1500).contains(&secs), "got {secs}s to limit");
    }

    #[test]
    fn flat_series_has_no_forecast() {
        let s = series(&[(0, 42.0), (900, 42.0), (1800, 42.0)]);
        assert!(project(&s, None).is_none());
    }

    #[test]
    fn declining_series_has_no_forecast() {
        let s = series(&[(0, 80.0), (900, 60.0), (1800, 40.0)]);
        assert!(project(&s, None).is_none());
    }

    #[test]
    fn too_few_samples_has_no_forecast() {
        let s = series(&[(0, 10.0), (900, 50.0)]);
        assert!(project(&s, None).is_none());
    }

    #[test]
    fn reset_before_hit_suppresses_forecast() {
        // Rising, but the window resets in 5 min — sooner than the ~20 min hit.
        let s = series(&[(0, 50.0), (900, 65.0), (1800, 80.0)]);
        let reset = DateTime::<Utc>::from_timestamp(1_700_000_000 + 1800 + 300, 0);
        assert!(project(&s, reset).is_none());
    }

    #[test]
    fn push_clears_on_reset_drop() {
        let mut t = Trend::new();
        let base = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        t.push(base, 95.0);
        t.push(base + Duration::seconds(60), 2.0); // window reset
        assert!(t.forecast(None).is_none()); // history cleared → not enough samples
    }
}
