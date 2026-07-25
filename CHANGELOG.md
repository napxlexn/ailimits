# Changelog

Notable, user-visible changes. Dates are release dates.

## 0.6.0 - 2026-07-25

### Added

- **Automatic updates.** AI Limits now checks GitHub for new releases in the
  background and installs them silently, then restarts itself — no manual
  download. Each installer is verified against the release's published SHA-256
  before it runs; a mismatch is refused. Toggle it any time from the context
  menu (**Automatic updates**, on by default); the choice is saved to
  `config.toml` as `auto_update`.

### Fixed

- **Claude: a maxed weekly limit now shows on the bar itself, not only on
  hover.** When the weekly allowance was exhausted the widget kept the bar on
  the 5-hour session window — which reads low (or drops out) exactly when the
  weekly cap has already blocked new sessions — so the overlay looked far from
  full until you hovered. The bar now surfaces the weekly window whenever it is
  exhausted, matching how Codex already behaved.

## 0.5.3 - 2026-07-23

### Initial public release

- Published AI Limits as free software under the GNU General Public License,
  version 3 or later (`GPL-3.0-or-later`).
- Added a separate trademark policy for the AI Limits name and logo so
  modified builds cannot be presented as official project releases.
- Included the license and trademark policy in the source archive and Windows
  installer.

### Providers and interface

- Shows live usage limits for Claude, OpenAI Codex, GitHub Copilot, and Google
  Antigravity using the authentication sources already stored by official CLI
  tools.
- Provides a configurable floating Windows 11 widget plus tray and taskbar
  indicators, stale-data explanations, reset countdowns, and optional
  burn-rate forecasts.
- Stores optional user-supplied credentials in Windows Credential Manager and
  sends no telemetry.

### Reliability and maintenance

- Bounds external `gh` and user-hook processes with timeouts.
- Serialises and coalesces configuration writes so rapid changes cannot be
  persisted out of order or grow an unbounded queue.
- Declares Rust 1.86 as the minimum supported toolchain and excludes unused
  clipboard-image support from the Windows dependency tree.
- Keeps the Windows build free of the XML dependency covered by published
  RustSec advisories and documents the remaining platform-specific audit triage.
- Ships with formatting, Clippy, tests, RustSec audit, release build, WinGet
  validation, and reproducible resource-audit tooling.
