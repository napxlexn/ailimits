# AI Limits

<p align="center">
<a href="https://napxlexn.github.io/ailimits/"><img alt="Platform" src="https://img.shields.io/badge/platform-Windows%2011-0078d4?style=flat-square"></a>
<a href="https://www.rust-lang.org/"><img alt="Built with" src="https://img.shields.io/badge/built%20with-Rust-dea584?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/blob/master/LICENSE"><img alt="License" src="https://img.shields.io/github/license/napxlexn/ailimits?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/napxlexn/ailimits/total?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/napxlexn/ailimits/ci.yml?branch=master&style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/napxlexn/ailimits?style=flat-square"></a>
<a href="https://github.com/microsoft/winget-pkgs/tree/master/manifests/n/napxlexn/AILimits"><img alt="winget" src="https://img.shields.io/winget/v/napxlexn.AILimits?style=flat-square"></a>
<a href="https://community.chocolatey.org/packages/ailimits"><img alt="Chocolatey" src="https://img.shields.io/chocolatey/v/ailimits?style=flat-square"></a>
<a href="https://github.com/napxlexn/scoop-ailimits"><img alt="Scoop" src="https://img.shields.io/scoop/v/ailimits?bucket=https%3A%2F%2Fgithub.com%2Fnapxlexn%2Fscoop-ailimits&style=flat-square"></a>
<a href="https://apps.microsoft.com/detail/9P2B0J1VWPWH"><img alt="Microsoft Store" src="https://img.shields.io/badge/dynamic/regex?url=https%3A%2F%2Fdisplaycatalog.mp.microsoft.com%2Fv7.0%2Fproducts%3FbigIds%3D9P2B0J1VWPWH%26market%3DUS%26languages%3Den-us&search=AILimits_(%5Cd%2B%5C.%5Cd%2B%5C.%5Cd%2B)&replace=v%241&label=Microsoft%20Store&style=flat-square"></a>
</p>

A tiny native Windows 11 floating overlay that shows your AI provider usage
limits right on the desktop: **Claude**, **OpenAI Codex**, **GitHub Copilot**,
**Google Antigravity**, **xAI Grok**, **Cursor**.

<p align="center"><a href="https://napxlexn.github.io/ailimits/"><img alt="Open the website" src="https://img.shields.io/badge/OPEN%20THE%20WEBSITE-0078d4?style=for-the-badge"></a></p>

<p align="center"><a href="README.uk.md">Українська версія / Ukrainian version</a></p>

