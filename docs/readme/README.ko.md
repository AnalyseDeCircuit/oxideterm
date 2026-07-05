<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>원격 서버를 위한 AI 기반 네이티브 운영 워크스페이스 — 순수 Rust 네이티브 앱</strong>
  <br>
  SSH, Telnet, 시리얼, RDP/VNC, SFTP, 포트 포워딩, Raw TCP/UDP, 경량 편집을 하나의 네이티브 워크스페이스에.
  <br>
  GPU 직접 렌더링. 무료, 계정 불필요.
  <br>
  <strong>제로 WebView. 제로 OpenSSL. 제로 텔레메트리. 제로 구독. BYOK 우선. 순수 Rust SSH.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.13-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub><a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a>의 다음 주요 네이티브 버전 — GPU 렌더링, WebView 없음, <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a>(Zed 렌더링 프레임워크) 사용</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm Native 기능 개요" width="920">
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens가 OxideTerm 안에서 터미널을 여는 데모" width="920">
</a>

*OxideSens가 사용자 요청을 따라 OxideTerm 안에서 터미널을 여는 모습입니다.*

</div>

---

## OxideTerm Native란

OxideTerm Native는 **순수 Rust GPUI 데스크톱 앱**——SSH, 파일, 포트 포워딩, Raw TCP/UDP, 원격 데스크톱 작업 흐름을 위한 오픈소스 운영 워크스페이스입니다.

**할 수 있는 일:**

- SSH, Telnet, 시리얼, RDP/VNC, SFTP, 포트 포워딩, Raw TCP/UDP, 로컬 셸, 가벼운 편집을 하나의 네이티브 작업 공간에서 관리
- Grace Period 재연결로 네트워크가 흔들려도 원격 작업 유지
- OxideSens AI가 사용자의 AI 제공자로 실행 중인 세션을 확인하고 승인된 작업 공간 동작을 실행하도록 요청

호스팅형 클라우드 에이전트 플랫폼이 아닙니다. Electron, Tauri, 웹 터미널도 아닙니다. Chromium, WebView, JavaScript, CSS가 없습니다.

---

## 왜 OxideTerm Native인가?

| 당신이 중요하게 생각하는 것... | OxideTerm Native가 제공하는 것... |
|---|---|
| 하나의 원격 노드, 많은 도구 | 터미널, SFTP, 포트 포워딩, RDP/VNC, Raw TCP/UDP, trzsz, 네이티브 IDE, 모니터링, OxideSens AI가 동일한 작업 공간에 연결 |
| WebView 없는 네이티브 셸 | GPUI가 GPU 표면에 데스크톱 UI를 직접 그림 — DOM, CSS, JavaScript, Chromium, WebKit 런타임 없음 |
| 로컬 우선 운영 작업 흐름 | SSH, Telnet, SFTP, 포워딩, RDP/VNC, Raw TCP/UDP, 로컬 셸, 시리얼 터미널, 설정 작업에 가입 불필요 |
| BYOK OxideSens AI | OxideSens는 사용자의 OpenAI/Anthropic/Gemini/Ollama/호환 엔드포인트를 MCP, RAG, 승인된 작업 공간 동작과 함께 사용 |
| 재연결 안정성 | Grace Period가 30초 동안 기존 연결을 확인 — TUI 앱이 짧은 네트워크 단절에서도 살아남음 |
| 순수 Rust SSH와 자격 증명 안전성 | `russh` + `ring`, OpenSSL/libssh2 없음; 비밀번호와 API 키는 OS 키체인에 저장, `.oxide` 번들은 ChaCha20-Poly1305 + Argon2id 사용 |

---

## 스크린샷

네이티브 UI는 현재 Tauri 계열과 같은 OxideTerm 작업 공간 모델과 시각 언어를 따릅니다.

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
| 렌더링 | Chromium/Safari/WebKit2GTK + CSS | GPUI GPU 표면, 즉시 모드, 순수 Rust |
| 터미널 데이터 흐름 | WebSocket → JS 이벤트 루프 → xterm.js | Rust 입력 → `TerminalState` → GPUI 렌더링 |
| IPC | 명령별 JSON-RPC | 프로세스 내 함수 호출 |
| SSH keepalive | JavaScript timer | Rust async task |
| 플러그인 실행 환경 | 브라우저 샌드박스의 ESM | wasmtime WASM + 타입화된 Rust 호스트 API |
| CLI | desktop app 필요 | standalone binary |
| 런타임 경계 | 브라우저 런타임 + WebView 브리지 | 네이티브 프로세스, 번들 브라우저 런타임 없음 |

## 기능 개요

