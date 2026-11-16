# CONFIG.md — the configuration file

Location: `%APPDATA%\AiLimits\config.toml`. Created automatically on the
first launch; every context-menu change is persisted here. Unknown fields
and providers are ignored (old configs never break parsing). UTF-8 with or
without a BOM and UTF-16 with a BOM (Notepad's encodings) are read.

When the file cannot be used, it is never overwritten with defaults:

- **It cannot be read** (another program holds it, the folder's
  permissions): the read is retried for about 2 seconds. Then the app runs
  with nothing switched on and no welcome, keeps the file exactly as it is
  and saves nothing over it until it is restarted. One message says so.
- **It cannot be understood** (broken TOML, text that is not UTF-8 or
  UTF-16): it is set aside as `config.toml.<UTC yyyymmdd-hhmmss>.corrupt`
  (with `-2`, `-3`... when that name is taken; an earlier one is never
  replaced). The app starts as on a new install, and the old settings remain
  in that file. One message names it.
- **It cannot be understood and cannot be set aside**: it is kept and
  nothing is saved, as for a file that cannot be read.

Next to it the app keeps files of its own (all removed by the installer's
uninstaller together with the folder):

| File | What it holds |
|---|---|
| `provider-cache.json` | the last known metrics with a future reset, so they survive restarts. Written when the values, limits or resets change (to the minute), otherwise at most every 10 minutes to refresh the fetch times, and at exit. After a crash a cached row can read at most 10 minutes older than it is, never younger. |
| `alerts.json` | which threshold alerts were given, per provider (the windows alerted and the last alert), so a restart does not repeat them. Entries older than a day are dropped. |
| `update-attempt.json` | the last self-update attempt: the version and the downloaded installer's path, and whether an unfinished attempt was told. Installer copies only. |
| `crash.log` | one line per crash: UTC time, version, thread, source file:line:column. The panic's text is never written. Rotates at 64 KB into `crash.log.1`. A start that failed writes `could not start: <error>` (which may name a local path, such as config.toml's). |
| `ailimits.log` | only while `AILIMITS_LOG` or `RUST_LOG` is set; rotates at 5 MB into `ailimits.log.1`. Tokens are never written. |
| `config.toml.<stamp>.corrupt` | a config that could not be understood, set aside as above. |

## Full example

