# PROVIDERS.md — how the widget talks to each provider

Ground rules that apply to every provider:

- **Read-only tokens.** The widget reuses tokens owned by the official CLI
  tools and NEVER refreshes or rotates them (rotation would log the CLI out).
  An expired token is a quiet fallback, not an error dialog.
- **Undocumented endpoints are treated as hostile.** Tolerant parsers; any
  unknown schema or non-200 status degrades honestly (NetworkError) instead
  of inventing numbers.
- **Secrets live only in the Windows Credential Manager**, service
  `ailimits`. The config stores labels, never values. Tokens are never logged.
- **Stale-data policy** (implemented in `app.rs` + `renderer.rs`): a transient
  error never wipes the last real data; it is shown greyed with its age.
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
3. **statusline.jsonl** — Claude Code status bar snapshots (tolerant field
   name search).
4. A source exists but gave no data → `NetworkError("token expired")`.
5. No sources at all → `NotConfigured` (the row is hidden).

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
token → `AuthError("token expired — run Codex CLI once")`.

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

As of June 18, 2026, Google stopped serving Gemini CLI and Gemini Code Assist
IDE-extension requests for Gemini Code Assist for individuals, Google AI Pro,
and Google AI Ultra. Gemini CLI remains supported for Gemini Code Assist
Standard/Enterprise and paid Gemini / Gemini Enterprise Agent Platform API-key
flows. Individual users are expected to migrate to Antigravity CLI; Antigravity
stores session tokens in the OS keyring. On Windows, current Antigravity CLI
stores the consumer session under `gemini:antigravity`.

Quota is a chain of three sources (all verified live 2026-07-09):

1. `loadCodeAssist` (empty body) → `cloudaicompanionProject`, cached per
   session. Everything below needs it.
2. **`fetchAvailableModels`** — the endpoint Antigravity's own quota manager
   uses, and the only one that tracks the pools Antigravity actually
   consumes. Requires the project id in the body AND an identified client
   (`User-Agent` + `X-Goog-Api-Client` headers), else 403:

   ```
   POST https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels
   Body: {"project": "<cloudaicompanionProject>"}
   → {"models": {"gemini-2.5-pro": {"displayName": "Gemini 2.5 Pro",
        "quotaInfo": {"remainingFraction": 0.63, "resetTime": "RFC3339"}}, …}}
   ```

   All Gemini models share one pool today (identical fraction + reset), so
   pools are deduped and represented by their newest Pro member. Antigravity
   also fronts Claude / GPT-OSS pools — skipped, they are not Gemini.
3. Fallback: `retrieveUserQuota` with `{"project": …}` (the Code Assist
   bucket view; does NOT track Antigravity usage), then with `{}` as the
   last resort — that default view reads `remainingFraction: 1` everywhere
   regardless of usage, which is what used to render as an eternal 0%.

used % = `(1 - remainingFraction) * 100`; the epoch placeholder
(`1970-01-01…`) in `resetTime` is parsed as "no reset known". 401/403 →
"Antigravity token rejected". Parsers walk the JSON tolerantly so a shape change
degrades to an honest error. The hosts (`cloudcode-pa` / `daily-` twin) are
flaky — individual requests time out routinely; the widget's normal
retry/retention covers that.

## Removed providers and methods

- **Gemini API-key method** — the public Generative Language API exposes no
  rate-limit data (binary 200/429 only), so the original key-based Gemini
  provider was removed. The CURRENT Antigravity provider (section above) instead
  reads Antigravity / legacy Gemini CLI OAuth tokens and queries the Code Assist
  quota endpoint, which does return real per-model usage.
- **Codex API-key probe** (paid chat/completions request for headers) and
  **Copilot Billing API** — removed; the subscription endpoints provide
  strictly more data for free. Recoverable from git history.
