# OxideTerm

OxideTerm is a Rust-native terminal and remote workspace application built with
GPUI. The native app is the active implementation in this repository; the Tauri
code is kept as a behavioral and visual source of truth while the Rust/GPUI
version is migrated feature by feature.

The project goal is not to redesign OxideTerm. Native code should preserve the
Tauri product semantics, UI hierarchy, spacing, labels, state transitions, and
runtime behavior unless a documented native constraint requires a different
implementation boundary.

## Current Native Scope

- Local terminal panes backed by `alacritty_terminal`, with GPUI rendering,
  split panes, tabs, search, shell integration, and terminal graphics plumbing.
- SSH connection management, saved targets, host-key handling, reconnect
  orchestration, node/session ownership, and terminal endpoint routing.
- SFTP browsing and transfer management, including progress storage, resumable
  transfers, directory transfer strategies, and conflict handling paths.
- Port forwarding primitives for local, remote, and dynamic forwarding, plus
  saved-forward storage and runtime events.
- Native settings surfaces translated from the Tauri settings UI, with persisted
  Rust settings models and 11-locale i18n catalogs.
- AI chat infrastructure with provider settings, model context-window handling,
  memory settings, reasoning-effort settings, tool policy, high-level
  orchestrator tools, MCP server integration, and Knowledge/RAG storage.
- Editor, IDE, preview, launcher, local file, topology, notification, and
  workspace surfaces that are being moved into native GPUI modules.

This is still an active migration branch. Treat the parity maps under `docs/` as
the working contract for what is implemented, what is intentionally different,
and what still needs verification.

## Repository Layout

- `crates/oxideterm-gpui-app` - native GPUI application entry point and workspace
  shell. The app binary is `oxideterm-native`.
- `crates/oxideterm-gpui-ui` - shared native UI primitives, tokens, overlays,
  form controls, and reusable visual building blocks.
- `crates/oxideterm-gpui-terminal` and `crates/oxideterm-terminal` - terminal UI,
  PTY/session ownership, shell integration, search, graphics, and terminal data
  flow.
- `crates/oxideterm-ssh`, `crates/oxideterm-sftp`, and
  `crates/oxideterm-forwarding` - remote connection, file transfer, reconnect,
  and forwarding backends.
- `crates/oxideterm-ai` - provider adapters, chat state, tool definitions and
  policy, MCP runtime, Knowledge/RAG store, embeddings, memory, and context
  handling.
- `crates/oxideterm-settings` and `crates/oxideterm-i18n` - persisted settings
  schema, migrations, sanitization, and locale catalogs.
- `agent/` - remote node-agent source used by native remote-resource execution.
- `docs/` - source maps, parity plans, and system invariants for native
  migration work.
- `tauri版本代码/` - reference Tauri implementation used for parity auditing.

## Run

Use a Rust toolchain with Edition 2024 support.

```sh
cargo run -p oxideterm-gpui-app --bin oxideterm-native
```

If the GPU-backed GPUI renderer cannot open a window on a machine, retry with
the compatibility render profile:

```sh
OXIDETERM_RENDER_PROFILE=compatibility cargo run -p oxideterm-gpui-app --bin oxideterm-native
```

Remote node-agent artifacts can be built with:

```sh
./scripts/build-agent.sh
```

That script expects the Linux musl targets used by the agent release artifacts to
be installed in the active Rust toolchain.

## Validate

Common checks for native app work:

```sh
cargo fmt --check
cargo check -p oxideterm-gpui-app
cargo test -p oxideterm-ai
cargo test -p oxideterm-settings
```

Backend changes should also run the relevant package tests. For example, SSH,
SFTP, forwarding, terminal rendering, settings, AI, and i18n changes should be
validated against their owning crates instead of relying only on the app check.

## Migration Rules

- Tauri remains the source of truth for behavior and visual structure until the
  native implementation has an explicit documented replacement.
- Translate UI from Tauri into GPUI through shared OxideTerm primitives. Do not
  redraw forms, menus, settings pages, or overlays by taste.
- Keep semantic tokens and named constants for colors, radii, spacing, sizes,
  and reusable control metrics.
- Keep feature behavior source-driven. If native must diverge from Tauri, record
  the reason in the relevant parity document.
- Keep user-facing strings in the i18n catalogs. Native currently maintains
  `en`, `zh-CN`, `zh-TW`, `de`, `es-ES`, `fr-FR`, `it`, `ja`, `ko`, `pt-BR`,
  and `vi`.
- Preserve clean source ownership. New native implementation code should be
  written for OxideTerm's architecture and documented dependency boundaries.

The most important project-wide guardrails live in
`docs/SYSTEM_INVARIANTS.md`.

## License

OxideTerm is licensed under `GPL-3.0-only`. Third-party notices and dependency
attribution are recorded in `NOTICE`.
