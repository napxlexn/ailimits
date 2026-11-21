# PROVIDERS.md — як віджет працює з кожним провайдером

Базові правила для всіх провайдерів:

- **Токени read-only.** Віджет повторно використовує токени офіційних
  CLI-інструментів і НІКОЛИ їх не оновлює та не ротує (ротація розлогінила б
  CLI). Прострочений токен тихо робить рядок сірим, а не відкриває діалог
  із помилкою. Єдиний сеанс, який віджет оновлює, — його власний сеанс
  Cursor, яким не
  користується жоден інший застосунок (див. Cursor).
- **Недокументовані endpoint'и вважаються ворожими.** Толерантні парсери;
  будь-яка незнайома схема чи статус ≠ 200 деградує чесно (NetworkError),
  а не вигадує цифри.
- **Секрети живуть лише у Windows Credential Manager**, service `ailimits`.
  Конфіг зберігає мітки, не значення. Токени ніколи не логуються.
- **Політика застарілих даних** (`src/app/` + `renderer.rs`): тимчасова
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
- **Жодного оновлення від старішого облікового запису.** Кожне оновлення
  несе serial екземпляра провайдера, який його зробив, і цикл подій
  відкидає оновлення, чий екземпляр тим часом замінили. Провайдер
  перебудовується лише тоді, коли могли змінитися його власні налаштування
  чи обліковий запис: вхід чи вихід у Cursor, вставлений чи видалений ключ
  або usage-токен, інше джерело, вибране в його Sign-in, увімкнення чи
  вимкнення провайдера (позначено перший його рядок чи знято останній),
  дана чи відкликана згода. Тоді його кешовані числа, остання помилка,
  тренд і позначки сповіщень очищаються, і рядок показує Loading до першої
  відповіді. Перебудований провайдер опитується одразу. Зміна одного
  провайдера не чіпає інших (і їхніх кешів: токена gh, плану Cursor,
  проєкту Antigravity).
- **Лише вибране джерело.** Вхід кожного провайдера береться з одного
  джерела, вибраного під його рядками в Rows and models (Sign-in) і
  збереженого як `auth_method` (див. CONFIG.md): власний вхід інструмента
  або вставлений вами ключ чи usage-токен. Коли воно не спрацьовує, жодне
  інше джерело його не підміняє, тож відмова показується як відмова, а не
  ховається за числами іншого акаунта.

## Запити за цикл

Один запит використання на провайдера за цикл і жодного, поки опитування
на паузі. Цикли йдуть не частіше за інтервал оновлення (щонайменше 60 с),
з трьома винятками в часі, але ніколи в кількості: цикл одразу після кінця
паузи; один повтор через 20 с, коли перший цикл після пробудження (зі сну
чи з паузи простою) не отримав жодної відповіді; і щойно перебудований
провайдер, який опитується одразу. Цикл, у якому не відповів жоден
провайдер, чекає довше: 120, 240, далі 300 с при інтервалі 60 с
(`monitor/scheduler.rs`). Кілька допоміжних запитів, точно:

- **Cursor**: план (`GetPlanInfo`) не частіше разу на годину для кожного
  акаунта (через десять хвилин після запиту, що нічого не приніс); Grok Bot
  (`GetSandUsageStatus`) лише поки його рядок `cursor.bot` вибрано у
  віджеті чи підказці; оновлення сеансу (`api2.cursor.sh/oauth/token`) лише
  в останні 7 днів перед закінченням сеансу, ніколи після нього, не частіше
  разу на добу, разом із відмовами й спробами без відповіді. Перебудований
  Cursor (позначено чи
  знято Grok Bot, новий вхід) починає без плану й питає його знову.
- **Antigravity**: `loadCodeAssist`, доки відповідь не назве проєкт, який
  потім тримається весь сеанс (повторно — лише після відмови токена). Після
  відсутньої відповіді, статусу, відмінного від 200, чи 200 без проєкту
  нічого не зберігається: цикл на цьому закінчується, а наступний питає
  знову. `fetchAvailableModels` лише тоді, коли `retrieveUserQuotaSummary`
  не дав нічого придатного (200 з невідомим чи порожнім тілом або статус,
  відмінний від 200, 401, 403, 429 і 5xx: 3xx, за яким ніколи не йдуть,
  інший 2xx чи 4xx); після цього він іде першим одну годину, тож цикл
  лишається одним запитом. Ніколи після 429, 5xx чи відсутньої відповіді.
- **Claude і Codex**: питають лише вибране джерело, власний токен
  інструмента або вставлений вами usage-токен, ніколи одне за одним.
  Збережений usage-токен, який сервер відхилив (401/403), не надсилається
  30 хвилин (щойно вставлений пробується одразу); тим часом статус —
  "usage token rejected". 429, 5xx, невідоме тіло чи відсутність відповіді
  завершують цикл без другого запиту.
- **Copilot**: коли вибрано вхід GitHub CLI,
  `gh auth token --hostname github.com` не частіше разу на 15 хвилин,
  разом із невдачами, плюс один запуск одразу, коли сервер
  відхиляє (401/403) токен gh, збережений з попереднього циклу: gh 2.103.0
  і новіші дають токени з терміном дії, які `gh auth token` сам оновлює.
  Тоді запит надсилається ще раз, із новим токеном (але не тоді, коли gh
  знову дає той самий). Токен, який gh дав у цьому ж циклі, повторно не
  питають. Токен gh, який сервер відхилив (401, 403, 404), зокрема й
  новий, не надсилається повторно 15 хвилин.

## Мережа

- Кожен запит іде по HTTPS до хостів, названих нижче для кожного
  провайдера, з тайм-аутом 10 с і без редиректів, тож токен ніколи не
  пересилається на інший хост. Тіло відповіді читається щонайбільше до
  1 MiB (`MAX_ANSWER_BYTES`): Content-Length понад це відхиляється ще до
  читання, а тіло без нього покидається, щойно перейде межу.
