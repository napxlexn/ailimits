# PROVIDERS.md — як віджет працює з кожним провайдером

Базові правила для всіх провайдерів:

- **Токени read-only.** Віджет повторно використовує токени офіційних
  CLI-інструментів і НІКОЛИ їх не оновлює та не ротує (ротація розлогінила б
  CLI). Прострочений токен — тихий fallback, а не діалог із помилкою.
- **Недокументовані endpoint'и вважаються ворожими.** Толерантні парсери;
  будь-яка незнайома схема чи статус ≠ 200 деградує чесно (NetworkError),
  а не вигадує цифри.
- **Секрети живуть лише у Windows Credential Manager**, service `ailimits`.
  Конфіг зберігає мітки, не значення. Токени ніколи не логуються.
- **Політика застарілих даних** (`app.rs` + `renderer.rs`): тимчасова
  помилка ніколи не затирає останні реальні дані — вони показуються сірими
  з давністю. Коли час скидання метрики гарантовано минув — відображення
  екстраполюється до `≈0%` (`ProviderStatus::Estimated`).
- Метрики з майбутнім скиданням зберігаються у `provider-cache.json` і
  переживають перезапуск віджета: після рестарту рядок показує останнє
  реальне значення (сіре, з давністю), а не текст помилки.

## Claude

### Підписка (за замовчуванням) — ланцюжок джерел у `fetch_via_subscription`

1. **Ручний usage token** — мітка `claude_usage_token` у Credential Manager
   (меню: *Paste usage token*; CLI: `ailimits-auth set-usage-token claude`).
2. **OAuth-токен Claude Code** — `%USERPROFILE%\.claude\.credentials.json`
   → `claudeAiOauth.accessToken` (пропускається, якщо `expiresAt` у минулому).
3. **statusline.jsonl** — знімки статус-бару Claude Code (толерантний пошук
   назв полів).
4. Джерело є, але даних не дало → `NetworkError("token expired")`.
5. Джерел немає взагалі → `NotConfigured` (рядок ховається).

Обидва токени звертаються до недокументованого usage-endpoint'а
(звірено 2026-06-10, HTTP 200):

```
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <token>
anthropic-beta: oauth-2025-04-20
```

Відповідь (null-вікна пропускаються):

```json
{"five_hour":  {"utilization": 83.0, "resets_at": "RFC3339"},
 "seven_day":  {"utilization": 6.0,  "resets_at": "RFC3339"},
 "seven_day_opus": null, "seven_day_sonnet": {...}}
```

→ метрики `Session`, `Weekly`, `Opus`, `Sonnet` (відсотки, ліміт 100).

**Rate limiting (перевірено 2026-07-09).** Per-token бакет endpoint'а
ділиться з опитуванням самого Claude Code: при активному сеансі проходить
приблизно один запит віджета з трьох, решта отримують HTTP 429 із
`Retry-After: 0`. Відбитий запит іде далі ланцюжком джерел (це не проблема
токена); на екрані лишаються останні дані. Підсумок: оновлення Claude
інколи займає 2–3 хвилини — серверна стеля.

### API ключ (опційно)

`auth_method = "api_key"`, мітка `claude_api_key`. Один запит до
`GET /v1/models` з `x-api-key`; ліміти — із заголовків відповіді
`anthropic-ratelimit-*` (запити + токени). Це моніторинг API rate limits,
а не підписки.

## OpenAI Codex (тільки підписка)

Джерела токена, по черзі:

1. **Ручний usage token** — мітка `codex_usage_token` (меню / CLI).
2. **Токен Codex CLI** — `%USERPROFILE%\.codex\auth.json`
   → `tokens.access_token`.

Endpoint (звірено 2026-06-10; сам Codex CLI його полить):

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

`primary_window` = 5-годинна сесія → `Session`; `secondary_window` =
тиждень → `Weekly`. `reset_at` — unix epoch секунди. 401/403 на токені CLI
→ `AuthError("token expired — run Codex CLI once")`.

## GitHub Copilot (тільки підписка)

Джерела токена, по черзі:

1. **PAT** — мітка `copilot_pat` (меню: *Paste PAT*; CLI: `set copilot`).
2. **gh CLI** — `gh auth token`, запускається з `CREATE_NO_WINDOW` і
   кешується в пам'яті на 15 хвилин (401 інвалідовує кеш).

Endpoint (звірено 2026-06-10; той самий, що в розширень Copilot):

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

