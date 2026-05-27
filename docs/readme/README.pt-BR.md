<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <em>Se você quer um workspace SSH local-first sem Electron, WebView, telemetria ou assinatura, dê uma estrela ao OxideTerm para que mais usuários de SSH possam encontrá-lo.</em>
</p>

<p align="center">
  <strong>Workspace SSH local-first: shell, SFTP, port forwarding, trzsz, edição remota e AI BYOK em torno de um nó remoto.</strong>
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Rust puro de ponta a ponta.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Versão">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plataforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licença">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Reescrita nativa em Rust do <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — renderizada por GPU, zero-WebView, usando <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (framework de renderização do Zed)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

---

## Por que OxideTerm Native?

| Se você se importa com... | OxideTerm Native entrega... |
|---|---|
| Workspace SSH, não só shell | Terminal, SFTP, forwarding, trzsz, mini IDE, monitoring e contexto AI em torno de um nó |
| Shell local e SSH remoto | zsh/bash/fish/pwsh/WSL2 e SSH no mesmo fluxo |
| Sem conta cloud | SSH, SFTP, forwarding, shell local e config funcionam local-first |
| AI BYOK | Seus próprios endpoints OpenAI, Anthropic, Gemini, Ollama ou compatíveis |
| Sem WebView | GPUI desenha direto em uma GPU surface, sem DOM, CSS ou JavaScript |
| Sem serialização no hot path | Bytes do terminal alteram estado Rust direto, sem WebSocket/JSON/Base64 |
| Sem OpenSSL | SSH puro Rust com `russh` + `ring` |
| Reconnect estável | Grace Period testa a conexão antiga antes de matar apps TUI |
| Arquivos remotos | SFTP integrado e IDE nativa para navegar, pré-visualizar, transferir e editar |
| Segurança de credenciais | Keychain do SO; `.oxide` com ChaCha20-Poly1305 + Argon2id |

## O que é / o que não é

OxideTerm Native é um **workspace SSH desktop nativo em Rust puro**. Terminal, SFTP, forwarding, edição, AI, cloud sync, plugins e CLI da versão Tauri são reimplementados em Rust com UI GPUI.

Não é Electron, Tauri, terminal web ou serviço hospedado. Não há Chromium, WebView, JavaScript ou CSS.

## Diferenças em relação ao WebView/Tauri

| Aspecto | WebView/Tauri | Native |
|---|---|---|
| Rendering | Chromium/Safari/WebKit2GTK + CSS | GPUI, GPU surface, immediate mode, Rust puro |
| Fluxo terminal | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC por comando | Chamadas in-process |
| SSH keepalive | Timer JavaScript | Rust async task |
| Plugins | ESM em sandbox do navegador | WASM wasmtime + typed Rust host API |
| CLI | Requer app desktop | Binário standalone |

## Funcionalidades

| Categoria | Funcionalidades |
|---|---|
| Terminal | Local PTY, SSH, split panes, shell integration, command marks, asciicast, trzsz, Sixel/Kitty graphics, rendering policy |
| SSH & Auth | connection pool, unlimited ProxyJump, Grace Period reconnect, Host-key TOFU, SSH Agent forwarding, password/key/cert/keyboard-interactive |
| SFTP / IDE | dual-pane browser, transfer queue, preview, bookmarks, atomic writes, remote file tree, multi-tab editor, conflict resolution |
| Forwarding | Local, Remote, Dynamic SOCKS5, saved rules, reconnect restore, death reporting, idle timeout |
| AI | OxideSens com OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG e aprovação de comandos |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, import/export criptografado |
| Plugins / CLI | WASM sandbox, native host API, plugin settings; CLI para settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Arquitetura

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

Não existe fronteira de serialização entre UI e backend SSH/terminal. Os bytes do terminal modificam `TerminalState` diretamente; GPUI lê o estado e emite draw calls GPU.

## Início rápido

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

## Segurança

| Tema | Implementação |
|---|---|
| Passwords & keys | macOS Keychain / Windows Credential Manager / libsecret |
| Memória secreta | `zeroize` / `Zeroizing` |
| Diagnóstico & contexto AI | valores secretos são redigidos antes de saída ou provider calls |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Escritas CLI | dry-run plans, proteções `--yes`, rollback backups |
| Plugins | isolamento wasmtime e capability-based host API |

## Roadmap / Contribuição

- [x] SSH Agent forwarding, Grace Period reconnect, GPUI desktop shell
- [x] Fluxo terminal in-process sem WebSocket
- [x] SFTP, forwarding, IDE, AI, cloud sync, plugins, CLI
- [ ] Full ProxyCommand, audit logging, packaged release builds

## Neutralidade de providers

OxideTerm é BYOK-first e neutro em relação a providers.

Integrações de providers existem para ajudar usuários a conectar as ferramentas em que já confiam. Elas não são um ranking, um outdoor ou um sistema de recompensa para quem pede atenção com mais entusiasmo.

Compatibilidade, manutenibilidade, segurança e valor real para usuários decidem o que entra na documentação. Visibilidade segue utilidade, não entusiasmo.

Quando uma função já existir na versão Tauri, mantenha comportamento, textos, estados de interação e workflows alinhados. Cada crate novo precisa ter responsabilidade real, não apenas re-export.

## Licença / Agradecimentos

**GPL-3.0-only**. Notices de terceiros ficam em `NOTICE`. Obrigado a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` e `tree-sitter`.
