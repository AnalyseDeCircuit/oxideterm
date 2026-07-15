<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>Espace de travail d’exploitation natif avec IA pour serveurs distants — Application native 100% Rust</strong>
  <br>
  Terminaux SSH, Telnet, série, RDP/VNC, SFTP, redirection de ports et édition légère dans un espace de travail natif.
  <br>
  Rendu GPU. Gratuit. Aucun compte requis.
  <br>
  <strong>Sans Electron. Sans WebView embarquée. Sans télémétrie. Sans abonnement. BYOK d'abord. SSH pur Rust sans OpenSSL/libssh2.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.17-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plateforme">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licence">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Open source, local-first et rendu par GPU avec GPUI.</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

> [!WARNING]
> **OxideTerm 2.0 n’est pas encore publié en version stable.** La branche `main` contient désormais le code source de la prochaine version 2.0. La dernière version stable reste `v1.6.12` ; les versions GPUI Preview sont des préversions.

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="Aperçu des fonctionnalités d'OxideTerm" width="920">
</p>

---

## Ce qu’est OxideTerm

OxideTerm est un espace de travail open source pour SSH et les opérations distantes. Terminaux, fichiers, redirections, outils hôte et bureaux distants restent réunis dans un même espace.

**Ce que vous pouvez faire :**

- Gérer SSH, Telnet, série, RDP/VNC, SFTP, redirections de ports, shells locaux et édition légère dans un seul espace de travail
- Maintenir le travail distant pendant de brèves coupures réseau grâce à la reconnexion Grace Period
- Demander à OxideSens d’examiner les sessions actives et d’exécuter des actions approuvées avec votre propre fournisseur d’IA

Vos connexions et données opérationnelles restent sous votre contrôle. OxideSens utilise votre propre fournisseur d’IA et aucun compte n’est requis.

---

## Pourquoi OxideTerm ?

- SSH, Telnet, série, RDP/VNC, SFTP, redirections et shells locaux dans une application de bureau
- Reconnexion Grace Period pour les brèves coupures réseau
- OxideSens avec vos propres identifiants IA et des actions approuvées
- Interface GPUI sans Electron ni runtime de navigateur embarqué

---

## Captures d’écran

Les captures ci-dessous présentent les parcours terminal, fichiers, édition et redirection d’OxideTerm.

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

## Conçu pour les opérations distantes

OxideTerm réunit connexions, fichiers, redirections, outils hôte, automatisation et contexte IA dans un espace Rust. Les outils partagent la même identité serveur et le même cycle de session.

---

## Fonctions

| Catégorie | Fonctions |
|---|---|
| **Terminal et connexions** | Shells locaux, SSH, Telnet, série, volets, routes multi-hop et reconnexion stable |
| **Fichiers et édition distante** | SFTP, files de transfert, favoris, écritures sûres, arbres de projet et édition par onglets |
| **Redirection et réseau** | Redirections locale, distante et SOCKS5 dynamique, règles enregistrées et débogage de sockets |
| **Opérations hôte et bureau distant** | Supervision, processus, services, journaux, ports, tâches, disques, paquets, conteneurs, tmux, RDP et VNC |
| **OxideSens et automatisation** | Fournisseurs IA personnels, MCP, RAG local, actions approuvées, synchronisation chiffrée et CLI |
| **Extensions et personnalisation** | Plugins WASM, onglets personnalisés, commandes rapides, thèmes, arrière-plans, raccourcis et 11 langues |

---

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens ouvre un terminal dans OxideTerm" width="920">
</a>

*OxideSens suit une demande utilisateur et ouvre un terminal dans OxideTerm.*

</div>

---

## Architecture

OxideTerm réunit terminal, SSH, Telnet, RDP, VNC, SFTP, redirection, IDE, IA, plugins et CLI dans une architecture Rust. Les détails techniques suivent ci-dessous.

<details>
<summary><strong>Architecture, internes SSH, shell GPUI, reconnexion, IA, plugins et plus</strong></summary>
<br>

### Architecture — cœur en processus, sans pont WebView

