<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>원격 서버를 위한 AI 기반 네이티브 운영 워크스페이스 — 순수 Rust 네이티브 앱</strong>
  <br>
  SSH, Telnet, 시리얼, RDP/VNC, SFTP, 포트 포워딩, 경량 편집을 하나의 네이티브 워크스페이스에.
  <br>
  GPU 직접 렌더링. 무료, 계정 불필요.
  <br>
  <strong>Electron 미사용. WebView 번들 없음. 텔레메트리 없음. 구독 없음. BYOK 우선. OpenSSL/libssh2 없는 순수 Rust SSH.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.9-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>오픈 소스, 로컬 우선, GPUI 기반 GPU 렌더링.</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm 기능 개요" width="920">
</p>

---

## OxideTerm란

OxideTerm는 SSH와 원격 운영을 위한 오픈 소스 작업 공간입니다. 터미널, 파일, 포트 포워딩, 호스트 도구, 원격 데스크톱을 한곳에서 다룹니다.

**할 수 있는 작업:**

- SSH, Telnet, 시리얼, RDP/VNC, SFTP, 포트 포워딩, 로컬 셸, 가벼운 편집을 하나의 작업 공간에서 관리
- Grace Period 재연결로 짧은 네트워크 중단 중에도 원격 작업 유지
- 자체 AI 공급자를 통해 OxideSens에 활성 세션 점검과 승인된 작업 공간 작업 실행 요청

연결 정보와 운영 데이터는 사용자가 관리합니다. OxideSens는 사용자의 AI 공급자를 사용하며 계정이 필요하지 않습니다.

---

## 왜 OxideTerm인가?

- SSH, Telnet, 시리얼, RDP/VNC, SFTP, 포트 포워딩, 로컬 셸을 하나의 데스크톱 앱에 통합
- 짧은 네트워크 중단을 견디는 Grace Period 재연결
- 자체 AI 자격 증명과 승인된 작업을 사용하는 OxideSens
- Electron과 번들 브라우저 런타임이 없는 GPUI 인터페이스

---

## 스크린샷

아래 스크린샷은 OxideTerm의 터미널, 파일, 편집, 포워딩 흐름을 보여 줍니다.

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

## 원격 운영을 위한 설계

OxideTerm는 연결, 파일, 포워딩, 호스트 도구, 자동화, AI 컨텍스트를 하나의 Rust 작업 공간에 둡니다. 도구는 같은 서버 ID와 세션 수명 주기를 공유합니다.

---

## 기능

| 범주 | 기능 |
|---|---|
| **터미널 및 연결** | 로컬 셸, SSH, Telnet, 시리얼, 분할 창, 멀티홉, 안정적인 재연결 |
| **파일 및 원격 편집** | SFTP, 전송 대기열, 즐겨찾기, 안전한 쓰기, 프로젝트 트리, 탭 편집 |
| **포워딩 및 네트워크** | 로컬·원격·동적 SOCKS5 포워딩, 저장된 규칙, 소켓 디버깅 |
| **호스트 운영 및 원격 데스크톱** | 모니터링, 프로세스, 서비스, 로그, 포트, 작업, 디스크, 패키지, 컨테이너, tmux, RDP, VNC |
| **OxideSens 및 자동화** | 자체 AI 공급자, MCP, 로컬 RAG, 승인된 작업, 암호화 동기화, CLI |
| **확장 및 개인화** | WASM 플러그인, 사용자 탭, 빠른 명령, 테마, 배경, 단축키, 11개 언어 |

---

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens가 OxideTerm 안에서 터미널을 여는 데모" width="920">
</a>

*OxideSens가 사용자 요청을 따라 OxideTerm 안에서 터미널을 여는 모습입니다.*

</div>

---

## 아키텍처

OxideTerm는 터미널, SSH, Telnet, RDP, VNC, SFTP, 포워딩, IDE, AI, 플러그인, CLI를 하나의 Rust 아키텍처에 통합합니다. 기술 세부 사항은 아래에 설명합니다.

<details>
<summary><strong>아키텍처, SSH 내부, GPUI 셸, 재연결, AI, 플러그인 등</strong></summary>
<br>

### 아키텍처 — 프로세스 내부 코어, WebView 브리지 없음

