# Reconnect Orchestrator (Frontend-Only)

> **状态**: ✅ 已完整实现（v1.6.2），当前版本 v1.11.1。v1.11.1 新增 Grace Period 阶段和主动探测机制。下文保留原始设计文档作为架构参考。

## Summary
Introduce a frontend-only reconnect orchestrator to replace the current debounce+retry logic in `useConnectionEvents`. The orchestrator owns reconnection state, queues per node, and runs a deterministic recovery pipeline for SSH, port forwards, SFTP transfers, and IDE state. No backend changes. Terminal recovery is handled automatically by React Key-Driven Reset and is NOT part of the pipeline.

## Why This Plan (Review Findings)
1. Backend auto-reconnect is intentionally removed (`NEUTRALIZED STUB`). Frontend must orchestrate.
2. `useConnectionEvents` currently debounces and calls `reconnectCascade`, but has no post-reconnect recovery for forwards/SFTP/IDE.
3. Every reconnect creates a **new connectionId** (UUID). The old connection is abandoned.
4. `resetNodeState` is a scorched-earth reset: it closes terminals (destroying forwarding managers + persisted rules), clears all session IDs, and resets node status to `pending`.
5. Terminal restoration happens automatically via Key-Driven Reset (`key={sessionId-connectionId}`). The orchestrator should NOT manage terminal lifecycle.
6. Port forwards and SFTP transfers need explicit recovery after reconnection succeeds.

## Goals
1. A single reconnect brain: queueing, throttling, retries, and observability live in one store.
2. Idempotent, cancellable pipeline: one node → one job, no duplicate work.
3. Deterministic recovery sequence: **Snapshot → Grace Period → SSH → Forwards → SFTP → IDE**.
4. Preserve user intent: do not restart user-stopped forwards; do not restore unsaved file contents.

## Non-Goals
1. ~~No backend changes or new Tauri commands.~~ **(Updated v1.11.1)**: Added `probe_connections` and `probe_single_connection` IPC commands for Grace Period support.
2. No automatic recovery for "disconnected" (hard close) events — only "link_down".
3. No content-level IDE restore (only reopen file tabs).
4. No terminal session management (handled by Key-Driven Reset).

## Constraints (From Code Verification)
1. `resetNodeState` closes terminals via `api.closeTerminal()`, which triggers `forwarding_registry.remove()` on the backend, **permanently destroying forwarding rules and persisted data**. Snapshots MUST be captured before `reconnectCascade` is called.
2. `recreate_terminal_pty` reuses the existing session on the same connection — but since reconnect always creates a new connectionId, old sessions become orphaned and new ones must be created fresh.
3. `node_sftp_list_incomplete_transfers` queries by nodeId. Progress data survives `close_terminal` (separate storage). Can resume on the same node after reconnection.
4. Backend `pause_forwards`/`restore_forwards` exist but are not exposed to frontend API layer. Use `listPortForwards` + `createPortForward` instead.
5. `connectNodeWithAncestors` calls `resetNodeState` internally — no hook between them. Snapshot must happen BEFORE entering `reconnectCascade`.

## Proposed Architecture

### New Store
Create `src/store/reconnectOrchestratorStore.ts` (Zustand).

```typescript
type ReconnectPhase =
  | 'queued'
  | 'snapshot'        // Capture pre-reset data
  | 'grace-period'    // (v1.11.1) Attempt to recover existing connection
  | 'ssh-connect'     // reconnectCascade
  | 'await-terminal'  // Wait for Key-Driven Reset to recreate terminals
  | 'restore-forwards'
  | 'resume-transfers'
  | 'restore-ide'
  | 'verify'
  | 'done'
  | 'failed'
  | 'cancelled';

type ReconnectJob = {
  nodeId: string;
  nodeName: string;           // For toast messages
  status: ReconnectPhase;
  attempt: number;
  maxAttempts: number;        // 5 (MAX_ATTEMPTS)
  startedAt: number;
  endedAt?: number;
  error?: string;
  snapshot: ReconnectSnapshot;
  abortController: AbortController;
  restoredCount: number;      // Number of services restored (for toast)
  phaseHistory: PhaseEvent[]; // Append-only phase event log for debugging
};

type ReconnectSnapshot = {
  nodeId: string;
  snapshotAt: number;          // Timestamp for user intent detection
  // Forward rules captured BEFORE resetNodeState destroys them
  forwardRules: Array<{
    nodeId: string;            // Node ID
    rules: ForwardRule[];      // Only active/suspended rules, NOT stopped ones
  }>;
  // Old terminal session IDs for querying incomplete transfers
  oldTerminalSessionIds: string[];
  // Per-node mapping of old terminal session IDs
  perNodeOldSessionIds: Map<string, string[]>;
  // Incomplete SFTP transfers captured BEFORE resetNodeState destroys old sessions
  incompleteTransfers: Array<{
    oldSessionId: string;
    transfers: IncompleteTransferInfo[];
  }>;
  // IDE state (if IDE tab was open for this node)
  ideSnapshot?: {
    projectPath: string;
    tabPaths: string[];
    connectionId: string;      // For topology resolution
  };
};
```