- **Проксі.** Програма використовує проксі, заданий у Windows (Settings >
  Network > Proxy > «Use a proxy server»: `ProxyEnable`, `ProxyServer`,
  `ProxyOverride` в Internet Settings користувача), для провайдерів, входу в
  Cursor і оновлювача. `HTTPS_PROXY`, `HTTP_PROXY` і `ALL_PROXY` мають
  пріоритет. Скрипт проксі (PAC) і автовизначення не підтримуються.
  Налаштування читається один раз; зміна діє після перезапуску.
  `ailimits-auth` іде лише за змінними.
- **Сертифікати.** TLS використовує корені, вбудовані в програму (rustls), а
  не сховище сертифікатів Windows, тож мережа, що перевіряє TLS власним
  коренем, довіри не має: рядки пишуть "untrusted certificate, retrying".
  Довіра до сховища Windows додала б нові DLL до exe.

## Стани помилок

Кожен статус має щонайбільше 32 символи, тож віджет показує його як є;
статус, що закінчується на ", retrying", минає сам, і підказка під
курсором не додає дії. Кожна помилка лишає останні реальні дані сірими з
давністю.

| Причина | Рядок пише |
|---|---|
| немає відповіді: офлайн, DNS, обірване тіло | `offline, retrying` |
| немає відповіді за 10 с | `timed out, retrying` |
| сертифікат відхилено | `untrusted certificate, retrying` |
| відповідь понад 1 MiB | `answer too large, retrying` |
| HTTP 429 | `rate-limited, retrying` |
| HTTP 5xx (зокрема 529 Anthropic) | `HTTP <code>, retrying` |
| будь-який інший статус | `HTTP <code>` |

Власні стани провайдера наведено в кожній секції. `AuthError` (лише ключ
чи токен, який дали ви: API-ключ Claude, токен Copilot, usage-токен Claude
чи Codex) показує "check the key", а підказка під курсором пише
"<статус> — paste a new one in Rows and models"; рядок без вашого ключа
ніколи цього не пише.

## Claude

### Джерела токена — вибране (`fetch`)

Sign-in у Claude у Rows and models пропонує три джерела; `auth_method`
зберігає вибране, і використовується лише воно:

1. **Вхід Claude Code** (`"subscription"`, за замовчуванням) —
   `%USERPROFILE%\.claude\.credentials.json` → `claudeAiOauth.accessToken`
   (пропускається, якщо `expiresAt` у минулому), лише коли це дозволяє
   `[reads] sign_in`.
2. **Вставлений вами usage-токен** (`"usage_token"`) — мітка
   `claude_usage_token` у Credential Manager (Rows and models: Claude,
   Sign-in, *Paste token from clipboard*; CLI:
   `ailimits-auth set-usage-token claude`). Відхилений (401/403) →
   `AuthError("usage token rejected")`; потім він не надсилається 30
   хвилин. Вхід Claude Code замість нього не читається.
3. **Вставлений вами API-ключ** (`"api_key"`) — див. API ключ нижче.

З джерелом 1 чи 2 (`fetch_via_subscription`):

- Джерело не дало даних → `NetworkError("token expired")`, коли токен
  Claude Code відхилено (401/403), він прострочений або `.credentials.json`
  не вдається прочитати чи розібрати; інакше те, що насправді зламалося:
  спільні слова (див. Стани помилок: офлайн, тайм-аут, сертифікат,
  `rate-limited, retrying` після 429, `HTTP 529, retrying` та інші 5xx) чи
  `"unrecognized usage schema"` для 200, якого парсер не знає. Підказка під
  курсором додає "run Claude Code" до статусу, що не закінчується на
  ", retrying".
- Вибраного джерела немає (немає `.credentials.json` чи дозволу його
  читати; не збережено usage-токена) → `NotConfigured` (рядок ховається).

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
`Retry-After: 0`. Відбитий запит одразу завершує цикл на
`rate-limited, retrying` (це не проблема токена; інше джерело не
пробується); на екрані лишаються останні дані, а наступний цикл питає знову
через інтервал оновлення. Підсумок: оновлення Claude інколи займає 2–3
хвилини — серверна стеля.

### API ключ (опційно)

`auth_method = "api_key"`, мітка `claude_api_key` (Rows and models: Claude,
Sign-in, *An API key you paste*, *Paste key from clipboard*; CLI:
`ailimits-auth set claude`). Один запит за цикл:

```
GET https://api.anthropic.com/v1/models
x-api-key: <key>
anthropic-version: 2023-06-01
```

