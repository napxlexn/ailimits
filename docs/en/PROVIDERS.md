# PROVIDERS.md — how the widget talks to each provider

Ground rules that apply to every provider:

- **Read-only tokens.** The widget reuses tokens owned by the official CLI
  tools and NEVER refreshes or rotates them (rotation would log the CLI out).
  An expired token is a quiet fallback, not an error dialog. The one session
  the widget refreshes is its own Cursor session, which no other app uses
  (see Cursor).
- **Undocumented endpoints are treated as hostile.** Tolerant parsers; any
  unknown schema or non-200 status degrades honestly (NetworkError) instead
  of inventing numbers.
- **Secrets live only in the Windows Credential Manager**, service
  `ailimits`. The config stores labels, never values. Tokens are never logged.
- **Stale-data policy** (implemented in `app.rs` + `renderer.rs`): a transient
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

## Claude

### Subscription (default) — source chain in `fetch_via_subscription`

1. **Manual usage token** — Credential Manager label `claude_usage_token`
   (menu: *Paste usage token*; CLI: `ailimits-auth set-usage-token claude`).
2. **Claude Code OAuth token** — `%USERPROFILE%\.claude\.credentials.json`
   → `claudeAiOauth.accessToken` (skipped when `expiresAt` is in the past).
3. A source exists (either token: a stored usage token counts as much as
   the credentials file) but gave no data → `NetworkError("token expired")`,
   or what actually failed: `"rate-limited, retrying"` after an HTTP 429,
   `"offline, retrying"` when a request got no answer, or
   `"unrecognized usage schema"` for a 200 the parser does not know.
4. No sources at all → `NotConfigured` (the row is hidden).

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
looks exactly as before until that row is chosen.

**Rate limiting (verified 2026-07-09).** The endpoint's per-token bucket is
shared with Claude Code's own polling: under an active session roughly one
widget request in three returns 200, the rest get HTTP 429 with
`Retry-After: 0`. A bounced request falls through the source chain (not a
token problem); the display keeps the last data. Net effect: a Claude
refresh occasionally takes 2–3 minutes — a server-side ceiling.

### API key (optional)

`auth_method = "api_key"`, label `claude_api_key`. One request to
`GET /v1/models` with `x-api-key`; limits come from the
`anthropic-ratelimit-*` response headers (requests + tokens).
This monitors API rate limits, not the subscription.

## OpenAI Codex (subscription only)

Token sources, in order:

1. **Manual usage token** — label `codex_usage_token` (menu / CLI).
2. **Codex CLI token** — `%USERPROFILE%\.codex\auth.json`
   → `tokens.access_token`.

Endpoint (verified 2026-06-10; the Codex CLI itself polls it):

```
GET https://chatgpt.com/backend-api/wham/usage
Authorization: Bearer <token>
```

```json
{"plan_type": "plus",
 "rate_limit": {
   "primary_window":   {"used_percent": 1,  "reset_at": 1781121633},
   "secondary_window": {"used_percent": 28, "reset_at": 1781553834}}}
```

`primary_window` = the 5-hour session → `Session`; `secondary_window` =
the week → `Weekly`. `reset_at` is unix epoch seconds. 401/403 on the CLI
token → `AuthError("token expired — run Codex CLI once")`. Without auth.json
a stored usage token that failed keeps the row (greyed, last data kept):
`AuthError("usage token rejected")` on 401/403, otherwise the network error.
An unreadable auth.json → `NetworkError("auth.json unreadable")`. No source
at all → `NotConfigured` (the row is hidden).

## GitHub Copilot (subscription only)

Token sources, in order:

1. **PAT** — label `copilot_pat` (menu: *Paste PAT*; CLI: `set copilot`).
2. **gh CLI** — `gh auth token`, spawned with `CREATE_NO_WINDOW` and cached
   in memory for 15 minutes (a 401 invalidates the cache).

Endpoint (verified 2026-06-10; the same one the Copilot extensions use):

```
GET https://api.github.com/copilot_internal/user
Authorization: token <token>
```

