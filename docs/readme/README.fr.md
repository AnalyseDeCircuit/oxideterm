<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm : vos serveurs, un seul espace de travail" width="920">

# ⚡ OxideTerm

**Un client SSH natif gratuit et un espace de travail pour administrer vos serveurs à distance, avec un assistant IA utilisant votre propre clé.**

SSH · Mosh · Telnet · série · RDP/VNC · SFTP · redirection de ports · éditeur intégré, dans une seule application avec rendu GPU.
Sans compte. Sans abonnement. Sans télémétrie. Sans Electron.

[![Dernière version](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![Plateformes](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![Licence](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Étoiles](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**Télécharger**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**Documentation**](https://oxideterm.app) ·
[**Journal des modifications**](../../.github/release-notes/stable-changelog.md) ·
[**Signaler un problème**](https://github.com/AnalyseDeCircuit/oxideterm/issues)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md)

</div>

---

## Premiers pas

1. **Installez OxideTerm.** Téléchargez un paquet de la [dernière version](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ; les indications par plateforme figurent dans la section [Installation](#install) ci-dessous.
2. **Ajoutez un serveur.** Ouvrez le gestionnaire de sessions et créez une connexion SSH, ou importez des hôtes depuis votre fichier `~/.ssh/config`.
3. **Connectez-vous.** Ouvrez un terminal. Les clés d’hôte sont vérifiées à partir de `~/.ssh/known_hosts`.
4. **Utilisez les autres outils de l’espace de travail.** Ouvrez SFTP, la redirection de ports ou l’éditeur intégré sur le même nœud. Par défaut, ils partagent une seule connexion SSH.
5. **Facultatif : activez l’IA.** Dans les paramètres, ajoutez votre propre point d’accès OpenAI, Anthropic, Gemini, Ollama ou compatible OpenAI pour activer OxideSens.

Pour une visite guidée, consultez la [documentation](https://oxideterm.app).

---

## Fonctionnalités

| | |
|---|---|
| **Terminaux et protocoles** | Shells locaux, SSH, Mosh, Telnet, série, volets divisés, routes à plusieurs sauts, agent SSH et transfert d’agent, authentification à deux facteurs (2FA) et identifiants TOTP, transfert X11, intégration du shell, repères de commandes, journaux de session configurables, enregistrement, graphiques Sixel et Kitty, transferts trzsz |
| **tmux et diffusion** | Mode de contrôle natif `tmux -CC` avec disposition des volets et séparateurs déplaçables, groupes de diffusion nommés et outil avancé d’envoi de commandes à plusieurs cibles pour des saisies planifiées et répétables |
| **Fiabilité** | La reconnexion Grace Period maintient les applications TUI en fonctionnement pendant de brèves coupures réseau, puis rétablit les redirections, les transferts et les fichiers ouverts dans l’éditeur |
| **Fichiers et édition** | Gestionnaire SFTP à deux volets, files de transfert avec limites de vitesse et estimation du temps restant, favoris, éditeur distant intégré avec écriture sécurisée, gestion des conflits et restauration de l’espace de travail |
| **Réseau** | Redirections locales, distantes et dynamiques SOCKS5, règles enregistrées, détection des ports distants, topologie des connexions et débogage ponctuel de sockets |
| **Bureau à distance** | RDP et VNC intégrés avec prise en charge du presse-papiers et des entrées |
| **Administration des hôtes** | Surveillance des processus, services, journaux, ports, tâches, disques, paquets, conteneurs et tmux |
| **IA et automatisation** | OxideSens avec votre propre clé, MCP, RAG local, Agent Skills, actions approuvées dans l’espace de travail et CLI autonome |
| **Suivi et audit** | Espace de travail facultatif de notifications et d’audit, et enregistrements de sessions chiffrés (tous deux désactivés par défaut) |
| **Synchronisation et portabilité** | Synchronisation cloud chiffrée, paquets `.oxide` portables |
| **Personnalisation** | Thèmes, images d’arrière-plan, raccourcis configurables, commandes rapides, 11 langues d’interface |

---

## Pourquoi OxideTerm

- **Gratuit et conçu pour un usage local.** Sans compte, sans abonnement, sans télémétrie. Vous gardez le contrôle de vos connexions et de vos données d’administration.
- **Un espace de travail par serveur.** Terminal, SFTP, redirections, RDP/VNC, éditeur, surveillance et IA sont rattachés au même nœud, plutôt que de fonctionner comme des outils isolés.
- **Une véritable application native.** L’interface est dessinée directement sur le GPU avec [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui). Sans Electron ni WebView embarquée.
- **L’IA selon vos choix.** OxideSens utilise votre fournisseur et votre clé, et n’exécute que les actions que vous approuvez.
- **Des connexions résilientes.** La reconnexion Grace Period teste l’ancienne connexion pendant 30 secondes avant de la remplacer, pour que les applications TUI puissent continuer à fonctionner lors de brèves coupures réseau.
- **SSH entièrement en Rust.** La pile SSH utilise `russh` avec `ring`, sans OpenSSL ni libssh2.

---

## Utilisation de la mémoire

**La réécriture native a réduit la mémoire utilisée au repos à environ un quart de celle de l’ancienne version sur macOS, et à environ un huitième sur Windows.** Voici les observations consignées par le mainteneur lors du passage de Tauri 1.x à la version native GPUI 2.0 :

| Plateforme | Tauri 1.x (au repos) | Version native 2.0 (au repos) | Réduction |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | Environ 74% |
| Windows | 182.4 MB | 23.5 MB | Environ 87% |

Le total de l’ancienne version comprend OxideTerm et les processus WebView associés. La version native n’a plus besoin de ces processus de navigateur.

![Comparaison de la mémoire au repos avec captures des processus système : Tauri 1.x et version native 2.0](../../docs/screenshots/oxideterm-memory-comparison.png)

---

## Captures d’écran

| Terminal SSH avec OxideSens | Gestionnaire de fichiers SFTP |
|---|---|
| ![Terminal SSH avec l’IA OxideSens](../../docs/screenshots/terminal/SSHTERMINAL.png) | ![Gestionnaire de fichiers SFTP à deux volets avec file de transfert](../../docs/screenshots/sftp/sftp.png) |

| IDE intégré | Redirection de ports intelligente |
|---|---|
| ![Mode IDE intégré](../../docs/screenshots/miniIDE/miniide.png) | ![Redirection de ports intelligente avec détection automatique](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png) |

<details>
<summary><b>Voyez OxideSens ouvrir un terminal à partir d’une demande en langage naturel</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens ouvre un terminal dans OxideTerm" width="720">
</a>

</details>

---

## IA OxideSens

OxideSens est un assistant facultatif qui peut examiner vos sessions actives et effectuer des actions dans l’espace de travail **uniquement après votre approbation**.

- **Utilisez votre propre clé.** Compatible avec OpenAI, Anthropic (Claude), Google Gemini, Ollama et tout point d’accès compatible OpenAI, avec des réglages de raisonnement adaptés au fournisseur. Il n’y a pas de crédits de plateforme.
- **MCP et Agent Skills.** Connectez des serveurs MCP (stdio et SSE) et chargez des Agent Skills dans des limites définies.
- **Base de connaissances locale (RAG).** Recherche plein texte BM25 et index vectoriel.
- **Vous contrôlez le contexte.** Vous choisissez le contexte de l’espace de travail et les actions à approuver ; les règles de la politique de commandes s’appliquent.
- **Masquage des identifiants.** Les messages envoyés à un fournisseur passent par un filtre qui masque les motifs correspondant à des identifiants.
- **Les clés restent dans le trousseau du système d’exploitation** et ne figurent pas dans les journaux structurés.

---

<a id="plugins"></a>

## Plugins

OxideTerm prend en charge trois types de plugins :

| Type | Exécution | Limites |
|---|---|---|
| **Manifeste uniquement** | Extensions déclaratives, sans code | Aucun code exécutable |
| **WASM** | Wasmtime/WASI ou processus auxiliaire | Appels à l’hôte contrôlés, limités aux capacités autorisées |
| **Processus** | Processus local ordinaire | Code local de confiance, **sans** bac à sable du système d’exploitation |

Les anciens plugins ESM de Tauri (1.x) peuvent apparaître dans la liste, mais l’application native 2.x ne les exécute pas. N’installez des plugins de type processus que depuis des sources de confiance.

---

## Sécurité et confidentialité

| Sujet | Fonctionnement |
|---|---|
| **Identifiants enregistrés** | Trousseau du système d’exploitation (macOS Keychain, Windows Credential Manager, libsecret) |
| **Secrets en mémoire** | Les types contenant des secrets et les tampons temporaires utilisent `zeroize` aux points de gestion de propriété pris en charge |
| **Clés d’hôte** | Confiance à la première utilisation avec `~/.ssh/known_hosts` ; les modifications inattendues sont refusées |
| **Exports portables** | Les paquets `.oxide` utilisent ChaCha20-Poly1305 avec Argon2id (256 MB de mémoire, 4 itérations) |
| **Contexte de l’IA** | Masquage des motifs d’identifiants avant tout envoi à un fournisseur ; vous approuvez le contexte et les actions |
| **Enregistrements de sessions** | Désactivés par défaut ; stockés chiffrés sur votre appareil et exclus de la synchronisation cloud ; les frappes au clavier ne sont pas enregistrées |
| **Audit** | Désactivé par défaut ; les données restent sur votre appareil et les détails sensibles sont chiffrés |
| **Modifications via la CLI** | Plans de simulation, confirmations `--yes` et sauvegardes de restauration pour les commandes qui modifient l’état |
| **Plugins** | Voir [Plugins](#plugins) |
| **Télémétrie** | Aucune |

**Utilisation légale.** OxideTerm est distribué sous licence GPL-3.0-only, sans restrictions supplémentaires. Accédez uniquement aux systèmes, réseaux et appareils qui vous appartiennent ou pour lesquels vous disposez d’une autorisation explicite, et respectez la législation applicable. N’utilisez pas OxideTerm pour des accès non autorisés, pour perturber des services ou pour contourner des contrôles d’accès.

---

## Limites actuelles

À savoir avant l’installation :

- Disponible uniquement sur ordinateur (macOS, Windows, Linux). Il n’existe pas d’application mobile.
- Le projet évolue rapidement, avec des versions fréquentes. Consultez le [journal des modifications](../../.github/release-notes/stable-changelog.md) et les [tickets ouverts](https://github.com/AnalyseDeCircuit/oxideterm/issues).
- L’audit et l’enregistrement des sessions doivent être activés explicitement et ne reflètent que ce qu’OxideTerm peut lui-même observer.
- Les plugins de type processus ne sont pas isolés par un bac à sable du système d’exploitation.
- Si le moteur de rendu ne fonctionne pas sur votre machine, essayez le profil de compatibilité : `OXIDETERM_RENDER_PROFILE=compatibility`.

---

<a id="for-developers"></a>

## Pour les développeurs

<details>
<summary><b>Exécuter depuis les sources</b></summary>

**Prérequis :** une chaîne d’outils Rust (édition 2024) et un environnement de bureau capable d’exécuter GPUI.

```bash
# Run the app
cargo run

# If the renderer fails on your machine
OXIDETERM_RENDER_PROFILE=compatibility cargo run

# Build the headless CLI companion
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

Avec Nix : `nix build .#oxideterm`, `nix run .#oxideterm` ou `nix develop`.

Les binaires de la CLI sont placés dans `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`.

</details>

<details>
<summary><b>Interface en ligne de commande</b></summary>

La CLI sans interface graphique `oxideterm` fonctionne sans lancer l’application, ce qui est utile pour l’automatisation, l’intégration continue et les diagnostics. Elle couvre les paramètres, connexions, redirections, plugins, commandes rapides, secrets, paquets portables, diagnostics, rapports, plans de traitement par lots, sauvegardes et synchronisation cloud.

```bash
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- forwards list --format json
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- oxide export ./profile.oxide --connection prod --password-stdin
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
cargo run -p oxideterm-cli -- completion install zsh --force

# Path and profile isolation for CI or fixtures
cargo run -p oxideterm-cli -- --config-dir ./fixture-config doctor --strict
```

</details>

<details>
<summary><b>Architecture</b></summary>

L’interface et le backend terminal/SSH partagent un même processus Rust ; les agents distants facultatifs et les auxiliaires de plateforme fonctionnent à l’extérieur de ce processus. Les octets du terminal modifient directement `TerminalState`, à partir duquel GPUI effectue le rendu, sans étape d’analyse JSON, WebSocket, Base64 ou xterm.js.

```
┌─────────────────────────────────────────────────┐
│               GPUI Render Loop                  │
│   WorkspaceApp  ·  Tab surfaces  ·  GPUI views  │
└──────────────────────┬──────────────────────────┘
                       │  in-process Arc<> / async
┌──────────────────────▼──────────────────────────┐
│             Domain Crates (Rust async)          │
│  NodeRouter → SshConnectionRegistry             │
│  TerminalState ← SSH PTY channel (russh)        │
│  SftpSession · ForwardingRuntime · IdeWorkspace │
│  Ai/ACP Entities · CloudSync · Plugin Runtimes  │
└─────────────────────────────────────────────────┘
```

| Aspect | Approche avec navigateur embarqué | OxideTerm |
|---|---|---|
| Rendu | Moteur de navigateur et mise en page web | GPUI sur une surface GPU |
| Flux des données du terminal | WebSocket → boucle d’événements JS → xterm.js | Entrée Rust → `TerminalState` → rendu GPUI |
| Cycle de vie des connexions | Réparti entre frontend et backend | Une seule chaîne de connexion et de reconnexion dans le même processus |
| Contexte de l’IA | Copié via un pont applicatif | Construit à partir de l’espace de travail actif avec l’approbation de l’utilisateur |
| CLI | Nécessite que l’application de bureau soit lancée | Binaire autonome, liaison directe avec les crates |

**Pool de connexions.** `SshConnectionRegistry` repose sur `DashMap` et s’utilise via `NodeRouter`. Les volets de terminal, SFTP, les redirections de ports et l’éditeur peuvent partager une connexion SSH physique par nœud ; une politique de terminal peut aussi opter pour une connexion dédiée. Chaque connexion suit les états `connecting → active → idle → link_down → reconnecting`. Une panne de l’hôte de rebond place les nœuds en aval dans l’état `link_down`. L’IA et les plugins utilisent des références de capacités et des instantanés des hôtes plutôt que de s’enregistrer comme consommateurs de connexion.

**Reconnexion Grace Period.**

1. Détecter l’expiration du délai keepalive.
2. Prendre un instantané des volets de terminal, transferts SFTP, redirections et fichiers de l’éditeur.
3. Tester l’ancienne connexion pendant 30 s pour que les applications TUI puissent continuer à fonctionner lors de brèves coupures réseau.
4. Ouvrir une nouvelle connexion, rétablir les redirections, reprendre les transferts et rouvrir les fichiers de l’éditeur.

Les sessions SFTP portent une génération de connexion : après une reconnexion, une session admissible est obtenue à nouveau, mais une opération d’une ancienne génération n’est jamais transférée silencieusement vers la nouvelle connexion.

**Redirection de ports.** Une crate autonome prend en charge `-L`, `-R` et `-D` (SOCKS5). Chaque canal SSH appartient à une seule tâche `ssh_io`, ce qui évite un mutex partagé sur le chemin critique.

**SSH entièrement en Rust.** `russh` avec `ring` : SSH2 complet, ChaCha20-Poly1305 et AES-GCM, clés Ed25519/RSA/ECDSA, agent SSH sur Unix (`SSH_AUTH_SOCK`) et Windows (`\\.\pipe\openssh-ssh-agent`), et chaînes à plusieurs sauts avec authentification indépendante à chaque saut.

**Technologies**

| Couche | Technologie |
|---|---|
| Interface | GPUI (framework d’interface de Zed avec rendu GPU) |
| Exécution | Tokio, DashMap |
| SSH | `russh` avec `ring` (sans OpenSSL ni libssh2) |
| PTY local | `portable-pty` (ConPTY sur Windows) |
| Émulation de terminal | `alacritty_terminal` (VT100–VT500, graphiques Sixel et Kitty) |
| Éditeur | Coloration syntaxique tree-sitter, tampon personnalisé |
| Chiffrement | ChaCha20-Poly1305, Argon2id |
| Plugins | Wasmtime/WASI, processus auxiliaire WASM et plugins de type processus |
| Flux de l’IA | SSE (OpenAI, Anthropic, Gemini), dans le même processus |
| RAG | BM25 + index vectoriel HNSW avec fusion des classements, tokenisation CJK par bigrammes |
| i18n | `oxideterm-i18n` (11 langues) |

</details>

---

<a id="install"></a>

## Installation

[**Télécharger la dernière version**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

| Système | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG (Intel) | DMG (Apple Silicon) |
| **Windows** | Installateur (`.exe`) | Installateur (`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

Vérifiez votre téléchargement à l’aide du fichier `sha256sums.txt` disponible sur la page de la version. Les archives portables et les signatures y figurent également.

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

Vous préférez compiler vous-même ? Consultez **Exécuter depuis les sources** dans la section [Pour les développeurs](#for-developers).

---

## Contribuer

Les contributions sont les bienvenues : code Rust, documentation, traductions, plugins, tests et reproduction de problèmes. Pour les changements importants, ouvrez d’abord un ticket afin d’en discuter.

Les rapports de bugs sont plus utiles lorsqu’ils sont accompagnés d’un dossier de diagnostic dont les données sensibles ont été masquées :

```bash
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

Les bugs reproductibles et les régressions sont prioritaires. Les demandes de fonctionnalités sont évaluées selon leur portée, leur sécurité et leur adéquation avec l’objectif d’OxideTerm : un espace de travail pour les serveurs distants. Si OxideTerm vous aide dans votre travail, une étoile sur GitHub, un rapport de bug reproductible, une correction de traduction ou un plugin contribuent à faire avancer le projet.

### Contributeurs

Merci à toutes les personnes qui contribuent à améliorer OxideTerm.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="Contributeurs d’OxideTerm">
  </a>
</p>

---

## Licence

**GPL-3.0-only.** Les mentions relatives aux dépendances figurent dans [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md), avec des mentions supplémentaires dans [`NOTICE`](../../NOTICE).

**Développé avec :** [russh](https://github.com/warp-tech/russh) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [alacritty_terminal](https://github.com/alacritty/alacritty) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [wasmtime](https://wasmtime.dev/) · [tree-sitter](https://tree-sitter.github.io/)
