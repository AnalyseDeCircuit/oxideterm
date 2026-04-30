<p align="center">
  <img src="src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
  <br>
  <em>If you like OxideTerm, please consider giving it a star on GitHub! ⭐️</em>
</p>


<p align="center">
  <strong>OxideTerm is a local-first SSH workspace, not just a terminal.</strong>
  <br>
  <em>Open a remote node once, then work around it: shell, SFTP, port forwarding, trzsz, lightweight editing, and BYOK AI.</em>
  <br>
  <strong>Zero Electron. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust SSH.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.4.0--beta.3-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-1.85+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
  <img src="https://img.shields.io/github/downloads/AnalyseDeCircuit/oxideterm/total?color=brightgreen" alt="Total Downloads">
</p>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases/latest">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=Download%20Latest&style=for-the-badge&color=brightgreen" alt="Download Latest Release">
  </a>
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?include_prereleases&label=Download%20Latest%20Beta&style=for-the-badge&color=orange" alt="Download Latest Beta">
  </a>
</p>

<p align="center">
  🌐 <strong><a href="https://oxideterm.app">oxideterm.app</a></strong> — Documentation & website
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="docs/readme/README.zh-Hans.md">简体中文</a> | <a href="docs/readme/README.zh-Hant.md">繁體中文</a> | <a href="docs/readme/README.ja.md">日本語</a> | <a href="docs/readme/README.ko.md">한국어</a> | <a href="docs/readme/README.fr.md">Français</a> | <a href="docs/readme/README.de.md">Deutsch</a> | <a href="docs/readme/README.es.md">Español</a> | <a href="docs/readme/README.it.md">Italiano</a> | <a href="docs/readme/README.pt-BR.md">Português</a> | <a href="docs/readme/README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

https://github.com/user-attachments/assets/4ba033aa-94b5-4ed4-980c-5c3f9f21db7e

*🤖 OxideSens AI — control live terminals and workspace tools from one assistant.*

</div>

---

## Why OxideTerm?

