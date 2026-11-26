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

Malé nativní plovoucí překryvné okno pro Windows 11, které zobrazuje limity
využití vašich poskytovatelů AI přímo na ploše: **Claude**, **OpenAI Codex**,
**GitHub Copilot**, **Google Antigravity**, **xAI Grok**, **Cursor**.

<p align="center"><a href="https://napxlexn.github.io/ailimits/"><img alt="Otevřít web" src="https://img.shields.io/badge/OPEN%20THE%20WEBSITE-0078d4?style=for-the-badge"></a></p>

<p align="center" class="languages"><a href="README.md">English</a> · <a href="README.uk.md">Українська</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.es.md">Español</a> · <a href="README.pt-BR.md">Português (Brasil)</a> · <a href="README.de.md">Deutsch</a> · <a href="README.fr.md">Français</a> · <a href="README.pl.md">Polski</a> · <a href="README.it.md">Italiano</a> · <a href="README.nl.md">Nederlands</a> · <b>Čeština</b></p>

Aplikace je k dispozici v [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH), podepsaná a aktualizovaná obchodem Store, a je uvedená na [AlternativeTo](https://alternativeto.net/software/ai-limits/about/) — pokud vám pomáhá, vaše kladné hodnocení tam pomůže ostatním ji najít.

<p align="center">
<a href="https://apps.microsoft.com/detail/9P2B0J1VWPWH"><img alt="Get it from Microsoft" src="https://get.microsoft.com/images/en-us%20dark.svg" height="52"></a>
</p>

<p align="center">
  <img src="docs/images/widget-detail.gif" width="380" alt="Plovoucí widget se za běhu přepíná mezi úrovněmi podrobností Compact, Medium a Expanded, s vodorovnými ukazateli průběhu">
  <br><sub>Tři úrovně podrobností: Compact, Medium, Expanded. Přepínání probíhá za běhu; volitelná předpověď podle tempa spotřeby (~čas do limitu) se zobrazuje na řádku s metaúdaji tam, kde čas obnovení není znám. Zde s vodorovnými pruhy.</sub>
</p>

<p align="center">
  <img src="docs/images/indicator.gif" width="440" alt="Indikátor na hlavním panelu: kurzor přejede z plochy na panel a pak na kroužky v oznamovací oblasti, nad každým se ve stejné výšce objeví popisek a každý režim se přepíná mezi tmavým a světlým motivem Windows">
  <br><sub>Když je widget skrytý, využití zůstává vidět na hlavním panelu. Ikona v oznamovací oblasti ukazuje jako dva kroužky tytéž dva poskytovatele jako panel: první dva ve widgetu, nebo dva, které si zvolíte. Najetí myší na panel nebo na ikonu v oznamovací oblasti zobrazí stejný seznam: řádky zaškrtnuté ve sloupci Tooltip. Obojí se řídí světlým a tmavým motivem systému.</sub>
</p>

<table>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-detail-vertical.gif" height="230" alt="Tytéž úrovně podrobností se svislými ukazateli průběhu">
  <br><sub>Tytéž tři úrovně se svislými pruhy.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-width.gif" height="230" alt="Widget se zužuje po krocích šířky; při nejužší šířce názvy poskytovatelů zmizí a jejich místo zaujmou pruhy">
  <br><sub>Tři kroky šířky. Při nejužší se názvy poskytovatelů vynechají a jejich sloupec zabere pruh, takže se nic nepřekrývá.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-opacity.gif" height="230" alt="Pozadí widgetu přechází od téměř úplně průhledného k téměř plnému, zatímco text zůstává čitelný">
  <br><sub>Neprůhlednost pozadí, od téměř průhledného po téměř plné. Kontrast textu a pruhů zůstává stejný.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-providers.gif" height="230" alt="Widget roste z jednoho řádku poskytovatele na čtyři a zpět">
  <br><sub>Karta se přizpůsobí zobrazeným řádkům: jeden na každého zapnutého poskytovatele plus další řádky, které si zvolíte.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/palette.gif" height="230" alt="Widget postupně prochází monochromatickou paletou a osmi barevnými paletami, přičemž se mění posuvníky jasu, sytosti a neprůhlednosti pozadí">
  <br><sub>Monochromatická paleta a osm barevných palet, s jasem, sytostí a neprůhledností pozadí. Vykresleno samotnou aplikací.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-arrangement.gif" height="230" alt="Rozložení se svislými pruhy se přepíná mezi poskytovateli vedle sebe a poskytovateli naskládanými do vysokého úzkého sloupce">
  <br><sub>Se svislými pruhy stojí poskytovatelé vedle sebe, nebo se skládají do sloupce.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/settings-providers.gif" width="400" alt="Okno Settings vedle widgetu: zaškrtnutí týdenního limitu Opus ve sloupci Widget hned přidá jeho řádek do widgetu, pak se otevře Sign-in poskytovatele Claude s tím, odkud přihlášení pochází, co se čte, a s Forget permission">
  <br><sub><em>Settings</em>, Providers: zaškrtnutý řádek se hned zobrazí ve widgetu; Sign-in každého poskytovatele uvádí, co se čte a kam putuje jeho token.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/settings-display.gif" width="400" alt="Karta Display okna Settings: zrušení zaškrtnutí Progress bar změní řádky widgetu na jednořádkové a widget sníží, Time to reset zmizí a vrátí se a pruh se vrátí">
  <br><sub><em>Settings</em>, Display: co který řádek ukazuje; bez pruhu je widget menší. Vykresleno samotnou aplikací.</sub>
</td>
</tr>
</table>

## Co aplikace dělá

- **Čte, nikdy nezapisuje.** Čte tokeny, které oficiální nástroje CLI už mají
  uložené ve vašem počítači. Nikdy je neobnovuje ani nerotuje, takže vás z
  těchto nástrojů nemůže odhlásit. Jedinou výjimkou je Cursor: k němu se
  aplikace přihlašuje vlastní relací a přihlášení editoru nikdy nečte ani se ho
  nedotýká. Klíče, které přidáte sami, i tato relace se ukládají ve Windows
  Credential Manager. Aplikace se připojuje pouze k vlastním koncovým bodům
  poskytovatelů (a u kopie nainstalované instalačním programem také ke GitHubu
  kvůli aktualizacím), posílá jeden požadavek na využití na poskytovatele a
  cyklus (plus několik pomocných požadavků uvedených v
  [docs/en/PROVIDERS.md](docs/en/PROVIDERS.md#requests-per-cycle) (anglicky)), řídí se
  proxy nastavenou ve Windows a neodesílá žádnou telemetrii.
- **Před čtením se zeptá.** Nic, co patří jinému nástroji, se nečte, dokud to
  nepovolíte, a každý dotaz uvádí soubor a kam se jeho token posílá. Na každý
  se ptá jen jednou a *Forget permission* povolení jedním kliknutím odebere.
  Viz [Co se čte a kdy](docs/en/CONFIG.md#what-is-read-and-when) (anglicky).
- **Minimum nastavování.** Při prvním spuštění jsou nástroje nalezené v tomto
  počítači už zaškrtnuté a jeden dotaz uvádí každý soubor s přihlášením, který
  by aplikace četla: stačí *Allow*. Potom tokeny udržuje platné vaše běžné
  používání CLI, takže už není co nastavovat. Jen Cursor vás jednou požádá o
  přihlášení, v *Settings*.
- **Nízká zátěž, měřená na běžící aplikaci.** 0.014% jednoho jádra CPU, žádné
  využití GPU (vykresluje na CPU), pracovní sada 53 MB (25 MB soukromé paměti),
  změřeno pro 0.7.1 dne 2026-11-23; žádné zvýšené rozlišení časovače
  (2026-11-21); exe o velikosti 4 MB. Měření zopakujete pomocí
  `bench/perf_audit.ps1`; úplná čísla jsou v
  [docs/en/ARCHITECTURE.md](docs/en/ARCHITECTURE.md#resource-footprint-measured-not-estimated) (anglicky).
- **Zastaralá data jsou označena jako zastaralá.** Hodnota, která se přestane
  aktualizovat, zešedne a ukazuje své stáří. Okno limitu, které se prokazatelně
  obnovilo, ukazuje `≈0%`. Aplikace nezaplňuje mezery odhady. Dotazování se
  pozastaví, když jste pryč, a ten čas se nepočítá: nic nezešedne jen proto, že
  jste si odskočili.
- **Soběstačná.** Staticky linkovaná, takže bez Visual C++ Redistributable a
  bez přibalených DLL. Instalační program nepotřebuje práva správce.

## Instalace

> **Aktualizujete z 0.7.0 (nebo 0.6.x) a cesta k vašemu uživatelskému profilu
> nebo ke složce TEMP obsahuje písmena mimo anglickou abecedu (například
> cyrilici), `&` nebo `%`?** Aktualizátor staré verze na takové cestě selhává:
> jakmile vyjde 0.7.1, může AI Limits zavírat asi půl minuty po každém
> spuštění. Nainstalujte 0.7.1 ručně: stáhněte `AiLimits-Setup-0.7.1.exe` z
> [Releases](https://github.com/napxlexn/ailimits/releases) a spusťte ho, nebo,
> jakmile bude winget nabízet 0.7.1, spusťte `winget upgrade napxlexn.AILimits`.
> Pokud se widget zavře dřív, než se k němu dostanete, klikněte na něj pravým
> tlačítkem hned po spuštění a nejdřív zrušte zaškrtnutí **Automatic updates**
> (nebo nastavte `auto_update = false` v
> `%APPDATA%\AiLimits\config.toml`). Od 0.7.1 spouští aktualizátor instalační
> program přímo, takové cesty fungují a Automatic updates můžete znovu
> zaškrtnout.

Z terminálu:

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

Nebo ji získejte z [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH):
stejný widget, podepsaný a průběžně aktualizovaný obchodem Store (za vydáním
může o několik dní zaostávat, než projde certifikací nový balíček).

Nebo stáhněte `AiLimits-Setup-<version>.exe` z
[Releases](https://github.com/napxlexn/ailimits/releases) a spusťte ho.
Práva správce nejsou potřeba (instaluje se do `%LOCALAPPDATA%\AiLimits`).
Volitelně během instalace: automatické spouštění s Windows (ve výchozím stavu
zaškrtnuté) a zástupce na ploše. winget i Chocolatey spouštějí tentýž
instalační program, takže jejich kopie se aktualizuje sama jako každá kopie z
instalačního programu a správce balíčků může uvádět starší verzi, než jaká
běží.
Ke každému vydání je přiložen také přenosný zip (bez instalačního programu).
Přenosná kopie se sama neaktualizuje: aktualizuje ji její správce balíčků, nebo
vy. Přenosný režim pro nastavení neexistuje: každá kopie, včetně přenosné, je
ukládá do `%APPDATA%\AiLimits`.

> Instalační program **není digitálně podepsaný** (Authenticode), takže Windows
> SmartScreen může zobrazit „Systém Windows ochránil váš počítač“. Klikněte na
> **Další informace → Přesto spustit**. Stažený soubor můžete ověřit podle
> otisku SHA-256 zveřejněného u každého vydání (viz níže). Od 0.7.1 nese každé
> vydání také podpis Ed25519 (`.sig`) pro instalační program i zip a od 0.7.1
> vlastní aktualizátor aplikace nic nenainstaluje bez platného podpisu.
> Aktualizátor verze 0.7.0 `.sig` ignoruje a kontroluje pouze otisk SHA-256,
> který GitHub zveřejňuje pro instalační program: jeho aktualizace na 0.7.1
> stojí na tomto otisku.

Chcete-li stažený soubor ověřit ručně, porovnejte jeho SHA-256 s otiskem
`sha256:` zobrazeným u souboru na stránce vydání; obojí se musí shodovat:

```powershell
(Get-FileHash .\AiLimits-Setup-0.7.1.exe -Algorithm SHA256).Hash
```

Chcete-li ověřit i podpis, umístěte `.sig` vedle souboru, oba pod zveřejněnými
názvy (podpis zahrnuje i název). Pak v klonu tohoto repozitáře přepnutém na
značku vydání (`git checkout v0.7.1`) spusťte příkaz `verify` nástroje
release-signer, který nepotřebuje žádný klíč (Rust, jako v
[Sestavení ze zdrojového kódu](#sestavení-ze-zdrojového-kódu)):

```powershell
cargo run --locked --release --manifest-path tools/release-signer/Cargo.toml -- verify $HOME\Downloads\AiLimits-Setup-0.7.1.exe
```

Vypíše `verified`, nebo `NOT verified` s důvodem.

Instalační program neobsahuje žádné klíče ani tokeny. Při prvním spuštění
widget nečte žádný z níže uvedených zdrojů přihlášení, dokud to nepovolíte.

### Odinstalace

Odinstalace v Nastavení Windows, Aplikace, příkazem
`winget uninstall napxlexn.AILimits` nebo příkazem `choco uninstall ailimits`
spustí odinstalační program instalátoru. Ten zavře AI Limits a offline odstraní:
instalační složku, `%APPDATA%\AiLimits` (nastavení, mezipaměť, protokoly),
každou položku v Credential Manager, kterou aplikace vytvořila (klíče, tokeny,
relaci Cursor), hodnotu automatického spouštění a registraci oznámení.

Odebrání kopie ze Scoop nebo přenosné kopie nic z toho neodstraní:
`scoop uninstall ailimits` nebo smazání složky odstraní jen vlastní soubory
aplikace. Než ji odstraníte, ukončete AI Limits a v její složce spusťte
(příkaz, který nemá co odstranit, nic neudělá):

```
ailimits-auth remove claude
ailimits-auth remove copilot
ailimits-auth remove-usage-token claude
ailimits-auth remove-usage-token codex
ailimits-auth logout cursor
```

Pak smažte `%APPDATA%\AiLimits` a klíč registru
`HKCU\Software\Classes\AppUserModelId\napxlexn.AILimits` (název a ikona, které
ukazují oznámení aplikace). Kopie z Microsoft Store: jak nejdřív odstranit její
klíče, popisuje [docs/en/CONFIG.md](docs/en/CONFIG.md#secrets) (anglicky); její
odebrání s sebou vezme i její vlastní složku s nastavením (složka kopie ze Store
je popsána na začátku tohoto souboru).

## Rychlý start

1. **Spusťte aplikaci.** Při prvním spuštění se otevře uvítací okno (stejné
   jako *Settings…* v nabídce). Jeho dotaz uvádí nástroje nalezené v tomto
   počítači (Claude Code, Codex CLI, Copilot CLI nebo GitHub CLI,
   Antigravity, Grok CLI, Cursor), každý se zaškrtávacím políčkem, a jmenuje
   každý soubor, který by aplikace četla, a kam putuje každý token.
   Nástroje nalezené podle vlastní složky jsou předem zaškrtnuté; samotné
   GitHub CLI (pro Copilot) nebo samotné `.gemini` (pro Antigravity) se
   zobrazí nezaškrtnuté.
   *Allow* spustí zaškrtnuté nástroje; *Cancel* nic nečte a nechá vás
   zaškrtnout, co chcete, přičemž se každý nástroj zeptá jen jednou. Okno
   nepřebírá fokus klávesnice (Enter tedy nic omylem nepovolí) a čeká, dokud je
   v popředí aplikace přes celou obrazovku. Stejně jako *Settings* zůstává
   otevřené, dokud ho nezavře Esc, jeho křížek nebo Alt+F4, a přesouvá se
   tažením za záhlaví. Cursor jednou potřebuje vlastní přihlášení aplikace: ve
   stejném okně otevřete řádek *Sign-in* pod Cursorem a klikněte na
   *Sign in to Cursor…*.
   Aktualizace z 0.7.0 nevyžaduje nic: poskytovatelé, které jste měli, dál
   fungují a jedno okno vypíše, co se čte.
2. **Umístěte widget.** Táhněte levým tlačítkem. Když při tažení podržíte
   **Shift**, widget se přitáhne těsně k nejbližšímu okraji pracovní oblasti
   monitoru (obrazovky bez hlavního panelu), jakmile je od tohoto okraje do
   64 px.
   `Lock position` v nabídce pravého tlačítka ho připne na místo.
3. **Nastavte vzhled.** Pravým tlačítkem zvolíte úroveň podrobností (Compact,
   Medium, Expanded), rozložení (Rows nebo Columns), paletu (monochromatickou
   nebo jednu z 8 barevných), neprůhlednost, jas a sytost. Animace výše ukazují každou z
   těchto voleb.
   Když ukazatel spočine na některé z těchto položek, nebo na Width, Column
   layout či Usage forecast, uvidíte výsledek na widgetu, panelu a ikoně v
   oznamovací oblasti ještě před kliknutím, a stejně tak při přechodu na položku
   klávesnicí; opuštění nabídky bez kliknutí vše vrátí.
   *Settings…*
   vybírá řádky, které ukazuje widget a popisek panelu (včetně dalších řádků,
   jako je týden Opus u Claude), volitelný štítek s používaným modelem (model,
   verze, úroveň uvažování) a odkud pochází přihlášení každého poskytovatele.
   *Model source* určuje, odkud se bere model ve štítku: *Tool settings*, nebo
   *Last session*, tedy model nejnovější relace Claude Code nebo Codex CLI,
   přečtený z konce jejího souboru (nejvýše posledních 1 MB); ostatní nástroje
   ukazují model tak, jak je nastaven.
   Sloupec Taskbar vybírá dva poskytovatele, které ukazuje panel a ikona v
   oznamovací oblasti; alespoň jeden zůstane zaškrtnutý. Percentage a Time to
   reset se zaškrtávají zvlášť pro popisek panelu (sloupec Tooltip) a pro panel
   (sloupec Taskbar): procenta, čas do obnovení limitu, nebo obojí; poslední
   zapnutou volbu nelze odškrtnout. Progress bar (ve výchozím stavu zapnutý)
   umí pruh panelu vynechat, aby panel zabíral méně místa na hlavním panelu.
4. **Najeďte myší na řádek poskytovatele** a uvidíte jeho týdenní limit místo
   odpočtu do obnovení (v Compact na spodním řádku).
   Zašedlý řádek nebo řádek s `≈` místo toho ukazuje příčinu a nápravu,
   například `token expired — run Claude Code`. Příčina se zobrazuje celá v
   řádcích Medium a Expanded a na spodním řádku Compact (při šířkách 100% a
   75%), pod sloupcem Expanded zkrácená tak, aby se vešla; sloupce Compact a
   Medium a Compact při 50% ji neukazují.
5. **Překáží widget?** Klikněte pravým tlačítkem, **Taskbar**, pak
   *Taskbar panel* nebo *Tray icon*. Když je widget skrytý nebo zakrytý jinými
   okny, využití zůstává vidět na hlavním panelu; dokud běží aplikace přes celou
   obrazovku, skryje se spolu s hlavním panelem a vrátí se, když aplikaci
   opustíte. Viz
   [Indikátor na hlavním panelu](#indikátor-na-hlavním-panelu).
6. **Kliknutím levým tlačítkem na ikonu v oznamovací oblasti nebo na panel**
   widget přenesete do popředí, nebo ho skryjete.
   Kliknutím pravým tlačítkem na kterýkoli z nich otevřete stejnou nabídku.
7. **Místo řádků se zobrazuje zpráva?** *Setup required* nebo
   *No data yet*: kliknutím na widget otevřete *Settings*.
   *Right-click → set up providers*: poskytovatelé jsou zaškrtnutí, ale žádný z
   nich není přihlášený; zkontrolujte *Sign-in* každého poskytovatele v
   *Settings*.

## Jak funguje autorizace

Widget vás nikdy nežádá o přihlášení, s výjimkou jediného přihlášení k Cursoru.
Sleduje limity **předplatného** tak, že znovu používá tokeny, které oficiální
nástroje CLI uchovávají ve vašem počítači, přísně jen pro čtení a teprve poté,
co každý z nich povolíte:

| Poskytovatel | Automatický zdroj (výchozí) | Jak ho zapnout |
|---|---|---|
| Claude | `%USERPROFILE%\.claude\.credentials.json` (token OAuth z Claude Code) | alespoň jednou spusťte [Claude Code](https://claude.com/claude-code) |
| Codex | `%USERPROFILE%\.codex\auth.json` (token z Codex CLI; v `CODEX_HOME`, pokud je nastavena) | jednou spusťte `codex login` |
| Copilot | `gh auth token --hostname github.com` (GitHub CLI) | jednou `gh auth login` |
| Antigravity | cíl `gemini:antigravity` ve Windows Credential Manager | spusťte `agy`, Antigravity CLI; jeho token platí asi hodinu a obnovuje ho jen `agy` (otevření editoru ne) |
| Grok | `%USERPROFILE%\.grok\auth.json` (přihlášení z Grok CLI; v `GROK_HOME`, pokud je nastavena) | spusťte `grok` a jednou se přihlaste (předplatné SuperGrok) |
| Cursor | vlastní relace widgetu ve Windows Credential Manager (`cursor_session`); z editoru Cursor se pro ni nic nečte | Jednou se přihlaste ke Cursoru v *Settings* (*Sign-in* u Cursoru, *Sign in to Cursor…*) nebo pomocí `ailimits-auth login cursor`: v prohlížeči se otevře vlastní přihlašovací stránka Cursoru; relace na ~60 dní |

Pod řádky každého poskytovatele na kartě *Providers* má *Settings* řádek
*Sign-in*, který uvádí, odkud přihlášení pochází a jak dopadlo
(„Claude Code · connected“). Otevřete ho a zvolte zdroj: účet nástroje výše
(*Claude Code account* a podobně), nebo pro Claude, Codex a Copilot vlastní
klíč či token (viz Ruční autorizace níže). Použije se jen zvolený zdroj; když
selže, žádný jiný ho nezastoupí.

Na každý nástroj se aplikace ptá jednou. Zrušení zaškrtnutí všech řádků
poskytovatele (Widget, Tooltip a Taskbar) ho pozastaví: nic z něj se nečte ani
neodesílá a zaškrtnutí ho znovu spustí bez dotazu. Chcete-li povolení odebrat,
otevřete jeho *Sign-in* a klikněte na *Forget permission* (zobrazuje se, dokud
je zdrojem vlastní přihlášení nástroje): jeho přihlášení se už nečte, jeho
řádky se odškrtnou a při dalším zaškrtnutí se aplikace zeptá znovu. Relace
Cursoru končí pomocí *Sign out*, vložený klíč nebo token odstraní *Remove*.
Čtení pro štítek modelu má vlastní *Forget permission* na kartě *Display*, pod
Model, Version a Reasoning effort. Povolení se ukládají pro každého uživatele
Windows zvlášť, v jeho `config.toml`.

Codex CLI zapisuje `auth.json`, jen když své přihlášení uchovává v souboru: s
`cli_auth_credentials_store = "keyring"` nebo `"auto"` v jeho `config.toml`
žádný `auth.json` neexistuje a jeho přihlášení nelze přečíst. Nastavte hodnotu
na `"file"`, nebo místo toho vložte usage token.

Jde o přístupové tokeny OAuth, krátkodobé, kromě tokenu Cursoru. CLI, kterému
token patří, ho obnovuje pokaždé, když ho používáte. Widget nikdy neobnovuje
token jiné aplikace, protože obnovení OAuth token rotuje a CLI by se odhlásilo.
Relace Cursoru (asi 60 dní) patří widgetu: ke Cursoru se přihlásíte v
prohlížeči, na vlastní stránce Cursoru, a widget si tuto relaci uchovává a
obnovuje sám. Cursor nemá přihlášení pro jiné aplikace, takže se widget
přihlašuje jako klient editoru Cursor; přihlášení editoru nikdy nečte.
`CODEX_HOME`, `GROK_HOME` a `COPILOT_HOME` se berou v úvahu, jen pokud jde o
absolutní cesty.

### Jak dlouho zůstávají údaje přesné bez zásahu

| Poskytovatel | Živá data vydrží | Potom |
|---|---|---|
| **Claude** | ~8 hodin po vaší poslední relaci Claude Code (skutečná životnost tokenu) | ukazuje poslední hodnotu šedě, s jejím stářím |
| **Codex** | dokud používáte Codex CLI (jak dlouho jeho token vydrží, zatím není změřeno) | poslední hodnota šedě, s jejím stářím |
| **Copilot** | Neomezeně. gh udržuje svůj token čerstvý, dokud zůstáváte přihlášeni. | |
| **Antigravity** | asi hodinu po posledním spuštění `agy` (svůj token obnovuje jen `agy`) | poslední hodnota šedě, s jejím stářím |
| **Grok** | až ~6 hodin po vaší poslední relaci Grok CLI (skutečná životnost tokenu) | poslední hodnota šedě, s jejím stářím |
| **Cursor** | asi 60 dní po přihlášení (životnost relace) | poslední hodnota šedě a `sign in to Cursor again` |

Google 18. června 2026 přestal obsluhovat požadavky Gemini CLI pro individuální
uživatele a uživatele Google AI Pro a Google AI Ultra. Od těchto uživatelů se
očekává přechod na Antigravity CLI; widget čte jen token Antigravity ve Windows
Credential Manager a už ne starší soubor Gemini CLI. Google některým účtům
pozastavil přístup za používání přihlášení Antigravity mimo Antigravity. Widget
čte jen vaši kvótu, jednou za aktualizaci, ale riziko nesete vy: dotaz, který
žádá o čtení přihlášení Antigravity, to uvádí.

**Claude má na straně serveru limit na to, jak často ho lze číst.** Anthropic
omezuje četnost dotazů na koncový bod využití (HTTP 429, `Retry-After: 0`,
ověřeno 2026-07-09) a rozpočet na token se sdílí s vlastním dotazováním
Claude Code. Když běží Claude Code, může obnovení údajů Claude trvat 2 až
3 minuty i při intervalu 1 minuta. Další cyklus se ptá znovu v intervalu
aktualizace; čekání se prodlužuje (2, 4, pak nejvýše 5 minut při intervalu
1 minuta) jen tehdy, když v cyklu žádný poskytovatel nepřinesl data, a omezení
četnosti se počítá jako žádná data. Mezitím se při najetí myší na řádek Claude
zobrazí `rate-limited, retrying`. Ostatní poskytovatelé nejsou dotčeni.

Prošlý token Claude, Codex, Antigravity, Grok nebo Cursor nevede ke špatným
číslům. Poslední hodnota zůstává šedá se svým stářím, okno, které se
prokazatelně obnovilo, ukazuje `≈0%` a odpočet do obnovení běží dál lokálně.
Pokud CLI používáte denně, jsou data téměř pořád živá.

### Ruční autorizace (pro nezávislost nebo bez CLI)

Pro prostředí bez CLI, nebo když chcete přihlašovací údaj, který nezávisí na
aktivitě CLI. Co která možnost skutečně dává:

| Poskytovatel | Ruční možnost | Kde ji získat | Ukazuje totéž co předplatné? |
|---|---|---|---|
| **Copilot** | Fine-grained Personal Access Token; nepotřebuje vůbec žádná oprávnění (nechte je prázdná) | github.com/settings/personal-access-tokens, *Generate new token* | Ověřeno naživo 2026-11-21: token bez oprávnění čte stejné kvóty jako přihlášení GitHub CLI. Nepotřebuje gh, a to až do vámi nastaveného vypršení platnosti. |
| **Claude** | Klíč API | console.anthropic.com, Settings, API Keys, *Create Key* (`sk-ant-…`) | Ne. Ukazuje limity četnosti vašeho účtu API, ne využití vašeho předplatného Pro nebo Max. |
| **Codex** | Žádná | Samostatný stabilní token neexistuje. Vložený *Usage token* je stejný druh tokenu OAuth jako ten z Codex CLI. | |
| **Antigravity** | Žádná | Kvóta je vázaná na token OAuth, který obnovuje Antigravity CLI, a žádný samostatný klíč nevrací stejná čísla. | |
| **Grok** | Žádná | Týdenní limit SuperGrok se čte jen s vlastním přihlášením Grok CLI. | |
| **Cursor** | Vlastní přihlášení, asi 60 dní | *Settings*, *Sign-in* u Cursoru, *Sign in to Cursor…*, nebo `ailimits-auth login cursor` | Ano. Jde o vlastní využití předplatného. |

**Dva způsoby, jak přidat přihlašovací údaj:**

1. **V *Settings*.** Zkopírujte klíč nebo token. Otevřete řádek *Sign-in*
   daného poskytovatele, zvolte zdroj (Claude: *Usage token* nebo *API key*;
   Codex: *Usage token*; Copilot: *Fine-grained token*) a klikněte na
   *Paste token from clipboard*
   (*Paste key from clipboard* pro klíč API Claude). Přijme se jen tento druh
   tajného údaje (`sk-ant-...`; `ghp_...`, `github_pat_...`, `gho_...`,
   `ghu_...` nebo klasický šestnáctkový token o 40 znacích; token ChatGPT
   `eyJ...`): cokoli jiného ve schránce se odmítne, aniž by se odeslalo, a nikdy
   se nezobrazí ani nezapíše do protokolu. Nejprve se usage token ověří jedním
   požadavkem: odmítnutý se neuloží. Klíč se uloží tak, jak je, a řádek ukáže,
   zda funguje. Okno oznámí výsledek (oznámením, pokud bylo okno mezitím
   zavřeno) a tajný údaj jde rovnou do Credential Manager. Použije se jen
   zvolený zdroj: přihlášení nástroje se nečte, dokud je zvolený váš token nebo
   klíč, a uložený token se neodesílá, dokud je zvolený účet nástroje.
2. **Z terminálu.** `ailimits-auth.exe` leží vedle widgetu v
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
   `set-usage-token` ověří token jedním požadavkem na koncový bod využití
   poskytovatele: odmítnutý (401/403) se neuloží; omezení četnosti (429) se
   počítá jako přijetí; bez odpovědi, která by rozhodla, se zeptá znovu po 2 a
   4 sekundách a pak token uloží s poznámkou. `status` nic nemění a přihlášení
   jiného nástroje čte jen tam, kde to smí widget. Běžící widget převezme změny
   provedené ostatními příkazy sám, bez restartu: změněné nastavení během
   několika sekund, nový nebo odstraněný klíč či token při své příští
   aktualizaci. `set` a `set-usage-token` z toho, co uloží, udělají zdroj
   poskytovatele.

**Návrat k přihlášení nástroje:** v *Sign-in* poskytovatele zvolte účet nástroje
(*Claude Code account*, *Codex CLI account* nebo *GitHub CLI account*). Váš klíč
nebo token zůstane uložený, ale nepoužívaný; *Remove*, když je zvolen jeho
zdroj, ho smaže. Z terminálu `ailimits-auth remove claude|copilot` smaže klíč a
přepne zpět; `remove-usage-token` smaže token, ale ponechá na něm zdroj, takže
řádek zůstane skrytý, dokud v okně nezvolíte účet nástroje.

## Kontextová nabídka

Klikněte pravým tlačítkem na widget:

- `Settings…`: řádky, štítky modelů, aliasy modelů, přihlášení každého poskytovatele.
- **Detail**: Compact / Medium / Expanded.
- **Layout**: Rows / Columns, dále Width (100 / 75 / 50%, šedé, pokud není
  zvoleno Rows) a Column layout (Side by side / Stacked, šedé, pokud není
  zvoleno Columns).
- **Appearance**: Palette (8 barev), Monochrome, Background opacity,
  Brightness, Saturation.
- **Taskbar**: Tray icon / Taskbar panel / None a Panel display, pokud je
  hlavních panelů víc.
- `Always on top` a `Lock position` (dva nezávislé přepínače), Usage
  forecast („~čas do limitu“ podle tempa spotřeby, ve výchozím stavu vypnuté).
- Refresh interval (1 / 5 / 15 / 30 min) a `Automatic updates` (tichá
  automatická aktualizace kopie z instalačního programu na pozadí, ve výchozím
  stavu zapnutá; kopie ze Store, ze Scoop, přenosné a vlastnoručně sestavené
  tuto položku nemají). Nová verze se ohlásí jednou, oznámením s *Install now*
  a *What's new*; pokud na ni nesáhnete, nainstaluje se sama, až budete příště
  pět minut pryč.
- *AI Limits 0.7.1* se značkou GitHubu (otevře stránku projektu) a Quit.

Usage forecast ukazuje svůj ~čas v řádcích Medium a Expanded a ve sloupcích
Expanded, jen tam, kde čas obnovení není znám, a nikdy u dat, která se přestala
aktualizovat.

### Indikátor na hlavním panelu

Když je překryvné okno skryté nebo zakryté jinými okny, zůstává na hlavním
panelu kompaktní ukazatel využití; dokud běží aplikace přes celou obrazovku,
skryje se spolu s hlavním panelem a vrátí se, když aplikaci opustíte. Pravé
tlačítko → **Taskbar**:

- **Tray icon** (výchozí). Jedna ikona 16px: dva soustředné kroužky pro tytéž
  dva poskytovatele, které ukazuje panel (první dva ve widgetu, nebo dva
  zaškrtnuté ve sloupci Taskbar v *Settings*), vytíženější vně, oba se plní po
  směru hodinových ručiček od 12 hodin. Monochromatická, vykreslená v
  systémovém motivu hlavního panelu.
- **Taskbar panel.** Průhledné překrytí vedle hodin. Není tu žádné okno ani
  rámeček, jen číslice a pruhy velikosti hodin vykreslené přes hlavní panel. Je
  monochromatický, řídí se světlým a tmavým motivem systému, ukazuje první dva
  poskytovatele v pořadí widgetu, nebo dva zaškrtnuté ve sloupci Taskbar v
  *Settings*, s jejich procenty (a jejich pruhem, pokud tam není odškrtnuto
  Progress bar), časem do obnovení jejich limitu, nebo obojím (hodnota, která
  se přestala aktualizovat, nebo odhad s `≈` se kreslí tlumeněji, tak jako je
  widget zobrazuje šedě), a sleduje hlavní panel, když se pohne (automatické
  skrývání, změny rozlišení, šířka oznamovací oblasti). Po najetí myší ukáže
  popisek s řádky zaškrtnutými ve sloupci Tooltip (ve výchozím stavu každý
  poskytovatel), každý s procenty a s tím, kdy se obnoví.
  Pokud panel zakryje Start, Hledat nebo automaticky skrývaný hlavní panel,
  převezmou jeho roli kroužky v oznamovací oblasti a panel se sám vrátí. Pokud
  obrazovku zabere aplikace přes celou obrazovku, skryje se spolu s hlavním
  panelem a vrátí se, když se z ní přepnete pomocí Alt+Tab. Funguje na hlavním
  panelu u kteréhokoli okraje obrazovky: na bočním panelu se dva poskytovatelé
  skládají pod sebe, procenta nad pruhem, nad oznamovací oblastí. Hlavní panel
  bez volného úseku mezi posledním tlačítkem aplikace a oznamovací oblastí panel
  nedostane; kroužky v oznamovací oblasti ho zastupují, dokud se neuvolní místo,
  takže panel nikdy neleží na tlačítkách.
- **None.** Žádný indikátor.

Pokud Windows zobrazuje hlavní panel na více než jednom displeji,
**Taskbar → Panel display** vybere, ke kterému hlavnímu panelu se panel připojí.
Podnabídka se zobrazí jen tehdy, když existuje druhý hlavní panel; pokud se
zvolený displej později odpojí, panel se vrátí na primární hlavní panel.
Nabídka se sestavuje při spuštění aplikace, takže displej připojený později se
v ní objeví až po restartu. *Display 2*, *Display 3* a další jsou ostatní hlavní
panely počítané zleva doprava, nikoli vlastní čísla displejů ve Windows.

Kliknutí levým tlačítkem na ikonu v oznamovací oblasti nebo na panel přenese
widget do popředí (nebo ho skryje, pokud už je navrchu); kliknutí pravým
tlačítkem **kamkoli na panel** otevře tutéž nabídku.

## Konfigurace a diagnostika

`%APPDATA%\AiLimits\config.toml` se vytvoří automaticky a ukládá se do něj
každá změna v nabídce. Úpravy v něm uložené se uplatní za běhu aplikace,
přibližně do dvou sekund, bez restartu; *Edit config.toml…* na konci karty
*Display* v *Settings* ho otevře v aplikaci Notepad. Řádek, který nelze
přečíst, se vynechá a pojmenuje a použije se jeho výchozí hodnota. Soubor se
vysvětluje sám: krátká poznámka před každým nastavením říká, co dělá, jaké
hodnoty přijímá a jaká je výchozí. Popis formátu:
[docs/en/CONFIG.md](docs/en/CONFIG.md) (anglicky).
Jemné ladění nad rámec nabídky (velikosti, barvy, prahy): [docs/en/TUNING.md](docs/en/TUNING.md) (anglicky).

Nastavte `AILIMITS_LOG=ailimits=debug` nebo `RUST_LOG=ailimits=debug` a
diagnostický protokol se bude zapisovat do `%APPDATA%\AiLimits\ailimits.log`
(stderr je v aplikaci s grafickým rozhraním neviditelný). Soubor se rotuje při
5 MB: předchozí se uchová jako `ailimits.log.1`.

Pád zanechá jeden řádek v `%APPDATA%\AiLimits\crash.log` (čas, verze, vlákno,
místo ve zdrojovém kódu; nikdy text chybové zprávy); spuštění, které selhalo,
to oznámí ve zprávě a v tomto souboru. Opětovné spuštění AI Limits, když už
běží, widget zobrazí a přenese do popředí.

Oznámení o prahu přijde jednou za okno limitu, když využití dosáhne hodnoty
`alert_threshold` poskytovatele (ve výchozím stavu 80%, pro Copilot 85%); znovu
až poté, co se okno obnoví, nebo poté, co využití kleslo o více než 5 bodů pod
práh a znovu vystoupalo. `[notifications]` v konfiguraci oznámení vypne
(`enabled = false`) a nastaví nejkratší rozestup mezi dvěma oznámeními jednoho
poskytovatele (`cooldown_minutes`).

Požadavky se řídí proxy nastavenou ve Windows (Nastavení, Síť, Proxy, „Použít
proxy server“); přednost mají `HTTPS_PROXY`, `HTTP_PROXY` a `ALL_PROXY`, skript
proxy (PAC) se nepoužívá. `ailimits-auth` se řídí jen proměnnými, ne nastavením
Windows. Síti, která kontroluje TLS vlastním kořenovým certifikátem, se
nedůvěřuje: v řádcích stojí „untrusted certificate, retrying“.

## Sestavení ze zdrojového kódu

Vyžaduje Rust 1.90 nebo novější s toolchainem MSVC: Visual Studio Build Tools s
úlohou C++ a Windows SDK, jehož `rc.exe` vkládá ikonu, manifest a verzi (viz
`build.rs`). Instalační program se sestavuje pomocí Inno Setup 6.7.4 ze souborů
exe profilu `release-min`. Kopie spuštěná z `target\` se sama neaktualizuje a
nemá položku *Automatic updates*.

```
# Rust 1.90+ (MSVC), Windows 11
cargo build --profile release-min --locked   # ~4 MB exe in target/release-min/
cargo test --locked
ISCC.exe installer\ailimits.iss              # the installer, in target/installer/
```

Dokumentace pro vývojáře: [docs/en/](docs/en/) (anglicky).

## Licence

Copyright (C) 2026 napxlexn.

AI Limits je svobodný software šířený pod licencí
[GNU General Public License, verze 3 nebo novější](LICENSE). Smíte ho
používat, studovat, upravovat a dále šířit. Pokud šíříte upravenou verzi,
musíte poskytnout její odpovídající zdrojový kód pod stejnou licencí, zachovat
upozornění na autorská práva a licenci a své změny zřetelně označit.

Licence GPL k softwaru neuděluje práva prezentovat upravená sestavení jako
oficiální vydání AI Limits. Pravidla pro název projektu a logo najdete v
[TRADEMARKS.md](TRADEMARKS.md). Komponenty třetích stran nadále podléhají svým
vlastním licencím; jejich upozornění jsou součástí každé kopie v
[THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).
