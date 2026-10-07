<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm: 하나의 작업 공간에서 서버 관리" width="920">

# ⚡ OxideTerm

**자신의 API 키로 사용하는 AI 도우미를 갖춘 무료 네이티브 SSH 클라이언트이자 원격 운영 작업 공간입니다.**

SSH · Mosh · Telnet · 시리얼 · RDP/VNC · SFTP · 포트 포워딩 · 내장 편집기를 하나의 GPU 렌더링 앱에서 사용하세요.
계정도, 구독도 필요 없습니다. 텔레메트리와 Electron을 사용하지 않습니다.

[![최신 릴리스](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![지원 플랫폼](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![라이선스](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![스타](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**다운로드**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**문서**](https://oxideterm.app) ·
[**변경 이력**](../../.github/release-notes/stable-changelog.md) ·
[**문제 보고**](https://github.com/AnalyseDeCircuit/oxideterm/issues)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md) | [Русский](README.ru.md)

</div>

---

## 빠른 시작

1. **OxideTerm을 설치하세요.** [최신 릴리스](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)에서 패키지를 다운로드하세요. 운영체제별 안내는 아래 [설치](#install)에서 확인할 수 있습니다.
2. **서버를 추가하세요.** 세션 관리자에서 SSH 연결을 만들거나 `~/.ssh/config`에서 호스트를 가져오세요.
3. **연결하세요.** 터미널을 여세요. 호스트 키는 `~/.ssh/known_hosts`와 대조하여 확인합니다.
4. **다른 기능도 활용하세요.** 같은 노드에서 SFTP, 포트 포워딩, 내장 편집기를 여세요. 기본적으로 하나의 SSH 연결을 공유합니다.
5. **필요하면 AI를 켜세요.** 설정에서 자신의 OpenAI, Anthropic, Gemini, Ollama 또는 OpenAI 호환 엔드포인트를 추가하면 OxideSens를 사용할 수 있습니다.

자세한 사용 안내는 [문서](https://oxideterm.app)를 참고하세요.

---

## 제공 기능

| | |
|---|---|
| **터미널과 프로토콜** | 로컬 셸, SSH, Mosh, Telnet, 시리얼, 분할 창, 다중 홉 경로, SSH 에이전트와 에이전트 포워딩, 2FA와 TOTP 인증 정보, X11 포워딩, 셸 통합, 명령 표시, 세션 로그 설정, 녹화, Sixel 및 Kitty 그래픽, trzsz 전송 |
| **tmux와 동시 입력** | 창 배치와 드래그 가능한 구분선을 지원하는 네이티브 `tmux -CC` 제어 모드, 이름을 지정하는 동시 입력 그룹, 예약 및 반복 입력을 위한 고급 다중 대상 명령 전송 |
| **연결 안정성** | Grace Period 재연결로 짧은 네트워크 중단에도 TUI 앱을 유지하고, 이후 포트 포워딩, 파일 전송, 열려 있던 편집 파일을 복원 |
| **파일과 편집** | SFTP 양쪽 창 파일 관리자, 속도 제한과 예상 완료 시간을 지원하는 전송 대기열, 북마크, 안전한 저장·충돌 처리·작업 공간 복원을 지원하는 내장 원격 편집기 |
| **네트워크** | 로컬·원격·동적 SOCKS5 포트 포워딩, 규칙 저장, 원격 포트 감지, 연결 토폴로지, 필요할 때 바로 수행하는 소켓 디버깅 |
| **원격 데스크톱** | 클립보드와 입력을 지원하는 내장 RDP 및 VNC |
| **호스트 운영** | 프로세스, 서비스, 로그, 포트, 작업, 디스크, 패키지, 컨테이너, tmux 모니터링 |
| **AI와 자동화** | 자신의 API 키로 사용하는 OxideSens, MCP, 로컬 RAG, Agent Skills, 승인된 작업 공간 작업, 독립 실행형 CLI |
| **확인과 감사** | 선택적으로 사용하는 알림 및 감사 작업 공간과 암호화된 세션 녹화(둘 다 기본적으로 꺼져 있음) |
| **동기화와 이동** | 암호화된 클라우드 동기화, 이동 가능한 `.oxide` 번들 |
| **개인 설정** | 테마, 배경 이미지, 단축키 설정, 빠른 명령, 11개 언어 인터페이스 |

---

## OxideTerm을 선택하는 이유

- **무료로, 로컬 데이터를 중심으로.** 계정, 구독, 텔레메트리가 없습니다. 연결 정보와 운영 데이터는 사용자가 관리합니다.
- **서버마다 하나의 작업 공간.** 터미널, SFTP, 포트 포워딩, RDP/VNC, 편집기, 모니터링, AI가 같은 노드에 연결되어 함께 작동합니다.
- **브라우저에 의존하지 않는 네이티브 앱.** [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)가 GPU에서 인터페이스를 직접 그립니다. Electron이나 번들 WebView를 사용하지 않습니다.
- **AI 사용 방식은 사용자가 결정.** OxideSens는 사용자의 공급자와 키를 사용하며 승인한 작업만 실행합니다.
- **끊김에 강한 연결.** Grace Period 재연결은 기존 연결을 30초 동안 확인한 후 교체하므로 짧은 네트워크 중단에도 TUI 앱을 유지할 수 있습니다.
- **순수 Rust SSH.** SSH 스택은 `russh`와 `ring`을 사용하며 OpenSSL이나 libssh2에 의존하지 않습니다.

---

## 메모리 사용량

**네이티브 재작성으로 유휴 상태 메모리 사용량이 macOS에서는 이전 버전의 약 4분의 1, Windows에서는 약 8분의 1로 줄었습니다.** 아래는 Tauri 1.x에서 네이티브 GPUI 2.0으로 전환할 때 관리자가 기록한 관찰 결과입니다.

| 플랫폼 | Tauri 1.x(유휴 상태) | 네이티브 2.0(유휴 상태) | 감소율 |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 약 74% |
| Windows | 182.4 MB | 23.5 MB | 약 87% |

이전 버전의 합계에는 OxideTerm과 관련 WebView 프로세스가 포함됩니다. 네이티브 버전에서는 이러한 브라우저 프로세스가 필요하지 않습니다.

![시스템 프로세스 화면으로 비교한 유휴 상태 메모리: Tauri 1.x와 네이티브 2.0](../../docs/screenshots/oxideterm-memory-comparison.png)

---

## 스크린샷

| OxideSens를 갖춘 SSH 터미널 | SFTP 파일 관리자 |
|---|---|
| ![OxideSens AI를 갖춘 SSH 터미널](../../docs/screenshots/terminal/SSHTERMINAL.png) | ![전송 대기열을 갖춘 SFTP 양쪽 창 파일 관리자](../../docs/screenshots/sftp/sftp.png) |

| 내장 IDE | 스마트 포트 포워딩 |
|---|---|
| ![내장 IDE 모드](../../docs/screenshots/miniIDE/miniide.png) | ![자동 감지를 지원하는 스마트 포트 포워딩](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png) |

<details>
<summary><b>자연어 요청으로 OxideSens가 터미널을 여는 모습 보기</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens가 OxideTerm 안에서 터미널을 여는 모습" width="720">
</a>

</details>

---

## OxideSens AI

OxideSens는 선택적으로 사용하는 도우미입니다. 실행 중인 세션을 확인하고 **사용자가 승인한 후에만** 작업 공간 작업을 수행할 수 있습니다.

- **자신의 키 사용.** OpenAI, Anthropic(Claude), Google Gemini, Ollama 및 모든 OpenAI 호환 엔드포인트를 지원하며 공급자별 추론 설정을 제공합니다. 플랫폼에서 제공하는 사용 크레딧은 없습니다.
- **MCP와 Agent Skills.** MCP 서버(stdio 및 SSE)를 연결하고 범위가 제한된 Agent Skills를 불러올 수 있습니다.
- **로컬 지식 기반(RAG).** BM25 전문 검색과 벡터 인덱스를 함께 사용합니다.
- **컨텍스트는 사용자가 관리.** 승인할 작업 공간 정보와 작업을 사용자가 선택하며 명령 정책 규칙도 적용됩니다.
- **인증 정보 마스킹.** 공급자에게 보내는 메시지에는 인증 정보 패턴에 따른 마스킹을 적용합니다.
- **키는 OS 키체인에 보관**하며 구조화된 로그에는 기록하지 않습니다.

---

<a id="plugins"></a>

## 플러그인

OxideTerm은 다음 세 가지 플러그인 방식을 지원합니다.

| 유형 | 실행 방식 | 실행 범위와 제약 |
|---|---|---|
| **매니페스트 전용** | 코드를 사용하지 않는 선언형 확장 | 실행 가능한 코드 없음 |
| **WASM** | Wasmtime/WASI 또는 사이드카 | 호스트 호출을 제어하고 허용된 기능 범위로 제한 |
| **프로세스** | 일반 로컬 프로세스 | 신뢰하는 로컬 코드로 실행되며 OS 샌드박스로 **격리되지 않음** |

이전 Tauri(1.x)용 ESM 플러그인이 목록에 표시될 수는 있지만 네이티브 2.x 앱에서는 실행되지 않습니다. 프로세스 플러그인은 신뢰하는 출처에서만 설치하세요.

---

## 보안과 개인정보 보호

| 항목 | 동작 방식 |
|---|---|
| **인증 정보 저장** | OS 키체인(macOS Keychain, Windows Credential Manager, libsecret) |
| **메모리의 비밀 정보** | 비밀 정보를 담은 타입과 임시 버퍼는 지원되는 소유권 경계에서 `zeroize`로 지움 |
| **호스트 키** | `~/.ssh/known_hosts`를 사용하는 최초 사용 시 신뢰 방식. 예기치 않은 변경은 거부 |
| **이동용 내보내기** | `.oxide` 번들은 ChaCha20-Poly1305와 Argon2id(메모리 256 MB, 반복 4회)를 사용 |
| **AI 컨텍스트** | 공급자에게 보내기 전에 인증 정보 패턴을 마스킹. 컨텍스트와 작업은 사용자가 승인 |
| **세션 녹화** | 기본적으로 꺼져 있음. 기기에 암호화하여 저장하고 클라우드 동기화에서 제외. 키보드 입력은 기록하지 않음 |
| **감사** | 기본적으로 꺼져 있음. 데이터는 기기에 저장하며 민감한 정보는 암호화 |
| **CLI 변경** | 상태를 변경하는 명령에 실행 전 계획 확인, `--yes` 확인 절차, 복원용 백업 제공 |
| **플러그인** | [플러그인](#plugins) 참고 |
| **텔레메트리** | 없음 |

**적법한 사용.** OxideTerm은 추가 제한 없이 GPL-3.0-only 라이선스로 제공됩니다. 자신이 소유하거나 명시적으로 접근 권한을 받은 시스템, 네트워크, 기기에만 접근하고 관련 법률을 준수하세요. 무단 접근, 서비스 방해, 접근 제어 우회에 OxideTerm을 사용하지 마세요.

---

## 현재의 제한 사항

설치하기 전에 다음 사항을 확인하세요.

- 데스크톱 전용입니다(macOS, Windows, Linux). 모바일 앱은 없습니다.
- 개발이 빠르게 진행되며 릴리스가 자주 이루어집니다. [변경 이력](../../.github/release-notes/stable-changelog.md)과 [미해결 이슈](https://github.com/AnalyseDeCircuit/oxideterm/issues)를 참고하세요.
- 감사와 세션 녹화는 사용자가 선택하여 켜는 기능이며 OxideTerm이 직접 관찰할 수 있는 내용만 기록합니다.
- 프로세스 플러그인은 OS 샌드박스로 격리되지 않습니다.
- 기기에서 렌더링이 제대로 동작하지 않으면 호환성 프로필을 사용해 보세요: `OXIDETERM_RENDER_PROFILE=compatibility`.

---

<a id="for-developers"></a>

## 개발자 안내

<details>
<summary><b>소스에서 실행</b></summary>

**필요 환경:** Rust 도구 체인(edition 2024)과 GPUI를 실행할 수 있는 데스크톱 환경.

```bash
# Run the app
cargo run

# If the renderer fails on your machine
OXIDETERM_RENDER_PROFILE=compatibility cargo run

# Build the headless CLI companion
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

Nix 사용 시: `nix build .#oxideterm`, `nix run .#oxideterm` 또는 `nix develop`.

CLI 빌드 결과물은 `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`에 생성됩니다.

</details>

<details>
<summary><b>명령줄 인터페이스</b></summary>

화면 없이 동작하는 `oxideterm` CLI는 앱을 실행하지 않고 사용할 수 있어 자동화, CI, 진단에 유용합니다. 설정, 연결, 포트 포워딩, 플러그인, 빠른 명령, 비밀 정보, 이동용 번들, 진단, 보고서, 일괄 실행 계획, 백업, 클라우드 동기화를 지원합니다.

```bash
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- forwards list --format json
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- oxide export ./profile.oxide --connection prod --password-stdin
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
cargo run -p oxideterm-cli -- completion install zsh --force

# Path and profile isolation for CI or fixtures
cargo run -p oxideterm-cli -- --config-dir ./fixture-config doctor --strict
```

</details>

<details>
<summary><b>아키텍처</b></summary>

UI와 터미널·SSH 백엔드는 하나의 Rust 프로세스를 공유하며 선택적인 원격 에이전트와 플랫폼 도우미는 그 밖에서 동작합니다. 터미널 바이트는 `TerminalState`를 직접 변경하고 GPUI는 이 상태를 렌더링합니다. JSON, WebSocket, Base64, xterm.js 파싱 단계를 거치지 않습니다.

```
┌─────────────────────────────────────────────────┐
│               GPUI Render Loop                  │
│   WorkspaceApp  ·  Tab surfaces  ·  GPUI views  │
└──────────────────────┬──────────────────────────┘
                       │  in-process Arc<> / async
┌──────────────────────▼──────────────────────────┐
│             Domain Crates (Rust async)          │
│  NodeRouter → SshConnectionRegistry             │
│  TerminalState ← SSH PTY channel (russh)        │
│  SftpSession · ForwardingRuntime · IdeWorkspace │
│  Ai/ACP Entities · CloudSync · Plugin Runtimes  │
└─────────────────────────────────────────────────┘
```

| 항목 | 브라우저를 포함하는 방식 | OxideTerm |
|---|---|---|
| 렌더링 | 브라우저 엔진과 웹 레이아웃 | GPU 화면에서 GPUI 렌더링 |
| 터미널 데이터 흐름 | WebSocket → JS 이벤트 루프 → xterm.js | Rust 입력 → `TerminalState` → GPUI 렌더링 |
| 연결 수명 주기 | 프런트엔드와 백엔드에 분산 | 프로세스 내부의 단일 연결 및 재연결 흐름 |
| AI 컨텍스트 | 앱의 연결 계층을 거쳐 복사 | 사용자 승인에 따라 활성 작업 공간에서 구성 |
| CLI | 데스크톱 앱 실행 필요 | 독립 실행 파일로 크레이트를 직접 연결 |

**연결 풀.** `SshConnectionRegistry`는 `DashMap`을 기반으로 하며 `NodeRouter`를 통해 사용합니다. 터미널 창, SFTP, 포트 포워딩, 편집기는 노드마다 하나의 물리 SSH 연결을 공유할 수 있고, 터미널 정책에서 전용 연결을 선택할 수도 있습니다. 각 연결은 `connecting → active → idle → link_down → reconnecting` 상태를 거칩니다. 점프 호스트에 장애가 발생하면 하위 노드는 `link_down` 상태가 됩니다. AI와 플러그인은 연결 소비자로 등록하는 대신 허용된 기능의 핸들과 호스트 스냅샷을 사용합니다.

**Grace Period 재연결.**

1. 연결 유지 확인의 시간 초과를 감지합니다.
2. 터미널 창, SFTP 전송, 포트 포워딩, 편집 파일의 스냅샷을 저장합니다.
3. 기존 연결을 30초 동안 확인하여 짧은 네트워크 중단에도 TUI 앱을 유지합니다.
4. 새 연결을 열고 포트 포워딩을 복원하며 전송을 재개하고 편집 파일을 다시 엽니다.

SFTP 세션에는 연결 세대 정보가 포함됩니다. 재연결 후에는 조건을 충족하는 세션을 다시 확보하지만 이전 세대의 작업을 새 연결로 조용히 옮기지는 않습니다.

**포트 포워딩.** `-L`, `-R`, `-D`(SOCKS5)를 지원하는 독립 크레이트입니다. 각 SSH 채널은 하나의 `ssh_io` 작업이 소유하므로 자주 실행되는 처리 경로에 공유 뮤텍스가 없습니다.

**순수 Rust SSH.** `russh`와 `ring`으로 전체 SSH2, ChaCha20-Poly1305와 AES-GCM, Ed25519/RSA/ECDSA 키, Unix(`SSH_AUTH_SOCK`)와 Windows(`\\.\pipe\openssh-ssh-agent`)의 SSH 에이전트, 홉마다 독립적으로 인증하는 다중 홉 연결을 지원합니다.

**기술 구성**

| 계층 | 기술 |
|---|---|
| UI | GPUI(Zed의 GPU 기반 UI 프레임워크) |
| 런타임 | Tokio, DashMap |
| SSH | `russh`와 `ring`(OpenSSL과 libssh2 미사용) |
| 로컬 PTY | `portable-pty`(Windows에서는 ConPTY) |
| 터미널 에뮬레이션 | `alacritty_terminal`(VT100–VT500, Sixel, Kitty 그래픽) |
| 편집기 | tree-sitter 구문 강조, 자체 버퍼 |
| 암호화 | ChaCha20-Poly1305, Argon2id |
| 플러그인 | Wasmtime/WASI, 사이드카 WASM, 프로세스 방식 |
| AI 스트리밍 | SSE(OpenAI, Anthropic, Gemini), 프로세스 내부 처리 |
| RAG | 순위 융합을 적용한 BM25 + HNSW 벡터 인덱스, CJK 바이그램 토크나이저 |
| 다국어 지원 | `oxideterm-i18n`(11개 언어) |

</details>

---

<a id="install"></a>

## 설치

[**최신 릴리스 다운로드**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

| OS | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG(Intel) | DMG(Apple Silicon) |
| **Windows** | 설치 프로그램(`.exe`) | 설치 프로그램(`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

릴리스 페이지의 `sha256sums.txt` 파일로 다운로드한 파일을 검증하세요. 이동용 압축 파일과 서명도 같은 페이지에 있습니다.

### macOS

Gatekeeper가 앱 실행을 차단하면 격리 속성을 제거하세요.

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

SmartScreen 경고가 표시되면 **추가 정보 → 실행**을 선택하세요.

### Linux

```bash
# AppImage
chmod +x OxideTerm_*_linux_*.AppImage && ./OxideTerm_*_linux_*.AppImage

# Debian / Ubuntu
sudo dpkg -i OxideTerm_*_linux_*.deb && sudo apt-get install -f

# Fedora / RHEL-compatible
sudo dnf install ./OxideTerm_*_linux_*.rpm

# Nix; updates are managed by Nix
nix run github:AnalyseDeCircuit/oxideterm
```

직접 빌드하려면 [개발자 안내](#for-developers)의 **소스에서 실행**을 참고하세요.

---

## 기여하기

Rust 코드, 문서, 번역, 플러그인, 테스트, 문제 재현 등 다양한 기여를 환영합니다. 큰 변경은 먼저 이슈를 열어 논의해 주세요.

버그를 보고할 때 민감한 정보를 마스킹한 진단 번들을 첨부하면 도움이 됩니다.

```bash
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

재현 가능한 버그와 회귀 문제를 우선적으로 처리합니다. 기능 요청은 범위, 안전성, OxideTerm의 원격 서버 작업 공간이라는 방향과의 적합성을 검토합니다. OxideTerm이 작업에 도움이 된다면 GitHub 스타, 재현 가능한 버그 보고, 번역 수정, 플러그인 제공으로 개발을 지원할 수 있습니다.

### 기여자

OxideTerm을 더 나은 앱으로 만드는 데 함께해 주시는 모든 분께 감사드립니다.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="OxideTerm 기여자">
  </a>
</p>

---

## 라이선스

**GPL-3.0-only.** 의존 라이브러리의 저작권 표기는 [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md)에, 추가 고지는 [`NOTICE`](../../NOTICE)에 있습니다.

**사용 기술:** [russh](https://github.com/warp-tech/russh) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [alacritty_terminal](https://github.com/alacritty/alacritty) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [wasmtime](https://wasmtime.dev/) · [tree-sitter](https://tree-sitter.github.io/)
