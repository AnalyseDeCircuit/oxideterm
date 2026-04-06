<p align="center">
  <img src="../../src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
  <br>
  <em>OxideTerm이 마음에 드신다면 GitHub에서 별 ⭐️을 눌러주세요!</em>
</p>


<p align="center">
  <strong>Electron 제로. OpenSSL 제로. 순수 Rust SSH.</strong>
  <br>
  <em>네이티브 바이너리 하나로 — 로컬 셸, SSH, SFTP, 원격 IDE, AI, 포트 포워딩, 플러그인, 30개 이상 테마, 11개 언어.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.1.0--beta.5-blue" alt="버전">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="플랫폼">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="라이선스">
  <img src="https://img.shields.io/badge/rust-1.85+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
</p>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases/latest">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=%EC%B5%9C%EC%8B%A0%20%EB%B2%84%EC%A0%84%20%EB%8B%A4%EC%9A%B4%EB%A1%9C%EB%93%9C&style=for-the-badge&color=brightgreen" alt="최신 버전 다운로드">
  </a>
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?include_prereleases&label=%EC%B5%9C%EC%8B%A0%20Beta%20%EB%8B%A4%EC%9A%B4%EB%A1%9C%EB%93%9C&style=for-the-badge&color=orange" alt="최신 Beta 다운로드">
  </a>
</p>

<p align="center">
  🌐 <strong><a href="https://oxideterm.app">oxideterm.app</a></strong> — Documentation & website
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

> [!NOTE]
> **라이선스 변경:** v1.0.0부터 OxideTerm의 라이선스가 **PolyForm Noncommercial 1.0.0**에서 **GPL-3.0(GNU General Public License v3.0)**으로 변경되었습니다. 이제 OxideTerm은 완전한 오픈소스이며, GPL-3.0 라이선스 조건에 따라 자유롭게 사용, 수정 및 배포할 수 있습니다. 자세한 내용은 [LICENSE](../../LICENSE) 파일을 참조하세요.

---

<div align="center">

https://github.com/user-attachments/assets/4ba033aa-94b5-4ed4-980c-5c3f9f21db7e

*🤖 OxideSens AI — "로컬 터미널을 열고 echo hello, world!를 실행해 줘"*

</div>

---

## 왜 OxideTerm인가?

| 문제점 | OxideTerm의 대답 |
|---|---|
| 로컬 셸을 지원하지 않는 SSH 클라이언트 | **하이브리드 엔진**: 로컬 PTY(zsh/bash/fish/pwsh/WSL2)와 원격 SSH를 하나의 창에 통합 |
| 재연결하면 모든 것을 잃음 | **Grace Period 재연결**: 연결 종료 전 30초간 기존 연결 프로브 — vim/htop/yazi가 그대로 살아남음 |
| 원격 파일 편집에 VS Code Remote 필요 | **내장 IDE**: CodeMirror 6 over SFTP, 30개 이상 언어, 선택적으로 Linux용 약 1 MB 원격 에이전트 |
| SSH 연결 재사용 불가 | **다중화**: 터미널, SFTP, 포워드, IDE가 참조 카운팅 풀로 하나의 SSH 연결 공유 |
| SSH 라이브러리가 OpenSSL에 의존 | **russh 0.59**: `ring`으로 컴파일된 순수 Rust SSH — C 의존성 제로 |
| 100 MB 이상의 Electron 앱 | **Tauri 2.0**: 네이티브 Rust 백엔드, 25~40 MB 바이너리 |
| AI가 특정 프로바이더에 종속 | **OxideSens**: 40개 이상 도구, MCP 프로토콜, RAG 지식 베이스 — OpenAI/Ollama/DeepSeek/호환 API 지원 |
| 자격 증명이 일반 텍스트 설정에 저장 | **OS 키체인만 사용**: 비밀번호와 API 키는 디스크에 기록되지 않음; `.oxide` 파일은 ChaCha20-Poly1305 + Argon2id 암호화 |
| 클라우드 종속, 계정 필수 도구 | **로컬 우선**: 계정 없음, 텔레메트리 없음, 클라우드 동기화 없음 — 데이터는 내 기기에만. AI 키는 직접 제공 |

---

## 스크린샷