```text
GPUI 렌더링 루프
  WorkspaceApp / 탭 화면 / GPUI 뷰
        │ in-process Arc<> / async
도메인 Crate
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

UI와 SSH/터미널 백엔드 사이에는 직렬화 경계가 없습니다. 터미널 바이트는 `TerminalState`를 직접 변경하고 GPUI는 상태를 읽어 GPU 그리기 명령을 발행합니다.

### 순수 Rust SSH — russh (ring)


- **SSH 스택에 OpenSSL/libssh2 없음** — SSH 암호화는 `ring`으로 제공
- 전체 SSH2: 키 교환, 채널, SFTP 서브시스템, 포트 포워딩
- ChaCha20-Poly1305 / AES-GCM, Ed25519/RSA/ECDSA 키
- SSH Agent: Unix (`SSH_AUTH_SOCK`)와 Windows (`\\.\pipe\openssh-ssh-agent`)
- 각 홉에서 독립적으로 인증하는 다중 홉 ProxyJump

### Grace Period 기반 스마트 재연결


1. JavaScript timer throttling 없이 SSH keepalive timeout 감지
2. 터미널 패널, SFTP 전송, 포워딩, IDE 파일 스냅샷
3. Grace Period 동안 기존 연결을 30초 probe하여 네트워크 전환 시 TUI apps가 살아남을 수 있게 함
4. 복구 실패 시 재연결, 포워딩 복원, 전송 재개, IDE 파일 재오픈

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-포워딩 → resume-transfers → restore-ide → verify → done`

### SSH 연결 풀 및 노드 라우팅


- 하나의 물리 SSH connection이 터미널 패널, SFTP, 포트 포워딩, IDE work를 공유
- 각 연결은 `connecting → active → idle → link_down → reconnecting` 상태를 이동
- UI는 `nodeId`로 command를 보내고 `NodeRouter`가 active `connectionId`를 atomic하게 resolve
- `NodeRuntimeStore`가 topology 스냅샷s를 `session_tree.json`에 persist
- 점프 호스트 장애는 하위 노드에 `link_down` 상태를 연쇄 전파

### OxideSens AI

OxideSens는 BYOK 우선을 유지하며 컨텍스트 구성은 프로세스 안에서 수행됩니다.

- 제공자: OpenAI, Anthropic, Gemini, Ollama 또는 OpenAI 호환 엔드포인트
- MCP: stdio/SSE 전송, 도구 검색과 호출
- RAG: BM25 전문 검색, HNSW 벡터 인덱스, Reciprocal Rank Fusion, CJK 바이그램 토크나이저
- 제공자에게 보낼 메시지는 자격 증명 패턴을 마스킹하며, 작업 공간 컨텍스트와 작업은 사용자가 제어합니다
- API 키는 OS 키체인에 저장하며 구조화된 로그와 데스크톱 코어 메시지 대상에서 명시적으로 제외합니다

### GPUI 데스크톱 셸

UI는 GPUI로 직접 그려지며 DOM/CSS/JavaScript rendering pipeline이 없습니다.

- 작업 공간 탭 유형: local terminal, SSH, Telnet, Serial, RDP, VNC, SFTP, IDE, Forwards, Settings, Plugin, Topology 등
- draggable dividers를 가진 binary pane tree, terminal tab당 최대 4 panes
- Command palette, global key bindings, sidebars는 GPUI primitives
- 즉시 모드 렌더링은 직렬화 왕복 없이 Rust 상태 변화에 반응

### 터미널 상태와 렌더링

터미널 렌더링은 먼저 Rust 상태로 모델링되고 GPUI가 그립니다.

- PTY 출력은 `TerminalState`로 들어가며 scrollback, cursor, selection, marks, search state는 Rust 안에 유지됩니다
- 렌더링 policy는 Boost, Normal, Idle 사이를 전환할 수 있고 브라우저 이벤트 루프 협조를 기다리지 않습니다
- Sixel과 Kitty graphics는 DOM nodes나 canvas overlays가 아니라 terminal-owned assets로 추적됩니다
- 분할 패널은 같은 작업 공간 상태 모델을 공유하므로 탭 복원과 재연결이 터미널 토폴로지를 함께 스냅샷할 수 있습니다

### SFTP 및 IDE 작업 공간

원격 파일은 분리된 부가 기능이 아니라 같은 노드 작업 공간의 일부입니다.

- SFTP sessions는 `NodeRouter`를 통해 resolve되어 재연결가 underlying SSH connection을 교체해도 UI의 node address는 유지됩니다
- 전송 대기열은 보이는 파일 창과 독립적으로 방향, 진행률, 재시도 상태, 속도 제한을 추적합니다
- IDE 탭은 수정된 버퍼, 원격 경로, 충돌 상태, 복원 메타데이터를 함께 보관합니다
- Backend가 지원하면 remote writes는 staged/atomic behavior를 사용해 일반 edit flow에서 partial writes를 줄입니다

### 플러그인, CLI, 진단

확장 기능과 지원 기능은 Rust가 소유하는 명확한 경계 안에서 동작합니다.

