# Changelog

Notable, user-visible changes. Dates are release dates.

## 0.7.1 - Unreleased

> **Updating from 0.7.0 (or 0.6.x), and your user profile or TEMP folder
> path has letters outside English (Cyrillic, for example), `&` or `%` in
> it?** The old version's updater fails on such a path: once 0.7.1 is out
> it may close AI Limits about half a minute after every start. Install
> 0.7.1 by hand: download `AiLimits-Setup-0.7.1.exe` from the Releases page
> and run it, or, once winget carries 0.7.1, run
> `winget upgrade napxlexn.AILimits`. If the widget closes
> before you get to it, right-click it right after a start and untick
> **Automatic updates** first (or set `auto_update = false` in
> `%APPDATA%\AiLimits\config.toml`). From 0.7.1 on the updater starts the
> installer directly, such paths work, and Automatic updates can go back on.

### Added

- **Grok (xAI SuperGrok).** The weekly SuperGrok limit, the one grok.com's
  Usage page shows, read with the Grok CLI's own login
  (`%USERPROFILE%\.grok\auth.json`, or in `GROK_HOME`) and never refreshed:
  the CLI refreshes it whenever it runs. A product's share of the limit
  (Grok Build) can be a row of its own. Only a session issued by xAI is
  ever sent to xAI. Asked for in #1 by KritBlade.
- **Cursor.** The subscription's included usage, labelled with the plan,
  from the dashboard service the editor itself calls. Cursor has a sign-in
  of the app's own: in *Rows and models*, Cursor's *Sign-in*, *Sign in to
  Cursor...* (or `ailimits-auth login cursor`) opens Cursor's own sign-in
  page in the browser. Cursor has no sign-in for other apps, so the app
  signs in as the Cursor editor's client; it keeps that session (about 60
  days) in Windows Credential Manager (`cursor_session`) and renews it in its
  last week, at most once a day, never once it has expired (the row then
  asks for a new sign-in). Nothing of the editor is read for it, and the
  editor's own sign-in is never touched. *Sign out* there or
  `ailimits-auth logout cursor` forgets the session; the uninstaller removes
  it too. Grok Bot's share is an extra row, asked for only while that row is
  chosen.
- **Rows and models, a window of its own.** Right-click, *Rows and
  models...*: every row there is in one list, with a Widget column and a
  Tooltip column, so the widget and the panel's tooltip are chosen
  separately, and a Taskbar column for the two providers the taskbar panel
  and the tray icon show (untouched, both show the first two, as the panel
  did in 0.7.0). A tick shows at
  once, with no restart. The window opens clear of the taskbar, also of an
  auto-hidden one that would slide over it. It keeps as far from every edge
  of the screen as from the taskbar's, plus 16 px (scaled with the
  display), so it sits as far from the top as from the bottom wherever the
  widget and the taskbar are, also when it grows. The window also
  opens by itself on a first start, and from a click on the widget while
  nothing is chosen yet ("Choose what to show"), or while the rows chosen
  have no data yet ("Nothing to show yet"). Resting the pointer on a model
  part, the source of the model, an extra row or a "found on this PC" mark,
  or moving to it with the keyboard, shows what it means. It scrolls with
  the wheel, PageUp/PageDown and the arrow keys, follows the apps' light or
  dark mode, high contrast and Windows' text size, and puts the IME
  candidates beside the caret. With the rows and labels left as they are,
  the widget, the panel and the tray look as in 0.7.0, but for what "What
  looks different from 0.7.0" lists under Changed.
