# AI Limits name and logo

Copyright (C) 2026 napxlexn.

The source code and other copyrightable parts of AI Limits are licensed under
the GNU General Public License, version 3 or later. That license does not grant
permission to use the AI Limits name or logo in a way that suggests a modified
build is an official release or is endorsed by the project.

You may:

- refer truthfully to AI Limits and the origin of derived code;
- redistribute unmodified official releases with their original name and logo;
- state that a modified work is based on or compatible with AI Limits.

Modified distributions must use a distinct name and visual identity unless
napxlexn gives prior written permission. This policy does not restrict
descriptive or other uses permitted by applicable trademark law.

## Forks: beside AI Limits, not over it

A modified build that keeps the identifiers below takes over an installed AI
Limits: its installer upgrades over it (or the other way round), the two
share one config folder and one set of stored keys, and only one of them can
run. Please give a fork its own:

- the installer's `AppId` and `AppName` (`installer/ailimits.iss`), and the
  autostart value `AiLimits` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`;
- the single-instance mutex `Local\AiLimitsWidgetSingleInstance` (`src/main.rs`);
- the config folder `%APPDATA%\AiLimits` (`src/config/storage.rs`);
- the Credential Manager service `ailimits` (`src/secrets.rs`, and the
  labels `ailimits-auth` and the providers read under it);
- the notification identity `napxlexn.AILimits` (`src/platform/win.rs`,
  `APP_USER_MODEL_ID`, and the `AppUserModelID` in the installer);
- the update source `REPO` and the release keys `RELEASE_KEYS`
  (`src/updater.rs`, `src/updater/signature.rs`): with ours left in, a fork
  either updates itself into AI Limits or refuses every release of its own;
- the package identities (`installer/msix`, `installer/winget`,
  `installer/choco`).

---

# Назва й логотип AI Limits

Copyright (C) 2026 napxlexn.

Вихідний код та інші об'єкти авторського права AI Limits ліцензовано за GNU
General Public License версії 3 або пізнішої. Ця ліцензія не надає дозволу
використовувати назву чи логотип AI Limits так, щоб змінена збірка виглядала
офіційним релізом або схваленим проєктом продуктом.

Дозволено:

- правдиво згадувати AI Limits і походження похідного коду;
- поширювати незмінені офіційні релізи з оригінальною назвою та логотипом;
- зазначати, що змінена робота базується на AI Limits або сумісна з ним.

Змінені дистрибутиви повинні мати іншу назву й візуальну ідентичність, якщо
napxlexn не надав попереднього письмового дозволу. Ця політика не обмежує
описове та інше використання, дозволене застосовним законодавством про
торговельні марки.

## Форки: поруч з AI Limits, а не поверх

Змінена збірка, яка лишає ідентифікатори нижче, займає місце встановленого
AI Limits: її інсталятор оновлюється поверх нього (або навпаки), обидві
ділять одну теку конфігу й один набір збережених ключів, і працювати може
лише одна з них. Будь ласка, дайте форку власні:

- `AppId` і `AppName` інсталятора (`installer/ailimits.iss`) і значення
  автозапуску `AiLimits` у `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`;
- м'ютекс єдиного запуску `Local\AiLimitsWidgetSingleInstance` (`src/main.rs`);
- теку конфігу `%APPDATA%\AiLimits` (`src/config/storage.rs`);
- сервіс Credential Manager `ailimits` (`src/secrets.rs`, а також мітки,
  які під ним читають `ailimits-auth` і провайдери);
- ідентичність сповіщень `napxlexn.AILimits` (`src/platform/win.rs`,
  `APP_USER_MODEL_ID`, і `AppUserModelID` в інсталяторі);
- джерело оновлень `REPO` і ключі релізів `RELEASE_KEYS` (`src/updater.rs`,
  `src/updater/signature.rs`): якщо лишити наші, форк або оновить себе до
  AI Limits, або відкидатиме кожен свій реліз;
- ідентичності пакетів (`installer/msix`, `installer/winget`,
  `installer/choco`).
