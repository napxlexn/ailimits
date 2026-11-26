# AI Limits

<p align="center">
<a href="https://napxlexn.github.io/ailimits/"><img alt="Plattform" src="https://img.shields.io/badge/platform-Windows%2011-0078d4?style=flat-square"></a>
<a href="https://www.rust-lang.org/"><img alt="Entwickelt mit" src="https://img.shields.io/badge/built%20with-Rust-dea584?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/blob/master/LICENSE"><img alt="Lizenz" src="https://img.shields.io/github/license/napxlexn/ailimits?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/napxlexn/ailimits/total?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/napxlexn/ailimits/ci.yml?branch=master&style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/napxlexn/ailimits?style=flat-square"></a>
<a href="https://github.com/microsoft/winget-pkgs/tree/master/manifests/n/napxlexn/AILimits"><img alt="winget" src="https://img.shields.io/winget/v/napxlexn.AILimits?style=flat-square"></a>
<a href="https://community.chocolatey.org/packages/ailimits"><img alt="Chocolatey" src="https://img.shields.io/chocolatey/v/ailimits?style=flat-square"></a>
<a href="https://github.com/napxlexn/scoop-ailimits"><img alt="Scoop" src="https://img.shields.io/scoop/v/ailimits?bucket=https%3A%2F%2Fgithub.com%2Fnapxlexn%2Fscoop-ailimits&style=flat-square"></a>
<a href="https://apps.microsoft.com/detail/9P2B0J1VWPWH"><img alt="Microsoft Store" src="https://img.shields.io/badge/dynamic/regex?url=https%3A%2F%2Fdisplaycatalog.mp.microsoft.com%2Fv7.0%2Fproducts%3FbigIds%3D9P2B0J1VWPWH%26market%3DUS%26languages%3Den-us&search=AILimits_(%5Cd%2B%5C.%5Cd%2B%5C.%5Cd%2B)&replace=v%241&label=Microsoft%20Store&style=flat-square"></a>
</p>

Ein winziges, natives, schwebendes Overlay für Windows 11, das die
Nutzungslimits Ihrer KI-Anbieter direkt auf dem Desktop anzeigt: **Claude**,
**OpenAI Codex**, **GitHub Copilot**, **Google Antigravity**, **xAI Grok**,
**Cursor**.

<p align="center"><a href="https://napxlexn.github.io/ailimits/"><img alt="Website öffnen" src="https://img.shields.io/badge/OPEN%20THE%20WEBSITE-0078d4?style=for-the-badge"></a></p>

<p align="center" class="languages"><a href="README.md">English</a> · <a href="README.uk.md">Українська</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.es.md">Español</a> · <a href="README.pt-BR.md">Português (Brasil)</a> · <b>Deutsch</b> · <a href="README.fr.md">Français</a> · <a href="README.pl.md">Polski</a> · <a href="README.it.md">Italiano</a> · <a href="README.nl.md">Nederlands</a> · <a href="README.cs.md">Čeština</a></p>

