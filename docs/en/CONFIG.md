# CONFIG.md — the configuration file

Location: `%APPDATA%\AiLimits\config.toml`. Created automatically on the
first launch; every context-menu change is persisted here, and edits saved in
it apply while the app runs (see "Live edits" below). There is no portable
mode: a copy from the installer, the portable zip or Scoop uses this same
folder, and only the installer's uninstaller removes it. A Microsoft Store
copy that finds no such folder makes its own, which Windows keeps with the
package:
`%LOCALAPPDATA%\Packages\napxlexn.AILimits_<publisher id>\LocalCache\Roaming\AiLimits`.
The app still sees it at the usual path, *Edit config.toml…* opens it where
it is, and removing the Store copy removes it; other programs (an
`ailimits-auth` from another copy among them) do not see it at
`%APPDATA%\AiLimits`. A folder another copy left there is shared with the
Store copy instead. Unknown fields and
providers are ignored (old configs never break parsing). An unknown field is
also dropped at the app's next save; an unknown provider's table and an
unknown row id are kept. A second `[[providers]]` table with an id already
used is skipped. UTF-8 with or without a BOM and UTF-16 with a BOM (Notepad's
encodings) are read. Fine-tuning beyond the menu, the `[tune]` tables, has a
guide of its own: [TUNING.md](TUNING.md).

The file explains itself. The app writes a short note before each table
and each setting: what it does, the values or the range it takes, its
default, and the menu item or window that sets the same. The four `[tune]`
tables are always there, every key not set commented out with its default
and its range, so taking the `# ` away is all it takes to use one. Comments
you add are not kept: each save of the app's own writes the file anew with
its notes (`src/config/annotate.rs`).

When the file cannot be used, it is never overwritten with defaults:

- **It cannot be read** (another program holds it, the folder's
  permissions): the read is retried for about 2 seconds. Then the app runs
  with nothing switched on and no welcome, keeps the file exactly as it is
  and saves nothing over it until it is restarted. One message says so.
- **A line does not read** (a `key = value` line whose value is of the
  wrong kind, a typo, a missing value): that line is left out and its
  default used; the rest of the file applies. A table header that does not
  read (a table written twice, a header without its `]`) is left out with
  its keys, up to the next header. Only these two kinds of line are left
  out. One message names the line
  numbers, never the text. The file as it was is kept beside it as
  `config.toml.<UTC yyyymmdd-hhmmss>.left-out`, since the app's next save
  writes the config without that line. The file is not rewritten before the
  app has a change of its own to save, so the line may still be in it at
  the next start: a copy of the same file already kept is named again, and
  no other is made.
- **It cannot be understood** (a mistake on any other line, such as
  `text_scale 1.2` with no `=`, or one TOML gives no line for; a mistake
  found after 8 lines were left out, a header counting every line of its
  table; text that is not UTF-8 or UTF-16; settings with neither `[reads]`
  nor any `[[providers]]`, such as a `[tune.widget]` snippet saved in place
  of the whole file): it is set aside as
  `config.toml.<UTC yyyymmdd-hhmmss>.corrupt`
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
| `icon.png` | the app's icon for its notifications. Every copy not from the Microsoft Store writes it here, and writes it again when its size differs from the icon the app carries. |
| `crash.log` | one line per crash: UTC time, version, thread, source file:line:column. The panic's text is never written. Rotates at 64 KB into `crash.log.1`. A start that failed writes `could not start: <error>` (which may name a local path, such as config.toml's). |
| `ailimits.log` | only while `AILIMITS_LOG` or `RUST_LOG` is set; rotates at 5 MB into `ailimits.log.1`. Tokens are never written. |
| `config.toml.<stamp>.corrupt` | a config that could not be understood, set aside as above. |
| `config.toml.<stamp>.left-out` | a config whose lines were left out at start, kept as it was before the app saves over it (see above). |

## Live edits

Edits saved in `config.toml` apply while the app runs, without a restart.
*Settings* has a small link at the end of its *Display* tab, *Edit
config.toml...*, for the fine-tuning that has no place in the window (sizes,
colours, thresholds): it opens this file in Notepad
(`%SystemRoot%\System32\notepad.exe`, started directly, no shell verb).
The welcome has no such link.