<table>
<tr>
<td align="center"><strong>SSH 터미널 + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="OxideSens AI 사이드바가 포함된 SSH 터미널" /></td>
<td align="center"><strong>SFTP 파일 관리자</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="전송 큐가 포함된 SFTP 이중 패널 파일 관리자" /></td>
</tr>
<tr>
<td align="center"><strong>내장 IDE (CodeMirror 6)</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="CodeMirror 6 에디터가 탑재된 내장 IDE 모드" /></td>
<td align="center"><strong>스마트 포트 포워딩</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="자동 감지 기능이 있는 스마트 포트 포워딩" /></td>
</tr>
</table>

---

## 기능 개요

| 카테고리 | 기능 |
|---|---|
| **터미널** | 로컬 PTY(zsh/bash/fish/pwsh/WSL2), SSH 원격, 분할 창, 브로드캐스트 입력, 세션 녹화/재생(asciicast v2), WebGL 렌더링, 30개 이상 테마 + 커스텀 에디터, 커맨드 팔레트(`⌘K`), Zen 모드 |
| **SSH 및 인증** | 연결 풀링 및 다중화, ProxyJump(무제한 홉) + 토폴로지 그래프, Grace Period 자동 재연결, Agent 포워딩. 인증: 비밀번호, SSH 키(RSA/Ed25519/ECDSA), SSH Agent, 인증서, keyboard-interactive 2FA, Known Hosts TOFU |
| **SFTP** | 이중 패널 브라우저, 드래그 앤 드롭, 스마트 미리보기(이미지/동영상/오디오/코드/PDF/Hex/폰트), 진행률 및 ETA가 포함된 전송 큐, 북마크, 아카이브 추출 |
| **IDE 모드** | CodeMirror 6, 30개 이상 언어, 파일 트리 + Git 상태, 멀티 탭, 충돌 해결, 통합 터미널. Linux용 선택적 원격 에이전트(9종 추가 아키텍처) |
| **포트 포워딩** | Local (-L), Remote (-R), Dynamic SOCKS5 (-D), 무잠금 메시지 패싱 I/O, 재연결 시 자동 복원, 종료 보고, 유휴 타임아웃 |
| **AI (OxideSens)** | 인라인 패널(`⌘I`) + 사이드바 채팅, 터미널 버퍼 캡처(단일/전체 창), 멀티 소스 컨텍스트(IDE/SFTP/Git), 40개 이상 자율 도구, MCP 서버 통합, RAG 지식 베이스(BM25 + 벡터 하이브리드 검색), 스트리밍 SSE |
| **플러그인** | 런타임 ESM 로딩, 18개 API 네임스페이스, 24개 UI Kit 컴포넌트, 동결 API + Proxy ACL, 서킷 브레이커, 오류 시 자동 비활성화 |
| **CLI** | `oxt` 컴패니언: JSON-RPC 2.0 over Unix Socket / Named Pipe, `status`/`list`/`ping`, 사람 읽기 & JSON 출력 |
| **보안** | .oxide 암호화 내보내기(ChaCha20-Poly1305 + Argon2id 256 MB), OS 키체인, Touch ID(macOS), 호스트 키 TOFU, `zeroize` 메모리 클리어 |
| **i18n** | 11개 언어: EN, 简体中文, 繁體中文, 日本語, 한국어, FR, DE, ES, IT, PT-BR, VI |

---

## 내부 구조

### 아키텍처 — 이중 평면 통신

OxideTerm은 터미널 데이터와 제어 명령을 두 개의 독립적인 평면으로 분리합니다:

```
┌─────────────────────────────────────┐
│        Frontend (React 19)          │
│  xterm.js 6 (WebGL) + 19 stores    │
└──────────┬──────────────┬───────────┘
           │ Tauri IPC    │ WebSocket (binary)
           │ (JSON)       │ per-session port
┌──────────▼──────────────▼───────────┐
│         Backend (Rust)              │
│  NodeRouter → SshConnectionRegistry │
│  Wire Protocol v1                   │
│  [Type:1][Length:4][Payload:n]       │
└─────────────────────────────────────┘
```

- **데이터 평면(WebSocket)**: 각 SSH 세션이 전용 WebSocket 포트를 가집니다. 터미널 바이트는 Type-Length-Payload 헤더가 포함된 바이너리 프레임으로 전송됩니다 — JSON 직렬화 없음, Base64 인코딩 없음, 핫 패스의 오버헤드 제로.
- **제어 평면(Tauri IPC)**: 연결 관리, SFTP 작업, 포워딩, 설정 — 구조화된 JSON이지만 크리티컬 패스 밖에 위치.
- **노드 우선 주소 지정**: 프론트엔드는 `sessionId`나 `connectionId`를 직접 다루지 않습니다. 모든 것이 `nodeId`로 지정되고, 서버 측 `NodeRouter`가 원자적으로 해석합니다. SSH 재연결로 내부 `connectionId`가 변경되어도 SFTP, IDE, 포워드는 전혀 영향을 받지 않습니다.