→ метрики `Premium`, `Chat`, `Completions` (used = entitlement − remaining;
від'ємний `remaining` = перевитрата; `unlimited`-квоти пропускаються).
`quota_reset_date` → скидання опівночі UTC.

## Час життя токенів і вікно точності

CLI-токени — короткоживучі OAuth access tokens; CLI-власник оновлює їх при
використанні. Віджет ніколи не оновлює (ротація розлогінила б CLI —
підтверджено наживо: NousResearch/hermes-agent#22903, де refresh одного
клієнта анулював токен для решти). Наслідки:

| Провайдер | Час життя токена | Вікно живих даних без дій |
|---|---|---|
| Claude | ~8 год access token (звірено), оновлює Claude Code при використанні | ~8 год після останнього сеансу Claude Code, далі fallback/сірий |
| Codex | короткоживучий, оновлює Codex CLI при кожному запуску | лише поки Codex CLI активно використовується |
| Copilot | gh сам керує своїм токеном | нескінченно, поки залогінений у gh |
| Antigravity | keyring token, потім legacy token Gemini CLI | лише поки одне з цих джерел оновлюється, далі fallback/сірий |

Команди штибу `gh auth token` («вивести/оновити токен») у Codex чи Claude
CLI **немає** (звірено з офіційною auth-докою OpenAI Codex). Єдиний
CLI-керований refresh — це `gh auth token` у Copilot. Для облікових даних,
що не залежать від активності CLI, офіційна рекомендація (і ручний шлях
віджета) — API ключ / PAT; мітки див. у CONFIG.md.

## Antigravity (підписка Code Assist)

Джерела токена, по черзі:

1. **Antigravity CLI** — Windows Credential Manager target
   `gemini:antigravity` → JSON `token.access_token` (READ-ONLY; пропускається,
   коли `token.expiry` в минулому).
2. **Legacy Gemini CLI** — `~/.gemini/oauth_creds.json` → `access_token`
   (READ-ONLY; пропускається, коли `expiry_date` в минулому).

Станом на 18 червня 2026 року Google припинив обслуговувати запити Gemini CLI
і IDE-розширень Gemini Code Assist для Gemini Code Assist for individuals,
Google AI Pro і Google AI Ultra. Gemini CLI лишається підтриманим для Gemini
Code Assist Standard/Enterprise і платних Gemini / Gemini Enterprise Agent
Platform API-key flows. Для individual-користувачів шлях міграції —
Antigravity CLI; Antigravity зберігає session tokens у OS keyring (системному
сховищі ключів). На Windows поточний Antigravity CLI зберігає consumer-сеанс
під `gemini:antigravity`.

Квота — ланцюжок джерел (усі звірені наживо 2026-08-01):

1. `loadCodeAssist` (порожнє тіло) → `cloudaicompanionProject`, кешується на
   сесію. Повертається ЛИШЕ ідентифікованому клієнту (`User-Agent:
   antigravity/1.0` + `X-Goog-Api-Client: gl-go antigravity`); анонімний
   виклик так само відповідає 200, але без цього поля. Все нижче потребує
   цього id, а без нього віджет показує чесну помилку замість квоти —
   кожен endpoint без project id відповідає дефолтним виглядом
   «все повне», який неможливо відрізнити від невикористаного акаунту.
2. **`retrieveUserQuotaSummary`** — спільні пули, які Antigravity сьогодні
   метрить:

   ```
   POST https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary
   Body: {"project": "..."}
   → {"groups": [{"displayName": "Gemini Models",
        "buckets": [{"bucketId": "...", "displayName": "Weekly Limit",
                     "window": "...", "resetTime": "RFC3339",
                     "remainingFraction": 0}]},
       {"displayName": "Claude and GPT models", "buckets": [...]}]}
   ```

   Витрачений пул може віддати `remainingFraction: 0` або взагалі
   пропустити це поле, лишивши тільки `resetTime` — обидва варіанти
   означають нуль залишку. Одна група `All Models` — це дефолтний вигляд
   без project id, і вона відкидається. Ці пули — загальні ліміти
   Antigravity, тож фіксуються як long-window метрики.
3. Fallback: `fetchAvailableModels` (per-model `quotaInfo`; витрачена модель
   пропускає `remainingFraction` і лишає `resetTime`).

Якщо і `retrieveUserQuotaSummary`, і `fetchAvailableModels` не спрацювали або
не дали нічого придатного, віджет показує чесну помилку замість квоти. Далі
немає fallback на `retrieveUserQuota`: цей endpoint відповідає bucket-виглядом
Code Assist, який НЕ відстежує використання Antigravity — для акаунту, що
використовує лише Antigravity, ці buckets показують `remainingFraction: 1`
незалежно від реального використання. І `retrieveUserQuotaSummary`, і
`fetchAvailableModels` живуть на `daily-cloudcode-pa.googleapis.com`, який
нестабільний (див. нижче), тож один таймаут може забрати обидва джерела за
один цикл опитування — це має проявлятись як помилка, а не як тихо неправильні
«0% використано».

used % = `(1 - remainingFraction) * 100`; epoch-заглушка (`1970-01-01…`) у
`resetTime` парситься як «скидання невідоме». 401/403 → "Antigravity token
rejected". Парсери обходять JSON толерантно, тож зміна форми деградує до
чесної помилки. Хости (`cloudcode-pa` / `daily-` близнюк) нестабільні —
окремі запити регулярно таймаутяться; це покривають звичайні
retry/retention віджета.

## Видалені провайдери і методи

- **Gemini через API-ключ** — публічний Generative Language API не віддає
  rate-limit даних (лише бінарні 200/429), тож початковий key-провайдер
  Gemini було видалено. ПОТОЧНИЙ Antigravity-провайдер (секція вище) натомість
  читає OAuth-токени Antigravity / legacy Gemini CLI і питає quota-endpoint
  Code Assist, який повертає реальне використання по моделях.
- **Codex API-key probe** (платний запит chat/completions заради заголовків)
  і **Copilot Billing API** — видалені; підписочні endpoint'и дають строго
  більше даних безкоштовно. Відновлюються з git history.
