<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm: 하나의 작업 공간에서 서버 관리" width="920">

# ⚡ OxideTerm

**원격 서버 작업을 위한 무료 네이티브 SSH 클라이언트.**

SSH · Mosh · Telnet · 시리얼 · RDP/VNC · SFTP · 포트 포워딩 · 코드 편집기

계정도, 구독도 필요 없습니다. 텔레메트리와 Electron을 사용하지 않습니다.

[![최신 릴리스](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![지원 플랫폼](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![라이선스](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![스타](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**다운로드**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**문서**](https://oxideterm.app) ·
[**변경 이력**](../../.github/release-notes/stable-changelog.md) ·
[**문제 보고**](https://github.com/AnalyseDeCircuit/oxideterm/issues) ·
[**위챗 사용자 모임**](https://github.com/AnalyseDeCircuit/oxideterm/issues/752)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md)

</div>

---

## OxideTerm 소개

OxideTerm은 macOS, Windows, Linux용 SSH 클라이언트입니다. 서버마다 작업 공간을 두고 터미널, 파일 관리자, 포트 포워딩, 코드 편집기, 호스트 모니터링, 선택 사항인 AI 도우미를 한 창에서 사용합니다. GPL-3.0으로 공개된 무료 오픈 소스 소프트웨어입니다.

## OxideTerm을 선택하는 이유

- **한 번 연결하면 여러 도구를 함께 사용.** 같은 서버의 터미널, SFTP, 포워딩, 편집기는 기본적으로 SSH 연결 하나를 공유하므로 도구마다 다시 로그인할 필요가 없습니다.
- **짧은 네트워크 단절에 대응.** 응답이 없는 연결을 교체하기 전에 30초간 계속 확인하여 vim, htop 같은 전체 화면 프로그램이 이전 상태에서 이어갈 수 있게 합니다.
- **낮은 메모리 사용량.** Zed의 UI 프레임워크인 [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)로 GPU에 직접 그립니다. 유휴 시 macOS에서 약 81 MB, Windows에서 약 24 MB를 사용합니다.
- **원하는 AI 사용.** OxideSens는 직접 선택한 제공업체와 API 키를 사용합니다. 별도로 구매할 OxideTerm 크레딧은 없습니다.
- **순수 Rust SSH.** russh와 ring 기반으로, OpenSSL이나 libssh2에 의존하지 않습니다.

<details>
<summary><b>OxideTerm 1.x와 메모리 비교</b></summary>

Tauri(1.x)에서 네이티브 GPUI(2.0)로 전환하기 전후의 유휴 메모리입니다. 1.x 수치는 WebView 프로세스를 포함합니다.

| 플랫폼 | 1.x(Tauri) | 2.0(네이티브) | 감소율 |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 74% |
| Windows | 182.4 MB | 23.5 MB | 87% |

![시스템 프로세스 모니터에서 비교한 OxideTerm 1.x와 2.0의 유휴 메모리](../../docs/screenshots/oxideterm-memory-comparison.png)

</details>

---

## 스크린샷

### 터미널

SSH 세션 화면입니다. 사이드바에서 이 서버의 추가 터미널, SFTP, IDE, 포트 포워딩을 열 수 있습니다.

![사이드바에 서버 도구가 표시된 SSH 터미널](../../docs/screenshots/terminal/SSHTERMINAL.png)

### 혼합 작업 공간

SSH 터미널, 로컬 터미널, Markdown 노트와 SFTP를 하나의 탭에 배치할 수 있습니다. 로컬 컴퓨터와 여러 서버의 페이지를 최대 16개 창으로 자유롭게 조합할 수 있습니다.

![SSH, 로컬 터미널, 노트와 SFTP를 조합한 4분할 작업 공간](../../docs/screenshots/mixed-workspace.png)

### 파일

로컬 및 원격 폴더를 나란히 표시하고 아래에 전송 대기열을 둡니다.

![전송 대기열이 있는 2분할 SFTP 파일 관리자](../../docs/screenshots/sftp/sftp.png)

### IDE

원격 프로젝트를 열고 탭에서 파일을 편집합니다.

![내장 IDE에서 열린 원격 프로젝트](../../docs/screenshots/miniIDE/miniide.png)

### 포트 포워딩

서버에서 수신 중인 포트를 찾아 클릭 한 번으로 포워딩합니다. Jupyter 같은 도구에는 빠른 포워딩도 사용할 수 있습니다.

![서버에서 감지한 포트가 표시된 포워딩 화면](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png)

<details>
<summary><b>자연어 요청으로 OxideSens가 터미널을 여는 모습 보기</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens가 OxideTerm 안에서 터미널을 여는 모습" width="720">
</a>

</details>

---

## 기능

*(플러그인)* 표시가 있는 기능은 앱의 플러그인 마켓에서 설치합니다.

### 작업 공간

- 저장된 연결 검색, 정렬, 그룹화, 가져오기/내보내기와 중첩 그룹, 태그, 색상, 아이콘, 메모를 제공하는 세션 관리자
- `Match` 블록, `ProxyJump` 체인, `RemoteCommand`를 포함한 `~/.ssh/config` 가져오기 및 가져온 호스트 정보 갱신
- SecureCRT, Xshell, Termius, MobaXterm, WindTerm, Electerm, FinalShell에서 저장 전 미리보기를 거쳐 가져오기
- 전체 탭, 전역 페이지, 로컬 터미널 또는 서버별 필터; 숨겨진 세션도 계속 실행
- 탭당 최대 16개 창에 터미널, SFTP, IDE, 포워딩, 메모, 설정, 원격 데스크톱 혼합 배치
- 세션을 끊지 않고 탭을 별도 창으로 이동하거나 다시 합치기
- 열린 터미널 2–16개를 재연결 없이 격자로 배치
- 작업 및 저장 연결용 명령 팔레트와 한 페이지에 집중하는 Zen 모드
- 셸, 시작 디렉터리, 아이콘을 지정하는 로컬 터미널 프로필
- 시작 시 작업 공간, 창 크기, 전체 화면 상태 및 모니터 위치 복원

### 터미널

- Unix 셸, 명령 프롬프트, Windows PowerShell, PowerShell Core, Git Bash, Nushell 자동 감지
- 드래그 가능한 구분선으로 분할; 새 로컬 창은 작업 디렉터리 상속
- 정규식, 대소문자 구분, 여러 줄 검색과 창별 독립 검색 상태
- Bash, Zsh, Fish, Nushell, PowerShell 통합: 디렉터리, 종료 코드, 명령 표시 추적 및 앱에서 원격 설치
- 완료된 명령 출력을 접고 종료 상태 유지
- 여러 줄 입력, 기록, 경로 제안, 자동 완성 및 빠른 명령을 갖춘 명령 모음
- 유사 일치 기록 제안과 선택적인 연결별 영구 기록
- 마우스로 현재 명령을 편집하는 자유 입력 모드
- 여러 줄 붙여넣기를 보내기 전 편집, 실행 취소 및 코드 블록 구분자 제거
- 텍스트나 정규식 출력 트리거로 입력 전송, 빠른 명령 실행, 프로그램 시작, 결과를 클릭 가능한 작업으로 전환
- 오류, Git 상태, 권한, 주소, 날짜 등 색 없는 출력에 의미별 색상 적용; 키워드 및 정규식 강조
- 클릭 가능한 URL/파일 경로, 밀리초 단위 줄별 타임스탬프, 탭의 읽지 않은 출력 표시
- Sixel/Kitty 그래픽 및 Kitty 애니메이션
- 창 배치와 드래그 가능한 구분선을 갖춘 네이티브 `tmux -CC` 제어 모드
- 여러 터미널에 입력을 보내는 동기화 그룹과 속도, 간격, 반복을 지정하는 텍스트/16진 명령 전송기
- 빠른 명령의 그룹, 고정, 텍스트/선택/비밀 매개변수 및 호스트별 표시 규칙
- 파일명 템플릿, 크기/보관 기간 제한, 선택적 ANSI가 있는 세션 로그; `.cast` 녹화 및 내장 재생기
- 로컬 및 지원되는 SSH 세션에서 Vim, Neovim, Emacs 선택 영역 복사, 잘라내기, 붙여넣기
- 연결별 Backspace/Delete 시퀀스와 텍스트 인코딩

### 연결

- 비밀번호, 개인 키, OpenSSH 인증서, 관리 키, SSH 에이전트, 키보드 대화형 인증, Kerberos 로그인
- SSH 에이전트 감지 또는 사용자 지정 엔드포인트(macOS/Linux의 `SSH_AUTH_SOCK`, Windows의 `\\.\pipe\openssh-ssh-agent`) 및 에이전트 포워딩
- Ed25519/RSA/ECDSA 키, ChaCha20-Poly1305/AES-GCM, 알고리즘 순서 조정 및 구형 서버 호환 옵션
- 서버나 각 점프 호스트에서 일치하는 인증 요청에 TOTP 생성 및 자동 입력
- PIN 및 터치 요청을 사용하는 FIDO 보안 키(`ed25519-sk`, `ecdsa-sk`) *(플러그인)*
- 홉별 독립 인증을 사용하는 점프 호스트 체인, HTTP CONNECT/SOCKS5 프록시, ProxyCommand
- 유예 기간 재연결로 새 연결에서 포워딩 복원, 전송 재개, 편집기 파일 다시 열기
- 서버별 공유 연결 또는 새 터미널마다 개별 인증
- 신뢰/비신뢰 X11 포워딩
- 저장 없는 임시 연결 및 앱을 여는 `ssh://`, `telnet://`, `mosh://`, `rdp://`, `vnc://` 링크
- 적응형 로컬 에코 예측을 사용하는 Mosh *(플러그인)*
- 옵션 협상, 프록시 라우팅, 줄 끝 설정을 지원하는 Telnet
- 시리얼 전송 속도, 데이터 비트, 패리티, 정지 비트, 흐름 제어, 16진 입출력, Break/DTR/RTS
- Nordic UART Service 또는 사용자 지정 GATT 특성 기반 Bluetooth LE 터미널

### 파일 및 전송

- 즐겨찾기, 경로 완성, 목록/트리 보기가 있는 로컬/원격 2분할 파일 관리자
- 연결별로 짝지어 저장하는 로컬/원격 경로 북마크
- 병렬 전송, 속도 제한, 남은 시간, 일시 중지, 재개 및 재시도; 중단된 SFTP 전송은 재시작 후 일시 중지 상태로 복원
- 충돌 시 덮어쓰기, 건너뛰기, 이름 변경을 파일 하나 또는 전체에 적용
- SFTP 하위 시스템이 없는 호스트에서 SCP로 자동 전환
- 독립 SFTP 연결 및 앱을 통한 서버 간 중계 전송
- FTP 및 명시적 FTPS
- SSH 터미널에 파일을 끌어 놓아 현재 디렉터리에 업로드
- 원격 셸의 `trz`/`tsz`(trzsz), `rz`/`sz`(ZMODEM)와 시리얼의 XMODEM/YMODEM/ZMODEM
- 텍스트, 코드, Markdown, 이미지, 오디오, 비디오, 16진 데이터, 글꼴 미리보기
- PDF, SQLite, 인증서 및 바이너리 뷰어 *(플러그인)*
- 터미널에서 같은 SFTP 디렉터리로 이동하거나 SFTP 경로를 터미널로 보내기

### 편집기 및 IDE

- SFTP, 일반 셸 명령 또는 심볼 인덱스, 완성, 파일 감시를 추가하는 원격 에이전트 기반 IDE
- 유휴 터미널을 같은 사용자, 컨테이너, 디렉터리의 IDE로 변환
- 탭, 다중 선택 프로젝트 트리, 프로젝트 검색, 정규식/캡처 그룹 찾기 및 바꾸기
- 최대 100 MiB 파일, 인코딩/BOM/줄 끝 보존
- 충돌 처리와 탭 전환 시 선택적 자동 저장을 제공하는 안전한 저장
- 재시작 후 열린 파일과 저장하지 않은 변경 복원
- Git 브랜치, worktree, 스테이징, 기록, 앞선/뒤처진 커밋 수, 병합/리베이스 상태 보기
- 프로젝트 작업 감지 및 실행
- 내장 15개 언어와 추가 20개 이상 언어 플러그인의 구문 강조 *(플러그인)*
- 표, 수식, 각주가 있는 Markdown 미리보기

### 포트 포워딩 및 네트워크

- 로컬/원격/동적 SOCKS5 포워딩과 연결 시 시작할 수 있는 저장 규칙
- 수신 중인 서버 포트 감지 및 원클릭 포워딩
- 터미널 없는 독립 SSH 터널, 선택적으로 앱 시작 시 실행
- 재연결 후 포워딩 복원; 수동 중지 규칙은 중지 유지
- 앱 전체 프록시와 기업 CA를 포함한 시스템 신뢰 인증서를 사용하는 네트워크 요청
- 연결 토폴로지 및 임시 소켓 디버깅

### 원격 데스크톱

- 키보드, 마우스, 클립보드, 배율, 재연결을 지원하는 RDP/VNC *(플러그인)*
- 다중 모니터와 네트워크 품질 프로필을 갖춘 RDP *(플러그인)*
- VeNCrypt TLS 기반 VNC *(플러그인)*
- SSH 점프 호스트를 통한 RDP/VNC *(플러그인)*
- 내장 뷰어에서 WSLg 앱 실행(Windows)

### 호스트 도구

- 로컬 컴퓨터 및 모든 SSH 서버 지원
- CPU, 메모리, 스왑, 디스크, 네트워크, 부하, 지연 시간과 추세 그래프
- 프로세스(신호, 중지/계속, nice), 서비스(systemd, launchd, BSD, Windows), 예약 작업, 패키지; Windows winget 업그레이드 및 제거
- 프리셋/추적 모드 로그, 외부 노출 표시가 있는 수신 포트, 공간 부족 경고가 있는 파일 시스템
- Docker 컨테이너의 상태, 포트, 시작, 중지, 재시작, 로그
- NVIDIA, AMD ROCm, Hygon DCU, Huawei Ascend, Intel XPU, Moore Threads MUSA, Cambricon GPU/NPU 모니터링
- tmux, Zellij, Herdr 세션 연결, 생성, 분할, 이름 변경
- 연결 풀/연결 매트릭스 대시보드 및 터미널 모음의 SSH 지연 시간

### OxideSens AI

- OpenAI(Responses API 포함), Anthropic, Gemini, DeepSeek, xAI Grok, Xiaomi MiMo, Ollama 및 OpenAI 호환 엔드포인트, 추론 강도 조절
- 선택한 터미널, 파일, 설정의 문맥과 명령 실행, 대화형 프로그램 조작, 파일 편집 및 전송 도구
- 읽기 전용/기본/승인 불필요 모드, 도구 분류별 권한, 명령 정책 및 파괴적이거나 인증 정보와 관련된 명령 추가 검토
- 이미지 첨부, 대화 분기, 메시지 편집, 후속 질문 대기열
- 병렬 하위 에이전트, 백그라운드 작업, 중단 후 작업 재개
- ACP를 통한 Codex/Claude Code 및 OpenCode, Antigravity, Cursor, Grok Build, GitHub Copilot CLI, Qwen Code, Kimi CLI 실행기 *(플러그인)*
- stdio, Streamable HTTP 또는 기존 SSE로 MCP 서버 연결
- 외부 MCP 제어: 편집기와 다른 AI 도구가 저장 연결을 사용하며, 도구 그룹별 권한 부여 및 철회 가능
- [Agent Skills](../../docs/agent-skills.md)(`SKILL.md`)와 `/skill-name` 자동 완성
- BM25/HNSW 벡터 검색, CJK 토큰화, Mermaid 미리보기를 갖춘 Markdown 노트 지식 기반

### 보안

- 연결을 유지하면서 Touch ID(macOS) 또는 Windows Hello(Windows)로 앱 잠금
- sudo/su 요청에 저장 비밀번호 제안, 로컬/연결/호스트 패턴으로 범위 제한, 2단계 요청용 TOTP 옵션
- 필터, 보관 기간, JSON/CSV 내보내기가 있는 작업 감사
- 암호화된 세션 녹화, 탐색 및 속도 조절 재생
- 플러그인 활성화 전 권한 검토

### 동기화 및 백업

- WebDAV, S3 호환 저장소(Alibaba Cloud OSS 포함), Git 저장소, GitHub Gist, Dropbox, OneDrive, Google Drive, HTTP JSON
- 동기화 대상 선택, 여러 기기 변경 병합, 충돌별 해결
- 롤백 백업 및 중단된 동기화 복구
- 연결, 포워딩, 설정, 빠른 명령, 키를 암호화 `.oxide`로 이동; 가져올 때 건너뛰기, 이름 변경, 대체, 병합
- 전용 암호화 키 저장소를 포함한 폴더에서 포터블 실행
- 안정/베타 채널을 통한 앱 내 업데이트

### 개인 설정

- 밝음/어두움/시스템 외관, 앱/터미널별 테마 및 테마 편집기
- 이미지/애니메이션/비디오 배경, 안개/입자/유성/수면 빛무늬 생성 효과와 추가 미디어 팩 *(플러그인)*
- 창 불투명도와 모서리 스타일
- 독립 CJK 글꼴, 합자, 굵기를 지원하는 글꼴 스택과 편집기별 글자 설정
- UI 배율, 커서, 여백, 동작 설정
- 사용자 지정 단축키와 검색 및 재배열 가능한 설정
- 11개 UI 언어, IME 입력, 양방향 텍스트

### 플러그인

| 종류 | 실행 방식 | 접근 범위 |
|---|---|---|
| **매니페스트** | 선언적 확장 | 없음; 코드 실행 없음 |
| **WASM** | Wasmtime/WASI 또는 보조 프로세스 | 허가된 호스트 기능만 |
| **프로세스** | 일반 로컬 프로세스 | 로컬 프로그램과 동일; OS 샌드박스 없음 |

프로세스 플러그인은 신뢰할 수 있는 출처에서만 설치하세요. 1.x 플러그인은 목록에 남아 있어도 2.x에서 실행하지 않습니다.

### 명령줄 도구

**설정 → 일반 → CLI Companion**에서 `oxideterm`을 설치합니다. 앱 설정을 공유하므로 창 없이 확인, 스크립트, 백업을 수행하거나 셸에서 새 탭을 열 수 있습니다. 변경 명령은 `--dry-run`으로 계획을 보여 주고 `--yes`로만 적용하며 롤백 백업을 남깁니다.

```bash
oxideterm terminal --cwd ~/project
oxideterm doctor --strict
oxideterm connections search prod
oxideterm forwards list --format json
oxideterm oxide export ./profile.oxide --connection prod --password-stdin
```

Bash, Zsh, Fish, PowerShell, Elvish 완성을 지원합니다. 전체 명령은 [CLI 안내서(영어)](../../docs/user-guide/en/cli.md)를 참조하세요.

---

<a id="install"></a>

## 설치

[최신 릴리스](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)에서 OxideTerm을 다운로드하세요.

| OS | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG(Intel) | DMG(Apple Silicon) |
| **Windows** | 설치 프로그램(`.exe`) | 설치 프로그램(`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

릴리스 페이지에는 포터블 압축 파일, 서명, 검증용 `sha256sums.txt`도 있습니다.

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

### 창이 그려지지 않는 경우

환경에 `OXIDETERM_RENDER_PROFILE=compatibility`를 설정하고 OxideTerm을 다시 시작하세요.

### 시작하기

1. **세션 관리자**에서 서버를 추가하거나 `~/.ssh/config`에서 가져옵니다.
2. 연결하여 터미널을 엽니다.
3. 같은 서버에서 SFTP, 포트 포워딩 또는 IDE를 엽니다.
4. OxideSens는 **설정**에서 AI 제공업체를 추가하면 사용할 수 있습니다.

기타 기능은 [문서](https://oxideterm.app)를 참조하세요. 직접 빌드하려면 [소스에서 빌드](#build-from-source)를 확인하세요.

---

## 개인정보 보호 및 보안

- 비밀번호와 키는 시스템 키 저장소(macOS Keychain, Windows 자격 증명 관리자, Linux libsecret)에 저장합니다. AI 키는 앱 로그에서 제외합니다.
- 메모리의 비밀 정보는 해제 시 zeroize로 지웁니다.
- 호스트 키는 최초 신뢰 후 `~/.ssh/known_hosts`와 비교하며, 변경되면 거부합니다.
- AI 제공업체에 보내는 텍스트는 먼저 인증 정보를 가립니다.
- 세션 녹화는 기본으로 꺼져 있습니다. 기기에 암호화 저장하고 동기화에서 제외하며 키 입력을 수집하지 않습니다.
- 알림 및 감사도 기본으로 꺼져 있습니다. 켜면 데이터는 기기에 남고 민감한 세부 사항은 암호화되며 OxideTerm 내부 작업만 다룹니다.
- `.oxide`는 ChaCha20-Poly1305 및 Argon2id 파생 키(메모리 256 MB, 4회)로 암호화합니다.

---

<a id="build-from-source"></a>

## 소스에서 빌드

Rust(rustup이 `rust-toolchain.toml`에 고정된 도구 체인을 설치)와 GPUI를 실행할 수 있는 데스크톱 환경이 필요합니다.

```bash
# Run the app
cargo run

# Build the oxideterm CLI
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

Nix: `nix build .#oxideterm`, `nix run .#oxideterm` 또는 `nix develop`. CLI 결과물은 `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`에 놓입니다.

### 전체 구조

UI, 터미널, SSH는 하나의 Rust 프로세스에서 실행합니다. 선택적인 원격 에이전트와 플랫폼 보조 도구는 별도 프로세스입니다. SSH/로컬 PTY 출력은 `TerminalState`로 직접 들어가 GPUI가 WebView, JSON 브리지, xterm.js 없이 그립니다.

`NodeRouter`는 DashMap 기반 풀 `SshConnectionRegistry`에서 연결을 제공합니다. 자체 연결로 설정된 터미널을 제외하고 터미널, SFTP, 포워딩, 편집기는 서버별 연결 하나를 공유합니다. 상태는 `connecting → active → idle → link_down → reconnecting`입니다. 점프 호스트가 실패하면 하위 서버는 `link_down`이 됩니다. 포워딩 규칙마다 리스너가 있으며, 재연결 후 SFTP 세션은 새로 취득하고 이전 연결의 작업을 몰래 옮기지 않습니다.

```text
crates/
├── oxideterm-gpui-app/        # Desktop app: windows, workspace, settings
├── oxideterm-gpui-terminal/   # Terminal panes: input, selection, overlays
├── oxideterm-terminal/        # Terminal model and escape-sequence parsing
├── oxideterm-ssh/             # SSH connections, NodeRouter, jump hosts
├── oxideterm-sftp/            # SFTP sessions and transfers
├── oxideterm-forwarding/      # Port forwarding runtime
├── oxideterm-gpui-ide/        # Remote IDE
├── oxideterm-ai/              # OxideSens: providers, MCP, RAG
├── oxideterm-cli/             # The oxideterm command
├── oxideterm-i18n/            # Locale catalogs for the 11 interface languages
└── gpui-ce/                   # Patched copy of GPUI
```

관련 자료(영어): [개발 안내서](../../docs/development/README.md), [crate 지도](../../docs/development/crate-map.md), [아키텍처](../../docs/user-guide/en/architecture.md).

---

## 기여하기

코드, 문서, 번역, 플러그인, 테스트, 버그 재현을 환영합니다. 큰 변경은 먼저 issue로 논의하세요. [CONTRIBUTING.md](../../docs/CONTRIBUTING.md)에 PR 준비 방법이 있습니다.

민감 정보를 가린 진단 패키지를 첨부하면 버그 수정에 도움이 됩니다:

```bash
oxideterm report --bundle ./oxideterm-report.zip
```

소스에서 실행할 때는 `cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip`을 사용하세요.

### 기여자

OxideTerm을 더 나은 앱으로 만드는 데 함께해 주시는 모든 분께 감사드립니다.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="OxideTerm 기여자">
  </a>
</p>

---

## 감사의 말

OxideTerm은 다음 오픈 소스 프로젝트를 기반으로 합니다:

- [russh](https://github.com/warp-tech/russh)와 [ring](https://github.com/briansmith/ring): SSH 및 암호화
- [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui): Zed의 GPU UI 프레임워크
- [alacritty_terminal](https://github.com/alacritty/alacritty): 터미널 에뮬레이션(VT100–VT500)
- [portable-pty](https://github.com/wez/wezterm/tree/main/pty): 로컬 셸, Windows에서는 ConPTY
- [Tokio](https://tokio.rs): 비동기 런타임
- [Wasmtime](https://wasmtime.dev/): WASM 플러그인 실행
- [tree-sitter](https://tree-sitter.github.io/): 편집기 구문 강조

---

## 라이선스

OxideTerm은 추가 제한 없이 [GPL-3.0-only](../../LICENSE)로 배포됩니다. 의존성 고지는 [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)와 [NOTICE](../../NOTICE)를 참조하세요.

소유하거나 접근을 허가받은 시스템에서만 사용하세요. 전체 법적 고지는 [LEGAL.md](../../LEGAL.md)에 있습니다.

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
