# Changelog

Notable, user-visible changes. Dates are release dates.

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
