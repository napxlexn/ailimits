# ARCHITECTURE.md — how the widget is built

A single-threaded **tao event loop** plus a small **tokio runtime**
(2 worker threads) for network fetches. The UI is rendered on the CPU
(tiny-skia → softbuffer), so the app needs no GPU at all.

```
main.rs ── single-instance guard, logging, app::run()
   │
app.rs ─── owns everything: config, display data, window, menu
   ├── monitor/scheduler.rs   tokio task: fetch_all → sleep(interval) loop
   │     └── providers/       claude.rs / codex.rs / copilot.rs / antigravity.rs
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
- **Honest display states** (see PROVIDERS.md): retention → grey + age →
  `≈0%` estimates after a provable reset.
- **Monotonic staleness anchor**: whether live data is "stale" is decided on a
  monotonic clock (`ProviderData.received_at: Instant`), not the wall clock, so
  an NTP/VM/DST time jump can't falsely grey fresh data or fabricate a reset.
  The wall clock still drives the displayed age and countdowns; the `≈0%`
  estimate also requires the reset to be past by a small grace window.

## Resource footprint (measured, not estimated)

Audited 2026-07-08 with `bench/perf_audit.ps1` (5 min at 1 Hz, running
`release-min`, Win11 26200, Ryzen 9 9950X3D / 32 threads, elevated). CPU is
exact kernel accounting, not sampling. Report:
`bench/results/perf-audit-published-2026-07-08.json`.

| Metric | Measured |
|---|---|
| CPU (exact accounting) | 0.078 CPU-s per 326 s = **0.024% of one core** (0.0007% of the machine) |
| GPU | **0%** — the process owns no GPU-engine counter instances at all (CPU rendering) |
| Working set | 61 MB avg / 69 MB peak |
| Threads | ~9 avg / 15 peak |
| Handles | ~300 |
| I/O | 136 B/s avg; spikes are the update-cycle HTTPS (3 requests / cycle, < 5 KB each) |
| Context switches (idle-wakeup proxy) | ~8.5/s whole-window average |
| Platform timer resolution | **not raised** (`powercfg /energy`: no request from ailimits) |
| SRUM energy estimate | ~160 units/min avg — CPU + network only; display/disk/GPU all zero |
| DPC/latency impact | none possible: user-mode, no drivers, no timer-resolution requests |
| Binary (`release-min`) | ~3 MB |