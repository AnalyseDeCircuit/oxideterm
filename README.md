<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>AI-Powered SSH Client and Remote Operations Workspace</strong>
  <br>
  SSH, Telnet, Serial, RDP/VNC, SFTP, port forwarding, and lightweight editing in one native workspace.
  <br>
  GPU-rendered. Free. No account needed.
  <br>
  <strong>No Electron. No bundled WebView. No telemetry. No subscription. BYOK-first. Pure-Rust SSH without OpenSSL/libssh2.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.9-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Open-source, local-first, and GPU-rendered with <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a>.</sub>
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="docs/readme/README.zh-Hans.md">简体中文</a> | <a href="docs/readme/README.zh-Hant.md">繁體中文</a> | <a href="docs/readme/README.ja.md">日本語</a> | <a href="docs/readme/README.ko.md">한국어</a> | <a href="docs/readme/README.fr.md">Français</a> | <a href="docs/readme/README.de.md">Deutsch</a> | <a href="docs/readme/README.es.md">Español</a> | <a href="docs/readme/README.it.md">Italiano</a> | <a href="docs/readme/README.pt-BR.md">Português</a> | <a href="docs/readme/README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="docs/media/oxideterm-native-hero.png" alt="OxideTerm feature overview" width="920">
</p>

---

## What OxideTerm Is

OxideTerm is an open-source, local-first workspace for connecting to servers and working across terminals, files, forwarding, host tools, and remote desktops.

**What you can do:**

- Manage SSH, Telnet, Serial, RDP/VNC, SFTP, port forwards, local shells, and lightweight editing in one native workspace
- Keep remote work alive through network hiccups with Grace Period reconnect
- Ask OxideSens AI to inspect live sessions and perform approved workspace actions through your own AI provider

Your connections and operational data stay under your control. OxideTerm requires no account, uses your own AI provider when OxideSens is enabled, and keeps the desktop experience free of Electron and bundled browser runtimes.

---

## Why OxideTerm?

| If you care about... | OxideTerm gives you... |
|---|---|
| One remote node, many tools | Terminal, SFTP, port forwarding, RDP/VNC, trzsz, native IDE, monitoring, and OxideSens AI stay attached to the same workspace |
| A desktop app without Electron or a bundled WebView | GPUI draws the interface directly on a GPU surface, without shipping a browser runtime |
| Local-first operations workflows | SSH, Telnet, SFTP, forwarding, RDP/VNC, local shell, serial terminals, and config work without signup |
| BYOK OxideSens AI instead of platform credits | OxideSens uses your OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible endpoint with MCP, RAG, and approved workspace actions |
| Reconnect stability | Grace Period probes the old connection for 30s before replacing it, so TUI apps can survive short network drops |
| Pure-Rust SSH and credential safety | The SSH stack uses `russh` + `ring` without OpenSSL/libssh2; stored credentials use the OS keychain, and `.oxide` bundles use ChaCha20-Poly1305 + Argon2id |

---

## Screenshots

The screenshots below show the OxideTerm workspace across terminal, file, editing, and forwarding workflows.

<table>
<tr>
<td align="center"><strong>SSH Terminal + OxideSens AI</strong><br/><br/><img src="docs/screenshots/terminal/SSHTERMINAL.png" alt="SSH Terminal with OxideSens AI" /></td>
<td align="center"><strong>SFTP File Manager</strong><br/><br/><img src="docs/screenshots/sftp/sftp.png" alt="SFTP dual-pane file manager with transfer queue" /></td>
</tr>
<tr>
<td align="center"><strong>Built-in IDE</strong><br/><br/><img src="docs/screenshots/miniIDE/miniide.png" alt="Built-in IDE mode" /></td>
<td align="center"><strong>Smart Port Forwarding</strong><br/><br/><img src="docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Smart port forwarding with auto-detection" /></td>
</tr>
</table>

---

## Built for Remote Operations

OxideTerm keeps terminal rendering, connection state, reconnect orchestration, files, forwarding, automation, and AI context inside one Rust application. The result is a workspace in which tools share the same server identity and session lifecycle instead of behaving like disconnected utilities.

