<p align="center">
  <img src="src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>Moteur de Terminal en Rust — Au-delà du SSH</strong>
  <br>
  <em>95 000+ lignes de Rust &amp; TypeScript. Zéro Electron. Zéro dépendance C dans la pile SSH.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.8.0-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-PolyForm%20Noncommercial-blueviolet" alt="License">
  <img src="https://img.shields.io/badge/rust-1.75+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="README.zh-CN.md">简体中文</a> | <a href="README.fr.md">Français</a>
</p>

---

## Qu'est-ce qu'OxideTerm ?

OxideTerm est une **application terminal multiplateforme** qui unifie shells locaux, sessions SSH distantes, gestion de fichiers, édition de code et assistance IA dans un seul binaire natif Rust. Ce n'est **pas** un wrapper Electron — le backend entier est écrit en Rust, livré sous forme d'exécutable natif d'environ 15 Mo via Tauri 2.0.

### Pourquoi un autre terminal ?

| Point de douleur | Réponse d'OxideTerm |
|---|---|
| Les clients SSH ne font pas de shell local | Moteur hybride : PTY local + SSH distant dans une fenêtre |
| Reconnexion = tout perdre | **Architecture Node-first** : reconnexion auto restaure redirections, transferts, état IDE |
| L'édition distante nécessite VS Code Remote | **Mode IDE intégré** : éditeur CodeMirror 6 via SFTP, zéro install serveur |
| Pas de réutilisation de connexion SSH | **Multiplexage SSH** : terminal, SFTP, redirections partagent une connexion |
| Les bibliothèques SSH dépendent d'OpenSSL | **russh 0.54** : SSH pur Rust, backend crypto `ring`, zéro deps C |

---

## Architecture en un coup d'œil

```
┌─────────────────────────────────────┐
│        Frontend (React 19)          │
│                                     │
│  SessionTreeStore ──► AppStore      │    10 stores Zustand
│  IdeStore    LocalTerminalStore     │    17 répertoires composants
│  ReconnectOrchestratorStore         │    11 langues × 18 espaces de noms
│  PluginStore  AiChatStore  ...      │
│                                     │
│        xterm.js 6 + WebGL           │
└──────────┬──────────────┬───────────┘
           │ Tauri IPC    │ WebSocket (binaire)
┌──────────▼──────────────▼───────────┐
│         Backend (Rust)              │
│                                     │
│  NodeRouter ── resolve(nodeId) ──►  │    22 modules commandes IPC
│  ├─ SshConnectionRegistry          │    État concurrent DashMap
│  ├─ SessionRegistry                │    PTY local feature-gated
│  ├─ ForwardingManager              │    Coffre ChaCha20-Poly1305
│  ├─ SftpSession (au niveau conn.)  │    russh 0.54 (backend ring)
│  └─ LocalTerminalRegistry          │    SSH Agent (AgentSigner)
│                                     │
│  Wire Protocol v1                   │
│  [Type:1][Longueur:4][Charge:n]     │
└─────────────────────────────────────┘
```

**Communication dual-plane** : trames binaires WebSocket pour les I/O terminal (zéro sérialisation), IPC Tauri pour les commandes structurées et événements. Le frontend ne touche jamais `sessionId` ni `connectionId` — tout est adressé par `nodeId`, résolu côté serveur par le `NodeRouter`.

---

## Points forts techniques

### 🔩 SSH pur Rust — russh 0.54

OxideTerm embarque **russh 0.54** compilé avec le backend crypto `ring` :
- **Zéro dépendance C/OpenSSL** dans le chemin SSH — pile crypto entièrement Rust
- Protocole SSH2 complet : échange de clés, canaux, sous-système SFTP, redirection de ports
- Suites ChaCha20-Poly1305 et AES-GCM, clés Ed25519/RSA/ECDSA

### 🔑 Authentification SSH Agent (AgentSigner)

Un `AgentSigner` sur mesure encapsule l'Agent SSH système et satisfait le trait `Signer` de russh :