- **When the file is looked at.** No change notification of Windows is
  used: the app compares the file's size and last write time (one metadata
  call, about 20 µs, nothing read) every 2 seconds while you are at the PC
  (any input in the last 5 minutes), every 30 seconds while you are away,
  and every 2 seconds for 30 minutes after *Edit config.toml…* opened the
  file, away or not: an edit applies within about two seconds of its save.
  It also looks once per update cycle, right before it applies a
  context-menu or Settings action, before each save of its own (the
  end of a drag saves the position), and when it quits, so a change made in
  the app lands on top of the file as saved.
- **The app's own saves are not edits.** The writer records the size and
  time of each file it writes, and a file that matches is left alone. A
  save with nothing new in it is not written at all, so the comments and
  layout of a hand edit stay until the app has a change of its own to save;
  that save writes the whole file anew with the app's own notes, and the
  comments you added are not kept.
- **An edit that reads** applies at once, brought up to date as a start
  would (renamed and added providers, `[reads]` as its origin calls for; a
  position on no monitor comes back onto one), except that it never brings
  the welcome back nor settles the sign-in sources again: `[reads] pending`
  and `sources_settled` stay the app's. Only what changed is redone: the
  providers whose entry or consent changed are rebuilt and fetched (an
  alert threshold rebuilds nothing and keeps the numbers on screen), the
  model labels, the widget's layout, colours, position and opacity, Always
  on top, Lock position, the menu's ticks, the tray icon, the taskbar panel
  (mode, display, offset, what it shows), the update interval (after the
  current wait, as from the menu), automatic updates, notifications, hooks,
  `[tune]`, and Settings if it is open. Nothing needs a restart;
  `on_startup` has run already and runs again at the next start.
- **A change made in the app while an edit was on its way** (in the moment
  before the file was saved, or while it did not read) is kept wherever the
  edit left that setting as it was, and saved together with the edit; where
  both changed the same setting, the file wins.
- **Consent in the file.** A read allowed by hand in `[reads]` (`sign_in`,
  `app_sign_in`, `tool_settings`, `last_session`) counts as your choice, as
  a tick in Settings does: the files it names are read from the next
  fetch on, without a question. A provider taken out of `[reads]` by hand
  stops at once. A provider allowed by hand also needs `enabled = true` in
  its `[[providers]]` table: it runs only with both. The terms a read
  allowed by hand is given under (`sign_in_terms`, `model_terms`) are
  written at the next start. Deleting the whole
  `[reads]` table by hand makes the file read as a 0.7.0 config: every
  provider with `enabled = true` is then allowed at once, without a
  question.
- **A line that does not read** is left out, as at start (see the top):
  its default applies, the rest of the edit applies, and one message names
  the line. The file stays as you saved it until the app has a change of
  its own to save, which writes it anew without that line (no copy is kept
  for a live edit): fix the line before that.
- **An edit that does not read** (broken TOML that leaving a line out does
  not mend, another encoding) is not applied: the app keeps running on the settings it had and says so in one
  message that names the line and column only, never the text. The file is
  never written over while it stays broken, not even when the app quits:
  changes made in the app meanwhile apply but are not saved, and are merged
  as above once the file reads again (one message says so). Quit before
  fixing it, and they are lost while your text stays exactly as it is.
- **A file emptied or removed** is not applied (it would read as a fresh
  install): the app keeps its settings and writes them back with its next
  save, or when it quits. To start over, quit the app first.
- **A file of comments only** is an emptied one, as above.
- **A file with settings but neither `[reads]` nor any `[[providers]]`**
  (only a pasted `[tune.widget]` snippet) is not applied either: it would
  start over as a fresh install. It counts as a file that does not read:
  one message says it is only a part of a config, and nothing is saved over
  it until the rest is back (undo in the editor), as above.
- **A file kept as it was at start** (one that could not be read, or
  neither understood nor set aside: see the top) is not looked at: that
  session still needs a restart, as its message says.