Core state:
- `jobs: Map<string, ReconnectJob>` keyed by nodeId.
- `isRunning: boolean` — single worker guard.

Core methods:
- `scheduleReconnect(nodeId: string)` — 500ms debounce, collapse to shallowest root.
- `cancel(nodeId: string)` — Cancel job + all descendant jobs, call `abortController.abort()`.
- `cancelAll()` — Cancel everything.
- `clearCompleted()` — Remove done/failed/cancelled jobs from map.

### Queueing
- Debounce: 500ms window collects multiple link_down nodes, then picks shallowest root.
- Idempotent: if `jobs.has(nodeId)` and status not terminal (`done`/`failed`/`cancelled`), skip.
- Concurrency: 1 (reuse existing `chainLock` mechanism).
- Retry: exponential backoff, `MAX_ATTEMPTS = 5`, `BASE_RETRY_DELAY_MS = 1000`, `MAX_RETRY_DELAY_MS = 15000`, `BACKOFF_MULTIPLIER = 1.5` (± 20% jitter).

## Pipeline Details

### Phase 0: `snapshot` (NEW — Critical Fix)

**MUST execute before `reconnectCascade` to capture data that resetNodeState destroys.**

1. Collect `oldTerminalSessionIds` from `nodeTerminalMap.get(nodeId)` and descendants.
2. Build `perNodeOldSessionIds` mapping (nodeId → old session IDs) for deterministic session mapping later.
3. For each old session ID:
   a. Call `api.listPortForwards(sessionId)` to snapshot forward rules.
   b. Filter: keep only rules with `status !== 'stopped'` (respect user intent).
4. Snapshot incomplete SFTP transfers (`nodeSftpListIncompleteTransfers`) BEFORE `resetNodeState` destroys old sessions.
5. Check `ideStore`: if current project's nodeId matches, save `{ projectPath, tabPaths, connectionId }`.
6. Store snapshot in job with `snapshotAt` timestamp (used for user intent detection in Phase 5).

**Why this works**: `listPortForwards` and `nodeSftpListIncompleteTransfers` query the backend which still exists at this point. `resetNodeState` hasn't been called yet.

### Phase 0.5: `grace-period` (v1.11.1 — NEW)

**Before executing destructive reconnect, attempt to recover the existing connection.**

This phase was introduced to solve the "焦土模式" problem: immediate reconnect kills TUI applications (yazi, vim, htop) by destroying the old SSH session.

**Logic**:
1. Collect `oldConnectionIds` from snapshot (connectionId for each affected node).
2. Loop every `GRACE_PROBE_INTERVAL_MS` (3s) for up to `GRACE_PERIOD_MS` (30s):
   a. Call `api.probeSingleConnection(oldConnectionId)` — sends SSH keepalive ping.
   b. If result is `"alive"`:
      - Clear `link_down` state for each affected node.
      - Recover child node states.
      - Show "connection recovered — session preserved" toast.
      - Return `true` → **skip all subsequent phases** (no destructive reconnect needed).
   c. If result is `"dead"` or API error: continue probing until timeout.
3. After 30s timeout: return `false` → proceed to `ssh-connect` (destructive reconnect).

