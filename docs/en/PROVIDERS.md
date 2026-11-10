# PROVIDERS.md — how the widget talks to each provider

Ground rules that apply to every provider:

- **Read-only tokens.** The widget reuses tokens owned by the official CLI
  tools (for Cursor, the editor itself) and NEVER refreshes or rotates them
  (rotation would log the CLI out).
  An expired token is a quiet fallback, not an error dialog.
- **Undocumented endpoints are treated as hostile.** Tolerant parsers; any
  unknown schema or non-200 status degrades honestly (NetworkError) instead
  of inventing numbers.
- **Secrets live only in the Windows Credential Manager**, service
  `ailimits`. The config stores labels, never values. Tokens are never logged.
- **Stale-data policy** (implemented in `app.rs` + `renderer.rs`): a transient
  error never wipes the last real data; it is shown greyed with its age.
  Data counts as stale once it has missed its next scheduled refresh: the
  update interval plus a minute, never under 5 minutes.
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
`quota_reset_date` → reset at midnight UTC.

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
| Cursor | ~60-day JWT, refreshed by the Cursor editor while it runs | until the token's `exp` (up to ~60 days after the editor last refreshed it), then grey |

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
   session. It is returned ONLY to an identified client (`User-Agent:
   antigravity/1.0` + `X-Goog-Api-Client: gl-go antigravity`); an anonymous
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
pass through). The widget does not draw the products yet; they are kept in
the data for a later per-product view.

The body is protobuf JSON, which leaves zero values out: before any use the
same call answered with no `creditUsagePercent` and no `productUsage` at all.
So a 200 with a well-formed `currentPeriod` and no percent is 0% — and only
then; a body with neither is an unknown schema. An unknown schema, a body
that is not JSON, or any non-200 is an honest `NetworkError`:
`unrecognized billing schema`, `token rejected` (401/403) or `HTTP <code>`.

Not to be confused with:

- the plain `/v1/billing` (no `format`): it answers the monthly on-demand API
  spend (`monthlyLimit`, `used`, `onDemandCap`), not the subscription limit;
- the HTTP 429 with `subscription:free-usage-exhausted` that an account
  without SuperGrok gets: that comes from the CLI's chat API, not from this
  call. What the billing call answers for such an account is not verified;
  whatever it is, a non-200 degrades honestly.

## Cursor (subscription only)

Token source: the Cursor editor's own session, kept in its VS Code state
database `%APPDATA%\Cursor\User\globalStorage\state.vscdb` (SQLite, table
`ItemTable(key, value)`, key `cursorAuth/accessToken`; READ-ONLY, never
refreshed). The token is a JWT (issuer `https://authentication.cursor.sh`,
audience `https://cursor.com`, lifetime ~60 days) that the editor refreshes
while it runs. The widget decodes only its payload, for `sub` and `exp`, and
never sends a token past its `exp`. The same table also holds
`cursorAuth/refreshToken`, `cursorAuth/cachedEmail`,
`cursorAuth/stripeMembershipType` and `cursorAuth/cachedSignUpType`; the
widget reads none of them.

The database belongs to the editor, which may hold it open and write to it
(it runs in WAL mode). It is read through the SQLite that ships with Windows
(`System32\winsqlite3.dll`, loaded on first use), opened
`SQLITE_OPEN_READONLY` with a 200 ms busy timeout. As with any SQLite reader,
only the shared-memory index (`state.vscdb-shm`) is touched; the database and
its `-wal` file are never written. A held lock is a transient error that the
next cycle retries.

Endpoint (response schema verified 2026-11-10, HTTP 200, on a free plan; the
cursor.com dashboard reads the same one):

```
GET https://cursor.com/api/usage-summary
Cookie: WorkosCursorSessionToken=<sub>%3A%3A<token>
Accept: application/json
```

`sub` reads like `github|user_…`: the sign-in provider, then the account id.
The cookie carries it whole; `%3A%3A` is a literal, already URL-encoded `::`.
Verified 2026-11-10 with a freshly issued token of the same account (a
one-off sign-in by link; the editor's stored token had expired 2026-09-29).
A request built from the editor's own stored token is the same request, but
awaits a live check once the editor is signed in again.

```json
{"billingCycleStart": "2026-10-28T20:49:51.046Z",
 "billingCycleEnd":   "2026-11-28T20:49:51.046Z",
 "membershipType": "free", "limitType": "user", "isUnlimited": false,
 "autoModelSelectedDisplayMessage":  "You've used 0% of your included total usage",
 "namedModelSelectedDisplayMessage": "You've used 0% of your included API usage",
 "individualUsage": {
   "plan": {"enabled": true, "used": 0, "limit": 0, "remaining": 0,
            "breakdown": {"included": 0, "bonus": 0, "total": 0},
            "autoPercentUsed": 0, "apiPercentUsed": 0, "totalPercentUsed": 0},
   "onDemand": {"enabled": false, "used": 0, "limit": null, "remaining": null}},
 "teamUsage": {}}
```

→ one metric per billing cycle: `individualUsage.plan.totalPercentUsed`
(0–100; `used / limit` when the percentage is missing, both in cents), reset
at `billingCycleEnd`, labelled with `membershipType` (`free` → `Free`, `pro`
→ `Pro`). `isUnlimited: true` → an `Unlimited` row with no percentage and no
reset, never a 0% bar. A disabled `individualUsage.plan`, malformed JSON or
any other shape → "unrecognized usage schema". Not shown yet: the auto/API
split (`autoPercentUsed` / `apiPercentUsed`; a spent API pool still leaves
Auto working, so it cannot be a long window that takes over the headline)
and on-demand spend. Team limits (`limitType: "team"`, `teamUsage`) are
unverified.

States: no `state.vscdb` (Cursor not installed) or no token (signed out) →
`NotConfigured`, the row is hidden; a locked database → "state database busy,
retrying"; a token past `exp` → "token expired", with no request sent (opening
Cursor refreshes it); 401/403 → "token rejected"; any other status →
"HTTP <code>". Every error keeps the last data greyed with its age.

## Removed providers and methods

- **Gemini API-key method** — the public Generative Language API exposes no
  rate-limit data (binary 200/429 only), so the original key-based Gemini
  provider was removed. The CURRENT Antigravity provider (section above) instead
  reads Antigravity / legacy Gemini CLI OAuth tokens and queries the Code Assist
  quota endpoint, which does return real per-model usage.
- **Codex API-key probe** (paid chat/completions request for headers) and
  **Copilot Billing API** — removed; the subscription endpoints provide
  strictly more data for free. Recoverable from git history.