## Full example

The notes the app writes in the file are shorter; this example says more.

```toml
[general]
# Refresh interval in seconds (menu: 60 / 300 / 900 / 1800; hard minimum 60).
# Polling pauses while idle/locked and backs off when a whole cycle fails
# (120, 240, then 300 s at a 60 s interval); after a sleep or an idle pause
# whose first cycle got no answer at all, one retry comes after 20 s.
# Claude's endpoint is also rate-limited server-side, so its refresh can
# occasionally take 2–3 minutes regardless of this setting.
update_interval_secs = 60
# Taskbar indicator (menu: Taskbar): "tray" — a 16px tray icon carrying
# the two providers the panel would show, by default two concentric rings
# (the busier outside; other looks: [ui.tray]), monochrome in the system
# taskbar theme; "panel_rows" /
# "panel_grid" (legacy alias) — a transparent overlay left of the tray: a
# per-pixel-alpha layered window, only digits and bars are painted. Shows the
# first two providers in the widget's order (clock-sized; by default percent
# + bar, see [ui.panel]), monochrome, following the SYSTEM light/dark theme;
# the rest stay in the tooltip. Tracks the taskbar's auto-hide and tray width
# event-driven, no polling. "bars" — legacy tray icon; "off" — nothing.
# Supersedes the legacy show_tray_icon flag (ignored). Which providers the
# panel and the tray icon show can be chosen: [ui.rows] taskbar below.
indicator = "tray"
# Automatic updates (menu: Automatic updates). Only a copy our installer put
# on disk updates itself; a Store, Scoop, portable or self-built copy never
# asks GitHub anything, and its menu has no such item. When true, the app
# checks GitHub for a newer release (~30 s after start, then daily, and
# ~10 s after you switch it on, unless it checked within the hour). A
# newer one waits until you have been away for 5 minutes (no input, or the
# session locked); then the installer is downloaded and checked against
# both the SHA-256 GitHub publishes for it and the release's Ed25519
# signature, and installed silently if you are still away (otherwise it
# waits again); the app starts again afterwards. A version whose update did not finish is not tried again by
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
# Which taskbar the mini panel attaches to (menu: Taskbar -> Panel display;
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
# Layout: "vertical" (providers stacked, horizontal bars: the menu's
# Rows) or "horizontal" (providers side by side, vertical bars: the menu's
# Columns).
layout = "vertical"
# Detail level: "compact" / "medium" / "expanded".
detail = "compact"
# Width step for the horizontal-bar (rows) layout; the vertical-bar layout
# ignores it. "full" / "three_quarters" / "half" (menu: Width).
width_scale = "full"
# Column arrangement for the vertical-bar layout; the rows layout ignores it.
# "row" (side by side, wide and short) or "column" (stacked, narrow and tall)
# (menu: Column layout: Side by side / Stacked).
column_flow = "row"
# Burn-rate forecast ("~Xh to limit" when usage is climbing; menu: Usage forecast).
# Never replaces the reset countdown — shown only when no future reset is known.
show_forecast = false

# Rows (menu: Settings…, Providers). Written only once chosen; without it
# every provider has one row, as before. A provider's own row is its id, an
# extra row is provider.part, and only these exist, each where the provider
# meters it on its own: claude.opus, claude.sonnet, claude.fable (weekly,
# on plans that have them), antigravity.cg (the Claude and GPT pool), grok.build,
# cursor.bot (Grok Bot; Cursor asks for it only while this row is chosen).
# The week of a model Claude adds later becomes the row claude.<model name
# in lowercase, words joined by "-"> (claude.haiku) without an update, as
# soon as Claude reports it.
# Once a provider has answered, the window offers only the extra rows it
# reports (and those chosen already, to be unticked); before that, all.
# The widget and the tooltip are chosen separately; the taskbar panel and
# the tray icon keep one entry per provider. Unknown ids are ignored and
# kept. Rows chosen that have no data yet leave the widget saying "No data
# yet"; a click opens the window.
# taskbar: the providers the taskbar panel and the tray icon show (the
# window's Taskbar column), at most two, in the widget's order. Written only
# once chosen; without it both show the first two running providers (the tray
# icon inherits the panel's), and the window ticks those two. When none of
# those listed runs, the first two that run show. A provider kept here keeps
# running though none of its widget or tooltip rows is ticked.
[ui.rows]
widget  = ["claude", "claude.opus", "codex"]
tooltip = ["claude", "claude.opus", "claude.sonnet", "codex", "grok"]
taskbar = ["claude", "grok"]

# Model labels (menu: Settings…, Display, Model). Parts per surface: "model",
# "version" (only with "model"), "effort" (may go alone). Empty: no label.
# Showing them reads each running tool's settings file, so the first tick
# asks first ([reads] tool_settings). The widget draws the label on a line of its
# own in Expanded and in the columns layout, and in what the reset leaves of
# the meta line in Medium (a line of its own there too when that room is
# narrower than a name); Compact leaves it to the tooltip. Narrow places
# drop effort first, then the version, then the model; a word is never cut.
# source (the window's Model source): "configured" (Tool settings: the
# model set in the tool) or "last_used" (Last session: the newest Claude
# Code or Codex CLI session; asks first, [reads] last_session; other tools
# keep showing the configured model).
[ui.model]
widget  = ["model"]
tooltip = ["model", "version", "effort"]
source  = "configured"

# Your own names for models (the window's Model aliases), by the id the
# tool stores. The automatic name
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

# What each widget row shows besides its name (menu: Settings…,
# Display, Widget column): the percentage, the time left until the
# reset, the progress bar, in any mix; the window keeps one on, and all
# three false in the file read as the default. Written only once changed;
# without it, all three: the widget as it always was. Without the bar a
# Compact or Medium row is one line ("Claude   2h 14min  25%") and the
# widget only as wide as its texts; an Expanded row keeps its two lines, the
# weekly limit under the name. A narrower Width step drops the reset first,
# never the name. With the bar, Compact shows each row's reset in place of
# the percentage when that is left out, and keeps the soonest reset on its
# bottom line while it shows both; Medium without the reset loses its line
# under the bar. In the Columns layout a column without its bar
# is its texts stacked, the percentage in the bar's colour, the name, the
# reset time, and the widget is only as tall as they are; with the bar,
# `percent` and `reset` give their room to the bar.
[ui.widget]
percent = true
reset   = true
bar     = false

# What each row of the taskbar panel's tooltip shows besides its name
# (menu: Settings…, Display, Tooltip column): the
# percentage, the time left until the limit the row shows resets
# ("Claude 25% · 2h 14min"), or one of them alone ("Claude 2h 14min").
# The window keeps one on; both false in the file leaves each row its name
# and a dot. Written only once changed; without it, both. With the
# percentage alone and no model labels or extra rows, the tooltip is one
# line, as in 0.7.0; otherwise it has one row per line. The tray icon's
# tooltip says the same (within the 127 characters Windows keeps).
[ui.tooltip]
percent = false
reset   = true

# What each row of the taskbar panel shows (menu: Settings…,
# Display, Taskbar column): the percentage with its bar (as in 0.7.0), the
# time alone before the bar ("2h14 ▬▬"), both with the bar between them
# ("25% ▬▬ 2h14"; on a side taskbar the time under the percentage), or the
# bar alone. Compact like the clock: 3d4h, 2h14, 12m, <1m; a dash when the
# row's own reset is not known.
# `bar = false` leaves the bar out for a narrower panel ("25%", "2h14",
# "25%  2h14"); without the key the bar is on. Written only once changed;
# without it, the percentage with its bar, and a key left out of the table
# keeps that default. The window keeps one of the three on; all three false
# in the file read as that default. The panel is as long as what it shows
# (119 px at 100% for a number and the bar, 161 px with both numbers, 77 px
# for the bar alone, 50 px for one number alone); with no room for it on the
# taskbar the tray
# icon stands in. While it shows a time it repaints when the time next reads
# differently (once a minute, once an hour past a day), and asks the shell
# nothing for it.
[ui.panel]
percent = true
reset   = true
bar     = true

# The tray icon's look (menu: Settings…, Display, Tray icon style):
# "rings" (default: two concentric rings, the busier outside), "bars" (two
# upright bars filling from the bottom, the busier on the left), "number"
# (the busier's percentage in digits; "!" from 100%) or "sector" (Pie in the
# window: the busier's slice of a disc, from 12 o'clock). Monochrome in the taskbar's
# theme; the other provider of the two is in the tooltip. The legacy
# indicator = "bars" keeps its horizontal bars. Written only once changed.
[ui.tray]
style = "rings"

[[providers]]
# Identifier: claude / codex / copilot / antigravity / grok / cursor ("gemini"
# is a legacy id, renamed to "antigravity" on load). A default provider missing
# from an older config is appended on load, switched off: the version that
# wrote the file did not know it, and ticking it asks first.
# The order of these tables is the order everywhere (the widget, the
# tooltip, the panel's first two): no menu changes it, an edit of the file
# does, and the order of the ids in [ui.rows] is ignored (extra rows follow
# their provider).
id = "claude"
# Switched on. A provider runs when it is switched on AND allowed in
# [reads]. Ticking a row of it switches it on; clearing its last row
# switches it off (paused: nothing of it is read or sent) and leaves it in
# [reads], so a tick switches it on again without asking.
enabled = true
# Where the sign-in comes from: chosen under the provider's rows in
# Settings (Sign-in). Only this source is used, never another as a
# fallback. "subscription" (default): the tool's own sign-in (Claude Code,
# Codex CLI, GitHub CLI, ...). "usage_token": a usage token you pasted
# (Claude, Codex). "api_key": a key you pasted (Claude's API key, Copilot's
# fine-grained token). Antigravity, Grok and Cursor have one source each.
# An older version reads "usage_token" as "subscription".
auth_method = "subscription"
# Credential Manager label of the pasted key with "api_key"
# (claude_api_key, copilot_pat); empty otherwise. The key is read only while
# this is exactly the provider's own label: any other value, or an empty
# one, reads nothing, and the row stays hidden.
credential_label = ""
# Notification threshold: the usage, in percent from 0 to 100, at which the
# alert comes (over 100: never). Default 80, 85 for copilot. No menu item.
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
# Cursor: only the widget's own session (Settings: Cursor, Sign-in,
# Sign in to Cursor…, or ailimits-auth login cursor), kept in Credential
# Manager. Nothing of the Cursor editor is read for it. The row stays
# hidden until you sign in.
id = "cursor"
enabled = true
auth_method = "subscription"
alert_threshold = 80

[notifications]
# false turns off the toasts only: the on_threshold hook still runs.
enabled = true
# The shortest time between two threshold alerts of one provider, minutes.
# An alert (the toast and the on_threshold hook) comes once per limit
# window, when the headline usage reaches the threshold; again only after
# the window resets and the new one reaches it, or after usage fell more
# than 5 points below the threshold and climbed back. Kept across restarts in
# alerts.json. A provider switched off alerts nothing. 0 reads as 1.
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
# Providers whose sign-in may be read. Ticking a provider's row in the
# picker asks first, once: clearing every row of a provider only switches it
# off (enabled = false) and keeps it here. Forget permission, in its opened
# Sign-in, removes it from here (and from sign_in_terms), switches it off and
# clears its rows; the next tick asks again.
sign_in       = ["claude", "codex", "grok"]
# Providers set up only with the app's own sign-in: a key or usage token you
# pasted (kept in Credential Manager) or Cursor's own sign-in. They use
# that and never the tool's sign-in; when it is removed they show nothing
# rather than fall back to the tool's sign-in. Their settings file is still
# read for the model label when tool_settings allows it, and their newest
# session when last_session does. Written only when not empty.
app_sign_in   = ["cursor"]
# The settings files of every running tool, for the model and effort
# ([ui.model]), however the tool signs in; Cursor's state database is one of
# them. With none of Model, Version and Reasoning effort ticked nothing is
# read, and this stays as it is. Forget permission (Settings, Display, under
# Model, Version and Reasoning effort) sets this and last_session back to
# false and clears the parts.
tool_settings = true
# The end of the newest Claude Code / Codex CLI session file, for the model
# last used ([ui.model] source = "last_used"). Switching back to "Tool
# settings" sets it back to false, and so does Forget permission.
last_session  = false
# A one-time window still owed: "welcome" (fresh install), "update_notice"
# (the first start after updating from 0.7.0 or older), or "none".
pending       = "none"
# The sign-in sources were settled, once (see "Secrets" below): a key or
# token pasted before the source could be chosen kept the lead it had.
# Never again after that, so a source chosen in the window stays. Written
# only when true; a fresh install writes it at once.
sources_settled = true
# The terms each permission in sign_in was given under, by provider (1 for
# every provider today). A later version that reads another file for a
# provider's sign-in, or sends its token to another host, has higher terms:
# at its first start the permission is taken back (removed from sign_in,
# the provider switched off, its rows cleared, a notification names the
# tool), and the next tick asks again. One given before terms were kept
# gets today's, written once at start. The app's own: leave it as it is.
# Written only when not empty.
sign_in_terms = { claude = 1, codex = 1, grok = 1 }
# The terms the model label's reads (tool_settings, last_session) were given
# under, held the same way: under older terms both go back to false and the
# parts are cleared. 0 = never allowed, and then not written. The app's
# own: leave it as it is.
model_terms   = 1
```