```toml
[general]
# Update interval in seconds (menu: 60 / 300 / 900 / 1800; hard minimum 60).
# Polling pauses while idle/locked and backs off when a whole cycle fails
# (120, 240, then 300 s at a 60 s interval); after a sleep or an idle pause
# whose first cycle got no answer at all, one retry comes after 20 s.
# Claude's endpoint is also rate-limited server-side, so its refresh can
# occasionally take 2–3 minutes regardless of this setting.
update_interval_secs = 60
# Taskbar indicator (menu: Indicator): "tray" — a 16px tray icon, two
# concentric rings carrying the two busiest providers, monochrome in the
# system taskbar theme; "panel_rows" / "panel_grid" (legacy alias) — a transparent
# overlay left of the tray: a per-pixel-alpha layered window, only digits
# and bars are painted. Shows the first two providers in the widget's order
# (percent + bar, clock-sized), monochrome, following the SYSTEM light/dark
# theme; the rest stay in the tooltip. Tracks the taskbar's auto-hide and
# tray width event-driven, no polling. "bars" — legacy tray icon;
# "off" — nothing. Supersedes the legacy show_tray_icon flag (ignored).
indicator = "tray"
# Automatic updates (menu: Automatic updates). Only a copy our installer put
# on disk updates itself; a Store, Scoop, portable or self-built copy never
# asks GitHub anything, and its menu has no such item. When true, the app
# checks GitHub for a newer release (~30 s after start, then daily),
# downloads the installer and checks both the SHA-256 GitHub publishes for
# it and the release's Ed25519 signature, waits until you have been away for
# 5 minutes (no input, or the session locked), installs it silently and
# starts again. A version whose update did not finish is not tried again by
# itself; the next start says so once. Missing in old configs → true.
auto_update = true
# Manual nudge for the panel's position, in pixels, applied after the
# computed placement. Both default to 0, meaning no adjustment. Clamped to
# -200..200 — a typo cannot push the panel off the desktop. A repair tool
# for machines where the automatic placement misses: replaced shells such
# as StartAllBack or ExplorerPatcher, unusual taskbar layouts, touch-device
# layouts. Deliberately config-only, no menu entry.
panel_offset_x = 0
panel_offset_y = 0
# Which taskbar the mini panel attaches to (menu: Indicator -> Display;
# that submenu only appears when a secondary taskbar exists, and it's built
# at startup, so plugging in a monitor later needs a restart before the
# entry shows up). "primary" (default), or { secondary = N } where N counts
# secondary taskbars from 0, left to right by monitor position — e.g.
# { secondary = 0 } for the first (leftmost) secondary taskbar. Secondary
# taskbars only exist when Windows is set to show the taskbar on all
# displays. Falls back to "primary" if the chosen display is gone (monitor
# unplugged, or that Windows setting turned off), or if the value is
# unrecognised, rather than breaking the whole config file.
panel_display = "primary"

[window]
pos_x = 50
pos_y = 50
# Background opacity 0.10–0.85.
opacity = 0.45
# Always on top.
pinned = false
# Locked position — dragging disabled (independent of `pinned`).
locked = false

[ui]
# Palette: default / ocean / sunset / forest / neon / ice / rose / slate.
palette = "default"
# Palette saturation 0–100 (0 = greyscale, 100 = full color).
saturation = 55
# Text and bar brightness 20–100.
brightness = 100
# Monochrome mode (overrides the palette).
monochrome = false
# Layout: "vertical" or "horizontal".
layout = "vertical"
# Detail level: "compact" / "medium" / "expanded".
detail = "compact"
# Width step for the horizontal-bar (rows) layout; the vertical-bar layout
# ignores it. "full" / "three_quarters" / "half" (menu: Width).
width_scale = "full"
# Column arrangement for the vertical-bar layout; the rows layout ignores it.
# "row" (side by side, wide and short) or "column" (stacked, narrow and tall)
# (menu: Arrangement).
column_flow = "row"
# Burn-rate forecast ("~Xh to limit" when usage is climbing; menu: Forecast).
# Never replaces the reset countdown — shown only when no future reset is known.
show_forecast = false

# Rows (menu: Rows and models...). Written only once chosen; without it
# every provider has one row, as before. A provider's own row is its id, an
# extra row is provider.part, and only these exist, each where the provider
# meters it on its own: claude.opus, claude.sonnet, claude.fable (weekly,
# on plans that have them), antigravity.cg (the Claude and GPT pool), grok.build,
# cursor.bot (Grok Bot; Cursor asks for it only while this row is chosen).
# The widget and the tooltip are chosen separately; the taskbar panel and
# the tray icon keep one entry per provider. Unknown ids are ignored and
# kept. Rows chosen that have no data yet leave the widget saying "Nothing
# to show yet"; a click opens the window.
[ui.rows]
widget  = ["claude", "claude.opus", "codex"]
tooltip = ["claude", "claude.opus", "claude.sonnet", "codex", "grok"]

# Model labels (menu: Rows and models...). Parts per surface: "model",
# "version" (only with "model"), "effort" (may go alone). Empty: no label.
# Showing them reads each running tool's settings file, so the first tick
# asks first ([reads] tool_settings). The widget draws the label on a line of its
# own in Expanded and in the columns layout, and in what the reset leaves of
# the meta line in Medium (a line of its own there too when that room is
# narrower than a name); Compact leaves it to the tooltip. Narrow places
# drop effort first, then the version, then the model; a word is never cut.
# source: "configured" (the model set in the tool) or "last_used" (the
# newest Claude Code or Codex CLI session; asks first, [reads] last_session;
# other tools keep showing the configured model).
[ui.model]
widget  = ["model"]
tooltip = ["model", "version", "effort"]
source  = "configured"

# Your own names for models, by the id the tool stores. The automatic name
# comes from one rule with no table (claude-opus-5-5 -> "Opus 5.5"), so a
# new model needs no update. A model several tools share has one name. A
# tool's own router has a name per tool, under "provider:id"
# ("copilot:auto", "cursor:auto"); a name an older build kept under a bare
# "auto" serves both until one of them is renamed. A name must fit the
# narrowest place the widget draws a model name, about ten ordinary
# characters; the picker stops typing there. An empty name brings the
# automatic one back.
[ui.model.names]
"claude-opus-5-5" = "Opus 5.5"
"copilot:auto" = "Auto"

[[providers]]
# Identifier: claude / codex / copilot / antigravity / grok / cursor ("gemini"
# is a legacy id, renamed to "antigravity" on load). A default provider missing
# from an older config is appended on load, switched off: the version that
# wrote the file did not know it, and ticking it asks first.
id = "claude"
enabled = true
# Method: "subscription" (default) or "api_key" — only Claude has a choice.
auth_method = "subscription"
# Credential Manager key label; empty for the subscription method.
credential_label = ""
# Notification threshold, %.
alert_threshold = 80

[[providers]]
id = "codex"
enabled = true
auth_method = "subscription"
alert_threshold = 80

[[providers]]
id = "copilot"
enabled = true
auth_method = "subscription"
alert_threshold = 85

[[providers]]
id = "antigravity"
enabled = true
auth_method = "subscription"
alert_threshold = 80

[[providers]]
id = "grok"
enabled = true
auth_method = "subscription"
alert_threshold = 80

[[providers]]
# Cursor: only the widget's own session (menu: Providers > Cursor > Sign in
# to Cursor..., or ailimits-auth login cursor), kept in Credential Manager.
# Nothing of the Cursor editor is read for it. The row stays hidden until
# you sign in.
id = "cursor"
enabled = true
auth_method = "subscription"
alert_threshold = 80

[notifications]
enabled = true
# The shortest time between two threshold alerts of one provider, minutes.
# An alert (the toast and the on_threshold hook) comes once per limit
# window, when the headline usage reaches the threshold; again only after
# the window resets and the new one reaches it, or after usage fell 5 points
# or more below the threshold and climbed back. Kept across restarts in
# alerts.json. A provider switched off alerts nothing.
cooldown_minutes = 15

[hooks]
# Optional shell commands run on usage events (empty = disabled, the
# default). Each runs hidden and detached as `cmd.exe /D /S /C "<command>"`:
# quotes work as in a console ("C:\Program Files\x\y.exe" --flag), and the
# registry's AutoRun commands are not run (/D). A command with a line break
# is refused. A run is stopped after 30 s; a non-zero exit, the time-out or a
# failed start goes to the log (AILIMITS_LOG) by event name and code, never
# the command itself. The event context arrives in environment variables:
# AILIMITS_EVENT (threshold|reset|startup), AILIMITS_PROVIDER (the display
# name: Claude, Codex...), AILIMITS_PERCENT, AILIMITS_RESET_AT (RFC3339,
# when known).
# SECURITY: these are deliberately config-file-only — there is no menu or
# clipboard path, so nothing can auto-populate a command to run.
# Fires with the threshold alert, once per limit window (even with toasts off):
on_threshold = ""
# Fires when a provider that was at or above its threshold drops by more
# than 30 points (its window reset):
on_reset = ""
# Fires once after the first successful fetch:
on_startup = ""

# What the app may read of other tools' files. Nothing of theirs is read
# before it is allowed here, and each is asked once, naming the file and the
# only hosts the token goes to (see "What is read, and when" below).
[reads]
# Providers whose sign-in may be read. Ticking a provider in the picker or
# in the Providers menu asks first; unticking every row of a provider in the
# picker removes it from here again (and switches it off).
sign_in       = ["claude", "codex", "grok"]
# Providers set up only with the app's own sign-in: a key or usage token you
# pasted (kept in Credential Manager) or the menu's Cursor sign-in. They use
# that and never the tool's sign-in; when it is removed they show nothing
# rather than fall back to the tool's sign-in. Their settings file is still
# read for the model label when tool_settings allows it, and their newest
# session when last_session does. Written only when not empty.
app_sign_in   = ["cursor"]
# The settings files of every running tool, for the model and effort
# ([ui.model]), however the tool signs in; Cursor's state database is one of
# them. Unticking the last of Model, Version and Effort on both surfaces sets
# this and last_session back to false.
tool_settings = false
# The end of the newest Claude Code / Codex CLI session file, for the model
# last used ([ui.model] source = "last_used"). Switching back to "As set in
# the tool" sets it back to false.
last_session  = false
# A one-time window still owed: "welcome" (fresh install), "update_notice"
# (the first start after updating from 0.7.0 or older), or "none".
pending       = "none"
```

