# AI Limits

<p align="center">
<a href="https://napxlexn.github.io/ailimits/"><img alt="Plateforme" src="https://img.shields.io/badge/platform-Windows%2011-0078d4?style=flat-square"></a>
<a href="https://www.rust-lang.org/"><img alt="Développé avec" src="https://img.shields.io/badge/built%20with-Rust-dea584?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/blob/master/LICENSE"><img alt="Licence" src="https://img.shields.io/github/license/napxlexn/ailimits?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/releases"><img alt="Téléchargements" src="https://img.shields.io/github/downloads/napxlexn/ailimits/total?style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/napxlexn/ailimits/ci.yml?branch=master&style=flat-square"></a>
<a href="https://github.com/napxlexn/ailimits/releases/latest"><img alt="Version" src="https://img.shields.io/github/v/release/napxlexn/ailimits?style=flat-square"></a>
<a href="https://github.com/microsoft/winget-pkgs/tree/master/manifests/n/napxlexn/AILimits"><img alt="winget" src="https://img.shields.io/winget/v/napxlexn.AILimits?style=flat-square"></a>
<a href="https://community.chocolatey.org/packages/ailimits"><img alt="Chocolatey" src="https://img.shields.io/chocolatey/v/ailimits?style=flat-square"></a>
<a href="https://github.com/napxlexn/scoop-ailimits"><img alt="Scoop" src="https://img.shields.io/scoop/v/ailimits?bucket=https%3A%2F%2Fgithub.com%2Fnapxlexn%2Fscoop-ailimits&style=flat-square"></a>
<a href="https://apps.microsoft.com/detail/9P2B0J1VWPWH"><img alt="Microsoft Store" src="https://img.shields.io/badge/dynamic/regex?url=https%3A%2F%2Fdisplaycatalog.mp.microsoft.com%2Fv7.0%2Fproducts%3FbigIds%3D9P2B0J1VWPWH%26market%3DUS%26languages%3Den-us&search=AILimits_(%5Cd%2B%5C.%5Cd%2B%5C.%5Cd%2B)&replace=v%241&label=Microsoft%20Store&style=flat-square"></a>
</p>

Une minuscule surcouche flottante native pour Windows 11 qui affiche les
limites d'utilisation de vos fournisseurs d'IA directement sur le bureau :
**Claude**, **OpenAI Codex**, **GitHub Copilot**, **Google Antigravity**,
**xAI Grok**, **Cursor**.

<p align="center"><a href="https://napxlexn.github.io/ailimits/"><img alt="Ouvrir le site web" src="https://img.shields.io/badge/OPEN%20THE%20WEBSITE-0078d4?style=for-the-badge"></a></p>

<p align="center" class="languages"><a href="README.md">English</a> · <a href="README.uk.md">Українська</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.es.md">Español</a> · <a href="README.pt-BR.md">Português (Brasil)</a> · <a href="README.de.md">Deutsch</a> · <b>Français</b> · <a href="README.pl.md">Polski</a> · <a href="README.it.md">Italiano</a> · <a href="README.nl.md">Nederlands</a> · <a href="README.cs.md">Čeština</a></p>