```rust
// Résout le problème RPITIT Send bound dans russh 0.54
// en clonant &PublicKey vers une valeur possédée avant le .await
pub struct AgentSigner { /* ... */ }
impl Signer for AgentSigner { /* défi-réponse via IPC Agent */ }
```

- **Plateformes** : Unix (`SSH_AUTH_SOCK`), Windows (`\\.\pipe\openssh-ssh-agent`)
- **Chaînes proxy** : chaque saut peut utiliser l'auth Agent indépendamment
- **Reconnexion** : `AuthMethod::Agent` rejoué automatiquement

### 🧭 Architecture Node-First (NodeRouter)

L'**abstraction Oxide-Next des nœuds** élimine une classe entière de conditions de course :

```
Frontend : useNodeState(nodeId) → { readiness, sftpReady, error }
Backend  : NodeRouter.resolve(nodeId) → ConnectionEntry → SftpSession
```

- Les opérations frontend SFTP/IDE ne transmettent que `nodeId`
- Le backend résout `nodeId → ConnectionEntry` de manière atomique
- La reconnexion SSH change `connectionId` — SFTP/IDE **insensibles**
- `NodeEventEmitter` pousse des événements typés avec compteurs de génération

### ⚙️ Terminal local — PTY thread-safe

Shell local multiplateforme via `portable-pty 0.8`, gate sous la feature `local-terminal` :

- **Thread safety** : `MasterPty` encapsulé dans `std::sync::Mutex` + `unsafe impl Sync`
- **Threads I/O dédiés** : les lectures PTY bloquantes ne touchent jamais la boucle Tokio
- **Détection de shell** : découvre auto `zsh`, `bash`, `fish`, `pwsh`, Git Bash, WSL2
- **Feature gate** : `cargo build --no-default-features` retire le PTY pour le mobile

### 🔌 Système de plugins runtime (v1.6.2+)

Chargement dynamique de plugins avec API gelée et sécurisée :

- **API PluginContext** : 8 espaces de noms (terminal, ui, commands, settings, lifecycle, events, storage, system)
- **24 composants UI Kit** : composants React pré-construits injectés dans les bacs à sable
- **Modèle de sécurité** : `Object.freeze` + Proxy ACL, disjoncteur, liste blanche IPC
- **Architecture Membrane** : plugins exécutés dans des contextes ESM isolés avec pont contrôlé

### 🛡️ Pool de connexions SSH

`SshConnectionRegistry` avec comptage de références, basé sur DashMap :

- Plusieurs terminaux, SFTP, redirections partagent **une seule connexion SSH physique**
- Machines d'état indépendantes par connexion
- Timeout d'inactivité (30 min), keep-alive (15s), détection de pannes par heartbeat
- Propagation en cascade : bastion down → tous les nœuds en aval marqués `link_down`

### 🔀 Redirection de ports — I/O sans verrou

Redirection locale (-L), distante (-R) et SOCKS5 dynamique (-D) complète :

- **Architecture message-passing** : Channel SSH détenu par une tâche `ssh_io` unique, pas de `Arc<Mutex<Channel>>`
- **Rapport de décès** : les tâches signalent activement leur raison de sortie
- **Auto-restauration** : les redirections `Suspended` reprennent après reconnexion
- **Timeout** : `FORWARD_IDLE_TIMEOUT` (300s) empêche les connexions zombies

### 🤖 Assistant terminal IA

IA dual-mode, priorité à la vie privée :

- **Panneau inline** (`⌘I`) : commandes rapides injectées via bracketed paste
- **Chat latéral** : conversation persistante avec historique
- **Capture de contexte** : Terminal Registry collecte le tampon des panneaux actifs ou tous les splits
- **Compatible** : OpenAI, Ollama, DeepSeek, OneAPI, tout endpoint `/v1/chat/completions`
- **Sécurisé** : clés API dans le trousseau système

### 💻 Mode IDE — Édition distante zéro-install

