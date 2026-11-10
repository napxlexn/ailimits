# ARCHITECTURE.md — how the widget is built

A single-threaded **tao event loop** plus a small **tokio runtime**
(2 worker threads) for network fetches. The UI is rendered on the CPU
(tiny-skia → softbuffer), so the app needs no GPU at all.

```
main.rs ── single-instance guard, logging, app::run()
   │
app.rs ─── owns everything: config, display data, window, menu
   ├── monitor/scheduler.rs   tokio task: fetch_all → sleep(interval) loop
   │     └── providers/       claude.rs / codex.rs / copilot.rs / antigravity.rs /
   │                          grok.rs / cursor.rs
   ├── ui/renderer.rs         tiny-skia drawing (the ONLY place that draws)
   ├── ui/layout.rs           zone rectangles per layout/detail level
   ├── ui/theme.rs            all colors (the ONLY source of colors)
   ├── ui/context_menu.rs     native muda menu
   ├── ui/tray.rs             tray icon, shared glyph + tooltip painting
   ├── ui/taskbar_panel.rs    taskbar overlay indicator (layered window)
   ├── monitor/trend.rs       burn-rate projection (the optional forecast)
   ├── platform/win.rs        Win32 glue: taskbar/tray WinEvent watch,
   │                          layered presents, tray-icon promotion
   ├── updater.rs             daily GitHub Releases poll, SHA-256 verify,
   │                          silent install + relaunch (menu: Automatic updates)
   ├── platform/taskbar_geom.rs  pure placement/fallback rules for the panel
   │                          (testable, no Win32 calls)
   ├── platform/sqlite.rs     read-only lookups in another app's SQLite
   │                          database (Cursor's state.vscdb) via winsqlite3
   ├── config/ (schema.rs, storage.rs)  config structs + tolerant TOML load/save
   └── notifications/toast.rs Windows toasts
```

## Data flow

```
[Scheduler] --sleep(SharedInterval)--> [Provider::fetch()]
        each provider in its own tokio task
                  │ ProviderData
        tokio mpsc → EventLoopProxy(UserEvent::Command)
                  │
   [event loop: UpdateProvider handler]
   ├─ toast check (threshold + cooldown)
   ├─ provider-cache merge / persist (unexpired metrics survive restarts)
   ├─ stale-data retention (errors never wipe real data)
   └─ visible_data() → layout resize if a row appeared/disappeared → redraw
```

Menu clicks travel the same way: `muda::MenuEvent` → proxy →
`UserEvent::Menu` → handlers mutate the config, rebuild providers
(`SharedProviders` is an `Arc<RwLock<Vec<Arc<dyn Provider>>>>` shared with
the scheduler), save the config, and request a redraw.

## Key decisions

- **softbuffer + tiny-skia, not DirectX/OpenGL** — for bars and text a GPU
  is pure overhead; the framebuffer copy is trivially cheap, idle CPU is 0%.
- **Blur via `SetWindowCompositionAttribute` ACCENT_ENABLE_BLURBEHIND (3)** —
  acrylic (4) has a known DWM bug (drops blur on drag/monitor change).
  The accent is re-applied off→on after drag end, on focus, and on DPI
  change, which forces DWM to restart a stuck blur.
- **Drag starts on the first CursorMoved**, not on the click: the cached
  cursor position can be stale right after the context menu, which used to
  teleport the window.
- **2 tokio workers** — the default one-per-core (32 here) wasted ~30
  threads for three tiny HTTP requests per cycle.
- **Single instance** via a named mutex in `main.rs`.
- **SQLite through Windows' own `winsqlite3.dll`, not a bundled engine** —
  Cursor keeps its session in a SQLite database, and one indexed SELECT a
  minute does not justify compiling SQLite in: `rusqlite` with `bundled`
  adds about 1 MB (+29%) to the `release-min` exe, the FFI reader together
  with the whole provider 13 KB (measured 2026-11-10). The DLL ships with
  Windows 10 and 11 and is loaded from System32 only
  (`LOAD_LIBRARY_SEARCH_SYSTEM32`) on the first lookup, then kept. It is not
  an import, so a machine without it gets an honest error instead of an exe
  that will not start. Connections are `SQLITE_OPEN_READONLY` with a 200 ms
  busy timeout. Measured: ~0.6 ms per lookup; +0.2 MB private memory after
  the first one, levelling off under +0.5 MB over 21,000 lookups.
- **Honest display states** (see PROVIDERS.md): retention → grey + age →
  `≈0%` estimates after a provable reset. One predicate,
  `ProviderData::is_aged` (stale or estimated), drives the widget's grey
  rows and the taskbar panel's dimmer ink; the panel's redraw cache holds it
  per painted row, so a row repaints when it turns stale or fresh and at no
  other extra moment.
- **Monotonic staleness anchor**: whether live data is "stale" is decided on a
  monotonic clock (`ProviderData.received_at: Instant`), not the wall clock, so
  an NTP/VM/DST time jump can't falsely grey fresh data or fabricate a reset.
  The wall clock still drives the displayed age and countdowns; the `≈0%`
  estimate also requires the reset to be past by a small grace window.
- **The idle pause does not age data**: the scheduler publishes when it
  pauses polling for an idle/locked user and when it resumes
  (`providers::set_polling_paused` → `PollingPause`). Live data is never
  stale while paused, and afterwards the most recent pause is subtracted
  from its monotonic age. Only that one pause is remembered: a fetch cycle
  always runs between two pauses, so live data older than an earlier pause
  belongs to a provider that failed, and greying it sooner is honest. Disk
  cache entries (no monotonic anchor) keep the wall-clock rule.

## Resource footprint (measured, not estimated)

Audited 2026-08-23 with `bench/perf_audit.ps1` (5 min at 1 Hz, running
`release-min`, Win11 26200, Ryzen 9 9950X3D / 32 threads, elevated). CPU is
exact kernel accounting, not sampling. Report:
`bench/results/perf-audit-published-2026-08-23.json`. The 2026-07-08 run, kept
beside it as `perf-audit-published-2026-07-08.json`, read 0.024% of one core
and 61 MB — the same window on the same machine, five releases earlier.

Re-measured 2026-09-23 for 0.7.0, 255 s on a quiet machine (a video playing,
nothing else; unelevated, so the timer-resolution and SRUM rows are carried
from the August run). The panel had asked the shell for the bar's room at
least once, which is the state it is in whenever it is on screen.

| Metric | Measured (0.7.0) | 0.6.3 (2026-08-23) |
|---|---|---|
| CPU (exact accounting) | 0.031 CPU-s per 255 s = **0.012% of one core** (0.0004% of the machine) | 0.005% of one core |
| GPU | **0%** — the process owns no GPU-engine counter instances at all (CPU rendering) | 0% |
| Memory | 45 MB private / 67 MB working set | 37 MB working set |
| Threads | ~13 avg / 16 peak | ~10 avg / 13 peak |
| Handles | ~360 | ~377 |
| I/O | 184 B/s avg; spikes are the update-cycle HTTPS (3 requests / cycle, < 5 KB each) |
| Context switches (idle-wakeup proxy) | ~0.9/s whole-window average |
| Platform timer resolution | **not raised** (`powercfg /energy`: no request from ailimits) |
| SRUM energy estimate | ~160 units/min avg — CPU + network only; display/disk/GPU all zero (carried from the 2026-07-08 report; the August run stores raw hourly buckets, not this normalisation) |
| DPC/latency impact | none possible: user-mode, no drivers, no timer-resolution requests |
| Binary (`release-min`) | ~3 MB |