```json
{"copilot_plan": "individual", "quota_reset_date": "2026-07-01",
 "quota_snapshots": {
   "premium_interactions": {"entitlement": 50,  "remaining": 10, "unlimited": false},
   "chat":        {"entitlement": 200,  "remaining": -1},
   "completions": {"entitlement": 2000, "remaining": 2000}}}
```

→ metrics `Premium`, `Chat`, `Completions` (used = entitlement − remaining;
negative `remaining` = overage; `unlimited` snapshots are skipped).
`quota_reset_date` → reset at midnight UTC. An unknown schema or a body that
is not JSON → `NetworkError("unrecognized copilot_internal schema")`;
401/403/404 → `AuthError("token has no Copilot access")`; any other status →
`HTTP <code>`; a body cut off mid-transfer → the network error.

## Token lifetimes and accuracy window

The CLI tokens are short-lived OAuth access tokens; the owning CLI refreshes
them on use. The widget never refreshes (rotation would log the CLI out —
confirmed in the wild: NousResearch/hermes-agent#22903, where a sibling
client's refresh invalidated the others). Consequences:

| Provider | Token lifetime | Live-data window without action |
|---|---|---|
| Claude | ~8h access token (verified), refreshed by Claude Code on use | ~8h after the last Claude Code session, then fallback/grey |
| Codex | short-lived, refreshed by Codex CLI on each run | only while Codex CLI is in active use |
| Copilot | gh manages its own token | indefinite while logged into gh |
| Antigravity | keyring token, then the legacy Gemini CLI token | only while one of those sources is refreshed, then fallback/grey |
| Grok | ~6h access token (verified), refreshed by the Grok CLI on use | up to ~6h after the last Grok CLI session, then fallback/grey |
| Cursor | the widget's own session: a ~60-day JWT, refreshed by the widget in its last week (whether that extends it is not verified yet) | until the session ends, then grey with "sign in to Cursor again" |

There is no `gh auth token`-style "print/refresh the token" command for the
Codex or Claude CLIs (verified against the OpenAI Codex auth docs). The only
CLI-driven refresh path is Copilot's `gh auth token`. For a credential that
does not depend on CLI activity, the official recommendation (and the
widget's manual path) is an API key / PAT — see CONFIG.md for the labels.

## Antigravity (Code Assist subscription)

Token sources, in order:

1. **Antigravity CLI** — Windows Credential Manager target
   `gemini:antigravity` → JSON `token.access_token` (READ-ONLY; skipped when
   `token.expiry` is in the past).
2. **Legacy Gemini CLI** — `~/.gemini/oauth_creds.json` → `access_token`
   (READ-ONLY; skipped when `expiry_date` is in the past).
3. A token exists but every one is expired → `NetworkError("token expired")`:
   the last data stays, greyed with its age, until Antigravity refreshes the
   token. A credential that cannot be read →
   `NetworkError("credentials unreadable")`. No token anywhere →
   `NotConfigured` (the row is hidden).

As of June 18, 2026, Google stopped serving Gemini CLI and Gemini Code Assist
IDE-extension requests for Gemini Code Assist for individuals, Google AI Pro,
and Google AI Ultra. Gemini CLI remains supported for Gemini Code Assist
Standard/Enterprise and paid Gemini / Gemini Enterprise Agent Platform API-key
flows. Individual users are expected to migrate to Antigravity CLI; Antigravity
stores session tokens in the OS keyring. On Windows, current Antigravity CLI
stores the consumer session under `gemini:antigravity`.

Quota is a chain of sources (all verified live 2026-08-01):

1. `loadCodeAssist` (empty body) → `cloudaicompanionProject`, cached per
   session until a 401/403 (another Google account signed in: the next
   cycle asks again with the new token). It is returned ONLY to an
   identified client (`User-Agent: antigravity/1.0` +
   `X-Goog-Api-Client: gl-go antigravity`); an anonymous
   call still answers 200 but omits the field. Everything below needs the id,
   and without it the widget reports an honest error instead of quota — every
   endpoint answers an "everything full" default view when the project is
   missing, which is indistinguishable from an unused account.
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
3. Fallback: `fetchAvailableModels` (per-model `quotaInfo`; a spent model omits
   `remainingFraction` and keeps `resetTime`).

If both `retrieveUserQuotaSummary` and `fetchAvailableModels` fail or return
nothing usable, the widget reports an honest error instead of quota. There is
no further fallback to `retrieveUserQuota`: that endpoint answers the Code
Assist *bucket* view, which does NOT track Antigravity consumption — for an
account that only uses Antigravity, those buckets read `remainingFraction: 1`
regardless of actual usage. Both `retrieveUserQuotaSummary` and
`fetchAvailableModels` live on `daily-cloudcode-pa.googleapis.com`, which is
flaky (see below), so a single timeout can lose both in one poll cycle — that
is expected to surface as an error, not as a silently wrong "0% used".

used % = `(1 - remainingFraction) * 100`; the epoch placeholder
(`1970-01-01…`) in `resetTime` is parsed as "no reset known". 401/403 →
"Antigravity token rejected". Parsers walk the JSON tolerantly so a shape change
degrades to an honest error. The hosts (`cloudcode-pa` / `daily-` twin) are
flaky — individual requests time out routinely; the widget's normal
retry/retention covers that.

## Grok (SuperGrok subscription)

Token source: the **Grok CLI login**, `auth.json` in the CLI's home —
`%GROK_HOME%\auth.json` when `GROK_HOME` is set, otherwise
`%USERPROFILE%\.grok\auth.json` (the CLI's own rule). READ-ONLY: the CLI
refreshes its token whenever it runs; the widget never does, and never starts
the CLI to make it.

File shape (verified 2026-11-10): an object keyed `"<issuer>::<client-id>"`,
each value a session record:

```json
{"https://auth.x.ai::...": {"key": "...", "refresh_token": "...",
  "expires_at": "RFC3339", "auth_mode": "oidc",
  "user_id": "...", "email": "...", "team_id": "...",
  "oidc_issuer": "https://auth.x.ai", "oidc_client_id": "..."}}
```

`key` is the access token (a JWT that lives about 6 hours). Sessions past
`expires_at` are skipped; of the rest the one valid longest is used, and a
session with no stated expiry comes last (the server judges it). Then:

1. A usable session → one billing request (below).
2. Sessions exist, but all have expired → `NetworkError("token expired")`;
   the last real data stays on screen, greyed with its age, and the hover
   adds "run Grok CLI".
3. No file, or no session in it → `NotConfigured` (the row is hidden).
4. A file that does not parse → `NetworkError("auth.json unreadable")`; it
   is read again on the next cycle.

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
pass through). Build has a row of its own (`grok.build`) that can be ticked
in *Rows and models...*; the other products stay in the data.

The body is protobuf JSON, which leaves zero values out: before any use the
same call answered with no `creditUsagePercent` and no `productUsage` at all.
So a 200 with a well-formed `currentPeriod` and no percent is 0% — and only
then; a body with neither is an unknown schema. An unknown schema, a body
that is not JSON, or any non-200 is an honest `NetworkError`:
`unrecognized billing schema`, `token rejected` (401/403) or `HTTP <code>`;
a body cut off mid-transfer is the network error.

Not to be confused with:

- the plain `/v1/billing` (no `format`): it answers the monthly on-demand API
  spend (`monthlyLimit`, `used`, `onDemandCap`), not the subscription limit;
- the HTTP 429 with `subscription:free-usage-exhausted` that an account
  without SuperGrok gets: that comes from the CLI's chat API, not from this
  call. What the billing call answers for such an account is not verified;
  whatever it is, a non-200 degrades honestly.

## Cursor (subscription only)

Token sources, in order:

1. **The widget's own session** — Credential Manager label `cursor_session`
   (menu: *Providers → Cursor → Sign in to Cursor...*; CLI:
   `ailimits-auth login cursor`). The widget signs in to Cursor by itself
   and owns this session, so it is the one Cursor token the widget ever
   refreshes.
2. **The editor's token, only while it is plain** — the Cursor editor's VS
   Code state database `%APPDATA%\Cursor\User\globalStorage\state.vscdb`
   (SQLite, table `ItemTable(key, value)`, key `cursorAuth/accessToken`;
   READ-ONLY, never refreshed, never sent past its `exp`), tried only when
   there is no own session. Current Cursor versions encrypt this value at
   rest: `cursor-safe-storage:v1:` + base64 of `v10` and AES-256-GCM, the
   key in `%APPDATA%\Cursor\Local State` (`os_crypt.encrypted_key`, under
   DPAPI). The widget never decrypts it — decrypting another app's stored
   credentials is what credential stealers do, and antivirus engines flag
   it. A value with the `cursor-safe-storage:` prefix counts as no token.
3. Neither → `NotConfigured`: the row is hidden until the user signs in.

The editor's database belongs to the editor, which may hold it open and
write to it (it runs in WAL mode). It is read through the SQLite that ships
with Windows (`System32\winsqlite3.dll`, loaded on first use), opened
`SQLITE_OPEN_READONLY` with a 200 ms busy timeout. As with any SQLite
reader, only the shared-memory index (`state.vscdb-shm`) is touched; the
database and its `-wal` file are never written. A held lock is a transient
error that the next cycle retries.

### Sign-in by link (verified live 2026-11-10)

The editor's own flow, bound by PKCE:

1. verifier = unpadded base64url of 32 random bytes; challenge = unpadded
   base64url of the SHA-256 of the verifier's ASCII; uuid = a random v4
   UUID.
2. The browser opens
   `https://cursor.com/loginDeepControl?challenge=<challenge>&uuid=<uuid>&mode=login`
   and the user clicks *Sign in*.
3. The widget polls `GET https://api2.cursor.sh/auth/poll?uuid=<uuid>&verifier=<verifier>`:
   404 until the sign-in is approved, then 200
   `{"accessToken": "...", "refreshToken": "...", "authId": "...", "challenge": "...", "uuid": "..."}`.
   The two tokens were the same JWT (`sub` like `github|user_…`, lifetime
   about 60 days). The poll waits 1 s, then 1.2 times longer each time up
   to 8 s, and gives up after 5 minutes. Only the poll carries the
   verifier, and it is never logged.

The session is kept as UTF-8 JSON: the token, its `exp`, the last refresh
attempt, and whether Cursor ended it. In the widget the sign-in waits in a
background task and ends with a toast: "Signed in to Cursor", "Cursor
sign-in timed out" or "Cursor sign-in failed". Signing in again replaces
the session (another account, or one Cursor ended); *Sign out of Cursor* or
`ailimits-auth logout cursor` forgets it. No restart is needed either way:
the provider reads the session on every update.

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
`access_token` as both tokens. `shouldLogout: true` ends the session, and so
does a refused refresh (401/403, not seen yet). Whether a refresh near `exp`
extends the session is not verified yet; if it does not, a session lasts
about 60 days and the row then asks for a new sign-in.

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

→ the label is `planName` (`Premium`); without one, the period
(`includedUsagePeriod`: `Weekly`, `Monthly`, `Daily`), else the cycle's
length, else `Included`.

**`GetSandUsageStatus`** — Grok Bot, every cycle, an extra metric:

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
without Grok Bot) gives no metric. It is a session-window metric, so it
never takes over the headline; it shows as a row of its own (`cursor.bot`)
when ticked in *Rows and models...*. On the
verified account Grok, Cursor and Grok Bot share ONE weekly pool (the same
reset, the same percentage); how to show that is an open decision, so
`isUnified` is carried as the metric's `unified_pool` for a later display
to use. Any failure of this call only leaves the metric out.

Without a session (verified): HTTP 401
`{"code": "unauthenticated", ..., "debug": {"error": "ERROR_NOT_LOGGED_IN", ...}}`.

States (every error keeps the last data greyed with its age):

- the own session refused (401/403): one refresh if within 7 days of `exp`,
  then one more try; otherwise the session is marked ended →
  `AuthError("sign in to Cursor again")`;
- the own session past `exp`, or ended by `shouldLogout` → the same, with
  no usage request sent;
- the editor's token refused → "token rejected"; past its `exp` → "token
  expired", with no request sent; a locked database → "state database busy,
  retrying";
- no answer → "offline, retrying"; any other status → "HTTP <code>".

Where a sign-in is the fix, the hover adds "sign in to Cursor".
Requests per cycle: two (usage and Grok Bot), plus the plan at most hourly
and a refresh at most daily in a session's last week.

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

### Settings files (`tool_settings`)

| Tool | File | Keys |
|---|---|---|
| Claude Code | `%USERPROFILE%\.claude\settings.json` | `model`; effort `modelSettings.<model>.effortLevel`, else that of the newest model of an alias's family, else the top-level `effortLevel` |
| Codex CLI | `%CODEX_HOME%\config.toml`, else `%USERPROFILE%\.codex\config.toml` | top-level `model`, `model_reasoning_effort` |
| Grok CLI | `%GROK_HOME%\config.toml`, else `%USERPROFILE%\.grok\config.toml` | `[models] default`, `[models] default_reasoning_effort` |
| Copilot CLI | `%COPILOT_HOME%\settings.json`, else `%USERPROFILE%\.copilot\settings.json` | `model`, `effortLevel` |
| Antigravity CLI | `%USERPROFILE%\.gemini\antigravity-cli\settings.json` | `model` (a display label: `Claude Sonnet 4.6 (Thinking)`; no separate effort) |
| Cursor (the editor) | `%APPDATA%\Cursor\User\globalStorage\state.vscdb` | one entry, `...persistentStorage.applicationUser`, and of it only `aiSettings.modelConfig.composer`: `modelName`, the `reasoning_effort` parameter |

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
  `opus`), so one rename covers both.