| 분류 | 기능 |
|---|---|
| 터미널 | 로컬 PTY, SSH, Telnet, Raw TCP/UDP 터미널, 로컬 시리얼 터미널, 분할 패널, 셸 통합, 명령 표시, asciicast, trzsz, Sixel/Kitty graphics, 렌더링 정책 |
| SSH 및 인증 | 연결 풀, 무제한 ProxyJump, Grace Period 재연결, 호스트 키 TOFU, SSH Agent 포워딩, password/key/cert/keyboard-interactive |
| SFTP / IDE | 듀얼 패널 브라우저, 전송 대기열, 미리보기, 북마크, 원자적 쓰기, 원격 파일 트리, 다중 탭 편집기, 충돌 해결 |
| 포워딩 | Local, Remote, Dynamic SOCKS5, 저장된 규칙, 재연결 복원, 종료 보고, 유휴 시간 초과 |
| 원격 데스크톱 | 내장 RDP/VNC 탭, 재연결 제어, 뷰포트 크기 조정, 키보드, 마우스, 클립보드, 커서 처리 |
| Raw TCP/UDP | 임시 서비스, 장치 프로토콜, 데이터그램 디버깅용 Raw TCP/UDP 터미널 |
| AI | OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG, 명령 승인을 지원하는 OxideSens |
| 클라우드 동기화 / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, 롤백 백업, 암호화 가져오기/내보내기 |
| 플러그인 / CLI | WASM 샌드박스, 네이티브 호스트 API, 플러그인 설정; CLI 명령: settings, connections, 포워딩, plugins, secrets, cloud-sync, backup, report |

## 아키텍처

OxideTerm Native는 WebView 브리지를 제거하고 터미널, SSH, Telnet, RDP, VNC, Raw TCP/UDP, SFTP, 포워딩, IDE, AI, 플러그인, CLI를 하나의 Rust 네이티브 아키텍처 안에 유지합니다. 구현 세부 사항은 아래에 보존했습니다.

<details>
<summary><strong>아키텍처, SSH 내부, GPUI 셸, 재연결, AI, 플러그인 등</strong></summary>
<br>

### 아키텍처 — 단일 프로세스, 브리지 없음

```text
GPUI 렌더링 루프
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
- 전체 SSH2: 키 교환, 채널, SFTP 서브시스템, 포트 포워딩
- ChaCha20-Poly1305 / AES-GCM, Ed25519/RSA/ECDSA 키
- SSH Agent: Unix (`SSH_AUTH_SOCK`)와 Windows (`\\.\pipe\openssh-ssh-agent`)
- 각 hop에서 독립 인증하는 multi-hop ProxyJump

### Grace Period 기반 Smart Reconnect

Reconnect semantics는 Tauri line과 같지만 orchestration은 Rust async task 안에서 완결됩니다.

1. JavaScript timer throttling 없이 SSH keepalive timeout 감지
2. 터미널 패널, SFTP 전송, 포워딩, IDE 파일 스냅샷
3. Grace Period 동안 기존 연결을 30초 probe하여 네트워크 전환 시 TUI apps가 살아남을 수 있게 함
4. 복구 실패 시 재연결, 포워딩 복원, 전송 재개, IDE 파일 재오픈

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-포워딩 → resume-transfers → restore-ide → verify → done`

### SSH Connection Pool 및 Node Routing

`SshConnectionRegistry`는 `DashMap` 기반이며 WebSocket lifecycle bridge 없이 Tauri의 node-first model을 유지합니다.

- 하나의 물리 SSH connection이 터미널 패널, SFTP, 포트 포워딩, IDE work를 공유
- 각 connection은 `connecting → active → idle → link_down → 재연결ing` 상태를 이동
- UI는 `nodeId`로 command를 보내고 `NodeRouter`가 active `connectionId`를 atomic하게 resolve
- `NodeRuntimeStore`가 topology 스냅샷s를 `session_tree.json`에 persist
- jump host failure는 downstream nodes에 `link_down`을 cascade

### OxideSens AI

OxideSens는 BYOK 우선을 유지하며 컨텍스트 구성은 프로세스 안에서 수행됩니다.

- 제공자: OpenAI, Anthropic, Gemini, Ollama 또는 OpenAI 호환 엔드포인트
- MCP: stdio/SSE transports, tool discovery, invocation
- RAG: BM25 전문 검색, HNSW 벡터 인덱스, Reciprocal Rank Fusion, CJK bigram tokenizer
- AI 컨텍스트는 작업 공간 상태에서 만들어지며 자격 증명은 제공자 호출 전에 마스킹됩니다
- API 키는 OS 키체인에 저장되고 로그 또는 IPC 프레임에 들어가지 않음

### GPUI Desktop Shell

UI는 GPUI로 직접 그려지며 DOM/CSS/JavaScript rendering pipeline이 없습니다.

- 작업 공간 탭 유형: local terminal, SSH, Telnet, Serial, RDP, VNC, Raw TCP/UDP, SFTP, IDE, Forwards, Settings, Plugin, Topology 등
- draggable dividers를 가진 binary pane tree, terminal tab당 최대 4 panes
- Command palette, global key bindings, sidebars는 GPUI primitives
- Immediate-mode rendering은 serialization round-trip 없이 Rust state에 반응

### 터미널 상태와 렌더링

