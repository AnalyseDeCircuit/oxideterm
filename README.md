<div align="center">

<img src="docs/media/oxideterm-native-hero.png" alt="OxideTerm: your servers, one workspace" width="920">

# ⚡ OxideTerm

**A free, native SSH client for working on remote servers.**

SSH · Mosh · Telnet · Serial · RDP/VNC · SFTP · port forwarding · code editor

No account. No subscription. No telemetry. No Electron.

[![Latest release](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue)](LICENSE)
[![Stars](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**Download**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**Documentation**](https://oxideterm.app) ·
[**Changelog**](.github/release-notes/stable-changelog.md) ·
[**Report an issue**](https://github.com/AnalyseDeCircuit/oxideterm/issues) ·
[**WeChat community**](https://github.com/AnalyseDeCircuit/oxideterm/issues/752)

[English](README.md) | [简体中文](docs/readme/README.zh-Hans.md) | [繁體中文](docs/readme/README.zh-Hant.md) | [日本語](docs/readme/README.ja.md) | [한국어](docs/readme/README.ko.md) | [Français](docs/readme/README.fr.md) | [Deutsch](docs/readme/README.de.md) | [Español](docs/readme/README.es.md) | [Italiano](docs/readme/README.it.md) | [Português](docs/readme/README.pt-BR.md) | [Tiếng Việt](docs/readme/README.vi.md)

</div>

---

## What is OxideTerm

OxideTerm is an SSH client for macOS, Windows and Linux. Each server you connect to gets its own workspace in one window: terminals, a file manager, port forwards, a code editor, host monitoring and an optional AI assistant. OxideTerm is free and open source under GPL-3.0.

## Why OxideTerm

- **Connect once, use everything.** Terminals, SFTP, port forwards and the editor for a server share one SSH connection by default, so opening another tool doesn't sign you in again.
- **Survives short network drops.** When a connection stops responding, OxideTerm keeps probing it for 30 seconds before replacing it, so vim, htop and other full-screen programs can pick up where they were.
- **Light on memory.** The interface is drawn on the GPU with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), the framework behind the Zed editor. At idle it uses about 81 MB on macOS and 24 MB on Windows.
- **Bring your own AI.** The OxideSens assistant runs on the provider and API key you choose. There are no OxideTerm credits to buy.
- **Pure-Rust SSH.** The SSH stack is built on russh and ring, without OpenSSL or libssh2.

<details>
<summary><b>Memory compared with OxideTerm 1.x</b></summary>

Idle memory before and after the move from Tauri (1.x) to native GPUI (2.0). The 1.x figures include its WebView processes.

| Platform | 1.x (Tauri) | 2.0 (native) | Reduction |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 74% |
| Windows | 182.4 MB | 23.5 MB | 87% |

![Idle memory of OxideTerm 1.x and 2.0 in the system process monitors](docs/screenshots/oxideterm-memory-comparison.png)

</details>

---

## Screenshots

### Terminal

An SSH session. The sidebar lists what you can open on this server: more terminals, SFTP, the IDE and port forwarding.

![SSH terminal with the server's tools in the sidebar](docs/screenshots/terminal/SSHTERMINAL.png)

### Mixed workspace

An SSH terminal, a local terminal, Markdown notes and SFTP together in one tab. Mix pages from different servers and your own machine in up to 16 panes.

![Four-pane workspace combining SSH, a local terminal, notes and SFTP](docs/screenshots/mixed-workspace.png)

### Files

Local and remote folders side by side, with the transfer queue below.

![Dual-pane SFTP file manager with a transfer queue](docs/screenshots/sftp/sftp.png)

### IDE

Open a remote project and edit its files in tabs.

![Remote project open in the built-in IDE](docs/screenshots/miniIDE/miniide.png)

### Port forwarding

OxideTerm lists the ports listening on the server. Forward any of them with one click, or use a quick forward for tools like Jupyter.

![Port forwarding page listing ports detected on the server](docs/screenshots/PORTFORWARD/PORTFORWARD.png)

<details>
<summary><b>Watch OxideSens open a terminal from a plain-language request</b></summary>

<a href="docs/media/ai-terminal-demo.mp4">
  <img src="docs/media/ai-terminal-demo.gif" alt="OxideSens opening a terminal inside OxideTerm" width="720">
</a>

</details>

---

## Features

Items marked *(plugin)* are installed from the Plugin Marketplace inside the app.

### Workspace

- Session Manager for searching, sorting, grouping, importing and exporting saved connections, with nested groups, tags, colors, icons and notes
- Import from `~/.ssh/config`, including `Match` blocks, `ProxyJump` chains and `RemoteCommand`, with imported hosts kept up to date
- Import from SecureCRT, Xshell, Termius, MobaXterm, WindTerm, Electerm and FinalShell, with a preview before anything is saved
- Tab scopes that show all tabs, global pages, local terminals or a single server, while hidden sessions keep running
- Up to 16 panes in one tab, mixing terminals, SFTP, the IDE, forwards, notes, settings and remote desktops
- Detached windows: move a tab into its own window and back without dropping its sessions
- Tile 2–16 open terminals into one grid without reconnecting them
- Command palette for actions and saved connections, and Zen mode for focusing on one page
- Saved local terminal profiles with their own shell, starting directory and icon
- Workspace, window size, full-screen state and display placement restored at launch

### Terminal

- Local shells detected automatically: Unix shells, Command Prompt, Windows PowerShell, PowerShell Core, Git Bash and Nushell
- Split panes with draggable dividers, where a new local split opens in the same working directory
- Search with regular expressions, case matching and multi-line queries, kept separately in every pane
- Shell integration for Bash, Zsh, Fish, Nushell and PowerShell that tracks working directories, exit codes and command marks, and installs on remote servers from the app
- Command folding that collapses finished output and keeps its exit status
- Command bar with multi-line input, history, path suggestions, completions and Quick Commands
- Fuzzy history suggestions, plus optional per-connection history that survives restarts
- Free Type Mode for editing the current command with the mouse
- Edit multi-line pastes before they're sent, with undo and code-fence removal
- Triggers that watch output for text or regex matches, then send input, run a Quick Command, launch a program or turn the match into a clickable action
- Semantic coloring for unstyled output such as errors, Git states, permissions, addresses and dates, plus keyword and regex highlights
- Clickable URLs and file paths, per-line timestamps with millisecond precision, and unread-output markers on tabs
- Sixel and Kitty graphics, including Kitty animations
- Native `tmux -CC` control mode with pane layouts and draggable dividers
- Sync groups that send your typing to several terminals, and a command sender for text or hex with pacing, intervals and repeats
- Quick Commands with groups, pinning, text, choice or secret parameters, and rules for which hosts they appear on
- Session logs with filename templates, size and retention limits and optional ANSI codes, plus `.cast` recordings with a built-in player
- Copy, cut and paste that act on Vim, Neovim or Emacs selections, locally and in supported SSH sessions
- Per-connection Backspace and Delete sequences and text encodings

### Connections

- SSH sign-in with passwords, private keys, OpenSSH certificates, managed keys, an SSH agent, keyboard-interactive prompts or Kerberos
- SSH agent discovery or a custom agent endpoint (`SSH_AUTH_SOCK` on macOS and Linux, `\\.\pipe\openssh-ssh-agent` on Windows), and agent forwarding
- Ed25519, RSA and ECDSA keys, ChaCha20-Poly1305 and AES-GCM ciphers, adjustable algorithm order, and a compatibility option for legacy servers
- TOTP codes generated and filled in when a matching verification prompt appears, on the server or on any jump host
- FIDO security keys (`ed25519-sk`, `ecdsa-sk`) with PIN and touch prompts *(plugin)*
- Jump-host chains with separate sign-in for each hop, HTTP CONNECT and SOCKS5 upstream proxies, and ProxyCommand
- Grace Period reconnect, which restores your forwards, resumes transfers and reopens editor files on the new connection
- One shared connection per server, or separate authentication for each new terminal
- X11 forwarding, trusted or untrusted
- Temporary connections without saving, and `ssh://`, `telnet://`, `mosh://`, `rdp://` and `vnc://` links that open in the app
- Mosh with adaptive local echo prediction *(plugin)*
- Telnet with option negotiation, proxy routing and configurable line endings
- Serial ports with baud rate, data bits, parity, stop bits and flow control, hex input and output, and Break, DTR and RTS controls
- Bluetooth LE terminals over Nordic UART Service or custom GATT characteristics

### Files and transfers

- Dual-pane local and remote file manager with favorites, path completion, and list or tree views
- Paired local and remote bookmarks, saved per connection
- Transfer queue with parallel transfers, speed limits, ETA, pause, resume and retry, and interrupted SFTP transfers that come back paused after a restart
- Overwrite, skip or rename on conflicts, for one file or all of them
- SFTP with automatic SCP fallback for hosts without an SFTP subsystem
- Standalone SFTP connections, and server-to-server transfers relayed through the app
- FTP and explicit FTPS
- Drop files on an SSH terminal to upload them to its current directory
- `trz`/`tsz` (trzsz) and `rz`/`sz` (ZMODEM) from the remote shell, and XMODEM, YMODEM and ZMODEM on serial connections
- Previews for text, code, Markdown, images, audio, video, hex and fonts
- PDF, SQLite, certificate and binary viewers *(plugin)*
- Jump from a terminal to the same directory in SFTP, and send an SFTP directory back to the terminal

### Editor and IDE

- Remote IDE over SFTP, plain shell commands, or a remote agent that adds symbol indexing, completion and file watching
- Convert an idle terminal into an IDE with the same user, container and directory
- Tabs, a project tree with multi-select, project search, and find and replace with regex and capture groups
- Files up to 100 MiB, with encoding, BOM and line endings preserved
- Safe saves with conflict handling, and optional autosave when you switch tabs
- Open files and unsaved edits restored after a restart
- Git views for branches, worktrees, staging, history, ahead/behind counts and merge or rebase states
- Project task detection and execution
- Syntax highlighting for 15 languages built in, and over 20 more as plugins *(plugin)*
- Markdown preview with tables, formulas and footnotes

### Port forwarding and networking

- Local, remote and dynamic (SOCKS5) forwarding with saved rules that can start with the connection
- Ports listening on the server detected and forwarded in one click
- Standalone SSH tunnels that run without a terminal, optionally from app startup
- Forwards restored after a reconnect, while rules you stopped stay stopped
- App-wide proxy settings, and the system's trusted certificates (including corporate CAs) for the app's network requests
- Connection topology view and ad-hoc socket debugging

### Remote desktop

- RDP and VNC with keyboard, mouse, clipboard, scaling and reconnect *(plugin)*
- RDP across several monitors, with network-quality profiles *(plugin)*
- VNC over VeNCrypt TLS *(plugin)*
- RDP and VNC through SSH jump hosts *(plugin)*
- WSLg apps in a built-in viewer (Windows)

### Host Tools

- Works on the local machine and on any SSH server
- CPU, memory, swap, disks, network, load and latency, with trend charts
- Processes (signals, stop and continue, nice), services (systemd, launchd, BSD and Windows), scheduled tasks and packages, including winget upgrades and uninstalls on Windows
- Logs with presets and follow mode, listening ports with exposure hints, and filesystems with low-space warnings
- Docker containers: status, ports, start, stop, restart and logs
- GPU and NPU monitoring for NVIDIA, AMD ROCm, Hygon DCU, Huawei Ascend, Intel XPU, Moore Threads MUSA and Cambricon
- tmux, Zellij and Herdr sessions: attach, create, split and rename
- Connection pool and connection matrix dashboards, and SSH latency in the terminal toolbar

### OxideSens AI

- OpenAI (including the Responses API), Anthropic, Gemini, DeepSeek, xAI Grok, Xiaomi MiMo, Ollama or any OpenAI-compatible endpoint, with thinking-effort controls
- Context from the terminals, files and settings you select, and tools that run commands, drive interactive programs, edit files and transfer data
- Read-only, default or approval-free modes, per-category tool permissions, command policy rules, and extra review for destructive or credential-related commands
- Image attachments, conversation branches, message editing and queued follow-ups
- Subagents that work in parallel, background tasks, and tasks that resume after an interruption
- Codex and Claude Code through ACP, plus launchers for OpenCode, Antigravity, Cursor, Grok Build, GitHub Copilot CLI, Qwen Code and Kimi CLI *(plugin)*
- MCP servers over stdio, Streamable HTTP or legacy SSE
- External MCP Control, which lets editors and other AI tools use your saved connections through OxideTerm, with permissions you grant per tool group and can revoke
- [Agent Skills](docs/agent-skills.md) (`SKILL.md`) with `/skill-name` completion
- A knowledge base of Markdown notes with BM25 keyword and HNSW vector search, CJK tokenization, and Mermaid diagrams in previews

### Security

- App Lock with Touch ID (macOS) or Windows Hello (Windows), while connections keep running
- Saved sudo and su passwords offered at the prompt, scoped to the local machine, one connection or a host pattern, with optional TOTP for two-step prompts
- Operation audit with filters, retention limits, and JSON or CSV export
- Encrypted session recordings with playback, seeking and speed control
- A permission review before any plugin is activated

### Sync and backup

- Cloud Sync over WebDAV, S3-compatible storage (including Alibaba Cloud OSS), a Git repository, GitHub Gist, Dropbox, OneDrive, Google Drive or HTTP JSON
- Pick what syncs, merge changes made on several devices, and resolve each conflict yourself
- Rollback backups, and recovery from an interrupted sync
- Encrypted `.oxide` bundles for connections, forwards, settings, Quick Commands and keys, with skip, rename, replace or merge on import
- Portable mode that runs from a folder with its own encrypted keystore
- In-app updates on the Stable or Beta channel

### Personalization

- Light, dark or system appearance with separate app and terminal themes, and a theme editor
- Background images, animations and videos, with generated effects such as fog, particles, meteors and water caustics, plus extra media packs *(plugin)*
- Window opacity and corner styles
- Font stacks with separate CJK fonts, ligatures and weights, and separate editor typography
- Interface scaling, cursor style, padding and motion settings
- Custom keyboard shortcuts, and searchable settings you can rearrange
- 11 interface languages, IME input and bidirectional text

### Plugins

| Type | How it runs | What it can reach |
|---|---|---|
| **Manifest** | Declarative extension | Nothing; no code runs |
| **WASM** | Wasmtime/WASI or a sidecar process | Only the host capabilities it is granted |
| **Process** | An ordinary local process | Anything a local program can; the OS does not sandbox it |

Install process plugins only from sources you trust. Plugins made for OxideTerm 1.x may still appear in the list, but 2.x doesn't run them.

### Command-line tool

Install the `oxideterm` command from **Settings → General → CLI Companion**. It shares the app's configuration, so you can check, script and back up your setup without opening the window, or open a new tab from any shell. Commands that change something show a plan with `--dry-run`, apply only with `--yes`, and keep a rollback backup.

```bash
oxideterm terminal --cwd ~/project
oxideterm doctor --strict
oxideterm connections search prod
oxideterm forwards list --format json
oxideterm oxide export ./profile.oxide --connection prod --password-stdin
```

Shell completions are available for Bash, Zsh, Fish, PowerShell and Elvish. The [CLI guide](docs/user-guide/en/cli.md) lists every command.

---

## Install

Download OxideTerm from the [latest release](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest).

| OS | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG (Intel) | DMG (Apple Silicon) |
| **Windows** | Installer (`.exe`) | Installer (`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

The release page also has portable archives, signatures and a `sha256sums.txt` file for checking your download.

### macOS

If Gatekeeper blocks the app, remove the quarantine flag:

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

If SmartScreen shows a warning, choose **More info → Run anyway**.

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

### If the window doesn't draw

Set `OXIDETERM_RENDER_PROFILE=compatibility` in the environment and start OxideTerm again.

### First steps

1. Open the **Session Manager** and add a server, or import hosts from `~/.ssh/config`.
2. Connect to open a terminal.
3. From the same server, open SFTP, port forwarding or the IDE.
4. To use OxideSens, add an AI provider in **Settings**.

The [documentation](https://oxideterm.app) covers the rest of the app. To build OxideTerm yourself, see [Build from source](#build-from-source).

---

## Privacy and security

- Saved passwords and keys are stored in your system keychain (macOS Keychain, Windows Credential Manager or libsecret on Linux). AI provider keys are kept out of the app's logs.
- Secrets held in memory are zeroized when they're dropped.
- Host keys are trusted on first use and checked against `~/.ssh/known_hosts`. A changed key is rejected.
- Text sent to an AI provider passes through credential redaction first.
- Session recording is off by default. Recordings are encrypted on your device, left out of Cloud Sync, and don't capture what you type.
- Notifications & Audit is off by default. When you turn it on, its data stays on your device with sensitive details encrypted, and it covers only what happens inside OxideTerm.
- `.oxide` bundles are encrypted with ChaCha20-Poly1305, using a key derived with Argon2id (256 MB of memory, 4 iterations).

---

## Build from source

You need Rust (rustup installs the toolchain pinned in `rust-toolchain.toml`) and a desktop environment that can run GPUI.

```bash
# Run the app
cargo run

# Build the oxideterm CLI
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

With Nix, use `nix build .#oxideterm`, `nix run .#oxideterm` or `nix develop`. The CLI build is staged at `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`.

### How it fits together

The interface, terminals and SSH run in one Rust process. The optional remote agent and platform helpers run as separate processes. Terminal output goes from the SSH channel or local PTY straight into `TerminalState`, which GPUI draws, with no WebView, JSON bridge or xterm.js in between.

`NodeRouter` hands out connections from `SshConnectionRegistry`, a DashMap-backed pool, so terminal panes, SFTP, forwards and the editor share one SSH connection per server unless a terminal is set to use its own. Each connection moves through `connecting → active → idle → link_down → reconnecting`. When a jump host fails, the servers behind it go to `link_down`. Each forwarding rule runs its own listener. After a reconnect, SFTP sessions are reacquired on the new connection, and an operation started on the old connection is never moved over silently.

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

Read next: the [development guide](docs/development/README.md), the [crate map](docs/development/crate-map.md) and the [architecture overview](docs/user-guide/en/architecture.md).

---

## Contributing

Code, documentation, translations, plugins, testing and bug reproductions are all welcome. For a larger change, open an issue first. [CONTRIBUTING.md](docs/CONTRIBUTING.md) explains how to prepare a pull request.

Bug reports are easiest to fix with a redacted diagnostic bundle:

```bash
oxideterm report --bundle ./oxideterm-report.zip
```

From a source checkout, run `cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip` instead.

### Contributors

Thanks to everyone who helps make OxideTerm better.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="OxideTerm contributors">
  </a>
</p>

---

## Credits

OxideTerm is built on these open source projects:

- [russh](https://github.com/warp-tech/russh) and [ring](https://github.com/briansmith/ring): SSH protocol and cryptography
- [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui): GPU-accelerated UI framework from the Zed editor
- [alacritty_terminal](https://github.com/alacritty/alacritty): terminal emulation (VT100–VT500)
- [portable-pty](https://github.com/wez/wezterm/tree/main/pty): local shells, using ConPTY on Windows
- [Tokio](https://tokio.rs): async runtime
- [Wasmtime](https://wasmtime.dev/): runs WASM plugins
- [tree-sitter](https://tree-sitter.github.io/): syntax highlighting in the editor

---

## License

OxideTerm is licensed under [GPL-3.0-only](LICENSE), with no additional restrictions. Dependency notices are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and [NOTICE](NOTICE).

Use OxideTerm only on systems you own or are authorized to access. [LEGAL.md](LEGAL.md) has the full legal notice.

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
