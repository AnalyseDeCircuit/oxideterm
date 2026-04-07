<p align="center">
  <img src="../../src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
  <br>
  <em>Si vous aimez OxideTerm, s'il vous plaît donnez-lui une étoile sur GitHub ⭐️ !</em>
</p>


<p align="center">
  <strong>Zéro Electron. Zéro OpenSSL. Zéro télémétrie. Zéro abonnement. SSH pur Rust.</strong>
  <br>
  <em>Un seul binaire natif — shells locaux, SSH, SFTP, IDE distant, IA, redirection de ports, plugins, 30+ thèmes, 11 langues.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.1.0--beta.6-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plateforme">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licence">
  <img src="https://img.shields.io/badge/rust-1.85+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
  <img src="https://img.shields.io/github/downloads/AnalyseDeCircuit/oxideterm/total?color=brightgreen" alt="Téléchargements totaux">
</p>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases/latest">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=Télécharger%20la%20dernière%20version&style=for-the-badge&color=brightgreen" alt="Télécharger la dernière version">
  </a>
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?include_prereleases&label=Télécharger%20la%20dernière%20bêta&style=for-the-badge&color=orange" alt="Télécharger la dernière bêta">
  </a>
</p>

<p align="center">
  🌐 <strong><a href="https://oxideterm.app">oxideterm.app</a></strong> — Documentation & website
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

> [!NOTE]
> **Changement de licence :** À partir de la v1.0.0, OxideTerm a changé sa licence de **PolyForm Noncommercial 1.0.0** à **GPL-3.0 (GNU General Public License v3.0)**. OxideTerm est désormais entièrement open source — vous êtes libre de l'utiliser, le modifier et le distribuer selon les termes de la licence GPL-3.0. Voir le fichier [LICENSE](../../LICENSE) pour plus de détails.

---

<div align="center">

https://github.com/user-attachments/assets/4ba033aa-94b5-4ed4-980c-5c3f9f21db7e

*🤖 OxideSens AI — « Ouvre un terminal local et exécute echo hello, world ! »*

</div>

---

## Pourquoi OxideTerm ?

| Problème | La réponse d'OxideTerm |
|---|---|
| Les clients SSH ne font pas de shells locaux | **Moteur hybride** : PTY local (zsh/bash/fish/pwsh/WSL2) + SSH distant dans une seule fenêtre |
| Reconnexion = tout perdu | **Reconnexion avec période de grâce** : sonde l'ancienne connexion 30 s avant de la couper — vos vim/htop/yazi survivent |
| L'édition distante nécessite VS Code Remote | **IDE intégré** : CodeMirror 6 via SFTP avec 30+ langages, agent distant optionnel (~1 Mo) sur Linux |
| Pas de réutilisation de connexion SSH | **Multiplexage** : terminal, SFTP, redirections, IDE partagent une seule connexion SSH via un pool à comptage de références |
| Les bibliothèques SSH dépendent d'OpenSSL | **russh 0.59** : SSH pur Rust compilé avec `ring` — zéro dépendance C |
| Applications Electron de 100+ Mo | **Tauri 2.0** : backend Rust natif, binaire de 25–40 Mo |
| IA verrouillée sur un fournisseur | **OxideSens** : 40+ outils, protocole MCP, base de connaissances RAG — fonctionne avec OpenAI/Ollama/DeepSeek/toute API compatible |
| Identifiants stockés en clair | **Trousseau système uniquement** : mots de passe et clés API jamais écrits sur disque ; fichiers `.oxide` chiffrés ChaCha20-Poly1305 + Argon2id |
| Outils dépendants du cloud et nécessitant un compte | **Local d'abord** : zéro compte, zéro télémétrie, zéro synchronisation cloud — vos données restent sur votre machine. Clé AI à fournir soi-même |

---

## Captures d'écran

<table>
<tr>
<td align="center"><strong>Terminal SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminal SSH avec barre latérale OxideSens AI" /></td>
<td align="center"><strong>Gestionnaire de fichiers SFTP</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="Gestionnaire de fichiers SFTP double volet avec file de transfert" /></td>
</tr>
<tr>
<td align="center"><strong>IDE intégré (CodeMirror 6)</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Mode IDE intégré avec éditeur CodeMirror 6" /></td>
<td align="center"><strong>Redirection de ports intelligente</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Redirection de ports intelligente avec détection automatique" /></td>
</tr>
</table>