| Aspect | Bundled browser approach | OxideTerm |
|---|---|---|
| **Rendering** | Browser engine and web layout | GPUI on a GPU surface |
| **Terminal data flow** | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` mutation → GPUI render |
| **Connection lifecycle** | Split across frontend and backend layers | One in-process connection and reconnect pipeline |
| **AI context** | Copied through an application bridge | Built from the active workspace with user approval |
| **Plugin runtime** | Browser scripting environment | Capability-scoped WASM runtime |
| **CLI** | Requires the desktop app running | Standalone binary, direct crate linkage |
| **Runtime boundary** | Desktop wrapper plus browser runtime | Native process with no bundled browser runtime |

---

## Feature Overview

| Category | Features |
|---|---|
| **Terminal & Connections** | Local shells, SSH, Telnet, serial, split panes, shell integration, command marks, recording, trzsz transfers, terminal graphics, multi-hop routes, host-key verification, Agent forwarding, 2FA, and Grace Period reconnect |
| **Files & Remote Editing** | SFTP browsing, transfer queues, speed limits, progress and ETA, bookmarks, safe writes, local file management, remote project trees, multi-tab editing, conflict handling, and workspace restore |
| **Forwarding & Networking** | Local, remote, and dynamic SOCKS5 forwarding, saved rules, reconnect-aware restore, remote port detection, connection topology, and ad-hoc socket debugging |
| **Host Operations & Remote Desktop** | Host monitoring, processes, services, logs, ports, tasks, disks, packages, containers, tmux, built-in RDP and VNC, clipboard, input, reconnect, and viewport-aware sizing |
| **OxideSens & Automation** | BYOK providers, MCP, local RAG, approved workspace actions, command policy, chat history, encrypted cloud sync, portable `.oxide` bundles, and a standalone CLI for automation and diagnostics |
| **Extensions & Personalization** | Capability-scoped WASM plugins, custom tabs and settings, Quick Commands, themes, background images, configurable shortcuts, and 11 interface languages |

---

<div align="center">

<a href="docs/media/ai-terminal-demo.mp4">
  <img src="docs/media/ai-terminal-demo.gif" alt="OxideSens opening a terminal inside OxideTerm" width="920">
</a>

*Watch OxideSens follow a user request and open a terminal inside OxideTerm.*

</div>

---

## Under the Hood

OxideTerm keeps terminal, SSH, Telnet, RDP, VNC, SFTP, forwarding, editing, AI, plugins, and CLI in one Rust architecture. The implementation notes below are for readers who want the engineering details.

<details>
<summary><strong>Architecture, SSH internals, GPUI shell, reconnect, AI, plugins, and more</strong></summary>
<br>

### Architecture — In-Process Core, No WebView Bridge

GPUI and the terminal/SSH backend share one Rust process; optional remote agents and platform helpers remain outside this boundary:

```
┌─────────────────────────────────────────────────┐
│               GPUI Render Loop                  │
│   WorkspaceApp  ·  Tab surfaces  ·  GPUI views  │
└──────────────────────┬──────────────────────────┘
                       │  in-process Arc<> / async