## Secrets

The config never holds keys or tokens — only labels. The actual values live
in the Windows Credential Manager under service `ailimits`:

| Label | Meaning |
|---|---|
| `claude_api_key` | Claude API key (the api_key method) |
| `claude_usage_token` | manual Claude subscription usage token |
| `codex_usage_token` | manual ChatGPT/Codex usage token |
| `copilot_pat` | GitHub fine-grained token (the "api_key" source, instead of the gh CLI) |
| `cursor_session` | the widget's own Cursor session, as UTF-8 JSON: the token, its expiry, the last refresh attempt, whether Cursor ended it (Settings, Cursor's Sign-in: *Sign in to Cursor…* / *Sign out*; CLI: `ailimits-auth login cursor` / `logout cursor`) |

The installer's uninstaller removes all five (with `ailimits-auth`, locally,
no network). A Microsoft Store copy has no uninstall step of ours: before
removing it, use *Sign out* and *Remove* in each provider's Sign-in in
*Settings* (*Remove* acts on the source chosen, so choose each pasted source
in turn), or delete the `ailimits` entries under *Windows Credentials* in
Credential Manager.

A key or usage token is pasted in Settings: in the provider's
Sign-in, with its source chosen, *Paste token from clipboard* (*Paste key
from clipboard* for Claude's API key). It is taken only in the shape of what
was asked for: a Claude API key or usage token starts `sk-ant-`; a Copilot
token is `ghp_...`, `github_pat_...`, `gho_...`, `ghu_...` or a classic
40-character hex token; a Codex usage token is a JWT (`eyJ...`, three parts).
10 bytes to 8 KB, no spaces. The message says what was expected and never
quotes the clipboard, which is never logged. A usage token is then checked
with one request to the same host the provider sends it to: refused (401/403)
it is not stored; with no answer that says either way it is stored with the
note "Stored, but not checked: ...". A key is stored without a check, and
the row says whether it works. What came of it is said in the window, and
as a toast only while the window is closed. A second paste or a removal
while one is checked replaces it. A key or token stored sets the provider
to that source and switches it on.

Once, for a config from before the source could be chosen (`[reads]
sources_settled` not yet true), the first start sets a provider left on
`"subscription"` while a usage token (Claude, Codex) or a Copilot token is
stored to that token (`app/sign_in.rs`, `settle_sources_once`): such a
token took the lead over the tool's sign-in then, and keeps it. After that
the source chosen in the window stays, though a token is still stored: a
stored key or token that is not the source chosen is not used. A fresh
install has nothing to settle.

## What is read, and when

Every read of another tool's file is optional and asked for first:

- **A fresh install reads nothing.** The widget and the tray say "Setup
  required", and the picker opens by itself as a welcome. Its question "Show
  the detected tools?" has a tick for each tool found, judged only
  by whether a folder exists (nothing inside is opened), and names every
  file it would read and every host. Ticked at first are the tools found by
  a folder of their own: `%USERPROFILE%\.claude`; `CODEX_HOME` or
  `%USERPROFILE%\.codex`; `COPILOT_HOME` or `%USERPROFILE%\.copilot`;
  `%APPDATA%\Antigravity` or `%USERPROFILE%\.gemini\antigravity-cli`;
  `GROK_HOME` or `%USERPROFILE%\.grok`; `%APPDATA%\Cursor`. A tool found only
  by a related folder (`%APPDATA%\GitHub CLI` for Copilot, `.gemini` for
  Antigravity) is listed unticked, with a note why; when only such tools are
  found, no question opens and their rows are only marked
  "detected". Allow starts the ticked tools only, and is inactive with none ticked;
  Cancel reads nothing and leaves the choice to you.
- **Any other tool's first tick asks once**, naming the file and the host.
  While other tools found on this PC by a folder of their own have not been
  allowed yet, that question offers *Allow all*: it allows those too
  (`[reads] sign_in`), so ticking one of them later asks nothing, but it
  switches none of them on and ticks none of their rows: a tool runs once a
  row of it is ticked. A tool already allowed is left as it is. It is not
  offered in a window where the welcome's question was already answered.
- **The welcome and the update notice open quietly.** They take no keyboard
  focus, so Enter or Space allows nothing until Tab or an arrow key moves to
  a button. They wait while a fullscreen window (a game, a video, a browser
  on F11, a slide show) fills its monitor, and open at the next change of
  the window in front, the next update or the next start of the app. An
  answered question, Esc, the close button, Alt+F4, OK or Review counts as
  seen. Nothing else closes it: a click beside the box and going to
  another window leave it open, as they leave *Settings*.
- **After updating from 0.7.0 or older** nothing stops: the providers that
  were switched on keep reading as before (`sign_in` is filled with them),
  and one window lists every file read and every host contacted, once. A
  provider the old version did not have (Grok and Cursor, for 0.7.0) is
  added switched off and asks when you tick it.
- **A downgrade to 0.7.0 and back.** When 0.7.0 saves the file, it drops
  `[reads]`, `[ui.rows]`, `[ui.model]`, `[ui.widget]`, `[ui.tooltip]`,
  `[ui.panel]`, `[ui.tray]` and `[tune]`, and writes `"usage_token"` as `"subscription"`. The next 0.7.1
  start takes the file for an upgrade: it allows every provider switched on
  and shows the update notice again. Its record of the sources settled is
  gone too, so a stored usage token or Copilot token becomes the source
  again.
- **A config that cannot be understood** is set aside (see the top) and the
  app starts as a fresh install: what the lost file allowed is unknown, so
  nothing is read until chosen again. An empty config, or one of comments
  only, counts as a fresh install too. One with settings but neither
  `[reads]` nor `[[providers]]` (a `[tune]` snippet alone) cannot be
  understood, so it is set aside the same way. One with `[reads]` but no
  `[[providers]]` is no fresh install: it gets the six default providers,
  and `[reads]` decides which run. One that cannot be read is
  kept, and the app runs with nothing switched on until restarted.
- **A fresh install switches every provider off** as well as allowing
  nothing, so even an older version started on the same config afterwards
  reads only what was allowed.
- **Each tool is asked once; clearing its rows pauses it.** Clearing all of
  a tool's rows in Settings, the Taskbar column included, switches it off
  (`enabled = false`): nothing of it is read or sent, and its cached
  numbers, last error, trend and alert marks are cleared. Its permission
  stays in `[reads]`, so ticking a row again switches it on without a
  question. Clearing the last of Model, Version and Reasoning effort on
  both surfaces pauses the model label the same way: nothing is read
  without a part, and `tool_settings` and `last_session` stay as they are.
- **To take a permission back**, use *Forget permission*. In a provider's
  opened Sign-in (shown while the tool's own sign-in is the source chosen
  and allowed) it removes the provider from `sign_in`, switches it off and
  clears its rows on every surface. On the Display tab, under the note
  "Read from each running tool's own settings file; asked once, before the
  first read." (shown once those reads are allowed), it sets
  `tool_settings` and `last_session` to false and clears Model, Version and
  Reasoning effort. The next tick asks again. Switching back to "Tool
  settings" still takes back the session read alone. Cursor's own sign-in
  ends with *Sign out*, a pasted key or token with *Remove*, as before.
- **A permission keeps its terms.** Each is recorded with the terms it was
  given under (`[reads] sign_in_terms`, `model_terms`). A later version that
  reads another file for a tool, or sends its token to another host, has
  higher terms for it: at its first start, before any provider runs, it
  takes that permission back, clears the tool's rows (or the model label's
  parts) and says once "What AI Limits reads for <tool> has changed. Select
  it again in Settings to allow the new reads." The next tick asks with the
  new files and hosts. A permission from before terms were kept gets
  today's, with no question.