Disponible sur le [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH), signée et mise à jour par le Store, et référencée sur [AlternativeTo](https://alternativeto.net/software/ai-limits/about/) — si elle vous est utile, un like sur ce site aide d'autres personnes à la trouver.

<p align="center">
<a href="https://apps.microsoft.com/detail/9P2B0J1VWPWH"><img alt="Télécharger depuis Microsoft" src="https://get.microsoft.com/images/en-us%20dark.svg" height="52"></a>
</p>

<p align="center">
  <img src="docs/images/widget-detail.gif" width="380" alt="Le widget flottant passe en direct d'un niveau de détail à l'autre, Compact, Medium et Expanded, avec des barres de progression horizontales">
  <br><sub>Trois niveaux de détail : Compact, Medium, Expanded. Le changement s'applique en direct ; la prévision facultative du rythme de consommation (~temps avant la limite) apparaît sur la ligne méta lorsqu'aucune heure de réinitialisation n'est connue. Ici avec des barres horizontales.</sub>
</p>

<p align="center">
  <img src="docs/images/indicator.gif" width="440" alt="L'indicateur de la barre des tâches : le curseur passe du bureau au panneau, puis aux anneaux de la zone de notification, l'info-bulle de survol apparaît au-dessus de chacun à la même hauteur, et chaque mode bascule entre les thèmes sombre et clair de Windows">
  <br><sub>Quand le widget est masqué, l'utilisation reste visible dans la barre des tâches. L'icône de la zone de notification affiche, sous forme de deux anneaux, les deux mêmes fournisseurs que le panneau : les deux premiers du widget, ou les deux que vous choisissez. Le survol du panneau ou de l'icône affiche la même liste : les lignes cochées dans la colonne Tooltip. Tous deux suivent le thème clair ou sombre du système.</sub>
</p>

<table>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-detail-vertical.gif" height="230" alt="Les mêmes niveaux de détail avec des barres de progression verticales">
  <br><sub>Les trois mêmes niveaux avec des barres verticales.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-width.gif" height="230" alt="Le widget se rétrécit au fil de ses paliers de largeur ; au plus étroit, les noms des fournisseurs disparaissent et les barres prennent leur place">
  <br><sub>Trois paliers de largeur. Au plus étroit, les noms des fournisseurs sont retirés et la barre prend leur colonne, si bien que rien ne se chevauche.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-opacity.gif" height="230" alt="L'arrière-plan du widget passe de presque entièrement transparent à presque opaque, tandis que le texte reste lisible">
  <br><sub>Opacité de l'arrière-plan, de presque transparent à presque opaque. Le contraste du texte et des barres ne change pas.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-providers.gif" height="230" alt="Le widget passe d'une seule ligne de fournisseur à quatre, puis revient">
  <br><sub>La carte s'ajuste aux lignes affichées : une par fournisseur activé, plus les lignes supplémentaires que vous choisissez.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/palette.gif" height="230" alt="Le widget parcourt la palette monochrome et les huit palettes de couleurs, en faisant varier les curseurs de luminosité, de saturation et d'opacité de l'arrière-plan">
  <br><sub>Monochrome et huit palettes de couleurs, avec luminosité, saturation et opacité de l'arrière-plan. Rendu par l'application elle-même.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/widget-arrangement.gif" height="230" alt="La disposition à barres verticales bascule entre des fournisseurs côte à côte et des fournisseurs empilés en une colonne haute et étroite">
  <br><sub>Avec des barres verticales, les fournisseurs se placent côte à côte ou s'empilent en colonne.</sub>
</td>
</tr>
<tr>
<td width="50%" align="center" valign="top">
  <img src="docs/images/settings-providers.gif" width="400" alt="La fenêtre Settings à côté du widget : cocher la limite hebdomadaire Opus dans la colonne Widget ajoute aussitôt sa ligne au widget, puis le Sign-in de Claude s'ouvre et montre la provenance de sa connexion, ce qui est lu et Forget permission">
  <br><sub><em>Settings</em>, Providers : une ligne cochée apparaît aussitôt sur le widget ; le Sign-in de chaque fournisseur indique ce qui est lu et où va son jeton.</sub>
</td>
<td width="50%" align="center" valign="top">
  <img src="docs/images/settings-display.gif" width="400" alt="L'onglet Display de la fenêtre Settings : décocher Progress bar réduit chaque ligne du widget à une seule ligne de texte et le rend moins haut, Time to reset disparaît puis revient, et la barre revient">
  <br><sub><em>Settings</em>, Display : ce que montre chaque ligne ; sans la barre, le widget est plus petit. Dessiné par l'application elle-même.</sub>
</td>
</tr>
</table>

## Ce que fait l'application

- **Lit, n'écrit jamais.** L'application lit les jetons que les outils CLI
  officiels stockent déjà sur votre machine. Elle ne les rafraîchit jamais et
  n'en fait jamais la rotation ; elle ne peut donc pas vous déconnecter de ces
  outils. Cursor est la seule exception : l'application s'y connecte avec sa
  propre session et ne lit ni ne touche jamais la connexion de l'éditeur. Les
  clés que vous ajoutez vous-même, ainsi que cette session, sont stockées dans
  le Gestionnaire d'identification Windows. L'application se connecte
  uniquement aux points de terminaison des fournisseurs eux-mêmes (et, pour
  une copie installée par le programme d'installation, à GitHub pour les mises
  à jour), envoie une seule requête d'utilisation par fournisseur et par cycle
  (plus les quelques requêtes auxiliaires listées dans
  [docs/en/PROVIDERS.md](docs/en/PROVIDERS.md#requests-per-cycle) (en anglais)), suit
  le proxy défini dans Windows et n'envoie aucune télémétrie.
- **Demande avant de lire.** Rien de ce qui appartient à un autre outil n'est
  lu avant que vous ne l'autorisiez, et chaque question nomme le fichier
  concerné et l'endroit où son jeton est envoyé. Chaque question n'est posée
  qu'une fois, et *Forget permission* retire l'autorisation en un clic. Voir
  [Ce qui est lu, et quand](docs/en/CONFIG.md#what-is-read-and-when) (en anglais).
- **Peu de configuration.** Au premier lancement, les outils trouvés sur ce
  PC sont déjà cochés, et une seule question nomme chaque fichier de connexion
  qui serait lu : *Allow* suffit. Ensuite, votre utilisation habituelle des
  CLI maintient leurs jetons valides ; il n'y a donc rien d'autre à
  configurer. Seul Cursor vous demande de vous connecter une fois, dans
  *Settings*.
- **Faible charge, mesurée sur l'application en cours d'exécution.** 0.014%
  d'un cœur de CPU, aucune utilisation du GPU (le rendu se fait sur le CPU),
  un jeu de travail de 53 MB (25 MB privés), mesurés pour la 0.7.1 le
  2026-11-23 ; aucune résolution de minuterie augmentée (2026-11-21) ; un exe de 4 MB. Pour reproduire la mesure :
  `bench/perf_audit.ps1` ; les chiffres complets se trouvent dans
  [docs/en/ARCHITECTURE.md](docs/en/ARCHITECTURE.md#resource-footprint-measured-not-estimated) (en anglais).
- **Les données périmées sont signalées comme telles.** Une valeur qui cesse
  de se mettre à jour devient grise et affiche son ancienneté. Une fenêtre de
  limite dont la réinitialisation est avérée affiche `≈0%`. L'application ne
  comble pas les lacunes par des suppositions. L'interrogation se met en pause
  pendant votre absence, et ce temps ne compte pas : rien ne devient gris
  simplement parce que vous avez fait une pause.
- **Autonome.** Liée statiquement : ni Visual C++ Redistributable ni DLL
  fournies. Le programme d'installation ne requiert aucun droit
  d'administrateur.

## Installation

> **Vous effectuez la mise à jour depuis la 0.7.0 (ou la 0.6.x), et le chemin
> de votre profil utilisateur ou de votre dossier TEMP contient des lettres
> hors de l'alphabet anglais (cyrilliques, par exemple), `&` ou `%` ?** Le
> module de mise à jour de l'ancienne version échoue sur un tel chemin : une
> fois la 0.7.1 publiée, il peut fermer AI Limits environ une demi-minute
> après chaque démarrage. Installez la 0.7.1 manuellement : téléchargez
> `AiLimits-Setup-0.7.1.exe` depuis
> [Releases](https://github.com/napxlexn/ailimits/releases) et exécutez-le,
> ou, dès que winget propose la 0.7.1, exécutez
> `winget upgrade napxlexn.AILimits`. Si le widget se ferme avant que vous
> ayez pu le faire, faites un clic droit dessus juste après un démarrage et
> décochez d'abord **Automatic updates** (ou définissez `auto_update = false`
> dans `%APPDATA%\AiLimits\config.toml`). À partir de la 0.7.1, le module de
> mise à jour lance directement le programme d'installation, ces chemins
> fonctionnent, et vous pouvez cocher de nouveau Automatic updates.

Depuis le terminal :

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

Ou obtenez-la sur le [Microsoft Store](https://apps.microsoft.com/detail/9P2B0J1VWPWH) :
le même widget, signé et tenu à jour par le Store (il peut avoir quelques
jours de retard sur une nouvelle version, le temps qu'un nouveau paquet soit
certifié).

Ou téléchargez `AiLimits-Setup-<version>.exe` depuis
[Releases](https://github.com/napxlexn/ailimits/releases) et exécutez-le.
Aucun droit d'administrateur n'est nécessaire (installation dans
`%LOCALAPPDATA%\AiLimits`). En option pendant l'installation : démarrage
automatique avec Windows (coché par défaut), un raccourci sur le bureau.
winget et Chocolatey exécutent ce même programme d'installation ; leur copie
se met donc à jour d'elle-même comme toute copie installée par le programme
d'installation, et le gestionnaire de paquets peut indiquer une version plus
ancienne que celle en cours d'exécution.
Une archive zip portable (sans programme d'installation) est également jointe
à chaque version. Une copie portable ne se met pas à jour d'elle-même : c'est
son gestionnaire de paquets, ou vous, qui s'en charge. Il n'existe pas de mode
portable pour les paramètres : chaque copie, portable comprise, les conserve
dans `%APPDATA%\AiLimits`.

> Le programme d'installation **n'est pas signé numériquement** (Authenticode) ;
> Windows SmartScreen peut donc afficher « Windows a protégé votre
> ordinateur ». Cliquez sur **Informations complémentaires → Exécuter quand
> même**. Vous pouvez vérifier le téléchargement à l'aide du SHA-256 publié
> avec chaque version (voir ci-dessous). À partir de la 0.7.1, chaque version
> comporte aussi une signature Ed25519 (`.sig`) pour le programme
> d'installation et le zip, et à partir de la 0.7.1, le module de mise à jour
> de l'application n'installe rien sans signature valide. Le module de mise à
> jour de la 0.7.0 ignore le `.sig` et vérifie uniquement le SHA-256 que
> GitHub publie pour le programme d'installation : sa mise à jour vers la
> 0.7.1 repose sur cette empreinte.

Pour vérifier un téléchargement manuellement, comparez son SHA-256 avec
l'empreinte `sha256:` affichée à côté du fichier sur la page de la version ;
les deux doivent correspondre :

```powershell
(Get-FileHash .\AiLimits-Setup-0.7.1.exe -Algorithm SHA256).Hash
```

Pour vérifier également la signature, placez le `.sig` à côté du fichier, tous
deux sous leur nom publié (la signature couvre le nom). Ensuite, dans un clone
de ce dépôt placé sur l'étiquette de la version (`git checkout v0.7.1`),
exécutez la commande `verify` de l'outil de signature des versions, qui ne
nécessite aucune clé (Rust, comme dans
[Compiler depuis les sources](#compiler-depuis-les-sources)) :

```powershell
cargo run --locked --release --manifest-path tools/release-signer/Cargo.toml -- verify $HOME\Downloads\AiLimits-Setup-0.7.1.exe
```

Elle affiche `verified`, ou `NOT verified` avec la raison.

Le programme d'installation ne contient ni clé ni jeton. Au premier lancement,
le widget ne lit aucune des sources d'authentification listées ci-dessous tant
que vous ne l'avez pas autorisé.

### Désinstallation

La désinstallation depuis les Paramètres de Windows, Applications, avec
`winget uninstall napxlexn.AILimits` ou avec `choco uninstall ailimits`
exécute le programme de désinstallation fourni par le programme
d'installation. Il ferme AI Limits et supprime, hors ligne : son dossier
d'installation, `%APPDATA%\AiLimits` (paramètres, cache, journaux), chaque
entrée du Gestionnaire d'identification créée par l'application (clés, jetons,
la session Cursor), la valeur de démarrage automatique et l'inscription aux
notifications.

La suppression d'une copie Scoop ou portable n'enlève rien de tout cela :
`scoop uninstall ailimits`, ou la suppression du dossier, ne retire que les
fichiers propres à l'application. Avant de la supprimer, quittez AI Limits et
exécutez, dans son dossier (une commande qui n'a rien à supprimer ne fait
rien) :

```
ailimits-auth remove claude
ailimits-auth remove copilot
ailimits-auth remove-usage-token claude
ailimits-auth remove-usage-token codex
ailimits-auth logout cursor
```

Supprimez ensuite `%APPDATA%\AiLimits` et la clé de registre
`HKCU\Software\Classes\AppUserModelId\napxlexn.AILimits` (le nom et l'icône
qu'affichent les notifications de l'application). Pour une copie du Microsoft
Store, la manière de supprimer d'abord ses clés est décrite dans
[docs/en/CONFIG.md](docs/en/CONFIG.md#secrets) (en anglais) ; sa suppression
emporte son propre dossier de paramètres (le dossier de la copie du Store est
décrit au début de ce fichier).

## Démarrage rapide

1. **Lancez l'application.** Au premier lancement, une fenêtre d'accueil
   s'ouvre (la même que *Settings…* dans le menu). Sa question liste les
   outils trouvés sur ce PC (Claude Code, Codex CLI, Copilot CLI ou GitHub
   CLI, Antigravity, Grok CLI, Cursor), chacun avec une case à
   cocher, et nomme chaque fichier qui serait lu ainsi que l'endroit où va
   chaque jeton. Les outils détectés grâce à un dossier qui leur est propre
   sont cochés d'emblée ; GitHub CLI seul (pour Copilot) ou `.gemini` seul
   (pour Antigravity) apparaît non coché.
   *Allow* démarre les outils cochés ; *Cancel* ne lit rien et vous laisse
   cocher ce que vous voulez, chaque outil posant sa question une fois. La
   fenêtre ne prend pas le focus clavier (Entrée n'autorise rien par accident)
   et attend tant qu'une application en plein écran est au premier plan.
   Comme *Settings*, elle reste ouverte jusqu'à sa fermeture par Échap, sa
   croix ou Alt+F4, et se déplace par sa barre de titre. Cursor nécessite
   une connexion propre à l'application, une seule fois : dans la même
   fenêtre, ouvrez la ligne *Sign-in* sous Cursor et cliquez sur
   *Sign in to Cursor…*.
   La mise à jour depuis la 0.7.0 ne demande rien : les fournisseurs que vous
   aviez continuent de fonctionner, et une fenêtre liste ce qui est lu.
2. **Placez le widget.** Faites-le glisser avec le bouton gauche. Maintenez
   **Maj** enfoncée pendant le glissement pour le plaquer contre le bord le
   plus proche de la zone de travail de l'écran (l'écran sans la barre des
   tâches) dès qu'il se trouve à moins de 64 px de ce bord.
   `Lock position` dans le menu du clic droit le fige.
3. **Réglez l'apparence.** Le clic droit donne accès au niveau de détail
   (Compact, Medium, Expanded), à la disposition (Rows ou Columns), à la
   palette (monochrome ou 8 couleurs), à l'opacité, à la luminosité et à la
   saturation. Les animations ci-dessus montrent chacun de ces réglages.
   Laisser le pointeur sur l'un de ces éléments, ou sur Width, Column layout
   ou Usage forecast, en montre l'effet sur le widget, le panneau et l'icône
   de la zone de notification avant même le clic, tout comme s'y rendre au
   clavier ; quitter le menu sans cliquer rétablit l'état précédent.
   *Settings…*
   choisit les lignes qu'affichent le widget et l'info-bulle du panneau
   (lignes supplémentaires comprises, comme la semaine Opus de Claude), une
   étiquette facultative indiquant le modèle utilisé (modèle, version, effort
   de raisonnement) et la provenance de la connexion de chaque fournisseur.
   *Model source* choisit d'où vient le modèle de l'étiquette :
   *Tool settings*, ou *Last session*, le modèle de la session Claude Code
   ou Codex CLI la plus récente, lu à la fin de son fichier (au plus le
   dernier 1 MB) ; les autres outils affichent le modèle tel qu'il est
   configuré.
   La colonne Taskbar choisit les deux fournisseurs qu'affichent le panneau
   et l'icône de la zone de notification ; au moins un reste coché.
   Percentage et Time to reset se cochent séparément pour l'info-bulle du
   panneau (colonne Tooltip) et pour le panneau (colonne Taskbar) : le
   pourcentage, le temps restant avant la réinitialisation de la limite, ou
   les deux ; la dernière option active ne peut pas être décochée. Progress
   bar (activé par défaut) permet d'omettre la barre du panneau pour occuper
   moins de place dans la barre des tâches.
4. **Survolez une ligne de fournisseur** pour voir sa limite hebdomadaire à la
   place du compte à rebours avant réinitialisation (en Compact, sur la ligne
   du bas).
   Une ligne grisée ou marquée `≈` affiche à la place la raison et la
   solution, par exemple `token expired — run Claude Code`. La raison
   s'affiche en entier sur les lignes en Medium et Expanded et sur la ligne du
   bas de Compact (aux largeurs 100% et 75%), tronquée sous une colonne en
   Expanded ; les colonnes en Compact et Medium, ainsi que Compact à 50%,
   n'en affichent aucune.
5. **Le widget vous gêne ?** Clic droit, **Taskbar**, puis *Taskbar panel*
   ou *Tray icon*. Lorsque le widget est masqué ou recouvert par d'autres
   fenêtres, l'utilisation reste affichée dans la barre des tâches ; tant
   qu'une application en plein écran occupe l'écran, l'indicateur se masque
   avec la barre des tâches et revient lorsque vous quittez l'application.
   Voir [Indicateur dans la barre des tâches](#indicateur-dans-la-barre-des-tâches).
6. **Cliquez avec le bouton gauche sur l'icône de la zone de notification ou
   sur le panneau** pour afficher le widget au premier plan ou le masquer.
   Un clic droit sur l'un ou l'autre ouvre le même menu.
7. **Un message au lieu des lignes ?** *Setup required* ou
   *No data yet* : cliquez sur le widget pour ouvrir *Settings*.
   *Right-click → set up providers* : des fournisseurs sont cochés, mais aucun
   n'est connecté ; vérifiez le *Sign-in* de chaque fournisseur dans *Settings*.

## Fonctionnement de l'autorisation

Le widget ne vous demande jamais de vous connecter, sauf une fois pour Cursor.
Il surveille les limites d'**abonnement** en réutilisant les jetons que les
outils CLI officiels conservent sur votre machine, strictement en lecture
seule et uniquement une fois que vous avez autorisé chacun d'eux :

| Fournisseur | Source automatique (par défaut) | Pour l'activer |
|---|---|---|
| Claude | `%USERPROFILE%\.claude\.credentials.json` (jeton OAuth de Claude Code) | exécuter [Claude Code](https://claude.com/claude-code) au moins une fois |
| Codex | `%USERPROFILE%\.codex\auth.json` (jeton de Codex CLI ; dans `CODEX_HOME` s'il est défini) | exécuter `codex login` une fois |
| Copilot | `gh auth token --hostname github.com` (GitHub CLI) | `gh auth login` une fois |
| Antigravity | cible `gemini:antigravity` du Gestionnaire d'identification Windows | exécuter `agy`, l'outil Antigravity CLI ; son jeton dure environ une heure et seul `agy` le renouvelle (ouvrir l'éditeur ne suffit pas) |
| Grok | `%USERPROFILE%\.grok\auth.json` (connexion de Grok CLI ; dans `GROK_HOME` s'il est défini) | exécuter `grok` et se connecter une fois (abonnement SuperGrok) |
| Cursor | la session propre au widget, dans le Gestionnaire d'identification Windows (`cursor_session`) ; rien de l'éditeur Cursor n'est lu pour cela | Se connecter une fois à Cursor dans *Settings* (*Sign-in* de Cursor, *Sign in to Cursor…*) ou avec `ailimits-auth login cursor` : la page de connexion de Cursor s'ouvre dans le navigateur ; session de ~60 jours |

Sous les lignes de chaque fournisseur, dans l'onglet *Providers*, *Settings*
comporte une ligne *Sign-in* qui indique la provenance de la connexion et son
résultat (« Claude Code · connected »). Ouvrez-la pour choisir la source : le
compte de l'outil ci-dessus (*Claude Code account*, etc.) ou, pour Claude,
Codex et Copilot, une clé ou un jeton à vous (voir Autorisation manuelle
ci-dessous). Seule la source choisie est utilisée ; aucune autre ne prend le
relais si elle échoue.

Chaque outil n'est sollicité qu'une fois. Décocher toutes les lignes d'un
fournisseur (Widget, Tooltip et Taskbar) le met en pause : rien de lui n'est
lu ni envoyé, et une coche le relance sans nouvelle question. Pour retirer
l'autorisation, ouvrez son *Sign-in* et cliquez sur *Forget permission*
(affiché tant que la connexion propre à l'outil est la source) : sa connexion
n'est plus lue, ses lignes sont décochées, et la coche suivante redemande
l'autorisation. La session Cursor se termine avec *Sign out* ; une clé ou un
jeton collé disparaît avec *Remove*. Les lectures de l'étiquette du modèle ont
leur propre *Forget permission* dans l'onglet *Display*, sous Model, Version et
Reasoning effort. Les autorisations sont conservées par utilisateur Windows,
dans le `config.toml` de cet utilisateur.

Codex CLI n'écrit `auth.json` que lorsqu'elle conserve sa connexion dans un
fichier : avec `cli_auth_credentials_store = "keyring"` ou `"auto"` dans son
`config.toml`, il n'y a pas de `auth.json`, et sa connexion ne peut pas être
lue. Réglez ce paramètre sur `"file"`, ou collez plutôt un jeton
d'utilisation.

Ce sont des jetons d'accès OAuth, de courte durée, sauf celui de Cursor. La
CLI à laquelle appartient chacun d'eux le rafraîchit chaque fois que vous
l'utilisez. Le widget ne rafraîchit jamais le jeton d'une autre application,
car un rafraîchissement OAuth en fait la rotation et déconnecterait la CLI.
La session Cursor (environ 60 jours) appartient au widget : vous vous
connectez à Cursor dans le navigateur, sur la page de Cursor, et le widget
conserve et rafraîchit lui-même cette session. Cursor ne propose pas de
connexion pour les applications tierces ; le widget se connecte donc en tant
que client de l'éditeur Cursor, sans jamais lire la connexion de l'éditeur.
`CODEX_HOME`, `GROK_HOME` et `COPILOT_HOME` ne sont pris en compte que s'il
s'agit de chemins absolus.

### Combien de temps les données restent exactes sans rien faire

| Fournisseur | Les données en direct restent valables | Ensuite |
|---|---|---|
| **Claude** | ~8 heures après votre dernière session Claude Code (la durée de vie réelle du jeton) | affiche la dernière valeur grisée, avec son ancienneté |
| **Codex** | tant que vous utilisez Codex CLI (la durée de vie de son jeton n'est pas encore mesurée) | la dernière valeur grisée, avec son ancienneté |
| **Copilot** | Indéfiniment. gh maintient son jeton à jour tant que vous restez connecté. | |
| **Antigravity** | environ une heure après la dernière exécution de `agy` (seul `agy` renouvelle son jeton) | la dernière valeur grisée, avec son ancienneté |
| **Grok** | jusqu'à ~6 heures après votre dernière session Grok CLI (la durée de vie réelle du jeton) | la dernière valeur grisée, avec son ancienneté |
| **Cursor** | environ 60 jours après votre connexion (la durée de vie de la session) | la dernière valeur grisée, et `sign in to Cursor again` |

Le 18 juin 2026, Google a cessé de traiter les requêtes de Gemini CLI pour les
utilisateurs individuels, Google AI Pro et Google AI Ultra. Ces utilisateurs
sont invités à migrer vers Antigravity CLI ; le widget lit uniquement le jeton
d'Antigravity dans le Gestionnaire d'identification Windows, et non plus
l'ancien fichier de Gemini CLI. Google a suspendu certains comptes pour avoir
utilisé la connexion d'Antigravity en dehors d'Antigravity. Le widget se
contente de lire votre quota, une fois par mise à jour, mais le risque vous
incombe : la question qui demande à lire la connexion d'Antigravity le
précise.

**Claude limite côté serveur la fréquence à laquelle l'utilisation peut être lue.**
Anthropic limite le débit du point de terminaison d'utilisation (HTTP 429,
`Retry-After: 0`, vérifié le 2026-07-09), et le budget par jeton est partagé
avec les propres interrogations de Claude Code. Pendant que Claude Code
s'exécute, une actualisation de Claude peut prendre 2 à 3 minutes, même avec
l'intervalle de 1 minute. Le cycle suivant redemande à l'intervalle de mise à
jour ; l'attente ne s'allonge (2, 4, puis au plus 5 minutes avec l'intervalle
de 1 minute) que lorsqu'aucun fournisseur n'a apporté de données au cours d'un
cycle, et une limitation de débit compte comme une absence de données. Pendant
ce temps, le survol de la ligne Claude affiche `rate-limited, retrying`. Les
autres fournisseurs ne sont pas concernés.

Un jeton Claude, Codex, Antigravity, Grok ou Cursor expiré ne produit pas de chiffres erronés.
La dernière valeur reste grisée avec son ancienneté, une fenêtre dont la
réinitialisation est avérée affiche `≈0%`, et le compte à rebours jusqu'à la
réinitialisation continue localement. Si vous utilisez les CLI tous les
jours, les données sont en direct presque en permanence.

### Autorisation manuelle (pour l'indépendance ou sans CLI)

Pour les configurations sans les CLI, ou lorsque vous voulez un identifiant
qui ne dépend pas de l'activité des CLI. Ce que chaque option vous apporte
réellement :

| Fournisseur | Option manuelle | Où l'obtenir | Affiche la même chose que l'abonnement ? |
|---|---|---|---|
| **Copilot** | Un Personal Access Token fine-grained ; il ne nécessite aucune autorisation (laissez-les vides) | github.com/settings/personal-access-tokens, *Generate new token* | Vérifié en direct le 2026-11-21 : un jeton sans autorisations lit les mêmes quotas que la connexion de GitHub CLI. Il ne nécessite pas gh, jusqu'à l'expiration que vous avez fixée. |
| **Claude** | Clé API | console.anthropic.com, Settings, API Keys, *Create Key* (`sk-ant-…`) | Non. Elle affiche les limites de débit de votre compte API, pas l'utilisation de votre abonnement Pro ou Max. |
| **Codex** | Aucune | Il n'existe pas de jeton stable distinct. Un *Usage token* collé est le même type de jeton OAuth que celui de Codex CLI. | |
| **Antigravity** | Aucune | Le quota est lié au jeton OAuth que rafraîchit Antigravity CLI, et aucune clé distincte ne renvoie les mêmes chiffres. | |
| **Grok** | Aucune | La limite hebdomadaire SuperGrok n'est lue qu'avec la connexion propre de Grok CLI. | |
| **Cursor** | Sa propre connexion, environ 60 jours | *Settings*, *Sign-in* de Cursor, *Sign in to Cursor…*, ou `ailimits-auth login cursor` | Oui. Il s'agit de l'utilisation de l'abonnement lui-même. |

**Deux façons d'ajouter un identifiant :**

1. **Dans *Settings*.** Copiez la clé ou le jeton. Ouvrez la ligne
   *Sign-in* du fournisseur, choisissez la source (Claude : *Usage token* ou
   *API key* ; Codex : *Usage token* ; Copilot : *Fine-grained token*) et
   cliquez sur *Paste token from clipboard*
   (*Paste key from clipboard* pour la clé API de Claude). Seul ce type de
   secret est accepté (`sk-ant-...` ; `ghp_...`, `github_pat_...`, `gho_...`,
   `ghu_...` ou un jeton hexadécimal classique de 40 caractères ; un jeton
   ChatGPT `eyJ...`) : tout autre contenu du presse-papiers est refusé sans
   être envoyé, et n'est jamais affiché ni journalisé. Un jeton d'utilisation
   est d'abord vérifié par une requête : un jeton refusé n'est pas stocké. Une
   clé est stockée telle quelle, et la ligne indique si elle fonctionne. La
   fenêtre indique le résultat (par une notification si la fenêtre a été
   fermée entre-temps), et le secret va directement dans le Gestionnaire
   d'identification. Seule la source choisie est utilisée : la connexion de
   l'outil n'est pas lue tant que votre jeton ou votre clé est sélectionné, et
   un jeton stocké n'est pas envoyé tant que le compte de l'outil est
   sélectionné.
2. **Depuis le terminal.** `ailimits-auth.exe` se trouve à côté du widget dans
   `%LOCALAPPDATA%\AiLimits` :
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
   `set-usage-token` vérifie le jeton par une requête au point de terminaison
   d'utilisation du fournisseur : un jeton refusé (401/403) n'est pas stocké ;
   une limitation de débit (429) vaut acceptation ; sans réponse tranchée dans
   un sens ou dans l'autre, la commande redemande après 2 puis 4 secondes,
   puis stocke le jeton avec une remarque. `status` ne modifie rien et ne lit
   la connexion d'un autre outil que là où le widget y est autorisé. Un widget
   en cours d'exécution prend en compte de lui-même ce que modifient les
   autres commandes, sans redémarrage : un paramètre modifié en quelques
   secondes, une clé ou un jeton nouveau ou supprimé à sa prochaine mise à
   jour. `set` et `set-usage-token` font de ce qu'elles stockent la source du
   fournisseur.

**Pour revenir à la connexion de l'outil :** dans le *Sign-in* du fournisseur,
choisissez le compte de l'outil (*Claude Code account*, *Codex CLI account* ou
*GitHub CLI account*). Votre clé ou votre jeton reste stocké mais inutilisé ;
*Remove*, avec sa source sélectionnée, le supprime. Depuis le terminal,
`ailimits-auth remove claude|copilot` supprime la clé et rétablit la source de
l'outil ; `remove-usage-token` supprime le jeton mais laisse la source sur
celui-ci, si bien que la ligne reste masquée jusqu'à ce que vous choisissiez
le compte de l'outil dans la fenêtre.

## Le menu contextuel

Faites un clic droit sur le widget :

- `Settings…` : lignes, étiquettes de modèle, alias de modèles, connexion de
  chaque fournisseur.
- **Detail** : Compact / Medium / Expanded.
- **Layout** : Rows / Columns, puis Width (100 / 75 / 50%, grisé sauf en
  Rows) et Column layout (Side by side / Stacked, grisé sauf en Columns).
- **Appearance** : Palette (8 couleurs), Monochrome, Background opacity,
  Brightness, Saturation.
- **Taskbar** : Tray icon / Taskbar panel / None, et Panel display lorsqu'il y
  a plus d'une barre des tâches.
- `Always on top` et `Lock position` (deux bascules indépendantes), Usage
  forecast (prévision du rythme de consommation « ~time to limit », désactivée
  par défaut).
- Refresh interval (1 / 5 / 15 / 30 min) et `Automatic updates` (mise à jour
  automatique silencieuse en arrière-plan d'une copie installée par le
  programme d'installation, activée par défaut ; les copies du Store, de
  Scoop, portables et compilées soi-même n'ont pas cet élément). Une nouvelle
  version est annoncée une seule fois, dans une notification avec
  *Install now* et *What's new* ; sans action de votre part, elle s'installe
  d'elle-même la prochaine fois que vous vous absentez cinq minutes.
- *AI Limits 0.7.1*, avec le logo GitHub (ouvre la page du projet), et Quit.

Usage forecast affiche son ~temps sur les lignes en Medium et Expanded et sur
les colonnes en Expanded, uniquement lorsqu'aucune heure de réinitialisation
n'est connue, et jamais pour des données qui ne se mettent plus à jour.

### Indicateur dans la barre des tâches

Lorsque la surcouche est masquée ou recouverte par d'autres fenêtres, un
affichage compact de l'utilisation reste dans la barre des tâches ; tant
qu'une application en plein écran occupe l'écran, il se masque avec la barre
des tâches et revient lorsque vous quittez l'application. Clic droit →
**Taskbar** :

- **Tray icon** (par défaut). Une seule icône de 16px : deux anneaux
  concentriques pour les deux mêmes fournisseurs que ceux du panneau (les deux
  premiers du widget, ou les deux cochés dans la colonne Taskbar de
  *Settings*), le plus chargé à l'extérieur, tous deux se remplissant dans le
  sens horaire à partir de 12 heures. Monochrome, dessinée selon le thème de
  la barre des tâches du système.
- **Taskbar panel.** Une surcouche transparente à côté de l'horloge. Il n'y a
  ni fenêtre ni cadre, seulement des chiffres de la taille de ceux de
  l'horloge et des barres, dessinés par-dessus la barre des tâches. Le panneau
  est monochrome, suit le thème clair ou sombre du système, affiche les deux
  premiers fournisseurs dans l'ordre du widget, ou les deux cochés dans la
  colonne Taskbar de *Settings*, avec leur pourcentage (et sa barre, sauf si
  Progress bar y est décoché), le temps restant avant la réinitialisation de
  leur limite, ou les deux (une valeur qui ne se met plus à jour, ou une
  estimation `≈`, est dessinée plus pâle, de même que le widget la grise), et
  suit la barre des tâches quand elle se déplace
  (masquage automatique, changements de résolution, largeur de la zone de
  notification). Survolez-le pour afficher une info-bulle avec les lignes
  cochées dans la colonne Tooltip (tous les fournisseurs par défaut), chacune
  avec son pourcentage et l'heure de sa réinitialisation.
  Si Démarrer, Rechercher ou la barre masquée automatiquement le recouvre, il
  cède la place aux anneaux de la zone de notification et revient de
  lui-même. Si une application en plein écran occupe l'écran, il se masque
  avec la barre des tâches et revient lorsque vous en sortez par Alt+Tab. Il
  fonctionne avec une barre des tâches placée sur n'importe quel bord de
  l'écran : sur une barre latérale, les deux fournisseurs s'empilent,
  pourcentage au-dessus de la barre, au-dessus de la zone de notification.
  Une barre sans espace libre entre son dernier bouton d'application et la
  zone de notification n'a pas de panneau ; les anneaux de la zone de
  notification prennent le relais jusqu'à ce que de la place se libère, de
  sorte que le panneau ne chevauche jamais les boutons.
- **None.** Aucun indicateur.

Si Windows affiche la barre des tâches sur plus d'un écran, **Taskbar → Panel display**
choisit la barre des tâches à laquelle le panneau s'attache. Le sous-menu
n'apparaît que s'il existe une deuxième barre des tâches ; si l'écran choisi
est déconnecté par la suite, le panneau revient sur la barre des tâches
principale. Le menu est construit au démarrage de l'application ; un écran
connecté plus tard n'y apparaît donc qu'après un redémarrage. *Display 2*,
*Display 3* et les suivants sont les autres barres des tâches, comptées de
gauche à droite, et non les numéros d'écran propres à Windows.

Un clic gauche sur l'icône de la zone de notification ou sur le panneau amène
le widget au premier plan (ou le masque s'il y est déjà, au-dessus) ; un clic
droit **n'importe où sur le panneau** ouvre ce même menu.

## Configuration et diagnostic

`%APPDATA%\AiLimits\config.toml` est créé automatiquement, et chaque
modification faite dans le menu y est enregistrée. Les modifications
enregistrées dans ce fichier s'appliquent pendant que l'application tourne, en
deux secondes environ, sans redémarrage ; *Edit config.toml…* à la fin de
l'onglet *Display* de *Settings* l'ouvre dans le Bloc-notes. Une ligne
illisible est ignorée et signalée, et sa valeur par défaut est utilisée. Le
fichier s'explique de lui-même : une courte note avant chaque paramètre
indique ce qu'il fait, les valeurs qu'il accepte et sa valeur par défaut.
Référence du format : [docs/en/CONFIG.md](docs/en/CONFIG.md) (en anglais).
Réglages fins au-delà du menu (tailles, couleurs, seuils) : [docs/en/TUNING.md](docs/en/TUNING.md) (en anglais).

Définissez `AILIMITS_LOG=ailimits=debug` ou `RUST_LOG=ailimits=debug` pour
écrire un journal de diagnostic dans `%APPDATA%\AiLimits\ailimits.log` (stderr
est invisible dans une application graphique). Le fichier est renouvelé à
5 MB : le précédent est conservé sous le nom `ailimits.log.1`.

Un plantage laisse une ligne dans `%APPDATA%\AiLimits\crash.log` (heure,
version, thread, emplacement dans le code source ; jamais le texte du message
de plantage) ; un démarrage qui a échoué le signale par un message et dans ce
fichier. Relancer AI Limits alors qu'il tourne déjà affiche le widget et le
met au premier plan.

Une notification de seuil arrive une fois par fenêtre de limite, lorsque
l'utilisation atteint le seuil `alert_threshold` du fournisseur (80% par
défaut, 85% pour Copilot) ; à nouveau seulement après la réinitialisation de
la fenêtre, ou après que l'utilisation est descendue de plus de 5 points sous
le seuil puis remontée. `[notifications]` dans la configuration désactive les
notifications (`enabled = false`) et définit l'intervalle minimal entre deux
notifications d'un même fournisseur (`cooldown_minutes`).

Les requêtes suivent le proxy défini dans Windows (Paramètres, Réseau et
Internet, Proxy, « Utiliser un serveur proxy ») ; `HTTPS_PROXY`, `HTTP_PROXY` et `ALL_PROXY` ont la priorité, un script de proxy (PAC)
n'est pas suivi. `ailimits-auth` ne suit que les variables, pas le réglage de
Windows. Un réseau qui inspecte le TLS avec son propre certificat racine n'est
pas considéré comme fiable : les lignes affichent « untrusted certificate,
retrying ».

## Compiler depuis les sources

Nécessite Rust 1.90 ou plus récent avec la chaîne d'outils MSVC : Visual
Studio Build Tools avec la charge de travail C++ et le Windows SDK, dont
`rc.exe` intègre l'icône, le manifeste et la version (voir `build.rs`). Le
programme d'installation est construit avec Inno Setup 6.7.4 à partir des
exe `release-min`. Une copie lancée depuis `target\` ne se met pas à jour
d'elle-même et n'a pas d'élément *Automatic updates*.

```
# Rust 1.90+ (MSVC), Windows 11
cargo build --profile release-min --locked   # ~4 MB exe in target/release-min/
cargo test --locked
ISCC.exe installer\ailimits.iss              # the installer, in target/installer/
```

Documentation pour les développeurs : [docs/en/](docs/en/) (en anglais).

## Licence

Copyright (C) 2026 napxlexn.

AI Limits est un logiciel libre distribué sous la
[GNU General Public License, version 3 ou ultérieure](LICENSE). Vous pouvez
l'utiliser, l'étudier, le modifier et le redistribuer. Si vous distribuez une
version modifiée, vous devez fournir le code source correspondant sous la même
licence, conserver les mentions de droit d'auteur et de licence, et signaler
clairement vos modifications.

La licence GPL du logiciel n'accorde aucun droit de présenter des builds
modifiés comme des versions officielles d'AI Limits. Voir
[TRADEMARKS.md](TRADEMARKS.md) pour les règles relatives au nom et au logo du
projet. Les composants tiers restent soumis à leurs licences respectives ;
leurs mentions accompagnent chaque copie dans
[THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).