---

## Aperçu des fonctionnalités

| Catégorie | Fonctionnalités |
|---|---|
| **Terminal** | PTY local (zsh/bash/fish/pwsh/WSL2), SSH distant, panneaux divisés, diffusion d'entrée, enregistrement/lecture de sessions (asciicast v2), rendu WebGL, 30+ thèmes + éditeur personnalisé, palette de commandes (`⌘K`), mode zen |
| **SSH & Auth** | Pool de connexions & multiplexage, ProxyJump (sauts illimités) avec graphe topologique, reconnexion automatique avec période de grâce, Transfert d'agent. Auth : mot de passe, clé SSH (RSA/Ed25519/ECDSA), SSH Agent, certificats, 2FA interactif clavier, Known Hosts TOFU |
| **SFTP** | Navigateur double volet, glisser-déposer, aperçu intelligent (images/vidéo/audio/code/PDF/hex/polices), file de transfert avec progression & ETA, signets, extraction d'archives |
| **Mode IDE** | CodeMirror 6 avec 30+ langages, arborescence + statut Git, multi-onglets, résolution de conflits, terminal intégré. Agent distant optionnel pour Linux (9 architectures supplémentaires) |
| **Redirection de ports** | Local (-L), distant (-R), SOCKS5 dynamique (-D), I/O par passage de messages sans verrou, restauration automatique à la reconnexion, rapport d'arrêt, délai d'inactivité |
| **IA (OxideSens)** | Panneau inline (`⌘I`) + chat latéral, capture du buffer terminal (panneau unique/tous), contexte multi-sources (IDE/SFTP/Git), 40+ outils autonomes, intégration serveur MCP, base de connaissances RAG (recherche hybride BM25 + vecteurs), streaming SSE |
| **Plugins** | Chargement ESM en runtime, 18 espaces de noms API, 24 composants UI Kit, API gelée + ACL Proxy, disjoncteur, désactivation automatique en cas d'erreurs |
| **CLI** | Companion `oxt` : JSON-RPC 2.0 via Unix Socket / Named Pipe, `status`/`list`/`ping`, sortie humaine + JSON |
| **Sécurité** | Export chiffré .oxide (ChaCha20-Poly1305 + Argon2id 256 Mo), trousseau OS, Touch ID (macOS), TOFU clé hôte, nettoyage mémoire `zeroize` |
| **i18n** | 11 langues : EN, 简体中文, 繁體中文, 日本語, 한국어, FR, DE, ES, IT, PT-BR, VI |

---

## Sous le capot

### Architecture — Communication à double plan

OxideTerm sépare les données du terminal des commandes de contrôle en deux plans indépendants :

```
┌─────────────────────────────────────┐
│        Frontend (React 19)          │
│  xterm.js 6 (WebGL) + 19 stores    │
└──────────┬──────────────┬───────────┘
           │ Tauri IPC    │ WebSocket (binaire)
           │ (JSON)       │ port par session
┌──────────▼──────────────▼───────────┐
│         Backend (Rust)              │
│  NodeRouter → SshConnectionRegistry │
│  Wire Protocol v1                   │
│  [Type:1][Length:4][Payload:n]       │
└─────────────────────────────────────┘
```

- **Plan de données (WebSocket)** : chaque session SSH obtient son propre port WebSocket. Les octets du terminal circulent sous forme de trames binaires avec un en-tête Type-Length-Payload — aucune sérialisation JSON, aucun encodage Base64, zéro surcharge sur le chemin critique.
- **Plan de contrôle (Tauri IPC)** : gestion des connexions, opérations SFTP, redirections, configuration — JSON structuré, mais hors du chemin critique.
- **Adressage par nœud** : le frontend ne touche jamais `sessionId` ni `connectionId`. Tout est adressé par `nodeId`, résolu atomiquement côté serveur par le `NodeRouter`. La reconnexion SSH modifie le `connectionId` sous-jacent — mais SFTP, IDE et redirections ne sont absolument pas affectés.

