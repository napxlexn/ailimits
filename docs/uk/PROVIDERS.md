# PROVIDERS.md — як віджет працює з кожним провайдером

Базові правила для всіх провайдерів:

- **Токени read-only.** Віджет повторно використовує токени офіційних
  CLI-інструментів і НІКОЛИ їх не оновлює та не ротує (ротація розлогінила б
  CLI). Прострочений токен — тихий fallback, а не діалог із помилкою. Єдиний
  сеанс, який віджет оновлює, — його власний сеанс Cursor, яким не
  користується жоден інший застосунок (див. Cursor).
- **Недокументовані endpoint'и вважаються ворожими.** Толерантні парсери;
  будь-яка незнайома схема чи статус ≠ 200 деградує чесно (NetworkError),
  а не вигадує цифри.
- **Секрети живуть лише у Windows Credential Manager**, service `ailimits`.
  Конфіг зберігає мітки, не значення. Токени ніколи не логуються.
- **Політика застарілих даних** (`app.rs` + `renderer.rs`): тимчасова
  помилка ніколи не затирає останні реальні дані — вони показуються сірими
  з давністю. Панель на таскбарі, де немає місця для давності, малює такий
  рядок (і оцінку `≈`) блідішим чорнилом. Застарілими дані стають, коли пропустили наступне заплановане
  оновлення: інтервал оновлення плюс хвилина, але не менше 5 хвилин.
  Опитування стає на паузу, поки користувач неактивний або екран заблоковано
  (5 хвилин без вводу), і цей час не рахується: рядок не сіріє через те, що
  застосунок перестав питати, поки користувача не було, а після повернення
  лишається живим, доки не прийде негайне оновлення після паузи. Провайдер,
  що падає, поки користувач активний, і далі сіріє, а дані, застарілі ще до
  паузи, лишаються застарілими й після неї. Показана давність — як і раніше
  звичайний настінний вік. Коли час скидання метрики гарантовано минув —
  відображення екстраполюється до `≈0%` (`ProviderStatus::Estimated`).
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

**Власний тиждень моделі (перевірено 2026-11-16, HTTP 200).** Та сама
відповідь містить `limits[]`; `/usage` у Claude Code показує запис моделі
як "Current week (Fable)":

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

Записи `session` і `weekly_all` повторюють `five_hour` і `seven_day`.
Кожен запис `weekly_scoped` з назвою моделі стає метрикою з назвою моделі
(`Fable`; `percent` — від власного ліміту цієї моделі), лише поруч із
вікнами вище і ніколи двічі під однією назвою. Вона позначена лише для
свого додаткового рядка (`claude.fable`, за замовчуванням вимкнений): вона
ніколи не стає головним числом Claude, його скиданням, його сповіщеннями
чи частиною його власного рядка, тож рядок Claude виглядає точно як
раніше, доки цей рядок не вибрано.

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
`quota_reset_date` → скидання опівночі UTC. Незнайома схема або тіло не в
JSON → `NetworkError("unrecognized copilot_internal schema")`; 401/403/404 →
`AuthError("token has no Copilot access")`; будь-який інший статус →
`HTTP <code>`; тіло, обірване посеред передачі, → мережева помилка.

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
| Cursor | власний сеанс віджета: JWT на ~60 днів, віджет оновлює його в останній тиждень (чи це його подовжує, ще не звірено) | доки сеанс не завершиться, далі сірий із "sign in to Cursor again" |

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
3. Токен є, але всі прострочені → `NetworkError("token expired")`: останні
   дані лишаються, сірі, з давністю, доки Antigravity не оновить токен.
   Облікові дані, які не вдається прочитати →
   `NetworkError("credentials unreadable")`. Токена немає ніде →
   `NotConfigured` (рядок ховається).

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
   сесію до першого 401/403 (увійшов інший акаунт Google: наступний цикл
   питає знову з новим токеном). Повертається ЛИШЕ ідентифікованому
   клієнту (`User-Agent: antigravity/1.0` +
   `X-Goog-Api-Client: gl-go antigravity`); анонімний
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
як-от Chat, лишаються як є). Build має власний рядок (`grok.build`), який
можна позначити в *Rows and models...*; інші продукти лишаються в даних.

