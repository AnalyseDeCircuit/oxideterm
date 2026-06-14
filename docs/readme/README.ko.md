<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>AI 기반 SSH 터미널 · SFTP 브라우저 · 포트 포워딩 · 시리얼 콘솔 · mini IDE —— 순수 Rust 네이티브 앱 </strong>
  <br>
  GPU 직접 렌더링. 무료, 계정 불필요.
  <br>
  <strong>제로 WebView. 제로 OpenSSL. 제로 텔레메트리. 제로 구독. BYOK 우선. 순수 Rust SSH.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.7-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub><a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a>의 next major native edition — GPU-rendered, zero-WebView, <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a>(Zed rendering framework) 사용</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens가 OxideTerm 안에서 터미널을 여는 데모" width="920">
</a>

*OxideSens가 사용자 요청을 따라 OxideTerm 안에서 터미널을 여는 모습입니다.*

</div>

---

## OxideTerm Native란

OxideTerm Native는 **순수 Rust GPUI 데스크톱 앱**——Termius & SecureCRT의 오픈소스 대안입니다.

**할 수 있는 일:**

- SSH 및 Telnet terminal, SFTP, port forward, serial console, local shell, lightweight editing을 하나의 native workspace에서 관리
- Grace Period reconnect로 네트워크가 흔들려도 원격 작업 유지
- OxideSens AI가 사용자의 AI provider로 live session을 확인하고 승인된 workspace action을 실행하도록 요청

hosted cloud agent platform도 아닙니다. Electron, Tauri, web terminal도 아닙니다. Chromium, WebView, JavaScript, CSS가 없습니다.

---

## 왜 OxideTerm Native인가?

| 당신이 중요하게 생각하는 것... | OxideTerm Native가 제공하는 것... |
|---|---|
| 하나의 원격 노드, 많은 도구 | Terminal, SFTP, port forwarding, trzsz, native IDE, monitoring, OxideSens AI가 동일 SSH workspace에 연결 |
| Zero WebView native shell | GPUI가 GPU surface에서 직접 데스크톱 UI를 그림 — DOM, CSS, JavaScript, Chromium, WebKit runtime 없음 |
| Local-first SSH workflows | SSH, Telnet, SFTP, forwarding, local shell, serial terminal, config 작업에 가입 불필요 |
| BYOK OxideSens AI | OxideSens는 사용자의 OpenAI/Anthropic/Gemini/Ollama/compatible endpoint를 MCP, RAG, 승인된 workspace action과 함께 사용 |
| Reconnect stability | Grace Period가 30초 동안 기존 연결을 probe — TUI apps가 짧은 네트워크 단절에서도 살아남음 |
| 순수 Rust SSH와 credential safety | `russh` + `ring`, OpenSSL/libssh2 없음; 비밀번호와 API 키는 OS keychain에 저장, `.oxide` 번들은 ChaCha20-Poly1305 + Argon2id 사용 |

---

## 스크린샷

Native UI는 현재 Tauri line과 같은 OxideTerm workspace model과 visual language를 따릅니다.

<table>
<tr>
<td align="center"><strong>SSH 터미널 + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="OxideSens AI가 포함된 SSH 터미널" /></td>
<td align="center"><strong>SFTP 파일 관리자</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="전송 큐가 포함된 SFTP 이중 패널 파일 관리자" /></td>
</tr>
<tr>
<td align="center"><strong>내장 IDE</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="내장 IDE 모드" /></td>
<td align="center"><strong>스마트 포트 포워딩</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="자동 감지 기능이 있는 스마트 포트 포워딩" /></td>
</tr>
</table>

---

## WebView 버전과의 차이

| Aspect | WebView/Tauri | Native |
|---|---|---|
| Rendering | Chromium/Safari/WebKit2GTK + CSS | GPUI GPU surface, immediate mode, pure Rust |
| Terminal data flow | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC per command | in-process function calls |
| SSH keepalive | JavaScript timer | Rust async task |
| Plugin runtime | ESM in browser sandbox | wasmtime WASM + typed Rust host API |
| CLI | desktop app 필요 | standalone binary |
| 런타임 경계 | 브라우저 runtime + WebView bridge | native process, bundled browser runtime 없음 |

## Feature Overview

