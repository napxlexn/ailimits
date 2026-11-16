// ui/visual_baseline.rs — the look of every surface, pinned (tests only).
//
// Each surface module renders its default-config states with the fixtures
// below and hands the pixmaps to `check`, which compares a hash of every one
// with tests/visual/<group>.txt.
//
// Where the pinned groups come from. `widget`, `panel` and `tray` (`PINNED`)
// were recorded in 3e8aa80 (2026-11-11) on the rows-and-models branch, not
// rendered from the 0.7.0 tag, and they hold the looks of that commit. These
// differ from 0.7.0 on purpose:
//   - ac17bd2: stale data greys before its reset, not only after it;
//   - a755fe3: the taskbar panel inks a stale or estimated row dimmer;
//   - 9eada81: data counts as stale once the update interval plus a minute
//     has passed;
//   - 387fc59: a provably passed reset reads as about 0 at once, without
//     waiting to go stale;
//   - 3d6093c: hovering a stale row whose error retries by itself
//     ("offline, retrying") shows just that, where 0.7.0 added the
//     provider's action ("offline, retrying — check gh auth").
// The owner approved the first four (2026-11-16); the fifth was found by the
// comparison below. The fixtures also use what came after 0.7.0 (the Grok and
// Cursor providers, `unified_pool`, `extra_only`), so these very files
// cannot be rendered by 0.7.0. What the pins promise since 3e8aa80: with the
// default config nothing in these groups changes by a single pixel; a
// feature may add new looks, in groups of its own.
//
// Against 0.7.0 itself (2026-11-16): the 0.7.0 tag and this tree rendered
// the same fixtures cut down to what 0.7.0 can draw (no Grok or Cursor rows,
// no new fields), plus a set of network errors and a set that puts the
// estimated and the stale row first, through these very tests. Of 116 widget,
// 36 panel and 84 tray entries, 83 widget, 30 panel and all 84 tray entries
// came out byte-identical. The rest differ only in the stale row (ac17bd2),
// in the panel's stale and estimated rows (a755fe3) and in the hover text
// over the stale row (3d6093c). 9eada81 and 387fc59 change when data counts
// as stale or reset, not how it is drawn, so no fixture shows them.
//
// The hashes depend on the UI font (renderer::FONT_CANDIDATES, the first one
// that loads). tests/visual/font.txt records the font they were made with;
// on a machine that renders with another one no hash could match, so every
// group is skipped there with a visible "font differs, baseline not
// comparable" warning instead of failing — and nothing can be recorded.
//
// Record a NEW group (a look that did not exist in 0.7.0):
//     UPDATE_BASELINE=1 cargo test visual_baseline
// The pinned groups (`PINNED`) ignore that: they change only on the owner's
// decision, with UPDATE_PINNED_BASELINE=1. On a mismatch the PNGs of the
// changed entries land in %TEMP%\ailimits-visual\<group>\ for a side-by-side
// look; DUMP_VISUAL=1 writes every entry there, for a contact sheet.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use chrono::{DateTime, Duration, Utc};
use ring::digest::{digest as sha256, SHA256};
use tiny_skia::Pixmap;

use crate::providers::{
    Metric, MetricUnit, MetricWindow, ProviderData, ProviderId, ProviderStatus,
};

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

/// The groups of the looks 0.7.0 already had (recorded in 3e8aa80, see the
/// top of the file): with the default config nothing in them may change.
const PINNED: &[&str] = &["widget", "panel", "tray"];

fn visual_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("visual")
}

fn baseline_path(group: &str) -> PathBuf {
    visual_dir().join(format!("{group}.txt"))
}

/// A font as the baselines know it: the file's name and the SHA-256 of its
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FontId {
    file: String,
    sha256: String,
}

impl std::fmt::Display for FontId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.file, self.sha256)
    }
}

/// The font the baselines were recorded with: the first line of
/// tests/visual/font.txt that is not a comment, `<file> <sha256>`.
fn parse_font_record(text: &str) -> Option<FontId> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))?;
    let (file, sha) = line.split_once(' ')?;
    let sha = sha.trim().to_ascii_lowercase();
    (sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then(|| FontId {
        file: file.to_string(),
        sha256: sha,
    })
}