Available on the [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH), signed and updated by the Store, and listed on [AlternativeTo](https://alternativeto.net/software/ai-limits/about/) — if it helps you, a like there helps others find it.

<p align="center">
<a href="https://apps.microsoft.com/detail/9P2B0J1VWPWH"><img alt="Get it from Microsoft" src="https://get.microsoft.com/images/en-us%20dark.svg" height="52"></a>
</p>

<p align="center">
  <img src="docs/images/widget-detail.gif" width="380" alt="The floating widget switching live between the Compact, Medium and Expanded detail levels, with horizontal progress bars">
  <br><sub>Three detail levels: Compact, Medium, Expanded. Switching is live; the optional burn-rate forecast (~time to limit) appears on the meta line where no reset time is known. Horizontal bars here.</sub>
</p>

<p align="center">
  <img src="docs/images/indicator.gif" width="440" alt="The taskbar indicator: the cursor moves from the desktop onto the panel and then the tray rings, the hover tooltip appears over each at the same height, and every mode switches between the dark and light Windows themes">
  <br><sub>With the widget hidden, usage stays on the taskbar. The tray icon shows the same two providers as the panel as two rings: the first two in the widget, or the two you choose. Hovering the panel or the tray icon gives the same list: the rows ticked in the Tooltip column. Both follow the system light and dark theme.</sub>
</p>

<table>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-detail-vertical.gif" height="230" alt="The same detail levels with vertical progress bars">
  <br><sub>The same three levels with vertical bars.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-width.gif" height="230" alt="The widget narrowing through its width steps; at the narrowest the provider names drop out and the bars take their place">
  <br><sub>Three width steps. At the narrowest, provider names are dropped and the bar takes their column, so nothing overlaps.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-opacity.gif" height="230" alt="The widget background going from almost fully transparent to almost solid while the text stays readable">
  <br><sub>Background opacity, from near-clear to near-solid. Text and bar contrast stay put.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-providers.gif" height="230" alt="The widget growing from a single provider row to four and back">
  <br><sub>The card is sized to the rows you show: one per provider you enabled, plus any extra rows you choose.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/palette.gif" height="230" alt="The widget cycling through the monochrome and eight colour palettes with the brightness, saturation and background-opacity sliders varied">
  <br><sub>Monochrome plus eight colour palettes, with brightness, saturation and background opacity. Rendered by the app itself.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-arrangement.gif" height="230" alt="The vertical-bar layout switching between providers side by side and providers stacked into a tall narrow column">
  <br><sub>With vertical bars, providers sit side by side or stack into a column.</sub>
</td>
</tr>
</table>

## What it does

- **Reads, never writes.** It reads the tokens the official CLI tools already
  store on your machine. It never refreshes or rotates them, so it cannot log
  you out of those tools. Cursor is the one exception: the app signs in to it
  with a session of its own and never reads or touches the editor's sign-in.
  Keys you add yourself, and that session, are stored in Windows Credential
  Manager. The app connects only to the providers' own endpoints (and, for
  an installer copy, GitHub for updates), makes one usage request per
  provider per cycle (plus the few helper requests listed in
  [docs/en/PROVIDERS.md](docs/en/PROVIDERS.md#requests-per-cycle)), follows
  the proxy set in Windows, and sends no telemetry.
- **Asks before it reads.** Nothing that belongs to another tool is read
  until you allow it, and each question names the file and where its token
  is sent. Each is asked once, and *Forget permission* takes it back in one
  click. See [What is read, and when](docs/en/CONFIG.md#what-is-read-and-when).
- **Little setup.** On first launch the tools found on this PC are already
  ticked, and one question names every sign-in file it would read: *Allow*
  is enough. After that your normal CLI use keeps their tokens valid, so
  there is nothing else to configure. Only Cursor asks you to sign in once,
  in *Settings*.
- **Low load, measured on the running app.** 0.014% of one CPU core, no GPU
  use (it renders on the CPU), a 53 MB working set (25 MB private), measured
  for 0.7.1 on 2026-11-23; no raised timer resolution (2026-11-21); a 4 MB exe. Reproduce it with
  `bench/perf_audit.ps1`; the full numbers are in
  [docs/en/ARCHITECTURE.md](docs/en/ARCHITECTURE.md#resource-footprint-measured-not-estimated).
- **Stale data is marked as stale.** A value that stops updating greys out and
  shows its age. A limit window that has provably reset shows `≈0%`. The app
  does not fill gaps with guesses. Polling pauses while you are away, and that
  time does not count: nothing greys just because you stepped out.
- **Self-contained.** Statically linked, so no Visual C++ Redistributable and
  no bundled DLLs. The installer needs no admin rights.

## Install

> **Updating from 0.7.0 (or 0.6.x), and your user profile or TEMP folder
> path has letters outside English (Cyrillic, for example), `&` or `%` in
> it?** The old version's updater fails on such a path: once 0.7.1 is out it
> may close AI Limits about half a minute after every start. Install 0.7.1
> by hand: download `AiLimits-Setup-0.7.1.exe` from
> [Releases](https://github.com/napxlexn/ailimits/releases) and run it, or,
> once winget carries 0.7.1, run `winget upgrade napxlexn.AILimits`. If the
> widget closes before you get to it, right-click it right after a start and
> untick **Automatic updates** first (or set `auto_update = false` in
> `%APPDATA%\AiLimits\config.toml`). From 0.7.1 on the updater starts the
> installer directly, such paths work, and you can tick Automatic updates
> again.

From the terminal:

```powershell
# winget
winget install napxlexn.AILimits

# one-line install: downloads the latest release, verifies its SHA-256
# against the published digest (the SHA-256 only, not the .sig), runs the
# silent per-user install (no admin rights)
irm https://raw.githubusercontent.com/napxlexn/ailimits/master/install.ps1 | iex

# Scoop, from the project's own bucket
scoop bucket add ailimits https://github.com/napxlexn/scoop-ailimits
scoop install ailimits

# Chocolatey
choco install ailimits

# Microsoft Store (signed and updated by the Store), or https://apps.microsoft.com/detail/9P2B0J1VWPWH
winget install --id 9P2B0J1VWPWH --source msstore
```

Or get it from the [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH):
the same widget, signed and kept up to date by the Store (it may trail a
release by a few days while a new package is certified).

Or download `AiLimits-Setup-<version>.exe` from
[Releases](https://github.com/napxlexn/ailimits/releases) and run it.
No admin rights needed (installs to `%LOCALAPPDATA%\AiLimits`).
Optional during setup: autostart with Windows (ticked by default), a desktop
shortcut. winget and Chocolatey run this same installer, so their copy
updates itself like any installer copy, and the package manager may list an
older version than the one running.
A portable zip (no installer) is attached to each release as well. A portable
copy does not self-update: its package manager, or you, updates it. There is
no portable mode for settings: every copy, portable included, keeps them in
`%APPDATA%\AiLimits`.

> The installer is **not code-signed** (Authenticode), so Windows
> SmartScreen may show "Windows protected your PC". Click **More info → Run
> anyway**. You can verify the download against the SHA-256 published on
> each release (below). From 0.7.1 each release also carries an Ed25519
> signature (`.sig`) for the installer and the zip, and from 0.7.1 on the
> app's own updater installs nothing without a valid one. The 0.7.0 updater
> ignores the `.sig` and checks only the SHA-256 GitHub publishes for the
> installer: its update to 0.7.1 rests on that digest.

To check a download by hand, compare its SHA-256 with the `sha256:` digest
shown beside the file on the release page; the two must match:

```powershell
(Get-FileHash .\AiLimits-Setup-0.7.1.exe -Algorithm SHA256).Hash
```

To check the signature as well, put the `.sig` beside the file, both under
their published names (the signature covers the name). Then, in a clone of
this repository switched to the release tag (`git checkout v0.7.1`), run the
release signer's `verify`, which needs no key (Rust, as in
[Build from source](#build-from-source)):

```powershell
cargo run --locked --release --manifest-path tools/release-signer/Cargo.toml -- verify $HOME\Downloads\AiLimits-Setup-0.7.1.exe
```

It prints `verified`, or `NOT verified` with the reason.

The installer contains no keys or tokens. On first launch the widget reads
none of the auth sources listed below until you allow it.

### Uninstall

Uninstalling in Windows Settings, Apps, with
`winget uninstall napxlexn.AILimits` or with `choco uninstall ailimits` runs
the installer's uninstaller. It closes AI Limits and removes, offline: its
install folder, `%APPDATA%\AiLimits` (settings, cache, logs), every
Credential Manager entry the app made (keys, tokens, the Cursor session),
the autostart value and the notification registration.

Removing a Scoop or portable copy takes none of that:
`scoop uninstall ailimits`, or deleting the folder, removes only the app's
own files. Before you remove it, quit AI Limits and run, in its folder (a
command with nothing to remove does nothing):

```
ailimits-auth remove claude
ailimits-auth remove copilot
ailimits-auth remove-usage-token claude
ailimits-auth remove-usage-token codex
ailimits-auth logout cursor
```

Then delete `%APPDATA%\AiLimits` and the registry key
`HKCU\Software\Classes\AppUserModelId\napxlexn.AILimits` (the name and icon
the app's toasts show). A Microsoft Store copy: how to remove its keys
first is in [docs/en/CONFIG.md](docs/en/CONFIG.md#secrets); removing it
takes its own settings folder with it (the Store copy's folder is described
at the top of that file).

## Quick start

1. **Run it.** On first launch a welcome window opens (the same one as
   *Settings…* in the menu). Its question lists the tools found on
   this PC (Claude Code, Codex CLI, the Copilot CLI or GitHub CLI,
   Antigravity, the Grok CLI, Cursor), each with a
   tick, and names every file it would read and where each token goes.
   Tools found by a folder of their own start ticked; GitHub CLI alone (for
   Copilot) or `.gemini` alone (for Antigravity) is listed unticked.
   *Allow* starts the ticked ones; *Cancel* reads nothing and lets you tick
   what you want, each tool asking once. The window takes no keyboard focus
   (Enter allows nothing by accident) and waits while a fullscreen app is in
   front. Like *Settings*, it stays open until Esc, its cross or
   Alt+F4 closes it, and moves by its title. Cursor needs
   a sign-in of the app's own, once: in the same window, open the *Sign-in*
   line under Cursor and click *Sign in to Cursor…*.
   Updating from 0.7.0 needs nothing: the providers you had keep working,
   and one window lists what is read.
2. **Place it.** Drag with the left button. Hold **Shift** while dragging to
   pull it flush against the nearest edge of the monitor's work area (the
   screen without the taskbar) once it is within 64 px of that edge.
   `Lock position` in the right-click menu pins it.
3. **Set the look.** Right-click for detail level (Compact, Medium, Expanded),
   layout (Rows or Columns), palette (monochrome or 8 colors),
   opacity, brightness and saturation. The animations above show each one.
   Resting the pointer on one of these items, or on Width, Column layout or
   Usage forecast, shows it on the widget, the panel and the tray icon before you
   click, and so does moving to it with the keyboard; leaving the menu
   without a click puts it back.
   *Settings…*
   picks the rows the widget and the panel's tooltip show (extra rows such as
   Claude's Opus week included), an optional label with the model in use
   (model, version, reasoning effort) and where each provider's sign-in
   comes from. *Model source* picks where the label's model comes from:
   *Tool settings*, or *Last session*, the model of the newest Claude Code
   or Codex CLI session, read from the end of its file (at most the last
   1 MB); other tools show the model as set.
   The Taskbar column picks the two providers the panel and the tray icon
   show; at least one stays ticked. Percentage and Time to reset are ticked
   separately for the panel's tooltip (Tooltip column) and the panel
   (Taskbar column): the percentage, the time until the limit resets, or
   both; the last one left on cannot be unticked. Progress bar (on by
   default) can leave the panel's bar out to take less of the taskbar.
4. **Hover a provider row** to see its weekly limit, in place of its reset
   countdown (in Compact, on the bottom line).
   A greyed or `≈` row shows the reason and the fix instead, for example
   `token expired — run Claude Code`. The reason shows in full on Medium and
   Expanded rows and in Compact's bottom line (at the 100% and 75% widths),
   cut to fit under an Expanded column; Compact and Medium columns, and
   Compact at 50%, show none.
5. **Widget in the way?** Right-click, **Taskbar**, then *Taskbar panel*
   or *Tray icon*. With the widget hidden or covered by other windows, usage
   stays on the taskbar; while a fullscreen app has the screen, it hides with
   the taskbar and comes back when you leave the app. See
   [Taskbar indicator](#taskbar-indicator).
6. **Left-click the tray icon or the panel** to raise the widget or hide it.
   Right-click either one for the same menu.
7. **A message instead of rows?** *Setup required* or
   *No data yet*: click the widget to open *Settings*.
   *Right-click → set up providers*: providers are ticked, but none of them
   is signed in; check each provider's *Sign-in* in *Settings*.

## How authorization works

The widget never asks you to log in, except once for Cursor. It monitors
**subscription** limits by reusing the tokens the official CLI tools keep on
your machine, strictly read-only and only once you allow each one:

| Provider | Automatic source (default) | To enable it |
|---|---|---|
| Claude | `%USERPROFILE%\.claude\.credentials.json` (Claude Code OAuth token) | run [Claude Code](https://claude.com/claude-code) at least once |
| Codex | `%USERPROFILE%\.codex\auth.json` (Codex CLI token; in `CODEX_HOME` when it is set) | run `codex login` once |
| Copilot | `gh auth token --hostname github.com` (GitHub CLI) | `gh auth login` once |
| Antigravity | Windows Credential Manager target `gemini:antigravity` | run `agy`, the Antigravity CLI; its token lasts about an hour and only `agy` renews it (opening the editor does not) |
| Grok | `%USERPROFILE%\.grok\auth.json` (Grok CLI login; in `GROK_HOME` when it is set) | run `grok` and sign in once (a SuperGrok subscription) |
| Cursor | the widget's own session, in Windows Credential Manager (`cursor_session`); nothing of the Cursor editor is read for it | Sign in to Cursor once in *Settings* (Cursor's *Sign-in*, *Sign in to Cursor…*) or with `ailimits-auth login cursor`: Cursor's own sign-in page opens in the browser; ~60-day session |

Under each provider's rows on the *Providers* tab, *Settings* has a
*Sign-in* line that says where the sign-in comes from and how it went
("Claude Code · connected"). Open it to choose the source: the tool's
account above (*Claude Code account* and so on) or, for Claude,
Codex and Copilot, a key or token of your own (see Manual authorization
below). Only the source chosen is used; no other one stands in when it
fails.

Each tool is asked once. Clearing all of a provider's rows (Widget, Tooltip
and Taskbar) pauses it: nothing of it is read or sent, and a tick starts it
again without a question. To take the permission back, open its *Sign-in*
and click *Forget permission* (shown while the tool's own sign-in is the
source): its sign-in is not read any more, its rows are cleared, and the
next tick asks again. Cursor's session ends with *Sign out*, a pasted key or
token goes with *Remove*. The model label's reads have a *Forget
permission* of their own on the *Display* tab, under Model, Version and
Reasoning effort. Permissions are kept per Windows user, in that user's
`config.toml`.

Codex CLI writes `auth.json` only when it keeps its sign-in in a file: with
`cli_auth_credentials_store = "keyring"` or `"auto"` in its `config.toml`
there is no `auth.json`, and its sign-in cannot be read. Set it to `"file"`,
or paste a usage token instead.

These are OAuth access tokens, short-lived except Cursor's. The CLI that
owns each one refreshes it whenever you use it. The widget never refreshes
another app's token, because an OAuth refresh rotates it and would log the
CLI out. The Cursor session (about 60 days) belongs to the widget: you sign
in to Cursor in the browser, on Cursor's own page, and the widget keeps and
refreshes that session itself. Cursor has no sign-in for other apps, so the
widget signs in as the Cursor editor's client; it never reads the editor's
sign-in. `CODEX_HOME`, `GROK_HOME` and `COPILOT_HOME` count only when they
are absolute paths.

### How long it stays accurate without any action

| Provider | Live data lasts | After that |
|---|---|---|
| **Claude** | ~8 hours after your last Claude Code session (the token's real lifetime) | shows the last value greyed, with its age |
| **Codex** | while you use Codex CLI (how long its token lasts is not measured yet) | the last value greyed, with its age |
| **Copilot** | Indefinitely. gh keeps its token fresh while you stay logged in. | |
| **Antigravity** | about an hour after `agy` last ran (only `agy` renews its token) | the last value greyed, with its age |
| **Grok** | up to ~6 hours after your last Grok CLI session (the token's real lifetime) | the last value greyed, with its age |
| **Cursor** | about 60 days after you sign in (the session's lifetime) | the last value greyed, and `sign in to Cursor again` |

Google stopped serving Gemini CLI requests for individual, Google AI Pro, and
Google AI Ultra users on June 18, 2026. Those users are expected to migrate to
Antigravity CLI; the widget reads only Antigravity's Windows Credential
Manager token, and no longer the legacy Gemini CLI file. Google has suspended
some accounts for using Antigravity's sign-in outside Antigravity. The widget
only reads your quota, once per update, but the risk is yours: the question
that asks to read Antigravity's sign-in says so.

**Claude has a server-side limit on how often it can be read.** Anthropic
rate-limits the usage endpoint (HTTP 429, `Retry-After: 0`, verified
2026-07-09), and the per-token budget is shared with Claude Code's own
polling. While Claude Code is running, a Claude refresh can take 2 to 3
minutes even on the 1-minute interval. The next cycle asks again at the
update interval; the wait grows (2, 4, then at most 5 minutes at the
1-minute interval) only when no provider brought data in a cycle, and a
rate limit counts as none. Meanwhile, hovering the Claude row reads
`rate-limited, retrying`. Other providers are unaffected.

An expired Claude, Codex, Antigravity, Grok or Cursor token does not produce wrong numbers.
The last value stays greyed with its age, a window that has provably reset
shows `≈0%`, and the reset countdown keeps running locally. If you use the CLIs daily, the
data is live nearly all the time.

### Manual authorization (for independence or no-CLI setups)

For setups without the CLIs, or when you want a credential that does not
depend on CLI activity. What each option actually gives you:

| Provider | Manual option | Where to get it | Shows the same as the subscription? |
|---|---|---|---|
| **Copilot** | A fine-grained Personal Access Token; it needs no permissions at all (leave them empty) | github.com/settings/personal-access-tokens, *Generate new token* | Verified live on 2026-11-21: a token with no permissions reads the same quotas as GitHub CLI's sign-in. It needs no gh, until the expiry you set. |
| **Claude** | API key | console.anthropic.com, Settings, API Keys, *Create Key* (`sk-ant-…`) | No. It shows your API account rate limits, not your Pro or Max subscription usage. |
| **Codex** | None | No separate stable token exists. A pasted *Usage token* is the same kind of OAuth token as Codex CLI's. | |
| **Antigravity** | None | The quota is tied to the OAuth token that Antigravity CLI refreshes, and no separate key reports the same numbers. | |
| **Grok** | None | The weekly SuperGrok limit is read with the Grok CLI's own login only. | |
| **Cursor** | Its own sign-in, about 60 days | *Settings*, Cursor's *Sign-in*, *Sign in to Cursor…*, or `ailimits-auth login cursor` | Yes. It is the subscription's own usage. |

**Two ways to add a credential:**

1. **In *Settings*.** Copy the key or token. Open the provider's
   *Sign-in* line, choose the source (Claude: *Usage token* or *API key*;
   Codex: *Usage token*; Copilot: *Fine-grained token*) and click *Paste
   token from clipboard*
   (*Paste key from clipboard* for Claude's API key). Only that kind of
   secret is taken (`sk-ant-...`; `ghp_...`, `github_pat_...`, `gho_...`,
   `ghu_...` or a classic 40-character hex token; a ChatGPT token
   `eyJ...`): anything else on the clipboard is refused without being
   sent, and never shown or logged. A usage token is checked with one
   request first: a refused one is not stored. A key is stored as it is, and
   the row says whether it works. The window says what came of it (a toast,
   if the window was closed meanwhile), and the secret goes straight into
   Credential Manager. Only the source chosen is used: the tool's sign-in
   is not read while your token or key is chosen, and a stored token is not
   sent while the tool's account is chosen.
2. **From the terminal.** `ailimits-auth.exe` sits next to the widget in
   `%LOCALAPPDATA%\AiLimits`:
   ```
   ailimits-auth status                       # show every detected source
   ailimits-auth set copilot                  # store a PAT (hidden input)
   ailimits-auth set claude                   # store a Claude API key
   ailimits-auth set-usage-token claude|codex # store a manual usage token
   ailimits-auth remove copilot|claude
   ailimits-auth remove-usage-token claude|codex
   ailimits-auth login cursor                 # sign in to Cursor in the browser
   ailimits-auth logout cursor                # forget that session
   ```
   `set-usage-token` checks the token with one request to the provider's
   usage endpoint: a refused one (401/403) is not stored; a rate limit (429)
   counts as accepted; with no answer that says either way it asks again
   after 2 and 4 seconds, then stores it with a note. `status` changes
   nothing and reads another tool's sign-in only where the widget may. A
   running widget picks up what the other commands change by itself, with no
   restart: a changed setting within a few seconds, a new or removed key or
   token at its next update. `set` and `set-usage-token` make what they
   store the provider's source.

**To go back to the tool's sign-in:** in the provider's *Sign-in*, choose
the tool's account (*Claude Code account*, *Codex CLI account* or *GitHub
CLI account*). Your key or token stays stored but unused; *Remove*,
with its source chosen, deletes it. From the terminal,
`ailimits-auth remove claude|copilot` deletes the key and switches back;
`remove-usage-token` deletes the token but leaves the source on it, so the
row hides until you choose the tool's account in the window.

## The context menu

Right-click the widget:

- `Settings…`: rows, model labels, model aliases, each provider's sign-in.
- **Detail**: Compact / Medium / Expanded.
- **Layout**: Rows / Columns, then Width (100 / 75 / 50%, greyed unless
  Rows) and Column layout (Side by side / Stacked, greyed unless Columns).
- **Appearance**: Palette (8 colors), Monochrome, Background opacity,
  Brightness, Saturation.
- **Taskbar**: Tray icon / Taskbar panel / None, and Panel display when
  there is more than one taskbar.
- `Always on top` and `Lock position` (two independent toggles), Usage
  forecast (burn-rate "~time to limit", off by default).
- Refresh interval (1 / 5 / 15 / 30 min) and `Automatic updates` (silent
  background self-update of an installer copy, on by default; Store, Scoop,
  portable and self-built copies have no such item).
- *AI Limits 0.7.1 on GitHub* (opens the project page) and Quit.

Usage forecast shows its ~time on Medium and Expanded rows and on Expanded
columns, only where no reset time is known, and never for data that has
stopped updating.

### Taskbar indicator

With the overlay hidden or covered by other windows, a compact usage
readout stays on the taskbar; while a fullscreen app has the screen, it
hides with the taskbar and comes back when you leave the app. Right-click →
**Taskbar**:

- **Tray icon** (the default). One 16px icon: two concentric rings for the
  same two providers the panel shows (the first two in the widget, or the
  two ticked in the Taskbar column of *Settings*), the busier
  outside, both filling clockwise from 12 o'clock. Monochrome, drawn in the
  system taskbar theme.
- **Taskbar panel.** A transparent overlay next to the clock. There is no window or
  box, only clock-sized digits and bars drawn over the taskbar. It is
  monochrome, follows the system light and dark theme, shows the first two
  providers in the widget's order, or the two ticked in the Taskbar column of
  *Settings*, with their percentage (and its bar, unless Progress
  bar is unticked there), the time until their limit resets, or both (a
  value that has stopped updating, or an
  `≈` estimate, is drawn dimmer, as the widget greys it), and tracks the
  taskbar as it moves
  (auto-hide, resolution changes, tray width). Hover it for a tooltip with
  the rows ticked in the Tooltip column (every provider by default), each
  with its percentage and when it resets.
  If Start, Search or the auto-hide bar covers it, it hands over to the
  tray rings and comes back on its own. If a fullscreen app takes
  the screen, it hides with the taskbar and returns when you alt-tab out. It
  works on a taskbar at any edge of the screen: on a side bar the two
  providers stack, percent over bar, above the tray. A bar with no free
  stretch between its last app button and the tray gets no panel; the tray
  rings stand in until room appears, so the panel never sits on the buttons.
- **None.** No indicator.

If Windows shows the taskbar on more than one display, **Taskbar → Panel display**
picks which taskbar the panel attaches to. The submenu only appears when a
second taskbar exists; if the chosen display is later disconnected, the panel
falls back to the primary taskbar. The menu is built when the app starts, so
a display connected later shows up there after a restart. *Display 2*,
*Display 3* and on are the other taskbars counted left to right, not
Windows' own display numbers.

Left-clicking the tray icon or the panel brings the widget to the front (or
hides it if it is already there, on top); right-clicking **anywhere on the
panel** opens this same menu.

## Config & diagnostics

`%APPDATA%\AiLimits\config.toml` is created automatically, and every menu change
is persisted there. Edits saved in it apply while the app runs, within about
two seconds, with no restart; *Edit config.toml…* at the end of the
*Display* tab of *Settings* opens it in Notepad. A line that does not read is left out and
named, its default used. The file explains itself: a short note before each
setting says what it does, the values it takes and its default. Format
reference: [docs/en/CONFIG.md](docs/en/CONFIG.md).
Fine-tuning beyond the menu (sizes, colours, thresholds): [docs/en/TUNING.md](docs/en/TUNING.md).

Set `AILIMITS_LOG=ailimits=debug` or `RUST_LOG=ailimits=debug` to write a
diagnostic log to `%APPDATA%\AiLimits\ailimits.log` (stderr is invisible in a
GUI app). The file rotates at 5 MB: the previous one is kept as `ailimits.log.1`.

A crash leaves one line in `%APPDATA%\AiLimits\crash.log` (time, version,
thread, source location; never the crash message's text); a start that
failed says so in a message and in that file. Starting AI Limits again while
it runs shows and raises the widget.

A threshold toast comes once per limit window, when usage reaches the
provider's `alert_threshold` (80% by default, 85% for Copilot); again only
after the window resets, or after usage fell more than 5 points below the
threshold and climbed back. `[notifications]` in the config turns toasts
off (`enabled = false`) and sets the shortest gap between two toasts of one
provider (`cooldown_minutes`).

Requests follow the proxy set in Windows (Settings, Network, Proxy, "Use a
proxy server"); `HTTPS_PROXY`, `HTTP_PROXY` and `ALL_PROXY` win, a proxy script (PAC) is
not followed. `ailimits-auth` follows only the variables, not the Windows
setting. A network that inspects TLS with its own root certificate is not
trusted: the rows read "untrusted certificate, retrying".

## Build from source

Needs Rust 1.90 or newer with the MSVC toolchain: Visual Studio Build Tools
with the C++ workload and the Windows SDK, whose `rc.exe` embeds the icon,
the manifest and the version (see `build.rs`). The installer is built with
Inno Setup 6.7.4 from the `release-min` exes. A copy run from `target\`
does not update itself and has no *Automatic updates* item.

```
# Rust 1.90+ (MSVC), Windows 11
cargo build --profile release-min --locked   # ~4 MB exe in target/release-min/
cargo test --locked
ISCC.exe installer\ailimits.iss              # the installer, in target/installer/
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
