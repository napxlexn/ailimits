# PROVIDERS.md — how the widget talks to each provider

Ground rules that apply to every provider:

- **Read-only tokens.** The widget reuses tokens owned by the official CLI
  tools and NEVER refreshes or rotates them (rotation would log the CLI out).
  An expired token greys the row quietly, never an error dialog. The one session
  the widget refreshes is its own Cursor session, which no other app uses
  (see Cursor).
- **Undocumented endpoints are treated as hostile.** Tolerant parsers; any
  unknown schema or non-200 status degrades honestly (NetworkError) instead
  of inventing numbers.
- **Secrets live only in the Windows Credential Manager**, service
  `ailimits`. The config stores labels, never values. Tokens are never logged.
- **Stale-data policy** (implemented in `src/app/` + `renderer.rs`): a transient
  error never wipes the last real data; it is shown greyed with its age. The
  taskbar panel, which has no room for the age, draws such a row (and an
  `≈` estimate) in a dimmer ink.
  Data counts as stale once it has missed its next scheduled refresh: the
  update interval plus a minute, never under 5 minutes. Polling pauses while
  the user is idle or locked (5 minutes without input), and that time does
  not count: a row does not grey because the app stopped asking while the
  user was away, and on return it stays live until the immediate resume fetch
  lands. A provider that fails while the user is active still greys, and data
  already stale before a pause stays stale after it. The displayed age is
  still the plain wall-clock age.
  Once a metric's reset time has provably passed, the display extrapolates
  to `≈0%` (`ProviderStatus::Estimated`).
- Metrics whose reset is still in the future are persisted to
  `provider-cache.json` and survive widget restarts: after a relaunch the
  row shows the last real value (greyed, with its age), not an error text.
- **No update from an older account.** Every update carries the serial of
  the provider instance that made it, and the event loop drops one whose
  instance was replaced meanwhile. A provider is rebuilt only when its own
  settings or account may have changed: a sign-in or sign-out of Cursor, a
  key or usage token pasted or removed, another source chosen in its
  Sign-in, the provider switched on or off (its first row ticked, its last
  unticked), a consent given or taken back. Its cached numbers, last error,
  trend and alert marks are then cleared, and the row reads Loading until
  the first answer. A rebuilt provider is fetched at once. A change to one
  provider leaves the other providers alone (and their caches: gh's token,
  Cursor's plan, Antigravity's project).
- **Only the source chosen.** Each provider's sign-in comes from the one
  source chosen under its rows in Rows and models (Sign-in), saved as
  `auth_method` (see CONFIG.md): the tool's own sign-in, or a key or usage
  token you pasted. No other source stands in when that one fails, so a
  refusal is shown as one, never hidden behind another account's numbers.

## Requests per cycle

One usage request per provider per cycle, and none while polling is paused.
Cycles run no more often than the update interval (60 s minimum), with
three exceptions in timing, never in count: the cycle right after a pause
ends; one retry 20 s later when the first cycle after a wake (from sleep or
from the idle pause) got no answer at all; and a provider just rebuilt,
fetched at once. A cycle in which no provider answered waits longer: 120,
240, then 300 s at a 60 s interval (`monitor/scheduler.rs`). The few helper
requests, exactly:

- **Cursor**: the plan (`GetPlanInfo`) at most once an hour per account (ten
  minutes after a request that brought nothing); Grok Bot
  (`GetSandUsageStatus`) only while its row `cursor.bot` is chosen on the
  widget or the tooltip; the session refresh (`api2.cursor.sh/oauth/token`)
  only in the last 7 days before the session expires, never after it, at
  most once a day, refusals and attempts that got no answer included. A
  rebuilt Cursor (Grok Bot ticked or
  unticked, a new sign-in) starts with no plan and asks for it again.
- **Antigravity**: `loadCodeAssist` until an answer names the project, which
  is then kept for the session (asked again only after the token is
  refused). After no answer, a status other than 200, or a 200 without the
  project, nothing is kept: the cycle ends there and the next one asks
  again. `fetchAvailableModels` only when `retrieveUserQuotaSummary` gave
  nothing usable (a 200 with an unknown or empty body, or a status other
  than 200, 401, 403, 429 and 5xx: a 3xx, which is never followed, another
  2xx or a 4xx); it then goes first for an hour, so a cycle stays one
  request. Never after a 429, a 5xx or no answer.
- **Claude and Codex**: only the source chosen is asked, the tool's own
  token or the usage token you pasted, never one after the other. A stored
  usage token the server refused (401/403) is not sent again for 30 minutes
  (a newly pasted one is tried at once); its status meanwhile is "usage
  token rejected". A 429, a 5xx, an unknown body or no answer ends the
  cycle with no second request.
- **Copilot**: with GitHub CLI's sign-in chosen,
  `gh auth token --hostname github.com` at most once every 15 minutes, its
  failures included, plus one run right away when the server
  refuses (401/403) a gh token kept from an earlier cycle: gh 2.103.0 and
  newer give expiring tokens that `gh auth token` renews by itself. The
  request is then sent once more, with the new token (not when gh gives the
  same one again). A token gh gave in the same cycle is not asked again. A
  gh token the server refused (401, 403, 404), the new one included, is not
  sent again for 15 minutes.

## Network

- Every request goes over HTTPS to the hosts named per provider below, with
  a 10 s time-out and no redirects, so a token is never forwarded to another
  host. An answer's body is read up to 1 MiB (`MAX_ANSWER_BYTES`): a
  Content-Length past it is refused before anything is read, and a body
  without one is given up on as soon as it passes the cap.