| If you care about... | OxideTerm gives you... |
|---|---|
| SSH workspace, not just a shell | **Remote-node workspace**: one node with terminal, SFTP, port forwarding, trzsz, mini IDE, monitoring, and AI context around it |
| Local shells in the same workflow | **Hybrid engine**: local PTY (zsh/bash/fish/pwsh/WSL2) and remote SSH live side by side, so local and remote work stay in one workspace |
| No cloud account for SSH workflows | **Local-first core**: SSH, SFTP, forwarding, local shell, and config work without signup |
| BYOK AI instead of platform credits | **OxideSens**: use your own OpenAI/Ollama/DeepSeek/OpenAI-compatible endpoint with MCP and RAG support |
| No Electron runtime | **Tauri 2.0**: native Rust backend, 25–40 MB binary |
| No OpenSSL baggage | **russh 0.59**: pure Rust SSH compiled against `ring` — zero OpenSSL/libssh2 dependency |
| No telemetry or app subscription | **Zero tracking, zero subscription for core SSH workflows**: SSH/SFTP/port forwarding/local shell need no account or app subscription; your data stays on your machine by default; cloud sync is opt-in via [official plugin](#official-plugins) |
| Reconnect stability | **Grace Period reconnect**: probes old connection 30s before killing it — your vim/htop/yazi can survive network hiccups |
| Remote file work without VS Code Remote | **Built-in SFTP + mini IDE**: browse, preview, transfer, and edit remote files over the same SSH workspace |
| Credential safety | **Encrypted at rest**: passwords and API keys stay in OS keychain, saved connection metadata is sealed locally, and `.oxide` files use ChaCha20-Poly1305 + Argon2id encryption |

## What It Is / Is Not

OxideTerm is a **local-first SSH workspace**: open a remote node once, then operate its shell, files, ports, in-terminal transfers, lightweight editing, and AI context from one place.

OxideTerm is **not** a cloud AI platform, a hosted agent service, a generic remote-protocol toolbox, or a project whose main selling point is terminal-rendering benchmarks. Many modern terminals are evolving around local shells, AI panels, or cloud agent platforms; OxideTerm focuses on the local-first SSH workspace.

---

## Screenshots

<table>
<tr>
<td align="center"><strong>SSH Terminal + OxideSens AI</strong><br/><br/><img src="docs/screenshots/terminal/SSHTERMINAL.png" alt="SSH Terminal with OxideSens AI sidebar" /></td>
<td align="center"><strong>SFTP File Manager</strong><br/><br/><img src="docs/screenshots/sftp/sftp.png" alt="SFTP dual-pane file manager with transfer queue" /></td>
</tr>
<tr>
<td align="center"><strong>Built-in IDE (CodeMirror 6)</strong><br/><br/><img src="docs/screenshots/miniIDE/miniide.png" alt="Built-in IDE mode with CodeMirror 6 editor" /></td>
<td align="center"><strong>Smart Port Forwarding</strong><br/><br/><img src="docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Smart port forwarding with auto-detection" /></td>
</tr>
</table>

---

## Download

Download the latest release from [GitHub Releases](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest).

---

## Feature Overview

| Category | Features |
|---|---|
| **Terminal** | Local PTY (zsh/bash/fish/pwsh/WSL2), SSH remote, split panes, broadcast input, session recording/playback (asciicast v2), WebGL rendering, 30+ themes + custom editor, command palette (`⌘K`), zen mode, **trzsz** in-band file transfer |
| **SSH & Auth** | Connection pooling & multiplexing, ProxyJump (unlimited hops) with topology graph, auto-reconnect with Grace Period, Agent Forwarding. Auth: password, SSH key (RSA/Ed25519/ECDSA), SSH Agent, certificates, keyboard-interactive 2FA, Known Hosts TOFU |
| **SFTP** | Dual-pane browser, drag-and-drop, smart preview (images/video/audio/code/PDF/hex/fonts), transfer queue with progress & ETA, bookmarks, archive extraction |
| **IDE Mode** | CodeMirror 6 with 24 languages, file tree + Git status, multi-tab, conflict resolution, integrated terminal. Optional remote agent for Linux; unsupported architectures can self-build and upload |
| **Port Forwarding** | Local (-L), Remote (-R), Dynamic SOCKS5 (-D), lock-free message-passing I/O, auto-restore on reconnect, death reporting, idle timeout |
| **AI (OxideSens)** | Target-first assistant for saved connections, live SSH sessions, terminal buffers, SFTP paths, settings, and knowledge base entries; can diagnose remote output, run approved commands, inspect files, and explain failures without an OxideTerm account |
| **Plugins** | Runtime ESM loading, 18 API namespaces, 24 UI Kit components, frozen API + Proxy ACL, circuit breaker, auto-disable on errors |
| **CLI** | `oxt` companion: JSON-RPC 2.0 over Unix Socket / Named Pipe, status/health/list/session inspect/forward/config/connect/focus/attach/SFTP/import/AI, human + JSON output |
| **Security** | .oxide encrypted export (ChaCha20-Poly1305 + Argon2id 256 MB), encrypted local config at rest, OS keychain, Touch ID (macOS), portable encrypted keystore, host key TOFU, `zeroize` memory clearing |
| **i18n** | 11 languages: EN, 简体中文, 繁體中文, 日本語, 한국어, FR, DE, ES, IT, PT-BR, VI |

---

## Under the Hood

### Architecture — Dual-Plane Communication

OxideTerm separates terminal data from control commands into two independent planes:

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

- **Data plane (WebSocket)**: each SSH session gets its own WebSocket port. Terminal bytes flow as binary frames with a Type-Length-Payload header — no JSON serialization, no Base64 encoding, zero overhead in the hot path.
- **Control plane (Tauri IPC)**: connection management, SFTP ops, forwarding, config — structured JSON, but off the critical path.
- **Node-first addressing**: the frontend never touches `sessionId` or `connectionId`. Everything is addressed by `nodeId`, resolved atomically server-side by the `NodeRouter`. SSH reconnect changes the underlying `connectionId` — but SFTP, IDE, and forwards are completely unaffected.

### 🔩 Pure Rust SSH — russh 0.59

The entire SSH stack is **russh 0.59** compiled against the **`ring`** crypto backend:

- **Zero C/OpenSSL dependencies** — the full crypto stack is Rust. No more "which OpenSSL version?" debugging.
- Full SSH2 protocol: key exchange, channels, SFTP subsystem, port forwarding
- ChaCha20-Poly1305 and AES-GCM cipher suites, Ed25519/RSA/ECDSA keys
- Custom **`AgentSigner`**: wraps system SSH Agent and satisfies russh's `Signer` trait, solving RPITIT `Send` bound issues by cloning `&AgentIdentity` to an owned value before crossing `.await`

```rust
pub struct AgentSigner { /* wraps system SSH Agent */ }
impl Signer for AgentSigner { /* challenge-response via Agent IPC */ }
```

- **Platform support**: Unix (`SSH_AUTH_SOCK`), Windows (`\\.\pipe\openssh-ssh-agent`)
- **Proxy chains**: each hop independently uses Agent auth
- **Reconnect**: `AuthMethod::Agent` replayed automatically

### 🔄 Smart Reconnect with Grace Period

Most SSH clients kill everything on disconnect and start fresh. OxideTerm's reconnect orchestrator takes a fundamentally different approach:

1. **Detect** WebSocket heartbeat timeout (300s, tuned for macOS App Nap and JS timer throttling)
2. **Snapshot** full state: terminal panes, in-flight SFTP transfers, active port forwards, open IDE files
3. **Intelligent probing**: `visibilitychange` + `online` events trigger proactive SSH keepalive (~2s detection vs 15-30s passive timeout)
4. **Grace Period** (30s): probe the old SSH connection via keepalive — if it recovers (e.g., WiFi AP switch), your TUI apps (vim, htop, yazi) survive completely untouched
5. If recovery fails → new SSH connection → auto-restore forwards → resume SFTP transfers → reopen IDE files

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

All logic runs through a dedicated `ReconnectOrchestratorStore` — zero reconnect code scattered in hooks or components.

### 🛡️ SSH Connection Pool

Reference-counted `SshConnectionRegistry` backed by `DashMap` for lock-free concurrent access:

- **One connection, many consumers**: terminal, SFTP, port forwards, and IDE share a single physical SSH connection — no redundant TCP handshakes
- **State machine per connection**: `connecting → active → idle → link_down → reconnecting`
- **Lifecycle management**: configurable idle timeout (5m / 15m / 30m / 1h / never), 15s keepalive interval, heartbeat failure detection
- **WsBridge heartbeat**: 30s interval, 5 min timeout — tolerates macOS App Nap and browser JS throttling
- **Cascade propagation**: jump host failure → all downstream nodes automatically marked `link_down` with status sync
- **Idle disconnect**: emits `connection_status_changed` to frontend (not just internal `node:state`), preventing UI desync

### 🤖 OxideSens AI

Privacy-first AI assistant with dual interaction modes:

- **Inline panel** (`⌘I`): quick terminal commands, output injected via bracketed paste
- **Sidebar chat**: persistent conversations with full history
- **Target-first workspace context**: sees saved connections, live SSH sessions, terminal buffers, SFTP paths, settings, and knowledge base entries as workspace targets
- **Approved actions**: can diagnose remote output, run approved commands, inspect files, and explain failures without requiring an OxideTerm account
- **MCP support**: connect external [Model Context Protocol](https://modelcontextprotocol.io) servers (stdio & SSE) for third-party tool integration
- **RAG Knowledge Base** (v0.20): import Markdown/TXT documents into scoped collections (global or per-connection). Hybrid search fuses BM25 keyword index + vector cosine similarity via Reciprocal Rank Fusion. Markdown-aware chunking preserves heading hierarchy. CJK bigram tokenizer for Chinese/Japanese/Korean.
- **Providers**: OpenAI, Ollama, DeepSeek, OneAPI, or any `/v1/chat/completions` endpoint
- **Security**: API keys stored in OS keychain; on macOS, key reads gated behind **Touch ID** via `LAContext` — no entitlements or code-signing required, cached after first auth per session

### 🔀 Port Forwarding — Lock-Free I/O

Full local (-L), remote (-R), and dynamic SOCKS5 (-D) forwarding:

- **Message-passing architecture**: SSH Channel owned by a single `ssh_io` task — no `Arc<Mutex<Channel>>`, eliminating mutex contention entirely
- **Death reporting**: forward tasks actively report exit reason (SSH disconnect, remote port close, timeout) for clear diagnostics
- **Auto-restore**: `Suspended` forwards automatically resume on reconnect without user intervention
- **Idle timeout**: `FORWARD_IDLE_TIMEOUT` (300s) prevents zombie connections from accumulating

### 📦 trzsz — In-Band File Transfer

Upload and download files directly through the SSH terminal session — no SFTP connection required:

- **In-band protocol**: files travel as base64-encoded frames inside the existing terminal stream — works transparently through ProxyJump chains and tmux without extra ports or agents
- **Bidirectional**: server runs `tsz <file>` to send files to the client; `trz` triggers client upload; drag-and-drop supported
- **Directory support**: recursive transfers via `trz -d` / `tsz -d`
- **Transfer limits**: configurable per-session limits for chunk size, file count, and total bytes
- **Native Tauri I/O**: file reads and writes use Tauri native file dialogs and Rust I/O — no browser memory constraints
- **Live notifications**: toast notifications for start, completion, cancellation, and errors — including a hint when trzsz is detected but the feature is disabled
- Enable in **Settings → Terminal → In-Band Transfer**

### 🔌 Runtime Plugin System

Dynamic ESM loading with a security-hardened, frozen API surface:

- **PluginContext API**: 8 namespaces — terminal, ui, commands, settings, lifecycle, events, storage, system
- **24 UI Kit components**: pre-built React components (buttons, inputs, dialogs, tables…) injected into plugin sandboxes via `window.__OXIDE__`
- **Security membrane**: `Object.freeze` on all context objects, Proxy-based ACL, IPC whitelist, circuit breaker with auto-disable after repeated errors
- **Shared modules**: React, ReactDOM, zustand, lucide-react exposed for plugin use without bundling duplicates

### ⚡ Adaptive Rendering

Three-tier render scheduler that replaces fixed `requestAnimationFrame` batching:

| Tier | Trigger | Rate | Benefit |
|---|---|---|---|
| **Boost** | Frame data ≥ 4 KB | 120 Hz+ (ProMotion native) | Eliminates scroll lag on `cat largefile.log` |
| **Normal** | Standard typing | 60 Hz (RAF) | Smooth baseline |
| **Idle** | 3s no I/O / tab hidden | 1–15 Hz (exponential backoff) | Near-zero GPU load, battery savings |

Transitions are fully automatic — driven by data volume, user input, and Page Visibility API. Background tabs continue flushing data via idle timer without waking RAF.

### 🔐 .oxide Encrypted Export

Portable, tamper-proof connection backup:

- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations — GPU brute-force resistant
- **SHA-256** integrity checksum
- **Optional key embedding**: private keys base64-encoded into the encrypted payload
- **Pre-flight analysis**: auth type breakdown, missing key detection before export

### 📡 ProxyJump — Topology-Aware Multi-Hop

- Unlimited chain depth: `Client → Jump A → Jump B → … → Target`
- Auto-parse `~/.ssh/config`, build topology graph, Dijkstra pathfinding for optimal route
- Jump nodes reusable as independent sessions
- Cascade failure propagation: jump host down → all downstream nodes auto-marked `link_down`

### ⚙️ Local Terminal — Thread-Safe PTY

Cross-platform local shell via `portable-pty 0.8`, feature-gated behind `local-terminal`:

- `MasterPty` wrapped in `std::sync::Mutex` — dedicated I/O threads keep blocking PTY reads off the Tokio event loop
- Shell auto-detection: `zsh`, `bash`, `fish`, `pwsh`, Git Bash, WSL2
- `cargo build --no-default-features` strips PTY for mobile/lightweight builds

### 🪟 Windows Optimization

- **Native ConPTY**: directly invokes Windows Pseudo Console API — full TrueColor and ANSI support, no legacy WinPTY
- **Shell scanner**: auto-detects PowerShell 7, Git Bash, WSL2, CMD via Registry and PATH

### And More

- **IDE Mode**: CodeMirror 6 over SFTP, 24 languages, file tree with Git status, multi-tab, conflict resolution — optional remote agent (~1 MB) for enhanced features on Linux
- **Resource profiler**: live CPU/memory/network via persistent SSH channel reading `/proc/stat`, delta-based calculation, auto-degrades to RTT-only on non-Linux
- **Custom theme engine**: 31 built-in themes, visual editor with live preview, 20 xterm.js fields + 24 UI color variables, auto-derive UI colors from terminal palette
- **Session recording**: asciicast v2 format, full record and playback
- **Broadcast input**: type once, send to all split panes — batch server operations
- **Background gallery**: per-tab background images, 16 tab types, opacity/blur/fit control
- **CLI companion** (`oxt`): ~1 MB binary, JSON-RPC 2.0 over Unix Socket / Named Pipe, status/health/list/session inspect/forward/config/connect/focus/attach/SFTP/import/AI with human or `--json` output
- **WSL Graphics** ⚠️ experimental: built-in VNC viewer — 9 desktop environments + single-app mode, WSLg detection, Xtigervnc + noVNC

#### Official Plugins

| Plugin | Description | Repository |
|---|---|---|
| **Cloud Sync** | Encrypted self-hosted sync — upload and import `.oxide` snapshots via WebDAV, HTTP JSON, Dropbox, Git, or S3 | [oxideterm.cloud-sync](https://github.com/AnalyseDeCircuit/oxideterm.cloud-sync) |
| **Telnet Client** | Native Telnet client for routers, switches, and legacy devices — no external binary needed | [oxideterm.telnet](https://github.com/AnalyseDeCircuit/oxideterm.telnet) |

<details>
<summary>📸 11 languages in action</summary>
<br>
<table>
  <tr>
    <td align="center"><img src="docs/screenshots/overview/en.png" width="280"><br><b>English</b></td>
    <td align="center"><img src="docs/screenshots/overview/zhHans.png" width="280"><br><b>简体中文</b></td>
    <td align="center"><img src="docs/screenshots/overview/zhHant.png" width="280"><br><b>繁體中文</b></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/overview/ja.png" width="280"><br><b>日本語</b></td>
    <td align="center"><img src="docs/screenshots/overview/ko.png" width="280"><br><b>한국어</b></td>
    <td align="center"><img src="docs/screenshots/overview/fr.png" width="280"><br><b>Français</b></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/overview/de.png" width="280"><br><b>Deutsch</b></td>
    <td align="center"><img src="docs/screenshots/overview/es.png" width="280"><br><b>Español</b></td>
    <td align="center"><img src="docs/screenshots/overview/it.png" width="280"><br><b>Italiano</b></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/overview/pt-BR.png" width="280"><br><b>Português</b></td>
    <td align="center"><img src="docs/screenshots/overview/vi.png" width="280"><br><b>Tiếng Việt</b></td>
    <td></td>
  </tr>
</table>
</details>

---

## Runtime Requirements

OxideTerm uses the native WebView runtime provided by the operating system. Most users already have it installed; install these manually only if the app fails to launch or your environment is air-gapped.

| Platform | Runtime Dependency |
|---|---|
| **Windows** | [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) — pre-installed on Windows 10 (1803+) and Windows 11. For **air-gapped / intranet** environments, use the [Evergreen Standalone Installer](https://developer.microsoft.com/en-us/microsoft-edge/webview2/#download) (offline, ~170 MB) or deploy the **Fixed Version** runtime via group policy. |
| **macOS** | None (uses native WebKit) |
| **Linux** | `libwebkit2gtk-4.1` (usually pre-installed on modern desktops) |

---

## Portable Mode

OxideTerm supports a fully self-contained portable mode — all data (connections, secrets, settings) is stored beside the application binary, making it suitable for USB drives or air-gapped environments.

### Activation

**Option A — Marker file** (simplest): create an empty file named `portable` (no extension) next to the app.

| Platform | Where to place the `portable` file |
|---|---|
| **macOS** | Next to `OxideTerm.app` (its sibling directory) |
| **Windows** | Next to `OxideTerm.exe` |
| **Linux (AppImage)** | Next to the `.AppImage` file |

```
/my-usb/
├── OxideTerm.app   (or .exe / .AppImage)
├── portable        ← empty file you create
└── data/           ← created automatically on first launch
```

**Option B — `portable.json`** (custom data directory): place a `portable.json` in the same location:

```json
{
  "enabled": true,
  "dataDir": "my-data"
}
```

- `enabled` defaults to `true` if omitted
- `dataDir` must be a **relative path** (no `..`); defaults to `data` if omitted

### How It Works

1. **First launch** — a bootstrap screen prompts you to create a portable password. This password encrypts a local keystore (ChaCha20-Poly1305 + Argon2id) that protects all saved secrets.
2. **Subsequent launches** — enter the password to unlock. On macOS with Touch ID, you can optionally bind biometric unlock in **Settings → General → Portable Runtime**.
3. **Instance lock** — only one OxideTerm instance can use a portable data directory at a time (`data/.portable.lock`).
4. **Management** — change the portable password or toggle biometric unlock in **Settings → General → Portable Runtime**.
5. **Portability** — copy the entire folder (app + `portable` marker + `data/`) to another machine. The password travels with the keystore.

> [!TIP]
> Automatic updates are disabled in portable mode. To update, replace the application binary while keeping the `data/` directory.

---

## Quick Start

### Prerequisites

- **Rust** 1.85+
- **Node.js** 18+ (pnpm recommended)
- **Platform tools**:
  - macOS: Xcode Command Line Tools
  - Windows: Visual Studio C++ Build Tools
  - Linux: `build-essential`, `libwebkit2gtk-4.1-dev`, `libssl-dev`

### Development

```bash
git clone https://github.com/AnalyseDeCircuit/oxideterm.git
cd oxideterm && pnpm install

# Build CLI companion (required for CLI features)
pnpm cli:build

# Full app (frontend + Rust backend with hot reload)
pnpm run tauri dev

# Frontend only (Vite on port 1420)
pnpm dev

# Production build
pnpm run tauri build
```

---

## Tech Stack

| Layer | Technology | Details |
|---|---|---|
| **Framework** | Tauri 2.0 | Native binary, 25–40 MB |
| **Runtime** | Tokio + DashMap 6 | Full async, lock-free concurrent maps |
| **SSH** | russh 0.59 (`ring`) | Pure Rust, zero C deps, SSH Agent |
| **Local PTY** | portable-pty 0.8 | Feature-gated, ConPTY on Windows |
| **Frontend** | React 19.1 + TypeScript 5.8 | Vite 7, Tailwind CSS 4 |
| **State** | Zustand 5 | 19 specialized stores |
| **Terminal** | xterm.js 6 + WebGL | GPU-accelerated, 60fps+ |
| **Editor** | CodeMirror 6 | 24 language modes |
| **Encryption** | ChaCha20-Poly1305 + Argon2id | AEAD + memory-hard KDF (256 MB) |
| **Storage** | redb 2.1 | Embedded KV store |
| **i18n** | i18next 25 | 11 languages × 22 namespaces |
| **Plugins** | ESM Runtime | Frozen PluginContext + 24 UI Kit |
| **CLI** | JSON-RPC 2.0 | Unix Socket / Named Pipe |

---

## Project Scale

Measured with `tokei`, excluding dependencies and build artifacts.

| Metric | Current Size |
|---|---:|
| Total code | 286K+ |
| TypeScript / TSX | 130K+ |
| Rust | 100K+ |
| Frontend test code | 24K+ |
| Frontend test files | 128 |
| Source files (`src` + `src-tauri/src`) | 664 |

---

## Security

| Concern | Implementation |
|---|---|
| **Passwords** | OS keychain (macOS Keychain / Windows Credential Manager / libsecret) |
| **Portable Keystore** | ChaCha20-Poly1305 encrypted vault beside the app, optional biometric binding via OS keychain |
| **AI API Keys** | OS keychain + Touch ID biometric gate on macOS |
| **Export** | .oxide: ChaCha20-Poly1305 + Argon2id (256 MB memory, 4 iterations) |
| **Memory** | Rust memory safety + `zeroize` for sensitive data clearing |
| **Host keys** | TOFU with `~/.ssh/known_hosts`, rejects changes (MITM prevention) |
| **Plugins** | Object.freeze + Proxy ACL, circuit breaker, IPC whitelist |
| **WebSocket** | Single-use tokens with time limits |

---

## Roadmap

- [x] SSH Agent forwarding
- [ ] Full ProxyCommand support
- [ ] Audit logging
- [ ] Agent enhancements
- [ ] Session search & quick-switch

---

## Support and Maintenance

OxideTerm is maintained on a **best-effort basis** by a solo developer. Bug reports and reproducible regressions are prioritized; feature requests are welcome, but may not always be implemented.

If OxideTerm helps your workflow, a GitHub star, issue reproduction, translation fix, plugin, or pull request all make the project easier to keep moving.

---

## License

**GPL-3.0** — this software is free software licensed under the [GNU General Public License v3.0](https://www.gnu.org/licenses/gpl-3.0.html).

You are free to use, modify, and distribute this software under the terms of the GPL-3.0. Any derivative work must also be distributed under the same license.

OxideTerm changed from **PolyForm Noncommercial 1.0.0** to **GPL-3.0** starting with v1.0.0. We made this switch deliberately: no "open source" cosplay with noncommercial traps or no-competition riders, just clear copyleft freedom for users, forks, redistributors, and commercial operators.

Public code is not automatically open source. If a project advertises a familiar license while adding riders like "no redistribution", "no repackaging", "no competing products", or "no unauthorized distribution platforms", that is source-available branding, not the freedom users expect from open source. OxideTerm does not add no-compete or anti-redistribution riders: the GPL-3.0 terms are the terms.

Full text: [GNU General Public License v3.0](https://www.gnu.org/licenses/gpl-3.0.html)

---

## Acknowledgments

[russh](https://github.com/warp-tech/russh) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [Tauri](https://tauri.app/) · [xterm.js](https://xtermjs.org/) · [CodeMirror](https://codemirror.net/) · [Radix UI](https://www.radix-ui.com/)

---

<p align="center">
  <sub>286,000+ lines of code — built with ⚡ and ☕</sub>
</p>

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