```text
GPUI Render Loop
  WorkspaceApp / Tab surfaces / GPUI views
        │ dans le processus Arc<> / async
Domain Crates
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

Il n'y a pas de frontière de sérialisation entre l'UI et le backend SSH/terminal. Les octets du terminal modifient directement `TerminalState`, puis GPUI lit l'état et émet les draw calls GPU.

### SSH pur Rust — russh (ring)


- **Sans OpenSSL/libssh2 dans la pile SSH** — `ring` fournit la cryptographie SSH
- SSH2 complet : échange de clés, canaux, sous-système SFTP, redirection de ports
- ChaCha20-Poly1305 / AES-GCM, clés Ed25519/RSA/ECDSA
- SSH Agent sur Unix (`SSH_AUTH_SOCK`) et Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump multi-hop avec authentification indépendante à chaque saut

### Reconnexion intelligente avec Grace Period


1. Détecter le timeout SSH keepalive sans JavaScript timer throttling
2. Instantané des panneaux de terminal, transferts SFTP, redirections et fichiers IDE
3. Sonder l’ancienne connexion pendant 30 secondes de Grace Period pour laisser survivre les TUI lors d’un changement réseau
4. Si la récupération échoue, reconnecter, restaurer les redirections, reprendre les transferts et rouvrir les fichiers IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### Pool de connexions SSH et routage par nœud


- Une connexion SSH physique peut servir panneaux de terminal, SFTP, redirections de ports et travail IDE
- Chaque connexion passe par `connecting → active → idle → link_down → reconnecting`
- L’UI adresse `nodeId`; `NodeRouter` résout atomiquement le `connectionId` actif
- `NodeRuntimeStore` persiste les snapshots de topologie dans `session_tree.json`
- La panne d’un jump host propage `link_down` aux nœuds descendants

### OxideSens AI

OxideSens reste BYOK d’abord, avec construction du contexte dans le processus :

- Fournisseurs : OpenAI, Anthropic, Gemini, Ollama ou tout point d’accès OpenAI-compatible
- MCP : transports stdio et SSE, découverte et invocation d’outils
- RAG : BM25 full-text, index vectoriel HNSW, Reciprocal Rank Fusion, tokenizer CJK bigram
- Les messages envoyés aux fournisseurs passent par un filtre de motifs d’identifiants ; le contexte et les actions du workspace restent sous le contrôle de l’utilisateur
- Les clés API sont conservées dans le trousseau système et délibérément exclues des journaux structurés et des messages du cœur de l’application

### Shell desktop GPUI

L’UI est dessinée directement avec GPUI, sans pipeline DOM/CSS/JavaScript :

- Types d’onglets de l’espace de travail : terminaux locaux, SSH, Telnet, série, RDP, VNC, SFTP, IDE, Forwards, Settings, plugins, Topology, etc.
- Arbre binaire de panes avec séparateurs déplaçables, jusqu’à quatre panes par onglet terminal
- Command palette, raccourcis globaux et sidebars construits avec des primitives GPUI
- Immediate-mode rendering réagit à l’état Rust sans round-trip de sérialisation

### État du terminal et rendu

Le rendu terminal est d’abord modélisé comme état Rust, puis dessiné par GPUI :

- La sortie PTY arrive dans `TerminalState` ; scrollback, curseur, sélection, marks et état de recherche restent en Rust
- La rendering policy peut passer entre Boost, Normal et Idle sans attendre un browser event loop
- Les graphiques Sixel et Kitty sont suivis comme assets propres au terminal, pas comme DOM nodes ou canvas overlays
- Les panneaux divisés partagent le même modèle de état de l’espace de travail, ce qui permet à restauration d’onglet et reconnect de prendre un instantané ensemble la topologie terminal

### Workspace SFTP et IDE

Les fichiers distants font partie du même node espace de travail, pas d’une fonction séparée :

- Les sessions SFTP sont résolues via `NodeRouter`, donc reconnect peut remplacer la connexion SSH sous-jacente sans changer l’adresse node de l’UI
- Les transfer queues suivent direction, progression, retry state et speed limits indépendamment des file panes visibles
- Les onglets IDE gardent ensemble dirty buffers, remote paths, conflict state et restore metadata
- Lorsque le backend le permet, les écritures distantes utilisent un staged/atomic behavior pour éviter les partial writes dans le flux d’édition normal

### Plugins, CLI et diagnostics

Les extensions et surfaces de support respectent des limites explicites définies en Rust :

- Les plugins tournent dans une sandbox wasmtime avec capacités hôte typées plutôt que objets globaux du navigateur
- La CLI lie directement les crates de domaine pour doctor, settings, connections, redirections, portable bundles, backups et reports
- Les diagnostics privilégient compteurs, chemins, indicateurs de fonctionnalité et indices expurgés plutôt que des charges utiles brutes porteurs de secrets
- Les flows CLI qui modifient l’état utilisent dry-run plans, `--yes` guards et rollback backups lorsque c’est pertinent

### Redirection de ports — Lock-Free I/O


- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- Un seul task `ssh_io` possède chaque SSH Channel et évite `Arc<Mutex<Channel>>`
- Auto-restauration après reconnexion, rapport de fin et expiration d’inactivité

### trzsz — transfert in-band

trzsz continue d’utiliser le flux terminal, sans port supplémentaire ni agent distant :

- Upload/download via le flux terminal existant
- Fonctionne à travers les chaînes ProxyJump
- Les sélecteurs de fichiers natifs évitent les limites mémoire du navigateur
- Transfert bidirectionnel, dossiers, limites configurables

### Export `.oxide` chiffré


- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF** : 256 MB memory cost, 4 iterations, augmente le coût du brute force GPU
- Couvre connections, redirections, settings, quick commands, réglages de plugin et secrets portables

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

## Technologies

| Couche | Technologie | Notes |
|---|---|---|
| Interface | GPUI (Zed) | Mode immédiat accéléré par GPU, entièrement en Rust |
| Exécution | Tokio + DashMap | Exécution asynchrone et tables concurrentes |
| SSH | russh (`ring`) | Sans OpenSSL/libssh2 dans la pile SSH ; SSH Agent |
| Terminal | portable-pty + alacritty_terminal | PTY locaux, émulation de terminal et graphismes Sixel/Kitty |
| Plugins | wasmtime | Isolation WASM avec API hôte native |
| IA et recherche | SSE + BM25 + HNSW | Diffusion des fournisseurs, bigrammes CJK et fusion RRF |

## Sécurité

| Sujet | Implémentation |
|---|---|
| Identifiants enregistrés | macOS Keychain / Windows Credential Manager / libsecret |
| Secrets en mémoire | Les types contenant des secrets et les tampons temporaires utilisent `zeroize` / `Zeroizing` aux frontières de possession compatibles |
| Diagnostics | Les rapports d’assistance privilégient les métadonnées structurées et les indices expurgés aux charges contenant des secrets |
| Contexte IA | Les messages envoyés aux fournisseurs passent par un filtre de motifs d’identifiants ; le contexte et les actions du workspace restent sous le contrôle de l’utilisateur |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Écritures CLI | dry-run, garde `--yes`, sauvegardes rollback |
| Plugins | isolation wasmtime et API hôte à capacités |

## Avis d’utilisation légale

OxideTerm est distribué sous licence GPL-3.0-only sans restriction de licence supplémentaire. Lors de son utilisation, accédez uniquement aux systèmes, réseaux et appareils qui vous appartiennent ou pour lesquels vous disposez d’une autorisation explicite, et respectez la législation applicable. N’utilisez pas OxideTerm pour un accès non autorisé, une interruption de service ou le contournement de contrôles d’accès.

## Contribution

Les contributions au code, à la documentation, aux traductions, aux plugins, aux tests et aux rapports de bugs sont bienvenues. Discutez les changements importants dans une issue ou envoyez une pull request ciblée pour une correction bien délimitée.

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

---

## Support et maintenance

Les bugs et régressions reproductibles avec diagnostics expurgés sont prioritaires. Les demandes de fonctionnalités sont évaluées selon leur périmètre, leur sûreté et leur alignement avec la direction d’OxideTerm pour le espace de travail de serveurs distants.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Si OxideTerm aide votre workflow, une étoile GitHub, une reproduction, une correction de traduction, un plugin ou une pull request rendent le projet plus facile à maintenir.

---

## Licence

**GPL-3.0-only**. Les avis détaillés sur les composants tiers figurent dans [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md), avec des informations complémentaires dans [`NOTICE`](../../NOTICE).

## Remerciements

Merci à `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` et `tree-sitter`.