- **Proxy.** The app uses the proxy set in Windows (Settings > Network >
  Proxy > "Use a proxy server": `ProxyEnable`, `ProxyServer`,
  `ProxyOverride` under the user's Internet Settings) for the providers, the
  Cursor sign-in and the updater. `HTTPS_PROXY`, `HTTP_PROXY` and
  `ALL_PROXY` take precedence. A proxy script (PAC) and automatic detection
  are not followed. The setting is read once; a change applies after a
  restart. `ailimits-auth` follows only the variables.
- **Certificates.** TLS uses the roots built into the app (rustls), not the
  Windows certificate store, so a network that inspects TLS with its own
  root is not trusted: the rows say "untrusted certificate, retrying".
  Trusting the Windows store would add new DLLs to the exe.

## Error states

Each status is at most 32 characters, so the widget shows it as it is; one
that ends in ", retrying" passes by itself and the hover adds no action.
Every error keeps the last real data, greyed with its age.

| Cause | Row says |
|---|---|
| no answer: offline, DNS, a body cut off | `offline, retrying` |
| no answer in 10 s | `timed out, retrying` |
| the certificate is refused | `untrusted certificate, retrying` |
| an answer over 1 MiB | `answer too large, retrying` |
| HTTP 429 | `rate-limited, retrying` |
| HTTP 5xx (Anthropic's 529 among them) | `HTTP <code>, retrying` |
| any other status | `HTTP <code>` |

The provider's own states are listed in each section. An `AuthError` (only
a key or token you gave: a Claude API key, a Copilot token, a Claude or
Codex usage token) shows "check the key", and its hover reads "<status> —
paste a new one in Rows and models"; a row without a key of yours never
does.

## Claude

### Token sources — the one chosen (`fetch`)

Claude's Sign-in in Rows and models offers three sources; `auth_method`
keeps the one chosen, and only that one is used:

1. **Claude Code's sign-in** (`"subscription"`, the default) —
   `%USERPROFILE%\.claude\.credentials.json` → `claudeAiOauth.accessToken`
   (skipped when `expiresAt` is in the past), only when `[reads] sign_in`
   allows it.
2. **A usage token you paste** (`"usage_token"`) — Credential Manager label
   `claude_usage_token` (Rows and models: Claude, Sign-in, *Paste token from
   clipboard*; CLI: `ailimits-auth set-usage-token claude`). Refused
   (401/403) → `AuthError("usage token rejected")`; it is then not sent again
   for 30 minutes. Claude Code's sign-in is not read in its place.
3. **An API key you paste** (`"api_key"`) — see API key below.

With source 1 or 2 (`fetch_via_subscription`):

- The source gave no data → `NetworkError("token expired")` when Claude
  Code's token was refused (401/403), has expired, or `.credentials.json`
  cannot be read or parsed; otherwise what actually failed: the shared
  words (see Error states: offline, time-out, certificate,
  `rate-limited, retrying` after a 429, `HTTP 529, retrying` and other
  5xx), or `"unrecognized usage schema"` for a 200 the parser does not
  know. The hover adds "run Claude Code" to a status that does not end in
  ", retrying".
- The source chosen is not there (no `.credentials.json` or no leave to
  read it; no usage token stored) → `NotConfigured` (the row is hidden).

(`statusline.jsonl` snapshots were a source until 0.7.0; Claude Code no
longer writes that file.)

Both token sources call the undocumented usage endpoint
(verified 2026-06-10, HTTP 200):

```
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <token>
anthropic-beta: oauth-2025-04-20
```

Response (null windows are skipped):

```json
{"five_hour":  {"utilization": 83.0, "resets_at": "RFC3339"},
 "seven_day":  {"utilization": 6.0,  "resets_at": "RFC3339"},
 "seven_day_opus": null, "seven_day_sonnet": {...}}
```

→ metrics `Session`, `Weekly`, `Opus`, `Sonnet` (percent, limit 100).

**A model's own week (verified 2026-11-16, HTTP 200).** The same response
also carries `limits[]`; Claude Code's `/usage` shows its model entry as
"Current week (Fable)":

```json
{"limits": [
  {"group": "session", "kind": "session", "percent": 0, "resets_at": "RFC3339", "scope": null, ...},
  {"group": "weekly", "kind": "weekly_all", "percent": 1, "resets_at": "RFC3339", "scope": null, ...},
  {"group": "weekly", "kind": "weekly_scoped", "percent": 0, "resets_at": "RFC3339",
   "is_active": false, "severity": "normal",
   "scope": {"model": {"display_name": "Fable", "id": null}, "surface": null}}],
 "weekly_scoped_shares": [{"allowance_percent_of_weekly": 50, "limit_index": 2,
                           "used_percent_of_weekly": 0}]}
```

The `session` and `weekly_all` entries repeat `five_hour` and `seven_day`.
Each `weekly_scoped` entry with a model name becomes a metric named after
the model (`Fable`; `percent` is of that model's own allowance), only
beside the windows above and never twice under one name. It is marked for
its extra row only (`claude.fable`, off by default): it is never Claude's
headline, its reset, its alerts or a part of its own row, so the Claude row
looks exactly as before until that row is chosen. A model the app does not
name yet (one Claude adds later) gets an extra row of its own the same way,
made from the reply: `claude.<name in lowercase, words joined by "-">`
(`claude.haiku`), named "Claude · Haiku", "Cl · Haiku", "Haiku" as space
allows, off by default. A name longer than 20 characters ends in "…" and
steps down to its first word; at most 16 such rows are made in one run.

**Rate limiting (verified 2026-07-09).** The endpoint's per-token bucket is
shared with Claude Code's own polling: under an active session roughly one
widget request in three returns 200, the rest get HTTP 429 with
`Retry-After: 0`. A bounced request ends the cycle at once on
`rate-limited, retrying` (not a token problem; no other source is tried);
the display keeps the last data, and the next cycle asks again at the
update interval. Net effect: a Claude refresh occasionally takes 2–3
minutes — a server-side ceiling.

### API key (optional)

`auth_method = "api_key"`, label `claude_api_key` (Rows and models: Claude,
Sign-in, *An API key you paste*, *Paste key from clipboard*; CLI:
`ailimits-auth set claude`). One request per cycle:

```
GET https://api.anthropic.com/v1/models
x-api-key: <key>
anthropic-version: 2023-06-01
```

Limits come from the response headers
`anthropic-ratelimit-requests-{limit,remaining,reset}` and
`anthropic-ratelimit-tokens-{limit,remaining,reset}` → metrics `Requests`
and `Tokens` (session windows), each only when its headers are there; a 429
still carries them and is read. 401 → `AuthError("invalid API key")` (the
row says "check the key"; the hover "invalid API key — paste a new one in
Rows and models"); a 200 without the headers →
`NetworkError("no ratelimit headers in response")`; any other status without
them, or no answer → the shared words. The API key chosen and none stored →
`NotConfigured`: the row is hidden, and the Sign-in reads "Your API key ·
none stored". This monitors API rate limits, not the subscription.

## OpenAI Codex (subscription only)

Token sources: Codex's Sign-in in Rows and models offers two;
`auth_method` keeps the one chosen, and only that one is used:

1. **Codex CLI's sign-in** (`"subscription"`, the default) — `auth.json` in
   the CLI's home: `%CODEX_HOME%\auth.json` when `CODEX_HOME` is set to an
   absolute path, otherwise `%USERPROFILE%\.codex\auth.json` (a relative
   `CODEX_HOME`: no file is read at all) → `tokens.access_token`; only when
   `[reads] sign_in` allows it. Codex CLI writes this file only when it
   keeps its sign-in in a file: with `cli_auth_credentials_store =
   "keyring"` or `"auto"` in its `config.toml` there is no `auth.json`, and
   the sign-in cannot be read. Set it to `"file"`, or paste a usage token.
2. **A usage token you paste** (`"usage_token"`) — label `codex_usage_token`
   (Rows and models: Codex, Sign-in, *Paste token from clipboard*; CLI:
   `ailimits-auth set-usage-token codex`). Refused (401/403) →
   `AuthError("usage token rejected")`, the row kept (greyed, last data
   kept); it is then not sent again for 30 minutes. Codex CLI's `auth.json`
   is not read in its place.

Endpoint (verified 2026-06-10; the Codex CLI itself polls it):

```
GET https://chatgpt.com/backend-api/wham/usage
Authorization: Bearer <token>
User-Agent: ailimits-widget
```

```json
{"plan_type": "plus",
 "rate_limit": {
   "primary_window":   {"used_percent": 1,  "reset_at": 1781121633},
   "secondary_window": {"used_percent": 28, "reset_at": 1781553834}}}
```

**Verified live 2026-11-21** (a Plus account): each of
`rate_limit.primary_window` and `rate_limit.secondary_window` carries
`used_percent`, `reset_at`, `reset_after_seconds` and
`limit_window_seconds`; `additional_rate_limits` was `null`; beside them
the answer has `credits`, `model_usage`, `plan_type`,
`rate_limit_reset_credits` and `spend_control`. Only `used_percent` and
`reset_at` of the two windows are read.

A window's own length (`limit_window_seconds`) names it: up to a day →
`Session`, six days or more → `Weekly`, so a plan with no 5-hour limit that
sends its week as `primary_window` still reads as the week; without the
length, `primary_window` = the 5-hour session → `Session` and
`secondary_window` = the week → `Weekly`. The session row comes first. `reset_at` is unix epoch seconds. 401/403 on the CLI
token → `NetworkError("token expired")` (the hover adds "run Codex CLI";
not "check the key": no key was given). A body that is not the known JSON →
`"unrecognized wham/usage schema"`; other statuses and no answer → the
shared words (see Error states). An unreadable auth.json →
`NetworkError("auth.json unreadable")`. The source chosen is not there (no
auth.json or no leave to read it; no usage token stored) → `NotConfigured`
(the row is hidden).

## GitHub Copilot (subscription only)

Token sources: Copilot's Sign-in in Rows and models offers two;
`auth_method` keeps the one chosen, and only that one is used (gh is never
run for a pasted token, and a stored token is never sent while GitHub
CLI's sign-in is chosen):

1. **GitHub CLI's sign-in** (`"subscription"`, the default) —
   `gh auth token --hostname github.com` (never another host,
   so a GitHub Enterprise sign-in or `GH_HOST` is not sent to github.com),
   spawned with `CREATE_NO_WINDOW` (found by Rust's own lookup: the app's
  folder, System32, the Windows folder, then PATH, never the working
  folder; System32 is only the working folder it runs in), killed after
  15 s; only
   when `[reads] sign_in` allows it. gh's answer stands for 15 minutes, its
   failures included: gh not installed or nobody signed in to github.com →
   the row is hidden (`NotConfigured`) and gh is asked again in 15 minutes;
   gh timing out → `"gh timed out, retrying"`. One exception: when the
   server refuses (401/403) a token kept from an earlier cycle, gh is asked
   once more right away (gh 2.103.0 and newer give expiring tokens and
   renew them in `gh auth token`), and the request is sent once more with
   the new token; gh giving the same token again, or none, sends nothing
   more. A token the server refused, the new one included, is not sent
   again for 15 minutes.
2. **A fine-grained token you paste** (`"api_key"`) — label `copilot_pat`
   (Rows and models: Copilot, Sign-in, *Paste token from clipboard*; CLI:
   `ailimits-auth set copilot`). GitHub documents classic PATs (`ghp_...`)
   as not supported for Copilot CLI. A fine-grained PAT (`github_pat_...`)
   needs no permissions at all: verified live on 2026-11-21, a token with
   *Plan* and *Copilot Requests* (read), with *Plan* alone and with none got
   the same 200 and the same quotas from `copilot_internal/user` as GitHub
   CLI's sign-in (the endpoint names no permission it accepts). *Copilot
   Requests* also lets a token send Copilot requests, so it is not asked
   for. GitHub's public premium-request usage report
   (`/users/{user}/settings/billing/premium_request/usage`) needs *Plan*
   (read): 403 without it; it answers `timePeriod`, `user` and a list of
   the month's `usageItems` (empty on this account) and no quota at all,
   so it is no replacement for the endpoint above. None
   stored → `NotConfigured` (the row is hidden).

Endpoint (verified 2026-06-10 and 2026-11-21; the same one the Copilot
extensions use):

```
GET https://api.github.com/copilot_internal/user
Authorization: token <token>
User-Agent: ailimits-widget
```

Verified 2026-06-10 (free plan):

```json
{"copilot_plan": "individual", "quota_reset_date": "2026-07-01",
 "quota_snapshots": {
   "premium_interactions": {"entitlement": 50,  "remaining": 10, "unlimited": false},
   "chat":        {"entitlement": 200,  "remaining": -1},
   "completions": {"entitlement": 2000, "remaining": 2000}}}
```

Verified 2026-11-21 (individual plan with token-based billing; every
snapshot carries the same fields):

```json
{"copilot_plan": "individual", "token_based_billing": true,
 "quota_reset_date": "2026-12-01",
 "quota_reset_date_utc": "2026-12-01T00:00:00.000Z",
 "quota_snapshots": {
   "chat": {"entitlement": 200, "remaining": 199, "quota_remaining": 199.6,
            "percent_remaining": 99.8, "has_quota": true, "unlimited": false,
            "credits_used": ..., "overage_count": 0, "overage_permitted": false,
            "quota_reset_at": 0, "token_based_billing": true,
            "timestamp_utc": "..."},
   "completions": {"entitlement": 2000, ...},
   "premium_interactions": {"entitlement": 0, "has_quota": false, ...}}}
```

In June 2026 GitHub moved Copilot to usage-based billing in AI Credits
(`token_based_billing`). Third-party sources say paid credit-billed plans
may report decimal entitlements; that is not verified live.

→ metrics `Premium`, `Chat`, `Completions`, in that order (used =
entitlement − remaining; `quota_remaining`, a decimal, is preferred to
`remaining`; decimal numbers are rounded to whole requests; negative
`remaining` = overage). Snapshots that are `unlimited`, have
`has_quota: false` or no entitlement are skipped; when every snapshot is
skipped (each quota unlimited or absent) the row reads
`NetworkError("no limited quota")`: the schema is known, there is nothing
to count, and the hover adds no advice. `quota_reset_date_utc`,
else `quota_reset_date` at midnight UTC → the reset. An unknown schema or a body that
is not JSON → `NetworkError("unrecognized Copilot schema")`. A refusal reads
by where the token came from. gh's token: 404 → `"no Copilot on this
account"`, 401 → `"gh token rejected"`, 403 → `"no Copilot access"` (the row
shows the words; the hover adds "check gh auth"). A pasted PAT: 401/403/404
→ `AuthError("token has no Copilot access")`, shown as "check the key" (the
hover: "token has no Copilot access — paste a new one in Rows and models"). Any
other status, no answer or a body cut off mid-transfer → the shared words
(see Error states).

## Token lifetimes and accuracy window

The CLI tokens are short-lived OAuth access tokens; the owning CLI refreshes
them on use. The widget never refreshes (rotation would log the CLI out —
confirmed in the wild: NousResearch/hermes-agent#22903, where a sibling
client's refresh invalidated the others). Consequences:

| Provider | Token lifetime | Live-data window without action |
|---|---|---|
| Claude | ~8h access token (verified), refreshed by Claude Code on use | ~8h after the last Claude Code session, then grey |
| Codex | not measured yet, refreshed by Codex CLI when it runs | while Codex CLI is in use; how long after is not verified |
| Copilot | gh manages its own token; since gh 2.103.0 (2026-10-21) an expiring OAuth token that `gh auth token` renews by itself | indefinite while logged into gh |
| Antigravity | the Antigravity CLI's keyring token, about an hour, renewed only when `agy` runs | about an hour after `agy` last ran, then grey with "token expired"; no other source |
| Grok | ~6h access token (verified), refreshed by the Grok CLI on use | up to ~6h after the last Grok CLI session, then grey; no other source |
| Cursor | the widget's own session: a ~60-day JWT, refreshed by the widget in its last week (whether that extends it is not verified yet) | until the session ends, then grey with "sign in to Cursor again" |

There is no `gh auth token`-style "print/refresh the token" command for the
Codex or Claude CLIs (verified against the OpenAI Codex auth docs). The only
CLI-driven refresh path is Copilot's `gh auth token`: since gh 2.103.0
(2026-10-21) gh signs in with expiring OAuth tokens and renews one in
`gh auth token` itself, keeping it in its own store, so gh stays signed in.
A token the widget kept can therefore lapse within its 15 minutes; a refusal
of it asks gh once more (see Requests per cycle). For a credential that
does not depend on CLI activity, the official recommendation (and the
widget's manual path) is an API key / PAT — see CONFIG.md for the labels.

## Antigravity (Code Assist subscription)

Token source, the only one:

1. **Antigravity CLI** — Windows Credential Manager target
   `gemini:antigravity` → JSON `token.access_token` (READ-ONLY; skipped when
   `token.expiry` is in the past).
2. A token exists but it is expired → `NetworkError("token expired")`:
   the last data stays, greyed with its age, until `agy` renews the token
   (the hover adds "run agy"); nothing else renews it, and no other source
   stands in. This is deliberate (d6abbde): an
   expired token greys the row rather than hiding it. A credential that
   cannot be read → `NetworkError("credentials unreadable")`. No token →
   `NotConfigured` (the row is hidden).

The entry is read only when `[reads] sign_in` allows it.

As of June 18, 2026, Google stopped serving Gemini CLI and Gemini Code Assist
IDE-extension requests for Gemini Code Assist for individuals, Google AI Pro,
and Google AI Ultra. Gemini CLI remains supported for Gemini Code Assist
Standard/Enterprise and paid Gemini / Gemini Enterprise Agent Platform API-key
flows. Individual users are expected to migrate to Antigravity CLI; Antigravity
stores session tokens in the OS keyring. On Windows, current Antigravity CLI
stores the consumer session under `gemini:antigravity`. The legacy Gemini CLI
file `%USERPROFILE%\.gemini\oauth_creds.json`, a fallback until 0.7.1, is
therefore no longer read. The `.gemini` folder stays a weak sign that
Antigravity may be installed (the Antigravity CLI lives in
`.gemini\antigravity-cli`); only its existence is checked.

**Account risk.** Google has suspended some accounts for using
Antigravity's sign-in outside Antigravity. Every place that asks for or lists
this read says so before it is allowed: Antigravity's own question, the
welcome, *Allow all* and the update notice (`consent::ANTIGRAVITY_RISK`):
"Google has suspended some accounts for using Antigravity's sign-in outside
Antigravity. AI Limits only reads your quota, once per update, but the risk
is yours."

Quota is a chain of sources (all verified live 2026-08-01):

1. `loadCodeAssist` (empty body) → `cloudaicompanionProject`, kept for the
   session once an answer names it, until a 401/403 (another Google account
   signed in: the next cycle asks again with the new token). After no
   answer, a status other than 200 or a 200 without the field nothing is
   kept, and the next cycle asks again. It is returned ONLY to an
   identified client (`User-Agent: antigravity/1.0` +
   `X-Goog-Api-Client: gl-go antigravity`); an anonymous
   call still answers 200 but omits the field. Everything below needs the id,
   and without it the widget reports an honest error instead of quota — every
   endpoint answers an "everything full" default view when the project is
   missing, which is indistinguishable from an unused account. A 200 without
   the field → `"no Code Assist project"` (the hover adds "run agy":
   its first run sets the account up); no answer or any other status here
   (but 401/403) is not a missing project and reads as the shared words (see
   Error states).
2. **`retrieveUserQuotaSummary`** — the shared pools Antigravity meters today:

   ```
   POST https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary
   Body: {"project": "..."}
   → {"groups": [{"displayName": "Gemini Models",
        "buckets": [{"bucketId": "...", "displayName": "Weekly Limit",
                     "window": "...", "resetTime": "RFC3339",
                     "remainingFraction": 0}]},
       {"displayName": "Claude and GPT models", "buckets": [...]}]}
   ```

   A spent pool may report `remainingFraction: 0` or omit the field and keep
   only `resetTime` — both mean zero remaining. A single `All Models` group is
   the project-less default view and is refused. These pools are Antigravity's
   general limits, so they are recorded as long-window metrics.
   **Verified live 2026-11-21:** each bucket names its window:
   `{"groups": [{"displayName": "...", "description": "...", "buckets":
   [{"bucketId": "...", "displayName": "...", "description": "...",
   "remainingFraction": 0.937382, "resetTime": "2026-11-28T00:07:14Z",
   "window": "weekly"}]}]}`, one group with one weekly bucket on that
   account. Google's own `/usage` docs also describe a five-hour limit per
   group; none came back. A bucket whose `window` names hours or a session
   is read as a session window labelled "<pool> 5h"; any other, or none, is
   the weekly pool.
   **The sign-in this reads is the Antigravity CLI's** (`agy`). Its access
   token lasts about an hour and is renewed only when `agy` runs; the
   Antigravity editor keeps a sign-in of its own (encrypted, never read), so
   opening the editor does not renew it. Until `agy` runs again the row
   reads "token expired" with the hover "run agy".
3. Fallback: `fetchAvailableModels` (per-model `quotaInfo`; a spent model omits
   `remainingFraction` and keeps `resetTime`), on the same host. Asked only
   when the summary gave nothing usable: a 200 with an unknown or empty body,
   or a status other than 200, 401, 403, 429 and 5xx (a 3xx, which is never
   followed, another 2xx or a 4xx). Once it saved a cycle it goes first for
   an hour (the summary then becomes its fallback), so a cycle stays one
   request. A 429, a 5xx or no answer ends the cycle on the shared words
   without the other endpoint: they say nothing about the body.

If both `retrieveUserQuotaSummary` and `fetchAvailableModels` return nothing
usable, the widget reports `"Antigravity quota unavailable"` instead of
quota. There is no further fallback to `retrieveUserQuota`: that endpoint
answers the Code Assist *bucket* view, which does NOT track Antigravity
consumption — for an account that only uses Antigravity, those buckets read
`remainingFraction: 1` regardless of actual usage.

used % = `(1 - remainingFraction) * 100`; the epoch placeholder
(`1970-01-01…`) in `resetTime` is parsed as "no reset known". 401/403 on any
of the calls → `"token rejected"` (the hover adds "run agy"), and the
cached project id is forgotten, so after a switch to another Google account
the next cycle asks `loadCodeAssist` again. A pool name the app does not know
is shown as the server sent it, cleaned: no control characters, at most 32
characters, `Pool` when nothing is left. Parsers walk the JSON tolerantly so
a shape change degrades to an honest error. The hosts (`cloudcode-pa` / `daily-` twin) are
flaky — individual requests time out routinely; the widget's normal
retry/retention covers that.

## Grok (SuperGrok subscription)

Token source: the **Grok CLI login**, `auth.json` in the CLI's home —
`%GROK_HOME%\auth.json` when `GROK_HOME` is set to an absolute path,
otherwise `%USERPROFILE%\.grok\auth.json` (the CLI's own rule; a relative
`GROK_HOME`: no file is read at all). Read only when `[reads] sign_in`
allows it. READ-ONLY: the CLI refreshes its token whenever it runs; the
widget never does, and never starts the CLI to make it.

File shape (verified 2026-11-10): an object keyed `"<issuer>::<client-id>"`,
each value a session record:

```json
{"https://auth.x.ai::...": {"key": "...", "refresh_token": "...",
  "expires_at": "RFC3339", "auth_mode": "oidc",
  "user_id": "...", "email": "...", "team_id": "...",
  "oidc_issuer": "https://auth.x.ai", "oidc_client_id": "..."}}
```

`key` is the access token (a JWT that lives about 6 hours). Only xAI's
sessions count: one whose `oidc_issuer` (or, without it, the issuer in its
key) is not an https address on `x.ai` or `grok.com` is passed over, so a
token another issuer gave never goes to xAI. Sessions past `expires_at` are
skipped; of the rest the one valid longest is used, and a session with no
stated expiry comes last (the server judges it). Then:

1. A usable session → one billing request (below).
2. Sessions exist, but all have expired → `NetworkError("token expired")`;
   the last real data stays on screen, greyed with its age, and the hover
   adds "run Grok CLI".
3. No file, or no session in it → `NotConfigured` (the row is hidden).
4. A file that cannot be read (locked, access denied, any read error but a
   missing file) or does not parse → `NetworkError("auth.json unreadable")`;
   it is read again on the next cycle.

Expired or unreadable, there is no other source to fall back to.

Endpoint (verified 2026-11-10, HTTP 200; the CLI's own billing call):

```
GET https://cli-chat-proxy.grok.com/v1/billing?format=credits
Authorization: Bearer <key>
x-xai-token-auth: xai-grok-cli
Accept: application/json
```

```json
{"config": {
   "currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY",
                     "start": "2026-11-09T23:01:28.407+00:00",
                     "end":   "2026-11-16T23:01:28.407+00:00"},
   "creditUsagePercent": 1.0,
   "productUsage": [{"product": "GrokBuild", "usagePercent": 1.0}],
   "isUnifiedBillingUser": true, "prepaidBalance": {"val": 0},
   "topUpMethod": "TOP_UP_METHOD_SAVED_PAYMENT_METHOD",
   "billingPeriodStart": "2026-11-09T23:01:28.407+00:00",
   "billingPeriodEnd":   "2026-11-16T23:01:28.407+00:00"}}
```

→ one long-window metric `Weekly` = `creditUsagePercent` (0–100), reset =
`currentPeriod.end` (fallback `billingPeriodEnd`). This is the "Weekly
SuperGrok Limit" that grok.com's Usage page shows. A period type other than
weekly is labelled `Daily` / `Monthly`, or `Limit` when unknown. Each
`productUsage` entry follows as its own long-window metric: that product's
share of the same limit (`GrokBuild` → `Build`; other names, such as Chat,
pass through, cleaned: no control characters, at most 32 characters). These
shares are marked for their extra row only: never the Grok row's headline,
its reset or its alerts. Build has a row of its own (`grok.build`) that can
be ticked in *Rows and models...*; the other products stay in the data.

The body is protobuf JSON, which leaves zero values out: before any use the
same call answered with no `creditUsagePercent` and no `productUsage` at all.
So a 200 with a well-formed `currentPeriod` and no percent is 0% — and only
then; a body with neither is an unknown schema. A `creditUsagePercent` that
is there but is neither a number nor a string holding one is an unknown
schema too, never 0%. An unknown schema, a body
that is not JSON, or any non-200 is an honest `NetworkError`:
`unrecognized billing schema`, `token rejected` (401/403; the hover adds
"run Grok CLI") or the shared words (see Error states).

Not to be confused with:

- the plain `/v1/billing` (no `format`): it answers the monthly on-demand API
  spend (`monthlyLimit`, `used`, `onDemandCap`), not the subscription limit;
- the HTTP 429 with `subscription:free-usage-exhausted` that an account
  without SuperGrok gets: that comes from the CLI's chat API, not from this
  call. What the billing call answers for such an account is not verified;
  whatever it is, a non-200 degrades honestly.

## Cursor (subscription only)

Token source: **only the widget's own session** — Credential Manager label
`cursor_session` (Rows and models: Cursor, Sign-in, *Sign in to
Cursor...*; CLI: `ailimits-auth login cursor`). The widget signs in to
Cursor by itself, on Cursor's own sign-in page, and owns this session, so
it is the one Cursor
token the widget ever refreshes. Nothing of the Cursor editor is read for
it: not its state database, not its token, and the editor's own sign-in is
never touched. Without a session → `NotConfigured`: the row is hidden until
the user signs in.

The editor's state database (`%APPDATA%\Cursor\User\globalStorage\state.vscdb`)
is read only for the model label, under `[reads] tool_settings` (see Model
labels below).

### Sign-in by link (verified live 2026-11-10)

The editor's own flow, bound by PKCE. There is no sign-in for other apps,
so the widget signs in as the Cursor editor's OAuth client (the
`client_id` below); the consent question and the privacy page say so.

1. verifier = unpadded base64url of 32 random bytes; challenge = unpadded
   base64url of the SHA-256 of the verifier's ASCII; uuid = a random v4
   UUID.
2. The browser opens
   `https://cursor.com/loginDeepControl?challenge=<challenge>&uuid=<uuid>&mode=login`
   and the user clicks *Sign in*. The app sends nothing to cursor.com
   itself.
3. The widget polls `GET https://api2.cursor.sh/auth/poll?uuid=<uuid>&verifier=<verifier>`:
   404 until the sign-in is approved, then 200
   `{"accessToken": "...", "refreshToken": "...", "authId": "...", "challenge": "...", "uuid": "..."}`.
   The two tokens were the same JWT (`sub` like `github|user_…`, lifetime
   about 60 days). The poll waits 1 s, then 1.2 times longer each time up
   to 8 s, and gives up after 5 minutes. Only the poll carries the
   verifier, and it is never logged.

The session is kept as UTF-8 JSON: the token, its `exp`, the last refresh
attempt, and whether Cursor ended it. In the widget the sign-in waits in a
background task; meanwhile Cursor's Sign-in reads "Waiting for the browser"
and offers *Cancel*. The end is said in the window ("Signed in.", "The
sign-in timed out after 5 minutes. Try again.", "The sign-in did not work.
Try again." or "Cancelled."), and as a toast only while the window is
closed. A sign-in switches Cursor on and shows its row. Signing in again
replaces the session (another account, or one Cursor ended); *Sign out*
there or `ailimits-auth logout cursor` forgets it, and the installer's
uninstaller runs the latter. From the window no restart is needed: the
provider is rebuilt, and an update that was already under way for the
previous session is dropped. `ailimits-auth login cursor` for a Cursor the
widget does not run yet switches it on in the config, and a running widget
takes that up within a few seconds: no restart either.

### Refresh (the own session only)

```
POST https://api2.cursor.sh/oauth/token
Content-Type: application/json

{"grant_type": "refresh_token", "client_id": "KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB",
 "refresh_token": "<token>"}
→ 200 {"access_token": "...", "id_token": "...", "shouldLogout": false}
```

Verified 2026-11-10 right after a sign-in: the new token differed, its `exp`
did not move, and the old token kept working. So the widget refreshes only
in the last 7 days before `exp`, at most once a day, and stores the new
`access_token` as both tokens. A session past its `exp` is not renewed: the
row asks to sign in again, and nothing is sent for it.
Only `shouldLogout: true` ends the session.
A refused refresh (401/403, not seen yet) keeps the session: it is noted
as the day's attempt, and the session is used until the service itself
refuses it. A refresh that gets no answer at all (offline, a time-out)
counts as the day's attempt too, so a PC that is offline does not try
again each cycle. The attempt is noted in the stored session and in the
provider's memory, so it counts even when Credential Manager does not take
the updated session. Whether a refresh near `exp` extends the session is not verified
yet; if it does not, a session lasts about 60 days and the row then asks
for a new sign-in.

### Usage (verified 2026-11-10, HTTP 200)

Three methods of the dashboard service the editor calls (Connect protocol,
JSON), all the same request:

```
POST https://api2.cursor.sh/aiserver.v1.DashboardService/<Method>
Authorization: Bearer <token>
Content-Type: application/json
Connect-Protocol-Version: 1

{}
```

**`GetCurrentPeriodUsage`** — every cycle, the headline:

```json
{"billingCycleStart": "1794265288407", "billingCycleEnd": "1794870088407",
 "planUsage": {"remainingBonus": false, "bonusTooltip": "...",
   "autoPercentUsed": 1.191765980498375, "totalPercentUsed": 1.191765980498375},
 "spendLimitUsage": {"limitType": "user"}, "displayThreshold": 200,
 "enabled": true, "displayMessage": "You've used 1% of your included usage",
 "autoModelSelectedDisplayMessage": "...",
 "namedModelSelectedDisplayMessage": "...",
 "autoBucketModels": ["default", "composer-2", "..."]}
```

→ one metric: `planUsage.totalPercentUsed` (0–100), reset at
`billingCycleEnd`. The cycle bounds are millisecond epochs written as
strings. The body is protobuf JSON, which leaves zero values out, so a
`planUsage` without `totalPercentUsed` beside a valid cycle is 0% — and
only then. No `planUsage` object, a body that is not JSON, or any other
shape → "unrecognized usage schema". Not shown yet: the auto / named-model
split.

**`GetPlanInfo`** — at most once an hour per account (ten minutes after a
request that brought nothing, keeping the last plan meanwhile); it labels
the row:

```json
{"planInfo": {"planName": "Premium", "price": "",
  "billingCycleEnd": "1796857287995",
  "includedUsagePeriod": "INCLUDED_USAGE_PERIOD_WEEKLY",
  "includedUsageResetsAt": "1794870088407"}}
```

→ the label is `planName` (`Premium`, cleaned: no control characters, at
most 32 characters); without one, the period (`includedUsagePeriod`:
`Weekly`, `Monthly`, `Daily`), else the cycle's length, else `Included`.

**`GetSandUsageStatus`** — Grok Bot, an extra metric, asked for only while
its row (`cursor.bot`) is chosen on the widget or the tooltip; choosing or
unchoosing that row rebuilds Cursor alone and keeps its data. The new
instance is fetched at once, and its plan cache and refusal count start
over, so it asks for the plan again:

```json
{"currentPeriodStart": "2026-11-09T23:01:28.407Z",
 "nextResetTimestampUtc": "2026-11-16T23:01:28.407Z",
 "usagePercent": 1.191765980498375, "hasAvailableUsage": true,
 "grokPlanLabel": "Grok Bot Plan", "cursorPlanName": "Premium",
 "isUnified": true, "billingBrand": "SAND_BILLING_BRAND_GROK"}
```

→ a second metric `Grok Bot` = `usagePercent`, reset at
`nextResetTimestampUtc`. A missing percent is 0% only beside a known period
and `hasAvailableUsage: true`; a body with everything left out (an account
without Grok Bot) gives no metric. It is marked for its extra row only, so
it never takes over the headline, the reset or the alerts. Until the row is
ticked, the picker says "Its share of the same limit, asked for once this
row is ticked". On the verified account Grok, Cursor and Grok Bot share ONE
weekly pool (the same reset, the same percentage); how to show that is an
open decision, so `isUnified` is carried as the metric's `unified_pool` for
a later display to use. Any failure of this call only leaves the metric
out.

Without a session (verified): HTTP 401
`{"code": "unauthenticated", ..., "debug": {"error": "ERROR_NOT_LOGGED_IN", ...}}`.

States (every error keeps the last data greyed with its age):

- the own session refused (401/403): within 7 days of `exp`, one refresh
  (at most one a day) and one more try; while the refusals in a row are
  fewer than three → `"token rejected, retrying"`; the third ends the
  session → `"sign in to Cursor again"`. Any other answer breaks the run;
- the own session past `exp`, or ended by `shouldLogout`, or a stored
  record this version cannot read → `"sign in to Cursor again"`, with no
  usage request and no refresh sent;
- Credential Manager not answering → `"Credential Manager unavailable"`;
- `"unrecognized usage schema"` as above; no answer and other statuses →
  the shared words (see Error states).

The hover adds "sign in to Cursor" to a status that neither ends in
", retrying" nor says it already; the row never says "check the key", as no
key was given. For the requests per cycle, see
Requests per cycle at the top.

## Model labels

The model each tool is set to (or last used) and its effort, shown on a
provider's own row when the Model, Version or Effort tick is on
(`src/models`). These are local reads only: no request is made for them and
nothing from these files is sent anywhere. Only the model id and the effort
leave a reader; no other part of any file is kept.

Each read is asked for first and is off by default:

- `[reads] tool_settings`: the tools' settings files (table below). Off: no
  settings file is opened.
- `[reads] last_session`: the end of the newest session file, used only with
  `[ui.model] source = "last_used"`. Off: no session file is opened.

They are read for every provider that runs (`Config::provider_active`),
however it signs in: also for one set up only with the app's own sign-in (a
pasted key or usage token, Cursor's sign-in of its own), whose tool's sign-in
is never read. The settings question says so. Unticking the last of Model,
Version and Effort on both surfaces sets both reads back to false;
switching back to "As set in the tool" sets `last_session` back to false.
At most one read runs at a time; one asked for while another runs is done
by the same worker right after it, and a read that was taken back reads
nothing.

### Settings files (`tool_settings`)

| Tool | File | Keys |
|---|---|---|
| Claude Code | `%USERPROFILE%\.claude\settings.json` | `ANTHROPIC_MODEL` (the file's `env` table, else the environment the app sees), else `model`; effort `modelSettings.<model>.effortLevel`, else that of the newest model of an alias's family, else the top-level `effortLevel` |
| Codex CLI | `%CODEX_HOME%\config.toml`, else `%USERPROFILE%\.codex\config.toml` | top-level `model`, `model_reasoning_effort`; with a top-level `profile = "x"`, those of `[profiles.x]` first |
| Grok CLI | `%GROK_HOME%\config.toml`, else `%USERPROFILE%\.grok\config.toml` | `[models] default`, `[models] default_reasoning_effort` |
| Copilot CLI | `%COPILOT_HOME%\settings.json`, else `%USERPROFILE%\.copilot\settings.json` | `model`, `effortLevel` (JSON with comments: comments and trailing commas are allowed) |
| Antigravity CLI | `%USERPROFILE%\.gemini\antigravity-cli\settings.json` | `model` (a display label: `Claude Sonnet 4.6 (Thinking)`; no separate effort) |
| Cursor (the editor) | `%APPDATA%\Cursor\User\globalStorage\state.vscdb` | one entry, `...persistentStorage.applicationUser`, and of it only `aiSettings.modelConfig.composer`: `modelName`, the `reasoning_effort` parameter |

`CODEX_HOME`, `GROK_HOME` and `COPILOT_HOME` count only as absolute paths
(the consent text's "when it is set" means that); a relative one means the
tool's files are not read at all (`config/paths.rs`).

Shapes verified 2026-11-11 on the owner's PC, key names only:

- Claude Code: no top-level `model` while the default model is in use (the
  default is written nowhere, so the label is the effort alone); the effort
  is kept per model under `modelSettings`, keyed by the model's full id.
  Verified live 2026-11-13 with Claude Code 2.1.323: `/model opus` writes
  the alias (`"model": "opus"`) while `/effort` saves under
  `modelSettings.claude-opus-5-5`, so an alias takes the effort of the
  newest full id of its family (the key's version can lag the model's, so
  the label then carries no version: "Opus · high"). With no `model`, the
  older top-level `effortLevel`, else the per-model efforts when they all
  agree (the owner's PC 2026-11-13: two models, both `xhigh`); when they
  differ, which one the default model takes is written nowhere, so none is
  shown. A context option is dropped from the id (`opus[1m]` reads as
  `opus`), so one rename covers both. `ANTHROPIC_MODEL` comes before
  `model`: from settings.json's own `env` table first, then the environment
  the app sees. This follows Claude Code's documented order (`/model`, then
  `--model`, then `ANTHROPIC_MODEL`, then the settings file); it was not
  verified live.
- Codex CLI: `model` and `model_reasoning_effort` at the top level. A
  top-level `profile = "x"` is followed: the keys of `[profiles.x]` override
  the top-level ones, a key the profile does not set keeps its top-level
  value, and a profile that is named but missing adds nothing. A model set
  only in a `[profiles.*]` table no profile chooses is not read.
- Grok CLI: `[models] default_reasoning_effort` present; `[models] default`
  is from the CLI's own docs (`user-guide/05-configuration.md`) and was not
  set there (the CLI's built-in default: the label is the effort alone).
- Copilot CLI: `model` (`"auto"`); `effortLevel` from its docs, not set there.
  GitHub documents the file as JSON with comments (JSONC): `//` and `/* */`
  comments outside strings and trailing commas are read past.
- Antigravity CLI: `model`, stored as the label the CLI shows; it is shown
  as written, never split at its hyphens (`GPT-OSS 120B (Medium)`).
- Cursor (verified 2026-11-16 on the owner's PC): the editor keeps the
  model chosen for each place in the entry
  `src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser`
  of its state database, under `aiSettings.modelConfig`; the Agent's is
  `composer`: `{"modelName": "grok-4.7", "maxMode": false,
  "selectedModels": [{"modelId": "grok-4.7", "parameters": [{"id":
  "reasoning_effort", "value": "high"}, {"id": "context", "value":
  "256k"}, ...]}]}`. `"default"` is the model Cursor calls Auto (shown as
  "Auto"). SQLite's `json_extract` hands over that one object: the rest of
  the entry (about 0.5 MB of the editor's settings) and every other entry,
  the editor's sign-in among them, never leave the database. A Windows
  SQLite without JSON functions hands over the entry whole only when it is
  at most 8 MB; the size is checked inside SQLite, so a larger entry never
  leaves the database, and the object is then taken out in the app. Read
  again only when the database or its `-wal` file changed; one lookup took
  1.3 ms. The Cursor CLI keeps its own model in
  `%USERPROFILE%\.cursor\cli-config.json` (with the account's e-mail beside
  it); it is not read.

  **Nothing is ever written to or created in `%APPDATA%\Cursor`.** The
  database belongs to the editor, which may hold it open and write to it
  (it runs in WAL mode). It is read through the SQLite that ships with
  Windows (`System32\winsqlite3.dll`, loaded on first use) with a 200 ms
  busy timeout, in one of three ways: while Cursor is closed (no
  `state.vscdb-wal` beside it) as `file:...?immutable=1`, read-only, with no
  lock and no `-wal` or `-shm` created; while Cursor runs, read-only, reading
  Cursor's own `-wal` and `-shm` as they are and creating nothing; a
  rollback-journal database read-only the plain way. A held lock is a
  transient error that the next cycle retries.

A file with an effort but no model gives the effort alone ("xhigh"), and no
name to rename.

Never read for labels: `%USERPROFILE%\.claude.json`,
`%USERPROFILE%\.copilot\config.json`, `command-history-state.json`, any
sign-in file, any entry of Cursor's database but the one above, the Windows
Credential Manager.

### Last used (`last_session`)

Only for the two tools that keep sessions; the others show their configured
model. The newest file is read back from its end 64 KB at a time until a
model record turns up, at most its last 1 MB: Claude Code 2.1.323 writes a
110-160 KB `prompt_snapshot` record right after a session's first answer,
which put that answer past a 64 KB tail in every fresh session (measured
2026-11-13 on the owner's PC: the newest model record sat within 1 MB of the
end in 97.9% of the states a session file passed through).

- **Claude Code**: the newest `%USERPROFILE%\.claude\projects\<project>\<session>.jsonl`,
  looked for in the 32 project folders a session was most recently started
  in, top level only: `<session>\subagents\...` holds subagent transcripts,
  whose model is not the user's choice, and is never read. The model is the
  last record with `"type": "assistant"` that is not `isSidechain`:
  `message.model` (a `<synthetic>` model, written by Claude Code itself, is
  skipped) and the record's `effort`.
- **Codex CLI**: the newest `rollout-*.jsonl` in the two newest day folders
  of `sessions\YYYY\MM\DD` (under `CODEX_HOME` when set). The model is the
  last `turn_context` record (`payload.model`, `payload.effort`) or `event_msg`
  of type `thread_settings_applied` (`payload.thread_settings.model`,
  `.reasoning_effort`). Newer Codex versions compress finished sessions to
  `.jsonl.zst` (every session on the owner's PC was): a compressed newest
  session cannot be read without a decoder, so the label falls back to the
  configured model. In the uncompressed sessions the last model record sat
  2 KB to 850 KB from the end: within the 1 MB read.

No model in the tail: the label keeps the model last found, in this file
or in the session before it (a session just started has no answer yet),
else falls back to the configured model when `tool_settings` allows, else
shows nothing.

What a label cannot know:

- a session's `/model`, the `--model`, `--profile` and `-m` flags, a
  project's `.claude/settings*.json`, and variables set only in a shell
  profile are not visible to the app;
- an alias's effort is that of the newest saved model of its family, so
  after a new model ships, until `/effort` is set for it, the label can show
  the older model's effort;
- in Last used, tools without sessions (and sessions without a model yet)
  show the configured model, unmarked;
- with no Claude Code settings.json at all, `ANTHROPIC_MODEL` alone gives no
  label.

### Bounds and failures

- Settings files over 256 KB are not read; a session file is read 64 KB at
  a time and at most 1 MB from its end (memory: about 2 MB at most, while a
  line as long as the whole read is joined); a line that crosses a step is
  joined whole, a line cut by the 1 MB mark is dropped.
- A UTF-8 BOM and CRLF line ends are accepted. A missing file, broken JSON
  or TOML, a wrong type, a value longer than 128 bytes or with control
  characters: no label for that tool. Never an error state, never a panic.
- A file that cannot be read right now (held by a scanner or a backup, an
  I/O error, a session file cut short while it was read, Cursor's database
  busy) keeps the label shown and is read again on the next cycle. Only a
  file that was read decides the label.
- A file that was read is read again only when its modification time or
  length changed.
- Read once per update cycle (60 s minimum): the scheduler starts the read
  on one of the runtime's blocking threads (`spawn_blocking`), one read at
  a time (`ReadGate`: a cycle that finds a read still running only asks it
  to read once more); no watchers, timers or threads of its own. Finding the newest session is one folder
  listing per folder looked into, never a walk of every session. Measured
  2026-11-11 on the owner's PC (debug build): Claude Code 872 session files
  in 122 folders (a full walk 5.4 ms warm, the bounded search 0.7 ms);
  Codex CLI 114 files in 71 folders (a full walk 2.4 ms, the bounded search
  0.17 ms); a whole cycle with every read on and the files unchanged, about
  1 ms.

### Names

No table of names, so a new model needs no update: the generic rule drops
date stamps and `latest`, drops the provider's own name unless a version
follows it, joins version digits (`4-5` → `4.5`), capitalises, and joins an
acronym to its version with a hyphen (`GPT-6.1`). The model is the words
before the first version word (`Opus`, `GPT`), the version adds the rest
(`Opus 5.5`, `GPT-6.1 Sol`). Checked against 156 public model ids
(`tests/fixtures/model-names.tsv`). A name the user types in
`[ui.model.names]` replaces the automatic one and is split the same way.

Which models the picker offers to rename (a list closed until opened, then
one heading per tool) is a separate, fixed list per tool
(`src/models/catalog.rs`), plus the model the tool uses now, always first
and marked "in use". It names nothing: a model missing from it is named by
the rule all the same. Sources, 2026-11-13: Claude Code, the ids its
answers carried in one live call per model (`claude-fable-5-2`,
`claude-opus-5-5`, `claude-sonnet-5-5`, `claude-haiku-5-5`); Codex CLI,
`codex debug models` (the 8 not hidden); Grok CLI,
`~/.grok/models_cache.json` (4); Antigravity CLI, the labels it writes to
its settings.json (5); Copilot CLI, the 26 `copilot help config` lists;
Cursor, Auto only (the editor's list is not in a file read here, and
`cursor-agent models` needs the CLI signed in). The list is fixed in the
build, so it is brought up to date with every release. Only the tools that
run are listed. A model several tools share has one name, but a field under
each tool, marked "shared with ..." and "in use" where it is used; typing in
one shows in the others. A tool's own router (`auto`, `default`) has a name
per tool, kept under `"provider:id"` (`"copilot:auto"`, `"cursor:auto"`);
a name an older build kept under the bare `"auto"` serves both until one is
renamed, which splits it. The widget looks names up the same way, so the
picker and the widget agree.

## Removed providers and methods

- **Gemini API-key method** — the public Generative Language API exposes no
  rate-limit data (binary 200/429 only), so the original key-based Gemini
  provider was removed. The CURRENT Antigravity provider (section above) instead
  reads Antigravity's OAuth token and queries the Code Assist quota
  endpoints, which do return real usage: the pool summary first, the
  per-model quota as its fallback.
- **Legacy Gemini CLI sign-in** — `%USERPROFILE%\.gemini\oauth_creds.json`
  (`access_token`, `expiry_date` in epoch milliseconds) was Antigravity's
  fallback source until 0.7.1. Removed: Google stopped serving individual,
  Google AI Pro and Google AI Ultra accounts through Gemini CLI on
  2026-06-18.
- **Codex API-key probe** (paid chat/completions request for headers) and
  **Copilot Billing API** — removed; the subscription endpoints provide
  strictly more data for free. Recoverable from git history.
- **Cursor usage summary by cookie** — `GET https://cursor.com/api/usage-summary`
  with a `WorkosCursorSessionToken=<sub>%3A%3A<token>` cookie built from the
  editor's token (schema verified 2026-11-10 on a free plan:
  `individualUsage.plan.totalPercentUsed`, `membershipType`,
  `isUnlimited`). Superseded the same day by the dashboard service with the
  widget's own session, once the editor began encrypting its token.
- **Cursor's editor token** — reading the editor's own token from its state
  database, while it was still stored unencrypted, as a fallback without
  the widget's own session. Removed before 0.7.1 shipped: the widget never
  reads another app's sign-in from its files, and a key some credential
  stealers look for has no place in the exe. Only the widget's own sign-in
  (Rows and models, or `ailimits-auth login cursor`) is used.