┌──────────────────────▼──────────────────────────┐
│             Domain Crates (Rust async)           │
│  NodeRouter → SshConnectionRegistry             │
│  TerminalState ← SSH PTY channel (russh)        │
│  SftpSession · ForwardManager · IdeWorkspace    │
│  AiProvider · CloudSyncService · PluginHost     │
└─────────────────────────────────────────────────┘
```

There is no serialization boundary between the UI and the SSH/terminal backend. Terminal bytes mutate `TerminalState` directly — no JSON, no WebSocket, no Base64, no xterm.js parse pass. GPUI reads the state and emits GPU draw calls.

### 🔩 Pure Rust SSH — russh (ring)

The desktop app links the `russh` stack directly:

- **No OpenSSL/libssh2 in the SSH stack** — SSH cryptography is provided through `ring`
- Full SSH2: key exchange, channels, SFTP subsystem, port forwarding
- ChaCha20-Poly1305 and AES-GCM, Ed25519/RSA/ECDSA keys
- SSH Agent: Unix (`SSH_AUTH_SOCK`) and Windows (`\\.\pipe\openssh-ssh-agent`)
- Custom `AgentSigner` for russh `Signer` trait compatibility across `.await` bounds
- Multi-hop proxy chains with per-hop independent auth

### 🔄 Smart Reconnect with Grace Period

Reconnect is coordinated by one Rust pipeline:

1. **Detect** SSH keepalive timeout (Rust async task, no JS timer throttling)
2. **Snapshot** terminal panes, SFTP transfers, forwards, IDE files — all in-process
3. **Grace Period** (30 s): probe old SSH connection via keepalive; TUI apps survive on network switch
4. New SSH connection → restore forwards → resume transfers → reopen IDE files

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### 🛡️ SSH Connection Pool

`SshConnectionRegistry` is backed by `DashMap` and shared by workspace consumers:

- **One connection, many consumers**: terminal panes, SFTP, port forwards, and IDE share one physical SSH connection
- **State machine per connection**: `connecting → active → idle → link_down → reconnecting`
- **Node-first addressing**: everything is resolved by `nodeId` → `connectionId` by `NodeRouter`
- **NodeRuntimeStore**: serializable snapshot of all nodes, persisted to `session_tree.json` on every topology change, restored on startup
- **Cascade propagation**: jump host failure → downstream nodes automatically marked `link_down`

### 🤖 OxideSens AI

OxideSens is BYOK-first, with context building performed in-process:

- **Providers**: OpenAI, Anthropic (Claude), Google Gemini, Ollama/any OpenAI-compatible endpoint
- **MCP**: stdio + SSE transports, full tool discovery and invocation
- **RAG**: BM25 full-text + HNSW vector index, Reciprocal Rank Fusion, CJK bigram tokenizer
- **Context boundary**: provider-bound messages pass through credential-pattern redaction, while the user controls which workspace context and actions are approved
- **API keys**: stored in the OS keychain and deliberately excluded from structured logs and desktop-core message payloads

### 🎨 GPUI Desktop Shell

The entire UI is written in Rust using GPUI (Zed's GPU-backed UI framework):

- **No CSS, no DOM, no JavaScript** in the rendering pipeline
- **Workspace tab types**: local terminal, SSH, Telnet, Serial, RDP, VNC, SFTP, IDE, port forwards, session manager, cloud sync, settings, plugins, topology, monitoring, file manager, launcher, graphics, and custom plugin tabs
- **Split pane system**: binary pane tree, draggable dividers, up to 4 panes per terminal tab
- **Command palette**, global key bindings, sidebar panels — all GPUI primitives
- **Immediate-mode rendering**: UI reflects Rust state changes without a serialization round-trip

### 🧱 Terminal State and Rendering

Terminal rendering is modeled as Rust state first, then drawn by GPUI:

- PTY output lands in `TerminalState`; scrollback, cursor, selection, marks, and search state stay in Rust
- Rendering policy can shift between Boost, Normal, and Idle without asking a browser event loop to cooperate
- Sixel and Kitty graphics are tracked as terminal-owned assets instead of DOM nodes or canvas overlays
- Split panes share the same workspace state model, so tab restore and reconnect can snapshot terminal topology together

### 🗂️ SFTP and IDE Workspace

Remote files are part of the same node workspace rather than a separate disconnected feature:

- SFTP sessions are resolved through `NodeRouter`, so reconnect can swap the underlying SSH connection without changing the UI's node address
- Transfer queues track direction, progress, retry state, and speed limits independently from the visible file panes
- IDE tabs keep dirty buffers, remote paths, conflict state, and restore metadata together
- Remote writes use staged/atomic behavior where the backend supports it, keeping partial writes out of normal edit flows

### 🧩 Plugins, CLI, and Diagnostics

Extension and support surfaces stay inside explicit Rust-owned boundaries:

- Plugins run in a wasmtime sandbox with typed host capabilities instead of browser globals
- The CLI links directly to domain crates for doctor, settings, connections, forwards, portable bundles, backups, and reports
- Diagnostics prefer counts, paths, feature flags, and redacted hints over raw secret-bearing payloads
- Mutating CLI flows use dry-run plans, `--yes` guards, and rollback backups where applicable

### 🔀 Port Forwarding — Lock-Free I/O

Forwarding is implemented as a standalone Rust crate:

- Local (-L), Remote (-R), Dynamic SOCKS5 (-D)
- Message-passing architecture: SSH Channel owned by single `ssh_io` task — no `Arc<Mutex<Channel>>`
- Auto-restore on reconnect, death reporting, idle timeout

### 📦 trzsz — In-Band File Transfer

The in-band protocol is integrated directly with native file dialogs:

- Upload/download through the existing terminal stream — no extra ports or agents
- Works through ProxyJump chains
- Native file pickers (no browser memory constraints)
- Bidirectional, directory support, configurable limits

### 🔐 .oxide Encrypted Export

Portable exports use:

- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations — GPU brute-force resistant
- Covers: connections, forwards, settings, quick commands, plugin settings, portable secrets

</details>

---

## Run From Source

**Requirements:** Rust toolchain (Edition 2024), desktop environment capable of running GPUI.

```sh
# Run the app
cargo run

