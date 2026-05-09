# Native IDE Agent Source Map

Tauri is the source of truth for the native IDE agent boundary.

## What The Agent Is

There are two different pieces named "agent":

- Remote agent binary: a standalone Linux executable deployed to
  `~/.oxideterm/oxideterm-agent` on the SSH host.
- Host-side agent proxy: app code that uploads the binary, starts it over an SSH
  exec channel, speaks line-delimited JSON-RPC, and falls back to SFTP/exec when
  the agent is unavailable.

The remote agent binary is not part of the normal app compile. Tauri packages
prebuilt artifacts from `src-tauri/agents/oxideterm-agent-*-linux-musl` via
`tauri.conf.json` resources. The native GPUI app must keep the same contract:
remote-agent binaries are packaged resources under `resources/agents`, not
`include_bytes!` from source.

## Tauri Files Read

- `tauri版本代码/src-tauri/tauri.conf.json`
  - packages `agents/oxideterm-agent-*-linux-musl` as app resources.
- `tauri版本代码/src-tauri/build.rs`
  - only ensures `agents/` exists so resource packaging does not fail on dev
    builds.
- `tauri版本代码/src-tauri/src/agent/deploy.rs`
  - detects remote arch, resolves packaged binary, uploads via SFTP, chmods,
    starts the remote agent, and validates `sys/info`.
- `tauri版本代码/src-tauri/src/agent/transport.rs`
  - JSON-RPC over a persistent SSH exec channel.
- `tauri版本代码/src-tauri/src/agent/registry.rs`
  - stores one live agent session per SSH connection id.
- `tauri版本代码/src-tauri/src/agent/protocol.rs`
  - wire protocol and status/result types.
- `tauri版本代码/src-tauri/src/commands/node_agent.rs`
  - IPC command layer for deploy/status/files/tree/grep/git/watch/symbols.
- `tauri版本代码/src/lib/agentService.ts`
  - frontend facade: readiness cache, deploy de-dupe, agent-first file ops, and
    SFTP/exec fallback policy.
- `tauri版本代码/src/store/ideStore.ts`
  - IDE state consumes `agentService`; only `agentMode === "enabled"` auto
    deploys, `ask` is a UI opt-in, `disabled` skips agent.

## Native Mapping

- Host-side proxy lives in `oxideterm-ide-fs`.
- GPUI IDE consumes status and routes file open/save/list through the proxy.
- Remote-agent binaries must be resolved from:
  - `OXIDETERM_AGENT_DIR`,
  - app resources `agents/`,
  - development fallbacks.

## Native Agent Build

- `agent/` is a standalone crate with its own `Cargo.lock` and `[workspace]`
  table. It is intentionally not listed in the root workspace `members`.
- `scripts/build-agent.sh` builds:
  - `x86_64-unknown-linux-musl` -> `oxideterm-agent-x86_64-linux-musl`
  - `aarch64-unknown-linux-musl` -> `oxideterm-agent-aarch64-linux-musl`
- The script copies artifacts into
  `crates/oxideterm-gpui-app/resources/agents/`.
- `crates/oxideterm-gpui-app/Cargo.toml` packages `resources/agents` with the
  native app.

## Implemented Native Agent Methods

- `sys/ping`
- `sys/info`
- `sys/shutdown`
- `fs/readFile`
- `fs/writeFile`
- `fs/stat`
- `fs/listDir`
- `fs/listTree`
- `fs/mkdir`
- `fs/remove`
- `fs/rename`
- `fs/chmod`
- `search/grep`
- `git/status`

`watch/start`, `watch/stop`, and `symbols/*` are protocol-compatible stubs for
now. They must be replaced with real watcher and symbol-index implementations
before claiming full Tauri parity.

## Still Missing

- Ask-mode opt-in dialog.
- Watch relay events backed by real remote watcher notifications.
- Grep/git/symbol UI integration.