Тіло — protobuf JSON, який пропускає нульові значення: до будь-якого
використання той самий виклик відповідав без `creditUsagePercent` і без
`productUsage` взагалі. Тож 200 з коректним `currentPeriod` і без відсотка —
це 0%, і лише тоді; тіло без жодного з них — незнайома схема. Незнайома
схема, тіло не в JSON або будь-який статус ≠ 200 — чесний `NetworkError`:
`unrecognized billing schema`, `token rejected` (401/403) або `HTTP <code>`;
тіло, обірване посеред передачі, — мережева помилка.

Не плутати з:

- простим `/v1/billing` (без `format`): він віддає місячні on-demand витрати
  API (`monthlyLimit`, `used`, `onDemandCap`), а не ліміт підписки;
- HTTP 429 з `subscription:free-usage-exhausted`, який отримує акаунт без
  SuperGrok: це відповідь chat API самого CLI, а не цього виклику. Що
  billing-виклик відповідає такому акаунту, не звірено; хай що це буде,
  статус ≠ 200 деградує чесно.

## Cursor (тільки підписка)

Джерела токена, по черзі:

1. **Власний сеанс віджета** — мітка Credential Manager `cursor_session`
   (меню: *Providers → Cursor → Sign in to Cursor...*; CLI:
   `ailimits-auth login cursor`). Віджет сам входить у Cursor і володіє цим
   сеансом, тож це єдиний токен Cursor, який віджет будь-коли оновлює.
2. **Токен редактора, лише поки він не зашифрований** — база стану VS Code
   редактора Cursor `%APPDATA%\Cursor\User\globalStorage\state.vscdb`
   (SQLite, таблиця `ItemTable(key, value)`, ключ `cursorAuth/accessToken`;
   READ-ONLY, ніколи не оновлюється, ніколи не надсилається після свого
   `exp`); пробується лише тоді, коли власного сеансу немає. Свіжі версії
   Cursor шифрують це значення на диску: `cursor-safe-storage:v1:` + base64
   від `v10` і AES-256-GCM, ключ у `%APPDATA%\Cursor\Local State`
   (`os_crypt.encrypted_key`, під DPAPI). Віджет ніколи його не
   розшифровує — розшифровувати збережені облікові дані чужого застосунку
   саме те, що роблять крадії паролів, і антивіруси це ловлять. Значення з
   префіксом `cursor-safe-storage:` вважається відсутнім токеном.
3. Ні того, ні іншого → `NotConfigured`: рядок прихований, поки
   користувач не увійде.

База редактора належить редактору, який може тримати її відкритою і писати
в неї (вона в режимі WAL). Віджет читає її через SQLite, що постачається з
Windows (`System32\winsqlite3.dll`, вантажиться при першому використанні),
відкриває з `SQLITE_OPEN_READONLY` і busy timeout 200 мс. Як і будь-який
читач SQLite, він торкається лише індексу спільної пам'яті
(`state.vscdb-shm`); сама база і її `-wal` файл ніколи не записуються.
Утримане блокування — тимчасова помилка, яку повторює наступний цикл.

### Вхід за посиланням (звірено наживо 2026-11-10)

Власний сценарій редактора, зв'язаний PKCE:

1. verifier = base64url без доповнення від 32 випадкових байтів;
   challenge = base64url без доповнення від SHA-256 ASCII-рядка verifier;
   uuid = випадковий UUID версії 4.
2. Браузер відкриває
   `https://cursor.com/loginDeepControl?challenge=<challenge>&uuid=<uuid>&mode=login`,
   і користувач тисне *Sign in*.
3. Віджет опитує `GET https://api2.cursor.sh/auth/poll?uuid=<uuid>&verifier=<verifier>`:
   404, поки вхід не підтверджено, далі 200
   `{"accessToken": "...", "refreshToken": "...", "authId": "...", "challenge": "...", "uuid": "..."}`.
   Обидва токени були тим самим JWT (`sub` на кшталт `github|user_…`, час
   життя близько 60 днів). Опитування чекає 1 с, далі щоразу в 1.2 раза
   довше, до 8 с, і здається через 5 хвилин. Verifier несе лише опитування,
   і воно ніколи не логується.

