# OxideTerm

OxideTerm is a modern SSH terminal client and remote workspace application for
people who live between local shells, remote servers, files, tunnels, and AI
assistance.

It combines a fast terminal workspace with saved SSH targets, session topology,
SFTP file operations, port forwarding, remote editing, Knowledge/RAG search, MCP
servers, and an AI assistant that can understand terminal and workspace context.

## What It Does

- Work in local and SSH terminal sessions with tabs, split panes, search,
  shell-aware actions, recording playback, and terminal graphics support.
- Manage SSH targets, jump-host topology, reconnect state, host-key decisions,
  and long-lived remote sessions without tying connection lifetime to a single
  terminal pane.
- Browse and transfer remote files through SFTP, including resumable transfers,
  directory transfers, conflict handling, and background transfer state.
- Create local, remote, and dynamic port forwards, keep saved forwarding rules,
  and restore forwarding state across reconnect flows.
- Open remote workspaces with editor and IDE surfaces that can use SFTP/exec
  fallback paths or a lightweight remote agent when available.
- Use OxideSens, the built-in AI assistant, with provider configuration,
  context-window handling, memory, reasoning-effort controls, tool approvals,
  MCP servers, and Knowledge/RAG retrieval.
- Configure the app through native settings surfaces covering terminal,
  appearance, connections, SSH, reconnect, SFTP, IDE, AI, Knowledge, MCP,
  keybindings, portable mode, and help/about sections.

## Native Status

This repository is the Rust/GPUI native implementation of OxideTerm. The checked
in Tauri application under `tauri版本代码/` remains the source of truth for product
behavior and visual structure while native parity is migrated feature by
feature.

The native app currently includes the major workspace foundations:

- GPUI product shell and terminal surface.
- Local terminal, SSH, SFTP, forwarding, reconnect, topology, settings, AI,
  MCP, Knowledge/RAG, editor, IDE, preview, launcher, local file, notification,
  and workspace crates.
- Shared Rust settings schema, migrations, i18n catalogs, provider key storage,
  remote node-agent plumbing, and parity documentation.

Some areas are still being verified against the Tauri implementation. Use the
parity maps in `docs/` for exact status rather than assuming every listed
surface is complete product parity.

## Run Native

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
- `tauri版本代码/` - reference Tauri implementation used for parity auditing and
  source-driven native migration.

## Development Checks

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

## Tauri Reference

The Tauri reference product metadata is:

- Product name: `OxideTerm`
- Tauri version: `1.4.0-beta.6`
- Tauri identifier: `com.oxideterm.app`
- Tauri description: `A modern SSH terminal client built with Rust and Tauri`
- Tauri bundled resources: remote Linux agent artifacts and `cli-bin`

When native behavior is unclear, start from these Tauri sources:

- `tauri版本代码/src/App.tsx` - product-level providers, startup effects, modals,
  event bridges, plugin initialization, update UI, command palette, shortcuts,
  and global overlays.
- `tauri版本代码/src/components/layout/AppLayout.tsx` and
  `tauri版本代码/src/components/layout/Sidebar.tsx` - workspace shell, sidebar,
  tab layout, activity panels, and responsive structure.
- `tauri版本代码/src/store/*` - source stores for settings, tabs, session tree,
  local terminal, AI chat, RAG, MCP, transfers, plugins, notifications,
  reconnect, and command palette behavior.
- `tauri版本代码/src/components/settings/*` - settings UI, MCP server management,
  Knowledge document management, provider settings, embedding settings, and
  shared form behavior.
- `tauri版本代码/src/components/terminal/*` - terminal pane layout, command bar,
  search, recording controls, paste confirmation, split panes, cast playback,
  and AI inline panel.
- `tauri版本代码/src/components/sftp/*`,
  `tauri版本代码/src/components/forwards/*`,
  `tauri版本代码/src/components/sessionManager/*`, and
  `tauri版本代码/src/components/ide/*` - remote workspace feature surfaces.
- `tauri版本代码/src-tauri/src/*` - backend command semantics, runtime ownership,
  event payloads, persistence, node routing, transfer behavior, and security
  boundaries.

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