# If the renderer fails on your machine, try the compatibility profile
OXIDETERM_RENDER_PROFILE=compatibility cargo run
```

```sh
# Build the headless CLI companion
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

CLI artifacts land in `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`.

---

## CLI

The headless `oxideterm` CLI works without launching the app — useful for automation, CI, and diagnostics.

```sh
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- forwards list --format json
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- oxide export ./profile.oxide --connection prod --password-stdin
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
cargo run -p oxideterm-cli -- completion install zsh --force

# Path/profile isolation for CI or fixture testing
cargo run -p oxideterm-cli -- --config-dir ./fixture-config doctor --strict
```

---

## Tech Stack

| Layer | Technology | Notes |
|---|---|---|
| **UI framework** | GPUI (Zed) | GPU-backed immediate mode, pure Rust |
| **Runtime** | Tokio + DashMap | Full async, lock-free concurrent maps |
| **SSH** | russh (`ring`) | No OpenSSL/libssh2 in the SSH stack; SSH Agent |
| **Local PTY** | portable-pty | Feature-gated, ConPTY on Windows |
| **Terminal emulation** | alacritty_terminal | VT100–VT500, Sixel, Kitty graphics |
| **Editor** | tree-sitter (syntax), custom buffer | Multi-language, SFTP-backed |
| **Encryption** | ChaCha20-Poly1305 + Argon2id | AEAD + memory-hard KDF (256 MB) |
| **Plugin sandbox** | wasmtime | WASM isolation with native host API |
| **AI streaming** | SSE (OpenAI/Anthropic/Gemini) | In-process, no IPC boundary |
| **RAG** | BM25 + HNSW vector index | CJK bigram tokenizer, RRF fusion |
| **i18n** | oxideterm-i18n (custom) | Built-in loader, 11 shipped locales |

---

## Security

| Concern | Implementation |
|---|---|
| **Stored credentials** | OS keychain (macOS Keychain / Windows Credential Manager / libsecret) |
| **Secret memory** | Secret-bearing Rust types and temporary buffers use `zeroize` / `Zeroizing` at supported ownership boundaries |
| **Diagnostics** | Support output favors structured metadata and redacted hints over secret-bearing payloads |
| **AI context** | Provider-bound messages pass through credential-pattern redaction; workspace context and actions remain user-controlled |
| **`.oxide` export** | ChaCha20-Poly1305 + Argon2id (256 MB memory, 4 iterations) |
| **CLI writes** | Dry-run plans, `--yes` guards, rollback backups for state-changing commands |
| **Host keys** | TOFU with `~/.ssh/known_hosts`, rejects unexpected changes |
| **Plugin sandbox** | WASM isolation via wasmtime, capability-based host API |

---

## Lawful Use Notice

OxideTerm is licensed under GPL-3.0-only without additional license restrictions. When using it, access only systems, networks, and devices that you own or are explicitly authorized to access, and comply with applicable law. Do not use OxideTerm for unauthorized access, service disruption, or bypassing access controls.

---

## Contributing

Contributions are welcome across Rust code, documentation, translations, plugins, testing, and issue reproduction. Open an issue to discuss larger changes, or send a focused pull request for a well-scoped fix.

Bug reports are most useful with a redacted CLI bundle:

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

---

## Support and Maintenance

Reproducible bug reports and regressions with redacted diagnostics are prioritized. Feature requests are reviewed based on scope, safety, and alignment with OxideTerm's remote-server workspace direction.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

If OxideTerm helps your workflow, a GitHub star, issue reproduction, translation fix, plugin, or pull request all make the project easier to keep moving.

---

## License

**GPL-3.0-only**. Detailed dependency attribution is recorded in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md), with additional notices in [`NOTICE`](NOTICE).

---

## Acknowledgments

[russh](https://github.com/warp-tech/russh) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [alacritty_terminal](https://github.com/alacritty/alacritty) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [wasmtime](https://wasmtime.dev/) · [tree-sitter](https://tree-sitter.github.io/)
