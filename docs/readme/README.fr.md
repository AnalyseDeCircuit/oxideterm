<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Workspace AI-native pour serveurs distants.</strong>
  <br>
  Connectez-vous à vos serveurs via SSH, puis travaillez avec terminaux, fichiers, ports, transferts, édition légère, consoles série et OxideSens AI dans une app native local-first.
  <br>
  Application GPUI native · SSH pur Rust · OxideSens AI BYOK · aucun compte requis pour les workflows SSH essentiels
  <br>
  <strong>Zéro WebView. Zéro OpenSSL. Zéro télémétrie. Zéro abonnement. BYOK-first. Pure Rust all the way down.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.7-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plateforme">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licence">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Prochaine grande édition native de <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — rendu GPU, zéro WebView, avec <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (framework de rendu de Zed)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens ouvre un terminal dans OxideTerm" width="920">
</a>

*OxideSens suit une demande utilisateur et ouvre un terminal dans OxideTerm.*

</div>

---

## Ce que vous pouvez faire

- Gérer terminaux SSH, SFTP, redirections de ports, consoles série, shells locaux et édition légère dans un workspace natif
- Garder le travail distant en vie malgré les coupures réseau avec Grace Period reconnect
- Demander à OxideSens AI d’inspecter les sessions live et d’exécuter des actions workspace approuvées via votre propre fournisseur IA

---

## Pourquoi OxideTerm Native ?

| Si vous tenez à... | OxideTerm Native vous donne... |
|---|---|
| Un nœud distant, plusieurs outils | Terminal, SFTP, redirection de ports, trzsz, IDE natif, monitoring et OxideSens AI restent attachés au même workspace SSH |
| Shell native zéro WebView | GPUI dessine l’UI desktop directement sur une surface GPU, sans DOM, CSS, JavaScript, Chromium ni runtime WebKit |
| Workflows SSH local-first | SSH, SFTP, forwarding, shell local, terminaux série et configuration fonctionnent sans inscription |
| OxideSens AI BYOK plutôt que crédits de plateforme | OxideSens utilise votre endpoint OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible avec MCP, RAG et actions workspace approuvées |
| Reconnexion stable | Grace Period sonde l’ancienne connexion pendant 30 s avant de la remplacer, afin que les TUI survivent aux microcoupures |
| SSH pur Rust et sécurité des identifiants | `russh` + `ring`, sans OpenSSL/libssh2 ; mots de passe et clés API restent dans le trousseau OS, `.oxide` utilise ChaCha20-Poly1305 + Argon2id |

## Ce que c'est / ce que ce n'est pas

OxideTerm Native se concentre sur un **workspace IA local-first pour serveurs distants**, reconstruit comme application desktop GPUI en Rust pur. Il s’adresse aux utilisateurs qui veulent garder terminaux, fichiers, ports, transferts, édition légère, consoles série et une OxideSens AI autour de leurs propres machines et nœuds distants.

Ce n’est pas encore la ligne de téléchargement stable actuelle, ni une plateforme Agent cloud hébergée. Ce n’est pas non plus Electron, Tauri ou un terminal web : pas de Chromium, pas de WebView, pas de JavaScript, pas de CSS.

---

## Captures d’écran

L’UI native suit le même modèle de workspace OxideTerm et le même langage visuel que la ligne Tauri actuelle.

<table>
<tr>
<td align="center"><strong>Terminal SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminal SSH avec OxideSens AI" /></td>
<td align="center"><strong>Gestionnaire de fichiers SFTP</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="Gestionnaire de fichiers SFTP double volet avec file de transfert" /></td>
</tr>
<tr>
<td align="center"><strong>IDE intégré</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Mode IDE intégré" /></td>
<td align="center"><strong>Redirection de ports intelligente</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Redirection de ports intelligente avec détection automatique" /></td>
</tr>
</table>

---

## Différences avec WebView/Tauri

| Aspect | WebView/Tauri | Native |
|---|---|---|
| Rendu | Chromium/Safari/WebKit2GTK + CSS | GPUI, surface GPU, mode immédiat, Rust pur |
| Flux terminal | WebSocket → boucle JS → xterm.js | Entrée Rust → `TerminalState` → rendu GPUI |
| IPC | JSON-RPC à chaque commande | Appels de fonctions in-process |
| SSH keepalive | Timer JavaScript | Tâche async Rust |
| Plugins | ESM dans un sandbox navigateur | WASM wasmtime + API hôte Rust typée |
| CLI | Requiert l'application desktop | Binaire autonome |
| Taille de l'artefact | Installateurs souvent autour de 150–200 MB | macOS arm64 actuel : portable/DMG compressé ~50–60 MB ; binaire release brut ~132 MB |

## Fonctionnalités

