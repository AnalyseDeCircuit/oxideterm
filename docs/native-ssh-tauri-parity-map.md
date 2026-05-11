# Native SSH / Tauri Parity Map

This document is the working source map for aligning the Rust native SSH stack
with the Tauri implementation. Tauri remains the behavioral specification unless
the native runtime needs a stricter ownership rule to avoid stale handles.

## Tauri Source Of Truth

- `tauri版本代码/src/store/sessionTreeStore.ts`
  - Owns the session tree state, connection locks, link-down markers, reconnect
    progress, tab cleanup, and explicit disconnect cascade.
  - `computeUnifiedStatus` prioritizes `link-down > error > active/connected >
    connecting > idle`.
  - `clearLinkDown(root)` clears the root and descendants that do not own a
    separate `sshConnectionId`; descendants with their own connection stay
    link-down until their own connection recovers.
- `tauri版本代码/src/hooks/useConnectionEvents.ts`
  - Consumes runtime `connection_status_changed` events.
  - `link_down` marks affected nodes, schedules reconnect, and interrupts SFTP
    transfers.
  - `connected` clears link-down and reconnect progress.
  - `disconnected` closes node tabs, interrupts transfers, unregisters topology,
    and removes profiler listeners.
- `tauri版本代码/src/store/reconnectOrchestratorStore.ts`
  - Frontend-owned pipeline:
    `snapshot -> grace-period -> ssh-connect -> await-terminal ->
    restore-forwards -> resume-transfers -> restore-ide -> verify`.
  - Debounces link-down for 500 ms, collapses descendant reconnect requests under
    shallow roots, serializes pipelines, requeues at most 120 times.
  - Grace period probes old root connection every 3 seconds for 30 seconds.
  - Retry uses settings-driven max attempts, base delay, max delay, 1.5x backoff,
    0.8-1.2 jitter, and a non-retryable auth/host-key/user-cancel guard.
- `tauri版本代码/src-tauri/src/commands/session_tree.rs`
  - `connect_tree_node` and `disconnect_tree_node` are backend command entry
    points; disconnect is bottom-up for a subtree.
- `tauri版本代码/src-tauri/src/ssh/connection_registry.rs`
  - Registry heartbeat produces runtime facts. Backend emits state; frontend
    orchestrates reconnect.
- `tauri版本代码/src-tauri/src/router/mod.rs`
  - Node-first consumers resolve by node id. Link-down is not a usable transport.

## Native Map

| Tauri behavior | Native owner |
| --- | --- |
| Session tree state | `crates/oxideterm-gpui-app/src/workspace.rs`, `crates/oxideterm-ssh/src/router/runtime_store.rs` |
| Node-first transport routing | `crates/oxideterm-ssh/src/router/node_router.rs` |
| Registry state and consumers | `crates/oxideterm-ssh/src/connection_registry.rs` |
| New connection / node connect path | `crates/oxideterm-gpui-app/src/workspace/new_connection/ssh_flow.rs`, `crates/oxideterm-gpui-app/src/workspace/tabs/create.rs`, `crates/oxideterm-gpui-app/src/workspace/tabs/nodes.rs` |
| Link-down / node events | `crates/oxideterm-ssh/src/router/events.rs`, `crates/oxideterm-gpui-app/src/workspace/tabs/nodes.rs` |
| Reconnect pipeline | `crates/oxideterm-ssh/src/reconnect.rs`, `crates/oxideterm-gpui-app/src/workspace/tabs/nodes.rs` |
| Explicit disconnect | `crates/oxideterm-gpui-app/src/workspace/tabs/navigation.rs` |
| SFTP transfer interruption/resume | `crates/oxideterm-gpui-app/src/workspace/sftp/actions/transfers.rs`, `crates/oxideterm-sftp/src/progress.rs` |
| Forward restore | `crates/oxideterm-gpui-app/src/workspace/tabs/nodes.rs`, `crates/oxideterm-forwarding` |
| Reconnect settings | `crates/oxideterm-gpui-app/src/workspace/settings/*`, `crates/oxideterm-gpui-settings-view/src/types.rs` |