## Secrets

The config never holds keys or tokens — only labels. The actual values live
in the Windows Credential Manager under service `ailimits`:

| Label | Meaning |
|---|---|
| `claude_api_key` | Claude API key (the api_key method) |
| `claude_usage_token` | manual Claude subscription usage token |
| `codex_usage_token` | manual ChatGPT/Codex usage token |
| `copilot_pat` | GitHub PAT (used instead of the gh CLI) |
| `cursor_session` | the widget's own Cursor session, as UTF-8 JSON: the token, its expiry, the last refresh attempt, whether Cursor ended it (menu: *Sign in to Cursor...* / *Sign out of Cursor*; CLI: `ailimits-auth login cursor` / `logout cursor`) |

The installer's uninstaller removes all five (with `ailimits-auth`, locally,
no network). A Microsoft Store copy has no uninstall step of ours: before
removing it, use the menu's *Sign out of Cursor*, *Remove key* and *Remove
usage token*, or delete the `ailimits` entries under *Windows Credentials*
in Credential Manager.

A key or usage token pasted from the menu is taken only in the shape of what
was asked for: a Claude API key or usage token starts `sk-ant-`; a Copilot
token is `ghp_...`, `github_pat_...`, `gho_...`, `ghu_...` or a classic
40-character hex token; a Codex usage token is a JWT (`eyJ...`, three parts).
10 bytes to 8 KB, no spaces. The message says what was expected and never
quotes the clipboard, which is never logged. A usage token is then checked
with one request to the same host the provider sends it to: refused (401/403)
it is not stored; with no answer that says either way it is stored with a
toast "stored, but not checked: ...". A second paste or a removal while one
is checked replaces it.

