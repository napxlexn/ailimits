# Changelog

Notable, user-visible changes. Dates are release dates.

## Unreleased

### Fixed

- **The taskbar mini panel keeps following an auto-hide bar after Explorer
  restarts.** The move/auto-hide watch is a WinEvent hook scoped to
  Explorer's process; when Explorer is killed and started again (or crashes
  and comes back) the new process has a new id and the hook never fires
  again, so the panel only moved on the 60-second tick or when some other
  window change nudged it - a laggy panel drawn over a hidden bar and vice
  versa. The hook is now re-scoped to the new process the moment the new
  taskbar is seen.
- **A config file that cannot be read no longer ends the widget silently.**
  A read error (permissions, a scanner's lock, bytes that are not UTF-8) was
  the one config failure that propagated out of startup, and a GUI app has
  no stderr to say so. It is now handled like an unparseable file: one retry
  for a passing lock, then the file is set aside as `config.toml.corrupt`,
  the reason is logged and the widget runs on defaults.
- **Toasts are attributed to AI Limits.** Threshold alerts and menu feedback
  were shown under "Windows PowerShell", the identity a desktop app without
  one of its own borrows. The app now registers its own per user (name and
  icon; the installer's shortcut carries the same id), and the Store build
  notifies under the package's identity. Settings > Notifications lists
  "AI Limits" as a result, so its toasts can be muted on their own.
- **The opt-in diagnostic log rotates at 5 MB** (`ailimits.log` ->
  `ailimits.log.1`, one predecessor kept) instead of growing without bound
  for as long as `AILIMITS_LOG` stays set.

## 0.6.4 - 2026-09-12

### Fixed

- **The compact widget's name column fits every provider name.** It was 48px
  wide and "Antigravity" at the compact size runs 52.6px, so the name reached
  into the gap before its bar. The column is 56px now, in the app and in the
  site's renderer; every width step still lands on the same tier.

## 0.6.3 - 2026-09-11

### Added

- **A Microsoft Store package.** The same `ailimits.exe` the installer ships,
  packaged as MSIX (`installer/msix/`). Inside the package the app knows it
  is packaged and leaves two things to the Store: the tray-icon registry
  write (a packaged process's HKCU writes never reach Explorer, and the
  Store policy asks that settings not change without the user's say) and
  the silent self-update (the package directory is read-only; the Store
  updates it). Start at sign-in is the package's startup task. Installer,
  Scoop and portable copies behave exactly as before.
- **A privacy page** on the site, written from the code: which logins the
  widget reads and where each lives, which four provider hosts it sends
  them to, what it stores, and what each uninstall route leaves behind.

### Changed

- **The published footprint is the August audit.** READMEs, both
  architecture documents and the Chocolatey description quoted the July
  run (0.024% of one core, 61 MB); the 2026-08-23 audit of the same window
  reads 0.005% and 37 MB.
- **Chocolatey and winget metadata** point at the site as the project's
  home, and the Chocolatey icon comes from a CDN pinned to the release tag,
  as the repository's moderator asked.

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
