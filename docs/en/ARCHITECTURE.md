# ARCHITECTURE.md — how the widget is built

A single-threaded **tao event loop** plus a small **tokio runtime**
(2 worker threads) for network fetches. The UI is rendered on the CPU
(tiny-skia → softbuffer), so the app needs no GPU at all.

```
main.rs ── single-instance guard (a second start shows the running widget),
   │       logging, the crash log, app::run()
app.rs ─── App: owns everything (config, display data, window, menu, picker);
   │       run() builds it, the event loop is one App::handle() call
   ├── app/events.rs          the event loop's arms: provider updates, windows,
   │                          timers, shutdown
   ├── app/menu.rs            menu actions: the look, the toggles, the
   │                          indicator, the interval, opening the picker
   ├── app/sign_in.rs         each provider's Sign-in in the picker: the source
   │                          chosen, a key or usage token pasted (a usage
   │                          token checked first) or removed, Cursor's
   │                          sign-in; results said in the window (a toast
   │                          while it is closed); settle_sources_once at
   │                          start (only a config from before the choice)
   ├── app/menu_preview.rs    the look item under the pointer shown on the
   │                          widget, the panel and the tray before a click;
   │                          back as it was when the menu closes without one
   ├── app/flow.rs            re-layout, tray and panel sync, opening the
   │                          picker (also quietly, for the welcome), its actions
   ├── app/provider_set.rs    builds the providers; rebuilds one only when its
   │                          BuildKey (config, tool_files, Grok Bot) changed
   ├── app/cache.rs           provider-cache.json: one writer, fewer writes
   ├── app/model_job.rs       what the model labels read; one reader at a time
   ├── app/config_watch.rs    config.toml edited by hand: looked at by its stamp
   │                          (cycle, actions, 2 s after Edit config.toml…),
   │                          merged with the app's changes, applied part by part
   ├── app/clipboard.rs       the shape check of a pasted key or usage token
   ├── app/widget.rs, app/indicator.rs  the overlay window; the fallback tray icon
   ├── monitor/scheduler.rs   tokio task: fetch_all → sleep(interval) loop;
   │                          a CycleHook runs after each cycle's fetches
   │     └── providers/       claude.rs / codex.rs / copilot.rs / antigravity.rs /
   │                          grok.rs / cursor.rs (+ cursor/session.rs: the
   │                          widget's own Cursor sign-in and session);
   │                          proxy.rs (the Windows proxy), mod.rs (shared
   │                          request helpers, error words, pasted-token checks)
   ├── rows.rs                which rows the widget and the tooltip show:
   │                          SUB_ROWS (extra rows), the model label in parts,
   │                          the width limit for model names (NAME_SLOTS)
   ├── choices.rs             what the picker shows, and what each of its
   │                          actions does to the config (pure: no window, no files)
   ├── consent.rs             the files and hosts each read question names
   │                          (READ_INFO); found_tools only checks folders exist
   ├── config/paths.rs        where every other tool keeps its files (ToolPaths):
   │                          the one resolver for providers, labels, consent, CLI
   ├── models/                model labels: settings.rs (each tool's settings
   │                          file), session.rs (the newest session's tail),
   │                          files.rs (capped reads, change stamps), name.rs
   │                          (the generic name rule, no table of names),
   │                          catalog.rs (the models each tool offers)
   ├── ui/picker/             the "AI Limits Settings" window: mod.rs (Picker, a
   │                          pure state machine), text.rs, layout.rs, scene.rs,
   │                          input.rs, window.rs (PickerWindow, a thin tao window)
   ├── ui/renderer.rs         tiny-skia drawing of the widget and the picker
   ├── ui/layout.rs           zone rectangles per layout/detail level, and the
   │                          Shape (rows + model lines) a re-layout follows
   ├── ui/theme.rs            all colors (the ONLY source of colors), the
   │                          picker's levels, the tray's and the panel's inks
   ├── ui/context_menu.rs     native muda menu
   ├── ui/window.rs           the widget window's drag (it starts past 4 px),
   │                          the Shift magnet to the edges, the work-area clamp
   ├── ui/tray.rs             tray icon, shared glyph + tooltip painting
   ├── ui/taskbar_panel.rs    taskbar overlay indicator (layered window)
   ├── ui/visual_baseline.rs  (tests only) the pinned look of every surface
   ├── monitor/trend.rs       burn-rate projection (the optional forecast)
   ├── platform/win.rs        Win32 glue: taskbar/tray WinEvent watch,
   │                          layered presents, tray-icon promotion, the
   │                          fullscreen check, waking the running instance
   ├── platform/win_uia.rs    where the taskbar's app buttons end and the tray
   │                          begins, asked of UI Automation (no pixel scan)
   ├── updater.rs             daily GitHub Releases poll (installer copies only),
   │                          SHA-256 + Ed25519 signature check, silent install
   │                          (menu: Automatic updates)
   ├── updater/signature.rs   the release keys and the signature format, shared
   │                          as is with tools/release-signer
   ├── hooks.rs               the user's commands on usage events (cmd.exe /D /S /C)
   ├── utc.rs                 instants in UTC (jiff, no time zones) and the
   │                          exact text they are read from and written in
   ├── platform/taskbar_geom.rs  pure placement/fallback rules for the panel
   │                          (testable, no Win32 calls)
   ├── platform/menu_hover.rs what the context menu highlights (WM_MENUSELECT
   │                          through a subclass of the menu's owner) and
   │                          whether it closed with a click; no hook, no timer
   ├── platform/sqlite.rs     read-only lookups in another app's SQLite
   │                          database (Cursor's state.vscdb) via winsqlite3
   ├── config/ (schema.rs, storage.rs)  config structs + tolerant TOML load/save,
   │                          the [reads] migration
   ├── config/tune.rs         [tune]: config-only knobs (sizes, colours,
   │                          thresholds); defaults are today's look, values
   │                          clamped where read; KEY_DOCS, a line per key
   ├── config/annotate.rs     config.toml as written: a note before each table
   │                          and key, the [tune] keys not set commented out
   └── notifications/         toast.rs (Windows toasts), alerts.rs (one alert
                              per limit window, kept in alerts.json)
```