- 플러그인은 브라우저 전역 객체 대신 타입화된 호스트 기능을 사용하며 wasmtime 샌드박스에서 실행됩니다
- CLI는 도메인 crate에 직접 링크되어 doctor, settings, connections, 포워딩, 휴대용 번들, 백업, 보고서를 다룹니다
- 진단은 비밀이 포함된 원시 페이로드보다 개수, 경로, 기능 플래그, 마스킹된 힌트를 우선합니다
- 상태를 변경하는 CLI 흐름은 dry-run 계획, `--yes` 보호, 롤백 백업을 사용합니다

### 포트 포워딩 — 잠금 없는 I/O


- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- 하나의 `ssh_io` task가 각 SSH Channel을 소유하여 `Arc<Mutex<Channel>>` 회피
- 재연결 auto-restore, 종료 보고, 유휴 시간 초과

### trzsz — 대역 내 파일 전송

trzsz는 계속 terminal stream을 사용하며 extra port나 remote agent가 필요 없습니다.

- 기존 terminal stream을 통한 upload/download
- ProxyJump chains를 통과해 동작
- Native file pickers로 browser memory limits 회피
- bidirectional transfer, directory support, configurable limits

### `.oxide` 암호화 내보내기


- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations로 GPU brute-force cost 증가
- connections, 포워딩, settings, quick commands, 플러그인 설정, 휴대용 비밀 포함

</details>

---

## 소스에서 실행

```sh
cargo run
OXIDETERM_RENDER_PROFILE=compatibility cargo run
./scripts/build/build-cli.sh
./scripts/build/build-agent.sh
```

## CLI

```sh
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

## 기술 스택

| 계층 | 기술 | 설명 |
|---|---|---|
| UI | GPUI (Zed) | GPU 기반 즉시 모드, 순수 Rust |
| 런타임 | Tokio + DashMap | 비동기 실행과 동시성 맵 |
| SSH | russh (`ring`) | SSH 스택에 OpenSSL/libssh2 없음, SSH Agent 지원 |
| 터미널 | portable-pty + alacritty_terminal | 로컬 PTY, 터미널 에뮬레이션, Sixel/Kitty 그래픽 |
| 플러그인 | wasmtime | 네이티브 호스트 API를 갖춘 WASM 격리 |
| AI 및 검색 | SSE + BM25 + HNSW | 제공자 스트리밍, CJK 바이그램, RRF 결합 |

## 보안

| Concern | Implementation |
|---|---|
| 저장된 자격 증명 | macOS Keychain / Windows Credential Manager / libsecret |
| 메모리의 비밀 정보 | 비밀 정보를 소유한 타입과 임시 버퍼는 지원되는 소유권 경계에서 `zeroize` / `Zeroizing` 사용 |
| 진단 | 지원 출력은 비밀 정보를 담은 원문보다 구조화된 메타데이터와 마스킹된 힌트를 우선 |
| AI 컨텍스트 | 제공자에게 보낼 메시지는 자격 증명 패턴을 마스킹하며, 워크스페이스 컨텍스트와 작업은 사용자가 제어 |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI writes | dry-run 계획, `--yes` 보호, 롤백 백업 |
| Plugins | wasmtime 격리와 능력 기반 호스트 API |

## 적법한 사용 안내

OxideTerm은 추가 라이선스 제한 없이 GPL-3.0-only로 배포됩니다. 사용할 때에는 본인이 소유하거나 명시적인 접근 권한을 받은 시스템, 네트워크 및 장치에만 접근하고 관련 법률을 준수하십시오. 무단 접근, 서비스 방해 또는 접근 통제 우회에 OxideTerm을 사용하지 마십시오.

## 기여

코드, 문서, 번역, 플러그인, 테스트, 버그 재현 기여를 환영합니다. 큰 변경은 Issue에서 먼저 논의하고, 범위가 명확한 수정은 Pull Request로 보내 주세요.

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

---

## 지원 및 유지관리

재현 단계와 마스킹된 진단이 포함된 버그 보고 및 회귀를 우선합니다. 기능 요청은 범위, 안전성, OxideTerm의 원격 서버 작업 공간 방향성과의 일치 여부를 기준으로 검토합니다.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

OxideTerm이 작업 흐름에 도움이 된다면 GitHub 스타, 문제 재현 보고, 번역 수정, 플러그인, 풀 리퀘스트가 프로젝트 지속에 도움이 됩니다.

---

## 라이선스

**GPL-3.0-only**. 자세한 제3자 고지는 [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md)에, 추가 고지는 [`NOTICE`](../../NOTICE)에 기록되어 있습니다.

## 감사의 말

`russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, `tree-sitter` 프로젝트에 감사드립니다.