## Current Native Parity Fixes

- Native now debounces reconnect requests with the same 500 ms semantics and
  collapses descendant reconnect requests under shallow roots.
- Reconnect timing and max attempts are configured from settings instead of
  hardcoded defaults.
- Snapshot includes per-node old terminal session ids, active forward rules,
  old connection ids, IDE tabs, and dirty IDE contents.
- Grace recovery now follows Tauri `clearLinkDown(root)` semantics: root and
  inherited descendants recover, but a child with its own old SSH connection
  stays link-down unless that child connection also probes alive.
- Explicit disconnect now cancels reconnect jobs and pending/requeued reconnect
  requests for the affected subtree before closing tabs and clearing transports.
- Worker results now carry reconnect job identity where a stale async result
  could otherwise resurrect a user-disconnected node or overwrite a newer job.
- Native now has a settings-driven active connection probe loop. It marks dead
  active/idle connections as link-down through the registry/event path, rather
  than relying only on SFTP/forwarding callers to discover stale handles.
- SFTP transfer interruption now also pauses the background transfer control
  when possible, so reconnect snapshot/resume has a chance to see resumable work.
- Forward restore failures are surfaced as a failed reconnect phase instead of
  being hidden behind a successful verify message.
- Native now keeps an in-memory 500-entry connection/reconnect/node event log
  and captures node state changes plus reconnect phase transitions, with an
  activity badge for unread/error events.
- SessionTree now exposes a visible cancel action while a node reconnect job is
  active, and that action cancels the job, pending debounce/requeue state, and
  pending transfer resume bookkeeping.
- TabBar now mirrors Tauri's active reconnect replacement control: tabs tied to
  an active reconnect job show the current phase, attempt count, a compact phase
  timeline, and a cancel button instead of the normal close control.
- Native Activity/Notifications now has separate Notifications and Event Log
  subviews with DND toggles, clear/read actions, and severity/category/kind
  cycling filters.
- Link-down node events now also write a `connection_status_changed`-sourced log
  entry with affected child count and push a deduped connection notification;
  successful reconnect/grace recovery resolves node-scoped connection/security
  notifications.

## Intentional Native Differences

- Native keeps an extra physical-transport check in `NodeRouter::wait_for_active`.
  This is stricter than a status-only check and prevents borrowing a stale
  terminal-created transport after panes close. It preserves Tauri's node-owned
  resource semantics rather than copying a weaker state check.
- Native uses GPUI/terminal-owned panes instead of xterm.js tabs, so terminal
  remount is implemented by replacing pane sessions while preserving old session
  identity in the reconnect snapshot.

## Remaining Gaps To Close Before Claiming Full Parity

- User-visible reconnect surfaces are still incomplete: trace toast still needs
  a direct GPUI equivalent, and the new TabBar/Activity/Notifications surfaces
  still need visual parity review against Tauri screenshots.
- The active probe is now present, but it still needs live SSH end-to-end testing
  against root/child link-down to verify event timing and cascade behavior.
- Transfer resume needs one more audit pass: paused/failed progress is restored,
  but active transfers must be proven to persist progress before reconnect
  snapshot reads them.
- Reconnect event log parity is not finished. Native records node state,
  reconnect phase, and link-down affected-children entries in a 500-entry log,
  but it still lacks search text filtering, GPU timeline chart parity, exact
  i18n keys, and screenshot-verified ActivityPanel layout parity.

## Verification Gate

Do not describe SSH/reconnect parity as complete until:

- Focused reconnect and registry tests pass.
- `cargo check` passes for `oxideterm-ssh` and `oxideterm-gpui-app`.
- A live SSH scenario verifies:
  - root link-down schedules one reconnect;
  - grace recovery preserves inherited descendants but not dead independent
    child connections;
  - explicit disconnect does not resurrect via stale worker result;
  - forward restore failure is visible;
  - active SFTP transfer interruption produces resumable state.
