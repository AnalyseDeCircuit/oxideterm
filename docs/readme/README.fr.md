<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm : vos serveurs, un seul espace de travail" width="920">

# ⚡ OxideTerm

**Un client SSH natif et gratuit pour travailler sur des serveurs distants.**

SSH · Mosh · Telnet · série · RDP/VNC · SFTP · redirection de ports · éditeur de code

Sans compte. Sans abonnement. Sans télémétrie. Sans Electron.

[![Dernière version](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![Plateformes](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![Licence](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Étoiles](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**Télécharger**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**Documentation**](https://oxideterm.app) ·
[**Journal des modifications**](../../.github/release-notes/stable-changelog.md) ·
[**Signaler un problème**](https://github.com/AnalyseDeCircuit/oxideterm/issues) ·
[**Groupe WeChat**](https://github.com/AnalyseDeCircuit/oxideterm/issues/752)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md)

</div>

---

## Qu’est-ce qu’OxideTerm ?

OxideTerm est un client SSH pour macOS, Windows et Linux. Chaque serveur dispose de son espace de travail dans une même fenêtre : terminaux, gestionnaire de fichiers, redirections de ports, éditeur de code, surveillance et assistant IA facultatif. OxideTerm est gratuit et open source sous GPL-3.0.

## Pourquoi OxideTerm

- **Une connexion pour tous les outils.** Les terminaux, SFTP, redirections et l’éditeur d’un serveur partagent une connexion SSH par défaut : ouvrir un autre outil ne demande pas de nouvelle authentification.
- **Résiste aux brèves coupures.** OxideTerm teste une connexion qui ne répond plus pendant 30 secondes avant de la remplacer, pour que vim, htop et les autres programmes plein écran puissent reprendre leur état.
- **Peu gourmand en mémoire.** L’interface est dessinée sur le GPU avec [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), le framework de Zed. Au repos, elle utilise environ 81 MB sur macOS et 24 MB sur Windows.
- **Votre IA, vos choix.** OxideSens utilise le fournisseur et la clé API que vous choisissez, sans crédits OxideTerm à acheter.
- **SSH entièrement en Rust.** La pile SSH repose sur russh et ring, sans OpenSSL ni libssh2.

<details>
<summary><b>Comparaison de la mémoire avec OxideTerm 1.x</b></summary>

Mémoire au repos avant et après le passage de Tauri (1.x) à GPUI natif (2.0). Les chiffres de la version 1.x incluent ses processus WebView.

| Plateforme | 1.x (Tauri) | 2.0 (native) | Réduction |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 74% |
| Windows | 182.4 MB | 23.5 MB | 87% |

![Mémoire au repos d’OxideTerm 1.x et 2.0 dans les moniteurs de processus](../../docs/screenshots/oxideterm-memory-comparison.png)

</details>

---

## Captures d’écran

### Terminal

Une session SSH. La barre latérale présente les outils disponibles sur ce serveur : terminaux supplémentaires, SFTP, IDE et redirection de ports.

![Terminal SSH avec les outils du serveur dans la barre latérale](../../docs/screenshots/terminal/SSHTERMINAL.png)

### Espace de travail mixte

Un terminal SSH, un terminal local, des notes Markdown et SFTP réunis dans un même onglet. Combinez les pages de plusieurs serveurs et de votre machine dans un maximum de 16 volets.

![Espace de travail à quatre volets réunissant SSH, un terminal local, des notes et SFTP](../../docs/screenshots/mixed-workspace.png)

### Fichiers

Dossiers locaux et distants côte à côte, avec la file de transfert en dessous.

![Gestionnaire SFTP à deux volets avec file de transfert](../../docs/screenshots/sftp/sftp.png)

### IDE

Ouvrez un projet distant et modifiez ses fichiers dans des onglets.

![Projet distant ouvert dans l’IDE intégré](../../docs/screenshots/miniIDE/miniide.png)

### Redirection de ports

OxideTerm répertorie les ports en écoute sur le serveur. Redirigez-les en un clic ou utilisez une redirection rapide pour des outils comme Jupyter.

![Page des redirections avec les ports détectés sur le serveur](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png)

<details>
<summary><b>Voyez OxideSens ouvrir un terminal à partir d’une demande en langage naturel</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens ouvre un terminal dans OxideTerm" width="720">
</a>

</details>

---

## Fonctionnalités

Les éléments marqués *(plugin)* s’installent depuis le catalogue de plugins de l’application.

### Espace de travail

- Gestionnaire de sessions : recherche, tri, groupes, import et export des connexions, groupes imbriqués, étiquettes, couleurs, icônes et notes
- Import de `~/.ssh/config`, avec blocs `Match`, chaînes `ProxyJump` et `RemoteCommand` ; les hôtes importés restent synchronisés
- Import depuis SecureCRT, Xshell, Termius, MobaXterm, WindTerm, Electerm et FinalShell, avec aperçu avant enregistrement
- Filtrage des onglets : tous, pages globales, terminaux locaux ou un serveur ; les sessions masquées continuent de fonctionner
- Jusqu’à 16 volets par onglet, mêlant terminaux, SFTP, IDE, redirections, notes, paramètres et bureaux distants
- Fenêtres détachées : déplacer un onglet dans une autre fenêtre puis le ramener sans interrompre ses sessions
- Disposition de 2–16 terminaux ouverts en grille, sans reconnexion
- Palette de commandes pour les actions et connexions enregistrées, et mode Zen centré sur une page
- Profils de terminal local avec shell, répertoire initial et icône propres
- Restauration de l’espace de travail, de la taille de fenêtre, du plein écran et du placement sur les écrans au démarrage

### Terminal

- Détection des shells locaux : shells Unix, Invite de commandes, Windows PowerShell, PowerShell Core, Git Bash et Nushell
- Volets divisés avec séparateurs déplaçables ; un nouveau volet local reprend le répertoire courant
- Recherche par expressions régulières, respect de la casse et requêtes multilignes, indépendante dans chaque volet
- Intégration de Bash, Zsh, Fish, Nushell et PowerShell : répertoire courant, codes de sortie et repères de commandes ; installation sur les serveurs depuis l’application
- Repli des commandes terminées avec conservation de leur état de sortie
- Barre de commandes avec saisie multiligne, historique, suggestions de chemins, complétion et commandes rapides
- Suggestions approximatives depuis l’historique, et historique persistant par connexion en option
- Mode de saisie libre pour modifier la commande courante avec la souris
- Modification des collages multilignes avant envoi, avec annulation et retrait des délimiteurs de blocs de code
- Déclencheurs sur texte ou expressions régulières : envoyer une saisie, exécuter une commande rapide, lancer un programme ou rendre le résultat cliquable
- Coloration sémantique des sorties sans style : erreurs, états Git, permissions, adresses et dates ; surlignage par mots-clés et expressions régulières
- URL et chemins cliquables, horodatage par ligne à la milliseconde et marqueurs de sortie non lue sur les onglets
- Graphiques Sixel et Kitty, y compris les animations Kitty
- Mode de contrôle natif `tmux -CC` avec disposition des volets et séparateurs déplaçables
- Groupes de saisie synchronisée et envoi de commandes en texte ou hexadécimal avec cadence, intervalles et répétitions
- Commandes rapides avec groupes, épinglage, paramètres texte, choix ou secrets, et règles d’affichage par hôte
- Journaux de session : modèles de noms, limites de taille et de conservation, codes ANSI facultatifs ; enregistrements `.cast` et lecteur intégré
- Copier, couper et coller dans les sélections de Vim, Neovim ou Emacs, en local et dans les sessions SSH prises en charge
- Séquences Backspace et Delete et encodage du texte configurables par connexion

### Connexions

- Authentification SSH par mot de passe, clé privée, certificat OpenSSH, clé gérée, agent SSH, échange interactif au clavier ou Kerberos
- Agent SSH détecté ou endpoint personnalisé (`SSH_AUTH_SOCK` sur macOS/Linux, `\\.\pipe\openssh-ssh-agent` sur Windows), et transfert d’agent
- Clés Ed25519, RSA et ECDSA, chiffrements ChaCha20-Poly1305 et AES-GCM, ordre des algorithmes réglable et option pour serveurs anciens
- Génération et saisie des codes TOTP à une invite correspondante, sur le serveur ou chaque hôte de rebond
- Clés de sécurité FIDO (`ed25519-sk`, `ecdsa-sk`) avec demande de PIN et de contact *(plugin)*
- Chaînes d’hôtes de rebond avec authentification par saut, proxys HTTP CONNECT et SOCKS5 en amont, et ProxyCommand
- Reconnexion avec délai de grâce, restauration des redirections, reprise des transferts et réouverture des fichiers de l’éditeur sur la nouvelle connexion
- Connexion partagée par serveur ou authentification distincte pour chaque nouveau terminal
- Transfert X11 en mode de confiance ou non
- Connexions temporaires sans enregistrement ; liens `ssh://`, `telnet://`, `mosh://`, `rdp://` et `vnc://` ouverts dans l’application
- Mosh avec prédiction adaptative de l’écho local *(plugin)*
- Telnet : négociation des options, routage par proxy et fins de ligne configurables
- Ports série : débit, bits de données, parité, bits d’arrêt, contrôle de flux, entrées/sorties hexadécimales, Break, DTR et RTS
- Terminaux Bluetooth LE via Nordic UART Service ou caractéristiques GATT personnalisées

### Fichiers et transferts

- Gestionnaire local/distant à deux volets, favoris, complétion de chemins et vues en liste ou arborescence
- Favoris associant chemins locaux et distants, enregistrés par connexion
- File avec transferts parallèles, limites de débit, temps restant, pause, reprise et nouvel essai ; les transferts SFTP interrompus reviennent en pause après redémarrage
- Écraser, ignorer ou renommer en cas de conflit, pour un fichier ou tous
- SFTP avec repli automatique vers SCP si le serveur n’a pas de sous-système SFTP
- Connexions SFTP autonomes et transferts entre serveurs relayés par l’application
- FTP et FTPS explicite
- Déposer des fichiers sur un terminal SSH pour les envoyer dans son répertoire courant
- `trz`/`tsz` (trzsz) et `rz`/`sz` (ZMODEM) dans le shell distant ; XMODEM, YMODEM et ZMODEM sur les connexions série
- Aperçus du texte, code, Markdown, images, audio, vidéo, hexadécimal et polices
- Lecteurs PDF, SQLite, certificats et fichiers binaires *(plugin)*
- Passer du terminal au même répertoire dans SFTP et envoyer un répertoire SFTP au terminal

### Éditeur et IDE

- IDE distant via SFTP, commandes shell simples ou agent distant ajoutant indexation des symboles, complétion et surveillance des fichiers
- Conversion d’un terminal inactif en IDE avec le même utilisateur, conteneur et répertoire
- Onglets, arborescence de projet avec sélection multiple, recherche de projet et remplacement par expressions régulières avec groupes de capture
- Fichiers jusqu’à 100 MiB, avec conservation de l’encodage, du BOM et des fins de ligne
- Enregistrement sûr avec gestion des conflits et sauvegarde automatique facultative au changement d’onglet
- Restauration des fichiers ouverts et modifications non enregistrées après redémarrage
- Vues Git : branches, worktrees, index, historique, compteurs d’avance/retard et états de fusion ou rebase
- Détection et exécution des tâches du projet
- Coloration syntaxique de 15 langages intégrés et de plus de 20 autres via plugins *(plugin)*
- Aperçu Markdown avec tableaux, formules et notes de bas de page

### Redirection de ports et réseau

- Redirections locales, distantes et dynamiques (SOCKS5), règles enregistrées pouvant démarrer avec la connexion
- Détection des ports en écoute sur le serveur et redirection en un clic
- Tunnels SSH autonomes sans terminal, avec démarrage facultatif au lancement de l’application
- Restauration des redirections après reconnexion ; les règles arrêtées manuellement le restent
- Proxy global et certificats approuvés du système, y compris les autorités d’entreprise, pour les requêtes réseau de l’application
- Vue de la topologie des connexions et débogage ponctuel de sockets

### Bureau à distance

- RDP et VNC : clavier, souris, presse-papiers, mise à l’échelle et reconnexion *(plugin)*
- RDP sur plusieurs écrans avec profils de qualité réseau *(plugin)*
- VNC sur VeNCrypt TLS *(plugin)*
- RDP et VNC via des hôtes de rebond SSH *(plugin)*
- Applications WSLg dans un visualiseur intégré (Windows)

### Outils d’administration

- Sur la machine locale et tout serveur SSH
- CPU, mémoire, swap, disques, réseau, charge et latence, avec courbes d’évolution
- Processus (signaux, suspension/reprise, nice), services (systemd, launchd, BSD, Windows), tâches planifiées et paquets, avec mises à niveau et désinstallations winget sur Windows
- Journaux avec préréglages et suivi, ports en écoute avec indications d’exposition et systèmes de fichiers avec alertes de manque d’espace
- Conteneurs Docker : état, ports, démarrage, arrêt, redémarrage et journaux
- Surveillance GPU/NPU : NVIDIA, AMD ROCm, Hygon DCU, Huawei Ascend, Intel XPU, Moore Threads MUSA et Cambricon
- Sessions tmux, Zellij et Herdr : attacher, créer, diviser et renommer
- Tableaux de bord du pool et de la matrice de connexions ; latence SSH dans la barre d’outils du terminal

### IA OxideSens

- OpenAI (dont Responses API), Anthropic, Gemini, DeepSeek, xAI Grok, Xiaomi MiMo, Ollama ou endpoint compatible OpenAI, avec réglage de l’effort de raisonnement
- Contexte issu des terminaux, fichiers et paramètres sélectionnés ; outils pour exécuter des commandes, piloter des programmes interactifs, modifier des fichiers et transférer des données
- Modes lecture seule, par défaut ou sans approbation, permissions par catégorie d’outils, règles de commandes et examen renforcé des actions destructrices ou liées aux identifiants
- Images jointes, branches de conversation, modification des messages et questions suivantes mises en attente
- Sous-agents en parallèle, tâches en arrière-plan et reprise des tâches après interruption
- Codex et Claude Code via ACP ; lanceurs OpenCode, Antigravity, Cursor, Grok Build, GitHub Copilot CLI, Qwen Code et Kimi CLI *(plugin)*
- Serveurs MCP via stdio, Streamable HTTP ou SSE historique
- Contrôle MCP externe : éditeurs et autres outils IA utilisent vos connexions via OxideTerm, avec permissions révocables par groupe d’outils
- [Agent Skills](../../docs/agent-skills.md) (`SKILL.md`) avec complétion `/skill-name`
- Base de notes Markdown avec recherche BM25 et vectorielle HNSW, segmentation CJK et diagrammes Mermaid dans les aperçus

### Sécurité

- Verrouillage avec Touch ID (macOS) ou Windows Hello (Windows), sans arrêter les connexions
- Mots de passe sudo/su proposés aux invites, limités à la machine locale, une connexion ou un motif d’hôte ; TOTP facultatif pour les invites à deux étapes
- Audit des opérations : filtres, durée de conservation et export JSON ou CSV
- Sessions enregistrées chiffrées avec lecture, déplacement dans la chronologie et vitesse réglable
- Examen des permissions avant l’activation de chaque plugin

### Synchronisation et sauvegarde

- Synchronisation via WebDAV, stockage compatible S3 (dont Alibaba Cloud OSS), dépôt Git, GitHub Gist, Dropbox, OneDrive, Google Drive ou HTTP JSON
- Choix des données synchronisées, fusion des modifications de plusieurs appareils et résolution de chaque conflit
- Sauvegardes de restauration et récupération après synchronisation interrompue
- Paquets `.oxide` chiffrés pour connexions, redirections, paramètres, commandes rapides et clés ; ignorer, renommer, remplacer ou fusionner à l’import
- Mode portable exécuté depuis un dossier, avec son propre magasin de clés chiffré
- Mises à jour dans l’application sur le canal stable ou bêta

### Personnalisation

- Apparence claire, sombre ou système, thèmes distincts pour l’application et le terminal, et éditeur de thèmes
- Fonds en images, animations ou vidéos, effets générés de brouillard, particules, météores et caustiques aquatiques ; packs supplémentaires *(plugin)*
- Opacité et style des coins de fenêtre
- Piles de polices avec polices CJK distinctes, ligatures et graisses ; typographie propre à l’éditeur
- Échelle de l’interface, style du curseur, marges internes et animations
- Raccourcis personnalisables et paramètres consultables par recherche et réorganisables
- 11 langues d’interface, saisie IME et texte bidirectionnel

### Plugins

| Type | Exécution | Accès |
|---|---|---|
| **Manifeste** | Extension déclarative | Aucun ; pas de code exécuté |
| **WASM** | Wasmtime/WASI ou processus auxiliaire | Seulement les capacités accordées par l’hôte |
| **Processus** | Processus local ordinaire | Même accès qu’un programme local ; sans bac à sable du système |

N’installez des plugins de type processus que depuis des sources fiables. Les plugins pour OxideTerm 1.x peuvent encore figurer dans la liste, mais 2.x ne les exécute pas.

### Outil en ligne de commande

Installez `oxideterm` depuis **Paramètres → Général → CLI Companion**. Il partage la configuration de l’application pour vérifier, automatiser et sauvegarder sans fenêtre, ou ouvrir un onglet depuis un shell. Les commandes de modification affichent un plan avec `--dry-run`, ne s’appliquent qu’avec `--yes` et conservent une sauvegarde de restauration.

```bash
oxideterm terminal --cwd ~/project
oxideterm doctor --strict
oxideterm connections search prod
oxideterm forwards list --format json
oxideterm oxide export ./profile.oxide --connection prod --password-stdin
```

Complétion pour Bash, Zsh, Fish, PowerShell et Elvish. Le [guide CLI (anglais)](../../docs/user-guide/en/cli.md) présente toutes les commandes.

---

<a id="install"></a>

## Installation

Téléchargez OxideTerm depuis la [dernière version](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest).

| Système | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG (Intel) | DMG (Apple Silicon) |
| **Windows** | Installateur (`.exe`) | Installateur (`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

La page de version propose aussi des archives portables, signatures et un fichier `sha256sums.txt` pour vérifier le téléchargement.

### macOS

Si Gatekeeper bloque l’application, supprimez l’attribut de quarantaine :

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

Si SmartScreen affiche un avertissement, choisissez **Informations complémentaires → Exécuter quand même**.

### Linux

```bash
# AppImage
chmod +x OxideTerm_*_linux_*.AppImage && ./OxideTerm_*_linux_*.AppImage

# Debian / Ubuntu
sudo dpkg -i OxideTerm_*_linux_*.deb && sudo apt-get install -f

# Fedora / RHEL-compatible
sudo dnf install ./OxideTerm_*_linux_*.rpm

# Nix; updates are managed by Nix
nix run github:AnalyseDeCircuit/oxideterm
```

### Si la fenêtre ne s’affiche pas

Définissez `OXIDETERM_RENDER_PROFILE=compatibility` dans l’environnement puis relancez OxideTerm.

### Premiers pas

1. Ouvrez le **gestionnaire de sessions** et ajoutez un serveur, ou importez les hôtes de `~/.ssh/config`.
2. Connectez-vous pour ouvrir un terminal.
3. Depuis le même serveur, ouvrez SFTP, les redirections ou l’IDE.
4. Pour utiliser OxideSens, ajoutez un fournisseur IA dans les **paramètres**.

La [documentation](https://oxideterm.app) présente le reste de l’application. Pour compiler, consultez [Compiler depuis les sources](#build-from-source).

---

## Confidentialité et sécurité

- Mots de passe et clés enregistrés dans le trousseau système (macOS Keychain, Windows Credential Manager, libsecret sur Linux). Les clés des fournisseurs IA sont exclues des journaux.
- Les secrets en mémoire sont effacés par zeroize à leur libération.
- Clés d’hôte approuvées à la première utilisation puis vérifiées avec `~/.ssh/known_hosts` ; une clé modifiée est refusée.
- Le texte envoyé à un fournisseur IA est d’abord filtré pour masquer les identifiants.
- Enregistrement des sessions désactivé par défaut, chiffré sur votre appareil, exclu de la synchronisation et sans capture des frappes.
- Notifications et audit désactivés par défaut. Une fois activés, les données restent sur votre appareil, les détails sensibles sont chiffrés et seules les opérations internes à OxideTerm sont couvertes.
- Paquets `.oxide` chiffrés avec ChaCha20-Poly1305 et une clé dérivée par Argon2id (256 MB, 4 itérations).

---

<a id="build-from-source"></a>

## Compiler depuis les sources

Rust est nécessaire (rustup installe la version fixée dans `rust-toolchain.toml`), ainsi qu’un environnement de bureau pouvant exécuter GPUI.

```bash
# Run the app
cargo run

# Build the oxideterm CLI
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

Avec Nix : `nix build .#oxideterm`, `nix run .#oxideterm` ou `nix develop`. Le binaire CLI est placé dans `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`.

### Organisation

Interface, terminaux et SSH partagent un processus Rust. L’agent distant facultatif et les auxiliaires de plateforme sont séparés. Les sorties SSH ou PTY local entrent directement dans `TerminalState`, dessiné par GPUI, sans WebView, pont JSON ni xterm.js.

`NodeRouter` distribue les connexions du pool `SshConnectionRegistry` basé sur DashMap. Terminaux, SFTP, redirections et éditeur partagent ainsi une connexion par serveur, sauf terminal configuré pour une connexion dédiée. Les états sont `connecting → active → idle → link_down → reconnecting`. Une panne de l’hôte de rebond place les serveurs en aval en `link_down`. Chaque règle de redirection a son écouteur. Après reconnexion, les sessions SFTP sont réacquises ; une opération de l’ancienne connexion n’est jamais transférée silencieusement.

```text
crates/
├── oxideterm-gpui-app/        # Desktop app: windows, workspace, settings
├── oxideterm-gpui-terminal/   # Terminal panes: input, selection, overlays
├── oxideterm-terminal/        # Terminal model and escape-sequence parsing
├── oxideterm-ssh/             # SSH connections, NodeRouter, jump hosts
├── oxideterm-sftp/            # SFTP sessions and transfers
├── oxideterm-forwarding/      # Port forwarding runtime
├── oxideterm-gpui-ide/        # Remote IDE
├── oxideterm-ai/              # OxideSens: providers, MCP, RAG
├── oxideterm-cli/             # The oxideterm command
├── oxideterm-i18n/            # Locale catalogs for the 11 interface languages
└── gpui-ce/                   # Patched copy of GPUI
```

À lire (anglais) : [guide de développement](../../docs/development/README.md), [carte des crates](../../docs/development/crate-map.md) et [architecture](../../docs/user-guide/en/architecture.md).

---

## Contribuer

Code, documentation, traductions, plugins, tests et reproductions de bugs sont bienvenus. Ouvrez un ticket avant une modification importante. [CONTRIBUTING.md](../../docs/CONTRIBUTING.md) explique la préparation d’une pull request.

Un rapport de bug accompagné d’un diagnostic dont les données sensibles sont masquées facilite la correction :

```bash
oxideterm report --bundle ./oxideterm-report.zip
```

Depuis les sources, utilisez plutôt `cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip`.

### Contributeurs

Merci à toutes les personnes qui contribuent à améliorer OxideTerm.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="Contributeurs d’OxideTerm">
  </a>
</p>

---

## Remerciements

OxideTerm repose sur ces projets open source :

- [russh](https://github.com/warp-tech/russh) et [ring](https://github.com/briansmith/ring) : protocole SSH et cryptographie
- [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) : framework d’interface GPU de Zed
- [alacritty_terminal](https://github.com/alacritty/alacritty) : émulation de terminal (VT100–VT500)
- [portable-pty](https://github.com/wez/wezterm/tree/main/pty) : shells locaux, via ConPTY sur Windows
- [Tokio](https://tokio.rs) : exécution asynchrone
- [Wasmtime](https://wasmtime.dev/) : exécution des plugins WASM
- [tree-sitter](https://tree-sitter.github.io/) : coloration syntaxique de l’éditeur

---

## Licence

OxideTerm est sous licence [GPL-3.0-only](../../LICENSE), sans restriction supplémentaire. Les mentions des dépendances figurent dans [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) et [NOTICE](../../NOTICE).

Utilisez OxideTerm uniquement sur vos systèmes ou ceux auxquels vous êtes autorisé à accéder. [LEGAL.md](../../LEGAL.md) contient la notice légale complète.

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