**Why this matters**: If network interruption is < 30s (common for Wi-Fi switching, sleep/wake, brief outages), the SSH TCP connection may still be alive on the server side. Probing recovers it without killing any running programs.

| Constant | Value | Purpose |
|----------|-------|---------|
| `GRACE_PERIOD_MS` | 30,000 | Maximum time to wait for recovery |
| `GRACE_PROBE_INTERVAL_MS` | 3,000 | Probe interval within grace period |

### Phase 1: `ssh-connect`

1. Call `reconnectCascade(rootNodeId)` — this internally:
   - `resetNodeState()` per node (destroys terminals, forwards, sessions)
   - `connectNodeInternal()` per node (creates new SSH connection with new UUID)
   - Reconnects descendants
   - `fetchTree()`
2. On success: new `connectionId` is set on each node in `rawNodes`.
3. On failure: mark job `failed`, show error toast.

### Phase 2: `await-terminal`

React Key-Driven Reset handles terminal creation automatically:
- `AppLayout` renders `TerminalView` with `key={sessionId-connectionId}`.
- `connectionId` changed → old component unmounts → new component mounts → calls `createTerminalForNode`.

Orchestrator waits for the new `terminalSessionId` to appear:
1. Determine which nodes NEED a terminal session (nodes that had forwards or incomplete transfers in the snapshot).
2. Poll `rawNodes[nodeId].terminalSessionId` every 500ms, timeout 10s.
3. For nodes that need a session but have no terminal tab open, explicitly call `createTerminalForNode()` to ensure a valid session exists for forward/transfer restore.
4. Build `oldSessionId → newSessionId` mapping from `perNodeOldSessionIds` + current state (deterministic per-node mapping).

### Phase 3: `restore-forwards`

1. Collect existing live forwards to avoid duplicating or resurrecting user-stopped rules.
2. For each entry in `snapshot.forwardRules`:
   a. Look up new sessionId from old→new mapping.
   b. If no new session exists for this node, skip.
   c. Re-check live forwards right before creation to catch user actions during the loop.
   d. For each rule (excluding `stopped`):
      - Skip if a forward with the same `type:bind_address:bind_port` key already exists.
      - Call `api.createPortForward({ sessionId: newSessionId, ...rule })`.
      - On failure: log warning, continue with next rule.
      - On success: increment `restoredCount`.
3. Check `abortController.signal` between each rule.

### Phase 4: `resume-transfers`

1. Use pre-captured incomplete transfers from `snapshot.incompleteTransfers` (captured in Phase 0 before `resetNodeState` destroyed old sessions).
2. Ensure SFTP sessions are initialized for all affected nodes before resuming (call `openSftpForNode` if needed).
3. For each incomplete transfer entry:
   a. The nodeId is already known from the snapshot.
   b. For each incomplete transfer:
      - Call `nodeSftpResumeTransfer(nodeId, transferId)`.
      - On failure: log warning, continue.
      - On success: update `transferStore`, increment `restoredCount`.
4. Check `abortController.signal` between each transfer.

### Phase 5: `restore-ide`

1. If `snapshot.ideSnapshot` exists:
   a. Use `topologyResolver.getNodeId(ideSnapshot.connectionId)` to find the target node.
   b. Look up new `connectionId` from current node state (SFTP is managed by ConnectionEntry, no separate sftpSessionId needed).
   c. **User intent detection**: Skip if user changed project or closed IDE after snapshot:
      - If `ideStore.project` exists with a different `rootPath`, skip (user changed project).
      - If `ideStore.project` exists with the same `rootPath`, skip (already open).
      - If `ideStore.lastClosedAt > snapshot.snapshotAt`, skip (user intentionally closed IDE).
   d. Call `ideStore.openProject(nodeId, projectPath)`.
   e. For each cached tab path: call `ideStore.openFile(path)`.
   f. Do NOT restore `content`/`originalContent` (files will be re-fetched from remote).
2. To enable this, enhance `ideStore.partialize` to persist `cachedProjectPath` and `cachedTabPaths`.

## Integration Points

### `useConnectionEvents` (simplify)
Remove from this hook:
- `pendingReconnectNodes` Set
- `reconnectDebounceTimer` ref
- `isReconnecting` flag
- `reconnectRetryCount` counter
- `scheduleReconnect` / `attemptReconnect` / `cancelPendingReconnect` / `clearAllPendingReconnects` functions

