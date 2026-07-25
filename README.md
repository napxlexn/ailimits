# AI Limits

A tiny native Windows 11 floating overlay that shows your AI provider usage
limits right on the desktop: **Claude**, **OpenAI Codex**, **GitHub Copilot**,
**Google Antigravity**.

🇺🇦 [Українська версія / Ukrainian version](README.uk.md)

<p align="center">
  <img src="docs/images/widget-detail.gif" width="380" alt="The floating widget switching live between the Compact, Medium and Expanded detail levels, with horizontal progress bars">
  <br><sub>The floating widget — a translucent, theme-aware acrylic card. Three detail levels (Compact · Medium · Expanded) switch live, here with horizontal progress bars.</sub>
</p>

<p align="center">
  <img src="docs/images/widget-detail-vertical.gif" width="380" alt="The same detail levels with vertical progress bars">
  <br><sub>…and the same three levels with the alternative vertical-bar layout.</sub>
</p>

<p align="center">
  <img src="docs/images/palette.gif" width="340" alt="The widget cycling through random readable looks — the monochrome and eight color palettes with the brightness, saturation and background-opacity sliders all varied">
  <br><sub>The look is yours: the monochrome and eight color palettes, plus the Brightness, Saturation and Background-opacity sliders — cycling through random readable presets, rendered exactly as the app draws them.</sub>
</p>

<p align="center">
  <img src="docs/images/indicator.gif" width="440" alt="The taskbar indicator: the cursor moves from the desktop onto the panel and then the tray pie, the hover tooltip appears over each at the same height, and every mode switches between the dark and light Windows themes">
  <br><sub>The taskbar indicator — when the overlay is hidden, a compact readout stays on the taskbar. The cursor reveals the hover tooltip over both the panel and the tray pie, and every mode follows the dark / light Windows theme.</sub>
</p>

## Why this widget

Every claim below is measured or verifiable, not marketing.

- **Secure by design.** It only **reads** tokens the official CLI tools
  already store on your machine — never refreshes or rotates them, so it
  can't log you out. Your own keys (if you add any) live only in the
  **Windows Credential Manager**. No telemetry: it talks only to the
  providers' own endpoints.
- **Autonomous while you work.** Your daily CLI use keeps the tokens fresh,
  so the widget shows live data with no setup and no logins of its own.
