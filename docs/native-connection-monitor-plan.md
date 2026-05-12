# Native Connection Monitor Port Plan

This plan pins the native GPUI connection monitor to the Tauri source of truth. The goal is a language/runtime translation, not a redesigned monitor.

## Sources Read

- `tauri版本代码/src/components/layout/AppLayout.tsx`
- `tauri版本代码/src/components/connections/ConnectionPoolMonitor.tsx`
- `tauri版本代码/src/components/layout/SystemHealthPanel.tsx`
- `tauri版本代码/src/store/profilerStore.ts`
- `tauri版本代码/src/lib/api.ts`
- `tauri版本代码/src/lib/runtimeEventHub.ts`
- `tauri版本代码/src-tauri/src/commands/ssh.rs`
- `tauri版本代码/src-tauri/src/commands/health.rs`
- `tauri版本代码/src-tauri/src/session/profiler.rs`
- `crates/oxideterm-ssh/src/connection_registry.rs`
- `crates/oxideterm-forwarding/src/profiler.rs`
- `crates/oxideterm-gpui-app/src/workspace/sidebar/activity.rs`

## Tauri Contract

The Tauri `connection_monitor` tab is a composed page, not a single widget:

1. `ConnectionPoolMonitor`
   - Calls `ssh_get_pool_stats` every 2000 ms.
   - Shows loading, error, compact, auto-hide, and full panel states.
   - Displays total/capacity, active, idle, reconnecting, link-down, terminal/SFTP/forward consumer totals, total reference count, and idle timeout.

2. `SystemHealthPanel`
   - Uses a connection selector sourced from the app connection map.
   - Auto-selects the first available connection and resets when the selected connection disappears.
   - Auto-starts a profiler only when a connection has no local profiler state.
   - Supports explicit start/stop toggle.
   - Renders disabled, sampling, no-data, no-connection, and metrics states.
   - Shows CPU, memory, network, load average, RTT, sparkline history, and source footer.

3. `profilerStore`
   - Is the single frontend source for profiler state.
   - Stores per-connection metrics, history, running/enabled flags, errors, and generation tokens.
   - Ignores stale async start/stop results by generation.
   - Listens to generic `profiler:update` events and appends history up to 60 entries.

4. Tauri backend `ProfilerRegistry`
   - Owns one `ResourceProfiler` per connection.
   - `start_resource_profiler` is idempotent while running; stopped/degraded profilers are removed and respawned.
   - `stop_resource_profiler` is idempotent.
   - `get_resource_metrics` returns latest or `None`.
   - `get_resource_history` returns history or an empty list.
   - Profiler lifetime is bound to the SSH connection disconnect stream; it must not own node liveness.

## Current Native State

- `oxideterm-ssh` already has `ConnectionPoolStats`, but it currently covers only connection state counts: total, active, idle, link-down, reconnecting, disconnected, errored.
- Native pool stats do not yet expose all Tauri monitor fields, especially consumer totals, total reference count, capacity, and idle timeout in the exact UI-facing shape.
- Native has `oxideterm-forwarding::PortDetectionProfiler`, but this is a forwarding-specific smart port detector, not a general resource profiler.
- Native does not yet have a GPUI `ConnectionMonitor` tab/page equivalent to Tauri's `connection_monitor`.
- Native sidebar currently overloads the monitor area for platform launch/graphics features; Tauri treats `connection_monitor` as its own tab.

## Crate Boundary Recommendation

Yes, this backend should get a small dedicated crate, but only for the connection-monitor/resource-profiler domain.

Recommended crate: `oxideterm-connection-monitor`.

Responsibilities:

- Define UI-consumed monitor types:
  - `ConnectionPoolMonitorStats`
  - `ResourceMetrics`
  - `MetricsSource`
  - `ProfilerState`
  - `ProfilerUpdate`
- Own resource-profiler registry semantics:
  - one profiler per connection
  - idempotent start/stop
  - latest/history storage
  - generation-safe event emission boundary
  - disconnect cleanup hook
- Own pure parsing/sampling helpers:
  - Linux `/proc` resource output parser
  - RTT-only and failed metric construction
  - history trimming
  - source classification: `full`, `partial`, `rtt_only`, `failed`
- Optionally share port detection parser types with forwarding later, but do not move GPUI UI or SSH registry internals into this crate.

Do not put in this crate:

- GPUI elements or theme/layout constants.
- `SshConnectionRegistry` ownership.
- NodeRouter ownership.
- Forwarding rule/listener lifecycle.
- SFTP/IDE runtime state.

Why a separate crate is better than `oxideterm-ssh`:

- The resource profiler is a monitor consumer of SSH connections, not core SSH routing.
- Keeping it outside `oxideterm-ssh` avoids turning SSH into a UI-observability crate.
- Forwarding already needs port detection, but resource metrics and forwarding lifecycle should not become one mixed profiler abstraction.

Why not put it directly in `oxideterm-gpui-app`:

- Tauri's profiler is backend-owned and is also used by terminal performance capsules and port detection hooks.
- GPUI should consume monitor state; it should not own the long-running profiler task.
- Tests for parsing, history, stale updates, and lifecycle cleanup should run without GPUI.

## Native Port Map

