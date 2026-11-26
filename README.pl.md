# AI Limits

<p align="center">
<a href="https://napxlexn.github.io/ailimits/"><img alt="Platform" src="https://img.shields.io/badge/platform-Windows%2011-0078d4?style=flat-square"></a>
<a href="https://www.rust-lang.org/"><img alt="Built with" src="https://img.shields.io/badge/built%20with-Rust-dea584?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/blob/master/LICENSE"><img alt="License" src="https://img.shields.io/github/license/napxlexn/ailimits?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/napxlexn/ailimits/total?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/napxlexn/ailimits/ci.yml?branch=master&style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/napxlexn/ailimits?style=flat-square"></a>
<a href="https://github.com/microsoft/winget-pkgs/tree/master/manifests/n/napxlexn/AILimits"><img alt="winget" src="https://img.shields.io/winget/v/napxlexn.AILimits?style=flat-square"></a>
<a href="https://community.chocolatey.org/packages/ailimits"><img alt="Chocolatey" src="https://img.shields.io/chocolatey/v/ailimits?style=flat-square"></a>
<a href="https://github.com/napxlexn/scoop-ailimits"><img alt="Scoop" src="https://img.shields.io/scoop/v/ailimits?bucket=https%3A%2F%2Fgithub.com%2Fnapxlexn%2Fscoop-ailimits&style=flat-square"></a>
<a href="https://apps.microsoft.com/detail/9P2B0J1VWPWH"><img alt="Microsoft Store" src="https://img.shields.io/badge/dynamic/regex?url=https%3A%2F%2Fdisplaycatalog.mp.microsoft.com%2Fv7.0%2Fproducts%3FbigIds%3D9P2B0J1VWPWH%26market%3DUS%26languages%3Den-us&search=AILimits_(%5Cd%2B%5C.%5Cd%2B%5C.%5Cd%2B)&replace=v%241&label=Microsoft%20Store&style=flat-square"></a>
</p>

Niewielka, natywna, pływająca nakładka dla systemu Windows 11, która pokazuje
limity użycia u dostawców AI bezpośrednio na pulpicie: **Claude**,
**OpenAI Codex**, **GitHub Copilot**, **Google Antigravity**, **xAI Grok**,
**Cursor**.

<p align="center"><a href="https://napxlexn.github.io/ailimits/"><img alt="Otwórz stronę internetową" src="https://img.shields.io/badge/OPEN%20THE%20WEBSITE-0078d4?style=for-the-badge"></a></p>

<p align="center" class="languages"><a href="README.md">English</a> · <a href="README.uk.md">Українська</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.es.md">Español</a> · <a href="README.pt-BR.md">Português (Brasil)</a> · <a href="README.de.md">Deutsch</a> · <a href="README.fr.md">Français</a> · <b>Polski</b> · <a href="README.it.md">Italiano</a> · <a href="README.nl.md">Nederlands</a> · <a href="README.cs.md">Čeština</a></p>

