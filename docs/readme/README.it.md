<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Workspace SSH local-first per chi vuole il workflow OxideTerm senza WebView.</strong>
  <br>
  Connettiti una volta a una macchina remota, poi lavora con shell, file, porte, trasferimenti, editor leggero, console seriali e AI BYOK da un workspace Rust nativo.
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Rust puro, fino in fondo.</strong>
</p>

<p align="center">
  <em>Se vuoi uno spazio SSH local-first senza Electron, WebView, telemetria o abbonamenti, lascia una star a OxideTerm così più utenti SSH potranno trovarlo.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Versione">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Piattaforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licenza">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Prossima grande edizione nativa di <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — renderizzata su GPU, zero-WebView, usando <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (framework di rendering di Zed)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

---

> **Stato release:** OxideTerm Native è in preparazione come prossima grande versione di OxideTerm. Gli installer pubblici non sono ancora disponibili; per ora eseguilo dal sorgente. Le release pacchettizzate attuali restano sulla linea Tauri finché gli installer native non saranno pronti.

## Perché OxideTerm Native?

| Se ti interessa... | OxideTerm Native offre... |
|---|---|
| Workspace SSH, non solo shell | Terminal, SFTP, forwarding, trzsz, mini IDE, monitoring e contesto AI attorno a un nodo |
| Shell locale, console seriali e SSH remoto | zsh/bash/fish/pwsh/WSL2, terminali seriali locali e SSH nello stesso workflow |
| Nessun account cloud | SSH, SFTP, forwarding, shell locale e config sono local-first |
| AI BYOK | Endpoint OpenAI, Anthropic, Gemini, Ollama o compatibili tuoi |
| Nessuna WebView | GPUI disegna direttamente su GPU surface, senza DOM, CSS o JavaScript |
| Nessuna serializzazione hot path | I byte del terminale mutano stato Rust direttamente, senza WebSocket/JSON/Base64 |
| Nessun OpenSSL | SSH pure Rust con `russh` + `ring` |
| Reconnect stabile | Grace Period controlla la vecchia connessione prima di terminare app TUI |
| File remoti | SFTP integrato e IDE nativo per browse, preview, transfer, edit |
| Sicurezza credenziali | OS keychain; `.oxide` cifrato con ChaCha20-Poly1305 + Argon2id |

## Cos'è / cosa non è

OxideTerm Native è un **workspace SSH desktop nativo in Rust puro** e la prossima grande edizione di OxideTerm. Porta la stessa idea di prodotto — terminale, SFTP, forwarding, editing, AI, cloud sync, plugin e CLI attorno a un nodo remoto — in una UI GPUI senza runtime browser.

Non è Electron, Tauri, un terminale web o un servizio hosted. Non ci sono Chromium, WebView, JavaScript o CSS.

## Differenze da WebView/Tauri

| Aspetto | WebView/Tauri | Native |
|---|---|---|
| Rendering | Chromium/Safari/WebKit2GTK + CSS | GPUI, GPU surface, immediate mode, pure Rust |
| Terminal data flow | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC per command | Chiamate in-process |
| SSH keepalive | Timer JavaScript | Rust async task |
| Plugin runtime | ESM in browser sandbox | WASM wasmtime + typed Rust host API |
| CLI | Richiede la desktop app | Binario standalone |
| Dimensione artefatto | Installer di solito ~150–200 MB | macOS arm64 attuale: portable/DMG compresso ~50–60 MB; binario release grezzo ~132 MB |

## Funzionalità

| Categoria | Funzioni |
|---|---|
| Terminal | Local PTY, SSH, local serial terminals, split panes, shell integration, command marks, asciicast, trzsz, Sixel/Kitty graphics, rendering policy |
| SSH & Auth | connection pool, unlimited ProxyJump, Grace Period reconnect, Host-key TOFU, SSH Agent forwarding, password/key/cert/keyboard-interactive |
| SFTP / IDE | dual-pane browser, transfer queue, preview, bookmarks, atomic writes, remote file tree, multi-tab editor, conflict resolution |
| Forwarding | Local, Remote, Dynamic SOCKS5, saved rules, reconnect restore, death reporting, idle timeout |
| AI | OxideSens con OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG e approvazione comandi |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, import/export cifrato |
| Plugins / CLI | WASM sandbox, native host API, plugin settings; CLI per settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Architettura

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

Non c'è confine di serializzazione tra UI e backend SSH/terminal. I byte del terminale modificano direttamente `TerminalState`; GPUI legge lo stato ed emette draw call GPU.

## Eseguire dal sorgente

Gli installer native pubblici non sono ancora disponibili. Finché i build pacchettizzati non saranno pronti, esegui l'edizione native dal sorgente.

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

## Sicurezza

| Tema | Implementazione |
|---|---|
| Password & keys | macOS Keychain / Windows Credential Manager / libsecret |
| Memoria segreta | `zeroize` / `Zeroizing` |
| Diagnostica & contesto AI | valori segreti redatti prima di output o provider call |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Scritture CLI | dry-run plans, guardie `--yes`, rollback backups |
| Plugins | isolamento wasmtime e capability-based host API |

## Stato release

- [x] SSH Agent forwarding, Grace Period reconnect, GPUI desktop shell
- [x] Terminal data flow in-process senza WebSocket
- [x] SFTP, forwarding, IDE, AI, cloud sync, plugins, CLI
- [x] Terminali seriali locali
- [ ] Installer pubblici pacchettizzati
- [ ] Full ProxyCommand, audit logging

## Contribuire

## Neutralità dei provider

OxideTerm è BYOK-first e neutrale rispetto ai provider.

Le integrazioni dei provider esistono per aiutare gli utenti a collegare gli strumenti di cui già si fidano. Non sono una classifica, uno spazio pubblicitario o un sistema di ricompensa per chi chiede attenzione con più entusiasmo.

Compatibilità, manutenibilità, sicurezza e valore reale per gli utenti decidono cosa viene documentato. La visibilità segue l'utilità, non l'entusiasmo.

Quando una funzione esiste già nella versione Tauri, mantieni comportamento, label, stati di interazione e workflow allineati. Ogni nuovo crate deve avere una responsabilità reale, non solo re-export.

## Licenza / Ringraziamenti

**GPL-3.0-only**. Le notice di terze parti sono in `NOTICE`. Grazie a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` e `tree-sitter`.