Erhältlich im [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH), signiert und vom Store aktualisiert, und gelistet auf [AlternativeTo](https://alternativeto.net/software/ai-limits/about/) – wenn Ihnen die App hilft, hilft ein Like dort anderen, sie zu finden.

<p align="center">
<a href="https://apps.microsoft.com/detail/9P2B0J1VWPWH"><img alt="Bei Microsoft herunterladen" src="https://get.microsoft.com/images/en-us%20dark.svg" height="52"></a>
</p>

<p align="center">
  <img src="docs/images/widget-detail.gif" width="380" alt="Das schwebende Widget wechselt live zwischen den Detailstufen Compact, Medium und Expanded, mit horizontalen Fortschrittsbalken">
  <br><sub>Drei Detailstufen: Compact, Medium, Expanded. Der Wechsel erfolgt live; die optionale Verbrauchsprognose (~Zeit bis zum Limit) erscheint in der Metazeile, wenn kein Zeitpunkt für das Zurücksetzen bekannt ist. Hier mit horizontalen Balken.</sub>
</p>

<p align="center">
  <img src="docs/images/indicator.gif" width="440" alt="Die Anzeige in der Taskleiste: Der Mauszeiger bewegt sich vom Desktop auf das Panel und dann auf die Ringe im Infobereich, der Tooltip erscheint beim Darüberfahren über jedem auf gleicher Höhe, und jeder Modus wechselt zwischen dem dunklen und dem hellen Windows-Design">
  <br><sub>Ist das Widget ausgeblendet, bleibt die Nutzung in der Taskleiste sichtbar. Das Tray-Symbol zeigt als zwei Ringe dieselben zwei Anbieter wie das Panel: die ersten beiden im Widget oder die zwei, die Sie auswählen. Wenn Sie auf das Panel oder das Tray-Symbol zeigen, erscheint dieselbe Liste: die in der Spalte Tooltip angehakten Zeilen. Beide folgen dem hellen und dunklen Systemdesign.</sub>
</p>

<table>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-detail-vertical.gif" height="230" alt="Dieselben Detailstufen mit vertikalen Fortschrittsbalken">
  <br><sub>Dieselben drei Stufen mit vertikalen Balken.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-width.gif" height="230" alt="Das Widget wird über seine Breitenstufen schmaler; in der schmalsten entfallen die Anbieternamen, und die Balken nehmen ihren Platz ein">
  <br><sub>Drei Breitenstufen. In der schmalsten entfallen die Anbieternamen, und der Balken übernimmt ihre Spalte, sodass sich nichts überlappt.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-opacity.gif" height="230" alt="Der Hintergrund des Widgets geht von fast vollständig transparent zu fast deckend über, während der Text lesbar bleibt">
  <br><sub>Deckkraft des Hintergrunds, von nahezu durchsichtig bis nahezu deckend. Der Kontrast von Text und Balken bleibt unverändert.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-providers.gif" height="230" alt="Das Widget wächst von einer Anbieterzeile auf vier und wieder zurück">
  <br><sub>Die Karte richtet ihre Größe nach den angezeigten Zeilen: eine pro aktiviertem Anbieter, plus alle zusätzlichen Zeilen, die Sie wählen.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/palette.gif" height="230" alt="Das Widget durchläuft die monochrome und die acht farbigen Paletten, während die Regler für Helligkeit, Sättigung und Deckkraft des Hintergrunds verändert werden">
  <br><sub>Monochrom und acht Farbpaletten, mit Helligkeit, Sättigung und Deckkraft des Hintergrunds. Von der App selbst gerendert.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-arrangement.gif" height="230" alt="Das Layout mit vertikalen Balken wechselt zwischen Anbietern nebeneinander und Anbietern, die zu einer hohen, schmalen Spalte gestapelt sind">
  <br><sub>Mit vertikalen Balken stehen die Anbieter nebeneinander oder stapeln sich zu einer Spalte.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/settings-providers.gif" width="400" alt="Das Fenster Settings neben dem Widget: Das Anhaken des wöchentlichen Opus-Limits in der Spalte Widget fügt dessen Zeile sofort dem Widget hinzu, danach öffnet sich das Sign-in von Claude und zeigt, woher die Anmeldung stammt, was gelesen wird, und Forget permission">
  <br><sub><em>Settings</em>, Providers: Eine angehakte Zeile erscheint sofort im Widget; das Sign-in jedes Anbieters nennt, was gelesen wird und wohin sein Token geht.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/settings-display.gif" width="400" alt="Der Tab Display des Fensters Settings: Das Abwählen von Progress bar macht die Zeilen des Widgets einzeilig und das Widget niedriger, Time to reset verschwindet und kommt zurück, und der Balken kommt zurück">
  <br><sub><em>Settings</em>, Display: was jede Zeile zeigt; ohne den Balken wird das Widget kleiner. Von der App selbst gezeichnet.</sub>
</td>
</tr>
</table>

## Was die App macht

- **Liest nur, schreibt nie.** Die App liest die Tokens, die die offiziellen
  CLI-Tools bereits auf Ihrem Rechner speichern. Sie erneuert oder rotiert sie
  nie und kann Sie daher nicht von diesen Tools abmelden. Die einzige Ausnahme
  ist Cursor: Dort meldet sich die App mit einer eigenen Sitzung an und liest
  oder berührt die Anmeldung des Editors nie. Schlüssel, die Sie selbst
  hinzufügen, und diese Sitzung werden in der
  Windows-Anmeldeinformationsverwaltung gespeichert. Die App verbindet sich
  nur mit den eigenen Endpunkten der Anbieter (und, bei einer per Installer
  installierten Kopie, für Updates mit GitHub), sendet pro Anbieter und Zyklus
  eine Nutzungsanfrage (plus die wenigen Hilfsanfragen, die in
  [docs/en/PROVIDERS.md](docs/en/PROVIDERS.md#requests-per-cycle) (auf Englisch)
  aufgeführt sind), folgt dem in Windows eingestellten Proxy und sendet keine
  Telemetrie.
- **Fragt, bevor sie liest.** Nichts, was zu einem anderen Tool gehört, wird
  gelesen, bevor Sie es erlauben, und jede Frage nennt die Datei und wohin
  ihr Token gesendet wird. Jede Frage wird einmal gestellt, und *Forget
  permission* nimmt die Erlaubnis mit einem Klick zurück. Siehe
  [Was gelesen wird und wann](docs/en/CONFIG.md#what-is-read-and-when) (auf Englisch).
- **Kaum Einrichtung.** Beim ersten Start sind die auf diesem PC gefundenen
  Tools bereits angehakt, und eine einzige Frage nennt jede Anmeldedatei, die
  gelesen würde: *Allow* genügt. Danach hält Ihre normale Nutzung der CLIs
  deren Tokens gültig, sodass nichts weiter zu konfigurieren ist. Nur Cursor
  verlangt eine einmalige Anmeldung, in *Settings*.
- **Geringe Last, gemessen an der laufenden App.** 0.014% eines CPU-Kerns,
  keine GPU-Nutzung (die App rendert auf der CPU), 53 MB Arbeitssatz (25 MB
  privat), gemessen für 0.7.1 am 2026-11-23; keine erhöhte Timer-Auflösung
  (2026-11-21); eine 4 MB große exe. Nachmessen können Sie mit
  `bench/perf_audit.ps1`; die vollständigen Zahlen stehen in
  [docs/en/ARCHITECTURE.md](docs/en/ARCHITECTURE.md#resource-footprint-measured-not-estimated) (auf Englisch).
- **Veraltete Daten werden als veraltet markiert.** Ein Wert, der nicht mehr
  aktualisiert wird, wird ausgegraut und zeigt sein Alter. Ein Limitfenster,
  das nachweislich zurückgesetzt wurde, zeigt `≈0%`. Die App füllt Lücken
  nicht mit Schätzungen. Die Abfragen pausieren, während Sie abwesend sind,
  und diese Zeit zählt nicht: Nichts wird ausgegraut, nur weil Sie kurz weg
  waren.
- **Eigenständig.** Statisch gelinkt, daher kein Visual C++ Redistributable
  und keine mitgelieferten DLLs. Der Installer benötigt keine
  Administratorrechte.

## Installation

> **Sie aktualisieren von 0.7.0 (oder 0.6.x), und der Pfad Ihres
> Benutzerprofils oder Ihres TEMP-Ordners enthält Buchstaben außerhalb des
> englischen Alphabets (zum Beispiel kyrillische), `&` oder `%`?** Der Updater
> der alten Version scheitert an einem solchen Pfad: Sobald 0.7.1 erschienen
> ist, kann er AI Limits etwa eine halbe Minute nach jedem Start schließen.
> Installieren Sie 0.7.1 von Hand: Laden Sie `AiLimits-Setup-0.7.1.exe` unter
> [Releases](https://github.com/napxlexn/ailimits/releases) herunter und
> führen Sie die Datei aus, oder führen Sie, sobald winget 0.7.1 anbietet,
> `winget upgrade napxlexn.AILimits` aus. Schließt sich das Widget, bevor Sie
> dazu kommen, klicken Sie direkt nach einem Start mit der rechten Maustaste
> darauf und wählen Sie zuerst **Automatic updates** ab (oder setzen Sie
> `auto_update = false` in `%APPDATA%\AiLimits\config.toml`). Ab 0.7.1
> startet der Updater den Installer direkt, solche Pfade funktionieren, und
> Sie können Automatic updates wieder anhaken.

Vom Terminal aus:

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

Oder holen Sie sich die App aus dem [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH):
dasselbe Widget, signiert und vom Store aktuell gehalten (es kann einem
Release einige Tage hinterherhinken, während ein neues Paket zertifiziert
wird).

Oder laden Sie `AiLimits-Setup-<version>.exe` unter
[Releases](https://github.com/napxlexn/ailimits/releases) herunter und führen
Sie die Datei aus. Keine Administratorrechte erforderlich (installiert nach
`%LOCALAPPDATA%\AiLimits`). Optional bei der Einrichtung: Autostart mit
Windows (standardmäßig angehakt), eine Desktopverknüpfung. winget und
Chocolatey führen genau diesen Installer aus, daher aktualisiert sich ihre
Kopie selbst wie jede per Installer installierte Kopie, und der Paketmanager
zeigt möglicherweise eine ältere Version an als die laufende.
Jedem Release liegt außerdem ein portables ZIP-Archiv (ohne Installer) bei.
Eine portable Kopie aktualisiert sich nicht selbst: Das übernimmt ihr
Paketmanager oder Sie. Einen portablen Modus für die Einstellungen gibt es
nicht: Jede Kopie, auch die portable, speichert sie in
`%APPDATA%\AiLimits`.

> Der Installer ist **nicht codesigniert** (Authenticode), daher zeigt Windows
> SmartScreen möglicherweise „Der Computer wurde durch Windows geschützt“ an.
> Klicken Sie auf **Weitere Informationen → Trotzdem ausführen**. Sie können
> den Download anhand des SHA-256 prüfen, der bei jedem Release veröffentlicht
> wird (siehe unten). Ab 0.7.1 enthält jedes Release außerdem eine
> Ed25519-Signatur (`.sig`) für den Installer und das ZIP-Archiv, und ab 0.7.1
> installiert der eigene Updater der App nichts ohne gültige Signatur. Der
> Updater von 0.7.0 ignoriert die `.sig` und prüft nur den SHA-256, den GitHub
> für den Installer veröffentlicht: Sein Update auf 0.7.1 stützt sich auf
> diesen Digest.

Um einen Download von Hand zu prüfen, vergleichen Sie seinen SHA-256 mit dem
`sha256:`-Digest, der auf der Release-Seite neben der Datei angezeigt wird;
beide müssen übereinstimmen:

```powershell
(Get-FileHash .\AiLimits-Setup-0.7.1.exe -Algorithm SHA256).Hash
```

Um zusätzlich die Signatur zu prüfen, legen Sie die `.sig` neben die Datei,
beide unter ihren veröffentlichten Namen (die Signatur deckt den Namen ab).
Führen Sie dann in einem Klon dieses Repositorys, der auf das Release-Tag
umgeschaltet ist (`git checkout v0.7.1`), das `verify` des Release-Signers
aus, das keinen Schlüssel benötigt (Rust, wie unter
[Aus dem Quellcode bauen](#aus-dem-quellcode-bauen)):

```powershell
cargo run --locked --release --manifest-path tools/release-signer/Cargo.toml -- verify $HOME\Downloads\AiLimits-Setup-0.7.1.exe
```

Es gibt `verified` aus, oder `NOT verified` mit dem Grund.

Der Installer enthält keine Schlüssel oder Tokens. Beim ersten Start liest das
Widget keine der unten aufgeführten Authentifizierungsquellen, bis Sie es
erlauben.

### Deinstallation

Die Deinstallation in den Windows-Einstellungen unter Apps, mit
`winget uninstall napxlexn.AILimits` oder mit `choco uninstall ailimits` führt
das Deinstallationsprogramm des Installers aus. Es schließt AI Limits und
entfernt offline: den Installationsordner, `%APPDATA%\AiLimits`
(Einstellungen, Cache, Protokolle), jeden Eintrag, den die App in der
Anmeldeinformationsverwaltung angelegt hat (Schlüssel, Tokens, die
Cursor-Sitzung), den Autostart-Wert und die Registrierung für
Benachrichtigungen.

Das Entfernen einer Scoop- oder portablen Kopie räumt nichts davon auf:
`scoop uninstall ailimits` oder das Löschen des Ordners entfernt nur die
eigenen Dateien der App. Beenden Sie AI Limits, bevor Sie die Kopie
entfernen, und führen Sie in ihrem Ordner Folgendes aus (ein Befehl, der
nichts zu entfernen findet, tut nichts):

```
ailimits-auth remove claude
ailimits-auth remove copilot
ailimits-auth remove-usage-token claude
ailimits-auth remove-usage-token codex
ailimits-auth logout cursor
```

Löschen Sie danach `%APPDATA%\AiLimits` und den Registrierungsschlüssel
`HKCU\Software\Classes\AppUserModelId\napxlexn.AILimits` (Name und Symbol, die
die Benachrichtigungen der App anzeigen). Eine Kopie aus dem Microsoft Store:
Wie Sie zuerst ihre Schlüssel entfernen, steht in
[docs/en/CONFIG.md](docs/en/CONFIG.md#secrets) (auf Englisch); beim Entfernen
wird ihr eigener Einstellungsordner mitgelöscht (der Ordner der Store-Kopie
ist am Anfang dieser Datei beschrieben).

## Schnellstart

1. **Starten Sie die App.** Beim ersten Start öffnet sich ein
   Willkommensfenster (dasselbe wie *Settings…* im Menü). Seine Frage listet
   die auf diesem PC gefundenen Tools auf (Claude Code, Codex CLI, die Copilot
   CLI oder GitHub CLI, Antigravity, die Grok CLI, Cursor), jedes mit einem
   Häkchen, und nennt jede Datei, die gelesen würde, und wohin jedes Token
   geht. Tools, die anhand eines eigenen Ordners gefunden wurden, sind von
   Anfang an angehakt; GitHub CLI allein (für Copilot) oder `.gemini` allein
   (für Antigravity) wird ohne Häkchen aufgeführt.
   *Allow* startet die angehakten; *Cancel* liest nichts und lässt Sie
   anhaken, was Sie möchten, wobei jedes Tool einmal fragt. Das Fenster
   übernimmt keinen Tastaturfokus (Enter erlaubt nichts versehentlich) und
   wartet, solange eine Vollbild-App im Vordergrund ist. Wie *Settings* bleibt
   es geöffnet, bis Esc, sein Schließen-Kreuz oder Alt+F4 es schließt, und
   lässt sich an seiner Titelleiste verschieben. Cursor benötigt einmalig eine
   eigene Anmeldung der App: Öffnen Sie im selben Fenster die Zeile *Sign-in*
   unter Cursor und klicken Sie auf *Sign in to Cursor…*.
   Ein Update von 0.7.0 erfordert nichts weiter: Die bisherigen Anbieter
   funktionieren weiter, und ein Fenster listet auf, was gelesen wird.
2. **Platzieren Sie das Widget.** Ziehen Sie es mit der linken Maustaste.
   Halten Sie beim Ziehen **Shift** gedrückt, um es bündig an den
   nächstgelegenen Rand des Arbeitsbereichs des Monitors (des Bildschirms ohne
   die Taskleiste) zu ziehen, sobald es weniger als 64 px von diesem Rand
   entfernt ist. `Lock position` im Kontextmenü fixiert es.
3. **Legen Sie das Aussehen fest.** Per Rechtsklick wählen Sie die Detailstufe
   (Compact, Medium, Expanded), das Layout (Rows oder Columns), die Palette
   (monochrom oder 8 Farben), Deckkraft, Helligkeit und Sättigung. Die
   Animationen oben zeigen jede dieser Optionen. Ruht der Mauszeiger auf einem
   dieser Einträge oder auf Width, Column layout oder Usage forecast, zeigen
   das Widget, das Panel und das Tray-Symbol die Einstellung schon vor dem
   Klick, ebenso, wenn Sie den Eintrag mit der Tastatur ansteuern; verlassen
   Sie das Menü ohne Klick, wird der vorherige Zustand wiederhergestellt.
   *Settings…*
   wählt die Zeilen, die das Widget und der Tooltip des Panels zeigen
   (einschließlich zusätzlicher Zeilen wie Claudes Opus-Woche), eine optionale
   Beschriftung mit dem verwendeten Modell (Modell, Version,
   Reasoning-Aufwand) und woher die Anmeldung jedes Anbieters stammt. *Model
   source* legt fest, woher das Modell der Beschriftung stammt: *Tool
   settings* oder *Last session*, das Modell der neuesten Sitzung von Claude
   Code oder Codex CLI, gelesen vom Ende ihrer Datei (höchstens das letzte
   1 MB); andere Tools zeigen das Modell so, wie es eingestellt ist.
   Die Spalte Taskbar wählt die zwei Anbieter, die das Panel und das
   Tray-Symbol zeigen; mindestens einer bleibt angehakt. Percentage und Time
   to reset werden für den Tooltip des Panels (Spalte Tooltip) und für das
   Panel (Spalte Taskbar) getrennt angehakt: der Prozentwert, die Zeit bis zum
   Zurücksetzen des Limits oder beides; die letzte noch aktive Option lässt
   sich nicht abwählen. Progress bar (standardmäßig an) kann den Balken des
   Panels weglassen, damit es weniger Platz in der Taskleiste beansprucht.
4. **Zeigen Sie mit der Maus auf eine Anbieterzeile**, um anstelle des
   Countdowns bis zum Zurücksetzen ihr Wochenlimit zu sehen (in Compact in der
   unteren Zeile).
   Eine ausgegraute oder mit `≈` markierte Zeile zeigt stattdessen den Grund
   und die Abhilfe, zum Beispiel `token expired — run Claude Code`. Der Grund
   erscheint vollständig in Zeilen bei Medium und Expanded und in der unteren
   Zeile von Compact (bei den Breiten 100% und 75%), gekürzt unter einer
   Spalte bei Expanded; Spalten bei Compact und Medium sowie Compact bei 50%
   zeigen keinen.
5. **Ist das Widget im Weg?** Rechtsklick, **Taskbar**, dann *Taskbar panel*
   oder *Tray icon*. Ist das Widget ausgeblendet oder von anderen Fenstern
   verdeckt, bleibt die Nutzung in der Taskleiste sichtbar; solange eine
   Vollbild-App den Bildschirm belegt, verschwindet die Anzeige mit der
   Taskleiste und kehrt zurück, wenn Sie die App verlassen. Siehe
   [Anzeige in der Taskleiste](#anzeige-in-der-taskleiste).
6. **Klicken Sie mit der linken Maustaste auf das Tray-Symbol oder das
   Panel**, um das Widget in den Vordergrund zu holen oder auszublenden.
   Ein Rechtsklick auf eines der beiden öffnet dasselbe Menü.
7. **Eine Meldung statt der Zeilen?** *Setup required* oder
   *No data yet*: Klicken Sie auf das Widget, um *Settings* zu öffnen.
   *Right-click → set up providers*: Anbieter sind angehakt, aber keiner davon
   ist angemeldet; prüfen Sie das *Sign-in* jedes Anbieters in *Settings*.

## So funktioniert die Autorisierung

Das Widget fordert Sie nie zur Anmeldung auf, außer einmal für Cursor. Es
überwacht **Abonnement**-Limits, indem es die Tokens wiederverwendet, die die
offiziellen CLI-Tools auf Ihrem Rechner ablegen, ausschließlich lesend und
erst, nachdem Sie es jeweils erlaubt haben:

| Anbieter | Automatische Quelle (Standard) | So aktivieren Sie sie |
|---|---|---|
| Claude | `%USERPROFILE%\.claude\.credentials.json` (OAuth-Token von Claude Code) | [Claude Code](https://claude.com/claude-code) mindestens einmal ausführen |
| Codex | `%USERPROFILE%\.codex\auth.json` (Token der Codex CLI; in `CODEX_HOME`, wenn gesetzt) | einmal `codex login` ausführen |
| Copilot | `gh auth token --hostname github.com` (GitHub CLI) | einmal `gh auth login` |
| Antigravity | Ziel `gemini:antigravity` in der Windows-Anmeldeinformationsverwaltung | `agy` ausführen, die Antigravity CLI; ihr Token gilt etwa eine Stunde, und nur `agy` erneuert es (das Öffnen des Editors nicht) |
| Grok | `%USERPROFILE%\.grok\auth.json` (Anmeldung der Grok CLI; in `GROK_HOME`, wenn gesetzt) | `grok` ausführen und sich einmal anmelden (ein SuperGrok-Abonnement) |
| Cursor | die eigene Sitzung des Widgets, in der Windows-Anmeldeinformationsverwaltung (`cursor_session`); vom Cursor-Editor wird dafür nichts gelesen | Einmal in *Settings* bei Cursor anmelden (*Sign-in* von Cursor, *Sign in to Cursor…*) oder mit `ailimits-auth login cursor`: Cursors eigene Anmeldeseite öffnet sich im Browser; Sitzung von ~60 Tagen |

Unter den Zeilen jedes Anbieters auf dem Tab *Providers* enthält *Settings*
eine Zeile *Sign-in*, die angibt, woher die Anmeldung stammt und wie sie
verlaufen ist („Claude Code · connected“). Öffnen Sie sie, um die Quelle zu
wählen: das oben genannte Konto des Tools (*Claude Code account* usw.) oder,
für Claude, Codex und Copilot, einen eigenen Schlüssel oder ein eigenes Token
(siehe Manuelle Autorisierung weiter unten). Nur die gewählte Quelle wird
verwendet; fällt sie aus, springt keine andere ein.

Jedes Tool wird einmal gefragt. Wenn Sie alle Zeilen eines Anbieters abwählen
(Widget, Tooltip und Taskbar), pausiert er: Nichts von ihm wird gelesen oder
gesendet, und ein Häkchen startet ihn ohne erneute Frage wieder. Um die
Erlaubnis zurückzunehmen, öffnen Sie sein *Sign-in* und klicken Sie auf
*Forget permission* (angezeigt, solange die eigene Anmeldung des Tools die
Quelle ist): Seine Anmeldung wird nicht mehr gelesen, seine Zeilen werden
abgewählt, und beim nächsten Häkchen wird erneut gefragt. Die Cursor-Sitzung
endet mit *Sign out*, ein eingefügter Schlüssel oder ein eingefügtes Token
verschwindet mit *Remove*. Die Lesezugriffe der Modellbeschriftung haben ein
eigenes *Forget permission* auf dem Tab *Display*, unter Model, Version und
Reasoning effort. Erlaubnisse werden pro Windows-Benutzer gespeichert, in der
`config.toml` dieses Benutzers.

Die Codex CLI schreibt `auth.json` nur, wenn sie ihre Anmeldung in einer Datei
speichert: Mit `cli_auth_credentials_store = "keyring"` oder `"auto"` in ihrer
`config.toml` gibt es keine `auth.json`, und ihre Anmeldung kann nicht gelesen
werden. Setzen Sie den Wert auf `"file"`, oder fügen Sie stattdessen ein
Nutzungstoken ein.

Dies sind OAuth-Zugriffstokens, kurzlebig mit Ausnahme desjenigen von Cursor.
Die CLI, der ein Token gehört, erneuert es, wann immer Sie sie verwenden. Das
Widget erneuert nie das Token einer anderen App, denn eine OAuth-Erneuerung
rotiert es und würde die CLI abmelden. Die Cursor-Sitzung (etwa 60 Tage)
gehört dem Widget: Sie melden sich im Browser auf Cursors eigener Seite bei
Cursor an, und das Widget verwahrt und erneuert diese Sitzung selbst. Cursor
bietet keine Anmeldung für andere Apps, daher meldet sich das Widget als
Client des Cursor-Editors an; die Anmeldung des Editors liest es nie.
`CODEX_HOME`, `GROK_HOME` und `COPILOT_HOME` zählen nur, wenn sie absolute
Pfade sind.

### Wie lange die Daten ohne Ihr Zutun genau bleiben

| Anbieter | Live-Daten gelten | Danach |
|---|---|---|
| **Claude** | ~8 Stunden nach Ihrer letzten Sitzung von Claude Code (die tatsächliche Lebensdauer des Tokens) | zeigt den letzten Wert ausgegraut, mit seinem Alter |
| **Codex** | solange Sie die Codex CLI verwenden (wie lange ihr Token gilt, ist noch nicht gemessen) | der letzte Wert ausgegraut, mit seinem Alter |
| **Copilot** | Unbegrenzt. gh hält sein Token aktuell, solange Sie angemeldet bleiben. | |
| **Antigravity** | etwa eine Stunde nach dem letzten Lauf von `agy` (nur `agy` erneuert sein Token) | der letzte Wert ausgegraut, mit seinem Alter |
| **Grok** | bis zu ~6 Stunden nach Ihrer letzten Sitzung der Grok CLI (die tatsächliche Lebensdauer des Tokens) | der letzte Wert ausgegraut, mit seinem Alter |
| **Cursor** | etwa 60 Tage nach Ihrer Anmeldung (die Lebensdauer der Sitzung) | der letzte Wert ausgegraut, und `sign in to Cursor again` |

Google bedient seit dem 18. Juni 2026 keine Anfragen der Gemini CLI mehr für
Einzelnutzer sowie für Nutzer von Google AI Pro und Google AI Ultra. Von diesen
Nutzern wird erwartet, dass sie zur Antigravity CLI wechseln; das Widget liest
nur das Token von Antigravity in der Windows-Anmeldeinformationsverwaltung und
nicht mehr die Datei der alten Gemini CLI. Google hat einige Konten gesperrt,
weil die Anmeldung von Antigravity außerhalb von Antigravity verwendet wurde.
Das Widget liest nur Ihr Kontingent, einmal pro Aktualisierung, aber das
Risiko tragen Sie: Die Frage, ob die Anmeldung von Antigravity gelesen werden
darf, weist darauf hin.

**Claude begrenzt serverseitig, wie oft die Nutzung abgefragt werden kann.**
Anthropic begrenzt die Anfragerate des Nutzungsendpunkts (HTTP 429,
`Retry-After: 0`, geprüft am 2026-07-09), und das Budget pro Token teilt sich
die App mit den eigenen Abfragen von Claude Code. Während Claude Code läuft,
kann eine Aktualisierung von Claude selbst beim 1-Minuten-Intervall 2 bis 3
Minuten dauern. Der nächste Zyklus fragt im Aktualisierungsintervall erneut;
die Wartezeit wächst (2, 4, dann höchstens 5 Minuten beim 1-Minuten-Intervall)
nur, wenn in einem Zyklus kein Anbieter Daten geliefert hat, und ein Rate-Limit
zählt als keine Daten. In der Zwischenzeit erscheint beim Zeigen auf die
Claude-Zeile `rate-limited, retrying`. Andere Anbieter sind nicht betroffen.

Ein abgelaufenes Token von Claude, Codex, Antigravity, Grok oder Cursor führt nicht zu falschen Zahlen.
Der letzte Wert bleibt ausgegraut mit seinem Alter, ein Fenster, das
nachweislich zurückgesetzt wurde, zeigt `≈0%`, und der Countdown bis zum
Zurücksetzen läuft lokal weiter. Wenn Sie die CLIs täglich nutzen, sind die
Daten fast immer live.

### Manuelle Autorisierung (für Unabhängigkeit oder Setups ohne CLI)

Für Setups ohne die CLIs oder wenn Sie Zugangsdaten möchten, die nicht von der
Aktivität der CLIs abhängen. Was Ihnen jede Option tatsächlich bietet:

| Anbieter | Manuelle Option | Bezugsquelle | Zeigt dasselbe wie das Abonnement? |
|---|---|---|---|
| **Copilot** | Ein Fine-grained Personal Access Token; es benötigt überhaupt keine Berechtigungen (lassen Sie sie leer) | github.com/settings/personal-access-tokens, *Generate new token* | Live geprüft am 2026-11-21: Ein Token ohne Berechtigungen liest dieselben Kontingente wie die Anmeldung der GitHub CLI. Es braucht kein gh, bis zu dem von Ihnen gesetzten Ablaufdatum. |
| **Claude** | API-Schlüssel | console.anthropic.com, Settings, API Keys, *Create Key* (`sk-ant-…`) | Nein. Er zeigt die Rate-Limits Ihres API-Kontos, nicht die Nutzung Ihres Pro- oder Max-Abonnements. |
| **Codex** | Keine | Es gibt kein separates, dauerhaftes Token. Ein eingefügtes *Usage token* ist dieselbe Art von OAuth-Token wie das der Codex CLI. | |
| **Antigravity** | Keine | Das Kontingent ist an das OAuth-Token gebunden, das die Antigravity CLI erneuert, und kein separater Schlüssel meldet dieselben Zahlen. | |
| **Grok** | Keine | Das wöchentliche SuperGrok-Limit wird nur mit der eigenen Anmeldung der Grok CLI gelesen. | |
| **Cursor** | Eigene Anmeldung, etwa 60 Tage | *Settings*, *Sign-in* von Cursor, *Sign in to Cursor…*, oder `ailimits-auth login cursor` | Ja. Es ist die Nutzung des Abonnements selbst. |

**Zwei Wege, Zugangsdaten hinzuzufügen:**

1. **In *Settings*.** Kopieren Sie den Schlüssel oder das Token. Öffnen Sie
   die Zeile *Sign-in* des Anbieters, wählen Sie die Quelle (Claude: *Usage
   token* oder *API key*; Codex: *Usage token*; Copilot: *Fine-grained token*)
   und klicken Sie auf *Paste token from clipboard*
   (*Paste key from clipboard* für Claudes API-Schlüssel). Nur diese Art von
   Secret wird übernommen (`sk-ant-...`; `ghp_...`, `github_pat_...`, `gho_...`,
   `ghu_...` oder ein klassisches hexadezimales Token mit 40 Zeichen; ein
   ChatGPT-Token `eyJ...`): Alles andere in der Zwischenablage wird abgelehnt,
   ohne gesendet zu werden, und nie angezeigt oder protokolliert. Ein
   Nutzungstoken wird zuerst mit einer Anfrage geprüft: Ein abgelehntes wird
   nicht gespeichert. Ein Schlüssel wird unverändert gespeichert, und die Zeile
   zeigt, ob er funktioniert. Das Fenster meldet das Ergebnis (per
   Benachrichtigung, falls das Fenster inzwischen geschlossen wurde), und das
   Secret gelangt direkt in die Anmeldeinformationsverwaltung. Nur die
   gewählte Quelle wird verwendet: Die Anmeldung des Tools wird nicht gelesen,
   solange Ihr Token oder Schlüssel gewählt ist, und ein gespeichertes Token
   wird nicht gesendet, solange das Konto des Tools gewählt ist.
2. **Vom Terminal aus.** `ailimits-auth.exe` liegt neben dem Widget in
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
   `set-usage-token` prüft das Token mit einer Anfrage an den Nutzungsendpunkt
   des Anbieters: Ein abgelehntes (401/403) wird nicht gespeichert; ein
   Rate-Limit (429) gilt als akzeptiert; ohne eindeutige Antwort fragt der
   Befehl nach 2 und 4 Sekunden erneut und speichert das Token dann mit einem
   Hinweis. `status` ändert nichts und liest die Anmeldung eines anderen Tools
   nur dort, wo das Widget es darf. Ein laufendes Widget übernimmt von selbst,
   was die anderen Befehle ändern, ohne Neustart: eine geänderte Einstellung
   innerhalb weniger Sekunden, einen neuen oder entfernten Schlüssel oder ein
   solches Token bei seiner nächsten Aktualisierung. `set` und
   `set-usage-token` machen das, was sie speichern, zur Quelle des Anbieters.

**Zurück zur Anmeldung des Tools:** Wählen Sie im *Sign-in* des Anbieters das
Konto des Tools (*Claude Code account*, *Codex CLI account* oder *GitHub
CLI account*). Ihr Schlüssel oder Token bleibt gespeichert, wird aber nicht
verwendet; *Remove* löscht ihn, wenn die zugehörige Quelle gewählt ist. Im
Terminal löscht `ailimits-auth remove claude|copilot` den Schlüssel und
schaltet zurück; `remove-usage-token` löscht das Token, lässt die Quelle aber
darauf eingestellt, sodass die Zeile ausgeblendet bleibt, bis Sie im Fenster
das Konto des Tools wählen.

## Das Kontextmenü

Klicken Sie mit der rechten Maustaste auf das Widget:

- `Settings…`: Zeilen, Modellbeschriftungen, Modell-Aliase, die Anmeldung
  jedes Anbieters.
- **Detail**: Compact / Medium / Expanded.
- **Layout**: Rows / Columns, dann Width (100 / 75 / 50%, ausgegraut außer bei
  Rows) und Column layout (Side by side / Stacked, ausgegraut außer bei Columns).
- **Appearance**: Palette (8 Farben), Monochrome, Background opacity,
  Brightness, Saturation.
- **Taskbar**: Tray icon / Taskbar panel / None, und Panel display, wenn es
  mehr als eine Taskleiste gibt.
- `Always on top` und `Lock position` (zwei unabhängige Schalter), Usage
  forecast (Verbrauchsprognose „~time to limit“, standardmäßig aus).
- Refresh interval (1 / 5 / 15 / 30 min) und `Automatic updates` (stille
  Selbstaktualisierung einer per Installer installierten Kopie im Hintergrund,
  standardmäßig an; Kopien aus dem Store, über Scoop, portable und selbst
  gebaute Kopien haben diesen Eintrag nicht). Eine neue Version wird einmal
  angekündigt, in einer Benachrichtigung mit *Install now* und *What's new*;
  ohne Ihr Zutun installiert sie sich von selbst, sobald Sie das nächste Mal
  fünf Minuten abwesend sind.
- *AI Limits 0.7.1*, mit dem GitHub-Logo (öffnet die Projektseite), und Quit.

Usage forecast zeigt seine ~Zeit in Zeilen bei Medium und Expanded und in
Spalten bei Expanded, nur dort, wo kein Zeitpunkt für das Zurücksetzen bekannt
ist, und nie für Daten, die nicht mehr aktualisiert werden.

### Anzeige in der Taskleiste

Ist das Overlay ausgeblendet oder von anderen Fenstern verdeckt, bleibt eine
kompakte Nutzungsanzeige in der Taskleiste; solange eine Vollbild-App den
Bildschirm belegt, verschwindet sie mit der Taskleiste und kehrt zurück, wenn
Sie die App verlassen. Rechtsklick →
**Taskbar**:

- **Tray icon** (Standard). Ein einziges 16px-Symbol: zwei konzentrische Ringe
  für dieselben zwei Anbieter, die das Panel zeigt (die ersten beiden im Widget
  oder die zwei, die in der Spalte Taskbar von *Settings* angehakt sind), der
  stärker ausgelastete außen, beide füllen sich im Uhrzeigersinn ab 12 Uhr.
  Monochrom, im Design der System-Taskleiste gezeichnet.
- **Taskbar panel.** Ein transparentes Overlay neben der Uhr. Es gibt kein
  Fenster und keinen Rahmen, nur Ziffern in Uhrgröße und Balken, die über die
  Taskleiste gezeichnet werden. Es ist monochrom, folgt dem hellen und dunklen
  Systemdesign, zeigt die ersten beiden Anbieter in der Reihenfolge des Widgets
  oder die zwei, die in der Spalte Taskbar von *Settings* angehakt sind, mit
  ihrem Prozentwert (und dessen Balken, sofern Progress bar dort nicht
  abgewählt ist), der Zeit bis zum Zurücksetzen ihres Limits oder beidem (ein
  Wert, der nicht mehr aktualisiert wird, oder eine
  `≈`-Schätzung wird blasser gezeichnet, so wie das Widget ihn ausgraut), und
  folgt der Taskleiste, wenn sie sich bewegt
  (automatisches Ausblenden, Auflösungsänderungen, Breite des Infobereichs).
  Beim Daraufzeigen erscheint ein Tooltip mit den in der Spalte Tooltip
  angehakten Zeilen (standardmäßig alle Anbieter), jeweils mit Prozentwert und
  Zeitpunkt des Zurücksetzens.
  Wenn Start, die Suche oder die automatisch ausgeblendete Leiste das Panel
  verdeckt, übergibt es an die Tray-Ringe und kehrt von selbst zurück. Wenn
  eine Vollbild-App den Bildschirm übernimmt, verschwindet es mit der
  Taskleiste und kehrt zurück, sobald Sie mit Alt+Tab hinauswechseln. Es
  funktioniert mit einer Taskleiste an jedem Bildschirmrand: Bei einer
  seitlichen Leiste stapeln sich die zwei Anbieter, Prozentwert über Balken,
  oberhalb des Infobereichs. Eine Leiste ohne freien Abschnitt zwischen ihrer
  letzten App-Schaltfläche und dem Infobereich bekommt kein Panel; die
  Tray-Ringe springen ein, bis Platz frei wird, sodass das Panel nie auf den
  Schaltflächen liegt.
- **None.** Keine Anzeige.

Wenn Windows die Taskleiste auf mehr als einem Bildschirm anzeigt, wählt **Taskbar → Panel display**,
an welche Taskleiste sich das Panel anheftet. Das Untermenü erscheint nur,
wenn eine zweite Taskleiste existiert; wird der gewählte Bildschirm später
getrennt, wechselt das Panel auf die primäre Taskleiste zurück. Das Menü wird
beim Start der App aufgebaut, daher erscheint ein später angeschlossener
Bildschirm dort erst nach einem Neustart. *Display 2*, *Display 3* und
folgende sind die übrigen Taskleisten, von links nach rechts gezählt, nicht
die eigenen Bildschirmnummern von Windows.

Ein Linksklick auf das Tray-Symbol oder das Panel holt das Widget in den
Vordergrund (oder blendet es aus, wenn es bereits dort ist, ganz oben); ein
Rechtsklick **an beliebiger Stelle des Panels** öffnet dasselbe Menü.

## Konfiguration und Diagnose

`%APPDATA%\AiLimits\config.toml` wird automatisch angelegt, und jede Änderung
im Menü wird dort gespeichert. Darin gespeicherte Änderungen werden während der
Laufzeit der App innerhalb von etwa zwei Sekunden übernommen, ohne Neustart;
*Edit config.toml…* am Ende des Tabs *Display* in *Settings* öffnet die Datei
im Editor. Eine Zeile, die sich nicht lesen lässt, wird ausgelassen und
benannt, und ihr Standardwert wird verwendet. Die Datei erklärt sich selbst:
Ein kurzer Hinweis vor jeder Einstellung beschreibt, was sie bewirkt, welche
Werte sie annimmt und was ihr Standardwert ist. Formatreferenz:
[docs/en/CONFIG.md](docs/en/CONFIG.md) (auf Englisch).
Feinabstimmung über das Menü hinaus (Größen, Farben, Schwellenwerte): [docs/en/TUNING.md](docs/en/TUNING.md) (auf Englisch).

Setzen Sie `AILIMITS_LOG=ailimits=debug` oder `RUST_LOG=ailimits=debug`, um ein
Diagnoseprotokoll nach `%APPDATA%\AiLimits\ailimits.log` zu schreiben (stderr
ist in einer GUI-App unsichtbar). Die Datei wird bei 5 MB rotiert: Die
vorherige bleibt als `ailimits.log.1` erhalten.

Ein Absturz hinterlässt eine Zeile in `%APPDATA%\AiLimits\crash.log` (Zeit,
Version, Thread, Stelle im Quellcode; nie den Text der Absturzmeldung); ein
fehlgeschlagener Start meldet dies in einer Nachricht und in dieser Datei. Wird
AI Limits erneut gestartet, während es läuft, wird das Widget angezeigt und in
den Vordergrund geholt.

Eine Schwellenwert-Benachrichtigung erscheint einmal pro Limitfenster, wenn
die Nutzung den `alert_threshold` des Anbieters erreicht (standardmäßig 80%,
85% für Copilot); erneut erst, nachdem das Fenster zurückgesetzt wurde oder
nachdem die Nutzung um mehr als 5 Punkte unter den Schwellenwert gefallen und
wieder gestiegen ist. `[notifications]` in der Konfiguration schaltet die
Benachrichtigungen ab (`enabled = false`) und legt den kürzesten Abstand
zwischen zwei Benachrichtigungen desselben Anbieters fest
(`cooldown_minutes`).

Anfragen folgen dem in Windows eingestellten Proxy (Einstellungen, Netzwerk
und Internet, Proxy, „Proxyserver verwenden“); `HTTPS_PROXY`, `HTTP_PROXY` und `ALL_PROXY` haben Vorrang, ein Proxyskript (PAC) wird
nicht befolgt. `ailimits-auth` folgt nur den Variablen, nicht der
Windows-Einstellung. Einem Netzwerk, das TLS mit einem eigenen Stammzertifikat
untersucht, wird nicht vertraut: Die Zeilen zeigen „untrusted certificate,
retrying“.

## Aus dem Quellcode bauen

Erfordert Rust 1.90 oder neuer mit der MSVC-Toolchain: Visual Studio Build
Tools mit der C++-Workload und dem Windows SDK, dessen `rc.exe` das Symbol,
das Manifest und die Version einbettet (siehe `build.rs`). Der Installer wird
mit Inno Setup 6.7.4 aus den exe-Dateien von `release-min` gebaut. Eine aus
`target\` gestartete Kopie aktualisiert sich nicht selbst und hat keinen
Eintrag *Automatic updates*.

```
# Rust 1.90+ (MSVC), Windows 11
cargo build --profile release-min --locked   # ~4 MB exe in target/release-min/
cargo test --locked
ISCC.exe installer\ailimits.iss              # the installer, in target/installer/
```

Entwicklerdokumentation: [docs/en/](docs/en/) (auf Englisch).

## Lizenz

Copyright (C) 2026 napxlexn.

AI Limits ist freie Software, lizenziert unter der
[GNU General Public License, Version 3 oder später](LICENSE). Sie dürfen sie
verwenden, untersuchen, verändern und weitergeben. Wenn Sie eine veränderte
Version verbreiten, müssen Sie den zugehörigen Quellcode unter derselben Lizenz
bereitstellen, die Urheberrechts- und Lizenzhinweise beibehalten und Ihre
Änderungen deutlich kennzeichnen.

Die GPL-Lizenz der Software gewährt keine Rechte, veränderte Builds als
offizielle Releases von AI Limits auszugeben. Die Regeln für den Projektnamen
und das Logo finden Sie in [TRADEMARKS.md](TRADEMARKS.md). Komponenten von
Drittanbietern unterliegen weiterhin ihren jeweiligen Lizenzen; ihre Hinweise
liegen jeder Kopie in [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt) bei.