- **Taking back is as easy as giving.** A permission is one click to give
  and one click to take back. Permissions are per Windows user, kept in
  that user's `config.toml`. Pausing and forgetting add no host, file or
  system call: they only stop reads.
- **A provider set up with the app's own sign-in** (a pasted key or usage
  token, Cursor's own sign-in) never has the tool's own sign-in read. Its
  settings file is read for the model label when `tool_settings` allows it,
  and its newest session when `last_session` does, as for every running
  provider.
- **Only the source chosen is read.** With a pasted key or usage token
  chosen as the source (`auth_method`), the tool's own sign-in is not read
  even where `[reads] sign_in` allows it; with the tool's account chosen, a
  stored key or token is not sent.
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
  otherwise prints "found (not read: ...)". Where it may read Claude Code's
  `.credentials.json`, it shows the subscription type with the token's
  expiry. Codex's `auth.json` and Claude Code's
  `%USERPROFILE%\.claude\stats-cache.json` are only checked for existence,
  never opened (the legacy Gemini `oauth_creds.json` is no longer
  looked at); gh is
  asked for github.com only (`gh auth token --hostname github.com`, 10 s
  time-out, its output discarded); of Cursor only the widget's own session
  is shown. It prints each provider's read mode and the `[reads]` state,
  names the lines of `config.toml` that do not read, and never prints a
  token.