Éditeur CodeMirror 6 via SFTP — aucun agent côté serveur requis :

- **Arborescence** : chargement paresseux SFTP avec indicateurs de statut Git
- **30+ modes de langage** : 16 packs CodeMirror natifs + modes legacy
- **Résolution de conflits** : verrouillage optimiste par `mtime`
- **Git piloté par événements** : rafraîchissement auto sur sauvegarde, création, suppression, renommage
- **State Gating** : IO bloqué si `readiness !== 'ready'`, Key-Driven Reset à la reconnexion

### 🔐 Export chiffré .oxide

Format de sauvegarde portable :

- **ChaCha20-Poly1305 AEAD** chiffrement authentifié
- **Argon2id KDF** (256 Mo mémoire, 4 itérations) — résistant au brute-force GPU
- **SHA-256** somme de contrôle d'intégrité
- **Intégration optionnelle de clés** : clés privées encodées en base64
- **Analyse pré-vol** : répartition des types d'auth, détection des clés manquantes

### 📡 ProxyJump — Multi-saut conscient de la topologie

- Profondeur de chaîne illimitée : `Client → Saut A → Saut B → … → Cible`
- Parse auto SSH Config, construction du graphe topologique, calcul de chemin Dijkstra
- Nœuds de saut réutilisables comme sessions indépendantes
- Propagation de pannes en cascade avec synchronisation auto en aval

### 📊 Profileur de ressources

Surveillance en temps réel des hôtes distants via canal shell SSH persistant :

- Lecture de `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/net/dev`
- Calcul CPU% et débit réseau basé sur le delta
- Canal unique — évite l'épuisement de MaxSessions
- Dégradation auto vers RTT-only sur hôtes non-Linux ou échecs consécutifs
### 🪟 Optimisation Windows approfondie

- **Intégration ConPTY native** : appel direct de l’API Windows Pseudo Console (ConPTY) pour un support parfait TrueColor et séquences ANSI — fini le WinPTY obsolète.
- **Détection intelligente de shell** : scanner intégré auto-détecte **PowerShell 7 (pwsh)**, **Git Bash**, **WSL2** et CMD via registre et PATH.
- **Expérience native** : Rust gère directement les événements fenêtre — vitesse de réponse bien supérieure aux apps Electron.

### 📊 Tampon de défilement backend

- **Persistance haute capacité** : **100 000 lignes** par défaut de sortie terminal, sérialisable sur disque (format MessagePack).
- **Recherche haute performance** : `spawn_blocking` isole les tâches de recherche regex, évitant de bloquer le runtime Tokio.
- **Mémoire efficace** : conception en tampon circulaire, éviction automatique des données les plus anciennes.

### ⚛️ Architecture Multi-Store

Le frontend adopte un pattern **Multi-Store** (10 stores) pour gérer des domaines d’état radicalement différents :

| Store | Rôle |
|---|---|
| **SessionTreeStore** | Intention utilisateur — arborescence, flux de connexion |
| **AppStore** | Couche factuelle — état SSH réel via `connections` Map |
| **IdeStore** | Mode IDE — édition distante, statut Git, multi-onglets |
| **LocalTerminalStore** | Cycle de vie PTY local, monitoring Shell |
| **ReconnectOrchestratorStore** | Pipeline auto-reconnexion (snapshot → ssh-connect → restore) |
| **TransferStore** | File de transfert SFTP et progrès |
| **PluginStore** | État runtime des plugins et registre UI |
| **ProfilerStore** | Métriques du profileur de ressources |
| **AiChatStore** | État des conversations IA |
| **SettingsStore** | Paramètres de l’application |

Malgré des sources d’état différentes, la logique de rendu est unifiée via les composants `TerminalView` et `IdeView`.
---

## Stack technique

