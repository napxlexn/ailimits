# ARCHITECTURE.md — how the widget is built

A single-threaded **tao event loop** plus a small **tokio runtime**
(2 worker threads) for network fetches. The UI is rendered on the CPU
(tiny-skia → softbuffer), so the app needs no GPU at all.

```
main.rs ── single-instance guard, logging, app::run()
   │
app.rs ─── owns everything: config, display data, window, menu, picker
   ├── monitor/scheduler.rs   tokio task: fetch_all → sleep(interval) loop;
   │                          a CycleHook runs after each cycle's fetches
   │     └── providers/       claude.rs / codex.rs / copilot.rs / antigravity.rs /
   │                          grok.rs / cursor.rs (+ cursor/session.rs: the
   │                          widget's own Cursor sign-in and session)
   ├── rows.rs                which rows the widget and the tooltip show:
   │                          SUB_ROWS (extra rows), the model label in parts,
   │                          the width limit for model names (NAME_SLOTS)
   ├── choices.rs             what the picker shows, and what each of its
   │                          actions does to the config (pure: no window, no files)
   ├── consent.rs             the files and hosts each read question names
   │                          (READ_INFO); found_tools only checks folders exist
   ├── models/                model labels: settings.rs (each tool's settings
   │                          file), session.rs (the newest session's tail),
   │                          files.rs (capped reads, change stamps), name.rs
   │                          (the generic name rule, no table of names)
   ├── ui/picker.rs           the "Rows and models" window: Picker (a pure state
   │                          machine) + PickerWindow (a thin tao window)
   ├── ui/renderer.rs         tiny-skia drawing (the ONLY place that draws)
   ├── ui/layout.rs           zone rectangles per layout/detail level, and the
   │                          Shape (rows + model lines) a re-layout follows
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
   ├── config/ (schema.rs, storage.rs)  config structs + tolerant TOML load/save,
   │                          the [reads] migration
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
   └─ visible_data() → rows::visible_rows() → layout::Shape;
      re-layout only when the Shape changed → redraw
```

Menu clicks travel the same way: `muda::MenuEvent` → proxy →
`UserEvent::Menu` → handlers mutate the config, rebuild providers
(`SharedProviders` is an `Arc<RwLock<Vec<Arc<dyn Provider>>>>` shared with
the scheduler), save the config, and request a redraw.

The model labels are read on the same cycle:

```
[Scheduler] fetch_all → CycleHook → spawn_model_read
        ModelJob { reader, request: (generation, ModelRequest) }
                  │ spawn_blocking: ModelReader::read — only what [reads]
                  │ allows, a file again only if its mtime or length changed
        tokio mpsc → AppCommand::Models(generation, Models)
                  │
   [event loop] older generation → dropped; else layout::Shape → redraw
```

`model_request` is `None` while no label part is ticked or no model read is
allowed, and then the hook does nothing. Every change of what is asked
(`refresh_models`) bumps the generation, so a read that started before it
never lands. The request and the reader have separate locks: the event loop
only ever takes the request, for a moment, and a slow disk holds only the
blocking thread.

The picker is a second window that exists only while open. Its window events
are matched by `WindowId` ahead of the widget's arms and go to
`PickerWindow::handle_window_event`, which feeds the pure `Picker` and returns
`PickerAction`s. `choices::apply` turns each into a config change plus an
`Applied` (rebuild and fetch a provider, re-read the models, save and
re-lay, close); app.rs then updates the widget, the panel, the tray and the
menu at once and hands the picker a fresh `PickerInput`. The picker never
touches a file. It opens from the menu (*Rows and models...*), from a click
on the widget's "Choose what to show", by itself at start while
`[reads] pending` owes the welcome or the update notice, and from the
Providers menu when a provider never allowed is ticked (on that provider's
question).

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
- **Consent is one config table read in two places.** `Config::provider_active`
  (switched on AND allowed by `[reads] sign_in` or `app_sign_in`) decides
  whether a provider runs or shows at all; a provider allowed nothing is
  built switched off and returns `NotConfigured` before it touches a file,
  the keyring, `gh` or the network. `build_providers` also sets
  `ProviderConfig::tool_files` from `sign_in` alone (never saved), and each
  provider checks it before any source that belongs to the other tool: its
  token file, its Credential Manager entry, `gh auth token`, Cursor's
  `state.vscdb`. A provider set up only with the app's own sign-in (a pasted
  key or usage token, the menu's Cursor sign-in) uses that and never falls back to the
  tool's files. Unticking every row of a provider in the picker calls
  `Config::revoke`. The text of each question comes from `consent::READ_INFO`,
  and a test holds its hosts to the URLs in the provider sources.
- **Where a config came from decides what is allowed** (`storage::Origin`,
  settled by `settle_reads` after `migrate`'s rename and append-missing
  passes). `FreshInstall` (no file, an unreadable one set aside as
  `config.toml.corrupt`, an empty or providerless one) clears `[reads]`,
  switches every provider off, so a downgraded version reads nothing either,
  and owes the welcome. `Upgrade` (`[[providers]]` without `[reads]`: 0.7.0
  and older) allows every provider the file has switched on and owes the
  one-time notice. The append pass adds a missing provider switched off on
  any origin: the version that wrote the file did not know it, so never
  read it. `Current` is left exactly as the user left it.
- **The default look is pinned.** With `[ui.rows]` and `[ui.model]` unset,
  `rows::visible_rows` yields one row per provider carrying its data
  untouched, and the panel's tooltip is the old single line. A test-only
  module (`ui/visual_baseline.rs`) holds the widget, the panel, the tray icon
  and the hover tooltip, rendered with the default config, to pixmap hashes
  recorded from 0.7.0, and fails on any changed pixel.
- **Model names are held to real widths, not a character count.**
  `rows::NAME_SLOTS` lists the narrowest places the renderer draws a model
  name (width and font size); `name_fits` measures with
  `Renderer::text_width`, the same measure `draw_text` uses, so a renamed
  model shows in full at every size. A renderer test holds the list to the
  geometry.
- **The picker is a pure state machine behind a thin window.** `Picker` takes
  `PickerEvent`s and returns `PickerAction`s plus a `PickerScene` of shapes,
  all unit-tested without a window; `PickerWindow` presents it through
  softbuffer like the widget, repaints only on input and has no timer. It
  reads the clipboard only on Ctrl+V into a name field being edited.

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