Сеанс зберігається як UTF-8 JSON: токен, його `exp`, остання спроба
оновлення і чи завершив його Cursor. У віджеті вхід чекає у фоновій задачі
і закінчується тостом: "Signed in to Cursor", "Cursor sign-in timed out" чи
"Cursor sign-in failed". Повторний вхід замінює сеанс (інший акаунт або
сеанс, який завершив Cursor); *Sign out of Cursor* чи
`ailimits-auth logout cursor` його забуває. Перезапуск не потрібен в
жодному разі: провайдер читає сеанс при кожному оновленні.

### Оновлення (лише власного сеансу)

```
POST https://api2.cursor.sh/oauth/token
Content-Type: application/json

{"grant_type": "refresh_token", "client_id": "KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB",
 "refresh_token": "<token>"}
→ 200 {"access_token": "...", "id_token": "...", "shouldLogout": false}
```

Звірено 2026-11-10 одразу після входу: новий токен відрізнявся, його `exp`
не зрушив, а старий токен і далі працював. Тож віджет оновлює сеанс лише в
останні 7 днів перед `exp`, не частіше разу на добу, і зберігає новий
`access_token` як обидва токени. `shouldLogout: true` завершує сеанс, так
само як і відхилене оновлення (401/403, поки не траплялося). Чи подовжує
оновлення біля `exp` сеанс, ще не звірено; якщо ні, сеанс живе близько 60
днів, а далі рядок просить увійти знову.

### Використання (звірено 2026-11-10, HTTP 200)

Три методи dashboard-сервісу, який викликає редактор (протокол Connect,
JSON), усі одним і тим самим запитом:

```
POST https://api2.cursor.sh/aiserver.v1.DashboardService/<Method>
Authorization: Bearer <token>
Content-Type: application/json
Connect-Protocol-Version: 1

{}
```

**`GetCurrentPeriodUsage`** — щоциклу, головна метрика:

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

→ одна метрика: `planUsage.totalPercentUsed` (0–100), скидання в
`billingCycleEnd`. Межі циклу — епохи в мілісекундах, записані рядками.
Тіло — protobuf JSON, який пропускає нульові значення, тож `planUsage` без
`totalPercentUsed` поряд із коректним циклом — це 0%, і лише тоді. Немає
об'єкта `planUsage`, тіло не JSON або будь-яка інша форма → "unrecognized
usage schema". Поки не показується: розподіл auto / названі моделі.

**`GetPlanInfo`** — не частіше разу на годину для кожного акаунта (через
десять хвилин після запиту, що нічого не приніс, а доти лишається останній
відомий план); дає підпис рядка:

```json
{"planInfo": {"planName": "Premium", "price": "",
  "billingCycleEnd": "1796857287995",
  "includedUsagePeriod": "INCLUDED_USAGE_PERIOD_WEEKLY",
  "includedUsageResetsAt": "1794870088407"}}
```

→ підпис — `planName` (`Premium`); без нього — період
(`includedUsagePeriod`: `Weekly`, `Monthly`, `Daily`), далі довжина циклу,
інакше `Included`.

**`GetSandUsageStatus`** — Grok Bot, щоциклу, додаткова метрика:

```json
{"currentPeriodStart": "2026-11-09T23:01:28.407Z",
 "nextResetTimestampUtc": "2026-11-16T23:01:28.407Z",
 "usagePercent": 1.191765980498375, "hasAvailableUsage": true,
 "grokPlanLabel": "Grok Bot Plan", "cursorPlanName": "Premium",
 "isUnified": true, "billingBrand": "SAND_BILLING_BRAND_GROK"}
```

→ друга метрика `Grok Bot` = `usagePercent`, скидання в
`nextResetTimestampUtc`. Відсутній відсоток — це 0% лише поряд із відомим
періодом і `hasAvailableUsage: true`; тіло, де пропущено все (акаунт без
Grok Bot), не дає метрики. Це метрика session-вікна, тож вона ніколи не
перехоплює головну; вона показується окремим рядком (`cursor.bot`), коли
його позначено в *Rows and models...*. На звіреному
акаунті Grok, Cursor і Grok Bot ділять ОДИН тижневий пул (те саме
скидання, той самий відсоток); як це показувати — відкрите рішення, тож
`isUnified` переноситься як `unified_pool` метрики, щоб пізніше ним
скористалося відображення. Будь-яка невдача цього виклику лише прибирає
метрику.

Без сеансу (звірено): HTTP 401
`{"code": "unauthenticated", ..., "debug": {"error": "ERROR_NOT_LOGGED_IN", ...}}`.