### 🔩 SSH pur Rust — russh 0.59

L'intégralité de la pile SSH est **russh 0.59** compilée avec le backend cryptographique **`ring`** :

- **Zéro dépendance C/OpenSSL** — toute la pile crypto est en Rust. Plus de débogage « quelle version d'OpenSSL ? ».
- Protocole SSH2 complet : échange de clés, canaux, sous-système SFTP, redirection de ports
- Suites de chiffrement ChaCha20-Poly1305 et AES-GCM, clés Ed25519/RSA/ECDSA
- **`AgentSigner`** personnalisé : encapsule le SSH Agent système et implémente le trait `Signer` de russh, résolvant les problèmes de bornes `Send` RPITIT en clonant `&AgentIdentity` vers une valeur possédée avant de traverser `.await`

```rust
pub struct AgentSigner { /* wraps system SSH Agent */ }
impl Signer for AgentSigner { /* challenge-response via Agent IPC */ }
```

- **Support plateforme** : Unix (`SSH_AUTH_SOCK`), Windows (`\\.\pipe\openssh-ssh-agent`)
- **Chaînes proxy** : chaque saut utilise indépendamment l'authentification Agent
- **Reconnexion** : `AuthMethod::Agent` rejoué automatiquement

### 🔄 Reconnexion intelligente avec période de grâce

La plupart des clients SSH détruisent tout à la déconnexion et repartent de zéro. L'orchestrateur de reconnexion d'OxideTerm adopte une approche fondamentalement différente :

1. **Détection** du timeout heartbeat WebSocket (300 s, calibré pour macOS App Nap et le throttling des timers JS)
2. **Snapshot** de l'état complet : panneaux terminal, transferts SFTP en cours, redirections de ports actives, fichiers IDE ouverts
3. **Sondage intelligent** : événements `visibilitychange` + `online` déclenchent un keepalive SSH proactif (~2 s de détection contre 15–30 s en timeout passif)
4. **Période de grâce** (30 s) : sonde l'ancienne connexion SSH via keepalive — si elle se rétablit (ex. : changement de point d'accès WiFi), vos applications TUI (vim, htop, yazi) survivent intégralement
5. En cas d'échec de récupération → nouvelle connexion SSH → restauration automatique des redirections → reprise des transferts SFTP → réouverture des fichiers IDE