Ліміти — із заголовків відповіді
`anthropic-ratelimit-requests-{limit,remaining,reset}` і
`anthropic-ratelimit-tokens-{limit,remaining,reset}` → метрики `Requests`
і `Tokens` (сесійні вікна), кожна лише тоді, коли є її заголовки; 429 теж
їх несе й читається. 401 → `AuthError("invalid API key")` (рядок пише
"check the key"; підказка — "invalid API key — paste a new one in Rows and
models"); 200 без заголовків →
`NetworkError("no ratelimit headers in response")`; будь-який інший статус
без них чи відсутність відповіді → спільні слова. Вибрано API-ключ, але
жодного не збережено → `NotConfigured`: рядок ховається, а Sign-in пише
"Your API key · none stored". Це моніторинг API rate limits, а не підписки.

## OpenAI Codex (тільки підписка)

Джерела токена: Sign-in у Codex у Rows and models пропонує два;
`auth_method` зберігає вибране, і використовується лише воно:

1. **Вхід Codex CLI** (`"subscription"`, за замовчуванням) — `auth.json` у
   домашній теці CLI: `%CODEX_HOME%\auth.json`, коли `CODEX_HOME` задано
   абсолютним шляхом, інакше `%USERPROFILE%\.codex\auth.json` (відносний
   `CODEX_HOME`: жоден файл не читається взагалі) → `tokens.access_token`;
   лише коли це дозволяє `[reads] sign_in`. Codex CLI пише цей файл, лише
   коли тримає вхід у файлі: з `cli_auth_credentials_store = "keyring"` чи
   `"auto"` у його `config.toml` `auth.json` немає, і вхід прочитати
   неможливо. Постав `"file"` або встав usage-токен.
2. **Вставлений вами usage-токен** (`"usage_token"`) — мітка
   `codex_usage_token` (Rows and models: Codex, Sign-in, *Paste token from
   clipboard*; CLI: `ailimits-auth set-usage-token codex`). Відхилений
   (401/403) → `AuthError("usage token rejected")`, рядок лишається (сірий,
   з останніми даними); потім він не надсилається 30 хвилин. `auth.json`
   Codex CLI замість нього не читається.

Endpoint (звірено 2026-06-10; сам Codex CLI його полить):

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

**Звірено живим запитом 2026-11-21** (акаунт Plus): кожне з
`rate_limit.primary_window` і `rate_limit.secondary_window` має
`used_percent`, `reset_at`, `reset_after_seconds` і
`limit_window_seconds`; `additional_rate_limits` був `null`; поруч із ними
відповідь має `credits`, `model_usage`, `plan_type`,
`rate_limit_reset_credits` і `spend_control`. Із двох вікон читаються лише
`used_percent` і `reset_at`.

`primary_window` = 5-годинна сесія → `Session`; `secondary_window` =
тиждень → `Weekly`. `reset_at` — unix epoch секунди. 401/403 на токені CLI
→ `NetworkError("token expired")` (підказка додає "run Codex CLI"; не
"check the key": ключа ніхто не давав). Тіло, що не є відомим JSON, →
`"unrecognized wham/usage schema"`; інші статуси й відсутність відповіді →
спільні слова (див. Стани помилок). Нечитабельний auth.json →
`NetworkError("auth.json unreadable")`. Вибраного джерела немає (немає
auth.json чи дозволу його читати; не збережено usage-токена) →
`NotConfigured` (рядок ховається).

## GitHub Copilot (тільки підписка)

Джерела токена: Sign-in у Copilot у Rows and models пропонує два;
`auth_method` зберігає вибране, і використовується лише воно (gh ніколи не
запускається для вставленого токена, а збережений токен ніколи не
надсилається, поки вибрано вхід GitHub CLI):

1. **Вхід GitHub CLI** (`"subscription"`, за замовчуванням) —
   `gh auth token --hostname github.com` (ніколи не інший
   хост, тож вхід GitHub Enterprise чи `GH_HOST` не потрапляє на
   github.com), запускається з `CREATE_NO_WINDOW` (його знаходить власний
   пошук Rust: тека програми, System32, тека Windows, далі PATH, ніколи не
   робоча тека; System32 — лише робоча тека, у якій він працює) і
   зупиняється через 15 с; лише коли це дозволяє `[reads] sign_in`.
   Відповідь gh діє 15 хвилин, разом із невдачами: gh не встановлено чи
   ніхто не ввійшов на github.com → рядок прихований (`NotConfigured`), і gh
   питають знову через 15 хвилин; gh не відповів вчасно →
   `"gh timed out, retrying"`. Один виняток: коли сервер відхиляє (401/403)
   токен, збережений з попереднього циклу, gh питають ще раз одразу (gh
   2.103.0 і новіші дають токени з терміном дії й оновлюють їх у
   `gh auth token`), і запит надсилається ще раз із новим токеном; якщо gh
   знову дає той самий токен або жодного, більше нічого не надсилається.
   Токен, який сервер відхилив, зокрема й новий, не надсилається 15
   хвилин.
2. **Вставлений вами fine-grained токен** (`"api_key"`) — мітка
   `copilot_pat` (Rows and models: Copilot, Sign-in, *Paste token from
   clipboard*; CLI: `ailimits-auth set copilot`). За документацією GitHub
   класичні PAT (`ghp_...`) не підтримуються для Copilot CLI; fine-grained
   PAT (`github_pat_...`) потребує дозволу *Copilot Requests*. Чи
   відповідає `copilot_internal/user` на fine-grained PAT, живим запитом ще
   не звірено. Не збережено → `NotConfigured` (рядок ховається).

Endpoint (звірено 2026-06-10 і 2026-11-21; той самий, що в розширень
Copilot):

```
GET https://api.github.com/copilot_internal/user
Authorization: token <token>
User-Agent: ailimits-widget
```

Звірено 2026-06-10 (безкоштовний план):

```json
{"copilot_plan": "individual", "quota_reset_date": "2026-07-01",
 "quota_snapshots": {
   "premium_interactions": {"entitlement": 50,  "remaining": 10, "unlimited": false},
   "chat":        {"entitlement": 200,  "remaining": -1},
   "completions": {"entitlement": 2000, "remaining": 2000}}}
```

Звірено 2026-11-21 (план individual з token-based billing; кожна квота має
ті самі поля):

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

У червні 2026 року GitHub перевів Copilot на оплату за використання в AI
Credits (`token_based_billing`). Сторонні джерела пишуть, що платні плани з
кредитами можуть віддавати дробові entitlement; живим запитом це не
звірено.

→ метрики `Premium`, `Chat`, `Completions`, саме в такому порядку (used =
entitlement − remaining; `quota_remaining`, дробове, має перевагу над
`remaining`; дробові числа округлюються до цілих запитів; від'ємний
`remaining` = перевитрата). Квоти з `unlimited`, з `has_quota: false` або
без entitlement пропускаються; коли пропущено всі (кожна квота безлімітна
чи відсутня), рядок пише `NetworkError("no limited quota")`: схема
відома, просто нема чого рахувати, і підказка під курсором не додає
поради. `quota_reset_date_utc`, інакше
`quota_reset_date` опівночі UTC → скидання. Незнайома схема або тіло не в
JSON → `NetworkError("unrecognized Copilot schema")`. Відмова читається за
тим, звідки токен. Токен gh: 404 → `"no Copilot on this account"`, 401 →
`"gh token rejected"`, 403 → `"no Copilot access"` (рядок показує слова;
підказка додає "check gh auth"). Вставлений PAT: 401/403/404 →
`AuthError("token has no Copilot access")`, що показується як "check the
key" (підказка: "token has no Copilot access — paste a new one in Rows and
models"). Будь-який інший статус, відсутність відповіді чи тіло, обірване
посеред передачі, → спільні слова (див. Стани помилок).

## Час життя токенів і вікно точності

CLI-токени — короткоживучі OAuth access tokens; CLI-власник оновлює їх при
використанні. Віджет ніколи не оновлює (ротація розлогінила б CLI —
підтверджено наживо: NousResearch/hermes-agent#22903, де refresh одного
клієнта анулював токен для решти). Наслідки:

| Провайдер | Час життя токена | Вікно живих даних без дій |
|---|---|---|
| Claude | ~8 год access token (звірено), оновлює Claude Code при використанні | ~8 год після останнього сеансу Claude Code, далі сірий |
| Codex | ще не виміряно, оновлює Codex CLI, коли запускається | поки Codex CLI використовується; скільки після цього, не звірено |
| Copilot | gh сам керує своїм токеном; від gh 2.103.0 (2026-10-21) це OAuth-токен із терміном дії, який `gh auth token` сам оновлює | нескінченно, поки залогінений у gh |
| Antigravity | keyring token Antigravity CLI, близько години, оновлюється лише тоді, коли запускається `agy` | близько години після останнього запуску `agy`, далі сірий із "token expired"; іншого джерела немає |
| Grok | ~6 год access token (звірено), оновлює Grok CLI при використанні | до ~6 год після останнього сеансу Grok CLI, далі сірий; іншого джерела немає |
| Cursor | власний сеанс віджета: JWT на ~60 днів, віджет оновлює його в останній тиждень (чи це його подовжує, ще не звірено) | доки сеанс не завершиться, далі сірий із "sign in to Cursor again" |

Команди штибу `gh auth token` («вивести/оновити токен») у Codex чи Claude
CLI **немає** (звірено з офіційною auth-докою OpenAI Codex). Єдиний
CLI-керований refresh — це `gh auth token` у Copilot: від gh 2.103.0
(2026-10-21) gh входить OAuth-токенами з терміном дії й сам оновлює токен у
`gh auth token`, зберігаючи його у власному сховищі, тож gh лишається
залогіненим. Тому токен, збережений віджетом, може протухнути протягом
його 15 хвилин; відмова в ньому ще раз питає gh (див. Запити за цикл).
Для облікових даних,
що не залежать від активності CLI, офіційна рекомендація (і ручний шлях
віджета) — API ключ / PAT; мітки див. у CONFIG.md.

## Antigravity (підписка Code Assist)

Джерело токена, єдине:

1. **Antigravity CLI** — Windows Credential Manager target
   `gemini:antigravity` → JSON `token.access_token` (READ-ONLY; пропускається,
   коли `token.expiry` в минулому).
2. Токен є, але прострочений → `NetworkError("token expired")`: останні
   дані лишаються, сірі, з давністю, доки `agy` не оновить токен (підказка
   додає "run agy"); ніщо інше його не оновлює, і жодне інше джерело його
   не підміняє. Так задумано (d6abbde): прострочений
   токен робить рядок сірим, а не ховає його. Облікові дані, які не вдається
   прочитати → `NetworkError("credentials unreadable")`. Токена немає →
   `NotConfigured` (рядок ховається).

Запис читається лише тоді, коли це дозволяє `[reads] sign_in`.

Станом на 18 червня 2026 року Google припинив обслуговувати запити Gemini CLI
і IDE-розширень Gemini Code Assist для Gemini Code Assist for individuals,
Google AI Pro і Google AI Ultra. Gemini CLI лишається підтриманим для Gemini
Code Assist Standard/Enterprise і платних Gemini / Gemini Enterprise Agent
Platform API-key flows. Для individual-користувачів шлях міграції —
Antigravity CLI; Antigravity зберігає session tokens у OS keyring (системному
сховищі ключів). На Windows поточний Antigravity CLI зберігає consumer-сеанс
під `gemini:antigravity`. Тому застарілий файл Gemini CLI
`%USERPROFILE%\.gemini\oauth_creds.json`, запасне джерело до 0.7.1, більше
не читається. Тека `.gemini` лишається слабкою ознакою того, що Antigravity
може бути встановлено (Antigravity CLI живе в `.gemini\antigravity-cli`);
перевіряється лише її наявність.

**Ризик для акаунта.** Google заблокував деякі акаунти за використання
входу Antigravity поза Antigravity. Кожне місце, що питає про це читання чи
перелічує його, каже про це до дозволу: власне запитання Antigravity,
вітання, *Allow all* і повідомлення після оновлення
(`consent::ANTIGRAVITY_RISK`): "Google has suspended some accounts for using
Antigravity's sign-in outside Antigravity. AI Limits only reads your quota,
once per update, but the risk is yours."

Квота — ланцюжок джерел (усі звірені наживо 2026-08-01):

1. `loadCodeAssist` (порожнє тіло) → `cloudaicompanionProject`, тримається
   весь сеанс, щойно відповідь його назвала, до першого 401/403 (увійшов
   інший акаунт Google: наступний цикл питає знову з новим токеном). Після
   відсутньої відповіді, статусу, відмінного від 200, чи 200 без цього поля
   нічого не зберігається, і наступний цикл питає знову. Повертається ЛИШЕ
   ідентифікованому
   клієнту (`User-Agent: antigravity/1.0` +
   `X-Goog-Api-Client: gl-go antigravity`); анонімний
   виклик так само відповідає 200, але без цього поля. Все нижче потребує
   цього id, а без нього віджет показує чесну помилку замість квоти —
   кожен endpoint без project id відповідає дефолтним виглядом
   «все повне», який неможливо відрізнити від невикористаного акаунту. 200
   без цього поля → `"no Code Assist project"` (підказка додає "run agy":
   перший запуск налаштовує акаунт); відсутність відповіді чи
   будь-який інший статус тут (крім 401/403) — не відсутній проєкт, і
   показується спільними словами (див. Стани помилок).
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
   **Звірено живим запитом 2026-11-21:** кожен bucket називає своє вікно:
   `{"groups": [{"displayName": "...", "description": "...", "buckets":
   [{"bucketId": "...", "displayName": "...", "description": "...",
   "remainingFraction": 0.937382, "resetTime": "2026-11-28T00:07:14Z",
   "window": "weekly"}]}]}`, у того акаунта одна група з одним тижневим
   bucket. Власна документація Google про `/usage` описує ще й
   п'ятигодинний ліміт на групу; такого не прийшло. Bucket, чиє `window`
   згадує години чи сесію, читається як сесійне вікно з підписом
   "<пул> 5h"; будь-який інший або без поля — тижневий пул.
   **Вхід, який тут читається, — Antigravity CLI** (`agy`). Його токен
   живе близько години й оновлюється лише тоді, коли запускається `agy`;
   редактор Antigravity має власний вхід (зашифрований, ніколи не
   читається), тож відкриття редактора його не оновлює. Доки `agy` не
   запуститься знову, рядок пише "token expired", а підказка — "run agy".
3. Fallback: `fetchAvailableModels` (per-model `quotaInfo`; витрачена модель
   пропускає `remainingFraction` і лишає `resetTime`), на тому самому хості.
   Питають лише тоді, коли зведення не дало нічого придатного: 200 з
   невідомим чи порожнім тілом або статус, відмінний від 200, 401, 403, 429
   і 5xx (3xx, за яким ніколи не йдуть, інший 2xx чи 4xx). Коли він урятував
   цикл, то йде першим одну годину (зведення тоді стає його fallback), тож
   цикл лишається одним запитом. 429, 5xx чи відсутність відповіді
   завершують цикл спільними словами без іншого endpoint'а: вони нічого не
   кажуть про тіло.

Якщо і `retrieveUserQuotaSummary`, і `fetchAvailableModels` не дали нічого
придатного, віджет показує `"Antigravity quota unavailable"` замість квоти.
Далі немає fallback на `retrieveUserQuota`: цей endpoint відповідає
bucket-виглядом Code Assist, який НЕ відстежує використання Antigravity —
для акаунту, що використовує лише Antigravity, ці buckets показують
`remainingFraction: 1` незалежно від реального використання.

used % = `(1 - remainingFraction) * 100`; epoch-заглушка (`1970-01-01…`) у
`resetTime` парситься як «скидання невідоме». 401/403 на будь-якому з
викликів → `"token rejected"` (підказка додає "run agy"), і
кешований project id забувається, тож після переходу на інший акаунт Google
наступний цикл знову питає `loadCodeAssist`. Назва пулу, якої програма не
знає, показується так, як її надіслав сервер, очищена: без керівних
символів, щонайбільше 32 символи, `Pool`, коли нічого не лишилося. Парсери
обходять JSON толерантно, тож зміна форми деградує до чесної помилки. Хости (`cloudcode-pa` / `daily-` близнюк) нестабільні —
окремі запити регулярно таймаутяться; це покривають звичайні
retry/retention віджета.

## Grok (підписка SuperGrok)

Джерело токена: **логін Grok CLI**, `auth.json` у домашній теці CLI —
`%GROK_HOME%\auth.json`, коли `GROK_HOME` задано абсолютним шляхом,
інакше `%USERPROFILE%\.grok\auth.json` (правило самого CLI; відносний
`GROK_HOME`: жоден файл не читається взагалі). Читається лише тоді, коли
це дозволяє `[reads] sign_in`. READ-ONLY: CLI оновлює свій токен щоразу,
коли запускається; віджет цього не робить ніколи і не запускає CLI, щоб це
зробити.

Форма файлу (звірено 2026-11-10): об'єкт із ключами `"<issuer>::<client-id>"`,
кожне значення — запис сесії:

```json
{"https://auth.x.ai::...": {"key": "...", "refresh_token": "...",
  "expires_at": "RFC3339", "auth_mode": "oidc",
  "user_id": "...", "email": "...", "team_id": "...",
  "oidc_issuer": "https://auth.x.ai", "oidc_client_id": "..."}}
```

`key` — це access token (JWT, живе близько 6 годин). Рахуються лише сесії
xAI: сесія, чий `oidc_issuer` (а без нього — видавець у її ключі) не є
https-адресою на `x.ai` чи `grok.com`, пропускається, тож токен іншого
видавця ніколи не йде до xAI. Сесії, у яких `expires_at` минув,
пропускаються; з решти береться та, що дійсна найдовше, а сесія без
указаного терміну йде останньою (її судить сервер). Далі:

1. Придатна сесія → один billing-запит (нижче).
2. Сесії є, але всі прострочені → `NetworkError("token expired")`; останні
   реальні дані лишаються на екрані, сірі, з давністю, а hover додає
   "run Grok CLI".
3. Немає файлу або сесії в ньому → `NotConfigured` (рядок ховається).
4. Файл не вдається прочитати (заблокований, доступ заборонено, будь-яка
   помилка читання, крім відсутнього файлу) або він не парситься →
   `NetworkError("auth.json unreadable")`; наступного циклу він читається
   знову.

Прострочений чи нечитабельний — іншого джерела, на яке можна перейти,
немає.

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
як-от Chat, лишаються як є, очищені: без керівних символів, щонайбільше 32
символи). Ці частки позначені лише для свого додаткового рядка: ніколи не
головне число рядка Grok, не його скидання й не його сповіщення. Build має
власний рядок (`grok.build`), який можна позначити в *Rows and models...*;
інші продукти лишаються в даних.

