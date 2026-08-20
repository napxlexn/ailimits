# Changelog

Notable, user-visible changes. Dates are release dates.

## 0.6.2 - 2026-08-21

### Added

- **Terminal installs.** A one-line PowerShell install
  (`irm .../install.ps1 | iex`) that verifies the installer against the
  release digest before running it, a Chocolatey package, and a Scoop
  manifest. A portable zip (the two exes plus the licence texts, no
  installer) is now attached to every release.

### Changed

- **A copy not managed by the installer no longer self-updates.** Running
  from a Scoop directory, an unpacked zip or a dev build, the auto-updater
  used to install a second copy into the regular install directory and leave
  the running one stale. It now recognises that the copy is not the one the
  installer put on disk, logs the available version, and leaves the update to
  whatever installed it. Installer copies, including those in a custom
  directory, update exactly as before.

## 0.6.1 - 2026-08-20

### Added

- **Widget width.** The rows layout can be shown at 100%, 75% or 50% of its
  natural width, from the context menu — useful when the widget shares a corner
  with something else.
- **Column arrangement.** With vertical progress bars the providers can sit side
  by side or stacked, flipping the widget between a wide, short shape and a
  narrow, tall one.
- **The taskbar panel can now follow a display other than the primary one.**
  On a multi-monitor setup where Windows shows the taskbar on more than one
  display, **Indicator → Display** picks which taskbar the panel attaches to.
  It falls back to the primary taskbar if the chosen display is later
  disconnected.
- **`panel_offset_x` / `panel_offset_y`** are new config-only settings that
  nudge the taskbar panel's position in pixels — a repair tool for unusual
  taskbar layouts, not exposed in the menu.

### Changed

- **The tray icon is now two rings.** The busiest provider is the outer ring,
  the runner-up the inner one, each filling clockwise from 12 o'clock. It is
  monochrome and inks itself in the system taskbar theme, so it reads on a light
  or a dark bar. A single provider shows the outer ring alone, so nothing moves
  when a second one starts reporting.
- **The panel's hover tooltip matches the shell's.** Size, padding, corner
  radius, text weight and the drop shadow were measured against a real Windows
  tooltip instead of estimated, so it now sits alongside the system's own
  tooltips rather than merely near them.

### Fixed

- **90% no longer looks like 100% in the tray.** A round line cap paints past
  the end of an arc — nearly a tenth of the circumference on the inner ring — so
  a nearly-full gauge closed completely and could not be told from a full one.
  That overhang is now subtracted, and only a true 100% closes a ring.
- **The panel comes back after the Start menu closes.** It could otherwise stay
  gone for the rest of the session, with the tray icon standing in until the app
  was restarted.
- **The panel no longer jumps to another display when Start opens**, and it
  settles into place after the taskbar's slide animation instead of mid-flight.
- **The panel survives an Explorer restart** — its taskbar watch re-points
  itself at the new bars instead of tracking windows that no longer exist.
- **A panel parked by a fullscreen app can be revived without restarting the
  app**, by switching its display.
- **The taskbar panel no longer overlaps the clock on taskbars that expose no
  notification area.** Some secondary Windows 11 taskbars have no
  `TrayNotifyWnd` to measure, so the panel now reserves a fixed width for the
  clock on those bars instead of assuming it can use the full edge.
- **A failed taskbar panel present is now logged instead of vanishing
  silently**, so a stuck or missing overlay leaves a diagnosable trace.
- **Antigravity showed a full quota while the weekly pool was spent.** The
  widget could not read the account's Code Assist project id, and Google
  answers project-less quota requests with a default view where every bucket
  reads full — so an exhausted Gemini allowance rendered as 0% used. AI Limits
  now identifies itself the way Antigravity's own client does, reads the shared
  quota pools ("Gemini Models", "Claude and GPT models") that Antigravity
  meters today, and reports an honest error instead of a quota it cannot
  verify.
- **A spent weekly limit now shows everywhere, not just on the widget.** The
  taskbar panel, the tray icon, both tooltips and the threshold notifications
  kept reporting the 5-hour session gauge — which reads low precisely when a
  spent weekly cap has already blocked new sessions. Every surface now shares
  one rule, and each limit window is classified explicitly, so Claude's Opus
  and Sonnet weekly pools count as well.
- **A failed update no longer closes the app.** If the installer handoff could
  not start, AI Limits exited anyway without updating; it now stays on the
  current version and logs the reason. The handoff also uses the absolute
  system command interpreter and relaunches the installed binary, so a copy
  running from another folder can no longer reinstall the same update forever.
- **Settings changed immediately before quitting are no longer lost** — the
  configuration is written synchronously as the window closes.

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