Стани (кожна помилка лишає останні дані сірими з давністю):

- власний сеанс відхилено (401/403): одне оновлення, якщо до `exp` лишилося
  не більше 7 днів, і ще одна спроба; інакше сеанс позначається
  завершеним → `AuthError("sign in to Cursor again")`;
- власний сеанс після `exp` або завершений через `shouldLogout` → те саме,
  без жодного запиту використання;
- токен редактора відхилено → "token rejected"; після його `exp` → "token
  expired", без жодного запиту; заблокована база → "state database busy,
  retrying";
- немає відповіді → "offline, retrying"; будь-який інший статус →
  "HTTP <code>".

Де виправлення — це вхід, підказка під курсором додає "sign in to Cursor".
Запитів за цикл: два (використання і Grok Bot), плюс план не частіше разу
на годину і оновлення не частіше разу на добу в останній тиждень сеансу.

## Підписи моделей

Модель, на яку налаштовано кожен інструмент (або яку він використав
останньою), і її effort; показуються у власному рядку провайдера, коли
ввімкнено позначки Model, Version чи Effort (`src/models`). Це лише локальні
читання: для них не робиться жодного запиту, і ніщо з цих файлів нікуди не
надсилається. З читача виходять тільки id моделі та effort; жодна інша
частина файлу не зберігається.

Про кожне читання спершу питають, і за замовчуванням воно вимкнене:

- `[reads] tool_settings`: файли налаштувань інструментів (таблиця нижче).
  Вимкнено: жоден файл налаштувань не відкривається.
- `[reads] last_session`: кінець найновішого файлу сесії, лише з
  `[ui.model] source = "last_used"`. Вимкнено: жоден файл сесії не
  відкривається.

### Файли налаштувань (`tool_settings`)

| Інструмент | Файл | Ключі |
|---|---|---|
| Claude Code | `%USERPROFILE%\.claude\settings.json` | `model`; effort `modelSettings.<model>.effortLevel`, інакше той, що в найновішої моделі родини псевдоніма, інакше `effortLevel` верхнього рівня |
| Codex CLI | `%CODEX_HOME%\config.toml`, інакше `%USERPROFILE%\.codex\config.toml` | `model`, `model_reasoning_effort` верхнього рівня |
| Grok CLI | `%GROK_HOME%\config.toml`, інакше `%USERPROFILE%\.grok\config.toml` | `[models] default`, `[models] default_reasoning_effort` |
| Copilot CLI | `%COPILOT_HOME%\settings.json`, інакше `%USERPROFILE%\.copilot\settings.json` | `model`, `effortLevel` |
| Antigravity CLI | `%USERPROFILE%\.gemini\antigravity-cli\settings.json` | `model` (підпис для показу: `Claude Sonnet 4.6 (Thinking)`; окремого effort немає) |
| Cursor (редактор) | `%APPDATA%\Cursor\User\globalStorage\state.vscdb` | один запис, `...persistentStorage.applicationUser`, і з нього лише `aiSettings.modelConfig.composer`: `modelName`, параметр `reasoning_effort` |

Форми звірено 2026-11-11 на ПК власника, лише назви ключів:

- Claude Code: поки використовується модель за замовчуванням, `model`
  верхнього рівня немає (типова модель ніде не записана, тож підпис — сам
  effort); effort зберігається для кожної моделі окремо в `modelSettings`
  під повним id моделі. Перевірено наживо 2026-11-13 з Claude Code
  2.1.323: `/model opus` записує псевдонім (`"model": "opus"`), а `/effort`
  зберігає під `modelSettings.claude-opus-5-5`, тож псевдонім бере effort
  найновішого повного id своєї родини (версія ключа може відставати від
  версії моделі, тому підпис тоді без версії: "Opus · high"). Без `model`
  — старий `effortLevel` верхнього рівня, інакше effort моделей, коли вони
  всі однакові (ПК власника 2026-11-13: дві моделі, обидві `xhigh`); коли
  вони різні, ніде не записано, який бере типова модель, тож не
  показується жоден. Опція контексту відкидається з id (`opus[1m]`
  читається як `opus`), тож одне перейменування покриває обидва.
- Codex CLI: `model` і `model_reasoning_effort` на верхньому рівні; модель,
  задана лише в таблиці `[profiles.*]`, не читається.
