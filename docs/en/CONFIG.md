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
# Taskbar indicator (menu: Indicator): "tray" — a 16px tray pie of the
# highest %; "panel_rows" / "panel_grid" (legacy alias) — a transparent
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
# Burn-rate forecast ("~Xh to limit" when usage is climbing; menu: Forecast).
# Never replaces the reset countdown — shown only when no future reset is known.
show_forecast = false

[[providers]]
# Identifier: claude / codex / copilot / antigravity ("gemini" is a legacy
# id, renamed to "antigravity" on load).
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

The Rust structs live in `src/config/schema.rs` — that file is the single
source of truth for the schema.