| Category | Features |
|---|---|
| Terminal | Local PTY, SSH, Telnet, local serial terminals, split panes, shell integration, command marks, asciicast, trzsz, Sixel/Kitty graphics, rendering policy |
| SSH & Auth | connection pool, unlimited ProxyJump, Grace Period reconnect, Host-key TOFU, SSH Agent forwarding, password/key/cert/keyboard-interactive |
| SFTP / IDE | dual-pane browser, transfer queue, preview, bookmarks, atomic writes, remote file tree, multi-tab editor, conflict resolution |
| Forwarding | Local, Remote, Dynamic SOCKS5, saved rules, reconnect restore, death reporting, idle timeout |
| AI | OxideSens with OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG, command approval |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backup, encrypted import/export |
| Plugins / CLI | WASM sandbox, native host API, plugin settings; CLI commands for settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Architecture

OxideTerm Native는 WebView bridge를 제거하고 terminal, SSH, Telnet, SFTP, forwarding, IDE, AI, plugins, CLI를 하나의 Rust-native architecture 안에 유지합니다. 구현 세부 사항은 아래에 보존했습니다.

<details>
<summary><strong>Architecture, SSH internals, GPUI shell, reconnect, AI, plugins 등</strong></summary>
<br>

### Architecture — Single-Process, Zero-Bridge

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

### 순수 Rust SSH — russh (ring)

Native edition은 Tauri line과 같은 `russh` stack을 desktop binary에 직접 link합니다.

- `ring` 기반으로 **OpenSSL 의존성 없음**
- 전체 SSH2: key exchange, channels, SFTP subsystem, port forwarding
- ChaCha20-Poly1305 / AES-GCM, Ed25519/RSA/ECDSA keys
- SSH Agent: Unix (`SSH_AUTH_SOCK`)와 Windows (`\\.\pipe\openssh-ssh-agent`)
- 각 hop에서 독립 인증하는 multi-hop ProxyJump

### Grace Period 기반 Smart Reconnect

Reconnect semantics는 Tauri line과 같지만 orchestration은 Rust async task 안에서 완결됩니다.

1. JavaScript timer throttling 없이 SSH keepalive timeout 감지
2. terminal panes, SFTP transfers, forwards, IDE files snapshot
3. Grace Period 동안 기존 연결을 30초 probe하여 네트워크 전환 시 TUI apps가 살아남을 수 있게 함
4. 복구 실패 시 재연결, forwards 복원, transfers 재개, IDE files 재오픈

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH Connection Pool 및 Node Routing

`SshConnectionRegistry`는 `DashMap` 기반이며 WebSocket lifecycle bridge 없이 Tauri의 node-first model을 유지합니다.

- 하나의 물리 SSH connection이 terminal panes, SFTP, port forwards, IDE work를 공유
- 각 connection은 `connecting → active → idle → link_down → reconnecting` 상태를 이동
- UI는 `nodeId`로 command를 보내고 `NodeRouter`가 active `connectionId`를 atomic하게 resolve
- `NodeRuntimeStore`가 topology snapshots를 `session_tree.json`에 persist
- jump host failure는 downstream nodes에 `link_down`을 cascade

### OxideSens AI

OxideSens는 BYOK-first를 유지하며 context building은 in-process로 수행됩니다.

- Providers: OpenAI, Anthropic, Gemini, Ollama 또는 OpenAI-compatible endpoint
- MCP: stdio/SSE transports, tool discovery, invocation
- RAG: BM25 full-text, HNSW vector index, Reciprocal Rank Fusion, CJK bigram tokenizer
- AI context는 workspace state에서 만들어지며 credentials는 provider call 전에 redact
- API keys는 OS keychain에 저장되고 logs 또는 IPC frames에 들어가지 않음

### GPUI Desktop Shell

UI는 GPUI로 직접 그려지며 DOM/CSS/JavaScript rendering pipeline이 없습니다.

- 17 workspace tab types: local terminal, SSH terminal, Telnet terminal, SFTP, IDE, Forwards, Settings, Plugin, Topology 등
- draggable dividers를 가진 binary pane tree, terminal tab당 최대 4 panes
- Command palette, global key bindings, sidebars는 GPUI primitives
- Immediate-mode rendering은 serialization round-trip 없이 Rust state에 반응

### Terminal State와 Rendering

Terminal rendering은 먼저 Rust state로 모델링되고 GPUI가 그립니다.