- **Effectively no system load — audited, not claimed.** Measured on the
  running app: **0.024% of one CPU core**, **zero GPU use** (CPU rendering),
  ~60 MB RAM, no raised timer resolution, ~3 MB exe. Reproduce it:
  `bench/perf_audit.ps1`; full numbers in
  [docs/en/ARCHITECTURE.md](docs/en/ARCHITECTURE.md#resource-footprint-measured-not-estimated).
- **Honest data.** A stale value greys out and shows its age; a provably
  reset window shows an `≈0%` estimate. It never invents numbers.
- **Self-contained.** Statically linked — no Visual C++ Redistributable, no
  bundled DLLs. The installer needs no admin rights.

## Install

Download `AiLimits-Setup-<version>.exe` from
[Releases](https://github.com/napxlexn/ailimits/releases) and run it.
No admin rights needed (installs to `%LOCALAPPDATA%\AiLimits`).
Optional during setup: autostart with Windows, a desktop shortcut.

> The installer is **not code-signed**, so Windows SmartScreen may show
> "Windows protected your PC". Click **More info → Run anyway**. You can
> verify the download against the SHA-256 published on each release.

The installer ships clean — no keys, no tokens; on first launch the widget
auto-detects the auth sources described below.

## Quick start

1. **Run it.** On first launch it silently finds the CLIs you already use —
   Claude Code, Codex, `gh`, Antigravity CLI, and supported legacy Gemini CLI
   sessions — and starts showing live usage. No login, no configuration.
2. **Place it.** Drag with the left button; **Shift** while dragging snaps
   to the nearest screen edge; `Lock position` (right-click) pins it.
3. **Choose the look.** Right-click for the detail level (Compact / Medium /
   Expanded), the bar layout (Vertical / Horizontal), a palette (monochrome or
   8 colors), and opacity / brightness / saturation — see the shots above.
4. **Hover a provider row** to swap its reset countdown for its weekly limit.
   A greyed or `≈` row explains itself on hover instead — the reason and the
   fix (e.g. `token expired — run Claude Code`).
5. **Fullscreen or gaming?** Right-click → **Indicator** → *Panel* or *Tray*
   keeps the usage on the taskbar while the overlay is hidden or covered —
   see [Taskbar indicator](#taskbar-indicator).
6. **Left-click the tray icon or the panel** to bring the widget to the front
   (or hide it); **right-click anywhere on the panel** for the same menu.

## How authorization works

The widget never asks you to log in. It monitors **subscription** limits by
reusing the tokens the official CLI tools keep on your machine, strictly
read-only:

| Provider | Automatic source (default) | To enable it |
|---|---|---|
| Claude | `%USERPROFILE%\.claude\.credentials.json` (Claude Code OAuth token) | run [Claude Code](https://claude.com/claude-code) at least once |
| Codex | `%USERPROFILE%\.codex\auth.json` (Codex CLI token) | run `codex login` once |
| Copilot | `gh auth token` (GitHub CLI) | `gh auth login` once |
| Antigravity | Windows Credential Manager target `gemini:antigravity`; fallback `%USERPROFILE%\.gemini\oauth_creds.json` | run Antigravity CLI once, or use a still-supported Gemini CLI flow |

These are short-lived **OAuth access tokens**; the owning CLI refreshes them
whenever you use it. The widget never refreshes them itself — an OAuth
refresh rotates the token and would log the CLI out.

### How long it stays accurate without any action

| Provider | Live data lasts | After that |
|---|---|---|
| **Claude** | ~8 hours after your last Claude Code session (the token's real lifetime) | falls back to `statusline.jsonl`, else shows the last value greyed |
| **Codex** | while you actively use Codex CLI (its token is shorter-lived) | the last value greyed, with its age |
| **Copilot** | **indefinitely** — gh keeps its token fresh as long as you're logged in | — |
| **Antigravity** | while Antigravity CLI refreshes its keyring token, or a supported Gemini CLI flow refreshes `oauth_creds.json` | the last value greyed, with its age |

Google stopped serving Gemini CLI requests for individual, Google AI Pro, and
Google AI Ultra users on June 18, 2026. Those users are expected to migrate to
Antigravity CLI; the widget reads Antigravity's Windows Credential Manager
token before trying the legacy Gemini CLI file.

**Claude has a server-side freshness ceiling:** Anthropic rate-limits the
usage endpoint (HTTP 429, `Retry-After: 0`, verified 2026-07-09), and the
per-token bucket is shared with Claude Code's own polling — so while Claude
Code is active, a Claude refresh can occasionally take 2–3 minutes even on
the 1-minute interval. The bounced cycle retries a minute later; the other
providers are unaffected.

An expired Claude/Codex token never means wrong numbers: the last value
stays greyed with its age, a provably reset window shows `≈0%`, and the
reset countdown keeps ticking locally. Use the CLIs daily and you see live
data almost all the time.

### Manual authorization (for independence or no-CLI setups)

For no-CLI setups, or a credential that doesn't depend on CLI activity.
What each option is actually worth:

| Provider | Manual option | Where to get it | Shows the same as the subscription? |
|---|---|---|---|
| **Copilot** | Personal Access Token (can be no-expiry) | github.com/settings/tokens → *Generate new token* | **yes** — the real "set and forget" option |
| **Claude** | API key | console.anthropic.com → Settings → API Keys → *Create Key* (`sk-ant-…`) | **no** — this shows your **API account** rate limits, not your Pro/Max subscription % |
| **Codex** | — | (no separate stable token exists; the usage token is the same short-lived OAuth token) | — |

**To add a credential — two ways:**

1. **From the clipboard (easiest).** Copy the key/token, right-click the
   widget → **Providers** → the provider → the matching `Paste … from
   clipboard` item (*Copilot*: PAT; *Claude*: API key — switches the method —
   or a usage token, tried before the auto-detected one; *Codex*: usage
   token). A toast confirms; the secret goes straight into the Credential
   Manager.
2. **From the terminal** — `ailimits-auth.exe` sits next to the widget
   (`%LOCALAPPDATA%\AiLimits`):
   ```
   ailimits-auth status                       # show every detected source
   ailimits-auth set copilot                  # store a PAT (hidden input)
   ailimits-auth set claude                   # store a Claude API key
   ailimits-auth set-usage-token claude|codex # store a manual usage token
   ailimits-auth remove copilot|claude
   ailimits-auth remove-usage-token claude|codex
   ```
   `set-usage-token` first sends one request to the provider's usage
   endpoint and only stores the token if it is accepted.

**To go back to automatic:** remove the key from the menu (`Remove key`) or
the CLI — the provider switches back to its subscription sources.

## The context menu

Right-click the widget: detail level (Compact / Medium / Expanded), layout
(Vertical / Horizontal), `Lock position` and `Always on top` (two independent
toggles), Palette (monochrome + 8 colors), Background opacity, Brightness,
Saturation, Forecast (burn-rate "~time to limit", off by default), Update
interval (1 / 5 / 15 / 30 min), `Automatic updates` (silent background
self-update, on by default), Indicator, per-provider settings, Quit.

### Taskbar indicator

Even with the overlay hidden or covered by a fullscreen app, a compact usage
readout stays visible. Right-click → **Indicator**:

- **Tray icon** — a single 16px system-tray pie of the highest current %.
- **Panel (rows / grid)** — a transparent overlay next to the clock: no
  window or box, just clock-sized digits and bars over the taskbar.
  Monochrome, follows the system light/dark theme, shows the first two
  providers in the widget's order, and tracks the taskbar live (auto-hide,
  resolution, tray width). **Hover** it for a shell-style tooltip with every
  provider. When **Start / Search** or the auto-hide bar covers it, it falls
  back to a tray pie and returns on its own; when a **fullscreen app** (a
  game, an F11 browser) takes the screen, it hides with the taskbar and
  returns the moment you alt-tab back to the desktop.
- **Off** — no indicator.

Left-clicking the tray icon or the panel brings the widget to the front (or
hides it if it is already there, on top); right-clicking **anywhere on the
panel** opens this same menu.

## Config & diagnostics

`%APPDATA%\AiLimits\config.toml` — created automatically; every menu change
is persisted there. Format reference: [docs/en/CONFIG.md](docs/en/CONFIG.md).

Set `AILIMITS_LOG=ailimits=debug` or `RUST_LOG=ailimits=debug` to write a
diagnostic log to `%APPDATA%\AiLimits\ailimits.log` (stderr is invisible in a
GUI app).

## Build from source

```
# Rust 1.86+, Windows 11
cargo build --profile release-min     # ~3 MB exe in target/release-min/
cargo test
```

Developer docs: [docs/en/](docs/en/) (English),
[docs/uk/](docs/uk/) (Ukrainian).

## License

Copyright (C) 2026 napxlexn.

AI Limits is free software licensed under the
[GNU General Public License, version 3 or later](LICENSE). You may use,
study, modify, and redistribute it. If you distribute a modified version,
you must provide its corresponding source code under the same license,
preserve the copyright and license notices, and clearly mark your changes.

The GPL license for the software does not grant rights to present modified
builds as official AI Limits releases. See [TRADEMARKS.md](TRADEMARKS.md)
for the rules governing the project name and logo. Third-party components
remain subject to their respective licenses.
