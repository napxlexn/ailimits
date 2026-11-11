// ui/visual_baseline.rs — the look of every surface, pinned (tests only).
//
// Each surface module renders its default-config states with the fixtures
// below and hands the pixmaps to `check`, which compares a hash of every one
// with tests/visual/<group>.txt. The baseline was recorded from 0.7.0 before
// the rows-and-models work: a feature may add new looks, but with the default
// config nothing that exists today may change by a single pixel.
//
// Record or update (only for a look that is meant to change, after review):
//     UPDATE_BASELINE=1 cargo test visual_baseline
// On a mismatch the PNGs of the changed entries land in
// %TEMP%\ailimits-visual\<group>\ for a side-by-side look.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use chrono::{DateTime, Duration, Utc};
use tiny_skia::Pixmap;

use crate::providers::{Metric, MetricUnit, MetricWindow, ProviderData, ProviderId, ProviderStatus};

/// FNV-1a, 64 bit: stable across Rust versions, unlike `DefaultHasher`.
fn fnv1a(bytes: &[u8], mut h: u64) -> u64 {
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn digest(pm: &Pixmap) -> String {
    let mut h = 0xcbf2_9ce4_8422_2325;
    h = fnv1a(&pm.width().to_le_bytes(), h);
    h = fnv1a(&pm.height().to_le_bytes(), h);
    h = fnv1a(pm.data(), h);
    format!("{}x{} {h:016x}", pm.width(), pm.height())
}

fn baseline_path(group: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("visual")
        .join(format!("{group}.txt"))
}

/// Compare every pixmap with the recorded baseline of `group`.
pub(crate) fn check(group: &str, items: Vec<(String, Pixmap)>) {
    let mut got = BTreeMap::new();
    let mut pixmaps = BTreeMap::new();
    for (name, pm) in items {
        assert!(
            !name.contains(char::is_whitespace),
            "baseline entry names carry no spaces: {name:?}"
        );
        let previous = got.insert(name.clone(), digest(&pm));
        assert!(previous.is_none(), "duplicate baseline entry {name}");
        pixmaps.insert(name, pm);
    }

    let path = baseline_path(group);
    if std::env::var_os("UPDATE_BASELINE").is_some() {
        let mut text = format!(
            "# Visual baseline '{group}': name, size, FNV-1a of the RGBA pixels.\n\
             # Written by `UPDATE_BASELINE=1 cargo test visual_baseline`; see src/ui/visual_baseline.rs.\n"
        );
        for (name, d) in &got {
            text.push_str(&format!("{name} {d}\n"));
        }
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }

    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "no visual baseline at {}: record it with UPDATE_BASELINE=1",
            path.display()
        )
    });
    let want: BTreeMap<String, String> = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.split_once(' '))
        .map(|(n, d)| (n.to_string(), d.to_string()))
        .collect();

    let mut problems = Vec::new();
    let dump = std::env::temp_dir().join("ailimits-visual").join(group);
    for (name, d) in &got {
        match want.get(name) {
            Some(w) if w == d => {}
            Some(w) => problems.push(format!("changed: {name} (was {w}, now {d})")),
            None => problems.push(format!("new, not in the baseline: {name}")),
        }
        if want.get(name) != Some(d) {
            std::fs::create_dir_all(&dump).unwrap();
            let png = pixmaps[name].encode_png().unwrap();
            std::fs::write(dump.join(format!("{name}.png")), png).unwrap();
        }
    }
    for name in want.keys() {
        if !got.contains_key(name) {
            problems.push(format!("missing, no longer rendered: {name}"));
        }
    }
    assert!(
        problems.is_empty(),
        "visual baseline '{group}' differs in {} entries (PNGs in {}):\n{}",
        problems.len(),
        dump.display(),
        problems.join("\n")
    );
}

// ─── Fixtures ────────────────────────────────────────────────────

/// A moment `secs` from now plus half a minute, so a countdown read a few
/// milliseconds later still shows the same whole minute.
fn at(secs: i64) -> Option<DateTime<Utc>> {
    Some(Utc::now() + Duration::seconds(secs + 30))
}

const MIN: i64 = 60;
const HOUR: i64 = 3600;
const DAY: i64 = 86_400;