| Couche | Technologie | Détails |
|---|---|---|
| **Framework** | Tauri 2.0 | Binaire natif, ~15 Mo, zéro Electron |
| **Runtime** | Tokio + DashMap 6 | Full async + maps concurrentes sans verrou |
| **SSH** | russh 0.54 (`ring`) | Pur Rust, zéro deps C, SSH Agent |
| **PTY local** | portable-pty 0.8 | Feature-gated, ConPTY sous Windows |
| **Frontend** | React 19.1 + TypeScript 5.8 | Vite 7, Tailwind CSS 4 |
| **État** | Zustand 5 | 10 stores spécialisés, sync événementielle |
| **Terminal** | xterm.js 6 + WebGL | Rendu GPU, 60fps+ |
| **Éditeur** | CodeMirror 6 | 16 packs de langage + modes legacy |
| **Chiffrement** | ChaCha20-Poly1305 + Argon2id | AEAD + KDF à dureté mémoire |
| **Stockage** | redb 2.1 | DB embarquée (sessions, redirections, transferts) |
| **Sérialisation** | MessagePack (rmp-serde) | Persistance binaire tampon/état |
| **i18n** | i18next 25 | 11 langues × 18 espaces de noms |
| **SFTP** | russh-sftp 2.0 | Protocole de transfert de fichiers SSH |
| **WebSocket** | tokio-tungstenite 0.24 | WebSocket async pour le plan de données terminal |
| **Protocole** | Wire Protocol v1 | Binaire `[Type:1][Length:4][Payload:n]` sur WebSocket |
| **Plugins** | Runtime ESM | PluginContext gelé + 24 composants UI Kit |

---

## Matrice de fonctionnalités

| Catégorie | Fonctionnalités |
|---|---|
| **Terminal** | PTY local, SSH distant, panneaux divisés (H/V), contexte IA cross-pane, rendu WebGL |
| **SSH** | Pool de connexions, multiplexage, ProxyJump (∞ sauts), graphe topologique, pipeline auto-reconnexion |
| **Auth** | Mot de passe, clé SSH (RSA/Ed25519/ECDSA), SSH Agent, certificat, Keyboard-Interactive (2FA), Known Hosts |
| **Fichiers** | Navigateur SFTP double panneau, glisser-déposer, aperçu (images/vidéo/audio/PDF/code/hex), file de transfert |
| **IDE** | Arborescence, éditeur CodeMirror, multi-onglets, statut Git, résolution de conflits, terminal intégré |
| **Redirection** | Locale (-L), distante (-R), SOCKS5 dynamique (-D), auto-restauration, rapport de décès, I/O sans verrou |
| **IA** | Panneau inline + chat latéral, SSE streaming, insertion de code, OpenAI/Ollama/DeepSeek |
| **Plugins** | Chargement ESM runtime, 8 espaces API, 24 UI Kit, exécution sandboxée, disjoncteur |
| **Sécurité** | Chiffrement .oxide, trousseau système, `zeroize` mémoire, TOFU clé d'hôte |
| **i18n** | EN, 简体中文, 繁體中文, 日本語, FR, DE, ES, IT, 한국어, PT-BR, VI |

---

## Fonctionnalités détaillées

### 🚀 Expérience terminale hybride
- **Shell local zéro latence** : IPC direct avec les processus locaux.
- **SSH distant haute performance** : flux binaire WebSocket, sans surcharge HTTP.
- **Héritage d’environnement complet** : hérite PATH, HOME et toutes les variables — expérience identique au terminal système.

### 🔐 Authentification diversifiée
- **Mot de passe** : stocké sécurisé dans le trousseau système.
- **Clés** : RSA / Ed25519 / ECDSA, scan auto de `~/.ssh/id_*`.
- **SSH Agent** : via `AgentSigner` (macOS/Linux/Windows).
- **Certificats** : OpenSSH Certificates.
- **2FA/MFA** : authentification Keyboard-Interactive.
- **Known Hosts** : vérification TOFU + `~/.ssh/known_hosts`.