Pipeline : `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

Toute la logique passe par un `ReconnectOrchestratorStore` dédié — zéro code de reconnexion dispersé dans les hooks ou composants.

### 🛡️ Pool de connexions SSH

`SshConnectionRegistry` à comptage de références s'appuyant sur `DashMap` pour un accès concurrent sans verrou :

- **Une connexion, plusieurs consommateurs** : terminal, SFTP, redirections de ports et IDE partagent une seule connexion SSH physique — pas de handshakes TCP redondants
- **Machine à états par connexion** : `connecting → active → idle → link_down → reconnecting`
- **Gestion du cycle de vie** : délai d'inactivité configurable (5 min / 15 min / 30 min / 1 h / jamais), intervalle keepalive de 15 s, détection de défaillance heartbeat
- **Heartbeat WsBridge** : intervalle de 30 s, timeout de 5 min — tolère macOS App Nap et le throttling JS du navigateur
- **Propagation en cascade** : défaillance de l'hôte de saut → tous les nœuds en aval automatiquement marqués `link_down` avec synchronisation du statut
- **Déconnexion en inactivité** : émet `connection_status_changed` vers le frontend (pas seulement un `node:state` interne), empêchant la désynchronisation de l'interface

### 🤖 OxideSens AI

Assistant IA axé sur la confidentialité avec deux modes d'interaction :

- **Panneau inline** (`⌘I`) : commandes terminal rapides, sortie injectée via bracketed paste
- **Chat latéral** : conversations persistantes avec historique complet
- **Capture de contexte** : le Terminal Registry collecte le buffer du panneau actif ou de tous les panneaux divisés simultanément ; injection automatique des fichiers IDE, chemins SFTP et statut Git
- **40+ outils autonomes** : opérations fichiers, gestion de processus, diagnostics réseau, interaction avec les apps TUI, traitement de texte — l'IA invoque ces outils sans déclenchement manuel
- **Support MCP** : connexion à des serveurs [Model Context Protocol](https://modelcontextprotocol.io) externes (stdio & SSE) pour l'intégration d'outils tiers
- **Base de connaissances RAG** (v0.20) : importez des documents Markdown/TXT dans des collections ciblées (globales ou par connexion). La recherche hybride fusionne index de mots-clés BM25 + similarité cosinus vectorielle via Reciprocal Rank Fusion. Découpage Markdown préservant la hiérarchie des titres. Tokenizer bigramme CJK pour chinois/japonais/coréen.
- **Fournisseurs** : OpenAI, Ollama, DeepSeek, OneAPI, ou tout endpoint `/v1/chat/completions`
- **Sécurité** : clés API stockées dans le trousseau OS ; sur macOS, la lecture des clés est protégée par **Touch ID** via `LAContext` — aucun entitlement ni signature de code requis, mis en cache après la première authentification par session

### 💻 Mode IDE — Édition distante

Éditeur CodeMirror 6 opérant via SFTP — aucune installation côté serveur requise par défaut :

- **Arborescence de fichiers** : chargement paresseux des répertoires avec indicateurs de statut Git (modifié/non suivi/ajouté)
- **24 modes de langage** : 14 natifs CodeMirror + modes hérités via `@codemirror/legacy-modes`
- **Résolution de conflits** : verrouillage optimiste par mtime — détecte les modifications distantes avant l'écrasement
- **Git événementiel** : rafraîchissement automatique à la sauvegarde, création, suppression, renommage et touche Entrée du terminal
- **State Gating** : toutes les E/S bloquées quand `readiness !== 'ready'`, Key-Driven Reset force un remontage complet à la reconnexion
- **Agent distant** (optionnel) : binaire Rust d'environ 1 Mo, déployé automatiquement sur x86_64/aarch64 Linux. 9 architectures supplémentaires (ARMv7, RISC-V64, LoongArch64, s390x, Power64LE, i686, ARM, Android aarch64, FreeBSD x86_64) dans `agents/extra/` pour upload manuel. Active l'arborescence améliorée, la recherche de symboles et la surveillance de fichiers.

### 🔀 Redirection de ports — I/O sans verrou

Redirection locale (-L), distante (-R) et dynamique SOCKS5 (-D) complète :

- **Architecture par passage de messages** : le canal SSH est détenu par une seule tâche `ssh_io` — pas de `Arc<Mutex<Channel>>`, éliminant totalement la contention mutex
- **Rapport d'arrêt** : les tâches de redirection signalent activement la raison de sortie (déconnexion SSH, fermeture du port distant, timeout) pour un diagnostic clair
- **Restauration automatique** : les redirections `Suspended` reprennent automatiquement à la reconnexion sans intervention utilisateur
- **Délai d'inactivité** : `FORWARD_IDLE_TIMEOUT` (300 s) empêche l'accumulation de connexions zombies

### 🔌 Système de plugins en runtime

Chargement ESM dynamique avec une surface API gelée et renforcée en sécurité :

- **API PluginContext** : 18 espaces de noms — terminal, ui, commands, settings, lifecycle, events, storage, system
- **24 composants UI Kit** : composants React préconstruits (boutons, champs de saisie, dialogues, tableaux…) injectés dans les sandboxes de plugins via `window.__OXIDE__`
- **Membrane de sécurité** : `Object.freeze` sur tous les objets de contexte, ACL basée sur Proxy, liste blanche IPC, disjoncteur avec désactivation automatique après erreurs répétées
- **Modules partagés** : React, ReactDOM, zustand, lucide-react exposés pour utilisation par les plugins sans duplication de bundles

### ⚡ Rendu adaptatif

Planificateur de rendu à trois niveaux remplaçant le batching fixe `requestAnimationFrame` :

| Niveau | Déclencheur | Fréquence | Avantage |
|---|---|---|---|
| **Boost** | Données de trame ≥ 4 Ko | 120 Hz+ (ProMotion natif) | Élimine le lag de défilement sur `cat largefile.log` |
| **Normal** | Saisie standard | 60 Hz (RAF) | Base fluide |
| **Inactif** | 3 s sans E/S / onglet masqué | 1–15 Hz (décroissance exponentielle) | Charge GPU quasi nulle, économie de batterie |

Les transitions sont entièrement automatiques — pilotées par le volume de données, les entrées utilisateur et l'API Page Visibility. Les onglets en arrière-plan continuent de vider les données via le timer d'inactivité sans réveiller RAF.

### 🔐 Export chiffré .oxide

Sauvegarde de connexion portable et inviolable :

- Chiffrement authentifié **ChaCha20-Poly1305 AEAD**
- **KDF Argon2id** : coût mémoire de 256 Mo, 4 itérations — résistant au brute-force GPU
- Somme de contrôle d'intégrité **SHA-256**
- **Intégration optionnelle de clés** : clés privées encodées en base64 dans la charge utile chiffrée
- **Analyse préalable** : ventilation des types d'authentification, détection des clés manquantes avant l'export

### 📡 ProxyJump — Multi-saut avec conscience topologique

- Profondeur de chaîne illimitée : `Client → Saut A → Saut B → … → Cible`
- Analyse automatique de `~/.ssh/config`, construction du graphe topologique, algorithme de Dijkstra pour la route optimale
- Nœuds de saut réutilisables comme sessions indépendantes
- Propagation de défaillance en cascade : hôte de saut en panne → tous les nœuds en aval automatiquement marqués `link_down`

### ⚙️ Terminal local — PTY thread-safe

Shell local multiplateforme via `portable-pty 0.8`, protégé par le feature gate `local-terminal` :

- `MasterPty` enveloppé dans `std::sync::Mutex` — des threads d'E/S dédiés gardent les lectures PTY bloquantes hors de la boucle d'événements Tokio
- Détection automatique du shell : `zsh`, `bash`, `fish`, `pwsh`, Git Bash, WSL2
- `cargo build --no-default-features` supprime le PTY pour les builds mobiles/légers

### 🪟 Optimisation Windows

- **ConPTY natif** : invoque directement l'API Windows Pseudo Console — support complet TrueColor et ANSI, pas de WinPTY obsolète
- **Scanner de shells** : détecte automatiquement PowerShell 7, Git Bash, WSL2, CMD via le Registre et le PATH

### Et plus encore

- **Profileur de ressources** : CPU/mémoire/réseau en direct via canal SSH persistant lisant `/proc/stat`, calcul basé sur les deltas, dégradation automatique vers RTT-only sur les systèmes non-Linux
- **Moteur de thèmes personnalisé** : 30+ thèmes intégrés, éditeur visuel avec aperçu en direct, 20 champs xterm.js + 24 variables de couleur UI, dérivation automatique des couleurs UI depuis la palette du terminal
- **Enregistrement de session** : format asciicast v2, enregistrement et lecture complets
- **Diffusion d'entrée** : tapez une fois, envoyez à tous les panneaux divisés — opérations serveur par lots
- **Galerie d'arrière-plans** : images d'arrière-plan par onglet, 16 types d'onglets, contrôle opacité/flou/ajustement
- **Companion CLI** (`oxt`) : binaire d'environ 1 Mo, JSON-RPC 2.0 via Unix Socket / Named Pipe, `status`/`list`/`ping` avec sortie humaine ou `--json`
- **WSL Graphics** ⚠️ expérimental : visionneuse VNC intégrée — 9 environnements de bureau + mode application unique, détection WSLg, Xtigervnc + noVNC

<details>
<summary>📸 11 langues en action</summary>
<br>
<table>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/en.png" width="280"><br><b>English</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/zhHans.png" width="280"><br><b>简体中文</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/zhHant.png" width="280"><br><b>繁體中文</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/ja.png" width="280"><br><b>日本語</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/ko.png" width="280"><br><b>한국어</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/fr.png" width="280"><br><b>Français</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/de.png" width="280"><br><b>Deutsch</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/es.png" width="280"><br><b>Español</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/it.png" width="280"><br><b>Italiano</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/pt-BR.png" width="280"><br><b>Português</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/vi.png" width="280"><br><b>Tiếng Việt</b></td>
    <td></td>
  </tr>
</table>
</details>

---

## Démarrage rapide

### Prérequis

- **Rust** 1.85+
- **Node.js** 18+ (pnpm recommandé)
- **Outils plateforme** :
  - macOS : Xcode Command Line Tools
  - Windows : Visual Studio C++ Build Tools
  - Linux : `build-essential`, `libwebkit2gtk-4.1-dev`, `libssl-dev`

### Développement

```bash
git clone https://github.com/AnalyseDeCircuit/oxideterm.git
cd oxideterm && pnpm install

