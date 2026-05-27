<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <em>Electron, WebView, telemetry, subscription 없는 local-first SSH workspace가 필요하다면 OxideTerm에 Star를 눌러 더 많은 SSH 사용자가 찾을 수 있게 해 주세요.</em>
</p>

<p align="center">
  <strong>Local-first SSH workspace: shell, SFTP, port forwarding, trzsz, remote editing, BYOK AI를 하나의 remote node 주변에 통합합니다.</strong>
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. 끝까지 Pure Rust.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub><a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a>의 native Rust rewrite — GPU-rendered, zero-WebView, <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a>(Zed rendering framework) 사용</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

---

## 왜 Native인가?

| 중요하게 보는 것 | OxideTerm Native가 제공하는 것 |
|---|---|
| shell 이상의 SSH workspace | terminal, SFTP, forwarding, trzsz, mini IDE, monitoring, AI context를 하나의 node에 통합 |
| local shell과 remote SSH | zsh/bash/fish/pwsh/WSL2와 remote SSH를 같은 workflow에서 사용 |
| cloud account 불필요 | SSH, SFTP, forwarding, local shell, config는 local-first |
| BYOK AI | OpenAI, Anthropic, Gemini, Ollama, compatible endpoint 사용 |
| WebView 없음 | DOM/CSS/JavaScript 없이 GPUI가 GPU surface에 직접 렌더링 |
| hot path serialization 없음 | terminal bytes가 Rust state를 직접 변경하고 WebSocket/JSON/Base64를 거치지 않음 |
| OpenSSL 없음 | `russh` + `ring` 기반 pure Rust SSH |
| reconnect 안정성 | Grace Period가 기존 connection을 먼저 probe해 TUI app을 보호 |
| remote file 작업 | built-in SFTP와 native IDE로 browse, preview, transfer, edit |
| credential 안전성 | OS keychain과 `.oxide` ChaCha20-Poly1305 + Argon2id encryption |

## 무엇인가 / 무엇이 아닌가

OxideTerm Native는 **pure-Rust native desktop SSH workspace**입니다. Tauri 버전의 terminal, SFTP, forwarding, editing, AI, cloud sync, plugins, CLI를 Rust와 GPUI UI layer로 재구현합니다.

Electron, Tauri, web terminal, hosted service가 아닙니다. Chromium, WebView, JavaScript, CSS가 없고 모든 UI는 GPUI가 GPU surface에 직접 그립니다.

## WebView 버전과의 차이

| Aspect | WebView/Tauri | Native |
|---|---|---|
| Rendering | Chromium/Safari/WebKit2GTK + CSS | GPUI GPU surface, immediate mode, pure Rust |
| Terminal data flow | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC per command | in-process function calls |
| SSH keepalive | JavaScript timer | Rust async task |
| Plugin runtime | ESM in browser sandbox | wasmtime WASM + typed Rust host API |
| CLI | desktop app 필요 | standalone binary |
| 배포 아티팩트 크기 | 보통 약 150–200 MB 설치 파일 | 현재 macOS arm64: 압축 portable/DMG 약 50–60 MB, 원본 release binary 약 132 MB |

## Feature Overview

| Category | Features |
|---|---|
| Terminal | Local PTY, SSH, split panes, shell integration, command marks, asciicast, trzsz, Sixel/Kitty graphics, rendering policy |
| SSH & Auth | connection pool, unlimited ProxyJump, Grace Period reconnect, Host-key TOFU, SSH Agent forwarding, password/key/cert/keyboard-interactive |
| SFTP / IDE | dual-pane browser, transfer queue, preview, bookmarks, atomic writes, remote file tree, multi-tab editor, conflict resolution |
| Forwarding | Local, Remote, Dynamic SOCKS5, saved rules, reconnect restore, death reporting, idle timeout |
| AI | OxideSens with OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG, command approval |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backup, encrypted import/export |
| Plugins / CLI | WASM sandbox, native host API, plugin settings; CLI commands for settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

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

UI와 SSH/terminal backend 사이에 serialization boundary가 없습니다. Terminal bytes는 `TerminalState`를 직접 변경하고 GPUI가 state를 읽어 GPU draw call을 발행합니다.

## Quick Start

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

## Security

| Concern | Implementation |
|---|---|
| Passwords & keys | macOS Keychain / Windows Credential Manager / libsecret |
| Secret memory | `zeroize` / `Zeroizing` |
| Diagnostics & AI context | secret values are redacted before output or provider calls |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI writes | dry-run plans, `--yes` guards, rollback backups |
| Plugins | wasmtime isolation and capability-based host API |

## Roadmap / Contributing

- [x] SSH Agent forwarding, Grace Period reconnect, GPUI desktop shell
- [x] in-process terminal data flow without WebSocket
- [x] SFTP, forwarding, IDE, AI, cloud sync, plugins, CLI
- [ ] Full ProxyCommand, audit logging, packaged release builds

## Provider Neutrality

OxideTerm은 BYOK-first이며 provider-neutral을 유지합니다.

Provider integration은 사용자가 이미 신뢰하는 도구에 연결하도록 돕기 위한 것입니다. leaderboard, billboard, 또는 가장 적극적으로 요청한 쪽을 보상하는 시스템이 아닙니다.

문서에 무엇을 올릴지는 compatibility, maintainability, security, 그리고 실제 user value가 결정합니다. Visibility는 usefulness를 따르며 enthusiasm을 따르지 않습니다.

Tauri 버전에 이미 있는 기능을 native로 옮길 때는 명시적인 대체 설계가 없는 한 behavior, labels, interaction states, workflows를 맞춰야 합니다. 새 crate는 re-export만 하는 shell이 아니라 실제 domain responsibility를 가져야 합니다.

## License / Acknowledgments

**GPL-3.0-only**. Third-party notices are recorded in `NOTICE`. Thanks to `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, and `tree-sitter`.