## What is read, and when

Every read of another tool's file is optional and asked for first:

- **A fresh install reads nothing.** The widget and the tray say "Choose what
  to show", and the picker opens by itself as a welcome. Its question "Show
  the tools found on this PC?" has a tick for each tool found, judged only
  by whether a folder exists (nothing inside is opened), and names every
  file it would read and every host. Ticked at first are the tools found by
  a folder of their own: `%USERPROFILE%\.claude`; `CODEX_HOME` or
  `%USERPROFILE%\.codex`; `COPILOT_HOME` or `%USERPROFILE%\.copilot`;
  `%APPDATA%\Antigravity` or `%USERPROFILE%\.gemini\antigravity-cli`;
  `GROK_HOME` or `%USERPROFILE%\.grok`; `%APPDATA%\Cursor`. A tool found only
  by a related folder (`%APPDATA%\GitHub CLI` for Copilot, `.gemini` for
  Antigravity) is listed unticked, with a note why; when only such tools are
  found, no question opens and their rows are only marked "found on this
  PC". Allow starts the ticked tools only, and is inactive with none ticked;
  Cancel reads nothing and leaves the choice to you.
- **Any other tool's first tick asks once**, naming the file and the host.
  While other tools found on this PC by a folder of their own are not running
  yet, that question offers *Allow all*: it starts only those, and never a
  tool switched off in the Providers menu or one already allowed. It is not
  offered in a window where the welcome's question was already answered.
