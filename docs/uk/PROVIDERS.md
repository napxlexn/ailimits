# PROVIDERS.md — як віджет працює з кожним провайдером

Базові правила для всіх провайдерів:

- **Токени read-only.** Віджет повторно використовує токени офіційних
  CLI-інструментів (для Cursor — самого редактора) і НІКОЛИ їх не оновлює та
  не ротує (ротація розлогінила б CLI). Прострочений токен — тихий fallback, а не діалог із помилкою.
- **Недокументовані endpoint'и вважаються ворожими.** Толерантні парсери;
  будь-яка незнайома схема чи статус ≠ 200 деградує чесно (NetworkError),
  а не вигадує цифри.
- **Секрети живуть лише у Windows Credential Manager**, service `ailimits`.
  Конфіг зберігає мітки, не значення. Токени ніколи не логуються.
- **Політика застарілих даних** (`app.rs` + `renderer.rs`): тимчасова
  помилка ніколи не затирає останні реальні дані — вони показуються сірими
  з давністю. Застарілими дані стають, коли пропустили наступне заплановане
  оновлення: інтервал оновлення плюс хвилина, але не менше 5 хвилин. Коли час скидання метрики гарантовано минув — відображення
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
3. Джерело є (будь-який із токенів: збережений usage token важить стільки ж,
   скільки файл credentials), але даних не дало →
   `NetworkError("token expired")`, або те, що насправді зламалося:
   `"rate-limited, retrying"` після HTTP 429, `"offline, retrying"`, коли
   запит не отримав відповіді, `"unrecognized usage schema"` для 200, якого
   парсер не знає.
4. Джерел немає взагалі → `NotConfigured` (рядок ховається).

(Знімки `statusline.jsonl` були джерелом до 0.7.0; Claude Code більше не
пише цей файл.)

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
→ `AuthError("token expired — run Codex CLI once")`. Без auth.json
збережений usage token, що не спрацював, тримає рядок (сірий, з останніми
даними): `AuthError("usage token rejected")` на 401/403, інакше мережева
помилка. Нечитабельний auth.json → `NetworkError("auth.json unreadable")`.
Джерел немає взагалі → `NotConfigured` (рядок ховається).

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
| Grok | ~6 год access token (звірено), оновлює Grok CLI при використанні | до ~6 год після останнього сеансу Grok CLI, далі fallback/сірий |
| Cursor | JWT на ~60 днів, оновлює редактор Cursor, поки він запущений | до `exp` токена (до ~60 днів після останнього оновлення редактором), далі сірий |

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

## Grok (підписка SuperGrok)

Джерело токена: **логін Grok CLI**, `auth.json` у домашній теці CLI —
`%GROK_HOME%\auth.json`, коли задано `GROK_HOME`, інакше
`%USERPROFILE%\.grok\auth.json` (правило самого CLI). READ-ONLY: CLI оновлює
свій токен щоразу, коли запускається; віджет цього не робить ніколи і не
запускає CLI, щоб це зробити.

Форма файлу (звірено 2026-11-10): об'єкт із ключами `"<issuer>::<client-id>"`,
кожне значення — запис сесії:

```json
{"https://auth.x.ai::...": {"key": "...", "refresh_token": "...",
  "expires_at": "RFC3339", "auth_mode": "oidc",
  "user_id": "...", "email": "...", "team_id": "...",
  "oidc_issuer": "https://auth.x.ai", "oidc_client_id": "..."}}
```

`key` — це access token (JWT, живе близько 6 годин). Сесії, у яких
`expires_at` минув, пропускаються; з решти береться та, що дійсна найдовше,
а сесія без указаного терміну йде останньою (її судить сервер). Далі:

1. Придатна сесія → один billing-запит (нижче).
2. Сесії є, але всі прострочені → `NetworkError("token expired")`; останні
   реальні дані лишаються на екрані, сірі, з давністю, а hover додає
   "run Grok CLI".
3. Немає файлу або сесії в ньому → `NotConfigured` (рядок ховається).
4. Файл не парситься → `NetworkError("auth.json unreadable")`; наступного
   циклу він читається знову.

Endpoint (звірено 2026-11-10, HTTP 200; власний billing-виклик CLI):

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

→ одна long-window метрика `Weekly` = `creditUsagePercent` (0–100),
скидання = `currentPeriod.end` (fallback `billingPeriodEnd`). Це той самий
"Weekly SuperGrok Limit", що показує сторінка Usage на grok.com. Тип періоду,
відмінний від тижневого, підписується `Daily` / `Monthly`, а невідомий —
`Limit`. Кожен запис `productUsage` іде слідом окремою long-window метрикою:
частка цього продукту в тому самому ліміті (`GrokBuild` → `Build`; інші назви,
як-от Chat, лишаються як є). Віджет поки не малює продукти; вони зберігаються
в даних для майбутнього вигляду по продуктах.