Dostępna w [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH), podpisana i aktualizowana przez Store, oraz w katalogu [AlternativeTo](https://alternativeto.net/software/ai-limits/about/) — jeśli aplikacja okaże się przydatna, polubienie jej tam pomoże innym ją znaleźć.

<p align="center">
<a href="https://apps.microsoft.com/detail/9P2B0J1VWPWH"><img alt="Get it from Microsoft" src="https://get.microsoft.com/images/en-us%20dark.svg" height="52"></a>
</p>

<p align="center">
  <img src="docs/images/widget-detail.gif" width="380" alt="Pływający widżet przełącza się na żywo między poziomami szczegółowości Compact, Medium i Expanded, z poziomymi paskami postępu">
  <br><sub>Trzy poziomy szczegółowości: Compact, Medium, Expanded. Przełączanie działa na żywo; opcjonalna prognoza tempa zużycia (~czas do limitu) pojawia się w wierszu z informacjami dodatkowymi tam, gdzie czas resetu nie jest znany. Tutaj z poziomymi paskami.</sub>
</p>

<p align="center">
  <img src="docs/images/indicator.gif" width="440" alt="Wskaźnik na pasku zadań: kursor przesuwa się z pulpitu na panel, a potem na pierścienie w zasobniku, nad każdym z nich na tej samej wysokości pojawia się podpowiedź, a każdy tryb przełącza się między ciemnym i jasnym motywem Windows">
  <br><sub>Gdy widżet jest ukryty, użycie pozostaje widoczne na pasku zadań. Ikona w zasobniku pokazuje jako dwa pierścienie tych samych dwóch dostawców co panel: pierwszych dwóch z widżetu albo dwóch, których wybierzesz. Po najechaniu kursorem na panel lub ikonę w zasobniku pojawia się ta sama lista: wiersze zaznaczone w kolumnie Tooltip. Oba dostosowują się do jasnego i ciemnego motywu systemu.</sub>
</p>

<table>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-detail-vertical.gif" height="230" alt="Te same poziomy szczegółowości z pionowymi paskami postępu">
  <br><sub>Te same trzy poziomy z pionowymi paskami.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-width.gif" height="230" alt="Widżet zwęża się kolejnymi stopniami szerokości; przy najwęższym nazwy dostawców znikają, a ich miejsce zajmują paski">
  <br><sub>Trzy stopnie szerokości. Przy najwęższym nazwy dostawców są pomijane, a ich kolumnę zajmuje pasek, więc nic na siebie nie nachodzi.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-opacity.gif" height="230" alt="Tło widżetu przechodzi od niemal całkowicie przezroczystego do niemal nieprzezroczystego, a tekst pozostaje czytelny">
  <br><sub>Krycie tła, od niemal przezroczystego do niemal pełnego. Kontrast tekstu i pasków się nie zmienia.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-providers.gif" height="230" alt="Widżet rośnie od jednego wiersza dostawcy do czterech i z powrotem">
  <br><sub>Rozmiar karty zależy od wyświetlanych wierszy: po jednym na każdego włączonego dostawcę plus wybrane wiersze dodatkowe.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/palette.gif" height="230" alt="Widżet przechodzi kolejno przez paletę monochromatyczną i osiem palet kolorowych przy zmienianych suwakach jasności, nasycenia i krycia tła">
  <br><sub>Paleta monochromatyczna i osiem kolorowych, z jasnością, nasyceniem i kryciem tła. Renderowane przez samą aplikację.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-arrangement.gif" height="230" alt="Układ z pionowymi paskami przełącza się między dostawcami obok siebie a dostawcami ułożonymi w wysoką, wąską kolumnę">
  <br><sub>Przy pionowych paskach dostawcy stoją obok siebie albo układają się w kolumnę.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/settings-providers.gif" width="400" alt="Okno Settings obok widżetu: zaznaczenie tygodniowego limitu Opus w kolumnie Widget od razu dodaje jego wiersz do widżetu, a potem rozwija się Sign-in dostawcy Claude, pokazując, skąd pochodzi logowanie, co jest odczytywane, oraz Forget permission">
  <br><sub><em>Settings</em>, Providers: zaznaczony wiersz od razu pojawia się w widżecie; Sign-in każdego dostawcy mówi, co jest odczytywane i dokąd trafia jego token.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/settings-display.gif" width="400" alt="Karta Display okna Settings: odznaczenie Progress bar zamienia wiersze widżetu w pojedyncze linie i obniża widżet, Time to reset znika i wraca, po czym wraca pasek">
  <br><sub><em>Settings</em>, Display: co pokazuje każdy wiersz; bez paska widżet jest mniejszy. Rysowane przez samą aplikację.</sub>
</td>
</tr>
</table>

## Co robi

- **Tylko odczytuje, nigdy nie zapisuje.** Odczytuje tokeny, które oficjalne
  narzędzia CLI już przechowują na tym komputerze. Nigdy ich nie odświeża ani
  nie rotuje, więc nie może spowodować wylogowania z tych narzędzi. Jedynym
  wyjątkiem jest Cursor: aplikacja loguje się do niego własną sesją i nigdy
  nie odczytuje ani nie zmienia logowania edytora. Klucze dodane
  samodzielnie oraz ta sesja są przechowywane w Windows Credential Manager.
  Aplikacja łączy się wyłącznie z punktami końcowymi samych dostawców (a
  kopia z instalatora także z serwisem GitHub w celu pobrania aktualizacji),
  wysyła jedno zapytanie o użycie na dostawcę na cykl (plus kilka zapytań
  pomocniczych wymienionych w
  [docs/en/PROVIDERS.md](docs/en/PROVIDERS.md#requests-per-cycle) (po angielsku)),
  korzysta z serwera proxy ustawionego w systemie Windows i nie wysyła
  telemetrii.
- **Pyta przed odczytem.** Nic, co należy do innego narzędzia, nie jest
  odczytywane, dopóki na to nie zezwolisz, a każde pytanie podaje plik i
  miejsce, do którego wysyłany jest jego token. O każdą zgodę aplikacja pyta
  raz, a *Forget permission* cofa ją jednym kliknięciem. Zobacz
  [Co jest odczytywane i kiedy](docs/en/CONFIG.md#what-is-read-and-when) (po angielsku).
- **Niewiele konfiguracji.** Przy pierwszym uruchomieniu narzędzia znalezione
  na tym komputerze są już zaznaczone, a jedno pytanie wymienia każdy plik
  logowania, który zostałby odczytany: wystarczy *Allow*. Potem zwykła praca
  z narzędziami CLI utrzymuje ważność ich tokenów, więc nie ma już nic do
  konfigurowania. Tylko Cursor wymaga jednorazowego zalogowania w
  *Settings*.
- **Niskie obciążenie, zmierzone na działającej aplikacji.** 0.014% jednego
  rdzenia CPU, zero użycia GPU (renderowanie odbywa się na CPU), zestaw
  roboczy 53 MB (25 MB pamięci prywatnej), zmierzone dla wersji 0.7.1 w dniu
  2026-11-23; bez podnoszenia rozdzielczości timera (2026-11-21); plik exe o
  rozmiarze 4 MB. Pomiar można powtórzyć skryptem
  `bench/perf_audit.ps1`; pełne wyniki są w
  [docs/en/ARCHITECTURE.md](docs/en/ARCHITECTURE.md#resource-footprint-measured-not-estimated) (po angielsku).
- **Nieaktualne dane są oznaczone jako nieaktualne.** Wartość, która przestaje
  się aktualizować, zostaje wyszarzona i pokazuje swój wiek. Okno limitu,
  które na pewno już się zresetowało, pokazuje `≈0%`. Aplikacja nie wypełnia
  luk domysłami. Gdy nie ma cię przy komputerze, odpytywanie jest
  wstrzymane, a ten czas się nie liczy: nic nie szarzeje tylko z powodu
  chwilowej nieobecności.
- **Samowystarczalna.** Linkowana statycznie, więc nie wymaga Visual C++
  Redistributable ani dołączonych bibliotek DLL. Instalator nie potrzebuje
  uprawnień administratora.

## Instalacja

> **Aktualizujesz z wersji 0.7.0 (lub 0.6.x), a ścieżka profilu użytkownika
> lub folderu TEMP zawiera litery spoza alfabetu angielskiego (np. cyrylicę),
> `&` lub `%`?** Mechanizm aktualizacji starej wersji nie radzi sobie z taką
> ścieżką: po wydaniu 0.7.1 może zamykać AI Limits mniej więcej pół minuty po
> każdym uruchomieniu. Zainstaluj 0.7.1 ręcznie: pobierz
> `AiLimits-Setup-0.7.1.exe` ze strony
> [Releases](https://github.com/napxlexn/ailimits/releases) i uruchom go albo,
> gdy winget będzie już oferować 0.7.1, wykonaj `winget upgrade napxlexn.AILimits`.
> Jeśli widżet zamknie się, zanim zdążysz to zrobić, kliknij go prawym
> przyciskiem zaraz po uruchomieniu i najpierw odznacz **Automatic updates**
> (albo ustaw `auto_update = false` w `%APPDATA%\AiLimits\config.toml`). Od
> wersji 0.7.1 mechanizm aktualizacji uruchamia instalator bezpośrednio, takie
> ścieżki działają i można ponownie zaznaczyć Automatic updates.

Z poziomu terminala:

```powershell
# winget
winget install napxlexn.AILimits

# one-line install: downloads the latest release, verifies its SHA-256
# against the published digest (the SHA-256 only, not the .sig), runs the
# silent per-user install (no admin rights)
irm https://raw.githubusercontent.com/napxlexn/ailimits/master/install.ps1 | iex

# Scoop, from the project's own bucket
scoop bucket add ailimits https://github.com/napxlexn/scoop-ailimits
scoop install ailimits

# Chocolatey
choco install ailimits

# Microsoft Store (signed and updated by the Store), or https://apps.microsoft.com/detail/9P2B0J1VWPWH
winget install --id 9P2B0J1VWPWH --source msstore
```

Albo pobierz aplikację ze sklepu [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH):
to ten sam widżet, podpisany i aktualizowany przez Store (może pojawiać się
tam z kilkudniowym opóźnieniem względem wydania, gdy nowy pakiet przechodzi
certyfikację).

Albo pobierz `AiLimits-Setup-<version>.exe` ze strony
[Releases](https://github.com/napxlexn/ailimits/releases) i uruchom go.
Uprawnienia administratora nie są potrzebne (instalacja do `%LOCALAPPDATA%\AiLimits`).
Opcjonalnie podczas instalacji: autostart z systemem Windows (domyślnie
zaznaczony) i skrót na pulpicie. winget i Chocolatey uruchamiają ten sam
instalator, więc ich kopia aktualizuje się sama jak każda kopia z instalatora,
a menedżer pakietów może pokazywać starszą wersję niż ta, która działa.
Do każdego wydania dołączone jest też przenośne archiwum zip (bez
instalatora). Kopia przenośna nie aktualizuje się sama: aktualizuje ją jej
menedżer pakietów albo trzeba to zrobić ręcznie. Nie ma trybu przenośnego dla
ustawień: każda kopia, także przenośna, przechowuje je w
`%APPDATA%\AiLimits`.

> Instalator **nie ma podpisu kodu** (Authenticode), więc Windows SmartScreen
> może wyświetlić komunikat „System Windows ochronił ten komputer”. Kliknij
> **Więcej informacji → Uruchom mimo to**. Pobrany plik można zweryfikować za
> pomocą sumy SHA-256 publikowanej przy każdym wydaniu (poniżej). Od wersji
> 0.7.1 każde wydanie zawiera też podpis Ed25519 (`.sig`) instalatora i
> archiwum zip, a od 0.7.1 własny mechanizm aktualizacji aplikacji nie
> instaluje niczego bez ważnego podpisu. Mechanizm aktualizacji z wersji 0.7.0
> ignoruje `.sig` i sprawdza tylko sumę SHA-256, którą GitHub publikuje dla
> instalatora: jego aktualizacja do 0.7.1 opiera się na tym skrócie.

Aby ręcznie sprawdzić pobrany plik, porównaj jego sumę SHA-256 ze skrótem
`sha256:` widocznym obok pliku na stronie wydania; obie wartości muszą się
zgadzać:

```powershell
(Get-FileHash .\AiLimits-Setup-0.7.1.exe -Algorithm SHA256).Hash
```

Aby sprawdzić również podpis, umieść plik `.sig` obok pobranego pliku, oba pod
nazwami, pod którymi zostały opublikowane (podpis obejmuje nazwę). Następnie w
klonie tego repozytorium przełączonym na tag wydania (`git checkout v0.7.1`)
uruchom polecenie `verify` narzędzia do podpisywania wydań, które nie wymaga
klucza (potrzebny jest Rust, jak w sekcji
[Kompilacja ze źródeł](#kompilacja-ze-źródeł)):

```powershell
cargo run --locked --release --manifest-path tools/release-signer/Cargo.toml -- verify $HOME\Downloads\AiLimits-Setup-0.7.1.exe
```

Polecenie wypisuje `verified` albo `NOT verified` wraz z przyczyną.

Instalator nie zawiera żadnych kluczy ani tokenów. Po pierwszym uruchomieniu
widżet nie odczytuje żadnego z opisanych niżej źródeł uwierzytelniania, dopóki
na to nie zezwolisz.

### Odinstalowanie

Odinstalowanie w Ustawieniach systemu Windows (Aplikacje), poleceniem
`winget uninstall napxlexn.AILimits` lub poleceniem `choco uninstall ailimits`
uruchamia deinstalator dostarczony przez instalator. Zamyka on AI Limits i
usuwa, bez łączenia się z siecią: folder instalacji, `%APPDATA%\AiLimits`
(ustawienia, pamięć podręczna, logi), każdy wpis w Credential Manager
utworzony przez aplikację (klucze, tokeny, sesja Cursor), wartość autostartu
oraz rejestrację powiadomień.

Usunięcie kopii zainstalowanej przez Scoop lub kopii przenośnej nie usuwa
niczego z powyższych: `scoop uninstall ailimits` albo skasowanie folderu
usuwa tylko pliki samej aplikacji. Przed jej usunięciem zamknij AI Limits i
uruchom w jej folderze (polecenie, które nie ma czego usuwać, nic nie robi):

```
ailimits-auth remove claude
ailimits-auth remove copilot
ailimits-auth remove-usage-token claude
ailimits-auth remove-usage-token codex
ailimits-auth logout cursor
```

Następnie usuń `%APPDATA%\AiLimits` oraz klucz rejestru
`HKCU\Software\Classes\AppUserModelId\napxlexn.AILimits` (nazwa i ikona
wyświetlane w powiadomieniach aplikacji). Kopia z Microsoft Store: jak
najpierw usunąć jej klucze, opisano w [docs/en/CONFIG.md](docs/en/CONFIG.md#secrets) (po angielsku);
jej odinstalowanie usuwa także jej własny folder ustawień (folder kopii ze
Store opisano na początku tego pliku).

## Szybki start

1. **Uruchom.** Przy pierwszym uruchomieniu otwiera się okno powitalne (to
   samo co *Settings…* w menu). Jego pytanie wymienia narzędzia znalezione na
   tym komputerze (Claude Code, Codex CLI, Copilot CLI lub GitHub CLI,
   Antigravity, Grok CLI, Cursor), każde z polem wyboru, oraz podaje każdy
   plik, który zostałby odczytany, i miejsce, do którego trafi każdy token.
   Narzędzia wykryte po ich własnym folderze są od razu zaznaczone; sam
   GitHub CLI (dla dostawcy Copilot) lub sam folder `.gemini` (dla dostawcy
   Antigravity) jest wymieniony bez zaznaczenia. *Allow* uruchamia
   zaznaczone; *Cancel* niczego nie odczytuje i pozwala zaznaczyć to, co
   potrzebne, a każde narzędzie zapyta raz. Okno nie przejmuje fokusu
   klawiatury (naciśnięcie Enter nie udzieli przypadkiem zgody) i czeka,
   dopóki na pierwszym planie jest aplikacja pełnoekranowa. Podobnie jak
   *Settings* pozostaje otwarte, dopóki nie zamknie go Esc, jego krzyżyk lub
   Alt+F4, i można je przesuwać za pasek tytułu. Cursor wymaga jednorazowo
   własnego logowania aplikacji: w tym samym oknie rozwiń wiersz *Sign-in*
   pod dostawcą Cursor i kliknij *Sign in to Cursor…*.
   Aktualizacja z wersji 0.7.0 nie wymaga żadnych działań: dotychczasowi
   dostawcy działają dalej, a jedno okno wymienia, co jest odczytywane.
2. **Ustaw położenie.** Przeciągnij widżet lewym przyciskiem myszy.
   Przytrzymaj **Shift** podczas przeciągania, aby dosunąć go do najbliższej
   krawędzi obszaru roboczego monitora (ekranu bez paska zadań), gdy znajdzie
   się w odległości do 64 px od tej krawędzi.
   `Lock position` w menu prawego przycisku przypina go w miejscu.
3. **Dostosuj wygląd.** Prawy przycisk otwiera menu z poziomem szczegółowości
   (Compact, Medium, Expanded), układem (Rows lub Columns), paletą
   (monochromatyczna lub 8 kolorów), kryciem, jasnością i nasyceniem.
   Animacje powyżej pokazują każdą z tych opcji. Zatrzymanie wskaźnika na
   jednej z tych pozycji, a także na Width, Column layout lub Usage forecast,
   pokazuje jej efekt na widżecie, panelu i ikonie w zasobniku jeszcze przed
   kliknięciem; tak samo działa przejście do niej klawiaturą, a wyjście z menu
   bez kliknięcia przywraca poprzedni stan.
   *Settings…*
   pozwala wybrać wiersze wyświetlane przez widżet i podpowiedź panelu (w tym
   dodatkowe, takie jak tydzień Opus dostawcy Claude), opcjonalną etykietę z
   używanym modelem (model, wersja, poziom rozumowania) oraz źródło logowania
   każdego dostawcy. *Model source* określa, skąd etykieta bierze model:
   *Tool settings* albo *Last session*, czyli model z najnowszej sesji
   Claude Code lub Codex CLI, odczytany z końca jej pliku (najwyżej ostatni
   1 MB); pozostałe narzędzia pokazują model ustawiony w ich konfiguracji.
   Kolumna Taskbar wybiera dwóch dostawców pokazywanych przez panel i ikonę w
   zasobniku; co najmniej jeden pozostaje zaznaczony. Percentage i Time to
   reset zaznacza się osobno dla podpowiedzi panelu (kolumna Tooltip) i dla
   samego panelu (kolumna Taskbar): procent, czas do resetu limitu albo oba;
   ostatniej włączonej opcji nie można odznaczyć. Progress bar (domyślnie
   włączony) pozwala pominąć pasek w panelu, aby zajmował mniej miejsca na
   pasku zadań.
4. **Najedź kursorem na wiersz dostawcy**, aby zamiast odliczania do resetu
   zobaczyć jego limit tygodniowy (w Compact w dolnej linii).
   Wiersz wyszarzony lub oznaczony `≈` pokazuje zamiast tego przyczynę i
   sposób naprawy, na przykład `token expired — run Claude Code`. Przyczyna
   jest widoczna w całości w wierszach Medium i Expanded oraz w dolnej linii
   Compact (przy szerokości 100% i 75%), a pod kolumną Expanded jest
   skracana, aby się zmieściła; kolumny Compact i Medium oraz Compact przy
   50% jej nie pokazują.
5. **Widżet przeszkadza?** Kliknij prawym przyciskiem, wybierz **Taskbar**, a
   potem *Taskbar panel* lub *Tray icon*. Gdy widżet jest ukryty lub
   zasłonięty innymi oknami, użycie pozostaje widoczne na pasku zadań; gdy
   ekran zajmuje aplikacja pełnoekranowa, wskaźnik chowa się razem z paskiem
   zadań i wraca po jej opuszczeniu. Zobacz
   [Wskaźnik na pasku zadań](#wskaźnik-na-pasku-zadań).
6. **Kliknij lewym przyciskiem ikonę w zasobniku lub panel**, aby wysunąć
   widżet na wierzch albo go ukryć. Prawy przycisk na dowolnym z nich otwiera
   to samo menu.
7. **Komunikat zamiast wierszy?** *Setup required* lub
   *No data yet*: kliknij widżet, aby otworzyć *Settings*.
   *Right-click → set up providers*: dostawcy są zaznaczeni, ale żaden z nich
   nie jest zalogowany; sprawdź *Sign-in* każdego dostawcy w *Settings*.

## Jak działa autoryzacja

Widżet nigdy nie prosi o zalogowanie, z wyjątkiem jednorazowego logowania w
usłudze Cursor. Monitoruje limity **subskrypcji**, wykorzystując tokeny, które
oficjalne narzędzia CLI przechowują na tym komputerze, wyłącznie do odczytu i
dopiero po wyrażeniu zgody na każde z nich:

| Dostawca | Źródło automatyczne (domyślne) | Jak je włączyć |
|---|---|---|
| Claude | `%USERPROFILE%\.claude\.credentials.json` (token OAuth Claude Code) | uruchom [Claude Code](https://claude.com/claude-code) co najmniej raz |
| Codex | `%USERPROFILE%\.codex\auth.json` (token Codex CLI; w `CODEX_HOME`, gdy ta zmienna jest ustawiona) | wykonaj raz `codex login` |
| Copilot | `gh auth token --hostname github.com` (GitHub CLI) | wykonaj raz `gh auth login` |
| Antigravity | Windows Credential Manager, wpis `gemini:antigravity` | uruchom `agy`, czyli Antigravity CLI; jego token jest ważny około godziny i odnawia go tylko `agy` (otwarcie edytora go nie odnawia) |
| Grok | `%USERPROFILE%\.grok\auth.json` (logowanie Grok CLI; w `GROK_HOME`, gdy ta zmienna jest ustawiona) | uruchom `grok` i zaloguj się raz (subskrypcja SuperGrok) |
| Cursor | własna sesja widżetu w Windows Credential Manager (`cursor_session`); nic z edytora Cursor nie jest do tego odczytywane | Zaloguj się raz w usłudze Cursor w *Settings* (*Sign-in* dostawcy Cursor, *Sign in to Cursor…*) lub poleceniem `ailimits-auth login cursor`: w przeglądarce otwiera się strona logowania samego Cursor; sesja na ~60 dni |

Pod wierszami każdego dostawcy na karcie *Providers* okno *Settings* ma wiersz
*Sign-in*, który informuje, skąd pochodzi logowanie i z jakim skutkiem
(„Claude Code · connected”). Rozwiń go, aby wybrać źródło: konto narzędzia z
tabeli powyżej (*Claude Code account* itd.) albo, w przypadku Claude,
Codex i Copilot, własny klucz lub token (zobacz Ręczna autoryzacja
poniżej). Używane jest tylko wybrane źródło; gdy zawiedzie, żadne inne go nie
zastępuje.

O każde narzędzie aplikacja pyta raz. Odznaczenie wszystkich wierszy dostawcy
(Widget, Tooltip i Taskbar) wstrzymuje go: żadne jego dane nie są odczytywane
ani wysyłane, a zaznaczenie ponownie go uruchamia bez pytania. Aby cofnąć
zgodę, rozwiń jego *Sign-in* i kliknij *Forget permission* (widoczne, gdy
źródłem jest własne logowanie narzędzia): jego logowanie nie jest już
odczytywane, jego wiersze zostają odznaczone, a kolejne zaznaczenie znów
wywoła pytanie. Sesję Cursor kończy *Sign out*, wklejony klucz lub token usuwa
*Remove*. Odczyty na potrzeby etykiety modelu mają własne *Forget
permission* na karcie *Display*, pod Model, Version i Reasoning effort.
Zgody są przechowywane osobno dla każdego użytkownika Windows, w jego
`config.toml`.

Codex CLI zapisuje `auth.json` tylko wtedy, gdy przechowuje logowanie w pliku:
przy `cli_auth_credentials_store = "keyring"` lub `"auto"` w jego `config.toml`
pliku `auth.json` nie ma i jego logowania nie da się odczytać. Ustaw tę opcję
na `"file"` albo zamiast tego wklej token użycia.

Są to tokeny dostępu OAuth, krótkotrwałe z wyjątkiem tokenu Cursor. Narzędzie
CLI, do którego należy dany token, odświeża go przy każdym jego użyciu. Widżet
nigdy nie odświeża tokenu innej aplikacji, ponieważ odświeżenie OAuth rotuje
token i wylogowałoby to narzędzie CLI. Sesja Cursor (około 60 dni) należy do
widżetu: logowanie odbywa się w przeglądarce, na stronie samego Cursor, a
widżet sam przechowuje i odświeża tę sesję. Cursor nie oferuje logowania dla
innych aplikacji, więc widżet loguje się jako klient edytora Cursor; nigdy nie
odczytuje logowania edytora. `CODEX_HOME`, `GROK_HOME` i `COPILOT_HOME` są
uwzględniane tylko wtedy, gdy zawierają ścieżki bezwzględne.

### Jak długo dane pozostają dokładne bez żadnych działań

| Dostawca | Dane na żywo przez | Potem |
|---|---|---|
| **Claude** | ~8 godzin od ostatniej sesji Claude Code (rzeczywisty czas życia tokenu) | pokazuje ostatnią wartość wyszarzoną, z informacją o jej wieku |
| **Codex** | czas korzystania z Codex CLI (czas życia jego tokenu nie został jeszcze zmierzony) | ostatnia wartość wyszarzona, z informacją o jej wieku |
| **Copilot** | Bez ograniczeń. gh utrzymuje świeży token, dopóki logowanie jest aktywne. | |
| **Antigravity** | około godziny od ostatniego uruchomienia `agy` (tylko `agy` odnawia jego token) | ostatnia wartość wyszarzona, z informacją o jej wieku |
| **Grok** | do ~6 godzin od ostatniej sesji Grok CLI (rzeczywisty czas życia tokenu) | ostatnia wartość wyszarzona, z informacją o jej wieku |
| **Cursor** | około 60 dni od zalogowania (czas życia sesji) | ostatnia wartość wyszarzona oraz `sign in to Cursor again` |

18 czerwca 2026 r. Google przestał obsługiwać zapytania Gemini CLI od
użytkowników indywidualnych oraz użytkowników Google AI Pro i Google AI Ultra.
Ci użytkownicy mają przejść na Antigravity CLI; widżet odczytuje tylko token
Antigravity z Windows Credential Manager, a nie odczytuje już starszego pliku
Gemini CLI. Google zawiesił niektóre konta za używanie logowania Antigravity
poza Antigravity. Widżet jedynie odczytuje stan limitu, raz na aktualizację,
ale ryzyko spoczywa na użytkowniku: informuje o tym pytanie o zgodę na
odczyt logowania Antigravity.

**Claude ma po stronie serwera limit częstotliwości odczytu.** Anthropic
ogranicza liczbę zapytań do punktu końcowego użycia (HTTP 429,
`Retry-After: 0`, sprawdzone 2026-07-09), a budżet na token jest współdzielony
z odpytywaniem prowadzonym przez sam Claude Code. Gdy Claude Code działa,
odświeżenie danych Claude może trwać od 2 do 3 minut nawet przy interwale
1-minutowym. Następny cykl pyta ponownie po upływie interwału aktualizacji;
oczekiwanie wydłuża się (2, 4, a potem najwyżej 5 minut przy interwale
1-minutowym) tylko wtedy, gdy w danym cyklu żaden dostawca nie dostarczył
danych, a ograniczenie liczby zapytań liczy się jako brak danych. W tym czasie
po najechaniu kursorem na wiersz Claude widać `rate-limited, retrying`.
Pozostałych dostawców to nie dotyczy.

Wygasły token Claude, Codex, Antigravity, Grok lub Cursor nie powoduje błędnych liczb.
Ostatnia wartość pozostaje wyszarzona z informacją o jej wieku, okno, które na
pewno się zresetowało, pokazuje `≈0%`, a odliczanie do resetu biegnie dalej
lokalnie. Przy codziennym korzystaniu z narzędzi CLI dane są aktualne niemal
przez cały czas.

### Ręczna autoryzacja (dla niezależności lub konfiguracji bez CLI)

Dla konfiguracji bez narzędzi CLI albo gdy potrzebne są dane uwierzytelniające
niezależne od aktywności CLI. Co naprawdę daje każda z opcji:

| Dostawca | Opcja ręczna | Skąd ją wziąć | Pokazuje to samo co subskrypcja? |
|---|---|---|---|
| **Copilot** | Fine-grained Personal Access Token; nie potrzebuje żadnych uprawnień (pozostaw je puste) | github.com/settings/personal-access-tokens, *Generate new token* | Sprawdzone na żywo 2026-11-21: token bez uprawnień odczytuje te same limity co logowanie GitHub CLI. Nie wymaga gh aż do ustawionej daty wygaśnięcia. |
| **Claude** | Klucz API | console.anthropic.com, Settings, API Keys, *Create Key* (`sk-ant-…`) | Nie. Pokazuje limity zapytań konta API, a nie wykorzystanie subskrypcji Pro lub Max. |
| **Codex** | Brak | Nie istnieje osobny stały token. Wklejony *Usage token* to ten sam rodzaj tokenu OAuth co token Codex CLI. | |
| **Antigravity** | Brak | Limit jest powiązany z tokenem OAuth odświeżanym przez Antigravity CLI i żaden osobny klucz nie zwraca tych samych liczb. | |
| **Grok** | Brak | Tygodniowy limit SuperGrok można odczytać tylko przez własne logowanie Grok CLI. | |
| **Cursor** | Własne logowanie, około 60 dni | *Settings*, *Sign-in* dostawcy Cursor, *Sign in to Cursor…* albo `ailimits-auth login cursor` | Tak. To wykorzystanie samej subskrypcji. |

**Dwa sposoby dodania danych uwierzytelniających:**

1. **W *Settings*.** Skopiuj klucz lub token. Rozwiń wiersz *Sign-in*
   dostawcy, wybierz źródło (Claude: *Usage token* lub *API key*;
   Codex: *Usage token*; Copilot: *Fine-grained token*) i kliknij
   *Paste token from clipboard*
   (*Paste key from clipboard* w przypadku klucza API Claude). Przyjmowany
   jest tylko sekret tego rodzaju (`sk-ant-...`; `ghp_...`, `github_pat_...`,
   `gho_...`, `ghu_...` lub klasyczny 40-znakowy token szesnastkowy; token
   ChatGPT `eyJ...`): wszystko inne w schowku jest odrzucane bez wysyłania i
   nigdy nie jest wyświetlane ani zapisywane w logach. Token użycia jest
   najpierw sprawdzany jednym zapytaniem: odrzucony nie zostaje zapisany.
   Klucz jest zapisywany od razu, a wiersz pokazuje, czy działa. Okno
   informuje o wyniku (powiadomieniem, jeśli w międzyczasie zostało
   zamknięte), a sekret trafia od razu do Credential Manager. Używane jest
   tylko wybrane źródło: gdy wybrany jest własny token lub klucz, logowanie
   narzędzia nie jest odczytywane, a gdy wybrane jest konto narzędzia,
   zapisany token nie jest wysyłany.
2. **Z terminala.** `ailimits-auth.exe` znajduje się obok widżetu w
   `%LOCALAPPDATA%\AiLimits`:
   ```
   ailimits-auth status                       # show every detected source
   ailimits-auth set copilot                  # store a PAT (hidden input)
   ailimits-auth set claude                   # store a Claude API key
   ailimits-auth set-usage-token claude|codex # store a manual usage token
   ailimits-auth remove copilot|claude
   ailimits-auth remove-usage-token claude|codex
   ailimits-auth login cursor                 # sign in to Cursor in the browser
   ailimits-auth logout cursor                # forget that session
   ```
   `set-usage-token` sprawdza token jednym zapytaniem do punktu końcowego
   użycia dostawcy: odrzucony (401/403) nie zostaje zapisany; ograniczenie
   liczby zapytań (429) liczy się jako akceptacja; przy braku jednoznacznej
   odpowiedzi pyta ponownie po 2 i 4 sekundach, a potem zapisuje token z
   adnotacją. `status` niczego nie zmienia i odczytuje logowanie innego
   narzędzia tylko tam, gdzie wolno to robić widżetowi. Działający widżet sam
   uwzględnia zmiany wprowadzone pozostałymi poleceniami, bez ponownego
   uruchamiania: zmienione ustawienie w ciągu kilku sekund, nowy lub usunięty
   klucz albo token przy następnej aktualizacji. `set` i `set-usage-token`
   ustawiają zapisany sekret jako źródło dostawcy.

**Aby wrócić do logowania narzędzia:** w *Sign-in* dostawcy wybierz konto
narzędzia (*Claude Code account*, *Codex CLI account* lub *GitHub
CLI account*). Klucz lub token pozostaje zapisany, ale nieużywany; *Remove*,
gdy wybrane jest jego źródło, usuwa go. W terminalu
`ailimits-auth remove claude|copilot` usuwa klucz i przełącza z powrotem;
`remove-usage-token` usuwa token, ale pozostawia go jako wybrane źródło, więc
wiersz pozostaje ukryty, dopóki w oknie nie wybierzesz konta narzędzia.

## Menu kontekstowe

Kliknij widżet prawym przyciskiem:

- `Settings…`: wiersze, etykiety modeli, aliasy modeli, logowanie każdego dostawcy.
- **Detail**: Compact / Medium / Expanded.
- **Layout**: Rows / Columns, a dalej Width (100 / 75 / 50%, nieaktywne poza
  trybem Rows) i Column layout (Side by side / Stacked, nieaktywne poza trybem Columns).
- **Appearance**: Palette (8 kolorów), Monochrome, Background opacity,
  Brightness, Saturation.
- **Taskbar**: Tray icon / Taskbar panel / None oraz Panel display, gdy
  pasków zadań jest więcej niż jeden.
- `Always on top` i `Lock position` (dwa niezależne przełączniki), Usage
  forecast (prognoza tempa zużycia „~czas do limitu”, domyślnie wyłączona).
- Refresh interval (1 / 5 / 15 / 30 min) i `Automatic updates` (cicha
  aktualizacja kopii z instalatora w tle, domyślnie włączona; kopie ze Store,
  ze Scoop, przenośne i skompilowane samodzielnie nie mają tej pozycji). O
  nowej wersji aplikacja informuje raz, powiadomieniem z *Install now* i
  *What's new*; pozostawiona bez reakcji wersja zainstaluje się sama, gdy
  następnym razem odejdziesz od komputera na pięć minut.
- *AI Limits 0.7.1* z logo GitHub (otwiera stronę projektu) oraz Quit.

Usage forecast pokazuje swój ~czas w wierszach Medium i Expanded oraz w
kolumnach Expanded, tylko tam, gdzie czas resetu nie jest znany, i nigdy dla
danych, które przestały się aktualizować.

### Wskaźnik na pasku zadań

Gdy nakładka jest ukryta lub zasłonięta innymi oknami, na pasku zadań
pozostaje kompaktowy odczyt użycia; gdy ekran zajmuje aplikacja
pełnoekranowa, chowa się on razem z paskiem zadań i wraca po jej opuszczeniu.
Prawy przycisk → **Taskbar**:

- **Tray icon** (domyślnie). Jedna ikona 16px: dwa koncentryczne pierścienie
  dla tych samych dwóch dostawców, których pokazuje panel (pierwszych dwóch z
  widżetu albo dwóch zaznaczonych w kolumnie Taskbar w *Settings*), bardziej
  wykorzystany na zewnątrz, oba wypełniają się zgodnie z ruchem wskazówek
  zegara od godziny 12. Monochromatyczna, rysowana w motywie systemowego
  paska zadań.
- **Taskbar panel.** Przezroczysta nakładka obok zegara. Nie ma okna ani
  ramki, tylko cyfry wielkości cyfr zegara i paski rysowane na pasku zadań.
  Jest monochromatyczny, dostosowuje się do jasnego i ciemnego motywu systemu,
  pokazuje pierwszych dwóch dostawców w kolejności z widżetu albo dwóch
  zaznaczonych w kolumnie Taskbar w *Settings*, z ich procentem (i jego
  paskiem, chyba że odznaczono tam Progress bar), czasem do resetu ich limitu
  albo jednym i drugim (wartość, która przestała się aktualizować, lub
  szacunek `≈` jest rysowana bledziej, tak jak widżet ją wyszarza), i podąża
  za paskiem zadań, gdy ten się zmienia
  (automatyczne ukrywanie, zmiana rozdzielczości, szerokość zasobnika). Po
  najechaniu kursorem pokazuje podpowiedź z wierszami zaznaczonymi w kolumnie
  Tooltip (domyślnie wszyscy dostawcy), każdy z procentem i czasem resetu.
  Jeśli zasłoni go menu Start, wyszukiwanie lub automatycznie ukrywany pasek,
  przekazuje zadanie pierścieniom w zasobniku i sam wraca. Jeśli ekran zajmie
  aplikacja pełnoekranowa, chowa się razem z paskiem zadań i wraca po
  przełączeniu się z niej klawiszami Alt+Tab. Działa z paskiem zadań przy
  dowolnej krawędzi ekranu: na pasku bocznym dwaj dostawcy układają się jeden
  pod drugim, procent nad paskiem, powyżej zasobnika. Pasek zadań bez wolnego
  miejsca między ostatnim przyciskiem aplikacji a zasobnikiem nie dostaje
  panelu; do czasu, aż pojawi się miejsce, zastępują go pierścienie w
  zasobniku, więc panel nigdy nie nachodzi na przyciski.
- **None.** Bez wskaźnika.

Jeśli system Windows wyświetla pasek zadań na więcej niż jednym ekranie, **Taskbar → Panel display**
wybiera, do którego paska zadań przypięty jest panel. Podmenu pojawia się
tylko wtedy, gdy istnieje drugi pasek zadań; jeśli wybrany ekran zostanie
później odłączony, panel wraca na główny pasek zadań. Menu jest budowane przy
starcie aplikacji, więc ekran podłączony później pojawi się w nim po ponownym
uruchomieniu. *Display 2*, *Display 3* i kolejne to pozostałe paski zadań
liczone od lewej do prawej, a nie numery ekranów nadane przez Windows.

Kliknięcie lewym przyciskiem ikony w zasobniku lub panelu wysuwa widżet na
pierwszy plan (albo ukrywa go, jeśli już tam jest, na wierzchu); kliknięcie
prawym przyciskiem **w dowolnym miejscu panelu** otwiera to samo menu.

## Konfiguracja i diagnostyka

`%APPDATA%\AiLimits\config.toml` jest tworzony automatycznie, a każda zmiana w
menu jest w nim zapisywana. Zmiany zapisane w tym pliku działają w trakcie
pracy aplikacji, w ciągu około dwóch sekund, bez ponownego uruchamiania;
*Edit config.toml…* na końcu karty *Display* w *Settings* otwiera go w
Notatniku. Wiersz, którego nie da się odczytać, jest pomijany i wskazywany w
komunikacie, a obowiązuje jego wartość domyślna. Plik objaśnia się sam: krótka
notatka przed każdym ustawieniem mówi, co ono robi, jakie przyjmuje wartości i
jaka jest wartość domyślna. Opis formatu:
[docs/en/CONFIG.md](docs/en/CONFIG.md) (po angielsku).
Dostrajanie wykraczające poza menu (rozmiary, kolory, progi): [docs/en/TUNING.md](docs/en/TUNING.md) (po angielsku).

Ustaw `AILIMITS_LOG=ailimits=debug` lub `RUST_LOG=ailimits=debug`, aby
zapisywać log diagnostyczny do `%APPDATA%\AiLimits\ailimits.log` (w aplikacji
z interfejsem graficznym stderr nie jest widoczny). Plik jest rotowany po
osiągnięciu 5 MB: poprzedni zostaje zachowany jako `ailimits.log.1`.

Awaria zostawia jeden wiersz w `%APPDATA%\AiLimits\crash.log` (czas, wersja,
wątek, miejsce w kodzie źródłowym; nigdy treść komunikatu awarii); nieudane
uruchomienie jest zgłaszane komunikatem i w tym pliku. Ponowne uruchomienie
AI Limits, gdy aplikacja już działa, pokazuje widżet i wysuwa go na wierzch.

Powiadomienie o progu pojawia się raz na okno limitu, gdy użycie osiągnie
`alert_threshold` dostawcy (domyślnie 80%, dla dostawcy Copilot 85%); ponownie
dopiero po resecie okna albo gdy użycie spadło o ponad 5 punktów poniżej progu
i znów wzrosło. `[notifications]` w konfiguracji wyłącza powiadomienia
(`enabled = false`) i ustawia najkrótszy odstęp między dwoma powiadomieniami
jednego dostawcy (`cooldown_minutes`).

Zapytania korzystają z serwera proxy ustawionego w systemie Windows
(Ustawienia, Sieć i Internet, Serwer proxy, „Użyj serwera proxy”);
pierwszeństwo mają `HTTPS_PROXY`, `HTTP_PROXY` i `ALL_PROXY`, a skrypt proxy
(PAC) nie jest obsługiwany. `ailimits-auth` uwzględnia tylko zmienne, a nie
ustawienie systemu Windows. Sieć, która inspekcjonuje TLS własnym certyfikatem
głównym, nie jest zaufana: wiersze pokazują „untrusted certificate, retrying”.

## Kompilacja ze źródeł

Wymagany jest Rust 1.90 lub nowszy z zestawem narzędzi MSVC: Visual Studio
Build Tools z pakietem roboczym C++ i Windows SDK, którego `rc.exe` osadza
ikonę, manifest i wersję (zobacz `build.rs`). Instalator jest budowany za
pomocą Inno Setup 6.7.4 z plików exe profilu `release-min`. Kopia uruchomiona
z `target\` nie aktualizuje się sama i nie ma pozycji *Automatic updates*.

```
# Rust 1.90+ (MSVC), Windows 11
cargo build --profile release-min --locked   # ~4 MB exe in target/release-min/
cargo test --locked
ISCC.exe installer\ailimits.iss              # the installer, in target/installer/
```

Dokumentacja dla programistów: [docs/en/](docs/en/) (po angielsku).

## Licencja

Copyright (C) 2026 napxlexn.

AI Limits jest wolnym oprogramowaniem objętym licencją
[GNU General Public License w wersji 3 lub późniejszej](LICENSE). Można z
niego korzystać, badać je, modyfikować i rozpowszechniać. Rozpowszechniając
zmodyfikowaną wersję, należy udostępnić odpowiadający jej kod źródłowy na tej
samej licencji, zachować informacje o prawach autorskich i licencji oraz
wyraźnie oznaczyć wprowadzone zmiany.

Licencja GPL na to oprogramowanie nie daje prawa do przedstawiania
zmodyfikowanych kompilacji jako oficjalnych wydań AI Limits. Zasady dotyczące
nazwy i logo projektu opisuje [TRADEMARKS.md](TRADEMARKS.md). Komponenty
zewnętrzne podlegają własnym licencjom; ich informacje licencyjne są dołączone
do każdej kopii w [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).