- **The welcome and the update notice open quietly.** They take no keyboard
  focus, so Enter or Space allows nothing until Tab or an arrow key moves to
  a button. They wait while a fullscreen window (a game, a video, a browser
  on F11, a slide show) fills its monitor, and open at the next change of
  the window in front, the next update or the next start of the app. An
  answered question, the close button, Esc, Alt+F4, OK or Review counts as
  seen. A click beside the box, or going to another window once it had the
  focus, closes it unanswered, so it opens again on the next start.
- **After updating from 0.7.0 or older** nothing stops: the providers that
  were switched on keep reading as before (`sign_in` is filled with them),
  and one window lists every file read and every host contacted, once. A
  provider the old version did not have (Grok and Cursor, for 0.7.0) is
  added switched off and asks when you tick it.
- **A config that cannot be understood** is set aside (see the top) and the
  app starts as a fresh install: what the lost file allowed is unknown, so
  nothing is read until chosen again. An empty config, or one with no
  `[[providers]]`, counts as a fresh install too. One that cannot be read is
  kept, and the app runs with nothing switched on until restarted.
- **A fresh install switches every provider off** as well as allowing
  nothing, so even an older version started on the same config afterwards
  reads only what was allowed.
- **To stop reading a tool**, untick all of its rows in Rows and models (or
  untick Enabled in the Providers menu). Its cached numbers, last error,
  trend and alert marks are cleared. Unticking the last of Model, Version and
  Effort on both surfaces takes back the settings and session reads
  (`tool_settings` and `last_session` become false); switching back to "As
  set in the tool" takes back the session read. The next tick asks again.
- **A provider set up with the app's own sign-in** (a pasted key or usage
  token, the menu's Cursor sign-in) never has the tool's own sign-in read.
  Its settings file is read for the model label when `tool_settings` allows
  it, as for every running provider.
- **Where the tools keep their files** can be moved by `CODEX_HOME`,
  `GROK_HOME` and `COPILOT_HOME`. Each moves every file of its tool the app
  reads: Codex's `auth.json`, `config.toml` and sessions; Grok's `auth.json`
  and `config.toml`; the Copilot CLI's `settings.json`. They must be absolute
  paths: a relative one means that tool's files are not read at all (never a
  folder relative to the app, never the default folder), and the tool is not
  looked for on this PC either. Claude Code's `CLAUDE_CONFIG_DIR` and gh's
  `GH_CONFIG_DIR` are not followed.
- **`ailimits-auth status`** is a diagnostic you run yourself. It changes
  nothing: it never creates, renames or writes `config.toml`. It reads
  another tool's sign-in only where the widget may (`[reads] sign_in`), and
  otherwise prints "found (not read: ...)". Codex's `auth.json` and the
  legacy Gemini `oauth_creds.json` are only checked for existence; gh is
  asked for github.com only (`gh auth token --hostname github.com`, 10 s
  time-out, its output discarded); of Cursor only the widget's own session
  is shown. It prints each provider's read mode and the `[reads]` state, and
  never a token.
- **The other `ailimits-auth` commands** (`set`, `remove`,
  `set-usage-token`, `login`) refuse a `config.toml` they cannot read or
  understand ("nothing was changed"). The widget writes its own settings back
  when it quits, so quit it before running one of them, then start it.

The files and hosts per provider are in [PROVIDERS.md](PROVIDERS.md) and in
`src/consent.rs`, the text the questions show; the paths are resolved in
`src/config/paths.rs`.

The Rust structs live in `src/config/schema.rs` — that file is the single
source of truth for the schema.