- Codex CLI: `model` and `model_reasoning_effort` at the top level; a model
  set only in a `[profiles.*]` table is not read.
- Grok CLI: `[models] default_reasoning_effort` present; `[models] default`
  is from the CLI's own docs (`user-guide/05-configuration.md`) and was not
  set there (the CLI's built-in default: the label is the effort alone).
- Copilot CLI: `model` (`"auto"`); `effortLevel` from its docs, not set there.
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
  "Auto"). The database is opened read-only through Windows' own SQLite,
  as for the sign-in, and SQLite's `json_extract` hands over that one
  object: the rest of the entry (about 0.5 MB of the editor's settings)
  and every other entry, the sign-in among them, never leave the database.
  A Windows SQLite without JSON functions gives the entry whole (read only
  up to 8 MB) and the object is taken out in the app. Read again only when
  the database or its `-wal` file changed; one lookup took 1.3 ms. The
  Cursor CLI keeps its own model in `%USERPROFILE%\.cursor\cli-config.json`
  (with the account's e-mail beside it); it is not read.

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

### Bounds and failures

- Settings files over 256 KB are not read; a session file is read 64 KB at
  a time and at most 1 MB from its end (memory: at most 1 MB plus a step);
  a line that crosses a step is joined whole, a line cut by the 1 MB mark
  is dropped.
- A UTF-8 BOM and CRLF line ends are accepted. A missing file, broken JSON
  or TOML, a wrong type, a value longer than 128 bytes or with control
  characters: no label for that tool. Never an error state, never a panic.
- A file is read again only when its modification time or length changed.
- Read once per update cycle (60 s minimum) from the scheduler: no
  watchers, threads or timers. Finding the newest session is one folder
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
`cursor-agent models` needs the CLI signed in). Only the tools that run are
listed; a model two tools share has one name, so one field, under the
first.

## Removed providers and methods

- **Gemini API-key method** — the public Generative Language API exposes no
  rate-limit data (binary 200/429 only), so the original key-based Gemini
  provider was removed. The CURRENT Antigravity provider (section above) instead
  reads Antigravity / legacy Gemini CLI OAuth tokens and queries the Code Assist
  quota endpoint, which does return real per-model usage.
- **Codex API-key probe** (paid chat/completions request for headers) and
  **Copilot Billing API** — removed; the subscription endpoints provide
  strictly more data for free. Recoverable from git history.
- **Cursor usage summary by cookie** — `GET https://cursor.com/api/usage-summary`
  with a `WorkosCursorSessionToken=<sub>%3A%3A<token>` cookie built from the
  editor's token (schema verified 2026-11-10 on a free plan:
  `individualUsage.plan.totalPercentUsed`, `membershipType`,
  `isUnlimited`). Superseded the same day by the dashboard service with the
  widget's own session, once the editor began encrypting its token.