### 🔍 Recherche plein texte
Recherche de contenu à l’échelle du projet avec cache intelligent :
- **Recherche en temps réel** : saisie anti-rebond 300ms avec résultats instantanés.
- **Cache de résultats** : TTL 60 secondes pour éviter les scans répétés.
- **Groupement** : résultats groupés par fichier avec positionnement par numéro de ligne.
- **Surlignage** : termes recherchés mis en évidence dans les aperçus.
- **Invalidation auto** : cache vidé lors des modifications de fichiers.

### 📦 Gestion de fichiers avancée
- **Protocole SFTP v3** : gestionnaire double panneau complet.
- **Glisser-déposer** : opérations multi-fichiers et dossiers par lots.
- **Aperçu intelligent** :
  - 🎨 Images (JPEG/PNG/GIF/WebP)
  - 🎬 Vidéos (MP4/WebM)
  - 💻 Coloration code (30+ langages)
  - 📄 Documents PDF
  - 🔍 Visionneuse Hex (fichiers binaires)
- **Suivi de progression** : vitesse en temps réel, barres, ETA.

### 🌍 Internationalisation (i18n)
- **11 langues** : English, 简体中文, 繁體中文, 日本語, Français, Deutsch, Español, Italiano, 한국어, Português, Tiếng Việt.
- **Chargement dynamique** : packs de langue à la demande via i18next.
- **Type-safe** : définitions TypeScript pour toutes les clés de traduction.

### 🌐 Optimisation réseau
- **Architecture dual-plane** : plan de données (WebSocket direct) et plan de contrôle (Tauri IPC) séparés.
- **Protocole binaire custom** : `[Type:1][Length:4][Payload:n]`, zéro surcharge JSON.
- **Contrôle de back-pressure** : prévient le débordement mémoire lors de pics.
- **Auto-reconnexion** : recul exponentiel, jusqu’à 5 tentatives.

---

## Démarrage rapide

### Prérequis

- **Rust** 1.75+
- **Node.js** 18+ (pnpm recommandé)
- **Outils plateforme** :
  - macOS : Xcode Command Line Tools
  - Windows : Visual Studio C++ Build Tools
  - Linux : `build-essential`, `libwebkit2gtk-4.1-dev`, `libssl-dev`

### Développement

```bash
git clone https://github.com/AnalyseDeCircuit/OxideTerm.git
cd OxideTerm && pnpm install

# Application complète (frontend + backend Rust + PTY local)
pnpm tauri dev

# Frontend seul (rechargement chaud sur port 1420)
pnpm dev

# Build production
pnpm tauri build

# Noyau léger — retirer le PTY local pour mobile
cd src-tauri && cargo build --no-default-features --release
```

---

## Structure du projet

```
OxideTerm/
├── src/                            # Frontend — 56K lignes TypeScript
│   ├── components/                 # 17 répertoires
│   │   ├── terminal/               #   Vues terminal, panneaux divisés
│   │   ├── sftp/                   #   Navigateur fichiers double panneau
│   │   ├── ide/                    #   Éditeur, arborescence, dialogues Git
│   │   ├── ai/                     #   Chat inline + latéral
│   │   ├── plugin/                 #   Gestionnaire de plugins & UI runtime
│   │   ├── forwards/               #   Gestion des redirections de ports
│   │   ├── connections/            #   CRUD connexions & import
│   │   ├── topology/               #   Graphe de topologie réseau
│   │   ├── layout/                 #   Barre latérale, en-tête, panneaux
│   │   └── ...                     #   sessions, settings, modals, etc.
│   ├── store/                      # 10 stores Zustand
│   ├── lib/                        # Couche API, fournisseurs IA, runtime plugins
│   ├── hooks/                      # Hooks React (événements, clavier, toast)
│   ├── types/                      # Définitions de types TypeScript
│   └── locales/                    # 11 langues × 18 espaces de noms
│
├── src-tauri/                      # Backend — 39K lignes Rust
│   └── src/
│       ├── router/                 #   NodeRouter (nodeId → ressource)
│       ├── ssh/                    #   Client SSH (12 modules incl. Agent)
│       ├── local/                  #   PTY local (feature-gated)
│       ├── bridge/                 #   Pont WebSocket & Wire Protocol v1
│       ├── session/                #   Gestion de sessions (16 modules)
│       ├── forwarding/             #   Redirection de ports (6 modules)
│       ├── sftp/                   #   Implémentation SFTP
│       ├── config/                 #   Coffre, trousseau, SSH Config
│       ├── oxide_file/             #   Chiffrement .oxide (ChaCha20)
│       ├── commands/               #   22 modules commandes Tauri IPC
│       └── state/                  #   Types d'état global
│
└── docs/                           # 28+ documents architecture & fonctionnalités
```

