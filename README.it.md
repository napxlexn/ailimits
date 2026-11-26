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

Un piccolo overlay nativo e flottante per Windows 11 che mostra i limiti di
utilizzo dei provider di IA direttamente sul desktop: **Claude**,
**OpenAI Codex**, **GitHub Copilot**, **Google Antigravity**, **xAI Grok**,
**Cursor**.

<p align="center"><a href="https://napxlexn.github.io/ailimits/"><img alt="Apri il sito web" src="https://img.shields.io/badge/OPEN%20THE%20WEBSITE-0078d4?style=for-the-badge"></a></p>

<p align="center" class="languages"><a href="README.md">English</a> · <a href="README.uk.md">Українська</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.es.md">Español</a> · <a href="README.pt-BR.md">Português (Brasil)</a> · <a href="README.de.md">Deutsch</a> · <a href="README.fr.md">Français</a> · <a href="README.pl.md">Polski</a> · <b>Italiano</b> · <a href="README.nl.md">Nederlands</a> · <a href="README.cs.md">Čeština</a></p>

Disponibile su [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH), firmato e aggiornato dallo Store, e presente su [AlternativeTo](https://alternativeto.net/software/ai-limits/about/) — se ti è utile, un like lì aiuta altre persone a trovarlo.

<p align="center">
<a href="https://apps.microsoft.com/detail/9P2B0J1VWPWH"><img alt="Get it from Microsoft" src="https://get.microsoft.com/images/en-us%20dark.svg" height="52"></a>
</p>

<p align="center">
  <img src="docs/images/widget-detail.gif" width="380" alt="Il widget flottante che passa in tempo reale tra i livelli di dettaglio Compact, Medium ed Expanded, con barre di avanzamento orizzontali">
  <br><sub>Tre livelli di dettaglio: Compact, Medium, Expanded. Il passaggio avviene in tempo reale; la previsione facoltativa del ritmo di consumo (~tempo al limite) compare nella riga delle informazioni aggiuntive quando l'orario di reset non è noto. Qui con barre orizzontali.</sub>
</p>

<p align="center">
  <img src="docs/images/indicator.gif" width="440" alt="L'indicatore sulla barra delle applicazioni: il cursore si sposta dal desktop sul pannello e poi sugli anelli nell'area di notifica, il tooltip compare sopra ciascuno alla stessa altezza e ogni modalità passa dal tema scuro al tema chiaro di Windows e viceversa">
  <br><sub>Con il widget nascosto, l'utilizzo resta visibile sulla barra delle applicazioni. L'icona nell'area di notifica mostra come due anelli gli stessi due provider del pannello: i primi due del widget, oppure i due che scegli. Passando il puntatore sul pannello o sull'icona nell'area di notifica compare lo stesso elenco: le righe spuntate nella colonna Tooltip. Entrambi seguono il tema chiaro e scuro del sistema.</sub>
</p>

<table>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-detail-vertical.gif" height="230" alt="Gli stessi livelli di dettaglio con barre di avanzamento verticali">
  <br><sub>Gli stessi tre livelli con barre verticali.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-width.gif" height="230" alt="Il widget che si restringe passando per le sue larghezze; alla più stretta i nomi dei provider scompaiono e le barre prendono il loro posto">
  <br><sub>Tre larghezze. Alla più stretta i nomi dei provider vengono omessi e la barra occupa la loro colonna, così nulla si sovrappone.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-opacity.gif" height="230" alt="Lo sfondo del widget che passa da quasi completamente trasparente a quasi opaco mentre il testo resta leggibile">
  <br><sub>Opacità dello sfondo, da quasi trasparente a quasi opaca. Il contrasto di testo e barre non cambia.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-providers.gif" height="230" alt="Il widget che cresce da una sola riga di provider a quattro e poi torna indietro">
  <br><sub>La scheda si adatta alle righe mostrate: una per ogni provider attivato, più le eventuali righe aggiuntive scelte.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/palette.gif" height="230" alt="Il widget che passa in rassegna la palette monocromatica e le otto palette a colori, variando i cursori di luminosità, saturazione e opacità dello sfondo">
  <br><sub>Palette monocromatica più otto palette a colori, con luminosità, saturazione e opacità dello sfondo. Renderizzato dall'app stessa.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-arrangement.gif" height="230" alt="Il layout a barre verticali che alterna provider affiancati e provider impilati in una colonna alta e stretta">
  <br><sub>Con le barre verticali, i provider si affiancano o si impilano in una colonna.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/settings-providers.gif" width="400" alt="La finestra Settings accanto al widget: spuntando il limite settimanale di Opus nella colonna Widget la sua riga si aggiunge subito al widget, poi il Sign-in di Claude si apre mostrando da dove proviene l'accesso, cosa viene letto e Forget permission">
  <br><sub><em>Settings</em>, Providers: una riga spuntata compare subito sul widget; il Sign-in di ogni provider indica cosa viene letto e dove va il suo token.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/settings-display.gif" width="400" alt="La scheda Display della finestra Settings: togliendo la spunta a Progress bar le righe del widget diventano a linea singola e il widget si abbassa, Time to reset scompare e ricompare, e la barra torna">
  <br><sub><em>Settings</em>, Display: cosa mostra ogni riga; senza la barra il widget si rimpicciolisce. Disegnato dall'app stessa.</sub>
</td>
</tr>
</table>

## Cosa fa

- **Legge, non scrive mai.** Legge i token che gli strumenti CLI ufficiali
  già conservano sul computer. Non li rinnova né li ruota mai, quindi non può
  disconnetterti da quegli strumenti. L'unica eccezione è Cursor: l'app vi
  accede con una propria sessione e non legge né tocca mai l'accesso
  dell'editor. Le chiavi aggiunte manualmente, e quella sessione, sono
  conservate in Windows Credential Manager. L'app si connette solo agli
  endpoint dei provider stessi (e, per una copia installata con il programma
  di installazione, a GitHub per gli aggiornamenti), invia una richiesta di
  utilizzo per provider per ciclo (più le poche richieste ausiliarie elencate in
  [docs/en/PROVIDERS.md](docs/en/PROVIDERS.md#requests-per-cycle) (in inglese)),
  segue il proxy impostato in Windows e non invia telemetria.
- **Chiede prima di leggere.** Nulla di ciò che appartiene a un altro
  strumento viene letto finché non dai il consenso, e ogni domanda indica il
  file e dove viene inviato il suo token. Ogni domanda viene posta una sola
  volta e *Forget permission* revoca il consenso con un clic. Vedi
  [Cosa viene letto e quando](docs/en/CONFIG.md#what-is-read-and-when) (in inglese).
- **Configurazione minima.** Al primo avvio gli strumenti trovati sul PC sono
  già spuntati e un'unica domanda elenca ogni file di accesso che verrebbe
  letto: basta *Allow*. Dopo di che il normale uso delle CLI mantiene validi i
  loro token, quindi non c'è altro da configurare. Solo Cursor chiede di
  accedere una volta, in *Settings*.
- **Carico ridotto, misurato sull'app in esecuzione.** 0.014% di un core
  della CPU, nessun uso della GPU (il rendering avviene sulla CPU), un working
  set di 53 MB (25 MB privati), misurati per la 0.7.1 il 2026-11-23; nessun
  aumento della risoluzione del timer (2026-11-21); un exe da 4 MB. Per
  riprodurre la misura:
  `bench/perf_audit.ps1`; i dati completi sono in
  [docs/en/ARCHITECTURE.md](docs/en/ARCHITECTURE.md#resource-footprint-measured-not-estimated) (in inglese).
- **I dati non aggiornati sono segnalati come tali.** Un valore che smette di
  aggiornarsi diventa grigio e ne mostra l'età. Una finestra di limite che si
  è sicuramente azzerata mostra `≈0%`. L'app non riempie i vuoti con
  supposizioni. Il polling si sospende quando non sei al computer e quel
  tempo non conta: nulla diventa grigio solo per una breve assenza.
- **Autosufficiente.** Linkato staticamente, quindi niente Visual C++
  Redistributable né DLL incluse. Il programma di installazione non richiede
  diritti di amministratore.

## Installazione

> **Aggiorni dalla 0.7.0 (o dalla 0.6.x) e il percorso del tuo profilo utente
> o della cartella TEMP contiene lettere non inglesi (ad esempio cirilliche),
> `&` o `%`?** Il meccanismo di aggiornamento della vecchia versione non
> funziona con un percorso del genere: una volta uscita la 0.7.1, potrebbe
> chiudere AI Limits circa mezzo minuto dopo ogni avvio. Installa la 0.7.1
> manualmente: scarica `AiLimits-Setup-0.7.1.exe` da
> [Releases](https://github.com/napxlexn/ailimits/releases) ed eseguilo
> oppure, quando winget offrirà la 0.7.1, esegui `winget upgrade napxlexn.AILimits`.
> Se il widget si chiude prima che tu riesca a farlo, fai clic destro su di
> esso subito dopo un avvio e per prima cosa togli la spunta a
> **Automatic updates** (oppure imposta `auto_update = false` in
> `%APPDATA%\AiLimits\config.toml`). Dalla 0.7.1 in poi il meccanismo di
> aggiornamento avvia direttamente il programma di installazione, questi
> percorsi funzionano e puoi spuntare di nuovo Automatic updates.

Dal terminale:

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

Oppure scaricalo da [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH):
lo stesso widget, firmato e mantenuto aggiornato dallo Store (può arrivare con
qualche giorno di ritardo rispetto a una release, mentre il nuovo pacchetto
viene certificato).

Oppure scarica `AiLimits-Setup-<version>.exe` da
[Releases](https://github.com/napxlexn/ailimits/releases) ed eseguilo.
Non servono diritti di amministratore (si installa in `%LOCALAPPDATA%\AiLimits`).
Facoltativi durante l'installazione: avvio automatico con Windows (spuntato
per impostazione predefinita) e un collegamento sul desktop. winget e
Chocolatey eseguono lo stesso programma di installazione, quindi la loro copia
si aggiorna da sola come ogni copia installata con il programma di
installazione, e il gestore di pacchetti potrebbe indicare una versione
precedente a quella in esecuzione.
A ogni release è allegato anche uno zip portable (senza programma di
installazione). Una copia portable non si aggiorna da sola: la aggiorna il
suo gestore di pacchetti, oppure lo fai tu. Non esiste una modalità portable
per le impostazioni: ogni copia, portable compresa, le conserva in
`%APPDATA%\AiLimits`.

> Il programma di installazione **non ha firma del codice** (Authenticode),
> quindi Windows SmartScreen potrebbe mostrare "Windows ha protetto il PC".
> Fai clic su **Ulteriori informazioni → Esegui comunque**. Puoi verificare il
> file scaricato con lo SHA-256 pubblicato in ogni release (vedi sotto). Dalla
> 0.7.1 ogni release include anche una firma Ed25519 (`.sig`) per il
> programma di installazione e per lo zip, e dalla 0.7.1 in poi il meccanismo
> di aggiornamento dell'app non installa nulla senza una firma valida. Il
> meccanismo di aggiornamento della 0.7.0 ignora il `.sig` e controlla solo lo
> SHA-256 che GitHub pubblica per il programma di installazione: il suo
> aggiornamento alla 0.7.1 si basa su quel digest.

Per verificare manualmente un file scaricato, confronta il suo SHA-256 con il
digest `sha256:` mostrato accanto al file nella pagina della release; i due
valori devono coincidere:

```powershell
(Get-FileHash .\AiLimits-Setup-0.7.1.exe -Algorithm SHA256).Hash
```

Per verificare anche la firma, metti il file `.sig` accanto al file, entrambi
con i nomi con cui sono stati pubblicati (la firma copre il nome). Poi, in un
clone di questo repository posizionato sul tag della release
(`git checkout v0.7.1`), esegui il comando `verify` dello strumento di firma
delle release, che non richiede alcuna chiave (serve Rust, come in
[Compilazione dai sorgenti](#compilazione-dai-sorgenti)):

```powershell
cargo run --locked --release --manifest-path tools/release-signer/Cargo.toml -- verify $HOME\Downloads\AiLimits-Setup-0.7.1.exe
```

Il comando stampa `verified`, oppure `NOT verified` con il motivo.

Il programma di installazione non contiene chiavi né token. Al primo avvio il
widget non legge nessuna delle fonti di autenticazione elencate più sotto
finché non dai il consenso.

### Disinstallazione

La disinstallazione da Impostazioni di Windows, App, con
`winget uninstall napxlexn.AILimits` o con `choco uninstall ailimits` esegue
il programma di disinstallazione fornito con il programma di installazione.
Questo chiude AI Limits e rimuove, senza accedere alla rete: la cartella di
installazione, `%APPDATA%\AiLimits` (impostazioni, cache, log), ogni voce di
Credential Manager creata dall'app (chiavi, token, la sessione di Cursor), il
valore di avvio automatico e la registrazione delle notifiche.

La rimozione di una copia Scoop o portable non elimina nulla di tutto ciò:
`scoop uninstall ailimits`, o l'eliminazione della cartella, rimuove solo i
file dell'app stessa. Prima di rimuoverla, chiudi AI Limits ed esegui, nella
sua cartella (un comando che non ha nulla da rimuovere non fa nulla):

```
ailimits-auth remove claude
ailimits-auth remove copilot
ailimits-auth remove-usage-token claude
ailimits-auth remove-usage-token codex
ailimits-auth logout cursor
```

Poi elimina `%APPDATA%\AiLimits` e la chiave di registro
`HKCU\Software\Classes\AppUserModelId\napxlexn.AILimits` (il nome e l'icona
mostrati nelle notifiche dell'app). Per una copia da Microsoft Store, come
rimuoverne prima le chiavi è spiegato in [docs/en/CONFIG.md](docs/en/CONFIG.md#secrets) (in inglese);
disinstallarla rimuove anche la sua cartella delle impostazioni (la cartella
della copia dello Store è descritta all'inizio di quel file).

## Avvio rapido

1. **Avvialo.** Al primo avvio si apre una finestra di benvenuto (la stessa di
   *Settings…* nel menu). La sua domanda elenca gli strumenti trovati sul
   PC (Claude Code, Codex CLI, Copilot CLI o GitHub CLI,
   Antigravity, Grok CLI, Cursor), ciascuno con una casella di spunta, e
   indica ogni file che verrebbe letto e dove va ogni token.
   Gli strumenti trovati tramite una propria cartella partono spuntati;
   GitHub CLI da solo (per Copilot) o `.gemini` da sola (per Antigravity)
   compaiono senza spunta. *Allow* avvia quelli spuntati; *Cancel* non legge
   nulla e ti permette di spuntare ciò che vuoi, e ogni strumento chiederà una
   sola volta. La finestra non prende il focus della tastiera (Invio non
   concede nulla per errore) e attende finché in primo piano c'è un'app a
   schermo intero. Come *Settings*, resta aperta finché non la chiudono Esc,
   la sua X o Alt+F4, e si sposta trascinandone la barra del titolo. Cursor
   richiede, una sola volta, un accesso proprio dell'app: nella stessa
   finestra apri la riga *Sign-in* sotto Cursor e fai clic su
   *Sign in to Cursor…*.
   Aggiornando dalla 0.7.0 non serve fare nulla: i provider che avevi
   continuano a funzionare e un'unica finestra elenca cosa viene letto.
2. **Posizionalo.** Trascinalo con il tasto sinistro. Tieni premuto **Shift**
   durante il trascinamento per accostarlo al bordo più vicino dell'area di
   lavoro del monitor (lo schermo senza la barra delle applicazioni) quando
   si trova entro 64 px da quel bordo.
   `Lock position` nel menu del clic destro lo blocca in posizione.
3. **Scegli l'aspetto.** Con il clic destro scegli il livello di dettaglio
   (Compact, Medium, Expanded), il layout (Rows o Columns), la palette
   (monocromatica o 8 colori), l'opacità, la luminosità e la saturazione. Le
   animazioni qui sopra mostrano ciascuna opzione.
   Fermando il puntatore su una di queste voci, o su Width, Column layout o
   Usage forecast, l'effetto compare sul widget, sul pannello e sull'icona
   nell'area di notifica prima ancora del clic, e lo stesso accade
   spostandosi sulla voce con la tastiera; uscendo dal menu senza fare clic
   tutto torna com'era.
   *Settings…*
   sceglie le righe mostrate dal widget e dal tooltip del pannello (comprese
   le righe aggiuntive come la settimana Opus di Claude), un'etichetta
   facoltativa con il modello in uso (modello, versione, livello di
   ragionamento) e da dove proviene l'accesso di ogni provider.
   *Model source* sceglie da dove l'etichetta prende il modello:
   *Tool settings*, oppure *Last session*, cioè il modello della sessione più
   recente di Claude Code o Codex CLI, letto dalla fine del suo file (al
   massimo l'ultimo 1 MB); gli altri strumenti mostrano il modello così come
   è impostato.
   La colonna Taskbar sceglie i due provider mostrati dal pannello e
   dall'icona nell'area di notifica; almeno uno resta spuntato. Percentage e
   Time to reset si spuntano separatamente per il tooltip del pannello
   (colonna Tooltip) e per il pannello (colonna Taskbar): la percentuale, il
   tempo mancante al reset del limite, o entrambi; l'ultima opzione rimasta
   attiva non si può togliere. Progress bar (attiva per impostazione
   predefinita) può escludere la barra del pannello per occupare meno spazio
   sulla barra delle applicazioni.
4. **Passa il puntatore su una riga di provider** per vederne il limite
   settimanale al posto del conto alla rovescia per il reset (in Compact,
   nella riga inferiore).
   Una riga grigia o con `≈` mostra invece il motivo e la soluzione, ad
   esempio `token expired — run Claude Code`. Il motivo appare per intero
   nelle righe Medium ed Expanded e nella riga inferiore di Compact (alle
   larghezze 100% e 75%), accorciato per starci sotto una colonna Expanded;
   le colonne Compact e Medium, e Compact al 50%, non lo mostrano.
5. **Il widget è d'intralcio?** Clic destro, **Taskbar**, poi *Taskbar panel*
   o *Tray icon*. Con il widget nascosto o coperto da altre finestre,
   l'utilizzo resta sulla barra delle applicazioni; quando un'app a schermo
   intero occupa lo schermo, si nasconde insieme alla barra delle
   applicazioni e ricompare quando esci dall'app. Vedi
   [Indicatore sulla barra delle applicazioni](#indicatore-sulla-barra-delle-applicazioni).
6. **Fai clic sinistro sull'icona nell'area di notifica o sul pannello** per
   portare in primo piano il widget o nasconderlo.
   Un clic destro su uno dei due apre lo stesso menu.
7. **Un messaggio al posto delle righe?** *Setup required* o
   *No data yet*: fai clic sul widget per aprire *Settings*.
   *Right-click → set up providers*: i provider sono spuntati, ma nessuno ha
   un accesso attivo; controlla il *Sign-in* di ogni provider in *Settings*.

## Come funziona l'autorizzazione

Il widget non ti chiede mai di accedere, salvo una volta per Cursor. Monitora i
limiti degli **abbonamenti** riutilizzando i token che gli strumenti CLI
ufficiali conservano sul computer, rigorosamente in sola lettura e solo dopo
che hai dato il consenso per ciascuno:

| Provider | Fonte automatica (predefinita) | Per attivarla |
|---|---|---|
| Claude | `%USERPROFILE%\.claude\.credentials.json` (token OAuth di Claude Code) | esegui [Claude Code](https://claude.com/claude-code) almeno una volta |
| Codex | `%USERPROFILE%\.codex\auth.json` (token di Codex CLI; in `CODEX_HOME` quando è impostata) | esegui `codex login` una volta |
| Copilot | `gh auth token --hostname github.com` (GitHub CLI) | `gh auth login` una volta |
| Antigravity | Windows Credential Manager, voce `gemini:antigravity` | esegui `agy`, cioè Antigravity CLI; il suo token dura circa un'ora e solo `agy` lo rinnova (aprire l'editor non basta) |
| Grok | `%USERPROFILE%\.grok\auth.json` (accesso di Grok CLI; in `GROK_HOME` quando è impostata) | esegui `grok` e accedi una volta (abbonamento SuperGrok) |
| Cursor | la sessione propria del widget, in Windows Credential Manager (`cursor_session`); dell'editor Cursor non viene letto nulla | Accedi a Cursor una volta in *Settings* (il *Sign-in* di Cursor, *Sign in to Cursor…*) o con `ailimits-auth login cursor`: nel browser si apre la pagina di accesso di Cursor stesso; sessione di ~60 giorni |

Sotto le righe di ogni provider nella scheda *Providers*, *Settings* ha una
riga *Sign-in* che indica da dove proviene l'accesso e com'è andato
("Claude Code · connected"). Aprila per scegliere la fonte: l'account dello
strumento indicato sopra (*Claude Code account* e così via) oppure, per Claude,
Codex e Copilot, una chiave o un token tuoi (vedi Autorizzazione manuale più
sotto). Viene usata solo la fonte scelta; se non funziona, nessun'altra la
sostituisce.

Il consenso per ogni strumento viene chiesto una sola volta. Togliendo tutte le
spunte di un provider (Widget, Tooltip e Taskbar) lo si mette in pausa: non
viene letto né inviato nulla che lo riguardi, e una spunta lo riavvia senza
domande. Per revocare il consenso, apri il suo *Sign-in* e fai clic su
*Forget permission* (visibile quando la fonte è l'accesso dello strumento
stesso): il suo accesso non viene più letto, le sue righe vengono deselezionate
e la spunta successiva chiederà di nuovo. La sessione di Cursor termina con
*Sign out*, una chiave o un token incollati si rimuovono con *Remove*. Le
letture per l'etichetta del modello hanno un proprio *Forget
permission* nella scheda *Display*, sotto Model, Version e Reasoning effort.
I consensi sono conservati per singolo utente di Windows, nel `config.toml` di
quell'utente.

Codex CLI scrive `auth.json` solo quando conserva l'accesso in un file: con
`cli_auth_credentials_store = "keyring"` o `"auto"` nel suo `config.toml`
non esiste alcun `auth.json` e il suo accesso non può essere letto. Impostalo
su `"file"`, oppure incolla invece un token di utilizzo.

Sono token di accesso OAuth, a breve durata tranne quello di Cursor. La CLI a
cui appartiene ciascun token lo rinnova ogni volta che la usi. Il widget non
rinnova mai il token di un'altra app, perché un refresh OAuth lo ruota e
disconnetterebbe la CLI. La sessione di Cursor (circa 60 giorni) appartiene al
widget: accedi a Cursor nel browser, sulla pagina di Cursor stesso, e il widget
conserva e rinnova quella sessione in autonomia. Cursor non offre un accesso
per altre app, quindi il widget accede come client dell'editor Cursor; non
legge mai l'accesso dell'editor. `CODEX_HOME`, `GROK_HOME` e `COPILOT_HOME`
contano solo quando sono percorsi assoluti.

### Per quanto restano accurati i dati senza alcun intervento

| Provider | Dati in tempo reale per | Dopo |
|---|---|---|
| **Claude** | ~8 ore dall'ultima sessione di Claude Code (la durata reale del token) | mostra l'ultimo valore in grigio, con la sua età |
| **Codex** | finché usi Codex CLI (la durata del suo token non è ancora stata misurata) | l'ultimo valore in grigio, con la sua età |
| **Copilot** | Indefinitamente. gh mantiene valido il suo token finché l'accesso resta attivo. | |
| **Antigravity** | circa un'ora dall'ultima esecuzione di `agy` (solo `agy` rinnova il suo token) | l'ultimo valore in grigio, con la sua età |
| **Grok** | fino a ~6 ore dall'ultima sessione di Grok CLI (la durata reale del token) | l'ultimo valore in grigio, con la sua età |
| **Cursor** | circa 60 giorni dall'accesso (la durata della sessione) | l'ultimo valore in grigio, e `sign in to Cursor again` |

Il 18 giugno 2026 Google ha smesso di servire le richieste di Gemini CLI per
gli utenti individuali, Google AI Pro e Google AI Ultra. Questi utenti
dovrebbero passare ad Antigravity CLI; il widget legge solo il token di
Antigravity in Windows Credential Manager, e non più il vecchio file di Gemini
CLI. Google ha sospeso alcuni account per aver usato l'accesso di Antigravity
al di fuori di Antigravity. Il widget legge solo la tua quota, una volta per
aggiornamento, ma il rischio è tuo: lo dice la domanda che chiede il consenso
a leggere l'accesso di Antigravity.

**Claude limita lato server la frequenza con cui può essere letto.** Anthropic
applica un rate limit all'endpoint di utilizzo (HTTP 429, `Retry-After: 0`,
verificato il 2026-07-09) e il budget per token è condiviso con il polling di
Claude Code stesso. Mentre Claude Code è in esecuzione, un aggiornamento di
Claude può richiedere da 2 a 3 minuti anche con l'intervallo di 1 minuto. Il
ciclo successivo chiede di nuovo allo scadere dell'intervallo di aggiornamento;
l'attesa cresce (2, 4, poi al massimo 5 minuti con l'intervallo di 1 minuto)
solo quando in un ciclo nessun provider ha portato dati, e un rate limit conta
come assenza di dati. Nel frattempo, passando il puntatore sulla riga di Claude
si legge `rate-limited, retrying`. Gli altri provider non ne risentono.

Un token scaduto di Claude, Codex, Antigravity, Grok o Cursor non produce numeri sbagliati.
L'ultimo valore resta in grigio con la sua età, una finestra che si è
sicuramente azzerata mostra `≈0%` e il conto alla rovescia per il reset
continua localmente. Se usi le CLI ogni giorno, i dati sono aggiornati quasi
sempre.

### Autorizzazione manuale (per indipendenza o configurazioni senza CLI)

Per configurazioni senza le CLI, o quando vuoi una credenziale che non dipenda
dall'attività delle CLI. Cosa offre davvero ciascuna opzione:

| Provider | Opzione manuale | Dove ottenerla | Mostra gli stessi dati dell'abbonamento? |
|---|---|---|---|
| **Copilot** | Un fine-grained Personal Access Token; non richiede alcun permesso (lasciali tutti vuoti) | github.com/settings/personal-access-tokens, *Generate new token* | Verificato dal vivo il 2026-11-21: un token senza permessi legge le stesse quote dell'accesso di GitHub CLI. Non richiede gh, fino alla scadenza che imposti. |
| **Claude** | Chiave API | console.anthropic.com, Settings, API Keys, *Create Key* (`sk-ant-…`) | No. Mostra i rate limit del tuo account API, non l'utilizzo dell'abbonamento Pro o Max. |
| **Codex** | Nessuna | Non esiste un token stabile separato. Un *Usage token* incollato è lo stesso tipo di token OAuth di Codex CLI. | |
| **Antigravity** | Nessuna | La quota è legata al token OAuth che Antigravity CLI rinnova, e nessuna chiave separata restituisce gli stessi numeri. | |
| **Grok** | Nessuna | Il limite settimanale di SuperGrok si legge solo con l'accesso di Grok CLI stesso. | |
| **Cursor** | Un accesso proprio, circa 60 giorni | *Settings*, il *Sign-in* di Cursor, *Sign in to Cursor…*, oppure `ailimits-auth login cursor` | Sì. È l'utilizzo dell'abbonamento stesso. |

**Due modi per aggiungere una credenziale:**

1. **In *Settings*.** Copia la chiave o il token. Apri la riga *Sign-in* del
   provider, scegli la fonte (Claude: *Usage token* o *API key*;
   Codex: *Usage token*; Copilot: *Fine-grained token*) e fai clic su
   *Paste token from clipboard*
   (*Paste key from clipboard* per la chiave API di Claude). Viene accettato
   solo quel tipo di segreto (`sk-ant-...`; `ghp_...`, `github_pat_...`,
   `gho_...`, `ghu_...` o un token esadecimale classico di 40 caratteri; un
   token ChatGPT `eyJ...`): qualsiasi altro contenuto degli appunti viene
   rifiutato senza essere inviato, e non viene mai mostrato né registrato nei
   log. Un token di utilizzo viene prima verificato con una richiesta: se
   rifiutato, non viene salvato. Una chiave viene salvata così com'è e la riga
   indica se funziona. La finestra comunica l'esito (con una notifica, se nel
   frattempo è stata chiusa) e il segreto va direttamente in Credential
   Manager. Viene usata solo la fonte scelta: l'accesso dello strumento non
   viene letto mentre è scelto il tuo token o la tua chiave, e un token
   salvato non viene inviato mentre è scelto l'account dello strumento.
2. **Dal terminale.** `ailimits-auth.exe` si trova accanto al widget in
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
   `set-usage-token` verifica il token con una richiesta all'endpoint di
   utilizzo del provider: uno rifiutato (401/403) non viene salvato; un rate
   limit (429) conta come accettazione; in mancanza di una risposta che
   indichi l'una o l'altra cosa, riprova dopo 2 e 4 secondi, poi lo salva con
   una nota. `status` non modifica nulla e legge l'accesso di un altro
   strumento solo dove può farlo il widget. Un widget in esecuzione recepisce
   da solo ciò che cambiano gli altri comandi, senza riavvio: un'impostazione
   modificata entro pochi secondi, una chiave o un token nuovi o rimossi al
   successivo aggiornamento. `set` e `set-usage-token` rendono ciò che salvano
   la fonte del provider.

**Per tornare all'accesso dello strumento:** nel *Sign-in* del provider, scegli
l'account dello strumento (*Claude Code account*, *Codex CLI account* o *GitHub
CLI account*). La tua chiave o il tuo token restano salvati ma inutilizzati;
*Remove*, con la relativa fonte scelta, li elimina. Dal terminale,
`ailimits-auth remove claude|copilot` elimina la chiave e torna all'accesso
dello strumento; `remove-usage-token` elimina il token ma lascia la fonte
impostata su di esso, quindi la riga resta nascosta finché non scegli
l'account dello strumento nella finestra.

## Il menu contestuale

Fai clic destro sul widget:

- `Settings…`: righe, etichette dei modelli, alias dei modelli, l'accesso di ogni provider.
- **Detail**: Compact / Medium / Expanded.
- **Layout**: Rows / Columns, poi Width (100 / 75 / 50%, in grigio se non è
  attivo Rows) e Column layout (Side by side / Stacked, in grigio se non è attivo Columns).
- **Appearance**: Palette (8 colori), Monochrome, Background opacity,
  Brightness, Saturation.
- **Taskbar**: Tray icon / Taskbar panel / None, e Panel display quando c'è
  più di una barra delle applicazioni.
- `Always on top` e `Lock position` (due interruttori indipendenti), Usage
  forecast (ritmo di consumo, "~tempo al limite", disattivato per
  impostazione predefinita).
- Refresh interval (1 / 5 / 15 / 30 min) e `Automatic updates` (aggiornamento
  automatico silenzioso in background di una copia installata con il
  programma di installazione, attivo per impostazione predefinita; le copie
  dello Store, di Scoop, portable e compilate in proprio non hanno questa
  voce). Una nuova versione viene annunciata una sola volta, con una notifica
  con *Install now* e *What's new*; se la ignori, si installa da sola la
  prossima volta che ti allontani dal computer per cinque minuti.
- *AI Limits 0.7.1*, con il logo di GitHub (apre la pagina del progetto), e Quit.

Usage forecast mostra il suo ~tempo nelle righe Medium ed Expanded e nelle
colonne Expanded, solo dove l'orario di reset non è noto, e mai per dati che
hanno smesso di aggiornarsi.

### Indicatore sulla barra delle applicazioni

Con l'overlay nascosto o coperto da altre finestre, sulla barra delle
applicazioni resta una lettura compatta dell'utilizzo; quando un'app a
schermo intero occupa lo schermo, si nasconde insieme alla barra delle
applicazioni e ricompare quando esci dall'app. Clic destro → **Taskbar**:

- **Tray icon** (l'impostazione predefinita). Un'unica icona da 16px: due
  anelli concentrici per gli stessi due provider mostrati dal pannello (i
  primi due del widget, oppure i due spuntati nella colonna Taskbar di
  *Settings*), quello più carico all'esterno, entrambi si riempiono in senso
  orario a partire dalle ore 12. Monocromatica, disegnata nel tema della barra
  delle applicazioni di sistema.
- **Taskbar panel.** Un overlay trasparente accanto all'orologio. Non c'è
  alcuna finestra o riquadro, solo cifre delle dimensioni di quelle
  dell'orologio e barre disegnate sopra la barra delle applicazioni. È
  monocromatico, segue il tema chiaro e scuro del sistema, mostra i primi due
  provider nell'ordine del widget, oppure i due spuntati nella colonna Taskbar
  di *Settings*, con la loro percentuale (e la relativa barra, a meno che lì
  non sia stata tolta la spunta a Progress bar), il tempo mancante al reset
  del loro limite, o entrambi (un valore che ha smesso di aggiornarsi, o una
  stima `≈`, viene disegnato più tenue, così come il widget lo mostra in
  grigio), e segue la barra delle applicazioni quando si sposta
  (nascondimento automatico, cambi di risoluzione, larghezza dell'area di
  notifica). Passandoci sopra il puntatore compare un tooltip con le righe
  spuntate nella colonna Tooltip (tutti i provider per impostazione
  predefinita), ciascuna con la sua percentuale e il momento del reset.
  Se il menu Start, la ricerca o la barra a scomparsa automatica lo coprono,
  cede il posto agli anelli nell'area di notifica e torna da solo. Se un'app a
  schermo intero occupa lo schermo, si nasconde insieme alla barra delle
  applicazioni e ricompare quando ne esci con Alt+Tab. Funziona con la barra
  delle applicazioni su qualsiasi bordo dello schermo: su una barra laterale i
  due provider si impilano, percentuale sopra la barra, sopra l'area di
  notifica. Una barra senza spazio libero tra l'ultimo pulsante delle app e
  l'area di notifica non riceve il pannello; gli anelli nell'area di notifica
  lo sostituiscono finché non si libera spazio, quindi il pannello non si
  sovrappone mai ai pulsanti.
- **None.** Nessun indicatore.

Se Windows mostra la barra delle applicazioni su più di uno schermo, **Taskbar → Panel display**
sceglie a quale barra delle applicazioni si aggancia il pannello. Il
sottomenu compare solo quando esiste una seconda barra delle applicazioni; se
lo schermo scelto viene poi scollegato, il pannello torna sulla barra delle
applicazioni principale. Il menu viene creato all'avvio dell'app, quindi uno
schermo collegato in seguito vi compare dopo un riavvio. *Display 2*,
*Display 3* e così via sono le altre barre delle applicazioni contate da
sinistra a destra, non i numeri degli schermi assegnati da Windows.

Un clic sinistro sull'icona nell'area di notifica o sul pannello porta il
widget in primo piano (o lo nasconde se è già lì, sopra le altre finestre); un
clic destro **in qualsiasi punto del pannello** apre lo stesso menu.

## Configurazione e diagnostica

`%APPDATA%\AiLimits\config.toml` viene creato automaticamente e ogni modifica
fatta dal menu viene salvata lì. Le modifiche salvate nel file si applicano
mentre l'app è in esecuzione, entro circa due secondi, senza riavvio;
*Edit config.toml…* in fondo alla scheda *Display* di *Settings* lo apre nel
Blocco note. Una riga che non si riesce a leggere viene esclusa e segnalata, e
al suo posto vale il valore predefinito. Il file si spiega da solo: una breve
nota prima di ogni impostazione dice cosa fa, quali valori accetta e qual è il
suo valore predefinito. Riferimento del formato:
[docs/en/CONFIG.md](docs/en/CONFIG.md) (in inglese).
Regolazioni oltre il menu (dimensioni, colori, soglie): [docs/en/TUNING.md](docs/en/TUNING.md) (in inglese).

Imposta `AILIMITS_LOG=ailimits=debug` o `RUST_LOG=ailimits=debug` per
scrivere un log diagnostico in `%APPDATA%\AiLimits\ailimits.log` (in un'app con
interfaccia grafica stderr non è visibile). Il file ruota a 5 MB: il
precedente viene conservato come `ailimits.log.1`.

Un arresto anomalo lascia una riga in `%APPDATA%\AiLimits\crash.log` (ora,
versione, thread, posizione nel sorgente; mai il testo del messaggio di
errore); un avvio non riuscito lo segnala con un messaggio e in quel file.
Avviare di nuovo AI Limits mentre è in esecuzione mostra il widget e lo porta
in primo piano.

Una notifica di soglia arriva una volta per finestra di limite, quando
l'utilizzo raggiunge la soglia `alert_threshold` del provider (80% per
impostazione predefinita, 85% per Copilot); di nuovo solo dopo il reset della
finestra, o dopo che l'utilizzo è sceso di oltre 5 punti sotto la soglia ed è
risalito. `[notifications]` nella configurazione disattiva le notifiche
(`enabled = false`) e imposta l'intervallo minimo tra due notifiche dello
stesso provider (`cooldown_minutes`).

Le richieste seguono il proxy impostato in Windows (Impostazioni, Rete e
Internet, Proxy, "Usa un server proxy"); `HTTPS_PROXY`, `HTTP_PROXY` e
`ALL_PROXY` hanno la precedenza, uno script di proxy (PAC) non viene seguito.
`ailimits-auth` segue solo le variabili, non l'impostazione di Windows. Una
rete che ispeziona il TLS con un proprio certificato radice non è considerata
attendibile: le righe mostrano "untrusted certificate, retrying".

## Compilazione dai sorgenti

Richiede Rust 1.90 o successivo con la toolchain MSVC: Visual Studio Build
Tools con il carico di lavoro C++ e Windows SDK, il cui `rc.exe` incorpora
l'icona, il manifest e la versione (vedi `build.rs`). Il programma di
installazione viene creato con Inno Setup 6.7.4 a partire dagli exe
`release-min`. Una copia eseguita da `target\` non si aggiorna da sola e non
ha la voce *Automatic updates*.

```
# Rust 1.90+ (MSVC), Windows 11
cargo build --profile release-min --locked   # ~4 MB exe in target/release-min/
cargo test --locked
ISCC.exe installer\ailimits.iss              # the installer, in target/installer/
```

Documentazione per sviluppatori: [docs/en/](docs/en/) (in inglese).

## Licenza

Copyright (C) 2026 napxlexn.

AI Limits è software libero distribuito con licenza
[GNU General Public License, versione 3 o successiva](LICENSE). Puoi usarlo,
studiarlo, modificarlo e ridistribuirlo. Se distribuisci una versione
modificata, devi fornirne il codice sorgente corrispondente con la stessa
licenza, mantenere le note di copyright e di licenza e indicare chiaramente le
tue modifiche.

La licenza GPL del software non concede il diritto di presentare build
modificate come release ufficiali di AI Limits. Consulta
[TRADEMARKS.md](TRADEMARKS.md) per le regole sul nome e sul logo del progetto.
I componenti di terze parti restano soggetti alle rispettive licenze; le
relative note sono incluse in ogni copia in
[THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).