Тіло — protobuf JSON, який пропускає нульові значення: до будь-якого
використання той самий виклик відповідав без `creditUsagePercent` і без
`productUsage` взагалі. Тож 200 з коректним `currentPeriod` і без відсотка —
це 0%, і лише тоді; тіло без жодного з них — незнайома схема.
`creditUsagePercent`, який є, але не є ні числом, ні рядком із числом, —
теж незнайома схема, ніколи не 0%. Незнайома
схема, тіло не в JSON або будь-який статус ≠ 200 — чесний `NetworkError`:
`unrecognized billing schema`, `token rejected` (401/403; підказка додає
"run Grok CLI") або спільні слова (див. Стани помилок).

Не плутати з:

- простим `/v1/billing` (без `format`): він віддає місячні on-demand витрати
  API (`monthlyLimit`, `used`, `onDemandCap`), а не ліміт підписки;
- HTTP 429 з `subscription:free-usage-exhausted`, який отримує акаунт без
  SuperGrok: це відповідь chat API самого CLI, а не цього виклику. Що
  billing-виклик відповідає такому акаунту, не звірено; хай що це буде,
  статус ≠ 200 деградує чесно.

## Cursor (тільки підписка)

Джерело токена: **лише власний сеанс віджета** — мітка Credential Manager
`cursor_session` (Rows and models: Cursor, Sign-in, *Sign in to
Cursor...*; CLI: `ailimits-auth login cursor`). Віджет сам входить у
Cursor на власній сторінці входу Cursor і володіє цим сеансом, тож це
єдиний токен Cursor,
який віджет будь-коли оновлює. Нічого з редактора Cursor для цього не
читається: ні його база стану, ні його токен, а власний вхід редактора
ніколи не зачіпається. Без сеансу → `NotConfigured`: рядок прихований, поки
користувач не ввійде.