---

## Feuille de route

### ✅ Livré (v1.8.0)

- [x] Terminal local (PTY) avec feature gating
- [x] Pool de connexions SSH & multiplexage
- [x] Authentification SSH Agent (AgentSigner)
- [x] Architecture Node-first (NodeRouter + événements)
- [x] Orchestrateur auto-reconnexion (pipeline 6 phases)
- [x] Chaîne ProxyJump bastion illimitée
- [x] Redirection de ports — locale / distante / SOCKS5 dynamique
- [x] Gestionnaire de fichiers SFTP double panneau avec aperçu
- [x] Mode IDE (CodeMirror 6 + statut Git)
- [x] Export chiffré .oxide avec intégration de clés
- [x] Assistant terminal IA (inline + latéral)
- [x] Système de plugins runtime (PluginContext + UI Kit)
- [x] Panneaux terminaux divisés avec raccourcis clavier
- [x] Profileur de ressources (CPU / mémoire / réseau)
- [x] i18n — 11 langues × 18 espaces de noms
- [x] Auth Keyboard-Interactive (2FA/MFA)
- [x] Recherche historique profonde (30K lignes, Rust regex)

### 🚧 En cours

- [ ] Palette de commandes (`⌘K`)
- [ ] Recherche & changement rapide de sessions

### 📋 Planifié

- [ ] Transfert SSH Agent
- [ ] Enregistrement & lecture de sessions
- [ ] Transfert X11
- [ ] Adaptation mobile (iOS / Android)

---

## Sécurité

| Préoccupation | Implémentation |
|---|---|
| **Mots de passe** | Trousseau système (macOS Keychain / Windows Credential Manager / Linux libsecret) |
| **Clés API IA** | Trousseau système sous service `com.oxideterm.ai` |
| **Fichiers config** | `~/.oxideterm/connections.json` — stocke uniquement les IDs du trousseau |
| **Export** | .oxide : ChaCha20-Poly1305 + Argon2id, intégration optionnelle de clés |
| **Mémoire** | `zeroize` efface les données sensibles ; Rust garantit la sécurité mémoire |
| **Clés d'hôte** | TOFU avec `~/.ssh/known_hosts` |
| **Plugins** | Object.freeze + Proxy ACL, disjoncteur, liste blanche IPC |

---

## Licence

**PolyForm Noncommercial 1.0.0**

- ✅ Usage personnel / non lucratif : gratuit
- 🚫 Usage commercial : nécessite une licence
- ⚖️ Clause de défense de brevet (Clause Nucléaire)

Texte complet : https://polyformproject.org/licenses/noncommercial/1.0.0/

---

## Remerciements

- [russh](https://github.com/warp-tech/russh) — SSH pur Rust
- [portable-pty](https://github.com/wez/wezterm/tree/main/pty) — Abstraction PTY multiplateforme
- [Tauri](https://tauri.app/) — Framework d'application natif
- [xterm.js](https://xtermjs.org/) — Émulateur de terminal
- [CodeMirror](https://codemirror.net/) — Éditeur de code
- [Radix UI](https://www.radix-ui.com/) — Primitives UI accessibles

---

<p align="center">
  <sub>Construit en Rust et Tauri — 95 000+ lignes de code</sub>
</p>
