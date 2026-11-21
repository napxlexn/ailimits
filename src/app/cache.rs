// app/cache.rs — the provider cache and the one writer of the app's small
// state files.
//
// Metrics with a still-future reset survive widget restarts: after a
// relaunch the row shows the last real value (greyed, with its age) even if
// the source is temporarily unavailable, instead of an error text.

use crate::utc::{self, SignedDuration};
use crate::{
    config::storage,
    providers::{Metric, MetricWindow, ProviderData, ProviderId, ProviderStatus},
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::warn;

pub(super) fn provider_cache_path() -> PathBuf {
    storage::config_path().with_file_name("provider-cache.json")
}

fn cache_metric_is_valid(metric: &Metric) -> bool {
    // Past its reset the number is dead: the window rolled over.
    metric
        .reset_at
        .is_some_and(|reset| reset > utc::earlier(utc::now(), SignedDuration::from_mins(1)))
        && metric.percentage().is_some()
}

pub(super) fn prune_provider_cache_entry(data: &ProviderData) -> Option<ProviderData> {
    if !matches!(data.status, ProviderStatus::Ok) {
        return None;
    }

    let metrics: Vec<Metric> = data
        .metrics
        .iter()
        .filter(|metric| cache_metric_is_valid(metric))
        .cloned()
        .collect();

    (!metrics.is_empty()).then(|| ProviderData {
        metrics,
        ..data.clone()
    })
}

pub(super) fn cacheable_provider_data(data: &ProviderData) -> Option<ProviderData> {
    prune_provider_cache_entry(data)
}

/// Whether a live metric already occupies the slot a cached metric would fill.
///
/// The label is authoritative when both sides have one. Beyond that, only
/// session-window metrics may collapse onto a shared slot by unit: providers
/// rename those rows between fetches (Antigravity follows Google's model
/// rotation), whereas long windows are distinct named pools — Claude reports
/// Weekly, Opus and Sonnet at once — so two differently-labelled long metrics
/// are always different slots.
pub(super) fn metrics_match_slot(a: &Metric, b: &Metric) -> bool {
    if a.label.eq_ignore_ascii_case(&b.label) {
        return true;
    }
    if matches!(a.window, MetricWindow::Long) || matches!(b.window, MetricWindow::Long) {
        return false;
    }
    std::mem::discriminant(&a.unit) == std::mem::discriminant(&b.unit)
}

pub(super) fn merge_cached_metrics(
    mut data: ProviderData,
    cached: Option<&ProviderData>,
) -> ProviderData {
    let Some(cached) = cached else {
        return data;
    };
    if data.id != cached.id || !matches!(cached.status, ProviderStatus::Ok) {
        return data;
    }

    for cached_metric in &cached.metrics {
        if !data
            .metrics
            .iter()
            .any(|metric| metrics_match_slot(metric, cached_metric))
        {
            data.metrics.push(cached_metric.clone());
        }
    }
    data
}

pub(super) fn prune_provider_cache_map(cache: &mut HashMap<ProviderId, ProviderData>) -> bool {
    let before: HashMap<ProviderId, usize> = cache
        .iter()
        .map(|(id, data)| (id.clone(), data.metrics.len()))
        .collect();
    let pruned: HashMap<ProviderId, ProviderData> = cache
        .values()
        .filter_map(prune_provider_cache_entry)
        .map(|data| (data.id.clone(), data))
        .collect();
    let changed = before.len() != pruned.len()
        || pruned.iter().any(|(id, data)| {
            before
                .get(id)
                .is_none_or(|metric_count| *metric_count != data.metrics.len())
        });
    *cache = pruned;
    changed
}

pub(super) async fn load_provider_cache() -> HashMap<ProviderId, ProviderData> {
    let path = provider_cache_path();
    let content = match tokio::fs::read_to_string(&path).await {
        Ok(content) => content,
        Err(_) => return HashMap::new(),
    };

    match serde_json::from_str::<Vec<ProviderData>>(&content) {
        Ok(items) => items
            .into_iter()
            .filter_map(|data| prune_provider_cache_entry(&data))
            .map(|data| (data.id.clone(), data))
            .collect(),
        Err(e) => {
            warn!("provider cache parse failed: {e}");
            HashMap::new()
        }
    }
}

/// The order entries are written in, so the file does not reshuffle.
fn provider_order(id: &ProviderId) -> u8 {
    match id {
        ProviderId::Claude => 0,
        ProviderId::Codex => 1,
        ProviderId::Copilot => 2,
        ProviderId::Antigravity => 3,
        ProviderId::Grok => 4,
        ProviderId::Cursor => 5,
    }
}

/// What `provider-cache.json` would hold for this cache: its bytes, or None
/// when nothing is worth keeping and the file goes.
pub(super) fn cache_file_bytes(cache: &HashMap<ProviderId, ProviderData>) -> Option<Vec<u8>> {
    let mut items: Vec<ProviderData> = cache
        .values()
        .filter_map(prune_provider_cache_entry)
        .collect();
    if items.is_empty() {
        return None;
    }
    items.sort_by_key(|d| provider_order(&d.id));
    match serde_json::to_string_pretty(&items) {
        Ok(text) => Some(text.into_bytes()),
        Err(e) => {
            warn!("provider cache not serialised: {e}");
            None
        }
    }
}

/// The part of the cache a restart would show differently: every metric's
/// value, limit and reset, but not the time it was fetched. A cycle that
/// brings the same numbers again changes nothing here. Resets count to the
/// minute: a provider that sends the time left rather than the reset time
/// moves its reset by a second or so from one fetch to the next.
fn cache_meaning(cache: &HashMap<ProviderId, ProviderData>) -> String {
    let mut items: Vec<(u8, Vec<Metric>)> = cache
        .values()
        .map(|d| {
            let metrics = d
                .metrics
                .iter()
                .map(|m| Metric {
                    reset_at: m.reset_at.map(utc::trunc_to_minute),
                    ..m.clone()
                })
                .collect();
            (provider_order(&d.id), metrics)
        })
        .collect();
    items.sort_by_key(|(order, _)| *order);
    serde_json::to_string(&items).unwrap_or_default()
}

/// How stale the fetch times in the file may get while the numbers stay the
/// same. A restart shows a cached row's age from its fetch time, so this is
/// also the most a row may read older than it is after the widget was killed
/// without a chance to write (a crash, a forced shutdown). Never younger.
pub(super) const CACHE_STAMP_REFRESH: std::time::Duration = std::time::Duration::from_secs(600);

/// When `provider-cache.json` is written. Before this it was rewritten after
/// every successful fetch - up to one write per provider per minute, each
/// one scanned by the antivirus - although a fetch mostly brings the same
/// numbers back. Now a write happens when the numbers change, otherwise at
/// most every `CACHE_STAMP_REFRESH` for the fetch times, and once at exit.
pub(super) struct CacheLedger {
    /// What the file holds now, as far as this session knows.
    written: Option<Option<Vec<u8>>>,
    meaning: String,
    at: std::time::Instant,
}

impl CacheLedger {
    /// Start from the cache as loaded: the file already holds that meaning.
    pub(super) fn new(loaded: &HashMap<ProviderId, ProviderData>) -> Self {
        Self {
            written: None,
            meaning: cache_meaning(loaded),
            at: std::time::Instant::now(),
        }
    }

    /// The bytes to write now, if the cache is to be written: `Some(None)`
    /// removes the file.
    pub(super) fn due(
        &mut self,
        cache: &HashMap<ProviderId, ProviderData>,
        now: std::time::Instant,
    ) -> Option<Option<Vec<u8>>> {
        let bytes = cache_file_bytes(cache);
        if self.written.as_ref() == Some(&bytes) {
            return None;
        }
        let meaning = cache_meaning(cache);
        let stamps_only = meaning == self.meaning;
        if stamps_only && now.duration_since(self.at) < CACHE_STAMP_REFRESH {
            return None;
        }
        self.meaning = meaning;
        self.at = now;
        self.written = Some(bytes.clone());
        Some(bytes)
    }

    /// The bytes to write at exit, if the file is behind the cache at all.
    pub(super) fn at_exit(
        &mut self,
        cache: &HashMap<ProviderId, ProviderData>,
    ) -> Option<Option<Vec<u8>>> {
        let bytes = cache_file_bytes(cache);
        if self.written.as_ref() == Some(&bytes) {
            return None;
        }
        self.written = Some(bytes.clone());
        Some(bytes)
    }
}

// ─── One writer per file ──────────────────────────────────────────

/// What a file writer is to do.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum FileJob {
    Write(Arc<Vec<u8>>),
    Remove,
}

