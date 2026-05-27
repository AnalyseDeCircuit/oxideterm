# OxideTerm Native

<p align="center">
  <strong>Local-first SSH workspace: terminal, SFTP, port forwarding, lightweight editing, cloud sync, portable exports, and BYOK AI around one remote node.</strong>
  <br>
  <strong>Rust-native desktop app. GPUI shell. Pure Rust SSH. No Webview. No telemetry. BYOK-first.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

OxideTerm Native is the Rust/GPUI desktop implementation of OxideTerm. It is
built for users who want SSH, local shells, remote files, tunnels, diagnostics,
portable configuration, and AI assistance in one local-first workspace instead
of a collection of unrelated terminal tabs and helper tools.

The native app keeps OxideTerm's core product model: a remote host is treated as
a workspace node. Terminals, SFTP, forwards, reconnect state, quick commands,
settings, sync state, and AI context attach to that node rather than drifting
across separate tools.

## Why OxideTerm?

| If you care about... | OxideTerm Native gives you... |
|---|---|
| SSH as a workspace | One node can own terminal panes, SFTP browsing, forwards, reconnect state, previews, diagnostics, and AI context. |
| Local and remote together | Local PTY sessions and SSH sessions live in the same workspace model. |
| BYOK AI | Configure OpenAI-compatible providers, local Ollama-style endpoints, reasoning settings, MCP, and knowledge context without an OxideTerm account. |
| Remote file work | Browse, preview, transfer, and edit remote files over the same SSH connection model. |
| Port forwarding | Manage local, remote, and dynamic forwards with persisted rules and reconnect-aware restore behavior. |
| Portable state | Export encrypted `.oxide` bundles with connections, forwards, settings, plugin settings, quick commands, and selected portable secrets. |
| Scriptable administration | Use the standalone `oxideterm` CLI for settings, connections, backups, cloud sync, reports, secrets, and imports. |
| Local-first security | Store secrets in the OS keychain or encrypted portable payloads; diagnostics and AI context must redact secret values. |

## What It Is / Is Not

OxideTerm Native is a desktop SSH workspace and management surface. It is meant
to keep terminal work, remote files, forwarding, configuration, and support
diagnostics close to the machine where you work.

It is not a hosted terminal service, a cloud AI platform, a browser extension, or
a subscription workflow. Cloud sync is opt-in and controlled by your own backend
configuration.

## Feature Overview

| Area | Current native scope |
|---|---|
| Terminal | Local shell sessions, SSH sessions, tabs, split-oriented workspace state, terminal search, rendering policy, recording support, graphics, and command helpers. |
| SSH lifecycle | Saved connections, host-key decisions, jump-host/topology work, reconnect state, connection monitoring, and shared node ownership. |
| SFTP and files | Remote browsing, transfer state, previews, local-file surfaces, remote editing, and optional Linux node-agent flows. |
| Port forwarding | Saved forwards, active forwarding runtime, validation, export/import, CLI CRUD, and reconnect-aware operation state. |
| AI workspace | Provider settings, provider keys, reasoning and context controls, MCP, knowledge/RAG, tool policy, chat state, and redacted support paths. |
| Settings | Native settings pages, persisted settings model, import/export, validation, profile/config-dir isolation, and settings view-model crates. |
| Cloud sync | State, preview, history, backups, backend configuration, push/pull/apply/resolve commands, secret hints, and redacted diagnostics. |
| Portable `.oxide` | Validate, preview, import, export, conflict strategies, app settings, connection data, forwards, quick commands, plugin settings, and portable secrets. |
| Plugins | Manifest, protocol, registry, host API boundary, settings management, enable/disable state, and native plugin host work. |
| CLI | Headless management for settings, connections, forwards, quick commands, plugins, secrets, cloud sync, backups, reports, errors, and shell completions. |
| i18n | Native i18n loader and translated UI surfaces, with source-product checks for labels and fallback text during UI work. |

## Quick Start

### Requirements

- Rust toolchain with Edition 2024 support.
- macOS, Windows, or Linux desktop environment capable of running GPUI.
- Platform build tools:
  - macOS: Xcode Command Line Tools.
  - Windows: Visual Studio C++ Build Tools.
  - Linux: standard build toolchain plus desktop/media libraries required by the
    GPUI and preview stack.