`bin/ailimits-auth.rs` is the console tool `ailimits-auth.exe` (status;
storing and removing keys and usage tokens; Cursor's login and logout). It
uses the library's providers and config but not the widget's event loop,
and it never calls `follow_windows_proxy`, so it follows only the
proxy variables (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`).
`tools/release-signer` (a crate of its own, ring its one dependency) makes
the release key and makes and checks release signatures; it includes the
app's `updater/signature.rs` (the keys, the signed message, the file
format) as is, so the two never disagree, and it is never built into the
app.

## Data flow

```
[Scheduler] --sleep(SharedInterval)--> [Provider::fetch()]
        each provider in its own tokio task
                  │ ProviderData
        tokio mpsc → AppCommand::UpdateProvider { data, serial }
                  │
   [event loop: UpdateProvider handler]
   ├─ serial no longer the provider's? → dropped (not shown, cached or alerted)
   ├─ threshold alert + on_threshold hook (once per limit window, alerts.json)
   ├─ on_reset hook (a drop of over 30 points from at or above the threshold)
   ├─ burn-rate trend and forecast (shown while Usage forecast is on)
   ├─ on_startup hook (once a run, on the first Ok answer)
   ├─ provider-cache merge (unexpired metrics survive restarts)
   ├─ stale-data retention (errors never wipe real data)
   └─ active_data() → rows::visible_rows() → layout::Shape;
      re-layout only when the Shape changed → redraw
```

`SharedProviders` is an `Arc<RwLock<Vec<ProviderSlot>>>` shared with the
scheduler, where `ProviderSlot { serial, provider }`. Menu clicks travel the
same way: `muda::MenuEvent` → proxy → `UserEvent::Menu` → handlers mutate the
config, save it, and request a redraw. The picker's actions (below) rebuild
the providers whose `BuildKey` changed (or, for a change of account: a
Cursor sign-in or sign-out, a key or usage token pasted or removed, another
source chosen in a Sign-in, a provider switched on or off by its rows, a
consent given or taken back, the one provider concerned, with a new serial)
and fetch each rebuilt one at once. When its source or account changed, a
rebuilt provider's cached numbers, last error, trend and alert marks are
cleared, and its row reads Loading until the first answer; a rebuild for
Grok Bot alone keeps them. The other providers keep their instances, and
with them what they keep between cycles: gh's answer, Cursor's plan and
refusal count, Antigravity's project; a rebuilt instance starts those over.

`surface_rows` takes the active data as it came; `rows::visible_rows` ages
each row on its own (a provider's own row by its own windows, an extra row
by its own metric), so a reset of an extra-only metric (Claude's Fable week,
say) never greys the provider's own row. The tray and the panel take
`visible_data` = the active data aged for display, and once the picker's
Taskbar column was touched (`ui.rows.taskbar` set), only the providers
ticked there (`rows::taskbar_ticked`, at most two).

The model labels are read on the same cycle:

```
[Scheduler] fetch_all → CycleHook → spawn_model_read
        ModelJob { reader, request: (generation, ModelRequest) }, ReadGate
                  │ spawn_blocking: ModelReader::read — only what [reads]
                  │ allows, a file again only if its mtime or length changed
                  │ (or its last read failed)
        tokio mpsc → AppCommand::Models(generation, Models)
                  │
   [event loop] older generation → dropped; else layout::Shape → redraw
```

`model_request` is `None` while no label part is ticked or no model read is
allowed, and then the hook does nothing; otherwise it names every provider
that runs (`Config::provider_active`). Every change of what is asked
(`refresh_models`) bumps the generation, so a read that started before it
never lands, and a read taken back reads nothing. `ReadGate` allows one
reader at a time: a request that comes while a read runs only marks that one
more is wanted, and the running worker reads again with the request as it is
by then, so a slow disk never piles up blocking threads. The request and the
reader have separate locks: the event loop only ever takes the request, for a
moment.

The picker is a second window that exists only while open. Its window events
are matched by `WindowId` ahead of the widget's arms and go to
`PickerWindow::handle_window_event`, which feeds the pure `Picker` and returns
`PickerAction`s. `choices::apply` turns each into a config change plus an
`Applied` (rebuild and fetch a provider, re-read the models, save and
re-lay, close); the app then updates the widget, the panel, the tray and the
menu at once and hands the picker a fresh `PickerInput`. The Sign-in
block's actions (choose a source, paste, remove, Cursor's sign-in, cancel,
sign-out) go to `app/sign_in.rs` instead, which reaches the clipboard,
Credential Manager and the browser and puts its result in the
`PickerInput` (`choices::SignInState`). The picker never touches a file.
It opens from the menu (*Settings…*), from a click on the widget's
"Setup required" or "No data yet", and by itself at start
while `[reads] pending` owes the welcome or the update notice. Opened by
itself it opens quietly
(`PickerWindow::open_quietly`, no activation), and waits while a fullscreen
window fills its monitor (`platform::foreground_fills_its_monitor`); it
opens at the next change of the foreground window, the next provider
update, or the next start of the app (a second start wakes the running one
with a message to its hidden tooltip window, `UserEvent::ShowWidget`).
It is a program's window: in the taskbar and Alt+Tab (its icon drawn from
`assets/icon-48.rgba` with `CreateIcon`, as the tray icon is, so no
`LoadImageW`), on top only while it is the active window and behind others
once another one is. Only Esc, its cross and Alt+F4 close it
(`PickerEvent::CloseRequested`); a click beside the box and the focus
going elsewhere leave it open. A press on its title starts Windows' own
move (`Window::drag_window`); where the user left it is kept in
`App::picker_at` and used at the next opening while the app runs, held
inside that monitor's work area (`hold_in`), and the work area and the
tallest box are found again for the monitor it was moved to
(`PickerWindow::find_work`).
Where it first opens keeps clear of the widget's growth: the app lays the
widget out with every row the window offers ticked (`App::widget_grown`),
and `Anchor::widget` turns that into the room it may take, held in the
work area as the widget holds itself (columns near the right edge grow to
the left, rows near the bottom upwards), and the side it grows to.
`place` puts the box beside that room, then below or above it, the roomier
side of each pair first (away from the nearer edge of the screen), and the
side it grows to only when no other has room.

## Key decisions

- **softbuffer + tiny-skia, not DirectX/OpenGL** — for bars and text a GPU
  is pure overhead; the framebuffer copy is trivially cheap (0.012% of one
  core at idle for 0.7.0, see Resource footprint).
- **Blur via `SetWindowCompositionAttribute` ACCENT_ENABLE_BLURBEHIND (3)** —
  acrylic (4) has a known DWM bug (drops blur on drag/monitor change).
  The accent is re-applied off→on after drag end, on focus, and on DPI
  change, which forces DWM to restart a stuck blur.
- **Drag starts on the first CursorMoved past 4 px**, not on the click: the
  cached cursor position can be stale right after the context menu, which
  used to teleport the window, and a click whose pointer wobbled stays a
  click.
- **2 tokio workers** — the default one-per-core (32 here) wasted ~30
  threads for a handful of tiny HTTP requests per cycle.
- **Single instance** via a named mutex in `main.rs`. A second start finds
  the first one's hidden tooltip window by class and posts it a message: the
  running widget is shown and raised without activation, and a welcome or
  update notice that was waiting opens.
- **One usage request per provider per cycle.** The helper requests
  (Cursor's plan hourly, ten minutes after one that brought none, Grok Bot
  only while its row is chosen, the session refresh near its expiry and
  never after it, Antigravity's project until an answer names it, the
  per-model fallback only after an unusable summary, a refused usage token
  paused 30 minutes, gh at most every 15 minutes and once more right after
  a kept token is refused) are listed in PROVIDERS.md, "Requests per cycle".
- **The Windows proxy without the registry in the CLI.** reqwest's
  `system-proxy` feature would add registry imports and a crate to
  `ailimits-auth.exe`, so `providers/proxy.rs` reads the three Internet
  Settings values with calls the widget's exe already makes, and only once
  `app::run` calls `follow_windows_proxy()`; code the CLI never calls is never
  linked into it. `HTTPS_PROXY`, `HTTP_PROXY` and `ALL_PROXY` still win; PAC
  is not followed.
- **No new DLLs, on purpose.** Trusting corporate root certificates
  (rustls-native-certs, a platform verifier or native-tls) would add crypt32
  or schannel; a UI Automation provider for screen readers would add
  `uiautomationcore.dll` and window subclassing. Both are left out to keep
  the exe's import table as it is (the least antivirus risk); such networks
  read "untrusted certificate, retrying", and the picker's window title
  names its mode for Narrator.
- **tao held at 0.37.0, with one fix patched in.** `Cargo.toml` pins
  `tao = "=0.37.0"`: from 0.37.1 on, `set_visible(true)` on a window built
  with `with_focused(false)` takes the focus (the picker's `open_quietly`
  would take the keyboard), and 0.37.3 adds the import
  `imm32!ImmSetCandidateWindow`. `[patch.crates-io]` builds it from
  `vendor/tao`: 0.37.0 as published plus the fix of tao#1394 (an event sent
  through `EventLoopProxy` could freeze the event loop). See
  `vendor/README.md`.
- **SQLite through Windows' own `winsqlite3.dll`, not a bundled engine** —
  Cursor keeps the model chosen for its Agent in a SQLite database, and one
  indexed SELECT a minute does not justify compiling SQLite in: `rusqlite`
  with `bundled` adds about 1 MB (+29%) to the `release-min` exe, the FFI
  reader with the first Cursor code 13 KB (measured 2026-11-10). The DLL
  ships with Windows 10 and 11 and is loaded from System32 only
  (`LOAD_LIBRARY_SEARCH_SYSTEM32`) on the first lookup, then kept. It is not
  an import, so a machine without it gets an honest error instead of an exe
  that will not start. Nothing is ever written to or created in the other
  app's folder: a WAL database whose owner is closed (no `-wal`) is opened as
  `file:...?immutable=1` (at a path with no such form, a UNC one, it is not
  opened at all, `SqliteError::NotQuietly`); one whose owner runs, and a
  rollback-journal one, `SQLITE_OPEN_READONLY`, with a 200 ms busy
  timeout. Measured: ~0.6 ms per
  lookup; +0.2 MB private memory after the first one, levelling off under
  +0.5 MB over 21,000 lookups.
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
- **Backoff, and one early retry after a wake.** A cycle in which every
  provider failed doubles the wait (120, 240, then 300 s at a 60 s
  interval). After a machine sleep (the wall clock passed the interval by
  90 s or more) or an idle pause, a first cycle that got no answer at all is
  retried once after 20 s, while the network comes back.
- **Consent is one config table read in two places.** `Config::provider_active`
  (switched on AND allowed by `[reads] sign_in` or `app_sign_in`) decides
  whether a provider runs or shows at all; a provider allowed nothing is
  built switched off and returns `NotConfigured` before it touches a file,
  the keyring, `gh` or the network. `provider_set` also sets
  `ProviderConfig::tool_files` from `sign_in` alone (never saved), and each
  provider checks it before any source that belongs to the other tool: its
  token file, its Credential Manager entry, `gh auth token`. Within what is
  allowed, `ProviderConfig::auth_method` (`AuthMethod::{Subscription,
  ApiKey, UsageToken}`, the source chosen in the provider's Sign-in) picks
  the one source the provider uses: a pasted key or usage token is never
  backed by the tool's sign-in, nor the tool's sign-in by a stored token. A
  provider set up only with the app's own sign-in (a pasted key or usage
  token, Cursor's own sign-in) never reads the tool's sign-in; its settings
  file is still read for the model label under `[reads] tool_settings`, and
  its newest session under `last_session`, as for every provider that
  runs. Unticking every
  row of a provider in the picker calls `Config::revoke`. The text of each
  question comes from `consent::READ_INFO`; tests hold its hosts to the URLs
  in every file under `src/providers` (both ways: every host called is named,
  every host named is called), and every file name and Credential Manager
  target a provider reads to the text.
- **One resolver for other tools' paths** (`config/paths.rs`, `ToolPaths`).
  The providers, the model labels, the consent texts, the welcome's
  "detected" and the CLI ask it, so they cannot disagree.
  `CODEX_HOME`, `GROK_HOME` and `COPILOT_HOME` count only when absolute; a
  relative one makes that tool unknown (nothing read, nothing found), never
  the working directory and never the default folder. `tool_signs` tells a
  tool's own folder (strong: ticked in the welcome) from a related one
  (weak: GitHub CLI for Copilot, `.gemini` for Antigravity).
- **Where a config came from decides what is allowed** (`storage::Origin`,
  settled by `settle_reads` after `migrate`'s rename and append-missing
  passes). `FreshInstall` (no file, one that could not be understood and was
  set aside as `config.toml.<stamp>.corrupt`, an empty or providerless one)
  clears `[reads]`, switches every provider off, so a downgraded version
  reads nothing either, and owes the welcome. `Upgrade` (`[[providers]]`
  without `[reads]`: 0.7.0 and older) allows every provider the file has
  switched on and owes the one-time notice. The append pass adds a missing
  provider switched off on any origin: the version that wrote the file did
  not know it, so never read it. `Current` is left exactly as the user left
  it. A file that cannot be read (after about 2 s of retries) is never
  renamed: the app runs on an in-memory config that reads nothing, and the
  path is held, so neither the saver nor the final save at exit writes it
  this session (`storage::saving_held`). `storage::peek` reads the config
  without creating, renaming or writing anything, for `ailimits-auth`, and
  names the lines left out (`Peek::LeftOut`): its commands that change the
  config refuse such a file, which a save would rob of those lines.
- **Few writes of the app's own files.** `provider-cache.json` has one
  serialised writer (the newest content wins, a cycle's answers merge into
  one write); it is written when the values, limits or resets change (to
  the minute), otherwise at most every 10 minutes, and at exit. The alert
  book (`alerts.json`) is written only when it changed.
- **A crash leaves a trace, never data.** `panic = "abort"` in release-min,
  so `main.rs` installs a panic hook that appends one line to `crash.log`
  (UTC time, version, thread, `file:line:col`, dependency paths cut to
  `crate-x.y.z\src\...`) and never the panic's message, which can quote the
  text it was slicing. A start that fails writes `could not start: <error>`
  there and shows "AI Limits could not start." with the file's path. Past
  64 KB the file becomes `crash.log.1` (replacing the one before) and
  starts over.
- **The default look is pinned.** With `[ui.rows]` and `[ui.model]` unset,
  `rows::visible_rows` yields one row per provider carrying its data
  untouched. The panel's tooltip is the one default that changed: with
  `[ui.tooltip]` unset (Percentage and Time to reset both on) it gives each
  provider a line with its reset; Percentage alone gives 0.7.0's single line.
  See "Visual baseline" below.
- **Model names are held to real widths, not a character count.**
  `rows::NAME_SLOTS` lists the narrowest places the renderer draws a model
  name (width and font size); `name_fits` measures with
  `Renderer::text_width`, the same measure `draw_text` uses, so a renamed
  model shows in full at every size. A renderer test holds the list to the
  geometry. Names are kept by `choices::model_key`: the model's id, or
  `"provider:id"` for a tool's own router (`auto`, `default`), and the
  widget looks them up through `choices::model_name` as the picker does.
- **The picker is a pure state machine behind a thin window.** `Picker` takes
  `PickerEvent`s and returns `PickerAction`s plus a `PickerScene` of shapes,
  all unit-tested without a window; `PickerWindow` presents it through
  softbuffer like the widget, repaints only on input and has no timer. It
  reads the clipboard only on Ctrl+V or Shift+Insert into a name field
  being edited (a Sign-in's paste is read by the app, `app/clipboard.rs`).
  It
  follows the apps' light or dark mode (`AppsUseLightTheme`), high contrast
  (the system colors) and Windows' Text size; its question scrolls with the
  buttons always in view; a row's description shows on hover and on keyboard
  focus; while a name is typed, a shell panel (Win+V, Win+.) taking the focus
  does not close it.

## Updates and releases

- **The channel is decided from local facts only, before any network.**
  `updater::channel()` reads the package identity
  (`GetCurrentPackageFullName`) and the installer's own uninstall entry:
  `Store` and `Unmanaged` (Scoop, the portable zip, a self-built exe) return
  at once, so they never contact api.github.com, and the menu has no
  *Automatic updates* item for them (`self_update_supported`). Only an
  `Installer` copy goes on.
- **Check, wait, verify.** About 30 s after start, then daily, the updater
  asks the GitHub Releases API for the latest release (through the Windows
  proxy, as the providers do). Ticking *Automatic updates* checks about
  10 s later (`AutoUpdate`, `SWITCHED_ON_DELAY`; a tick undone meanwhile
  asks nothing), unless a check ran within the hour. The updater
  takes only an `AiLimits-Setup-<v>.exe` asset with its `.sig`. A newer one
  waits until the user has been away for 5 minutes (no input, or the
  session locked, `platform::user_idle_secs`), checked every minute
  (switching *Automatic updates* off meanwhile cancels it; no daily check
  runs while it waits, so a release published meanwhile comes with the
  next check after this install); then the
  installer is downloaded and checked twice: against the SHA-256 GitHub
  publishes for the asset, and against the Ed25519 signature by a key
  built into the app (`RELEASE_KEYS`, key id `1507bd92b1a4bb7b`). The
  signature covers the file name and its SHA-256, so a signed installer
  cannot pass for another version. No signature, or one that does not
  verify: no install. If the user came back during the download, it waits
  again. Every request, the download included, times out after 120 s, and
  every answer is read up to a cap (the release 1 MiB, the signature 1 KiB,
  the installer 64 MiB; it is about 4 MB). The API's URL is fixed and the
  download URLs are the answer's `browser_download_url` as given; a
  redirect after any of them is followed only to https on the default port
  at github.com, api.github.com or a `*.githubusercontent.com` asset host,
  five at most.
- **The signature file.** A `.sig` is one line,
  `ailimits-signature-v1 <key id> <signature>`: the key id in 16 hex
  digits, the first 8 bytes of the SHA-256 of the public key, then the
  Ed25519 signature in 128. What is signed is exactly the text
  `signature::signed_message` builds,
  `"AI Limits release signature v1\nfile <name>\nsha256 <hex>\n"`, where
  `<name>` is the file's published name, `<hex>` the SHA-256 of its bytes
  in lower-case hex, and `\n` a line feed. The key built in today
  (`RELEASE_KEYS`) is
  `8691886023426a4f882779d754feaf850e977d5e70710013c092e4aca02c0dd9`, id
  `1507bd92b1a4bb7b`. Anyone can check a download, no key needed:
  `cargo run --locked --release --manifest-path tools/release-signer/Cargo.toml -- verify <file>`,
  with its `.sig` beside it and the file under its published name.
- **A refused update is quiet.** A release without
  `AiLimits-Setup-<v>.exe`, its SHA-256 digest or its `.sig` is refused
  before anything is downloaded; a signature that does not read, before
  the installer is; an installer whose digest or signature does not match,
  after its download. Each refusal is one debug-level log line
  (`auto-update check skipped: ...`): nothing tells the user and no
  `update-attempt.json` is written, so the same check runs again at the
  next daily check and at every start. After a key rotation, a copy that
  trusts only the old key and missed the bridging release (signed with the
  old key, trusting both) is refused every time and has to be updated by
  hand.
- **Signatures protect updates from 0.7.1 on.** A 0.6.0-0.7.0 copy updates
  with its own updater (`git show v0.7.0:src/updater.rs`): it takes the
  first release asset whose name ends in `.exe`, checks only the SHA-256
  digest GitHub reports for it, never the `.sig`, and installs at once,
  without waiting for the user to be away, through
  `%TEMP%\ailimits-update.cmd`. So the update to 0.7.1 rests on that digest
  alone, and while such copies exist a release must carry exactly one
  `.exe`, the installer. `release.yml` attaches exactly one: the other exes
  travel inside the zip.
- **The file run is the file checked.** The verified installer is written
  into a folder made new for that download, `%TEMP%\AiLimits-update-<random>`
  (`create_new`), and held from its last check until it runs: opened with
  read sharing only, so nothing can write, rename or delete it, read back
  through that handle against the verified SHA-256, and kept open until
  the installer process has started. Its folder goes with it.
- **No script in between.** The attempt is written to
  `update-attempt.json` first (version, installer path, `told`), then the
  installer itself is started by `CreateProcessW` (its path as UTF-16, so a
  Cyrillic, `&` or `%` profile or TEMP path works like any other) with ASCII
  switches only: `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART
  /CLOSEAPPLICATIONS /RELAUNCH=1`. The app exits; the installer's `[Run]`
  entry with `Check: RelaunchAfterUpdate` starts the new version from the
  install folder, only in a silent run with `/RELAUNCH=1`, so winget and
  Chocolatey silent installs still start nothing. 0.6.0-0.7.0 handed over
  to a `.cmd` script in `%TEMP%`, which failed on such paths; 0.7.1 removes
  what that left (`ailimits-update.cmd`, `AiLimits-Setup-<running>.exe`).
- **A failed update is not retried by itself.** If the app still runs the
  older version after an attempt, that version is not tried again
  automatically (a newer release still is), the downloaded installer is
  deleted, and the next start shows one message
  (`updater::unfinished_update_note`, marked `told`).
- **The release pipeline** (`.github/workflows/release.yml`, on a `v*` tag):
  every action pinned to a commit SHA, checkout without a persisted token, no
  build cache, Inno Setup 6.7.4 pinned by SHA-256, and three jobs, each on a
  fresh machine. *build* (no secret, a read-only token) runs fmt, clippy and
  the tests, checks that the tag, `Cargo.toml` and `installer/ailimits.iss`
  agree, builds the exes with the build machine's paths rewritten to `~`
  (`USERPROFILE`) and `ailimits` (`GITHUB_WORKSPACE`) by
  `--remap-path-prefix`, added with `--config` so that `.cargo/config.toml`'s
  `+crt-static` stays (the `RUSTFLAGS` variables would replace it); the next
  step fails the release if an exe still names `VCRUNTIME140`,
  `api-ms-win-crt-` or a build path. It then builds the installer and the
  portable zip. A local build, or one by ci.yml, is not remapped. *sign* (the
  `release` environment, the only job with the key) fails at once without the
  secret, builds `tools/release-signer` from a clean checkout before the key
  is in reach, signs the files *build* handed over (artifact digests checked)
  and checks each signature against `RELEASE_KEYS`, so a wrong secret fails
  the release, not users' updates. None of the app's other dependencies ever
  runs where the key is. *publish* (the only write token) makes sure every
  file has its signature and creates a **draft** release with the CHANGELOG
  section of that version as its notes. The maintainer checks the files
  (VirusTotal, a quick install) and publishes it by hand: `gh release edit
  vX.Y.Z --draft=false`. A tag without a `## <version>` section in
  CHANGELOG.md gets GitHub's generated notes instead.
- **A run by hand** (`workflow_dispatch`) runs *build* alone, without the
  tag's version check: nothing is signed or published. Its files are the
  run's `release-files` artifact, kept 7 days, to check before the tag.
- **The Rust version.** There is no `rust-toolchain` file: CI and the
  release build with the stable Rust of the day, which decides the exes'
  import tables. The minimum, 1.90 (`rust-version` in Cargo.toml), is held
  by `cargo check` in ci.yml.
- **The signing key.** The private key is kept offline by the maintainer
  and in the secret `AILIMITS_RELEASE_SIGNING_KEY` of the `release`
  environment. Adding it, once, passes the key on stdin so it never appears
  on a command line:

  ```
  gh api -X PUT repos/napxlexn/ailimits/environments/release
  Get-Content -Raw <release-signing-key.hex> |
      gh secret set AILIMITS_RELEASE_SIGNING_KEY --env release --repo napxlexn/ailimits
  ```

  Back it up offline (an encrypted drive or a password manager): without it
  no release can be signed for 0.7.1 and later, and every user would have to
  update once by hand. A new key: `release-signer keygen` into a new
  folder, add its public key to `RELEASE_KEYS` beside the old one, ship a
  release signed with the OLD key, switch the secret, and drop the old key
  from `RELEASE_KEYS` a release later. Details in
  `tools/release-signer/src/main.rs`.

## Visual baseline

`ui/visual_baseline.rs` (tests only) renders the widget, the panel, the tray
icon and the hover tooltip with the default config and fixed data, and
compares a hash of each pixmap with `tests/visual/<group>.txt`.

- **Pinned groups** (`widget`, `panel`, `tray`) were recorded in 3e8aa80
  (2026-11-11), not rendered from the 0.7.0 tag. They hold that commit's
  look, which differs from 0.7.0 on purpose, as the owner approved: stale
  data greys before its reset (ac17bd2); the panel inks a stale or
  estimated row dimmer (a755fe3); data is stale once the update interval
  plus a minute has passed (9eada81); a provably passed reset reads as
  about 0 at once (387fc59); the hover over a stale row whose error passes
  by itself shows just that, where 0.7.0 added the provider's action
  ("offline, retrying — check gh auth"), advice that did not help (3d6093c);
  since 2026-11-23 a refused key reads "key rejected" and a network error
  too long for the row "offline", where 0.7.0 had "check the key" and
  "⚠ offline" (re-recorded on the owner's word).
  Rendered against the 0.7.0 tag with the same fixtures cut down to what
  0.7.0 can draw, 83 of 116 widget, 30 of 36 panel and all 84 tray entries
  were byte-identical; the rest differ only by ac17bd2, a755fe3 and 3d6093c
  (9eada81 and 387fc59 change rules, not drawing), and since 2026-11-23 the
  entries with those two status words. They change only on the
  owner's decision (`UPDATE_PINNED_BASELINE=1`); never re-record them to
  make a test pass.
- **Other groups** (`picker`, `widget-rows`, `tooltip-rows`,
  `panel-numbers`: the panel's percentage, reset time or both, with or
  without its bar, or the bar alone, each at the size the app gives it;
  `widget-numbers`: every mix of `[ui.widget]` but the default, in every
  layout, with models; `tray-styles`: the tray icon's other looks;
  `widget-tune`,
  `panel-tune`, `tray-tune`: the `[tune]` knobs away from their defaults,
  each alone and a few together) cover new looks; they are re-recorded with
  `UPDATE_BASELINE=1` only for an intended change, said in the commit. The
  `[tune]` defaults themselves are held by the pinned groups, which render
  with no `[tune]` table.
- **The font.** Every hash depends on the UI font (the first of
  `renderer::FONT_CANDIDATES` that loads). `tests/visual/font.txt` records
  it (`segoeui.ttf`, Segoe UI 5.72, and its SHA-256). On a machine that
  renders with another font every group is skipped with a visible "font
  differs, baseline not comparable" warning instead of failing, and nothing
  can be recorded there.
- On a mismatch the PNGs of the changed entries land in
  `%TEMP%\ailimits-visual\<group>\`; `DUMP_VISUAL=1` writes every entry.

## Resource footprint (measured, not estimated)

The table's reference column is the 0.7.0 measurement of 2026-09-23: 254 s
on a quiet machine (a video playing, nothing else; unelevated, so the
timer-resolution and SRUM rows are carried from the August run;
`bench/results/perf-audit-2026-09-23_0424.json`). The panel had asked the
shell for the bar's room at least once, which is the state it is in
whenever it is on screen; about 20 MB of the memory is UI Automation,
loaded for that question.

The README quotes the first 0.7.1 run, of b1ca5da on 2026-11-21 (322 s,
`bench/results/perf-audit-2026-11-21_0423.json`): 0.0097% of one core
(0.031 CPU-s), no GPU, a 54 MB working set, ~11 threads (14 peak), no
timer request. CLAUDE.md quotes both runs. The release build of 0.7.1 is
measured the same way before its release.

The earlier column is the 0.6.3 audit of 2026-08-23 with
`bench/perf_audit.ps1` (5 min at 1 Hz, running `release-min`, Win11 26200,
Ryzen 9 9950X3D / 32 threads, elevated; report
`bench/results/perf-audit-published-2026-08-23.json`), before the panel
asked UI Automation anything. CPU is exact kernel accounting, not sampling.
The 2026-07-08 run, kept beside it as `perf-audit-published-2026-07-08.json`,
read 0.024% of one core and 61 MB, five releases before that.

| Metric | 0.7.0 (2026-09-23) | 0.6.3 (2026-08-23) |
|---|---|---|
| CPU (exact accounting) | 0.031 CPU-s per 254 s = **0.012% of one core** (0.0004% of the machine) | 0.005% of one core |
| GPU | **0%** — the process owns no GPU-engine counter instances at all (CPU rendering) | 0% |
| Memory | 45 MB private / 67 MB working set | 37 MB working set (40 MB peak) |
| Threads | ~13 avg / 16 peak | ~10 avg / 13 peak |
| Handles | ~360 | ~377 |
| I/O | 576 B/s avg; spikes are the update cycle's HTTPS, one request per provider, < 5 KB each | 184 B/s avg |
| Context switches (idle-wakeup proxy) | ~1.2/s whole-window average | ~0.9/s |
| Platform timer resolution | **not raised** (`powercfg /energy`: no request from ailimits) | not raised |
| SRUM energy estimate | carried from the 2026-07-08 report: ~160 units/min avg, CPU + network only; display/disk/GPU all zero | the same |
| DPC/latency impact | none possible: user-mode, no drivers, no timer-resolution requests | the same |
| Binary (`release-min`) | 3.8 MB (`ailimits.exe`; 0.7.1: 4.1 MB) | ~3 MB |