Базу стану редактора (`%APPDATA%\Cursor\User\globalStorage\state.vscdb`)
читають лише для підпису моделі, за `[reads] tool_settings` (див. Підписи
моделей нижче).

### Вхід за посиланням (звірено наживо 2026-11-10)

Власний сценарій редактора, зв'язаний PKCE. Входу для інших застосунків
немає, тож віджет входить від імені OAuth-клієнта редактора Cursor
(`client_id` нижче); запитання згоди і сторінка приватності кажуть про це.

1. verifier = base64url без доповнення від 32 випадкових байтів;
   challenge = base64url без доповнення від SHA-256 ASCII-рядка verifier;
   uuid = випадковий UUID версії 4.
2. Браузер відкриває
   `https://cursor.com/loginDeepControl?challenge=<challenge>&uuid=<uuid>&mode=login`,
   і користувач тисне *Sign in*. Сама програма на cursor.com нічого не
   надсилає.
3. Віджет опитує `GET https://api2.cursor.sh/auth/poll?uuid=<uuid>&verifier=<verifier>`:
   404, поки вхід не підтверджено, далі 200
   `{"accessToken": "...", "refreshToken": "...", "authId": "...", "challenge": "...", "uuid": "..."}`.
   Обидва токени були тим самим JWT (`sub` на кшталт `github|user_…`, час
   життя близько 60 днів). Опитування чекає 1 с, далі щоразу в 1.2 раза
   довше, до 8 с, і здається через 5 хвилин. Verifier несе лише опитування,
   і воно ніколи не логується.

