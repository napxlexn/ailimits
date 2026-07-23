# CONFIG.md — конфігураційний файл

Розташування: `%APPDATA%\AiLimits\config.toml`. Створюється автоматично
при першому запуску; кожна зміна з контекстного меню зберігається сюди.
Невідомі поля і провайдери ігноруються (старі конфіги ніколи не ламають
парсинг). При помилці парсингу віджет пише попередження в лог і бере
дефолти.

Поруч віджет може тримати `provider-cache.json` (останні відомі метрики з
майбутнім скиданням — переживають рестарт), `history.jsonl` (обмежена історія
використання для спарклайна в Expanded) і, при заданому `AILIMITS_LOG` або
`RUST_LOG`, `ailimits.log`.

## Повний приклад

```toml
[general]
# Інтервал оновлення в секундах (меню: 60 / 300 / 900 / 1800; мінімум 60).
# Опитування стає на паузу при бездіяльності/заблокованому екрані та
# відступає при повному провалі циклу; endpoint Claude ще й обмежений на
# боці сервера, тож його оновлення інколи займає 2–3 хвилини незалежно від
# цього налаштування.
update_interval_secs = 60
# Індикатор на панелі завдань (меню: Indicator): "tray" — кругова
# трей-іконка 16px з найвищим %; "panel_rows" / "panel_grid" (застарілий
# псевдонім) — прозорий overlay зліва від трею: layered-вікно з per-pixel
# alpha, малюються лише цифри й бари. Показує перші два провайдери в
# порядку віджета (відсоток + бар, розмір годинника), монохром, слідує
# СИСТЕМНІЙ світлій/темній темі; решта — у tooltip. Стежить за
# авто-приховуванням і шириною трея подієво, без полінгу. "bars" —
# застаріла трей-іконка; "off" — нічого. Замінює застарілий прапорець
# show_tray_icon (ігнорується).
indicator = "tray"

[window]
pos_x = 50
pos_y = 50
# Прозорість фону 0.10–0.85.
opacity = 0.45
# Поверх всіх вікон.
pinned = false
# Заблокована позиція — drag вимкнено (незалежно від `pinned`).
locked = false

[ui]
# Палітра: default / ocean / sunset / forest / neon / ice / rose / slate.
palette = "default"
# Насиченість палітри 0–100 (0 = грейскейл, 100 = повний колір).
saturation = 55
# Яскравість тексту і барів 20–100.
brightness = 100
# Монохромний режим (перекриває палітру).
monochrome = false
# Лейаут: "vertical" або "horizontal".
layout = "vertical"
# Рівень деталізації: "compact" / "medium" / "expanded".
detail = "compact"
# Прогноз burn-rate ("~Xh to limit", коли використання росте; меню: Forecast).
# Ніколи не замінює зворотний відлік до скидання — показується лише коли
# майбутній reset невідомий.
show_forecast = false

[[providers]]
# Ідентифікатор: claude / codex / copilot / antigravity ("gemini" —
# застарілий id, перейменовується на "antigravity" при завантаженні).
id = "claude"
enabled = true
# Метод: "subscription" (дефолт) або "api_key" — вибір має лише Claude.
auth_method = "subscription"
# Мітка ключа в Credential Manager; порожня для підписки.
credential_label = ""
# Поріг сповіщень, %.
alert_threshold = 80

[[providers]]
id = "codex"
enabled = true
auth_method = "subscription"
alert_threshold = 80

[[providers]]
id = "copilot"
enabled = true
auth_method = "subscription"
alert_threshold = 85

[[providers]]
id = "antigravity"
enabled = true
auth_method = "subscription"
alert_threshold = 80

[notifications]
enabled = true
# Cooldown тостів на провайдера, хвилини.
cooldown_minutes = 15

[hooks]
# Опційні shell-команди на події використання (порожньо = вимкнено,
# дефолт). Кожна запускається відокремлено через `cmd /C`, приховано,
# fire-and-forget; контекст події — в змінних оточення: AILIMITS_EVENT
# (threshold|reset|startup), AILIMITS_PROVIDER, AILIMITS_PERCENT,
# AILIMITS_RESET_AT (RFC3339, коли відомо).
# БЕЗПЕКА: ці поля навмисно редагуються ЛИШЕ у конфізі — немає шляху через
# меню чи буфер обміну, тож ніщо не може само-підставити команду на запуск.
# Спрацьовує при перетині порогу сповіщень (ділить cooldown з тостом):
on_threshold = ""
# Спрацьовує, коли вікно провайдера біля ліміту скидається (різке падіння %):
on_reset = ""
# Спрацьовує один раз після першого успішного запиту:
on_startup = ""
```

## Секрети

Конфіг ніколи не містить ключів чи токенів — лише мітки. Значення живуть
у Windows Credential Manager під service `ailimits`:

| Мітка | Призначення |
|---|---|
| `claude_api_key` | API ключ Claude (метод api_key) |
| `claude_usage_token` | ручний usage-токен підписки Claude |
| `codex_usage_token` | ручний usage-токен ChatGPT/Codex |
| `copilot_pat` | GitHub PAT (замість gh CLI) |

Rust-структури — у `src/config/schema.rs`; цей файл є єдиним джерелом
правди для схеми.
