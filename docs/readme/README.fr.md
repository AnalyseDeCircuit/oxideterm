<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Espace de travail SSH local-first pour celles et ceux qui veulent le workflow OxideTerm sans WebView.</strong>
  <br>
  Connectez-vous une fois à une machine distante, puis travaillez avec son shell, ses fichiers, ses ports, ses transferts, un éditeur léger, des consoles série et une IA BYOK depuis un workspace Rust natif.
  <br>
  <strong>Zéro WebView. Zéro OpenSSL. Zéro télémétrie. Zéro abonnement. BYOK-first. Rust pur, de bout en bout.</strong>
</p>

<p align="center">
  <em>Si vous voulez un espace de travail SSH local-first sans Electron, WebView, télémétrie ni abonnement, ajoutez une étoile à OxideTerm pour aider davantage d'utilisateurs SSH à le découvrir.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version">
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

---

> **État de la release :** OxideTerm Native est préparé comme la prochaine grande version d'OxideTerm. Les installateurs publics ne sont pas encore publiés ; lancez-le depuis le code source pour l'instant. Les releases packagées actuelles restent sur la ligne Tauri jusqu'à ce que les installateurs native soient prêts.

## Pourquoi OxideTerm Native ?

| Si vous tenez à... | OxideTerm Native apporte... |
|---|---|
| Un espace SSH, pas seulement un shell | Terminal, SFTP, forwarding, trzsz, mini IDE, monitoring et contexte IA autour d'un noeud |
| Shell local, consoles série et SSH distant ensemble | zsh/bash/fish/pwsh/WSL2, terminaux série locaux et SSH dans le même workflow |
| Aucun compte cloud | SSH, SFTP, forwarding, shell local et configuration restent local-first |
| IA BYOK | Vos propres endpoints OpenAI, Anthropic, Gemini, Ollama ou compatibles |
| Aucun WebView | GPUI dessine directement sur une surface GPU, sans DOM, CSS ni JavaScript |
| Pas de sérialisation sur le chemin critique | Les octets du terminal mutent l'état Rust directement, sans WebSocket/JSON/Base64 |
| Pas d'OpenSSL | SSH pur Rust avec `russh` + `ring` |
| Reconnexion robuste | Grace Period sonde l'ancienne connexion avant de tuer les applications TUI |
| Travail sur fichiers distants | SFTP intégré et IDE natif pour parcourir, prévisualiser, transférer et éditer |
| Sécurité des identifiants | Keychain système ; bundles `.oxide` chiffrés avec ChaCha20-Poly1305 + Argon2id |

## Ce que c'est / ce que ce n'est pas

OxideTerm Native est un **espace de travail SSH desktop natif en Rust pur** et la prochaine grande édition d'OxideTerm. Il porte la même idée produit — terminal, SFTP, forwarding, édition, IA, cloud sync, plugins et CLI autour d'un noeud distant — dans une interface GPUI sans runtime navigateur.

Ce n'est ni Electron, ni Tauri, ni un terminal web, ni un service hébergé. Il n'y a pas de Chromium, WebView, JavaScript ou CSS ; GPUI dessine chaque surface directement.

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

## Lancer depuis le code source

Les installateurs native publics ne sont pas encore publiés. Tant que les builds packagés ne sont pas prêts, lancez l'édition native depuis le code source.

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
- [ ] Installateurs publics packagés
- [ ] ProxyCommand complet, audit logging

## Contribution

## Neutralité des providers

OxideTerm est BYOK-first et neutre vis-à-vis des providers.

Les intégrations de providers servent à aider les utilisateurs à connecter les outils auxquels ils font déjà confiance. Elles ne sont ni un classement, ni un panneau publicitaire, ni un système de récompense pour ceux qui demandent le plus chaleureusement.

La compatibilité, la maintenabilité, la sécurité et la valeur réelle pour les utilisateurs décident de ce qui est documenté. La visibilité suit l'utilité, pas l'enthousiasme.

Quand une fonctionnalité existe déjà dans Tauri, gardez le comportement, les libellés, les états d'interaction et les workflows alignés. Un nouveau crate doit posséder une vraie responsabilité de domaine, pas seulement réexporter du code.

## Licence / Remerciements

**GPL-3.0-only**. Les notices tierces sont dans `NOTICE`. Merci à `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` et `tree-sitter`.