Сеанс зберігається як UTF-8 JSON: токен, його `exp`, остання спроба
оновлення і чи завершив його Cursor. У віджеті вхід чекає у фоновій
задачі; тим часом Sign-in у Cursor пише "Waiting for the browser" і
пропонує *Cancel*. Чим він закінчився, каже вікно ("Signed in.", "The
sign-in timed out after 5 minutes. Try again.", "The sign-in did not work.
Try again." чи "Cancelled."), а тостом — лише поки вікно закрите. Вхід
вмикає Cursor і показує його рядок. Повторний вхід замінює сеанс (інший
акаунт або сеанс, який завершив Cursor); *Sign out* там само чи
`ailimits-auth logout cursor` його забуває, і деінсталятор інсталятора
запускає саме цю команду. З вікна перезапуск не потрібен: провайдер
перебудовується, а оновлення, що вже йшло для попереднього сеансу,
відкидається. `ailimits-auth login cursor` для Cursor, якого віджет ще не
запускає, вмикає його в конфігу, а запущений віджет підхоплює це за кілька
секунд: перезапуск теж не потрібен.

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
`access_token` як обидва токени. Сеанс після його `exp` не оновлюється:
рядок просить увійти знову, і для нього нічого не надсилається.
Завершує сеанс лише `shouldLogout: true`.
Відхилене оновлення (401/403, поки не траплялося) лишає сеанс: воно
зараховується як спроба цього дня, а сеанс використовується, доки його не
відхилить сам сервіс. Оновлення, що не отримало жодної відповіді (офлайн,
тайм-аут), теж зараховується як спроба цього дня, тож ПК без мережі не
пробує знову щоциклу. Спроба позначається і в збереженому сеансі, і в
пам'яті провайдера, тож вона зараховується, навіть коли Credential Manager
не прийняв оновлений сеанс. Чи подовжує оновлення біля `exp` сеанс, ще не
звірено; якщо ні, сеанс живе близько 60 днів, а далі рядок просить увійти
знову.

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

→ підпис — `planName` (`Premium`, очищений: без керівних символів,
щонайбільше 32 символи); без нього — період (`includedUsagePeriod`:
`Weekly`, `Monthly`, `Daily`), далі довжина циклу, інакше `Included`.

**`GetSandUsageStatus`** — Grok Bot, додаткова метрика, яку питають лише
поки її рядок (`cursor.bot`) вибрано у віджеті чи підказці; вибір чи
скасування цього рядка перебудовує лише Cursor і зберігає його дані. Новий
екземпляр опитується одразу, а його кеш плану й лічильник відмов
починаються спочатку, тож він знову питає план:

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
Grok Bot), не дає метрики. Вона позначена лише для свого додаткового рядка,
тож ніколи не перехоплює головне число, скидання чи сповіщення. Поки рядок
не позначено, вікно вибору пише "Its share of the same limit, asked for
once this row is ticked". На звіреному акаунті Grok, Cursor і Grok Bot
ділять ОДИН тижневий пул (те саме скидання, той самий відсоток); як це
показувати — відкрите рішення, тож `isUnified` переноситься як
`unified_pool` метрики, щоб пізніше ним скористалося відображення.
Будь-яка невдача цього виклику лише прибирає метрику.