Тіло — protobuf JSON, який пропускає нульові значення: до будь-якого
використання той самий виклик відповідав без `creditUsagePercent` і без
`productUsage` взагалі. Тож 200 з коректним `currentPeriod` і без відсотка —
це 0%, і лише тоді; тіло без жодного з них — незнайома схема. Незнайома
схема, тіло не в JSON або будь-який статус ≠ 200 — чесний `NetworkError`:
`unrecognized billing schema`, `token rejected` (401/403) або `HTTP <code>`.

Не плутати з:

- простим `/v1/billing` (без `format`): він віддає місячні on-demand витрати
  API (`monthlyLimit`, `used`, `onDemandCap`), а не ліміт підписки;
- HTTP 429 з `subscription:free-usage-exhausted`, який отримує акаунт без
  SuperGrok: це відповідь chat API самого CLI, а не цього виклику. Що
  billing-виклик відповідає такому акаунту, не звірено; хай що це буде,
  статус ≠ 200 деградує чесно.

## Cursor (тільки підписка)

Джерело токена: власний сеанс редактора Cursor у його базі стану VS Code
`%APPDATA%\Cursor\User\globalStorage\state.vscdb` (SQLite, таблиця
`ItemTable(key, value)`, ключ `cursorAuth/accessToken`; READ-ONLY, ніколи не
оновлюється). Токен — JWT (issuer `https://authentication.cursor.sh`, audience
`https://cursor.com`, час життя ~60 днів), який редактор оновлює, поки
запущений. Віджет декодує лише його payload, заради `sub` і `exp`, і ніколи
не надсилає токен, чий `exp` минув. У тій самій таблиці також лежать
`cursorAuth/refreshToken`, `cursorAuth/cachedEmail`,
`cursorAuth/stripeMembershipType` і `cursorAuth/cachedSignUpType`; віджет не
читає жодного з них.

База належить редактору, який може тримати її відкритою і писати в неї
(вона в режимі WAL). Віджет читає її через SQLite, що постачається з Windows
(`System32\winsqlite3.dll`, вантажиться при першому використанні), відкриває
з `SQLITE_OPEN_READONLY` і busy timeout 200 мс. Як і будь-який читач SQLite,
він торкається лише індексу спільної пам'яті (`state.vscdb-shm`); сама база і
її `-wal` файл ніколи не записуються. Утримане блокування — тимчасова
помилка, яку повторює наступний цикл.

Endpoint (схему відповіді звірено 2026-11-10, HTTP 200, на безкоштовному
плані; той самий читає дашборд cursor.com):

```
GET https://cursor.com/api/usage-summary
Cookie: WorkosCursorSessionToken=<sub>%3A%3A<token>
Accept: application/json
```

`sub` має вигляд `github|user_…`: провайдер входу, далі id акаунта. Cookie
несе його цілим; `%3A%3A` — буквальне, вже URL-кодоване `::`. Звірено
2026-11-10 зі щойно виданим токеном того самого акаунта (разовий вхід за
посиланням; збережений токен редактора протух 2026-09-29). Запит із
власного збереженого токена редактора — той самий запит, але чекає живої
перевірки, коли в редакторі знову виконано вхід.

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

→ одна метрика на білінговий цикл: `individualUsage.plan.totalPercentUsed`
(0–100; `used / limit`, якщо відсотка немає, обидва в центах), скидання в
`billingCycleEnd`, підпис — `membershipType` (`free` → `Free`, `pro` → `Pro`).
`isUnlimited: true` → рядок `Unlimited` без відсотка і без скидання, ніколи не
смужка 0%. Вимкнений `individualUsage.plan`, зіпсований JSON чи будь-яка інша
форма → "unrecognized usage schema". Поки не показується: розподіл auto/API
(`autoPercentUsed` / `apiPercentUsed`; витрачений API-пул лишає Auto
робочим, тож він не може бути long window, що перехоплює головну метрику) і
витрати on-demand. Командні ліміти (`limitType: "team"`, `teamUsage`) не
звірені.

Стани: немає `state.vscdb` (Cursor не встановлено) або немає токена (вихід
з акаунта) → `NotConfigured`, рядок прихований; заблокована база → "state
database busy, retrying"; токен із минулим `exp` → "token expired", без
жодного запиту (відкрий Cursor, і він оновить токен); 401/403 → "token
rejected"; будь-який інший статус → "HTTP <code>". Кожна помилка лишає
останні дані сірими з давністю.

## Видалені провайдери і методи

- **Gemini через API-ключ** — публічний Generative Language API не віддає
  rate-limit даних (лише бінарні 200/429), тож початковий key-провайдер
  Gemini було видалено. ПОТОЧНИЙ Antigravity-провайдер (секція вище) натомість
  читає OAuth-токени Antigravity / legacy Gemini CLI і питає quota-endpoint
  Code Assist, який повертає реальне використання по моделях.
- **Codex API-key probe** (платний запит chat/completions заради заголовків)
  і **Copilot Billing API** — видалені; підписочні endpoint'и дають строго
  більше даних безкоштовно. Відновлюються з git history.