| Tauri entry/state | Native target | Notes |
| --- | --- | --- |
| `ssh_get_pool_stats` | `SshConnectionRegistry` snapshot converted to `ConnectionPoolMonitorStats` | Extend current stats without changing registry ownership. |
| `ProfilerRegistry` | `oxideterm-connection-monitor::ProfilerRegistry` | One profiler per connection, idempotent start/stop. |
| `ResourceProfiler::spawn` | native profiler spawned on the app/backend runtime | Must subscribe to connection disconnect and stop without reviving the connection. |
| `profiler:update` event | GPUI app monitor event/channel | Payload must preserve `connectionId` + `metrics`. |
| `profilerStore` generations | GPUI `ConnectionMonitorState` generation map | Prevent stale start/stop or metrics from overwriting newer UI state. |
| `ConnectionPoolMonitor` | `workspace/connection_monitor.rs` pool section | 2000 ms refresh and matching loading/error/compact/full states. |
| `SystemHealthPanel` | same GPUI tab, health section | Selector, disabled/sampling/no-data/metrics states. |
| Tauri sidebar `connection_monitor` | native `TabKind::ConnectionMonitor` | Do not reuse Windows Graphics monitor entry. |

## Implementation Phases

### Phase 1: Backend Types And Pool Snapshot

- Add `oxideterm-connection-monitor` crate.
- Move no existing code initially; define the monitor-facing types and conversion helpers.
- Extend native pool stats to include Tauri-visible fields:
  - pool capacity
  - idle timeout seconds
  - total connections
  - active/idle/reconnecting/link-down counts
  - terminal/SFTP/forward consumer totals
  - total sessions/ref count summary
- Add focused tests around consumer counting and config conversion.

### Phase 2: Resource Profiler Runtime

- Port the Tauri `ResourceMetrics`, `MetricsSource`, and `ProfilerState` contract.
- Port parser behavior for full, partial, RTT-only, and failed samples.
- Implement profiler registry:
  - idempotent start while running
  - respawn stopped/degraded profiler
  - idempotent stop
  - latest/history getters
  - stop-all cleanup
- Bind profiler lifetime to the acquired SSH connection's disconnect state.
- Ensure sampling uses the existing node/registry handle and never reacquires or revives a manually disconnected node.

### Phase 3: GPUI Store And Event Bridge

- Add connection-monitor state to GPUI workspace state.
- Mirror Tauri `profilerStore` semantics:
  - per-connection state
  - max history 60
  - sparkline slice 12
  - generation tokens for stale async suppression
  - remove state when the connection disappears
- Wire backend profiler updates into the GPUI state with `connectionId` + metrics payload.

### Phase 4: GPUI Page Port

- Add `TabKind::ConnectionMonitor`.
- Add `workspace/connection_monitor.rs`.
- Translate the Tauri page structure:
  - outer `p-8 overflow-auto`
  - inner `max-w-5xl mx-auto space-y-8`
  - `ConnectionPoolMonitor` section first
  - `SystemHealthPanel` section second
- Translate visual states from Tailwind to GPUI tokens:
  - border `theme-border/50`
  - panel background
  - muted text
  - emerald/amber/red threshold colors
  - mono tabular numeric text
  - rounded-md card radius
  - progress and sparkline geometry
- Copy all required locale keys from Tauri for every native locale.

### Phase 5: Sidebar And Interaction Wiring

- Add a real connection-monitor open action rather than overloading the platform monitor/graphics entry.
- Keep existing launcher/graphics behavior intact.
- Make tab title/icon match Tauri `connection_monitor`.
- Ensure opening the tab does not start any profiler until a connection is selected/auto-selected, matching Tauri.

### Phase 6: Tests And Verification

Backend focused tests:

- pool stats count terminal, SFTP, and forward consumers separately
- pool stats expose config fields with Tauri naming/semantics
- start profiler is idempotent while running
- stopped/degraded profiler respawns on start
- stop profiler is idempotent
- history trims to 60 entries
- stale generation cannot overwrite stopped/removed state
- disconnect cleanup stops profiler without reacquiring connection

GPUI focused tests:

- no-connection state renders without starting profiler
- first connection auto-selects
- removed selected connection resets selection
- disabled toggle starts profiler
- enabled toggle stops profiler and clears metrics
- sampling state shows before first metric
- RTT-only source hides CPU/memory/network/load cards
- stale metric for removed connection is ignored

Manual/runtime checks:

- open connection monitor with zero connections
- open monitor with a direct SSH connection
- open monitor with a proxy-chain child connection
- close terminal while connection remains alive; profiler should keep following connection state
- manual disconnect stops profiler and clears visible running state
- parent link-down interrupts child monitor state without reviving the child

## Non-Goals

- Do not port notification center or timeline behavior here.
- Do not redesign the monitor as a dashboard.
- Do not merge `PortDetectionProfiler` and `ResourceProfiler` unless a later source audit proves their Tauri lifecycles are identical.
- Do not make terminal pane lifetime own connection health.
- Do not add extra metrics that Tauri does not expose.

## Confirmed Serialized Pool Stats Shape

Tauri serializes `ConnectionPoolStats` with `camelCase` fields:

- `totalConnections`
- `activeConnections`
- `idleConnections`
- `reconnectingConnections`
- `linkDownConnections`
- `totalTerminals`
- `totalSftpSessions`
- `totalForwards`
- `totalRefCount`
- `poolCapacity`
- `idleTimeoutSecs`

Native monitor-facing stats should preserve these names at the UI boundary.

## Open Checks Before Implementation

- Confirm all `profiler.panel.*` and `connections.monitor.*` locale keys are present in native i18n.
- Confirm whether terminal `PerformanceCapsule` exists or is planned in native; if yes, it should consume the same profiler state.
- Confirm how native GPUI should surface the new `ConnectionMonitor` entry without colliding with Windows Graphics/WSL monitor entry.