impl From<Option<Vec<u8>>> for FileJob {
    fn from(bytes: Option<Vec<u8>>) -> Self {
        match bytes {
            Some(b) => FileJob::Write(Arc::new(b)),
            None => FileJob::Remove,
        }
    }
}

#[derive(Clone, Default)]
struct WriterSlot {
    job: Option<FileJob>,
    /// Bumped with every job, so a job is carried out once.
    seq: u64,
    /// The writer finishes after this job.
    last: bool,
}

/// How long the final write at exit may take before the app quits anyway.
const FINAL_WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// The single writer of one small file. Jobs go through a one-slot channel
/// and the newest wins, so the file is written in order (an older snapshot
/// can never be renamed over a newer one) and a burst - every provider of
/// one cycle answering within a second or two - becomes one write after
/// `settle`.
pub(super) struct FileWriter {
    tx: Arc<tokio::sync::watch::Sender<WriterSlot>>,
    task: tokio::task::JoinHandle<()>,
}

impl FileWriter {
    pub(super) fn spawn(
        rt: &tokio::runtime::Handle,
        path: PathBuf,
        settle: std::time::Duration,
    ) -> Self {
        let (tx, mut rx) = tokio::sync::watch::channel(WriterSlot::default());
        let task = rt.spawn(async move {
            let mut done = 0u64;
            while rx.changed().await.is_ok() {
                if !rx.borrow().last && !settle.is_zero() {
                    tokio::time::sleep(settle).await;
                }
                let slot = rx.borrow_and_update().clone();
                if slot.seq != done {
                    done = slot.seq;
                    if let Some(job) = slot.job {
                        if let Err(e) = run_job(&path, &job).await {
                            warn!("writing {} failed: {e}", file_name(&path));
                        }
                    }
                }
                if slot.last {
                    break;
                }
            }
        });
        Self {
            tx: Arc::new(tx),
            task,
        }
    }