- **The other `ailimits-auth` commands** (`set`, `remove`,
  `set-usage-token`, `remove-usage-token`, `login`, `logout`) never replace
  or rename a `config.toml` they cannot read or understand, nor one with
  lines that do not read (saved, it would lose them, and they keep no copy):
  they name the lines ("config.toml line 12 does not read; fix it first").
  `set` and `set-usage-token` then store nothing ("nothing was changed"); `remove`
  removes the key all the same and leaves the config as it is; `login`
  keeps the session it got and says the config could not be updated. With
  no `config.toml` yet, `set`, `set-usage-token`, `remove` and `login`
  create one as a first start would, with their own change in it. A
  running widget picks up what they change by itself, with no restart (see
  Live edits): a changed setting within a few seconds, a new or removed key
  or token at its next update, since the stored keys, tokens and the Cursor
  session are read anew on every update. What they store becomes the source:
  `set claude|copilot` sets `auth_method = "api_key"` (with
  `credential_label`), `set-usage-token` sets `"usage_token"`; `remove
  claude|copilot` sets `"subscription"`. `remove-usage-token` deletes the
  token and leaves the config alone, so a provider whose source was that
  token is hidden until another source is chosen in the window.

The files and hosts per provider are in [PROVIDERS.md](PROVIDERS.md) and in
`src/consent.rs`, the text the questions show; the paths are resolved in
`src/config/paths.rs`.

The Rust structs live in `src/config/schema.rs`, and those of `[tune]` in
`src/config/tune.rs`: these two files are the single source of truth for
the schema.
