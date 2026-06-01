<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>OxideTerm의 다음 zero-WebView edition입니다.</strong>
  <br>
  원격 머신에 한 번 연결한 뒤 shell, 파일, 포트, 전송, 가벼운 편집기, 시리얼 콘솔, BYOK AI를 하나의 네이티브 Rust 워크스페이스에서 다룹니다.
  <br>
  네이티브 GPUI 앱 · 순수 Rust SSH · 핵심 SSH 워크플로에는 계정 불필요
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust all the way down.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version">
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

> **Release status:** OxideTerm Native는 OxideTerm의 다음 major release로 준비 중입니다. Public installer는 아직 공개되지 않았으므로 지금은 source에서 실행해 주세요. Native installer가 준비될 때까지 현재 packaged release는 Tauri line에 남아 있습니다.

## 할 수 있는 일

- SSH terminal, SFTP, port forward, serial console, local shell, lightweight editing을 하나의 native workspace에서 관리
- Grace Period reconnect로 네트워크가 흔들려도 원격 작업 유지
- 직접 선택한 AI provider로 live session을 확인하고 승인된 workspace action 실행

---

## 왜 Native인가?

| 관심사 | OxideTerm Native가 제공하는 것 |
|---|---|
| 하나의 remote node, 여러 도구 | Terminal, SFTP, port forwarding, trzsz, native IDE, monitoring, AI context가 같은 SSH workspace에 붙어 있습니다 |
| Zero WebView native shell | GPUI가 GPU surface에 desktop UI를 직접 그리며 DOM, CSS, JavaScript, Chromium, WebKit runtime이 없습니다 |
| Local-first SSH workflows | SSH, SFTP, forwarding, local shell, serial terminals, config가 signup 없이 동작합니다 |
| Platform credit 대신 BYOK AI | OxideSens는 OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible endpoint를 사용하며 MCP와 RAG를 지원합니다 |
| 재연결 안정성 | Grace Period가 기존 연결을 30초간 probe한 뒤 교체하므로 짧은 네트워크 끊김에도 TUI가 살아남을 수 있습니다 |
| 순수 Rust SSH와 자격 증명 안전 | `russh` + `ring`, OpenSSL/libssh2 없음. 비밀번호와 API key는 OS keychain에, `.oxide`는 ChaCha20-Poly1305 + Argon2id 사용 |

## 무엇인가 / 무엇이 아닌가

OxideTerm Native는 OxideTerm과 같은 **local-first SSH workspace**에 집중하며, 이를 pure Rust GPUI desktop app으로 다시 만든 버전입니다. Terminal, file, port, transfer, lightweight editing, serial console, AI context를 자신의 machine과 remote node 중심에 두고 싶은 사용자를 위한 것입니다.

아직 현재 stable download line이 아니며, hosted cloud agent platform도 아닙니다. Electron, Tauri, web terminal도 아닙니다. Chromium, WebView, JavaScript, CSS가 없습니다.

---

## 스크린샷

Native UI는 현재 Tauri line과 같은 OxideTerm workspace model과 visual language를 따릅니다.

<table>
<tr>
<td align="center"><strong>SSH 터미널 + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="OxideSens AI 사이드바가 포함된 SSH 터미널" /></td>
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
| 배포 아티팩트 크기 | 보통 약 150–200 MB 설치 파일 | 현재 macOS arm64: 압축 portable/DMG 약 50–60 MB, 원본 release binary 약 132 MB |

## Feature Overview

| Category | Features |
|---|---|
| Terminal | Local PTY, SSH, local serial terminals, split panes, shell integration, command marks, asciicast, trzsz, Sixel/Kitty graphics, rendering policy |
| SSH & Auth | connection pool, unlimited ProxyJump, Grace Period reconnect, Host-key TOFU, SSH Agent forwarding, password/key/cert/keyboard-interactive |
| SFTP / IDE | dual-pane browser, transfer queue, preview, bookmarks, atomic writes, remote file tree, multi-tab editor, conflict resolution |
| Forwarding | Local, Remote, Dynamic SOCKS5, saved rules, reconnect restore, death reporting, idle timeout |
| AI | OxideSens with OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG, command approval |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backup, encrypted import/export |
| Plugins / CLI | WASM sandbox, native host API, plugin settings; CLI commands for settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Architecture

OxideTerm Native는 WebView bridge를 제거하고 terminal, SSH, SFTP, forwarding, IDE, AI, plugins, CLI를 하나의 Rust-native architecture 안에 유지합니다. 구현 세부 사항은 아래에 보존했습니다.

<details>
<summary><strong>Architecture, SSH internals, GPUI shell, reconnect, AI, plugins 등</strong></summary>
<br>

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

</details>

---

## Source에서 실행

Public native installer는 아직 공개되지 않았습니다. Packaged build가 준비될 때까지 native edition은 source에서 실행해 주세요.

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
- [x] Local serial terminals
- [ ] Public packaged installers
- [ ] Full ProxyCommand, audit logging

## Contributing

## Provider Neutrality

OxideTerm은 BYOK-first이며 provider-neutral을 유지합니다.

Provider integration은 사용자가 이미 신뢰하는 도구에 연결하도록 돕기 위한 것입니다. leaderboard, billboard, 또는 가장 적극적으로 요청한 쪽을 보상하는 시스템이 아닙니다.

문서에 무엇을 올릴지는 compatibility, maintainability, security, 그리고 실제 user value가 결정합니다. Visibility는 usefulness를 따르며 enthusiasm을 따르지 않습니다.

Tauri 버전에 이미 있는 기능을 native로 옮길 때는 명시적인 대체 설계가 없는 한 behavior, labels, interaction states, workflows를 맞춰야 합니다. 새 crate는 re-export만 하는 shell이 아니라 실제 domain responsibility를 가져야 합니다.

## 지원 및 유지관리

OxideTerm Native는 OxideTerm의 다음 major release로 준비 중이며 best-effort로 유지관리됩니다. 재현 단계와 redacted diagnostics가 포함된 bug report를 우선합니다. feature request는 항상 구현되지 않을 수 있습니다.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

OxideTerm이 workflow에 도움이 된다면 GitHub star, issue reproduction, translation fix, plugin, pull request가 프로젝트 지속에 도움이 됩니다.

---

## License / Acknowledgments

**GPL-3.0-only**. Third-party notices are recorded in `NOTICE`. Thanks to `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, and `tree-sitter`.