### Run the App

The workspace default member is the GPUI app:

```sh
cargo run
```

You can also call the app binary explicitly:

```sh
cargo run -p oxideterm-gpui-app --bin oxideterm-native
```

If the renderer cannot open a window on a specific machine, retry with the
compatibility render profile:

```sh
OXIDETERM_RENDER_PROFILE=compatibility cargo run -p oxideterm-gpui-app --bin oxideterm-native
```

### Build the CLI Companion

The CLI is packaged as a separate binary. Normal app development should not
compile the CLI on every `cargo run`.

```sh
./scripts/build-cli.sh
./scripts/build-cli.sh aarch64-apple-darwin
```

Packaged CLI artifacts are staged under:

```text
crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm
```

The native settings UI can inspect, install, uninstall, and refresh that bundled
CLI. Unix installs use a symlink in `~/.local/bin`; Windows installs copy the
binary into the user-local OxideTerm bin directory.

### Build the Remote Agent

The optional Linux node-agent is built separately from the desktop app:

```sh
./scripts/build-agent.sh
```

The script stages artifacts under `crates/oxideterm-gpui-app/resources/agents`
for bundling with the native app.

## Headless CLI

Use `oxideterm-cli` when you need automation, CI checks, support diagnostics, or
configuration changes without launching the GPUI app.

```sh
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- forwards list --format json
cargo run -p oxideterm-cli -- quick-commands list --format table
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- backup restore ./backup.json --section settings --dry-run --json
cargo run -p oxideterm-cli -- oxide export ./profile.oxide --connection prod --password-stdin
cargo run -p oxideterm-cli -- completion install zsh --force
```

Global path controls are available for automation:

```sh
cargo run -p oxideterm-cli -- --config-dir ./fixture-config --profile ci doctor --strict
OXIDETERM_CONFIG_DIR=./fixture-config cargo run -p oxideterm-cli -- report --format json
```

Write commands default toward dry-run or explicit confirmation where state could
be changed. Machine-readable errors are available through:

```sh
cargo run -p oxideterm-cli -- errors --json
```

## Under the Hood

### Node-First Workspace Model

The native app models a remote target as a node. Terminal panes, SFTP surfaces,
forwards, IDE state, transfer state, reconnect snapshots, and diagnostics should
resolve through node ownership instead of assuming that a terminal tab is the
owner of workspace state.

### Pure Rust SSH

OxideTerm uses the workspace `russh` stack for SSH behavior. The native codebase
keeps SSH transport, SFTP, forwarding, reconnect, known-host decisions, and
connection monitoring in dedicated crates rather than burying that logic inside
the GPUI app shell.

### GPUI Desktop Shell

The native shell is written in Rust with GPUI. Shared controls, typography,
overlays, text inputs, buttons, settings presentation helpers, cloud-sync view
models, markdown rendering, terminal UI, editor UI, and IDE surfaces are split
into focused crates so product features are not trapped inside one app file.

### Portable Configuration

Portable `.oxide` bundles are encrypted, authenticated exports for moving
workspace state between machines. Native import/export paths cover saved
connections, forwards, quick commands, selected app settings, plugin settings,
and portable secrets where supported.

### Cloud Sync

Cloud sync is implemented as reusable sync state and operation crates plus GPUI
presentation adapters. The CLI can preview, configure, push, pull, apply,
resolve, inspect history, manage secret hints, and produce redacted support
reports without duplicating the sync engine inside command handlers.

### AI and Knowledge

OxideSens-style AI features are local configuration surfaces around your chosen
providers. Provider keys and tool context must be treated as sensitive data:
secrets are stored through keychain-backed flows, and content sent to providers
must be redacted before crossing the AI boundary.

## Architecture

The workspace is split by responsibility:

| Crate or path | Responsibility |
|---|---|
| `crates/oxideterm-gpui-app` | App entry point, GPUI workspace shell, dialogs, window actions, and UI event bridges. |
| `crates/oxideterm-gpui-ui` | Shared native UI primitives, tokens, controls, overlays, and typography. |
| `crates/oxideterm-gpui-terminal` / `crates/oxideterm-terminal*` | Terminal UI, parser/encoding/unicode/graphics support, recording, and terminal data flow. |
| `crates/oxideterm-ssh` | SSH transport, node IDs, connection ownership, reconnect data, and host lifecycle primitives. |
| `crates/oxideterm-sftp` | SFTP models, transfer state, preview integration, and remote file operations. |
| `crates/oxideterm-forwarding` | Saved forwards, runtime forwarding operations, validation, and persisted forward records. |
| `crates/oxideterm-connections` | Saved connections, groups, SSH config import, `.oxide` connection payloads, and connection validation. |
| `crates/oxideterm-quick-commands` | Quick command persistence, import/export, and CLI-facing snapshots. |
| `crates/oxideterm-settings*` | Persisted settings, settings model, validation, settings snapshots, and view-model ownership. |
| `crates/oxideterm-cloud-sync*` | Cloud-sync state, operation service, preview models, selection, and GPUI presentation adapters. |
| `crates/oxideterm-ai` | AI providers, key storage, MCP, RAG/Knowledge, tool policy, chat state, and context handling. |
| `crates/oxideterm-plugin-*` | Plugin manifest, protocol, registry, host API types, settings, and runtime boundary work. |
| `crates/oxideterm-cli` | Headless management CLI with path/profile isolation, write guards, reports, backups, and shell completion. |
| `agent/` | Optional Linux node-agent source used by remote-resource and IDE flows. |

## Development

Run focused checks while iterating, then broaden when the change crosses crate
boundaries.

```sh
cargo fmt --all --check
cargo check -p oxideterm-gpui-app --tests
cargo check -p oxideterm-cli --tests
cargo test -p oxideterm-cli -- --test-threads=1
```

Useful crate-level checks:

```sh
cargo test -p oxideterm-settings
cargo test -p oxideterm-connections
cargo check -p oxideterm-forwarding --no-default-features
cargo check -p oxideterm-forwarding --features runtime
```

Avoid running `cargo fmt` alone as a foreground verification step. Prefer
`cargo fmt --all --check`, or run formatting together with the next check when
you intentionally apply formatting changes.

## Repository Layout

```text
.
├── agent/                 # Optional remote Linux node-agent source
├── crates/                # Rust workspace crates
├── docs/                  # Native plans, invariants, product notes, and references
├── scripts/               # Build and verification helpers
├── tasks/                 # Local task notes and lessons
├── Cargo.toml             # Workspace definition
└── README.md
```

## Security

| Concern | Native rule |
|---|---|
| Passwords and keys | Store through OS keychain or encrypted portable payloads where applicable. |
| Secret memory | Use `zeroize` / `Zeroizing` for owned sensitive Rust values. |
| Diagnostics | Print paths, counts, flags, hashes, and hints; never print secret values. |
| AI context | Redact sensitive content before sending context to any provider. |
| `.oxide` exports | Use authenticated encryption and explicit password handling. |
| CLI writes | Use dry-run plans, `--yes`, rollback backups, or write guards for state-changing commands. |

## Contributor Notes

When a feature already exists in the older OxideTerm app, keep the native
behavior, labels, interaction states, and user-visible workflow aligned with that
product unless a deliberate native replacement is documented. Detailed source
maps and porting notes belong in `docs/`; this README should stay focused on the
product, setup, architecture, and contribution entry points.

## Build Philosophy

New crates must own real responsibilities. Do not create a crate that only holds
a large `lib.rs`, re-exports moved modules, or serves as a line-count hiding
place. Split by domain capability: protocol DTOs, validation, persistence,
settings view models, cloud-sync operations, host API dispatch, registry parsing,
and presentational builders belong in the crate that owns that job.

The GPUI app crate should stay focused on context sampling, dialogs, window
behavior, GPUI actions, and UI event bridges.

## Support and Maintenance

OxideTerm is maintained on a best-effort basis. Reproducible bug reports,
security-sensitive issues, UI or behavior gaps, translation fixes, and focused
pull requests are the most useful contributions.

When reporting issues, prefer a redacted CLI report:

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

## License

OxideTerm is licensed under `GPL-3.0-only`. Third-party notices and dependency
attribution are recorded in `NOTICE`.
