<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>AI-native workspace for remote servers.</strong>
  <br>
  Connect to your servers over SSH, then work with terminals, files, ports, transfers, lightweight editing, serial consoles, and OxideSens AI in one local-first native app.
  <br>
  Native GPUI app · Pure Rust SSH · BYOK OxideSens AI · No account required for core SSH workflows
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust — all the way down.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.5-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Next major native edition of <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — GPU-rendered, zero-WebView, using <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (Zed's rendering framework)</sub>
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="docs/readme/README.zh-Hans.md">简体中文</a> | <a href="docs/readme/README.zh-Hant.md">繁體中文</a> | <a href="docs/readme/README.ja.md">日本語</a> | <a href="docs/readme/README.ko.md">한국어</a> | <a href="docs/readme/README.fr.md">Français</a> | <a href="docs/readme/README.de.md">Deutsch</a> | <a href="docs/readme/README.es.md">Español</a> | <a href="docs/readme/README.it.md">Italiano</a> | <a href="docs/readme/README.pt-BR.md">Português</a> | <a href="docs/readme/README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

<a href="docs/media/ai-terminal-demo.mp4">
  <img src="docs/media/ai-terminal-demo.gif" alt="OxideSens opening a terminal inside OxideTerm" width="920">
</a>

*Watch OxideSens follow a user request and open a terminal inside OxideTerm.*

</div>

---

## What You Can Do

- Manage SSH terminals, SFTP, port forwards, serial consoles, local shells, and lightweight editing in one native workspace
- Keep remote work alive through network hiccups with Grace Period reconnect
- Ask OxideSens AI to inspect live sessions and perform approved workspace actions through your own AI provider

---

## Why OxideTerm Native?

| If you care about... | OxideTerm Native gives you... |
|---|---|
| One remote node, many tools | Terminal, SFTP, port forwarding, trzsz, native IDE, monitoring, and OxideSens AI stay attached to the same SSH workspace |
| Zero WebView native shell | GPUI draws the desktop UI directly on a GPU surface — no DOM, CSS, JavaScript, Chromium, or WebKit runtime |
| Local-first SSH workflows | SSH, SFTP, forwarding, local shell, serial terminals, and config work without signup |
| BYOK OxideSens AI instead of platform credits | OxideSens uses your OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible endpoint with MCP, RAG, and approved workspace actions |
| Reconnect stability | Grace Period probes the old connection for 30s before replacing it, so TUI apps can survive short network drops |
| Pure Rust SSH and credential safety | `russh` + `ring`, no OpenSSL/libssh2; passwords and API keys stay in OS keychain, and `.oxide` bundles use ChaCha20-Poly1305 + Argon2id |

## What It Is / Is Not

OxideTerm Native focuses on a **local-first AI workspace for remote servers**, rebuilt as a pure Rust GPUI desktop app. It is for users who want terminals, files, ports, transfers, lightweight editing, serial consoles, and OxideSens AI centered around their own machines and remote nodes.

It is not the current stable download line yet, and it is not a hosted cloud agent platform. It is also not an Electron app, a Tauri app, or a web-based terminal: no Chromium, no WebView, no JavaScript, no CSS.

---

## Screenshots

The native UI follows the same OxideTerm workspace model and visual language as the current Tauri line.

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

## What's Different

Most SSH workspace tools in this space ship a browser runtime inside a desktop wrapper. Terminal bytes flow through JavaScript, reconnect logic lives in the frontend, and the "native" Rust backend spends most of its time serializing events for the WebView. The result is 150–200 MB installs, 300+ MB idle RAM, and a WebView2 dependency on Windows just to open a terminal tab.

OxideTerm started there too (the Tauri version). The native branch removes the browser entirely:

| Aspect | WebView-based (incl. Tauri) | Native (this branch) |
|---|---|---|
| **Rendering** | Chromium/Safari/WebKit2GTK + CSS layout | GPUI — GPU surface, immediate mode, pure Rust |
| **Terminal data flow** | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` mutation → GPUI render |
| **IPC overhead** | JSON-RPC serialization on every command | In-process function calls |
| **SSH keepalive** | JavaScript timer, throttled by browser | Rust async task |
| **Reconnect** | Orchestrated across the WS bridge | Single in-process pipeline |
| **AI context** | Serialized through IPC into a handler | Built directly from in-process workspace state |
| **Plugin runtime** | ESM in browser sandbox | WASM in wasmtime with typed Rust host API |
| **CLI** | Requires the desktop app running | Standalone binary, direct crate linkage |
| **Release artifact size** | Usually ~150–200 MB installers | Current macOS arm64: ~50–60 MB compressed portable/DMG; raw release binary is ~132 MB |

---

## Feature Overview

| Category | Features |
|---|---|
| **Terminal** | Local PTY (zsh/bash/fish/pwsh/WSL2), SSH remote, local serial terminals, split panes, shell integration, command marks, recording/playback (asciicast v2), trzsz in-band file transfer, Sixel/Kitty graphics, rendering policy (Boost/Normal/Idle) |
| **SSH & Auth** | Connection pool, multi-hop ProxyJump (unlimited hops), Grace Period reconnect, host-key TOFU, SSH Agent forwarding. Auth: password, public key (RSA/Ed25519/ECDSA), SSH Agent, certificate, keyboard-interactive 2FA |
| **SFTP** | Dual-pane browser, transfer queue (concurrent, speed-limited), adaptive chunking, progress + ETA, text/binary/image/archive preview, bookmarks, atomic writes |
| **IDE** | SFTP-backed remote file tree, multi-tab editor, dirty tracking, conflict resolution, snapshot/restore, local + remote filesystem abstraction |
| **Port Forwarding** | Local (-L), Remote (-R), Dynamic SOCKS5 (-D), saved rules, reconnect-aware restore, death reporting, idle timeout, remote port detection |
| **AI (OxideSens)** | OpenAI, Anthropic, Gemini, Ollama/OpenAI-compatible; MCP (stdio + SSE); RAG with BM25 + HNSW vector index, CJK bigram tokenizer; chat history, tool policy, command approval |
| **Cloud Sync** | Push/pull/apply/resolve, S3/WebDAV/Git backends, structured manifest, conflict strategies, rollback backups, redacted diagnostics |
| **Portable `.oxide`** | Encrypted export/import (ChaCha20-Poly1305 + Argon2id), connections, forwards, settings, quick commands, plugin settings, portable secrets |
| **Plugins** | Manifest, protocol, registry, WASM sandbox (wasmtime), native host API, per-plugin settings, enable/disable, custom tab surfaces |
| **CLI** | 17 top-level commands — settings, connections, forwards, quick-commands, plugins, portable, secrets, oxide, cloud-sync, paths, diagnose, doctor, backup, batch, report, completion, errors |
| **i18n** | Native i18n loader with source-product parity checks |

---

## Under the Hood

OxideTerm Native removes the WebView bridge and keeps terminal, SSH, SFTP, forwarding, IDE, AI, plugins, and CLI in one Rust-native architecture. The full implementation notes are preserved below for readers who want the engineering details.

<details>
<summary><strong>Architecture, SSH internals, GPUI shell, reconnect, AI, plugins, and more</strong></summary>
<br>

### Architecture — Single-Process, Zero-Bridge

The Tauri version separates terminal data from control commands into two planes bridged by WebSocket and JSON-RPC. The native version collapses both planes into a single Rust process:

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

Same russh stack as the Tauri version, now linked directly into the desktop app binary:

- **Zero C/OpenSSL dependencies** — full crypto in Rust via `ring`
- Full SSH2: key exchange, channels, SFTP subsystem, port forwarding
- ChaCha20-Poly1305 and AES-GCM, Ed25519/RSA/ECDSA keys
- SSH Agent: Unix (`SSH_AUTH_SOCK`) and Windows (`\\.\pipe\openssh-ssh-agent`)
- Custom `AgentSigner` for russh `Signer` trait compatibility across `.await` bounds
- Multi-hop proxy chains with per-hop independent auth

### 🔄 Smart Reconnect with Grace Period

Identical reconnect semantics to the Tauri version, reimplemented entirely in Rust without a JavaScript orchestrator:

1. **Detect** SSH keepalive timeout (Rust async task, no JS timer throttling)
2. **Snapshot** terminal panes, SFTP transfers, forwards, IDE files — all in-process
3. **Grace Period** (30 s): probe old SSH connection via keepalive; TUI apps survive on network switch
4. New SSH connection → restore forwards → resume transfers → reopen IDE files

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### 🛡️ SSH Connection Pool

`SshConnectionRegistry` backed by `DashMap` — same architecture as Tauri, without the WebSocket lifecycle bridge:

- **One connection, many consumers**: terminal panes, SFTP, port forwards, and IDE share one physical SSH connection
- **State machine per connection**: `connecting → active → idle → link_down → reconnecting`
- **Node-first addressing**: everything is resolved by `nodeId` → `connectionId` by `NodeRouter`
- **NodeRuntimeStore**: serializable snapshot of all nodes, persisted to `session_tree.json` on every topology change, restored on startup
- **Cascade propagation**: jump host failure → downstream nodes automatically marked `link_down`

### 🤖 OxideSens AI

Same BYOK-first AI as Tauri, with all context building done in-process:

- **Providers**: OpenAI, Anthropic (Claude), Google Gemini, Ollama/any OpenAI-compatible endpoint
- **MCP**: stdio + SSE transports, full tool discovery and invocation
- **RAG**: BM25 full-text + HNSW vector index, Reciprocal Rank Fusion, CJK bigram tokenizer
- **Context boundary**: AI context is built from in-process workspace state; credentials are redacted before any provider call
- **API keys**: stored in OS keychain; never logged or serialized into IPC frames (there are no IPC frames)

### 🎨 GPUI Desktop Shell

The entire UI is written in Rust using GPUI (Zed's GPU-backed UI framework):

- **No CSS, no DOM, no JavaScript** in the rendering pipeline
- **17 workspace tab types**: `LocalTerminal`, `SshTerminal`, `Sftp`, `Ide`, `Forwards`, `SessionManager`, `CloudSync`, `Settings`, `PluginManager`, `Topology`, `ConnectionPool`, `ConnectionMonitor`, `NotificationCenter`, `FileManager`, `Launcher`, `Graphics`, custom `Plugin` tabs
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

The native branch keeps extension and support surfaces in Rust-native boundaries:

- Plugins run in a wasmtime sandbox with typed host capabilities instead of browser globals
- The CLI links directly to domain crates for doctor, settings, connections, forwards, portable bundles, backups, and reports
- Diagnostics prefer counts, paths, feature flags, and redacted hints over raw secret-bearing payloads
- Mutating CLI flows use dry-run plans, `--yes` guards, and rollback backups where applicable

### 🔀 Port Forwarding — Lock-Free I/O

Identical semantics to Tauri, implemented as standalone Rust crate:

- Local (-L), Remote (-R), Dynamic SOCKS5 (-D)
- Message-passing architecture: SSH Channel owned by single `ssh_io` task — no `Arc<Mutex<Channel>>`
- Auto-restore on reconnect, death reporting, idle timeout

### 📦 trzsz — In-Band File Transfer

Same in-band protocol as Tauri, integrated directly with native file dialogs:

- Upload/download through the existing terminal stream — no extra ports or agents
- Works through ProxyJump chains
- Native file pickers (no browser memory constraints)
- Bidirectional, directory support, configurable limits

### 🔐 .oxide Encrypted Export

Same cryptography as Tauri:

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
./scripts/build-cli.sh

# Build the optional Linux remote agent
./scripts/build-agent.sh
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
| **SSH** | russh (`ring`) | Pure Rust, zero C deps, SSH Agent |
| **Local PTY** | portable-pty | Feature-gated, ConPTY on Windows |
| **Terminal emulation** | alacritty_terminal | VT100–VT500, Sixel, Kitty graphics |
| **Editor** | tree-sitter (syntax), custom buffer | Multi-language, SFTP-backed |
| **Encryption** | ChaCha20-Poly1305 + Argon2id | AEAD + memory-hard KDF (256 MB) |
| **Plugin sandbox** | wasmtime | WASM isolation with native host API |
| **AI streaming** | SSE (OpenAI/Anthropic/Gemini) | In-process, no IPC boundary |
| **RAG** | BM25 + HNSW vector index | CJK bigram tokenizer, RRF fusion |
| **i18n** | oxideterm-i18n (custom) | Native loader, parity checks |

---

## Development

```sh
cargo check --workspace
cargo check -p oxideterm-gpui-app
cargo test --workspace
cargo test -p oxideterm-cli -- --test-threads=1
cargo fmt --all --check
```

Prefer scoped crate checks while iterating. Broaden to `--workspace` when a change crosses crate boundaries.

---

## Security

| Concern | Implementation |
|---|---|
| **Passwords & keys** | OS keychain (macOS Keychain / Windows Credential Manager / libsecret) |
| **Secret memory** | `zeroize` / `Zeroizing` on all owned sensitive Rust values |
| **Diagnostics** | Paths, counts, flags, hints only — never raw secret values |
| **AI context** | Credentials, keys, and terminal buffers redacted before any provider call |
| **`.oxide` export** | ChaCha20-Poly1305 + Argon2id (256 MB memory, 4 iterations) |
| **CLI writes** | Dry-run plans, `--yes` guards, rollback backups for state-changing commands |
| **Host keys** | TOFU with `~/.ssh/known_hosts`, rejects unexpected changes |
| **Plugin sandbox** | WASM isolation via wasmtime, capability-based host API |

---

## Release Status

- [x] SSH Agent forwarding
- [x] Grace Period reconnect
- [x] GPUI desktop shell
- [x] In-process terminal data flow (no WebSocket)
- [x] SFTP, port forwarding, IDE, AI, cloud sync, plugins, CLI
- [x] Local serial terminals
- [x] Full ProxyCommand support
- [ ] Audit logging

---

## Provider Neutrality

OxideTerm is BYOK-first and provider-neutral.

Provider integrations exist to help users connect the tools they already trust. They are not a leaderboard, a billboard, or a reward system for whoever asks most warmly.

Compatibility, maintainability, security, and real user value decide what gets documented. Visibility follows usefulness, not enthusiasm.

---

## Contributing

When a feature already exists in the Tauri version, keep native behavior, labels, interaction states, and workflows aligned with that product unless a deliberate replacement is documented. Parity notes live in `docs/`.

New crates must own a real domain responsibility — not just re-export modules. Split by capability: DTOs, validation, persistence, view models, protocol adapters, and presentational builders belong in the crate that owns that job.

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

**GPL-3.0-only**. Third-party notices and dependency attribution are recorded in `NOTICE`.

---

## Acknowledgments

[russh](https://github.com/warp-tech/russh) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [alacritty_terminal](https://github.com/alacritty/alacritty) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [wasmtime](https://wasmtime.dev/) · [tree-sitter](https://tree-sitter.github.io/)