- Grok CLI: `[models] default_reasoning_effort` є; `[models] default` — з
  власної документації CLI (`user-guide/05-configuration.md`), там його не
  задано (вбудована типова модель CLI: підпис — сам effort).
- Copilot CLI: `model` (`"auto"`); `effortLevel` — з його документації, там
  не задано.
- Antigravity CLI: `model`, збережений як підпис, який показує CLI; він
  показується як записаний, без розбиття за дефісами (`GPT-OSS 120B
  (Medium)`).
- Cursor (звірено 2026-11-16 на ПК власника): редактор зберігає модель,
  вибрану для кожного місця, у записі
  `src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser`
  своєї бази стану, під `aiSettings.modelConfig`; модель Agent — це
  `composer`: `{"modelName": "grok-4.7", "maxMode": false,
  "selectedModels": [{"modelId": "grok-4.7", "parameters": [{"id":
  "reasoning_effort", "value": "high"}, {"id": "context", "value":
  "256k"}, ...]}]}`. `"default"` — модель, яку Cursor називає Auto
  (показується як "Auto"). База відкривається лише для читання через
  вбудований у Windows SQLite, як і для входу, а `json_extract` самого
  SQLite віддає лише цей один об'єкт: решта запису (близько 0,5 МБ
  налаштувань редактора) і всі інші записи, зокрема вхід, із бази не
  виходять. SQLite у Windows без функцій JSON віддає запис цілим (читається
  лише до 8 МБ), і об'єкт дістає вже програма. Читається знову лише тоді,
  коли змінилася база або її файл `-wal`; один запит тривав 1,3 мс. Cursor
  CLI зберігає свою модель у `%USERPROFILE%\.cursor\cli-config.json` (поруч
  з e-mail акаунта); цей файл не читається.

Файл з effort, але без моделі, дає сам effort («xhigh»), і назви для
перейменування тоді немає.

Для підписів ніколи не читаються: `%USERPROFILE%\.claude.json`,
`%USERPROFILE%\.copilot\config.json`, `command-history-state.json`, жоден
файл входу, жоден запис бази Cursor, крім наведеного вище, Windows
Credential Manager.

### Остання використана (`last_session`)

Лише для двох інструментів, що зберігають сесії; решта показують
налаштовану модель. Найновіший файл читається від кінця по 64 КБ, доки
не трапиться запис моделі, щонайбільше останній 1 МБ: Claude Code 2.1.323
одразу після першої відповіді сесії пише запис `prompt_snapshot` на
110-160 КБ, і в кожній новій сесії ця відповідь опинялася за межами
хвоста в 64 КБ (виміряно 2026-11-13 на ПК власника: найновіший запис
моделі був у межах 1 МБ від кінця в 97,9% станів, через які проходив файл
сесії).

- **Claude Code**: найновіший `%USERPROFILE%\.claude\projects\<project>\<session>.jsonl`,
  який шукають у 32 теках проєктів, де найнедавніше починалася сесія, лише
  на верхньому рівні: `<session>\subagents\...` містить стенограми
  субагентів, чию модель обирав не користувач, і ніколи не читається.
  Модель — останній запис з `"type": "assistant"`, який не `isSidechain`:
  `message.model` (модель `<synthetic>`, яку пише сам Claude Code,
  пропускається) і `effort` цього запису.
- **Codex CLI**: найновіший `rollout-*.jsonl` у двох найновіших денних теках
  `sessions\YYYY\MM\DD` (у `CODEX_HOME`, коли задано). Модель — останній
  запис `turn_context` (`payload.model`, `payload.effort`) або `event_msg`
  типу `thread_settings_applied` (`payload.thread_settings.model`,
  `.reasoning_effort`). Новіші версії Codex стискають завершені сесії в
  `.jsonl.zst` (на ПК власника — всі): стиснену найновішу сесію не прочитати
  без декодера, тож підпис повертається до налаштованої моделі. У
  нестиснених сесіях останній запис з моделлю був від 2 КБ до 850 КБ від
  кінця: у межах читання в 1 МБ.

Немає моделі у хвості: підпис лишає модель, знайдену останньою, у цьому
файлі чи в попередній сесії (щойно розпочата сесія ще не має відповіді),
інакше повертається до налаштованої моделі, коли `tool_settings` дозволяє,
інакше нічого не показує.

### Межі та збої