| Catégorie | Fonctionnalités |
|---|---|
| Terminal | PTY local, SSH, terminaux série locaux, split panes, shell integration, marques de commande, asciicast, trzsz, graphiques Sixel/Kitty, politique de rendu |
| SSH & Auth | Pool de connexions, ProxyJump illimité, Grace Period reconnect, TOFU host-key, SSH Agent forwarding, mot de passe/clé/certificat/keyboard-interactive |
| SFTP / IDE | Navigateur double panneau, file de transferts, aperçu, favoris, écritures atomiques, arbre distant, éditeur multi-onglets, résolution de conflits |
| Forwarding | Local, Remote, Dynamic SOCKS5, règles sauvegardées, restauration après reconnexion, rapport de mort, idle timeout |
| IA | OxideSens avec OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG et approbation de commandes |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, sauvegardes rollback, import/export chiffré |
| Plugins / CLI | Sandbox WASM, API hôte native, réglages par plugin ; CLI pour settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Architecture

OxideTerm Native retire le pont WebView et garde terminal, SSH, SFTP, forwarding, IDE, IA, plugins et CLI dans une architecture Rust native. Les détails complets sont conservés ci-dessous.

<details>
<summary><strong>Architecture, internals SSH, shell GPUI, reconnexion, IA, plugins et plus</strong></summary>
<br>

### Architecture — processus unique, zéro bridge

```text
GPUI Render Loop
  WorkspaceApp / Tab surfaces / GPUI views
        │ in-process Arc<> / async
Domain Crates
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

Il n'y a pas de frontière de sérialisation entre l'UI et le backend SSH/terminal. Les octets du terminal modifient directement `TerminalState`, puis GPUI lit l'état et émet les draw calls GPU.

### SSH pur Rust — russh (ring)

L’édition native lie directement dans le binaire desktop le même stack `russh` que la ligne Tauri :

- **Zéro dépendance C/OpenSSL** grâce à `ring`
- SSH2 complet : key exchange, channels, sous-système SFTP, redirection de ports
- ChaCha20-Poly1305 / AES-GCM, clés Ed25519/RSA/ECDSA
- SSH Agent sur Unix (`SSH_AUTH_SOCK`) et Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump multi-hop avec authentification indépendante à chaque saut

### Reconnexion intelligente avec Grace Period

La sémantique de reconnexion correspond à la ligne Tauri, mais l’orchestration tourne entièrement dans des tâches async Rust :

1. Détecter le timeout SSH keepalive sans JavaScript timer throttling
2. Snapshot des terminal panes, transferts SFTP, forwards et fichiers IDE
3. Sonder l’ancienne connexion pendant 30 secondes de Grace Period pour laisser survivre les TUI lors d’un changement réseau
4. Si la récupération échoue, reconnecter, restaurer les forwards, reprendre les transferts et rouvrir les fichiers IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### Pool de connexions SSH et routage par nœud

`SshConnectionRegistry` s’appuie sur `DashMap` et conserve le modèle node-first de Tauri sans pont de cycle de vie WebSocket :

- Une connexion SSH physique peut servir terminal panes, SFTP, port forwards et travail IDE
- Chaque connexion passe par `connecting → active → idle → link_down → reconnecting`
- L’UI adresse `nodeId`; `NodeRouter` résout atomiquement le `connectionId` actif
- `NodeRuntimeStore` persiste les snapshots de topologie dans `session_tree.json`
- La panne d’un jump host propage `link_down` aux nœuds descendants

### OxideSens AI

OxideSens reste BYOK-first, avec construction du contexte dans le processus :

- Providers : OpenAI, Anthropic, Gemini, Ollama ou tout endpoint OpenAI-compatible
- MCP : transports stdio et SSE, découverte et invocation d’outils
- RAG : BM25 full-text, index vectoriel HNSW, Reciprocal Rank Fusion, tokenizer CJK bigram
- Le contexte IA vient de l’état du workspace ; les identifiants sont masqués avant les appels provider
- Les clés API restent dans le trousseau OS et n’entrent jamais dans les logs ou frames IPC

### Shell desktop GPUI

L’UI est dessinée directement avec GPUI, sans pipeline DOM/CSS/JavaScript :

- 17 types d’onglets workspace : terminal local/SSH, SFTP, IDE, Forwards, Settings, Plugin, Topology, etc.
- Arbre binaire de panes avec séparateurs déplaçables, jusqu’à quatre panes par onglet terminal
- Command palette, raccourcis globaux et sidebars construits avec des primitives GPUI
- Immediate-mode rendering réagit à l’état Rust sans round-trip de sérialisation

### État du terminal et rendu

Le rendu terminal est d’abord modélisé comme état Rust, puis dessiné par GPUI :

- La sortie PTY arrive dans `TerminalState` ; scrollback, curseur, sélection, marks et état de recherche restent en Rust
- La rendering policy peut passer entre Boost, Normal et Idle sans attendre un browser event loop
- Les graphiques Sixel et Kitty sont suivis comme assets propres au terminal, pas comme DOM nodes ou canvas overlays
- Les split panes partagent le même modèle de workspace state, ce qui permet à tab restore et reconnect de snapshotter ensemble la topologie terminal

### Workspace SFTP et IDE

Les fichiers distants font partie du même node workspace, pas d’une fonction séparée :

- Les sessions SFTP sont résolues via `NodeRouter`, donc reconnect peut remplacer la connexion SSH sous-jacente sans changer l’adresse node de l’UI
- Les transfer queues suivent direction, progression, retry state et speed limits indépendamment des file panes visibles
- Les onglets IDE gardent ensemble dirty buffers, remote paths, conflict state et restore metadata
- Lorsque le backend le permet, les écritures distantes utilisent un staged/atomic behavior pour éviter les partial writes dans le flux d’édition normal

### Plugins, CLI et diagnostics

La branche native garde extensions et surfaces de support dans des limites Rust-native :

- Les plugins tournent dans une sandbox wasmtime avec typed host capabilities plutôt que browser globals
- La CLI lie directement les domain crates pour doctor, settings, connections, forwards, portable bundles, backups et reports
- Les diagnostics privilégient counts, paths, feature flags et redacted hints plutôt que des payloads bruts porteurs de secrets
- Les flows CLI qui modifient l’état utilisent dry-run plans, `--yes` guards et rollback backups lorsque c’est pertinent

### Redirection de ports — Lock-Free I/O

Le forwarding conserve la sémantique Tauri dans un crate Rust autonome :

- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- Un seul task `ssh_io` possède chaque SSH Channel et évite `Arc<Mutex<Channel>>`
- Auto-restauration après reconnexion, death reporting et idle timeout

### trzsz — transfert in-band

trzsz continue d’utiliser le flux terminal, sans port supplémentaire ni agent distant :

- Upload/download via le flux terminal existant
- Fonctionne à travers les chaînes ProxyJump
- Les sélecteurs de fichiers natifs évitent les limites mémoire du navigateur
- Transfert bidirectionnel, dossiers, limites configurables

### Export `.oxide` chiffré

Le format de bundle chiffré correspond à la ligne Tauri :

- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF** : 256 MB memory cost, 4 iterations, augmente le coût du brute force GPU
- Couvre connections, forwards, settings, quick commands, plugin settings et portable secrets

</details>

---

## Lancer depuis le code source

```sh
cargo run
OXIDETERM_RENDER_PROFILE=compatibility cargo run
./scripts/build-cli.sh
./scripts/build-agent.sh
```

## CLI

```sh
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

