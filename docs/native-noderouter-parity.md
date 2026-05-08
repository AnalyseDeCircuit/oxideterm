# Native NodeRouter Parity

This document tracks the rustnative migration against the Tauri NodeRouter
architecture. It is intentionally narrow: NodeRouter is a resolver and capability
entry point, not a UI tab owner and not a connection builder.

## Tauri Source Of Truth

- `docs/tauri/tauri-noderouter-graceperiod.md`
- `tauri版本代码/src-tauri/src/router/mod.rs`
- `tauri版本代码/src-tauri/src/router/types.rs`
- `tauri版本代码/src-tauri/src/router/emitter.rs`
- `tauri版本代码/src-tauri/src/session/tree.rs`
- `tauri版本代码/src-tauri/src/commands/node_sftp.rs`
- `tauri版本代码/src-tauri/src/commands/node_forwarding.rs`

## Ownership Map

| Tauri owner | Rustnative owner | Notes |
| --- | --- | --- |
| `SessionTreeState` runtime snapshot | `NodeRuntimeStore` plus `session_tree.json` restore | Stores runtime ids, origin metadata, parent/child/depth topology, flat tree snapshots, summaries, startup restore, and registry reconciliation. |
| `SshConnectionRegistry` | `SshConnectionRegistry` | Shared SSH connection and consumer ownership. |
| `SessionRegistry` terminal endpoint | `WorkspaceApp::terminal_endpoint_sessions` plus `NodeRouter::terminal_url` | Native now has a SessionRegistry-shaped owner record and node endpoint snapshot. The GPUI pane consumes the shared terminal session instead of being the authoritative owner. |
| `NodeRouter` | `NodeRouter` | Now resolves from the shared runtime store and registry instead of owning node state maps directly. |
| `NodeEventEmitter` | `NodeEventEmitter` plus GPUI `node_event_tx` | Native now has a connection-id to node-id emitter, sequencer, listener dispatch, and registry state-change emission. |
| `ConnectionEntry.sftp` owner | `ConnectionEntry` / `SshConnectionHandle` SFTP owner | Shared and transfer SFTP entries are node-first, not terminal-pane-first. |

## Current Native Rules

- Opening a terminal must not make the terminal pane the lifetime owner of the node.
- Opening SFTP or forwarding may start a node-only SSH connection first; it must not
  create a terminal just to obtain a transport.
- Child nodes must connect through the parent node's active SSH transport by opening
  a `direct-tcpip` tunnel, matching Tauri `establish_tunneled_connection`.
- SFTP and port forwarding must resolve through `NodeRouter` by `NodeId`.
- Closing a terminal pane may unbind the terminal session id, but must not release the
  node's router consumer or kill SFTP/forwards by itself.
- Explicit node disconnect closes node-related tabs and releases node-scoped consumers.
- Sidebar section state is user-owned. Opening/closing tabs must not force a sidebar
  section jump.

## Remaining Gaps

- Native now carries Tauri-style `SessionTreeState` metadata in
  `NodeRuntimeStore`: `ManualPreset`, `AutoRoute`, `DrillDown`, `Direct`, and
  `Restored` origins, root ordering, child ordering, flat-node snapshots,
  summaries, and registry reconciliation. GPUI persists a `session_tree.json`
  snapshot and restores saved-connection nodes through `ConnectionStore` so
  runtime SSH/SFTP/terminal ids are rebuilt rather than trusted after restart.
  For secret safety, ad-hoc nodes that would require serializing runtime
  passwords or passphrases are not written to disk; saved connections restore
  through their keychain-backed connection records.
- Native has node-only connect for direct nodes and tunneled child nodes. The child
  path uses the parent connection's `direct-tcpip` channel before authenticating the
  child target.
- Async SFTP routes and forwarding manager creation wait for `Connecting` /
  `Reconnecting` up to the Tauri 15s window. Initial create, scan, and restore all
  resolve the node on the forwarding worker path before registering a manager.
- Registry `mark_state` now emits through `NodeEventEmitter` when the connection is
  registered to a node.
- Terminal endpoint ownership now has the Tauri-shaped owner boundary in native:
  workspace owns terminal endpoint/session records, `NodeRouter::terminal_url`
  resolves by node id, and GPUI panes consume the shared session. Native keeps
  this endpoint in-process by design; the Tauri loopback WebSocket was an IPC
  workaround and is not a rustnative requirement.
- Grace-period reconnect now falls back to the node-only connect path after probe
  expiry and records the Tauri phase order through ssh-connect, await-terminal,
  restore-forwards, resume-transfers, restore-ide, and verify. Terminal panes are
  remounted by replacing old terminal session ids inside the existing pane tree.
  SFTP transfer resume is snapshot-driven and routes through the node/router-backed
  transfer owner instead of the active SFTP tab. IDE restore remains explicitly
  skipped because the GPUI IDE store/project owner does not exist yet.

## Migration Rule

Do not add new SFTP, forwarding, reconnect, or terminal node behavior by reading a
terminal pane as the source of truth. Add the missing Tauri owner first, then route
the behavior through `NodeRouter`.