### 🔩 순수 Rust SSH — russh 0.59

전체 SSH 스택이 **`ring`** 암호화 백엔드로 컴파일된 **russh 0.59**로 구성됩니다:

- **C/OpenSSL 의존성 제로** — 전체 암호화 스택이 Rust 구현. "어떤 버전의 OpenSSL인가?" 디버깅 불필요.
- 완전한 SSH2 프로토콜: 키 교환, 채널, SFTP 서브시스템, 포트 포워딩
- ChaCha20-Poly1305 및 AES-GCM 암호 스위트, Ed25519/RSA/ECDSA 키
- 커스텀 **`AgentSigner`**: 시스템 SSH Agent를 래핑하고 russh의 `Signer` 트레이트를 구현. `.await`를 넘을 때의 RPITIT `Send` 바운드 문제를 `&AgentIdentity`를 소유 값으로 클론하여 해결

```rust
pub struct AgentSigner { /* wraps system SSH Agent */ }
impl Signer for AgentSigner { /* challenge-response via Agent IPC */ }
```

- **플랫폼 지원**: Unix(`SSH_AUTH_SOCK`), Windows(`\\.\pipe\openssh-ssh-agent`)
- **프록시 체인**: 각 홉이 독립적으로 Agent 인증 사용
- **재연결**: `AuthMethod::Agent`가 자동으로 리플레이

### 🔄 Grace Period를 통한 스마트 재연결

대부분의 SSH 클라이언트는 연결이 끊기면 모든 것을 종료하고 처음부터 시작합니다. OxideTerm의 재연결 오케스트레이터는 근본적으로 다른 접근 방식을 취합니다:

1. **감지** WebSocket 하트비트 타임아웃(300초, macOS App Nap 및 JS 타이머 스로틀링에 최적화)
2. **스냅샷** 전체 상태 저장: 터미널 창, 진행 중인 SFTP 전송, 활성 포트 포워드, 열린 IDE 파일
3. **지능형 프로빙**: `visibilitychange` + `online` 이벤트가 능동적 SSH keepalive를 트리거(수동 15~30초 타임아웃 대비 약 2초 감지)
4. **Grace Period**(30초): 기존 SSH 연결을 keepalive로 프로브 — 복구되면(예: WiFi AP 전환), TUI 앱(vim, htop, yazi)이 완전히 무사히 생존
5. 복구 실패 시 → 새 SSH 연결 → 포워드 자동 복원 → SFTP 전송 재개 → IDE 파일 재오픈

파이프라인: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

모든 로직은 전용 `ReconnectOrchestratorStore`를 통해 실행됩니다 — 훅이나 컴포넌트에 재연결 코드가 흩어지지 않습니다.

### 🛡️ SSH 연결 풀

`DashMap`을 백엔드로 한 참조 카운팅 방식의 `SshConnectionRegistry`로 무잠금 동시 접근 구현:

- **하나의 연결, 여러 소비자**: 터미널, SFTP, 포트 포워드, IDE가 하나의 물리적 SSH 연결 공유 — 불필요한 TCP 핸드셰이크 없음
- **연결별 상태 머신**: `connecting → active → idle → link_down → reconnecting`
- **라이프사이클 관리**: 설정 가능한 유휴 타임아웃(5분 / 15분 / 30분 / 1시간 / 무제한), 15초 keepalive 간격, 하트비트 장애 감지
- **WsBridge 하트비트**: 30초 간격, 5분 타임아웃 — macOS App Nap 및 브라우저 JS 스로틀링 허용
- **캐스케이드 전파**: 점프 호스트 장애 → 모든 다운스트림 노드 자동 `link_down` 마킹, 상태 동기화
- **유휴 연결 해제**: 프론트엔드에 `connection_status_changed` 발행(내부 `node:state`만이 아닌), UI 비동기화 방지

### 🤖 OxideSens AI

프라이버시 우선 AI 어시스턴트, 이중 인터랙션 모드:

- **인라인 패널**(`⌘I`): 빠른 터미널 명령, 출력은 괄호 붙여넣기로 삽입
- **사이드바 채팅**: 전체 히스토리를 포함한 지속적 대화
- **컨텍스트 캡처**: Terminal Registry가 활성 창 또는 모든 분할 창에서 버퍼를 동시 수집, IDE 파일, SFTP 경로, Git 상태 자동 삽입
- **40개 이상 자율 도구**: 파일 작업, 프로세스 관리, 네트워크 진단, TUI 앱 상호작용, 텍스트 처리 — AI가 수동 트리거 없이 호출
- **MCP 지원**: 외부 [Model Context Protocol](https://modelcontextprotocol.io) 서버(stdio & SSE) 연결로 서드파티 도구 통합
- **RAG 지식 베이스**(v0.20): Markdown/TXT 문서를 범위별 컬렉션(글로벌 또는 연결별)으로 가져오기. Reciprocal Rank Fusion으로 BM25 키워드 인덱스 + 벡터 코사인 유사도의 하이브리드 검색 융합. Markdown 인식 청킹으로 제목 계층 보존. CJK 바이그램 토크나이저로 중국어/일본어/한국어 지원.
- **프로바이더**: OpenAI, Ollama, DeepSeek, OneAPI, 또는 임의의 `/v1/chat/completions` 엔드포인트
- **보안**: API 키는 OS 키체인에 저장, macOS에서는 키 읽기 시 `LAContext` 기반 **Touch ID** 인증 게이트 — 엔타이틀먼트나 코드 서명 불필요, 세션당 첫 인증 후 캐시

### 💻 IDE 모드 — 원격 편집

SFTP 위에서 동작하는 CodeMirror 6 에디터 — 기본적으로 서버 측 설치 불필요:

- **파일 트리**: 지연 로딩 디렉터리, Git 상태 표시기(수정/미추적/추가)
- **24개 언어 모드**: 14개 네이티브 CodeMirror + `@codemirror/legacy-modes` 레거시 모드
- **충돌 해결**: 낙관적 mtime 잠금 — 덮어쓰기 전 원격 변경 감지
- **이벤트 기반 Git**: 저장, 생성, 삭제, 이름 변경, 터미널 Enter 키 입력 시 자동 새로고침
- **State Gating**: `readiness !== 'ready'`일 때 모든 IO 차단, Key-Driven Reset으로 재연결 시 완전 리마운트 강제
- **원격 에이전트**(선택적): 약 1 MB Rust 바이너리, x86_64/aarch64 Linux에 자동 배포. 9종 추가 아키텍처(ARMv7, RISC-V64, LoongArch64, s390x, Power64LE, i686, ARM, Android aarch64, FreeBSD x86_64)는 `agents/extra/`에 수동 업로드 가능. 향상된 파일 트리, 심볼 검색, 파일 감시 활성화.

### 🔀 포트 포워딩 — 무잠금 I/O

완전한 Local (-L), Remote (-R), Dynamic SOCKS5 (-D) 포워딩:

- **메시지 패싱 아키텍처**: SSH Channel은 단일 `ssh_io` 태스크가 소유 — `Arc<Mutex<Channel>>` 없음, 뮤텍스 경합 완전 제거
- **종료 보고**: 포워드 태스크가 종료 사유(SSH 연결 끊김, 원격 포트 닫힘, 타임아웃)를 능동적으로 보고하여 명확한 진단 제공
- **자동 복원**: `Suspended` 상태의 포워드가 재연결 시 사용자 개입 없이 자동 재개
- **유휴 타임아웃**: `FORWARD_IDLE_TIMEOUT`(300초)으로 좀비 연결 누적 방지

### 🔌 런타임 플러그인 시스템

보안이 강화된 동결 API 표면을 갖춘 동적 ESM 로딩:

- **PluginContext API**: 18개 네임스페이스 — terminal, ui, commands, settings, lifecycle, events, storage, system
- **24개 UI Kit 컴포넌트**: 플러그인 샌드박스에 `window.__OXIDE__`를 통해 주입되는 사전 빌드 React 컴포넌트(버튼, 입력, 다이얼로그, 테이블…)
- **보안 멤브레인**: 모든 컨텍스트 객체에 `Object.freeze`, Proxy 기반 ACL, IPC 화이트리스트, 반복 오류 시 자동 비활성화 서킷 브레이커
- **공유 모듈**: React, ReactDOM, zustand, lucide-react를 플러그인용으로 노출하여 중복 번들 방지

### ⚡ 적응형 렌더링

고정 `requestAnimationFrame` 배치 처리를 대체하는 3단계 렌더 스케줄러:

| 단계 | 트리거 | 레이트 | 효과 |
|---|---|---|---|
| **Boost** | 프레임 데이터 ≥ 4 KB | 120 Hz+(ProMotion 네이티브) | `cat largefile.log`에서 스크롤 랙 제거 |
| **Normal** | 일반 타이핑 | 60 Hz(RAF) | 부드러운 기본 성능 |
| **Idle** | 3초간 I/O 없음 / 탭 숨김 | 1~15 Hz(지수 백오프) | GPU 부하 거의 제로, 배터리 절약 |

전환은 완전 자동 — 데이터 양, 사용자 입력, Page Visibility API에 의해 구동. 백그라운드 탭은 RAF를 깨우지 않고 유휴 타이머로 데이터를 계속 플러시합니다.

### 🔐 .oxide 암호화 내보내기

이식 가능하고 변조 방지되는 연결 백업:

- **ChaCha20-Poly1305 AEAD** 인증 암호화
- **Argon2id KDF**: 메모리 비용 256 MB, 4회 반복 — GPU 무차별 대입 저항
- **SHA-256** 무결성 체크섬
- **선택적 키 임베딩**: 개인 키를 Base64 인코딩하여 암호화 페이로드에 포함
- **사전 분석**: 인증 유형 분류, 내보내기 전 누락 키 감지

### 📡 ProxyJump — 토폴로지 인식 멀티 홉

- 무제한 체인 깊이: `Client → Jump A → Jump B → … → Target`
- `~/.ssh/config` 자동 파싱, 토폴로지 그래프 구축, Dijkstra 경로 탐색으로 최적 경로 결정
- 점프 노드를 독립 세션으로 재사용 가능
- 캐스케이드 장애 전파: 점프 호스트 다운 → 모든 다운스트림 노드 자동 `link_down` 설정

### ⚙️ 로컬 터미널 — 스레드 안전 PTY

`portable-pty 0.8`을 통한 크로스 플랫폼 로컬 셸, `local-terminal` 피처 게이트:

- `MasterPty`를 `std::sync::Mutex`로 래핑 — 전용 I/O 스레드로 블로킹 PTY 읽기를 Tokio 이벤트 루프에서 분리
- 셸 자동 감지: `zsh`, `bash`, `fish`, `pwsh`, Git Bash, WSL2
- `cargo build --no-default-features`로 PTY 제거, 모바일/경량 빌드 대응

### 🪟 Windows 최적화

- **네이티브 ConPTY**: Windows Pseudo Console API 직접 호출 — 완벽한 TrueColor 및 ANSI 지원, 레거시 WinPTY 불필요
- **셸 스캐너**: 레지스트리와 PATH에서 PowerShell 7, Git Bash, WSL2, CMD 자동 감지

### 기타 기능

- **리소스 프로파일러**: 지속적 SSH 채널로 `/proc/stat` 읽기, 델타 기반 계산으로 실시간 CPU/메모리/네트워크 모니터링, 비 Linux에서는 RTT 전용으로 자동 격하
- **커스텀 테마 엔진**: 30개 이상 내장 테마, 라이브 미리보기 비주얼 에디터, 20개 xterm.js 필드 + 24개 UI 색상 변수, 터미널 팔레트에서 UI 색상 자동 생성
- **세션 녹화**: asciicast v2 형식, 완전한 녹화 및 재생
- **브로드캐스트 입력**: 한 번 입력하면 모든 분할 창에 전송 — 일괄 서버 작업
- **배경 갤러리**: 탭별 배경 이미지, 16가지 탭 유형, 불투명도/블러/맞춤 제어
- **CLI 컴패니언**(`oxt`): 약 1 MB 바이너리, JSON-RPC 2.0 over Unix Socket / Named Pipe, `status`/`list`/`ping`을 사람 읽기 형식 또는 `--json` 출력
- **WSL Graphics** ⚠️ 실험적: 내장 VNC 뷰어 — 9가지 데스크톱 환경 + 단일 앱 모드, WSLg 감지, Xtigervnc + noVNC

<details>
<summary>📸 11개 언어 실제 동작</summary>
<br>
<table>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/en.png" width="280"><br><b>English</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/zhHans.png" width="280"><br><b>简体中文</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/zhHant.png" width="280"><br><b>繁體中文</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/ja.png" width="280"><br><b>日本語</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/ko.png" width="280"><br><b>한국어</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/fr.png" width="280"><br><b>Français</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/de.png" width="280"><br><b>Deutsch</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/es.png" width="280"><br><b>Español</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/it.png" width="280"><br><b>Italiano</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/pt-BR.png" width="280"><br><b>Português</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/vi.png" width="280"><br><b>Tiếng Việt</b></td>
    <td></td>
  </tr>
</table>
</details>

---

## 빠른 시작

### 사전 요구사항

- **Rust** 1.85 이상
- **Node.js** 18 이상(pnpm 권장)
- **플랫폼 도구**:
  - macOS: Xcode 커맨드 라인 도구
  - Windows: Visual Studio C++ 빌드 도구
  - Linux: `build-essential`, `libwebkit2gtk-4.1-dev`, `libssl-dev`

### 개발

```bash
git clone https://github.com/AnalyseDeCircuit/oxideterm.git
cd oxideterm && pnpm install

# CLI 컴패니언 빌드 (CLI 기능에 필요)
pnpm cli:build

# 전체 앱 (프론트엔드 + Rust 백엔드, 핫 리로드 포함)
pnpm run tauri dev

# 프론트엔드만 (Vite, 포트 1420)
pnpm dev

# 프로덕션 빌드
pnpm run tauri build
```

---

## 기술 스택

| 계층 | 기술 | 상세 |
|---|---|---|
| **프레임워크** | Tauri 2.0 | 네이티브 바이너리, 25~40 MB |
| **런타임** | Tokio + DashMap 6 | 완전 비동기, 무잠금 동시 맵 |
| **SSH** | russh 0.59(`ring`) | 순수 Rust, C 의존성 제로, SSH Agent |
| **로컬 PTY** | portable-pty 0.8 | 피처 게이트, Windows에서 ConPTY |
| **프론트엔드** | React 19.1 + TypeScript 5.8 | Vite 7, Tailwind CSS 4 |
| **상태 관리** | Zustand 5 | 19개 특수 스토어 |
| **터미널** | xterm.js 6 + WebGL | GPU 가속, 60fps 이상 |
| **에디터** | CodeMirror 6 | 30개 이상 언어 모드 |
| **암호화** | ChaCha20-Poly1305 + Argon2id | AEAD + 메모리 하드 KDF(256 MB) |
| **스토리지** | redb 2.1 | 임베디드 KV 스토어 |
| **i18n** | i18next 25 | 11개 언어 × 22개 네임스페이스 |
| **플러그인** | ESM 런타임 | 동결 PluginContext + 24 UI Kit |
| **CLI** | JSON-RPC 2.0 | Unix Socket / Named Pipe |

---

## 보안

| 항목 | 구현 |
|---|---|
| **비밀번호** | OS 키체인(macOS Keychain / Windows Credential Manager / libsecret) |
| **AI API 키** | OS 키체인 + macOS Touch ID 생체 인증 게이트 |
| **내보내기** | .oxide: ChaCha20-Poly1305 + Argon2id(메모리 256 MB, 4회 반복) |
| **메모리** | Rust 메모리 안전성 + 민감 데이터의 `zeroize` 클리어 |
| **호스트 키** | `~/.ssh/known_hosts` TOFU, 변경 감지 시 거부(MITM 방지) |
| **플러그인** | Object.freeze + Proxy ACL, 서킷 브레이커, IPC 화이트리스트 |
| **WebSocket** | 시간 제한 일회용 토큰 |

---

## 로드맵

- [x] SSH Agent 포워딩
- [ ] 플러그인 마켓플레이스
- [ ] 세션 검색 및 빠른 전환

---

## 라이선스

**GPL-3.0** — 이 소프트웨어는 [GNU 일반 공중 사용 허가서 v3.0](https://www.gnu.org/licenses/gpl-3.0.html) 하에 배포되는 자유 소프트웨어입니다.

GPL-3.0 조건에 따라 이 소프트웨어를 자유롭게 사용, 수정 및 배포할 수 있습니다. 파생 작품도 동일한 라이선스 하에 배포해야 합니다.

전문: [GNU 일반 공중 사용 허가서 v3.0](https://www.gnu.org/licenses/gpl-3.0.html)

---

## 감사의 말

[russh](https://github.com/warp-tech/russh) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [Tauri](https://tauri.app/) · [xterm.js](https://xtermjs.org/) · [CodeMirror](https://codemirror.net/) · [Radix UI](https://www.radix-ui.com/)

---

<p align="center">
  <sub>236,000줄 이상의 Rust & TypeScript — ⚡와 ☕로 구축</sub>
</p>

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