fn pct(label: &str, used: u64, window: MetricWindow, reset_in: Option<i64>) -> Metric {
    Metric {
        label: label.to_string(),
        used,
        limit: Some(100),
        unit: MetricUnit::Percent,
        reset_at: reset_in.and_then(at),
        window,
        unified_pool: false,
    }
}

fn provider(id: ProviderId, status: ProviderStatus, metrics: Vec<Metric>) -> ProviderData {
    ProviderData {
        id,
        status,
        metrics,
        updated_at: Utc::now(),
        received_at: Some(Instant::now()),
    }
}

/// What a typical day looks like: five providers, every kind of window.
pub(crate) fn typical() -> Vec<ProviderData> {
    use MetricWindow::{Long, Session};
    use ProviderStatus::Ok;
    vec![
        provider(
            ProviderId::Claude,
            Ok,
            vec![
                pct("Session", 25, Session, Some(2 * HOUR + 14 * MIN)),
                pct("Weekly", 34, Long, Some(3 * DAY + 4 * HOUR)),
            ],
        ),
        provider(
            ProviderId::Codex,
            Ok,
            vec![
                pct("Session", 48, Session, Some(4 * HOUR + 51 * MIN)),
                pct("Weekly", 52, Long, Some(5 * DAY + 2 * HOUR)),
            ],
        ),
        provider(
            ProviderId::Grok,
            Ok,
            vec![pct("Weekly", 12, Long, Some(6 * DAY + 2 * HOUR))],
        ),
        provider(
            ProviderId::Antigravity,
            Ok,
            vec![
                pct("Gemini", 30, Long, Some(5 * DAY)),
                pct("Claude/GPT", 64, Long, Some(5 * DAY)),
            ],
        ),
        provider(
            ProviderId::Copilot,
            Ok,
            vec![Metric {
                label: "Premium".to_string(),
                used: 123,
                limit: Some(300),
                unit: MetricUnit::Requests,
                reset_at: at(20 * DAY + 3 * HOUR),
                window: MetricWindow::Long,
                unified_pool: false,
            }],
        ),
    ]
}

/// Usage levels: high, spent session, spent week over a low session.
pub(crate) fn levels() -> Vec<ProviderData> {
    use MetricWindow::{Long, Session};
    use ProviderStatus::Ok;
    vec![
        provider(
            ProviderId::Claude,
            Ok,
            vec![
                pct("Session", 85, Session, Some(37 * MIN)),
                pct("Weekly", 71, Long, Some(2 * DAY)),
            ],
        ),
        provider(
            ProviderId::Codex,
            Ok,
            vec![
                pct("Session", 20, Session, Some(HOUR + 5 * MIN)),
                pct("Weekly", 100, Long, Some(DAY + 7 * HOUR)),
            ],
        ),
        provider(
            ProviderId::Cursor,
            Ok,
            vec![pct("Premium", 100, Session, Some(9 * DAY))],
        ),
    ]
}

/// The honest states: loading, sign-in trouble, offline, estimated, aged.
pub(crate) fn states() -> Vec<ProviderData> {
    use MetricWindow::{Long, Session};
    let mut aged = provider(
        ProviderId::Copilot,
        ProviderStatus::Ok,
        vec![pct("Weekly", 44, Long, Some(4 * DAY))],
    );
    // A disk-cache entry two and a half hours old: wall-clock age, no
    // monotonic anchor, so the countdown and the age text are stable.
    aged.updated_at = Utc::now() - Duration::seconds(2 * HOUR + 30 * MIN + 30);
    aged.received_at = None;
    vec![
        provider(ProviderId::Claude, ProviderStatus::Loading, vec![]),
        provider(
            ProviderId::Codex,
            ProviderStatus::AuthError("token expired".to_string()),
            vec![],
        ),
        provider(
            ProviderId::Grok,
            ProviderStatus::NetworkError("offline, retrying".to_string()),
            vec![],
        ),
        provider(
            ProviderId::Antigravity,
            ProviderStatus::Estimated,
            vec![pct("Session", 0, Session, Some(4 * HOUR + 59 * MIN))],
        ),
        aged,
    ]
}

/// One provider only.
pub(crate) fn single() -> Vec<ProviderData> {
    typical().into_iter().take(1).collect()
}

/// Every fixture set, named.
pub(crate) fn sets() -> Vec<(&'static str, Vec<ProviderData>)> {
    vec![
        ("typical", typical()),
        ("levels", levels()),
        ("states", states()),
        ("single", single()),
    ]
}