/// The font this machine renders with: the first of the renderer's
/// candidates that reads and parses, exactly as `load_ui_font` picks it.
fn font_here() -> Option<FontId> {
    crate::ui::renderer::FONT_CANDIDATES
        .iter()
        .find_map(|path| {
            let data = std::fs::read(path).ok()?;
            let sha = crate::updater::hex_lower(sha256(&SHA256, &data).as_ref());
            fontdue::Font::from_bytes(data, fontdue::FontSettings::default()).ok()?;
            let file = Path::new(path).file_name()?.to_string_lossy().into_owned();
            Some(FontId { file, sha256: sha })
        })
}

/// Whether this machine renders with the recorded font: Err says how it
/// differs. Decided once per test run.
fn font_check() -> &'static Result<(), String> {
    static FONT: OnceLock<Result<(), String>> = OnceLock::new();
    FONT.get_or_init(|| {
        let path = visual_dir().join("font.txt");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("no font record at {}", path.display()));
        let recorded = parse_font_record(&text)
            .unwrap_or_else(|| panic!("{} names no font: `<file> <sha256>`", path.display()));
        match font_here() {
            Some(here) if here == recorded => Ok(()),
            Some(here) => Err(format!(
                "font differs, baseline not comparable: recorded with `{recorded}`, this \
                 machine renders with `{here}`"
            )),
            None => Err(format!(
                "font differs, baseline not comparable: recorded with `{recorded}`, this \
                 machine has none of the renderer's fonts"
            )),
        }
    })
}

/// A warning that shows in `cargo test` output even for a passing test (the
/// harness captures `eprintln!`, not the stream itself), and on GitHub as an
/// annotation.
fn warn_visible(text: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr().lock(), "warning: {text}");
    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        let _ = writeln!(
            std::io::stdout().lock(),
            "::warning title=Visual baseline::{text}"
        );
    }
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
    let dump = std::env::temp_dir().join("ailimits-visual").join(group);
    if std::env::var_os("DUMP_VISUAL").is_some() {
        std::fs::create_dir_all(&dump).unwrap();
        for (name, pm) in &pixmaps {
            std::fs::write(dump.join(format!("{name}.png")), pm.encode_png().unwrap()).unwrap();
        }
    }
    let update = if PINNED.contains(&group) {
        std::env::var_os("UPDATE_PINNED_BASELINE").is_some()
    } else {
        std::env::var_os("UPDATE_BASELINE").is_some()
    };
    let font = font_check();
    if update {
        if let Err(why) = font {
            panic!(
                "not recording visual baseline '{group}': {why}. Every group is recorded with \
                 the font in tests/visual/font.txt; if this machine's font is the reference \
                 from now on, put its line there and record every group again."
            );
        }
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
    if let Err(why) = font {
        // No hash can match with another font: say so, and do not fail.
        warn_visible(&format!(
            "visual baseline '{group}' skipped — {why} ({} of {} entries differ here; PNGs \
             of those in {})",
            problems.len(),
            got.len(),
            dump.display()
        ));
        return;
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
        extra_only: false,
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
                extra_only: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_font_record_names_one_font_by_its_hash() {
        let text = std::fs::read_to_string(visual_dir().join("font.txt")).unwrap();
        let font = parse_font_record(&text).expect("tests/visual/font.txt names a font");
        assert_eq!(font.file, "segoeui.ttf");
        assert_eq!(font.sha256.len(), 64);
        // The renderer must still pick that file first where it exists.
        assert!(crate::ui::renderer::FONT_CANDIDATES
            .first()
            .is_some_and(|p| p.ends_with(&font.file)));
    }

    #[test]
    fn a_font_record_is_read_strictly() {
        let sha = "704eada12961a27fd271f7d3b09e2066520e758390c1776744eff430fee30f6e";
        let want = FontId {
            file: "segoeui.ttf".into(),
            sha256: sha.into(),
        };
        let upper = format!("# comment\n\n  segoeui.ttf {}  \r\n", sha.to_uppercase());
        assert_eq!(parse_font_record(&upper), Some(want.clone()));
        assert_eq!(
            parse_font_record(&format!("segoeui.ttf {sha}\narial.ttf {sha}")),
            Some(want),
            "the first line counts"
        );
        let short = format!("segoeui.ttf {}", &sha[..8]);
        let not_hex = format!("segoeui.ttf {}zz", &sha[..62]);
        for bad in ["", "# only a comment", "segoeui.ttf", &short, &not_hex] {
            assert_eq!(parse_font_record(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn this_machine_renders_with_some_font() {
        // Where it is the recorded one the groups compare; where it is not,
        // they are skipped with a warning. Either way the check decides
        // without failing.
        let here = font_here().expect("a UI font");
        assert_eq!(here.sha256.len(), 64);
        let _ = font_check();
    }
}