- **Extra rows where a provider meters a part on its own.** Claude's Opus,
  Sonnet and Fable weeks (on plans that have them; Fable is the "Current
  week (Fable)" of Claude Code's `/usage`), Antigravity's Claude and GPT
  pool, Grok Build and Cursor's Grok Bot can each be a row of their own,
  under their provider. Once a provider has answered, the window offers only
  the rows it reports, so a plan without an Opus or Sonnet week of its own
  (where they count toward the weekly limit) lists Fable alone; a row chosen
  earlier stays listed so it can be unticked, and Grok Bot, asked for only
  while chosen, is always offered. Fable's, Grok Build's and Grok Bot's
  numbers never become their provider's headline, reset or alert, and a
  reset of their own never greys the provider's row; Claude's Opus and
  Sonnet weeks and Antigravity's Claude and GPT pool are their provider's
  own windows, as in 0.7.0, so a spent one can lead the provider's row. The
  taskbar panel and the tray icon keep one entry per provider. A model
  Claude adds later with a week of its own gets a row of its own the same
  way, as soon as Claude reports it, without an update of the app
  (`claude.haiku` in the config file, "Claude · Haiku" in the window).
- **Model labels.** Three separate ticks - Model, Version, Effort - add the
  model a tool is using to its provider's row ("Opus 5.5 · xhigh"), again
  chosen separately for the widget and the tooltip. The model comes either
  from the tool's settings file ("As set in the tool") or, for Claude Code
  and Codex CLI, from the end of the newest session ("Last used", read
  back from the end until the model turns up, at most 1 MB); the other
  tools keep showing the configured model. A Claude Code model chosen by
  its short name (`/model opus`) shows the effort saved for that model;
  `ANTHROPIC_MODEL` and a Codex CLI `profile` are followed. Cursor shows the
  model chosen for its Agent ("Grok 4.7 · high"), taken from the editor's
  state database as one value; nothing else of it is read, and nothing is
  ever written or created in Cursor's folder, also where `%APPDATA%` is on
  a network path (the database is then not opened while Cursor is
  closed). The settings file is read for
  every tool that runs, also one set up with a pasted key or token or
  Cursor's own sign-in, and only once you allow it. A tool left on its default
  model, which it does not write down, shows its effort alone. Expanded
  rows and the columns layout give the label a line of its own, Medium puts
  it beside the reset (on a line of its own where that leaves too little
  room), Compact leaves it to the tooltip. Where room runs out the effort
  goes first, then the version, then the model; a word is never cut. The
  files are read on the existing update cycle, with no timer of their own,
  and only again when one changed; a file that cannot be read for a moment
  keeps the label and is read again on the next cycle.
- **Model names you choose.** The automatic name is made from the id the
  tool stores by one rule, with no table of names, so a new model needs no
  update (`claude-opus-5-5` reads "Opus 5.5"). Any model can be renamed in
  the window, also before Model is ticked (the list then says where the
  names show), from a list closed until opened: one heading per running
  tool, with the models it is known to offer and the one in use first and
  marked. A model several tools share has one name and a field under each
  of them ("shared with ..."); a tool's own router (`auto`) is named per
  tool. Typing stops where the name would no longer fit the narrowest place
  the widget draws it, so a renamed model shows in full at every size.
  Clearing the field brings the automatic name back.
- **Signed releases.** Each release carries an Ed25519 signature (`.sig`)
  for the installer and the portable zip. The app's updater installs only an
  installer whose signature verifies with the release key built into the app
  (key id `1507bd92b1a4bb7b`), besides the SHA-256 GitHub publishes for it;
  the signature covers the file name, so an installer cannot pass for
  another version. Releases are published only after the files are checked.
- **The proxy set in Windows is followed.** The providers, the Cursor
  sign-in and the updater use the proxy from Settings, Network, Proxy ("Use
  a proxy server"); `HTTPS_PROXY`, `HTTP_PROXY` and `ALL_PROXY` still win, and
  `ailimits-auth` follows only those. A proxy script
  (PAC) is not followed, and a network that inspects TLS with its own root
  certificate is not trusted: its rows say so ("untrusted certificate,
  retrying").
- **The taskbar panel can show when a limit resets.** Percent and Reset
  time ticks of its own, in the Panel column of *Rows and models*, *What
  shows* (`[ui.panel]`; the tooltip has a pair of its own in the Tooltip
  column, `[ui.tooltip]`), decide what each of the panel's two rows shows: the
  percentage with its bar, as before and by default; the time alone before
  the bar ("2h14 ▬▬"); or both, with the bar between them ("25% ▬▬ 2h14";
  on a side taskbar the time under the percentage). The time is the shown
  window's own reset, compact like the clock beside it, and a dash when it
  is not known; it is repainted only when it reads differently. A new
  **Progress bar** tick (on by default) leaves the bar out for a narrower
  panel ("25%", "2h14", "25%  2h14"); the panel is as long as what it
  shows. Asked for in #2 by JCauz.
- **The widget chooses what its rows show, the bar too.** *Rows and models*
  has two tabs: *Rows*, which rows show where, and *What shows*, with a
  Widget, a Tooltip and a Panel column for Percent, Reset time and Progress
  bar (`[ui.widget]`, `[ui.tooltip]`, `[ui.panel]`) and the model label's
  Model, Version and Effort. One of the three always stays on wherever it
  shows, so nothing is ever empty: the last tick on there cannot be taken off
  and says why. Without its bar a Compact or Medium widget row is one line
  ("Claude   2h 14min  25%") and the widget is only as wide as its texts; an
  Expanded row keeps the line of its weekly limit; a narrower width step
  drops the reset before the name. The panel can show its bar alone.
  Untouched, the widget looks as it always did. On *Rows*, each provider
  has a dot for how it runs (green: it brings numbers; yellow: it runs
  without them, loading, retrying or not signed in; hollow: off), a hairline
  sets the providers apart, an extra row is one line ("+ Opus  weekly",
  the rest on hover), and the sign-in is a chip under its rows ("Sign-in
  Claude Code · works").
- **The tray icon has other looks.** *What shows*, *Tray icon* (`[ui.tray]
  style`): the two rings, as before and by default; two upright bars, like a
  level meter; the busier provider's percentage in digits; or its slice of a
  disc, like a clock face. Each is monochrome in the taskbar's theme, and the
  tooltip names both providers.
- ***Rows and models* is a window like any program's.** It stays open until
  its cross or Alt+F4 closes it: Esc, a click beside it and another window
  taking the focus no longer close it. It moves by its title and opens
  where it was left the next time (until the app restarts), is in the
  taskbar and Alt+Tab so it can be found behind other windows, and stays on
  top only while it is the window in use. The same goes for the welcome
  and the update notice.
- **Descriptions in *Rows and models* wait for the pointer.** One shows after
  the pointer has rested 0.6 s on what it describes, as Windows' tooltips do,
  and the next one straight after follows sooner; on keyboard focus it shows
  at once. Nothing runs while none is due.
- **Antigravity says what renews its sign-in.** The row reads the
  Antigravity CLI's sign-in (`agy`), so it needs that CLI signed in on this
  PC; the token lasts about an hour and is renewed only when `agy` runs. The
  question, the hover ("run agy") and the docs now say so, where they said
  "run Antigravity" (the editor keeps a sign-in of its own, which is never
  read, and opening it does not renew this one). A bucket's own window is
  read: a weekly pool stays the long window, a five-hour one would be a
  session window of its own.
- **The tooltip's percentage and reset time, chosen.** Two ticks in the
  Tooltip column of *Rows and models*, *What shows*, Percent and Reset time
  (`[ui.tooltip]`), decide what each row of the taskbar panel's tooltip
  shows: the percentage, the time left until the limit it shows resets
  ("Claude 25% · 2h 14min"), or either alone ("Claude 2h 14min"). By
  default both: the tooltip gives each provider a line of its own with when
  it resets, where 0.7.0 had one line of percentages (untick Reset time to
  get that line back). The time no longer depends on a model label or an
  extra row being shown.
- **The context menu shows a look before you click it.** Resting the
  pointer (or the keyboard) on a detail level, a layout, a width, the
  arrangement, Forecast, a palette, Monochrome, an opacity, a brightness or
  a saturation shows it at once on the widget, the taskbar panel and the
  tray icon; a click keeps it, and closing the menu without one puts back
  what was there. Nothing is saved until the click. The menu stays Windows'
  own: the item highlighted is read from the menu's messages to its own
  window, with no hook and no timer, and nothing runs while no menu is open.
- **A crash leaves a trace.** `%APPDATA%\AiLimits\crash.log` gets one line
  per crash: the time, the version, the thread and the place in the code,
  never the crash message's text. A start that fails says so in a message
  that names the file.
- **Edits to `config.toml` apply while the app runs.** Any setting saved in
  the file, by hand or by `ailimits-auth`, applies within about two seconds
  of its save, with no restart: the app looks at the file's size and time
  every 2 seconds while you are at the PC (every 30 while you are away), one
  metadata call each time, and redoes only what changed. A change made in the
  app meanwhile is kept, and the app's own saves are never taken for edits.
  *Settings file...* at the end of *Rows and models* opens the file in
  Notepad, in a Microsoft Store copy too, whose own settings folder Windows
  keeps with the package. A line that does not read (a value of the wrong
  kind, a typo, a missing value) is left out and its default used: one
  message names the line, never its text, and the rest of the file applies.
  An edit that does not read at all (a stray line without `=`, more than 8
  lines that do not read, or bytes that are not text) is not applied, and the
  file is never written over while it stays broken, not even when the app
  quits. Nor is a part of a config saved in place of the whole (settings with
  neither `[reads]` nor any `[[providers]]`, a `[tune]` snippet alone), which
  would otherwise start over as a fresh install; found at start, it is set
  aside.
- **Fine-tuning in `config.toml`.** Four tables that only the file holds
  tune what the menu does not reach: `[tune.widget]` (text size, the gap
  between rows, bar thickness, padding, corners, your own colours for the
  three levels and for each provider's name), `[tune.panel]` (the taskbar
  panel's digits, bar length and thickness, row gap, the gap to the tray, its
  ink on a dark and on a light taskbar, its tooltip's size), `[tune.tray]`
  (the tray rings' width and gap) and `[tune.format]` (the usage at which the
  middle and high colours start, and how the widget writes a time left:
  "2d 5h", "2d5h" or "2d 5h 12min"). *Settings file...* opens the file, and
  a saved edit applies within about two seconds (the entry above). Every
  key is optional and defaults to today's look; a value out of range is
  held to its range, and a mistyped one costs only that key. The guide,
  with recipes: `docs/en/TUNING.md`.
- **`config.toml` explains itself.** The app writes a short note before
  each table and each setting: what it does, the values or the range it
  takes, its default, and the menu item or window that sets the same. The
  four `[tune]` tables are always there, every key not set commented out
  with its default and its range, so taking the `# ` away is all it takes
  to use one. Comments you add are not kept when the app saves the file.
- **A security policy.** `SECURITY.md` in the repository says how to report a
  vulnerability privately, which version gets fixes, and how to check a
  release.

### Changed

- **The tray icon shows the same two providers as the taskbar panel.**
  Untouched, both show the first two in the widget, the busier on the outer
  ring; 0.7.0's rings took the two busiest of all. The Taskbar column of
  *Rows and models* chooses for both, as before. Its tooltip now says what
  the panel's does: the rows ticked in the Tooltip column, with the
  percentage and the reset time as chosen there.
- **Nothing that belongs to another tool is read before you allow it.**
  Each tool's sign-in, the settings files and the session files are asked
  about once, before their first read, and the question names the files
  and where a token is sent. A fresh install reads nothing: the widget and
  the tray say "Choose what to show", and the window opens as a welcome
  whose question has a tick for each tool found on this PC - found by
  whether its folder exists, with nothing inside opened - and names each
  file it would read and where each token goes. Tools found by a folder of
  their own start ticked; one found only by a related folder (GitHub CLI's
  for Copilot, `.gemini` for Antigravity) is listed unticked. *Allow* starts
  the ticked ones, *Cancel* reads nothing and leaves the choice to you.
  Later, a tool's own question offers *Allow all* while other tools found
  by their own folder have not been allowed yet: it allows them too, so
  ticking one later asks nothing, but ticks none of their rows; a tool
  already allowed is left as it is. The welcome and the update notice
  take no keyboard
  focus, so a stray Enter allows nothing, keep their buttons in view at any
  size and scaling, and wait while a fullscreen app is in front. A fresh
  install also switches every provider off, so an older version started on
  the same settings reads nothing either.
- **An update keeps what worked.** The providers switched on keep reading
  without a question, nothing has to be ticked again, and one window, shown
  once, lists every file read and every host contacted. A provider the
  earlier version did not have starts switched off and asks when ticked.
- **Unticking stops reading.** Untick every row of a provider and nothing of
  it is read any more until it is ticked and allowed again; its cached
  numbers are cleared too. Unticking Model, Version and Effort takes back
  the settings and session reads, and switching back to "As set in the
  tool" takes back the session read; the next tick asks again. A key or
  usage token you paste, or Cursor's sign-in, allows that alone: such a
  provider never falls back to the tool's own sign-in.
- **Sign-in moves into *Rows and models*; the source you choose is the only
  one.** The context menu's Providers submenu is gone. Each provider has a
  *Sign-in* line under its rows that says where its sign-in comes from and
  how it went ("Claude Code · works"), and opens to choose: Claude Code's
  sign-in, a usage token or an API key you paste (Claude); Codex CLI's
  sign-in or a usage token (Codex); GitHub CLI's sign-in or a fine-grained
  token you paste, which needs no permissions (Copilot); the CLI's own
  sign-in (Antigravity, Grok); *Sign in to Cursor...*, with a wait that
  *Cancel* ends, then *Sign out* (Cursor). Only the source chosen is read or
  sent; none stands in for another when it fails, so a refused pasted usage
  token says so ("usage token rejected", for Claude and Codex) and the hover
  on a key or token of your own says to paste a new one in *Rows and
  models*. A usage token is checked with the provider before it is stored
  (Claude, Codex); a key is stored, and the row says whether it works.
  *Remove* is offered only while something is stored. What an action came to
  is said in the window, and as a toast only while the window is closed. A
  provider runs while a row of it is ticked, where the menu had an Enabled
  tick. Once, on the first start after the update, a provider left on the
  tool's sign-in while a usage token (Claude, Codex) or a Copilot token of
  yours is stored is set to that token, the lead such a token had before;
  after that the source you choose stays, whatever is stored. Claude with
  the API key chosen and none stored hides its row instead of saying "check
  the key".
- **What looks different from 0.7.0, on purpose.** With the default rows,
  everything is drawn as in 0.7.0 except the panel's tooltip (its reset
  times, see "The tooltip's percentage and reset time" under Added) and
  stale and estimated data:
  stale data greys as soon as it misses its refresh, even while its reset
  is still ahead (ac17bd2); data counts as stale once the update interval
  plus a minute has passed, never under 5 minutes, instead of after a fixed
  5 minutes (9eada81); a window that has provably reset reads ≈0% at once,
  without waiting to go stale (387fc59); the taskbar panel draws a stale or
  ≈ row in a dimmer ink (a755fe3); hovering a stale row whose error passes
  by itself ("offline, retrying") shows just that, where 0.7.0 added advice
  that did not help ("— check gh auth") (3d6093c). Time away while polling
  is paused no longer counts towards staleness.
- **Rows say what is wrong, in their own words.** A request that got no
  answer reads "offline, retrying", "timed out, retrying" or "untrusted
  certificate, retrying"; a rate limit "rate-limited, retrying"; a server
  error "HTTP 503, retrying". A row without a key of yours no longer says
  "check the key": Codex reads "token expired", Antigravity "token rejected"
  or "no Code Assist project", Copilot through gh "no Copilot on this
  account", "gh token rejected" or "no Copilot access", Cursor "sign in to
  Cursor again". An expired Antigravity token greys the row with "token
  expired" instead of hiding it, and a stored usage token that stopped
  working keeps its row, so the cause stays visible.
- **One usage request per provider per cycle.** Cursor asks for its plan at
  most hourly (again ten minutes after a request that brought none) and for
  Grok Bot only while that row is chosen; Antigravity asks for its project
  until an answer names it, then keeps it for the session, and asks its
  second quota endpoint only after an unusable answer, never after a rate
  limit, a server error or no answer; a stored usage token the server
  refused is not sent again for 30
  minutes; `gh` runs at most once every 15 minutes, failures included, and
  once more right away when GitHub refuses the token it gave earlier: gh
  2.103 and newer sign in with expiring tokens that `gh auth token` renews,
  so the usage is asked again with the new token in the same update.
- **Antigravity says what its read may cost.** Before Antigravity's
  sign-in is read, its question (and the welcome, *Allow all* and the
  update notice) says that Google has suspended some accounts for using
  Antigravity's sign-in outside Antigravity: AI Limits only reads the quota,
  once per update, but the risk is yours.
- **Copilot reads the newer usage answer.** Plans billed in AI Credits
  (GitHub's usage-based billing since June 2026) may count in decimals; a
  quota with nothing in it (`has_quota: false`) is left out, and when every
  quota is unlimited or left out the row says "no limited quota". The Copilot
  CLI's `settings.json` may hold comments and trailing commas, as GitHub
  documents it; the model label reads past them.
- **A threshold alert comes once per limit window.** The toast and the
  `on_threshold` hook fire when usage reaches the threshold, and again only
  after the window resets, or after usage fell more than 5 points below the
  threshold and climbed back, not every 15 minutes while it stays up.
  `cooldown_minutes` is now the shortest time between two alerts of one
  provider. The alerts given are kept in `alerts.json`, so a restart does
  not repeat them.
- **Switching Automatic updates on checks within seconds**, not at the next
  daily check or the next start, unless a check ran within the hour. A tick
  undone at once asks nothing.
- **Automatic updates wait until you are away.** An update is installed
  only after 5 minutes without input (or with the session locked). The
  installer is started directly, with no script in between, and starts the
  app again itself. A version whose update did not finish is not tried again
  by itself, and the next start says so once. Store, Scoop, portable and
  self-built copies have no *Automatic updates* item, as something else
  updates them.
- **Pasting a key or usage token checks it first.** In *Rows and models*,
  only a secret of the kind asked for is taken (`sk-ant-...`; `ghp_...`,
  `github_pat_...`, `gho_...`, `ghu_...` or a classic GitHub token; a
  ChatGPT token `eyJ...`, now also when it is longer than 500 characters);
  anything else is refused without being sent, and the message never quotes
  the clipboard. A usage token is checked with one request: a refused one
  is not stored, one that could not be checked is stored with a note. In
  `ailimits-auth set-usage-token` a rate limit (429) now counts as accepted,
  and a server error is asked again twice before the token is stored with a
  note.
- **Starting AI Limits again while it runs shows the widget**, raised, and
  opens a welcome that was waiting. It used to do nothing.
- **`provider-cache.json` is written when the numbers change**, otherwise
  at most every 10 minutes, and at exit, no longer after every fetch.
- **After a sleep, one quick retry.** A first cycle after the machine wakes
  (or after the idle pause) that got no answer at all is tried again after
  20 seconds rather than 2 minutes.
- **Building from source needs Rust 1.90** or newer.
- **chrono, no longer maintained, is replaced by jiff**; nothing a user sees
  changes.
- **No raw keyboard and mouse input.** The app uses none, but the window
  library registered every keyboard and mouse for it: a wake-up per mouse
  move while a window of AI Limits was in front, and a pattern that
  behaviour-watching antivirus links to key loggers. Typing into Rows and
  models works as before.
- **The exes and the installer describe themselves in full**: product name
  AI Limits, publisher, copyright and version in their file properties
  (Task Manager now names the process "AI Limits").
- **The exes carry a standard application manifest**: they run as the user
  who starts them, never asking for administrator rights, and say they are
  made for Windows 10 and 11. Nothing a user sees changes.
- **Release builds no longer contain paths of the machine they were built
  on**, and the release is signed in a job of its own, on a fresh machine
  that builds only the signer: none of the app's other dependencies ever
  runs where the release key is.

### Removed

- **The older Gemini CLI's sign-in is no longer read for Antigravity.**
  `%USERPROFILE%\.gemini\oauth_creds.json` was a fallback when Antigravity's
  own sign-in (Windows Credential Manager, `gemini:antigravity`) had no
  usable token. Google stopped serving individual, Google AI Pro and Ultra
  accounts through Gemini CLI on June 18, 2026; run `agy`, the Antigravity
  CLI, instead.
  The `.gemini` folder still marks Antigravity as possibly installed.

### Fixed

- **Starting, AI Limits no longer takes the focus from the app in front.** At
  sign-in, or when an update starts it again while you are away and a film
  plays full screen, what you were using keeps the focus, and the update
  notice or the welcome waits for a fullscreen app as it should.
- **A long reason under a vertical bar stays under its own bar.** Hovering
  an Expanded column whose data had aged showed why ("rate-limited,
  retrying") on one line across the next column; it is now held to its
  column ("rate-limited"), and so is the forecast. The whole text shows in
  the Medium and Expanded rows and on the Compact rows' bottom line.
- **The widget no longer freezes on a busy PC.** The window library could
  make the widget wait, for seconds on a loaded machine, for an update that
  was announced before it was handed over. Fixed with the upstream fix of
  tauri-apps/tao#1394 applied to the version the app uses.
- **A table written twice in config.toml costs that table only.** Pasting a
  `[tune.widget]` under the one already there, or a header left unclosed,
  used to make the whole file unreadable: at the next start the app began
  as on a new install. Now that table's lines are left out and named, and
  the rest of the file is used.
- **Codex names a window by its length.** The window that lasts five hours
  is the session and the one that lasts a week is the week, whichever field
  the answer sends it in: a plan with no five-hour limit no longer shows its
  weekly usage as "Session".
- **The widget keeps its place after a hard restart.** When the display it
  was on was not awake yet at start (or was unplugged), the widget opened on
  another one and that place was saved over the one you chose. Now the
  saved place stays yours: the widget opens on a monitor it can, and goes
  back to its place as soon as that display is there again.
- **A click on the tray icon or the taskbar panel brings the widget in
  front of every window.** Shown again after being tucked away, or while
  another window covered it, the widget could stay behind the window in
  use. It now comes up above it, without taking the focus; Always on top
  stays as it was set.
- **Auto-update no longer closes the widget when the user profile or TEMP
  path has letters outside English, `&` or `%` in it.** The updater used to
  hand over to a script in `%TEMP%`, which failed on such paths, so the app
  quit and nothing was installed. The installer is now started directly; the
  script and installer that earlier attempts left in `%TEMP%` are removed.
  See the note at the top for updating from 0.7.0.
- **Microsoft Store, Scoop and portable copies no longer contact
  api.github.com.** They used to ask for the latest release every day and
  only then skip the install; now the update check stops before any request.
- **The updater runs only the `AiLimits-Setup-<version>.exe` asset**, never
  another `.exe` a release might carry.
- **An update starts the copy it installed.** With AI Limits installed in a
  folder of your own and an old copy left in the default folder, an update
  could install one and start the other.
- **Claude no longer reads "token expired" for a server error or when
  offline.** A 5xx or Anthropic's 529 reads "HTTP 529, retrying", no answer
  "offline, retrying"; "token expired" is left for a Claude Code sign-in
  that was refused, has expired or cannot be read (a refused pasted usage
  token reads "usage token rejected", an answer of an unknown shape
  "unrecognized usage schema").
- **Copilot**: `gh` was run every minute when nobody was signed in, and is
  now asked at most every 15 minutes; it is asked for github.com only
  (`gh auth token --hostname github.com`), so a GitHub Enterprise token is
  never sent to github.com; an account without Copilot no longer shows
  "check the key" right after install; an answer that is not the known JSON
  keeps the row with an honest status.
- **Antigravity**: after a switch to another Google account the next cycle
  finds the new account's project instead of reading "token rejected" until
  a restart; offline no longer reads "open Antigravity once".
- **Codex follows `CODEX_HOME`**, for its sign-in and its model label.
  `CODEX_HOME`, `GROK_HOME` and `COPILOT_HOME` count only as absolute
  paths; a relative one means that tool's files are not read, never a folder
  relative to the app.
- **A `config.toml` held by another program (right after an update, say) no
  longer resets the app or gets overwritten on exit.** It is read again for
  about 2 seconds; if it still cannot be read, it is kept as it is and
  nothing is saved over it until a restart, and one message says so. A line
  that does not read costs only that line: it is left out, its default used,
  and a message names it. At start, the file as it was is kept beside it
  (`config.toml.<date>-<time>.left-out`, one copy for each version of the
  file), since the next save writes it without that line; after a live edit,
  mend the line in the file itself. One that cannot be understood at all (a
  stray line without `=`, say) is set aside under a name of its own
  (`config.toml.<date>-<time>.corrupt`), never replacing an earlier one, and
  a message names it. A config saved by Notepad as UTF-16 now reads.
- **Hook commands with a quoted path work** (`"C:\Program Files\x\y.exe"
  --flag`): they run as `cmd.exe /D /S /C "<command>"`. A command with a
  line break is refused whole, never run in part. A failed hook is logged
  by event name and exit code, never the command.
- **Switching an account no longer brings its predecessor's numbers back.**
  A fetch that started before a sign-in or sign-out, a pasted or removed key
  or usage token, or a consent given or taken back is dropped when it lands.
- **`ailimits-auth status` changes nothing**: it no longer creates or sets
  aside `config.toml`, and reads another tool's sign-in only where the
  widget may. None of the commands replaces a config it cannot read, nor
  one with lines that do not read (saving it would drop them): they name
  the lines and wait until they are fixed. `set`
  and `set-usage-token` store nothing then; `remove` still removes the key
  and leaves the config as it is; `login` keeps the session it got and says
  the config could not be updated. `set` and `set-usage-token` make what
  they store the provider's source (`api_key`, `usage_token`).
- **The updater runs exactly the file it verified.** The installer is
  written into a new folder of its own in `%TEMP%` and held from its last
  check until it starts: nothing can change, rename or delete it meanwhile,
  and it is read back against the verified SHA-256 first. Downloads follow
  redirects only to GitHub over https, and every answer the app reads is
  capped in size (a provider's at 1 MiB, the installer at 64 MiB); a larger
  one reads "answer too large, retrying".
- **A pasted key goes only to its own provider.** Claude reads only its API
  key and Copilot only its token, whatever label `config.toml` names.
- **Removing a usage token, cancelling the Cursor sign-in or signing out
  always wins.** A check or sign-in still under way can no longer store its
  token or session afterwards: it is stored only while it is still awaited.
- **Cursor's session refresh stays once a day** when Credential Manager
  cannot save the refreshed session, instead of being tried every cycle.
- **A `config.toml` that cannot be understood is logged by line and column
  only**, never by the text of the line.
- **The TLS libraries are updated** for the October 2026 advisories.
- **Libraries brought up to date** (the menu, tray, drawing, settings-file
  and Windows-binding crates among them; the installer is built with Inno
  Setup 6.7.4). No new DLLs come with them, and the look is unchanged. The
  `cargo audit` ignore list is trimmed to what still applies. Reported in #4
  by zyl71.

### Going back to 0.7.0

Not supported. 0.7.0 cannot read a cache that has Grok or Cursor in it, so
it shows no cached readings. It keeps the Grok and Cursor entries in the
config but shows neither, and when it saves its settings it drops
`[ui.rows]`, `[ui.model]`, `[ui.tooltip]`, `[ui.panel]`, `[tune]` and
`[reads]`, and writes a usage token's source as `subscription`. Back on
0.7.1, the providers switched on in the config keep reading (Grok and
Cursor among them, if they were on; the Cursor sign-in stays in Credential
Manager), the update notice shows once more, a stored usage token or
Copilot token is the source again, and the rows, labels, names and `[tune]`
knobs are chosen again.

## 0.7.0 - 2026-09-23

### Added

- **The taskbar panel on a bar at any edge of the screen.** Windows 11 is
  bringing the taskbar's position back (top, left, right); the panel used
  to assume a bottom bar and refused a side one. It now reads the bar's
  edge, hides with an auto-hide bar sliding off any side, sits before the
  tray along the bar's axis, stacks the two providers on a side bar
  (percent over bar, the bar's full width), and opens its tooltip away
  from the bar. Where the buttons end and where the notification area
  starts are asked of the shell through UI Automation - on the primary bar
  and on a secondary one alike - so the panel lands in the gap between
  them, and a bar with no room for it gets the tray icon instead. Moving
  the bar to another edge is waited out rather than raced: the shell moves
  its bar's window at once and draws it the better part of a second later,
  and the panel holds back until the bar is really there.
- **The Microsoft Store listing** is linked from the site (hero, download
  row, structured data) and both READMEs (a badge that reads the Store's
  own version, the `winget --source msstore` line, the listing link). The
  Store carried 0.6.3 when this was written; it follows each release after
  certification.

### Changed

- **The panel stands down when the bar has no room for it**, and the tray
  icon stands in until room appears. It is a side bar that makes this
  plain: buttons run top to bottom into the notification area and there is
  nothing left for a panel. The stretch used to be read off the bar's own
  pixels, and that verdict drove itself - standing down raised a tray
  icon, the icon widened the tray, the stretch changed and the verdict
  flipped, twice a second - so it was withdrawn. It is the shell's own
  answer now (UI Automation), which our icon can only move one way: it
  takes a little more room to come back than it took to leave. It also
  survives what the shell does to that answer: an Explorer restart leaves
  the taskbar's automation tree empty until the next one, so the last
  answer for a bar of that shape is kept and used.
- **The panel and the tray icon are never on screen together.** The icon
  stands in for the panel; a user who sees both sees them swapping places.
  The panel goes first and the icon follows it down, the icon goes first
  and the panel follows it up. And the icon no longer stands in for a
  blink: pressing Windows puts the shell's bar over the panel for about a
  second, so the icon waits until the panel has been out of sight for
  700 ms. A quick press and press again shows nothing at all.
- **The version line in the menu opens the project.** "AI Limits v0.7.0 -
  GitHub" at the foot of the context menu is a live item now: the arrow
  marks it as leaving the app, and a click opens the GitHub page in the
  default browser. It used to be a greyed-out label.

### Removed

- **The `statusline.jsonl` fallback for Claude.** Claude Code stopped
  writing that file, so the reader between the OAuth request and the
  honest "token expired" state could never fire again; it is gone, along
  with the `statusline.jsonl: no` line in `ailimits-auth status`. The
  source chain is now: manual usage token, Claude Code's OAuth token, then
  the greyed last value.

### Fixed

- **Pressing the Windows key with the widget focused no longer freezes the
  app.** Click the widget, press Win: the event loop deadlocked inside the
  window library's keyboard handling (a lock held across a message peek that
  re-entered the same handler), and from then on nothing updated, the menu
  never opened and the taskbar panel stood still until the process was
  killed. The window, menu and tray libraries are updated (tao 0.31 to 0.37,
  muda 0.15 to 0.20, tray-icon 0.19 to 0.25); the new tao releases that lock
  before peeking. The update also collapses the two copies of the Windows
  bindings the app carried into one.
- **The panel stands down for a fullscreen app on its monitor.** Alt-tab
  into a game or a fullscreen video and the panel was drawn over it, since
  the taskbar it follows is gone but the panel is a topmost window of its
  own. It now takes the shell's own word for it - the notification an
  appbar gets when a fullscreen app opens and when it goes - paired with a
  geometric test that something really covers that monitor, so a game on
  the other screen leaves this one alone. The word alone flickers (it dips
  for a second at a time while a game is plainly still there), so a bar
  buried under one window at three points counts as well, and the verdict
  holds for a further 80 ms - four frames, enough to outlast the flicker
  and no more. The way back out of a game is the part anyone waits for, so
  it costs as little as it can: the tray icon is never put up while the
  verdict holds, the hold is skipped outright when the window covering the
  monitor is no longer the one in front (which is what alt-tab means, and
  what a flicker never looks like), and the panel keeps lifting itself
  until it is over the bar again - for a moment after a game lets go the
  shell holds its bar in a band no ordinary window can beat. Measured on a
  stand-in fullscreen window on the second monitor, sampling the panel's
  own pixel every 10 ms: back on screen 25-27 ms after the alt-tab, where
  it used to be 174-262 ms.
- **The panel survives a taskbar thumbnail.** Hovering an app button on the
  bar raises its preview, and the shell clears every window off the
  monitor for it - Aero Peek - which took the panel with it. The panel is
  now excluded from peek, as the shell's own surfaces are.
- **The panel no longer blinks between two places on a busy bar.** The
  verdict that withheld it for want of room drove itself: standing down
  put the tray icon up, the tray icon widened the tray by its own 32px,
  the stretch before the tray changed and the verdict flipped - twice a
  second, over anything on screen. The stretch is the shell's own answer
  now, not a reading of the bar's pixels, and our icon can only move it
  one way. See the room rule under Changed.
- **Unplugging a monitor no longer strands the widget or the panel until
  the next start.** A position that lands on no screen was rescued at
  startup only, so a display removed while the app ran left the overlay
  invisible and un-draggable (it is borderless and skips the taskbar), and
  the panel kept following a taskbar that no longer existed until some
  other event happened by. The app now hears the display change itself:
  the overlay is put back on the primary work area and the new position is
  saved, the taskbar watch is re-pointed, and the panel is placed again -
  on the bar it was set to if it is back, on the primary bar while it is
  not.
- **The taskbar's context menu is drawn over the panel, not under it.**
  Right-click the bar beside the panel and the menu opened behind it. The
  Windows 11 menu is a XAML popup, not the classic menu a menu hook
  reports, so it is caught by class when the shell shows one and measured
  when it takes its place a moment later; while it reaches over the panel,
  every raise the panel would do puts it directly under the menu instead.
  It returns on top when the menu goes. A menu over the panel does not
  raise the tray icon either: the panel is not obstructed there, it is
  under a menu on purpose, as every other window is.
- **The taskbar mini panel keeps following an auto-hide bar after Explorer
  restarts.** The move/auto-hide watch is a WinEvent hook scoped to
  Explorer's process; when Explorer is killed and started again (or crashes
  and comes back) the new process has a new id and the hook never fires
  again, so the panel only moved on the 60-second tick or when some other
  window change nudged it - a laggy panel drawn over a hidden bar and vice
  versa. The hook is now re-scoped to the new process the moment the new
  taskbar is seen.
- **A secondary taskbar is found even while the shell holds it in its raised
  band.** After a Start menu cycle that catches an auto-hide bar open, the
  shell keeps that bar above every ordinary topmost window until the next
  cycle. `EnumWindows` does not walk that band, so in that state the widget
  saw no secondary taskbar at all: the Display submenu vanished and a panel
  set to that bar fell back to the primary one on the next start. The bars
  are now found by class, which sees them in either band. (While the bar is
  raised the panel cannot draw over it and the tray rings stand in, as for
  the cursor-edge peek; the panel returns when the shell lets the bar back
  down, which a Start open and close with the cursor off the bar does.)
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
