# CONFIG.md — the configuration file

Location: `%APPDATA%\AiLimits\config.toml`. Created automatically on the
first launch; every context-menu change is persisted here. Unknown fields
and providers are ignored (old configs never break parsing). On a parse
error the widget logs a warning and falls back to the defaults.

Next to it the widget may keep `provider-cache.json` (last known metrics
with a future reset — they survive restarts), `history.jsonl` (bounded usage history feeding the
Expanded sparkline) and, when `AILIMITS_LOG` or `RUST_LOG` is set,
`ailimits.log`.

## Full example

```toml
[general]
# Update interval in seconds (menu: 60 / 300 / 900 / 1800; hard minimum 60).
# Polling pauses while idle/locked and backs off when a whole cycle fails;
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
# Automatic updates (menu: Automatic updates). When true, AI Limits checks
# GitHub for a newer release in the background (~30s after start, then daily),
# verifies the installer against the release's published SHA-256, installs it
# silently and restarts. Set false to disable; missing in old configs → true.
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
# meters it on its own: claude.opus, claude.sonnet (weekly, on plans that
# have them), antigravity.cg (the Claude and GPT pool), grok.build,
# cursor.bot (Grok Bot). The widget and the tooltip are chosen separately;
# the taskbar panel and the tray icon keep one entry per provider. Unknown
# ids are ignored and kept.
[ui.rows]
widget  = ["claude", "claude.opus", "codex"]
tooltip = ["claude", "claude.opus", "claude.sonnet", "codex", "grok"]

# Model labels (menu: Rows and models...). Parts per surface: "model",
# "version" (only with "model"), "effort" (may go alone). Empty: no label.
# Showing them reads each tool's settings file, so the first tick asks
# first ([reads] tool_settings). The widget draws the label on a line of its
# own in Expanded and in the columns layout, and in what the reset leaves of
# the meta line in Medium; Compact leaves it to the tooltip. Narrow places
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
# new model needs no update. A name must fit the narrowest place the widget
# draws a model name, about ten ordinary characters; the picker stops typing
# there. An empty name brings the automatic one back.
[ui.model.names]
"claude-opus-5-5" = "Opus 5.5"

[[providers]]
# Identifier: claude / codex / copilot / antigravity / grok / cursor ("gemini"
# is a legacy id, renamed to "antigravity" on load). A default provider missing
# from an older config is appended on load, enabled.
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
# Cursor: the widget's own session (menu: Sign in to Cursor...), kept in
# Credential Manager; without one, the editor's token while it is stored
# unencrypted. The row stays hidden until one of them exists.
id = "cursor"
enabled = true
auth_method = "subscription"
alert_threshold = 80

[notifications]
enabled = true
# Per-provider toast cooldown, minutes.
cooldown_minutes = 15

[hooks]
# Optional shell commands run on usage events (empty = disabled, the
# default). Each runs detached via `cmd /C`, hidden, fire-and-forget; the
# event context arrives in environment variables: AILIMITS_EVENT
# (threshold|reset|startup), AILIMITS_PROVIDER, AILIMITS_PERCENT,
# AILIMITS_RESET_AT (RFC3339, when known).
# SECURITY: these are deliberately config-file-only — there is no menu or
# clipboard path, so nothing can auto-populate a command to run.
# Fires when a provider crosses its alert threshold (shares the toast cooldown):
on_threshold = ""
# Fires when a near-limit provider's window resets (usage drops sharply):
on_reset = ""
# Fires once after the first successful fetch:
on_startup = ""

# What the app may read of other tools' files. Nothing of theirs is read
# before it is allowed here, and each is asked once, naming the file and the
# only host the token goes to (see "What is read, and when" below).
[reads]
# Providers whose sign-in may be read. Ticking a provider in the picker or
# in the Providers menu asks first; unticking every row of a provider in the
# picker removes it from here again (and switches it off).
sign_in       = ["claude", "codex", "grok"]
# Providers set up only with the app's own sign-in: a key or usage token you
# pasted (kept in Credential Manager) or the menu's Cursor sign-in. They use
# that alone and read nothing of the tool's files; when it is removed they
# show nothing rather than fall back to the tool's files. Written only when
# not empty.
app_sign_in   = ["cursor"]
# The tools' settings files, for the model and effort ([ui.model]).
tool_settings = false
# The end of the newest Claude Code / Codex CLI session file, for the model
# last used ([ui.model] source = "last_used").
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

## What is read, and when

Every read of another tool's file is optional and asked for first:

- **A fresh install reads nothing.** The widget and the tray say "Choose what
  to show", and the picker opens by itself with the tools found on this PC
  marked (found by whether their folder exists; nothing inside is opened).
  The first tick of each tool asks once, naming the file and the host.
- **After updating from 0.7.0 or older** nothing stops: the providers that
  were switched on keep reading as before (`sign_in` is filled with them),
  and one window lists every file read and every host contacted, once.
- **A config that cannot be read** is set aside as `config.toml.corrupt` and
  the app starts as a fresh install: what the lost file allowed is unknown,
  so nothing is read until chosen again. An empty config, or one with no
  `[[providers]]`, counts as a fresh install too.
- **A fresh install switches every provider off** as well as allowing
  nothing, so even an older version started on the same config afterwards
  reads only what was allowed.
- **To stop reading a tool**, untick all of its rows in Rows and models (or
  untick Enabled in the Providers menu). Model labels stop being read when
  Model, Version and Effort are all unticked.
- **Where the tools keep their files** can be moved by `CODEX_HOME`,
  `GROK_HOME` and `COPILOT_HOME`; the app follows them, and the questions say
  so.
- **`ailimits-auth status`** is a diagnostic you run yourself: it checks each
  tool's sign-in directly (expiry and presence only, never printing a token)
  and does not go by `[reads]`.

The files and hosts per provider are in [PROVIDERS.md](PROVIDERS.md) and in
`src/consent.rs`, the text the questions show.

The Rust structs live in `src/config/schema.rs` — that file is the single
source of truth for the schema.