Terminal rendering은 먼저 Rust state로 모델링되고 GPUI가 그립니다.

- PTY 출력은 `TerminalState`로 들어가며 scrollback, cursor, selection, marks, search state는 Rust 안에 유지됩니다
- 렌더링 policy는 Boost, Normal, Idle 사이를 전환할 수 있고 브라우저 이벤트 루프 협조를 기다리지 않습니다
- Sixel과 Kitty graphics는 DOM nodes나 canvas overlays가 아니라 terminal-owned assets로 추적됩니다
- 분할 패널은 같은 작업 공간 상태 모델을 공유하므로 탭 복원과 재연결이 터미널 토폴로지를 함께 스냅샷할 수 있습니다

### SFTP 및 IDE Workspace

원격 파일은 분리된 부가 기능이 아니라 같은 노드 작업 공간의 일부입니다.

- SFTP sessions는 `NodeRouter`를 통해 resolve되어 재연결가 underlying SSH connection을 교체해도 UI의 node address는 유지됩니다
- Transfer queues는 보이는 file panes와 독립적으로 direction, progress, retry state, speed limits를 추적합니다
- IDE tabs는 dirty buffers, remote paths, conflict state, restore metadata를 함께 보관합니다
- Backend가 지원하면 remote writes는 staged/atomic behavior를 사용해 일반 edit flow에서 partial writes를 줄입니다

### 플러그인, CLI, 진단

Native branch는 extension과 support surfaces를 Rust-native boundaries 안에 둡니다.

- 플러그인은 브라우저 전역 객체 대신 타입화된 호스트 기능을 사용하며 wasmtime 샌드박스에서 실행됩니다
- CLI는 도메인 crate에 직접 link되어 doctor, settings, connections, 포워딩, 휴대용 번들, backups, reports를 다룹니다
- 진단은 비밀이 포함된 원시 페이로드보다 개수, 경로, 기능 플래그, 마스킹된 힌트를 우선합니다
- 상태를 변경하는 CLI 흐름은 dry-run 계획, `--yes` 보호, 롤백 백업을 사용합니다

### Port Forwarding — Lock-Free I/O

Forwarding은 standalone Rust crate에서 Tauri semantics를 유지합니다.

- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- 하나의 `ssh_io` task가 각 SSH Channel을 소유하여 `Arc<Mutex<Channel>>` 회피
- 재연결 auto-restore, 종료 보고, 유휴 시간 초과

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
- connections, 포워딩, settings, quick commands, 플러그인 설정, 휴대용 비밀 포함

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
| 비밀번호와 키 | macOS Keychain / Windows Credential Manager / libsecret |
| Secret memory | `zeroize` / `Zeroizing` |
| 진단 및 AI 컨텍스트 | 비밀 값은 출력 또는 제공자 호출 전에 마스킹됩니다 |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI writes | dry-run 계획, `--yes` 보호, 롤백 백업 |
| Plugins | wasmtime 격리와 능력 기반 호스트 API |

## Release Status

- [x] SSH Agent 포워딩, Grace Period 재연결, GPUI desktop shell
- [x] WebSocket 없는 프로세스 내 터미널 데이터 흐름
- [x] SFTP, forwarding, IDE, AI, 클라우드 동기화, plugins, CLI
- [x] 로컬 시리얼 및 Telnet 터미널
- [x] RDP/VNC 원격 데스크톱 및 Raw TCP/UDP 터미널
- [x] Full ProxyCommand
- [ ] Audit logging

## Contributing

## 제공자 중립성

OxideTerm은 BYOK 우선이며 제공자 중립을 유지합니다.

제공자 통합은 사용자가 이미 신뢰하는 도구에 연결하도록 돕기 위한 것입니다. 순위표, 광고판, 또는 가장 적극적으로 요청한 쪽을 보상하는 시스템이 아닙니다.

문서에 무엇을 올릴지는 호환성, 유지보수성, 보안, 실제 사용자 가치가 결정합니다. 가시성은 유용성을 따르며 열정의 크기를 따르지 않습니다.

Tauri 버전에 이미 있는 기능을 native로 옮길 때는 명시적인 대체 설계가 없는 한 behavior, labels, interaction states, workflows를 맞춰야 합니다. 새 crate는 re-export만 하는 shell이 아니라 실제 domain responsibility를 가져야 합니다.

## 지원 및 유지관리

재현 단계와 마스킹된 진단이 포함된 버그 보고 및 회귀를 우선합니다. 기능 요청은 범위, 안전성, OxideTerm의 원격 서버 작업 공간 방향성과의 일치 여부를 기준으로 검토합니다.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

OxideTerm이 workflow에 도움이 된다면 GitHub star, issue reproduction, translation fix, plugin, pull request가 프로젝트 지속에 도움이 됩니다.

---

## License / Acknowledgments

**GPL-3.0-only**. Third-party notices are recorded in `NOTICE`. Thanks to `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, and `tree-sitter`.
