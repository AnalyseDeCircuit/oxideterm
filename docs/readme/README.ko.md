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
  <strong>OxideTerm은 local-first SSH 워크스페이스이며, 단순한 터미널이 아닙니다.</strong>
  <br>
  <em>원격 노드(서버 연결)를 한 번 열고 그 주변에서 shell, SFTP, 포트 포워딩, trzsz, 경량 편집, BYOK AI를 사용하세요.</em>
  <br>
  <strong>Electron 제로. OpenSSL 제로. 텔레메트리 제로. 구독 제로. BYOK-first. 순수 Rust SSH.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.4.0--beta.3-blue" alt="버전">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="플랫폼">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="라이선스">
  <img src="https://img.shields.io/badge/rust-1.85+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
  <img src="https://img.shields.io/github/downloads/AnalyseDeCircuit/oxideterm/total?color=brightgreen" alt="총 다운로드 수">
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

<div align="center">

https://github.com/user-attachments/assets/4ba033aa-94b5-4ed4-980c-5c3f9f21db7e

*🤖 OxideSens AI — 하나의 어시스턴트에서 라이브 터미널과 워크스페이스 도구를 제어합니다.*

</div>

---

## 왜 OxideTerm인가?

| 문제점 | OxideTerm의 대답 |
|---|---|
| SSH 워크스페이스, 단순한 shell 이상 | **원격 노드 워크스페이스**: 터미널, SFTP, 포트 포워딩, trzsz, 경량 IDE, 모니터링, AI 컨텍스트를 같은 노드 아래에 묶습니다 |
| 로컬 shell도 필요함 | **통합 로컬 shell**: zsh/bash/fish/pwsh/WSL2를 SSH 세션과 나란히 사용하여 로컬 작업과 원격 작업을 같은 UI에서 처리합니다 |
| 재연결하면 모든 것을 잃음 | **Grace Period 재연결**: 연결 종료 전 30초간 기존 연결 프로브 — vim/htop/yazi가 그대로 살아남음 |
| 무거운 원격 IDE를 설치하고 싶지 않음 | **내장 경량 편집**: CodeMirror 6 over SFTP, 더 빠른 파일 작업이 필요할 때만 선택적 Linux 에이전트(~1 MB)를 사용합니다 |
| SSH 연결 재사용 불가 | **다중화**: 터미널, SFTP, 포워드, IDE가 참조 카운팅 풀로 하나의 SSH 연결 공유 |
| SSH 라이브러리가 OpenSSL에 의존 | **russh 0.59**: `ring`으로 컴파일된 순수 Rust SSH — C 의존성 제로 |
| Electron을 원하지 않음 | **Tauri 2.0**: 네이티브 Rust 백엔드, 25~40 MB 바이너리 |
| 텔레메트리나 앱 구독을 원하지 않음 | **추적 없음, 핵심 SSH 워크플로 앱 구독 없음**: SSH/SFTP/포트 포워딩/로컬 shell은 계정이나 구독이 필요 없습니다. 데이터는 기본적으로 기기에 남고, 클라우드 동기화는 [공식 플러그인](#공식-플러그인)으로 필요할 때만 켭니다 |
| 플랫폼 계정 없이 AI를 쓰고 싶음 | **BYOK-first OxideSens**: OpenAI/Ollama/DeepSeek/호환 API 키를 직접 사용합니다. OxideTerm은 AI 크레딧이나 클라우드 플랜을 판매하지 않습니다 |
| 자격 증명이 일반 텍스트 설정에 저장 | **저장 시 암호화**: 비밀번호와 API 키는 OS 키체인에 보관되고, 저장된 연결 메타데이터는 로컬에서 암호화되어 보관됨; `.oxide` 파일은 ChaCha20-Poly1305 + Argon2id 암호화 |

## 무엇이고 / 무엇이 아닌가

OxideTerm은 **local-first SSH 워크스페이스**입니다. 원격 노드를 한 번 열면 shell, 파일, 포트, 인밴드 전송, 경량 편집, AI 컨텍스트를 한곳에서 다룹니다.

OxideTerm은 클라우드 AI 플랫폼, 호스팅 Agent 서비스, 모든 원격 프로토콜을 위한 도구상자, 또는 터미널 렌더링 벤치마크를 핵심 목표로 삼는 프로젝트가 아닙니다. 많은 최신 터미널은 로컬 shell, AI 패널, 클라우드 Agent 플랫폼을 중심으로 진화하고 있지만, OxideTerm은 local-first SSH 워크스페이스에 집중합니다.

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

## 다운로드

[GitHub Releases](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)에서 최신 버전을 다운로드하세요.

---

## 기능 개요

| 카테고리 | 기능 |
|---|---|
| **터미널** | 로컬 PTY(zsh/bash/fish/pwsh/WSL2), SSH 원격, 분할 창, 브로드캐스트 입력, 세션 녹화/재생(asciicast v2), WebGL 렌더링, 30개 이상 테마 + 커스텀 에디터, 커맨드 팔레트(`⌘K`), Zen 모드, **trzsz** 인밴드 파일 전송 |
| **SSH 및 인증** | 연결 풀링 및 다중화, ProxyJump(무제한 홉) + 토폴로지 그래프, Grace Period 자동 재연결, Agent 포워딩. 인증: 비밀번호, SSH 키(RSA/Ed25519/ECDSA), SSH Agent, 인증서, keyboard-interactive 2FA, Known Hosts TOFU |
| **SFTP** | 이중 패널 브라우저, 드래그 앤 드롭, 스마트 미리보기(이미지/동영상/오디오/코드/PDF/Hex/폰트), 진행률 및 ETA가 포함된 전송 큐, 북마크, 아카이브 추출 |
| **IDE 모드** | CodeMirror 6, 30개 이상 언어, 파일 트리 + Git 상태, 멀티 탭, 충돌 해결, 통합 터미널. Linux용 선택적 원격 에이전트(9종 추가 아키텍처) |
| **포트 포워딩** | Local (-L), Remote (-R), Dynamic SOCKS5 (-D), 무잠금 메시지 패싱 I/O, 재연결 시 자동 복원, 종료 보고, 유휴 타임아웃 |
| **AI (OxideSens)** | 저장된 연결, 라이브 SSH 세션, 터미널 버퍼, SFTP 경로, 설정, 지식 베이스 항목을 워크스페이스 대상으로 보는 target-first 어시스턴트; OxideTerm 계정 없이 원격 출력 진단, 승인된 명령 실행, 파일 검사, 오류 설명을 수행할 수 있습니다 |
| **플러그인** | 런타임 ESM 로딩, 18개 API 네임스페이스, 24개 UI Kit 컴포넌트, 동결 API + Proxy ACL, 서킷 브레이커, 오류 시 자동 비활성화 |
| **CLI** | `oxt` 컴패니언: JSON-RPC 2.0 over Unix Socket / Named Pipe, status/health/list/forward/config/connect/focus/attach/SFTP/import/AI, 사람 읽기 & JSON 출력 |
| **보안** | .oxide 암호화 내보내기(ChaCha20-Poly1305 + Argon2id 256 MB), 로컬 설정 저장 시 암호화, OS 키체인, Touch ID(macOS), 휴대용 암호화 키스토어, 호스트 키 TOFU, `zeroize` 메모리 클리어 |
| **i18n** | 11개 언어: EN, 简体中文, 繁體中文, 日本語, 한국어, FR, DE, ES, IT, PT-BR, VI |

---

## 기술 상세

### 아키텍처 — 이중 평면 통신

OxideTerm은 터미널 데이터와 제어 명령을 두 개의 독립적인 평면으로 분리합니다:

```
┌─────────────────────────────────────┐
│        Frontend (React 19)          │
│  xterm.js 6 (WebGL) + 19 stores     │
└──────────┬──────────────┬───────────┘
           │ Tauri IPC    │ WebSocket (binary)
           │ (JSON)       │ per-session port
┌──────────▼──────────────▼───────────┐
│         Backend (Rust)              │
│  NodeRouter → SshConnectionRegistry │
│  Wire Protocol v1                   │
│  [Type:1][Length:4][Payload:n]      │
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
- **Target-first 워크스페이스 컨텍스트**: 저장된 연결, 라이브 SSH 세션, 터미널 버퍼, SFTP 경로, 설정, 지식 베이스 항목을 워크스페이스 대상으로 봅니다
- **승인된 작업**: OxideTerm 계정 없이 원격 출력 진단, 승인된 명령 실행, 파일 검사, 오류 설명을 수행할 수 있습니다
- **MCP 지원**: 외부 [Model Context Protocol](https://modelcontextprotocol.io) 서버(stdio & SSE) 연결로 서드파티 도구 통합
- **RAG 지식 베이스**(v0.20): Markdown/TXT 문서를 범위별 컬렉션(글로벌 또는 연결별)으로 가져오기. Reciprocal Rank Fusion으로 BM25 키워드 인덱스 + 벡터 코사인 유사도의 하이브리드 검색 융합. Markdown 인식 청킹으로 제목 계층 보존. CJK 바이그램 토크나이저로 중국어/일본어/한국어 지원.
- **프로바이더**: OpenAI, Ollama, DeepSeek, OneAPI, 또는 임의의 `/v1/chat/completions` 엔드포인트
- **보안**: API 키는 OS 키체인에 저장, macOS에서는 키 읽기 시 `LAContext` 기반 **Touch ID** 인증 게이트 — 엔타이틀먼트나 코드 서명 불필요, 세션당 첫 인증 후 캐시

###  포트 포워딩 — 무잠금 I/O

완전한 Local (-L), Remote (-R), Dynamic SOCKS5 (-D) 포워딩:

- **메시지 패싱 아키텍처**: SSH Channel은 단일 `ssh_io` 태스크가 소유 — `Arc<Mutex<Channel>>` 없음, 뮤텍스 경합 완전 제거
- **종료 보고**: 포워드 태스크가 종료 사유(SSH 연결 끊김, 원격 포트 닫힘, 타임아웃)를 능동적으로 보고하여 명확한 진단 제공
- **자동 복원**: `Suspended` 상태의 포워드가 재연결 시 사용자 개입 없이 자동 재개
- **유휴 타임아웃**: `FORWARD_IDLE_TIMEOUT`(300초)으로 좀비 연결 누적 방지

### 📦 trzsz — 인밴드 파일 전송

SFTP 연결 없이 SSH 터미널 세션을 통해 직접 파일을 업로드·다운로드:

- **인밴드 프로토콜**: 파일은 기존 터미널 스트림 안에서 Base64 인코딩 프레임으로 전송——추가 포트나 에이전트 없이 ProxyJump 체인과 tmux를 투명하게 통과
- **양방향 전송**: 서버에서 `tsz <file>` 실행으로 클라이언트에 파일 전송; `trz`로 클라이언트 업로드 시작; 드래그 앤 드롭 지원
- **디렉터리 지원**: `trz -d` / `tsz -d`를 통한 재귀적 디렉터리 전송
- **전송 제한**: 세션별 청크 크기, 파일 수, 총 바이트 수 상한 설정 가능
- **네이티브 Tauri I/O**: 파일 읽기·쓰기에 Tauri 네이티브 파일 다이얼로그와 Rust I/O 사용——브라우저 메모리 제약 없음
- **실시간 알림**: 전송 시작·완료·취소·오류 Toast 알림——trzsz가 감지되었지만 기능이 비활성화된 경우에도 힌트 표시
- **설정 → 터미널 → 인밴드 전송**에서 활성화

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

- **IDE 모드**: SFTP 기반 CodeMirror 6, 24개 언어, Git 상태 파일 트리, 멀티탭, 충돌 해결 — Linux에서 기능 강화를 위한 선택적 원격 에이전트(~1 MB) 지원
- **리소스 프로파일러**: 지속적 SSH 채널로 `/proc/stat` 읽기, 델타 기반 계산으로 실시간 CPU/메모리/네트워크 모니터링, 비 Linux에서는 RTT 전용으로 자동 격하
- **커스텀 테마 엔진**: 30개 이상 내장 테마, 라이브 미리보기 비주얼 에디터, 20개 xterm.js 필드 + 24개 UI 색상 변수, 터미널 팔레트에서 UI 색상 자동 생성
- **세션 녹화**: asciicast v2 형식, 완전한 녹화 및 재생
- **브로드캐스트 입력**: 한 번 입력하면 모든 분할 창에 전송 — 일괄 서버 작업
- **배경 갤러리**: 탭별 배경 이미지, 16가지 탭 유형, 불투명도/블러/맞춤 제어
- **CLI 컴패니언**(`oxt`): 약 1 MB 바이너리, JSON-RPC 2.0 over Unix Socket / Named Pipe, status/health/list/forward/config/connect/focus/attach/SFTP/import/AI를 사람 읽기 형식 또는 `--json` 출력
- **WSL Graphics** ⚠️ 실험적: 내장 VNC 뷰어 — 9가지 데스크톱 환경 + 단일 앱 모드, WSLg 감지, Xtigervnc + noVNC

#### 공식 플러그인

| 플러그인 | 설명 | 저장소 |
|---|---|---|
| **Cloud Sync** | 암호화된 셀프 호스팅 동기화 — WebDAV, HTTP JSON, Dropbox, Git, S3를 통한 `.oxide` 스냅샷 업로드 및 가져오기 | [oxideterm.cloud-sync](https://github.com/AnalyseDeCircuit/oxideterm.cloud-sync) |
| **Telnet Client** | 라우터, 스위치, 레거시 장치를 위한 네이티브 Telnet 클라이언트 — 외부 바이너리 불필요 | [oxideterm.telnet](https://github.com/AnalyseDeCircuit/oxideterm.telnet) |

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

## 런타임 요구 사항

OxideTerm은 운영체제가 제공하는 네이티브 WebView 런타임을 사용합니다. 대부분의 사용자는 이미 설치되어 있으며, 앱이 실행되지 않거나 에어갭 환경인 경우에만 수동 설치가 필요합니다.

| 플랫폼 | 런타임 의존성 |
|---|---|
| **Windows** | [WebView2 런타임](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) — Windows 10(1803+) 및 Windows 11에 기본 설치되어 있습니다. **에어갭 / 인트라넷** 환경에서는 [Evergreen 독립 실행형 설치 프로그램](https://developer.microsoft.com/en-us/microsoft-edge/webview2/#download)(오프라인, ~170 MB)을 사용하거나 그룹 정책을 통해 **고정 버전** 런타임을 배포하세요. |
| **macOS** | 없음 (네이티브 WebKit 사용) |
| **Linux** | `libwebkit2gtk-4.1` (대부분의 모던 데스크톱에 기본 설치됨) |

---

## 휴대용 모드

OxideTerm은 완전히 자체 포함된 휴대용 모드를 지원합니다 — 모든 데이터(연결, 비밀, 설정)가 애플리케이션 바이너리 옆에 저장되어 USB 드라이브나 에어갭 환경에 적합합니다.

### 활성화 방법

**방법 A — 마커 파일**(가장 간단): 앱 옆에 `portable`라는 이름의 빈 파일(확장자 없음)을 생성합니다.

| 플랫폼 | `portable` 파일 배치 위치 |
|---|---|
| **macOS** | `OxideTerm.app` 옆(동일 디렉토리) |
| **Windows** | `OxideTerm.exe` 옆 |
| **Linux (AppImage)** | `.AppImage` 파일 옆 |

```
/my-usb/
├── OxideTerm.app   (or .exe / .AppImage)
├── portable        ← 생성하는 빈 파일
└── data/           ← 최초 실행 시 자동 생성
```

**방법 B — `portable.json`**(사용자 정의 데이터 디렉토리): 동일한 위치에 `portable.json`을 배치합니다:

```json
{
  "enabled": true,
  "dataDir": "my-data"
}
```

- `enabled`는 생략 시 기본값 `true`
- `dataDir`는 **상대 경로**여야 합니다(`..` 사용 불가); 생략 시 기본값은 `data`

### 작동 원리

1. **최초 실행** — 부트스트랩 화면에서 휴대용 비밀번호를 생성하라는 메시지가 표시됩니다. 이 비밀번호로 로컬 키스토어(ChaCha20-Poly1305 + Argon2id)를 암호화하여 모든 저장된 비밀을 보호합니다.
2. **이후 실행** — 비밀번호를 입력하여 잠금 해제합니다. Touch ID가 있는 macOS에서는 **Settings → General → Portable Runtime**에서 생체 인증 잠금 해제를 선택적으로 활성화할 수 있습니다.
3. **인스턴스 잠금** — 한 번에 하나의 OxideTerm 인스턴스만 휴대용 데이터 디렉토리를 사용할 수 있습니다(`data/.portable.lock`).
4. **관리** — **Settings → General → Portable Runtime**에서 휴대용 비밀번호를 변경하거나 생체 인증 잠금 해제를 전환할 수 있습니다.
5. **휴대성** — 전체 폴더(앱 + `portable` 마커 + `data/`)를 다른 머신에 복사하면 바로 사용할 수 있습니다. 비밀번호는 키스토어와 함께 이동합니다.

> [!TIP]
> 휴대용 모드에서는 자동 업데이트가 비활성화됩니다. 업데이트하려면 `data/` 디렉토리를 유지한 채 애플리케이션 바이너리를 교체하세요.

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

## 프로젝트 규모

의존성과 빌드 산출물을 제외하고 `tokei`로 측정했습니다.

| 지표 | 현재 규모 |
|---|---:|
| 총 코드 | 286K+ |
| TypeScript / TSX | 130K+ |
| Rust | 100K+ |
| 프런트엔드 테스트 코드 | 24K+ |
| 프런트엔드 테스트 파일 | 128 |
| 소스 파일(`src` + `src-tauri/src`) | 664 |

---

## 보안

| 항목 | 구현 |
|---|---|
| **비밀번호** | OS 키체인(macOS Keychain / Windows Credential Manager / libsecret) |
| **휴대용 키스토어** | ChaCha20-Poly1305 암호화 볼트를 앱 옆에 배치, OS 키체인을 통한 생체 인증 바인딩 선택 가능 |
| **AI API 키** | OS 키체인 + macOS Touch ID 생체 인증 게이트 |
| **내보내기** | .oxide: ChaCha20-Poly1305 + Argon2id(메모리 256 MB, 4회 반복) |
| **메모리** | Rust 메모리 안전성 + 민감 데이터의 `zeroize` 클리어 |
| **호스트 키** | `~/.ssh/known_hosts` TOFU, 변경 감지 시 거부(MITM 방지) |
| **플러그인** | Object.freeze + Proxy ACL, 서킷 브레이커, IPC 화이트리스트 |
| **WebSocket** | 시간 제한 일회용 토큰 |

---

## 로드맵

- [x] SSH Agent 포워딩
- [ ] 완전한 ProxyCommand 지원
- [ ] 감사 로깅
- [ ] Agent 기능 강화
- [ ] 세션 검색 및 빠른 전환

---

## 지원 및 유지보수

OxideTerm은 개인 개발자가 **최선을 다해** 유지보수하고 있습니다. 버그 보고와 재현 가능한 회귀 문제를 우선 처리하며, 기능 요청은 환영하지만 항상 구현되지는 않을 수 있습니다.

OxideTerm이 워크플로에 도움이 되었다면 GitHub 스타, 문제 재현, 번역 수정, 플러그인, Pull Request 모두 프로젝트를 계속 나아가게 하는 데 도움이 됩니다.

---

## 라이선스

**GPL-3.0** — 이 소프트웨어는 [GNU 일반 공중 사용 허가서 v3.0](https://www.gnu.org/licenses/gpl-3.0.html) 하에 배포되는 자유 소프트웨어입니다.

GPL-3.0 조건에 따라 이 소프트웨어를 자유롭게 사용, 수정 및 배포할 수 있습니다. 파생 작품도 동일한 라이선스 하에 배포해야 합니다.

OxideTerm은 v1.0.0부터 **PolyForm Noncommercial 1.0.0**에서 **GPL-3.0**으로 전환했습니다. 이는 의도적인 선택입니다. 비상업 함정이나 경쟁 금지 조항을 단 "오픈소스 코스프레"가 아니라, 사용자, 포크, 재배포자, 상업 운영자에게 명확한 copyleft 자유를 주기 위해서입니다.

코드가 공개되어 있다고 해서 자동으로 오픈소스인 것은 아닙니다. 익숙한 오픈소스 라이선스를 내세우면서도 "재배포 금지", "재패키징 금지", "경쟁 제품 금지", "승인되지 않은 배포 플랫폼 금지" 같은 추가 조항을 붙인다면, 이는 사용자가 기대하는 오픈소스의 자유가 아니라 source-available 마케팅에 가깝습니다. OxideTerm은 비경쟁 또는 반재배포 조항을 추가하지 않습니다. GPL-3.0 조건이 전부입니다.

전문: [GNU 일반 공중 사용 허가서 v3.0](https://www.gnu.org/licenses/gpl-3.0.html)

---

## 감사의 말

[russh](https://github.com/warp-tech/russh) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [Tauri](https://tauri.app/) · [xterm.js](https://xtermjs.org/) · [CodeMirror](https://codemirror.net/) · [Radix UI](https://www.radix-ui.com/)

---

<p align="center">
  <sub>286,000줄 이상의 코드 — ⚡와 ☕로 구축</sub>
</p>

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