    /// Queue a job; a newer one replaces it if it has not run yet.
    pub(super) fn send(&self, job: FileJob) {
        self.tx.send_if_modified(|slot| {
            if slot.last {
                return false;
            }
            slot.job = Some(job);
            slot.seq += 1;
            true
        });
    }

    /// Carry out `last` (or whatever is still queued) and BLOCK until the
    /// writer has finished, as `storage::ConfigSaver::shutdown` does and for
    /// the same reasons: one writer, and the newest state on disk before the
    /// process exits.
    pub(super) fn shutdown(self, last: Option<FileJob>, rt: &tokio::runtime::Handle) {
        let FileWriter { tx, task } = self;
        tx.send_modify(|slot| {
            if let Some(job) = last {
                slot.job = Some(job);
                slot.seq += 1;
            }
            slot.last = true;
        });
        let rt = rt.clone();
        let waited = std::thread::spawn(move || {
            rt.block_on(async move { tokio::time::timeout(FINAL_WRITE_TIMEOUT, task).await })
        })
        .join();
        if !matches!(waited, Ok(Ok(Ok(())))) {
            warn!("a final state write did not finish in time");
        }
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

async fn run_job(path: &std::path::Path, job: &FileJob) -> std::io::Result<()> {
    match job {
        FileJob::Write(bytes) => {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            storage::atomic_write(path, bytes).await
        }
        FileJob::Remove => match tokio::fs::remove_file(path).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MetricUnit;

    fn pct(label: &str, used: u64, window: MetricWindow) -> Metric {
        Metric {
            label: label.to_string(),
            used,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: None,
            window,
            unified_pool: false,
            extra_only: false,
        }
    }

    fn data(metrics: Vec<Metric>) -> ProviderData {
        ProviderData {
            id: ProviderId::Claude,
            status: ProviderStatus::Ok,
            metrics,
            updated_at: utc::now(),
            received_at: Some(std::time::Instant::now()),
        }
    }

    #[test]
    fn a_session_metric_never_covers_a_long_windows_slot() {
        // The old label guess called neither of these "weekly" and then matched
        // them on their shared unit, so a cached Opus row was thrown away.
        let session = pct("Session", 20, MetricWindow::Session);
        let opus = pct("Opus", 100, MetricWindow::Long);
        assert!(!metrics_match_slot(&session, &opus));
    }

    #[test]
    fn two_different_long_pools_are_different_slots() {
        // Claude reports Weekly, Opus and Sonnet at the same time; one long
        // window no longer stands for all of them.
        let weekly = pct("Weekly", 50, MetricWindow::Long);
        let opus = pct("Opus", 100, MetricWindow::Long);
        assert!(!metrics_match_slot(&weekly, &opus));
    }

    #[test]
    fn the_same_label_is_always_the_same_slot() {
        let a = pct("Weekly", 50, MetricWindow::Long);
        let b = pct("Weekly", 90, MetricWindow::Long);
        assert!(metrics_match_slot(&a, &b));
        let c = pct("Session", 10, MetricWindow::Session);
        let d = pct("session", 80, MetricWindow::Session);
        assert!(metrics_match_slot(&c, &d));
    }

    #[test]
    fn session_metrics_with_different_labels_still_share_a_slot_by_unit() {
        // Antigravity renames its rows as Google rotates models, so two session
        // metrics of the same unit must keep collapsing onto one slot.
        let a = pct("2.5 Pro", 30, MetricWindow::Session);
        let b = pct("3.6 Flash", 40, MetricWindow::Session);
        assert!(metrics_match_slot(&a, &b));
    }

    #[test]
    fn merging_restores_long_pools_the_api_omitted_this_cycle() {
        // Claude answers "seven_day_opus": null on some cycles; the cached rows
        // must come back instead of being swallowed by the Session metric.
        let live = data(vec![
            pct("Session", 20, MetricWindow::Session),
            pct("Weekly", 60, MetricWindow::Long),
        ]);
        let cached = data(vec![
            pct("Session", 15, MetricWindow::Session),
            pct("Weekly", 55, MetricWindow::Long),
            pct("Opus", 100, MetricWindow::Long),
            pct("Sonnet", 40, MetricWindow::Long),
        ]);
        let merged = merge_cached_metrics(live, Some(&cached));
        let labels: Vec<&str> = merged.metrics.iter().map(|m| m.label.as_str()).collect();
        assert!(labels.contains(&"Opus"), "Opus was dropped: {labels:?}");
        assert!(labels.contains(&"Sonnet"), "Sonnet was dropped: {labels:?}");
        // The live values must win for the slots the API did return.
        let weekly = merged.metrics.iter().find(|m| m.label == "Weekly").unwrap();
        assert_eq!(weekly.used, 60);
        assert_eq!(merged.metrics.len(), 4, "no duplicate slots: {labels:?}");
    }

    fn cache_with(used: u64, fetched: utc::Timestamp) -> HashMap<ProviderId, ProviderData> {
        let mut m = pct("Session", used, MetricWindow::Session);
        // The reset as such a provider computes it: the fetch time plus the
        // time left, so it wobbles by the time a request takes.
        let wobble = SignedDuration::from_millis(i64::from(fetched.subsec_millisecond()) % 700);
        m.reset_at = Some(utc::parse_rfc3339("2030-01-01T13:00:20Z").unwrap() + wobble);
        let mut d = data(vec![m]);
        d.updated_at = fetched;
        HashMap::from([(ProviderId::Claude, d)])
    }

    #[test]
    fn the_same_numbers_again_are_not_written_every_cycle() {
        let start = std::time::Instant::now();
        let t0 = utc::now();
        let mut ledger = CacheLedger::new(&HashMap::new());
        // The first real numbers are written.
        assert!(ledger.due(&cache_with(20, t0), start).is_some());
        // The next cycle brings the same numbers, fetched a minute later.
        let a_minute = std::time::Duration::from_secs(60);
        let later = cache_with(20, t0 + SignedDuration::from_mins(1));
        assert!(ledger.due(&later, start + a_minute).is_none());
        // Changed numbers are written at once.
        let changed = cache_with(21, t0 + SignedDuration::from_mins(2));
        assert!(ledger.due(&changed, start + 2 * a_minute).is_some());
        // The same numbers for long enough: the fetch time is brought up.
        let much_later = cache_with(21, t0 + SignedDuration::from_mins(13));
        assert!(ledger
            .due(&much_later, start + 2 * a_minute + CACHE_STAMP_REFRESH)
            .is_some());
    }

    #[test]
    fn exit_writes_only_when_the_file_is_behind() {
        let start = std::time::Instant::now();
        let t0 = utc::now();
        let mut ledger = CacheLedger::new(&HashMap::new());
        let cache = cache_with(20, t0);
        assert!(ledger.due(&cache, start).is_some());
        assert!(ledger.at_exit(&cache).is_none(), "nothing new to write");
        let newer = cache_with(20, t0 + SignedDuration::from_mins(1));
        assert!(ledger.due(&newer, start).is_none());
        assert!(
            ledger.at_exit(&newer).is_some(),
            "the newer fetch time goes"
        );
    }

    #[test]
    fn an_empty_cache_removes_the_file() {
        let mut ledger = CacheLedger::new(&cache_with(20, utc::now()));
        assert_eq!(
            ledger.due(&HashMap::new(), std::time::Instant::now()),
            Some(None)
        );
    }

    #[test]
    fn the_writer_writes_the_newest_job_once_and_in_order() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let dir = std::env::temp_dir().join(format!("ailimits-writer-{}", std::process::id()));
        let path = dir.join("state.json");
        let writer = FileWriter::spawn(
            rt.handle(),
            path.clone(),
            std::time::Duration::from_millis(50),
        );
        // A burst: only the newest survives the settle time.
        writer.send(FileJob::Write(Arc::new(b"old".to_vec())));
        writer.send(FileJob::Write(Arc::new(b"new".to_vec())));
        std::thread::sleep(std::time::Duration::from_millis(400));
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        // The final job lands before shutdown returns.
        writer.shutdown(
            Some(FileJob::Write(Arc::new(b"final".to_vec()))),
            rt.handle(),
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"final");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `provider-cache.json` as 0.7.0 wrote it: every instant in UTC with "Z"
    /// and 0, 3, 6 or 9 fractional digits. It reads and is written back to
    /// the byte, so either version reads what the other wrote.
    const CACHE_FILE: &str = r#"[
  {
    "id": "claude",
    "status": "Ok",
    "metrics": [
      {
        "label": "Session",
        "used": 20,
        "limit": 100,
        "unit": "Percent",
        "reset_at": "2099-11-16T15:00:00Z",
        "window": "Session"
      },
      {
        "label": "Weekly",
        "used": 60,
        "limit": 100,
        "unit": "Percent",
        "reset_at": "2099-11-20T08:59:59.407Z",
        "window": "Long"
      },
      {
        "label": "Fable",
        "used": 5,
        "limit": 100,
        "unit": "Percent",
        "reset_at": "2099-11-20T09:00:00.123456Z",
        "window": "Long",
        "extra_only": true
      }
    ],
    "updated_at": "2026-11-21T07:58:01.123456700Z"
  },
  {
    "id": "codex",
    "status": "Ok",
    "metrics": [
      {
        "label": "Session",
        "used": 7,
        "limit": 100,
        "unit": "Percent",
        "reset_at": "2100-01-01T00:00:00.000000001Z",
        "window": "Session"
      },
      {
        "label": "Weekly",
        "used": 9,
        "limit": 100,
        "unit": "Percent",
        "reset_at": "2300-01-01T00:00:30.500Z",
        "window": "Long"
      }
    ],
    "updated_at": "2026-11-21T07:58:00Z"
  }
]"#;

    fn cache_file() -> HashMap<ProviderId, ProviderData> {
        serde_json::from_str::<Vec<ProviderData>>(CACHE_FILE)
            .unwrap()
            .into_iter()
            .map(|d| (d.id.clone(), d))
            .collect()
    }

    #[test]
    fn the_cache_file_reads_and_is_written_as_before() {
        let bytes = cache_file_bytes(&cache_file()).unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), CACHE_FILE);
    }

    /// Resets count to the minute, rounded down; one past 2262 (out of the
    /// i64-nanosecond range) is compared as it is.
    #[test]
    fn the_cache_meaning_holds_resets_to_the_minute() {
        assert_eq!(
            cache_meaning(&cache_file()),
            concat!(
                r#"[[0,[{"label":"Session","used":20,"limit":100,"unit":"Percent","#,
                r#""reset_at":"2099-11-16T15:00:00Z","window":"Session"},"#,
                r#"{"label":"Weekly","used":60,"limit":100,"unit":"Percent","#,
                r#""reset_at":"2099-11-20T08:59:00Z","window":"Long"},"#,
                r#"{"label":"Fable","used":5,"limit":100,"unit":"Percent","#,
                r#""reset_at":"2099-11-20T09:00:00Z","window":"Long","extra_only":true}]],"#,
                r#"[1,[{"label":"Session","used":7,"limit":100,"unit":"Percent","#,
                r#""reset_at":"2100-01-01T00:00:00Z","window":"Session"},"#,
                r#"{"label":"Weekly","used":9,"limit":100,"unit":"Percent","#,
                r#""reset_at":"2300-01-01T00:00:30.500Z","window":"Long"}]]]"#,
            )
        );
    }
}