- PTY output은 `TerminalState`로 들어가며 scrollback, cursor, selection, marks, search state는 Rust 안에 유지됩니다
- Rendering policy는 Boost, Normal, Idle 사이를 전환할 수 있고 browser event loop 협조를 기다리지 않습니다
- Sixel과 Kitty graphics는 DOM nodes나 canvas overlays가 아니라 terminal-owned assets로 추적됩니다
- Split panes는 같은 workspace state model을 공유하므로 tab restore와 reconnect가 terminal topology를 함께 snapshot할 수 있습니다

### SFTP 및 IDE Workspace

Remote files는 분리된 부가 기능이 아니라 같은 node workspace의 일부입니다.

- SFTP sessions는 `NodeRouter`를 통해 resolve되어 reconnect가 underlying SSH connection을 교체해도 UI의 node address는 유지됩니다
- Transfer queues는 보이는 file panes와 독립적으로 direction, progress, retry state, speed limits를 추적합니다
- IDE tabs는 dirty buffers, remote paths, conflict state, restore metadata를 함께 보관합니다
- Backend가 지원하면 remote writes는 staged/atomic behavior를 사용해 일반 edit flow에서 partial writes를 줄입니다

### Plugins, CLI, Diagnostics

Native branch는 extension과 support surfaces를 Rust-native boundaries 안에 둡니다.

- Plugins는 browser globals 대신 typed host capabilities를 사용하며 wasmtime sandbox에서 실행됩니다
- CLI는 domain crates에 직접 link되어 doctor, settings, connections, forwards, portable bundles, backups, reports를 다룹니다
- Diagnostics는 raw secret-bearing payloads보다 counts, paths, feature flags, redacted hints를 우선합니다
- 상태를 변경하는 CLI flows는 dry-run plans, `--yes` guards, rollback backups를 사용합니다

### Port Forwarding — Lock-Free I/O

Forwarding은 standalone Rust crate에서 Tauri semantics를 유지합니다.

- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- 하나의 `ssh_io` task가 각 SSH Channel을 소유하여 `Arc<Mutex<Channel>>` 회피
- reconnect auto-restore, death reporting, idle timeout

### trzsz — In-Band File Transfer

trzsz는 계속 terminal stream을 사용하며 extra port나 remote agent가 필요 없습니다.

- 기존 terminal stream을 통한 upload/download
- ProxyJump chains를 통과해 동작
- Native file pickers로 browser memory limits 회피
- bidirectional transfer, directory support, configurable limits

### `.oxide` Encrypted Export

Encrypted bundle format은 Tauri line과 동일합니다.

- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations로 GPU brute-force cost 증가
- connections, forwards, settings, quick commands, plugin settings, portable secrets 포함

</details>

---

## Source에서 실행

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

## Release Status

- [x] SSH Agent forwarding, Grace Period reconnect, GPUI desktop shell
- [x] in-process terminal data flow without WebSocket
- [x] SFTP, forwarding, IDE, AI, cloud sync, plugins, CLI
- [x] Local serial and Telnet terminals
- [x] Full ProxyCommand
- [ ] Audit logging

## Contributing

## Provider Neutrality

OxideTerm은 BYOK-first이며 provider-neutral을 유지합니다.

Provider integration은 사용자가 이미 신뢰하는 도구에 연결하도록 돕기 위한 것입니다. leaderboard, billboard, 또는 가장 적극적으로 요청한 쪽을 보상하는 시스템이 아닙니다.

문서에 무엇을 올릴지는 compatibility, maintainability, security, 그리고 실제 user value가 결정합니다. Visibility는 usefulness를 따르며 enthusiasm을 따르지 않습니다.

Tauri 버전에 이미 있는 기능을 native로 옮길 때는 명시적인 대체 설계가 없는 한 behavior, labels, interaction states, workflows를 맞춰야 합니다. 새 crate는 re-export만 하는 shell이 아니라 실제 domain responsibility를 가져야 합니다.

## 지원 및 유지관리

재현 단계와 redacted diagnostics가 포함된 bug report 및 regression을 우선합니다. feature request는 범위, 안전성, OxideTerm의 remote-server workspace 방향성과의 일치 여부를 기준으로 검토합니다.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

OxideTerm이 workflow에 도움이 된다면 GitHub star, issue reproduction, translation fix, plugin, pull request가 프로젝트 지속에 도움이 됩니다.

---

## License / Acknowledgments

**GPL-3.0-only**. Third-party notices are recorded in `NOTICE`. Thanks to `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, and `tree-sitter`.