Без сеансу (звірено): HTTP 401
`{"code": "unauthenticated", ..., "debug": {"error": "ERROR_NOT_LOGGED_IN", ...}}`.

Стани (кожна помилка лишає останні дані сірими з давністю):

- власний сеанс відхилено (401/403): якщо до `exp` лишилося не більше 7
  днів, одне оновлення (не частіше разу на добу) і ще одна спроба; поки
  відмов поспіль менше трьох → `"token rejected, retrying"`; третя
  завершує сеанс → `"sign in to Cursor again"`. Будь-яка інша відповідь
  перериває серію;
- власний сеанс після `exp`, завершений через `shouldLogout` або
  збережений запис, який ця версія не може прочитати, →
  `"sign in to Cursor again"`, без жодного запиту використання й без
  оновлення;
- Credential Manager не відповідає → `"Credential Manager unavailable"`;
- `"unrecognized usage schema"`, як вище; відсутність відповіді й інші
  статуси → спільні слова (див. Стани помилок).

Підказка під курсором додає "sign in to Cursor" до статусу, що не
закінчується на ", retrying" і ще цього не каже; рядок ніколи не пише
"check the key", бо ключа ніхто не давав. Про запити за цикл див. Запити за
цикл угорі.

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

Вони читаються для кожного провайдера, що працює (`Config::provider_active`),
хай як він входить: і для налаштованого лише власним входом програми
(вставлений ключ чи usage-токен, власний вхід у Cursor), чий вхід
інструмента ніколи не читається. Запитання про налаштування каже про це.
Якщо зняти останню з Model, Version і Effort на обох поверхнях, обидва
читання знову стають false; повернення до «As set in the tool» знову
ставить `last_session` у false. Водночас іде щонайбільше одне читання;
запит, що надійшов під час іншого, виконує той самий робітник одразу після
нього, а відкликане читання нічого не читає.

### Файли налаштувань (`tool_settings`)

| Інструмент | Файл | Ключі |
|---|---|---|
| Claude Code | `%USERPROFILE%\.claude\settings.json` | `ANTHROPIC_MODEL` (таблиця `env` самого файлу, інакше оточення, яке бачить програма), інакше `model`; effort `modelSettings.<model>.effortLevel`, інакше той, що в найновішої моделі родини псевдоніма, інакше `effortLevel` верхнього рівня |
| Codex CLI | `%CODEX_HOME%\config.toml`, інакше `%USERPROFILE%\.codex\config.toml` | `model`, `model_reasoning_effort` верхнього рівня; з `profile = "x"` верхнього рівня спершу ті, що в `[profiles.x]` |
| Grok CLI | `%GROK_HOME%\config.toml`, інакше `%USERPROFILE%\.grok\config.toml` | `[models] default`, `[models] default_reasoning_effort` |
| Copilot CLI | `%COPILOT_HOME%\settings.json`, інакше `%USERPROFILE%\.copilot\settings.json` | `model`, `effortLevel` (JSON із коментарями: коментарі й кінцеві коми дозволені) |
| Antigravity CLI | `%USERPROFILE%\.gemini\antigravity-cli\settings.json` | `model` (підпис для показу: `Claude Sonnet 4.6 (Thinking)`; окремого effort немає) |
| Cursor (редактор) | `%APPDATA%\Cursor\User\globalStorage\state.vscdb` | один запис, `...persistentStorage.applicationUser`, і з нього лише `aiSettings.modelConfig.composer`: `modelName`, параметр `reasoning_effort` |

`CODEX_HOME`, `GROK_HOME` і `COPILOT_HOME` враховуються лише як абсолютні
шляхи (саме це означає «when it is set» у тексті згоди); відносний шлях
означає, що файли інструмента не читаються взагалі (`config/paths.rs`).

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
  `ANTHROPIC_MODEL` іде перед `model`: спершу з таблиці `env` самого
  settings.json, потім з оточення, яке бачить програма. Це відповідає
  задокументованому порядку Claude Code (`/model`, потім `--model`, потім
  `ANTHROPIC_MODEL`, потім файл налаштувань); наживо не звірено.
- Codex CLI: `model` і `model_reasoning_effort` на верхньому рівні.
  `profile = "x"` верхнього рівня враховується: ключі `[profiles.x]`
  перекривають ключі верхнього рівня, ключ, якого профіль не задає, бере
  значення верхнього рівня, а названий, але відсутній профіль нічого не
  додає. Модель, задана лише в таблиці `[profiles.*]`, яку не вибирає жоден
  профіль, не читається.
- Grok CLI: `[models] default_reasoning_effort` є; `[models] default` — з
  власної документації CLI (`user-guide/05-configuration.md`), там його не
  задано (вбудована типова модель CLI: підпис — сам effort).