Keep:
- `link_down` handler: `updateConnectionState()` → `markLinkDownBatch()` → **`orchestrator.scheduleReconnect(nodeId)`** → `interruptTransfersBySession()`
- `connected` handler: `clearLinkDown()`, `setReconnectProgress(null)` (orchestrator handles the rest)
- `disconnected` handler: unchanged (hard disconnect, not orchestrator's concern)

### `TabBar`
- Replace `session?.state === 'reconnecting'` check with `orchestratorStore.jobs.get(nodeId)?.status`.
- Manual reconnect button: call `orchestrator.scheduleReconnect(nodeId)`.
- Cancel button: call `orchestrator.cancel(nodeId)`.

### `sessionTreeStore`
- `cancelPendingReconnect(nodeId)` usage sites → `orchestrator.cancel(nodeId)`.
- `reconnectCascade` remains unchanged (orchestrator calls it as-is).

### `appStore`
- Remove `cancelReconnect()` action (dead code — backend auto-reconnect disabled).

### `ideStore`
- Add to `partialize`: `cachedProjectPath`, `cachedTabPaths`, `cachedNodeId`.
- On `openProject`: save these cached fields.
- On `closeProject`: clear cached fields.

## Observability

Toast notifications via `useToastStore`:

| Event | Variant | Message Key |
|-------|---------|-------------|
| Job start | `info` | `connections.reconnect.starting` — "{nodeName} 正在恢复连接..." |
| SSH success | `info` | `connections.reconnect.ssh_restored` — "SSH 连接已恢复" |
| All done | `success` | `connections.reconnect.completed` — "连接已恢复，{count} 个服务已重建" |
| Failed | `error` | `connections.reconnect.failed` — "连接恢复失败: {error}"（附重试按钮） |
| Cancelled | `info` | `connections.reconnect.cancelled` — "已取消重连" |

Add keys to `connections.json` in all 11 locales. Do NOT create a new locale namespace.

## Files To Change

| File | Action | Scope |
|------|--------|-------|
| `src/store/reconnectOrchestratorStore.ts` | **New** | Core orchestrator (≈300 lines) |
| `src/hooks/useConnectionEvents.ts` | **Simplify** | Remove debounce/retry logic, delegate to orchestrator |
| `src/store/appStore.ts` | **Minor** | Remove dead `cancelReconnect()` |
| `src/store/ideStore.ts` | **Minor** | Add cached fields to `partialize` |
| `src/components/layout/TabBar.tsx` | **Minor** | Read job state from orchestrator |
| `src/locales/*/connections.json` | **Minor** | Add 5 reconnect toast keys × 11 locales |
| `docs/ARCHITECTURE.md` | **Update** | Remove stale backend auto-reconnect docs |
| `docs/SYSTEM_INVARIANTS.md` | **Update** | Add orchestrator invariants |

## Verification Checklist
- [x] Link down → job enqueued with snapshot → SSH reconnect → services restored → toast
- [x] Cancel mid-run → job cancelled, later phases skipped, toast
- [x] Multiple link_down events within 500ms → debounce to shallowest root
- [x] Idempotent: same nodeId enqueued twice → second is skipped
- [x] Suspended forwards restored on new session; user-stopped forwards NOT restored
- [x] SFTP incomplete transfers resume on new session using old session query
- [x] IDE project reopens with file tabs (content re-fetched, not from cache)
- [x] Terminal restored automatically via Key-Driven Reset (no orchestrator involvement)
- [x] `pnpm i18n:check` passes with all new keys

**实现常量（实际代码）**：
| 常量 | 值 |
|------|-----|
| `DEBOUNCE_MS` | 500 |
| `MAX_ATTEMPTS` | 5 |
| `BASE_RETRY_DELAY_MS` | 1,000 |
| `MAX_RETRY_DELAY_MS` | 15,000 |
| `BACKOFF_MULTIPLIER` | 1.5 |
| `MAX_RETAINED_JOBS` | 200 |
| `AUTO_CLEANUP_DELAY_MS` | 30,000 |
| `MAX_PHASE_HISTORY` | 64 |