## Sécurité

| Sujet | Implémentation |
|---|---|
| Mots de passe et clés | macOS Keychain / Windows Credential Manager / libsecret |
| Secrets en mémoire | `zeroize` / `Zeroizing` |
| Diagnostics et contexte IA | valeurs secrètes masquées avant toute sortie ou requête fournisseur |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Écritures CLI | dry-run, garde `--yes`, sauvegardes rollback |
| Plugins | isolation wasmtime et API hôte à capacités |

## État de la release

- [x] SSH Agent forwarding, Grace Period reconnect, shell desktop GPUI
- [x] Flux terminal in-process sans WebSocket
- [x] SFTP, forwarding, IDE, IA, cloud sync, plugins, CLI
- [x] Terminaux série locaux
- [x] ProxyCommand complet
- [ ] Audit logging

## Contribution

## Neutralité des providers

OxideTerm est BYOK-first et neutre vis-à-vis des providers.

Les intégrations de providers servent à aider les utilisateurs à connecter les outils auxquels ils font déjà confiance. Elles ne sont ni un classement, ni un panneau publicitaire, ni un système de récompense pour ceux qui demandent le plus chaleureusement.

La compatibilité, la maintenabilité, la sécurité et la valeur réelle pour les utilisateurs décident de ce qui est documenté. La visibilité suit l'utilité, pas l'enthousiasme.

Quand une fonctionnalité existe déjà dans Tauri, gardez le comportement, les libellés, les états d'interaction et les workflows alignés. Un nouveau crate doit posséder une vraie responsabilité de domaine, pas seulement réexporter du code.

## Support et maintenance

Les bugs et régressions reproductibles avec diagnostics expurgés sont prioritaires. Les demandes de fonctionnalités sont évaluées selon leur périmètre, leur sûreté et leur alignement avec la direction d’OxideTerm pour le workspace de serveurs distants.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Si OxideTerm aide votre workflow, une étoile GitHub, une reproduction, une correction de traduction, un plugin ou une pull request rendent le projet plus facile à maintenir.

---

## Licence / Remerciements

**GPL-3.0-only**. Les notices tierces sont dans `NOTICE`. Merci à `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` et `tree-sitter`.