- Copilot CLI: `model` (`"auto"`); `effortLevel` — з його документації, там
  не задано. GitHub документує файл як JSON із коментарями (JSONC):
  коментарі `//` і `/* */` поза рядками та кінцеві коми пропускаються.
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
  (показується як "Auto"). `json_extract` самого SQLite віддає лише цей
  один об'єкт: решта запису (близько 0,5 МБ налаштувань редактора) і всі
  інші записи, зокрема вхід редактора, із бази не виходять. SQLite у
  Windows без функцій JSON віддає запис цілим лише тоді, коли він не
  більший за 8 МБ; розмір перевіряється всередині SQLite, тож більший запис
  із бази не виходить, а об'єкт тоді дістає вже програма. Читається знову
  лише тоді, коли змінилася база або її файл `-wal`; один запит тривав
  1,3 мс. Cursor CLI зберігає свою модель у
  `%USERPROFILE%\.cursor\cli-config.json` (поруч з e-mail акаунта); цей файл
  не читається.

  **У `%APPDATA%\Cursor` ніколи нічого не пишеться й не створюється.** База
  належить редактору, який може тримати її відкритою і писати в неї (вона в
  режимі WAL). Її читають через SQLite, що постачається з Windows
  (`System32\winsqlite3.dll`, вантажиться при першому використанні), з busy
  timeout 200 мс, одним із трьох способів: поки Cursor закритий (поруч
  немає `state.vscdb-wal`) — як `file:...?immutable=1`, лише для читання,
  без блокування і без створення `-wal` чи `-shm`; поки Cursor працює — лише
  для читання, читаючи власні `-wal` і `-shm` Cursor як є і нічого не
  створюючи; базу в режимі rollback-журналу — просто лише для читання.
  Утримане блокування — тимчасова помилка, яку повторює наступний цикл.

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

Чого підпис знати не може:

- `/model` у сесії, прапорці `--model`, `--profile` і `-m`,
  `.claude/settings*.json` проєкту і змінні, задані лише в профілі
  оболонки, програмі не видно;
- effort псевдоніма — це effort найновішої збереженої моделі його родини,
  тож після виходу нової моделі, доки для неї не задано `/effort`, підпис
  може показувати effort старішої;
- у Last used інструменти без сесій (і сесії ще без моделі) показують
  налаштовану модель, без позначки;
- без settings.json Claude Code узагалі сам `ANTHROPIC_MODEL` підпису не
  дає.

### Межі та збої

- Файли налаштувань понад 256 КБ не читаються; файл сесії читається по
  64 КБ і щонайбільше на 1 МБ від кінця (пам'ять: щонайбільше близько 2 МБ,
  поки складається рядок завдовжки з усе читання); рядок, що перетинає
  межу кроку, складається цілим, рядок, обрізаний межею 1 МБ,
  відкидається.
- UTF-8 BOM і кінці рядків CRLF приймаються. Відсутній файл, зламаний JSON
  чи TOML, хибний тип, значення довше 128 байт чи з керівними символами:
  для цього інструмента підпису немає. Ніколи не стан помилки, ніколи не
  panic.
- Файл, який зараз не вдається прочитати (його тримає сканер чи резервне
  копіювання, помилка вводу-виводу, файл сесії, вкорочений під час
  читання, зайнята база Cursor), лишає показаний підпис і читається знову
  наступного циклу. Підпис визначає лише прочитаний файл.
- Прочитаний файл читається знову, лише коли змінився час його зміни чи
  довжина.
- Читається раз за цикл оновлення (щонайменше 60 с): планувальник запускає
  читання на одному з блокувальних потоків середовища виконання
  (`spawn_blocking`), по одному за раз (`ReadGate`: цикл, який застав
  читання ще в роботі, лише просить його прочитати ще раз); без
  спостерігачів, таймерів і власних потоків. Пошук найновішої сесії — один перелік
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
Перелік вшитий у збірку, тож його оновлюють із кожним релізом.
Перелічуються лише інструменти, що працюють. Модель, спільна для кількох
інструментів, має одну назву, але поле під кожним інструментом із
позначками "shared with ..." і "in use" там, де вона використовується;
набране в одному видно в інших. Власний роутер інструмента (`auto`,
`default`) має назву для кожного інструмента окремо, під ключем
`"provider:id"` (`"copilot:auto"`, `"cursor:auto"`); назва, яку старіша
збірка зберегла під голим `"auto"`, діє для обох, доки один не
перейменують, і тоді розщеплюється. Віджет шукає назви так само, тож вікно
вибору й віджет збігаються.

## Видалені провайдери і методи

- **Gemini через API-ключ** — публічний Generative Language API не віддає
  rate-limit даних (лише бінарні 200/429), тож початковий key-провайдер
  Gemini було видалено. ПОТОЧНИЙ Antigravity-провайдер (секція вище) натомість
  читає OAuth-токен Antigravity і питає quota-endpoint'и Code Assist, які
  повертають реальне використання: спершу зведення пулів, а квоту по
  моделях — як його fallback.
- **Legacy-вхід Gemini CLI** — `%USERPROFILE%\.gemini\oauth_creds.json`
  (`access_token`, `expiry_date` у мілісекундах epoch) був запасним
  джерелом Antigravity до 0.7.1. Видалено: Google припинив обслуговувати
  акаунти individual, Google AI Pro і Google AI Ultra через Gemini CLI
  2026-06-18.
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
- **Токен редактора Cursor** — читання власного токена редактора з його
  бази стану, поки той ще зберігався незашифрованим, як запасний шлях без
  власного сеансу віджета. Прибрано ще до виходу 0.7.1: віджет ніколи не
  читає чужий вхід із його файлів, а ключеві, який шукають деякі крадії
  облікових даних, не місце в exe. Використовується лише власний вхід
  віджета (Rows and models або `ailimits-auth login cursor`).