# Compiler le CLI companion (requis pour les fonctionnalités CLI)
pnpm cli:build

# Application complète (frontend + backend Rust avec hot reload)
pnpm run tauri dev

# Frontend uniquement (Vite sur le port 1420)
pnpm dev

# Build de production
pnpm run tauri build
```

---

## Stack technique

| Couche | Technologie | Détails |
|---|---|---|
| **Framework** | Tauri 2.0 | Binaire natif, 25–40 Mo |
| **Runtime** | Tokio + DashMap 6 | Entièrement asynchrone, maps concurrentes sans verrou |
| **SSH** | russh 0.59 (`ring`) | Pur Rust, zéro dépendance C, SSH Agent |
| **PTY local** | portable-pty 0.8 | Feature-gated, ConPTY sous Windows |
| **Frontend** | React 19.1 + TypeScript 5.8 | Vite 7, Tailwind CSS 4 |
| **État** | Zustand 5 | 19 stores spécialisés |
| **Terminal** | xterm.js 6 + WebGL | Accéléré GPU, 60 fps+ |
| **Éditeur** | CodeMirror 6 | 30+ modes de langage |
| **Chiffrement** | ChaCha20-Poly1305 + Argon2id | AEAD + KDF gourmande en mémoire (256 Mo) |
| **Stockage** | redb 2.1 | Store KV embarqué |
| **i18n** | i18next 25 | 11 langues × 22 espaces de noms |
| **Plugins** | ESM Runtime | PluginContext gelé + 24 UI Kit |
| **CLI** | JSON-RPC 2.0 | Unix Socket / Named Pipe |

---

## Sécurité

| Préoccupation | Implémentation |
|---|---|
| **Mots de passe** | Trousseau OS (macOS Keychain / Windows Credential Manager / libsecret) |
| **Clés API IA** | Trousseau OS + authentification biométrique Touch ID sous macOS |
| **Export** | .oxide : ChaCha20-Poly1305 + Argon2id (256 Mo de mémoire, 4 itérations) |
| **Mémoire** | Sécurité mémoire Rust + `zeroize` pour le nettoyage des données sensibles |
| **Clés hôtes** | TOFU avec `~/.ssh/known_hosts`, rejette les modifications (prévention MITM) |
| **Plugins** | Object.freeze + ACL Proxy, disjoncteur, liste blanche IPC |
| **WebSocket** | Tokens à usage unique avec limites de temps |

---

## Feuille de route

- [x] Transfert d'agent SSH
- [ ] Marketplace de plugins
- [ ] Recherche de sessions & changement rapide

---

## Licence

**GPL-3.0** — ce logiciel est un logiciel libre distribué sous la [Licence Publique Générale GNU v3.0](https://www.gnu.org/licenses/gpl-3.0.html).

Vous êtes libre d'utiliser, de modifier et de distribuer ce logiciel selon les termes de la GPL-3.0. Tout travail dérivé doit également être distribué sous la même licence.

Texte intégral : [Licence Publique Générale GNU v3.0](https://www.gnu.org/licenses/gpl-3.0.html)

---

## Remerciements

[russh](https://github.com/warp-tech/russh) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [Tauri](https://tauri.app/) · [xterm.js](https://xtermjs.org/) · [CodeMirror](https://codemirror.net/) · [Radix UI](https://www.radix-ui.com/)

---

<p align="center">
  <sub>236 000+ lignes de Rust & TypeScript — construit avec ⚡ et ☕</sub>
</p>

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