- Файли налаштувань понад 256 КБ не читаються; файл сесії читається по
  64 КБ і щонайбільше на 1 МБ від кінця (пам'ять: щонайбільше 1 МБ плюс
  крок); рядок, що перетинає межу кроку, складається цілим, рядок,
  обрізаний межею 1 МБ, відкидається.
- UTF-8 BOM і кінці рядків CRLF приймаються. Відсутній файл, зламаний JSON
  чи TOML, хибний тип, значення довше 128 байт чи з керівними символами:
  для цього інструмента підпису немає. Ніколи не стан помилки, ніколи не
  panic.
- Файл читається знову, лише коли змінився час його зміни чи довжина.
- Читається раз за цикл оновлення (щонайменше 60 с) з планувальника: без
  спостерігачів, потоків і таймерів. Пошук найновішої сесії — один перелік
  на кожну переглянуту теку, ніколи не обхід усіх сесій. Виміряно
  2026-11-11 на ПК власника (debug-збірка): Claude Code 872 файли сесій у
  122 теках (повний обхід 5,4 мс з теплим кешем, обмежений пошук 0,7 мс);
  Codex CLI 114 файлів у 71 теці (повний обхід 2,4 мс, обмежений пошук
  0,17 мс); цілий цикл з усіма дозволеними читаннями і незміненими файлами —
  близько 1 мс.

### Назви

Таблиці назв немає, тож нова модель не потребує оновлення: загальне правило
відкидає штампи дат і `latest`, відкидає власну назву провайдера, якщо за
нею не йде версія, з'єднує цифри версії (`4-5` → `4.5`), ставить велику
літеру, а абревіатуру з'єднує з версією дефісом (`GPT-6.1`). Модель — слова
до першого слова-версії (`Opus`, `GPT`), версія додає решту (`Opus 5.5`,
`GPT-6.1 Sol`). Звірено на 156 публічних id моделей
(`tests/fixtures/model-names.tsv`). Назва, яку користувач вводить у
`[ui.model.names]`, замінює автоматичну і ділиться так само.

Які моделі вікно вибору пропонує перейменувати (список, згорнутий, доки
його не розгорнути, а далі по заголовку на інструмент), — окремий сталий
перелік для кожного інструмента (`src/models/catalog.rs`) плюс модель, яку
інструмент використовує зараз: вона завжди перша й позначена "in use".
Назв він не задає: модель, якої в ньому немає, однаково називається за
правилом. Джерела, 2026-11-13: Claude Code — id, які несли його відповіді
в одному живому виклику на модель (`claude-fable-5-2`, `claude-opus-5-5`,
`claude-sonnet-5-5`, `claude-haiku-5-5`); Codex CLI — `codex debug models`
(8 не прихованих); Grok CLI — `~/.grok/models_cache.json` (4); Antigravity
CLI — підписи, які він пише у свій settings.json (5); Copilot CLI — 26, які
перелічує `copilot help config`; Cursor — лише Auto (списку редактора немає
у файлі, який тут читається, а `cursor-agent models` потребує входу в CLI).
Перелічуються лише інструменти, що працюють; модель, спільна для двох
інструментів, має одну назву, тож одне поле — під першим.

## Видалені провайдери і методи

- **Gemini через API-ключ** — публічний Generative Language API не віддає
  rate-limit даних (лише бінарні 200/429), тож початковий key-провайдер
  Gemini було видалено. ПОТОЧНИЙ Antigravity-провайдер (секція вище) натомість
  читає OAuth-токени Antigravity / legacy Gemini CLI і питає quota-endpoint
  Code Assist, який повертає реальне використання по моделях.
- **Codex API-key probe** (платний запит chat/completions заради заголовків)
  і **Copilot Billing API** — видалені; підписочні endpoint'и дають строго
  більше даних безкоштовно. Відновлюються з git history.
- **Зведення використання Cursor через cookie** —
  `GET https://cursor.com/api/usage-summary` із cookie
  `WorkosCursorSessionToken=<sub>%3A%3A<token>`, складеним із токена
  редактора (схему звірено 2026-11-10 на безкоштовному плані:
  `individualUsage.plan.totalPercentUsed`, `membershipType`,
  `isUnlimited`). Того ж дня замінене dashboard-сервісом із власним сеансом
  віджета, щойно редактор почав шифрувати свій токен.
