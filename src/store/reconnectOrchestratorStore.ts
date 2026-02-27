/**
 * Reconnect Orchestrator Store
 *
 * 统一的前端重连状态机。替代 useConnectionEvents 中分散的防抖/重试逻辑。
 *
 * 管道阶段: snapshot → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done
 *
 * 关键不变量:
 *   1. 每个 nodeId 只有一个活跃 job（幂等）
 *   2. Snapshot 必须在 reconnectCascade 之前执行（resetNodeState 会销毁 forward 规则）
 *   3. Terminal 恢复由 Key-Driven Reset 自动处理，不在管道内
 *   4. 用户手动停止的 forward（status === 'stopped'）不会被恢复
 */

import { create } from 'zustand';
import { api, nodeSftpListIncompleteTransfers, nodeSftpResumeTransfer, nodeGetState } from '../lib/api';
import { useSessionTreeStore } from './sessionTreeStore';
import { useIdeStore } from './ideStore';
import { useToastStore } from '../hooks/useToast';
import { slog } from '../lib/structuredLog';
import i18n from '../i18n';
import type { ForwardRule, IncompleteTransferInfo } from '../types';

// ═══════════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════════

export type ReconnectPhase =
  | 'queued'
  | 'snapshot'
  | 'grace-period'
  | 'ssh-connect'
  | 'await-terminal'
  | 'restore-forwards'
  | 'resume-transfers'
  | 'restore-ide'
  | 'verify'
  | 'done'
  | 'failed'
  | 'cancelled';

export type ReconnectSnapshot = {
  nodeId: string;
  /** Timestamp when the snapshot was taken — used to detect user actions after snapshot */
  snapshotAt: number;
  /** Forward rules per node, captured BEFORE resetNodeState destroys them */
  forwardRules: Array<{
    nodeId: string;
    rules: ForwardRule[];
  }>;
  /** Old terminal session IDs (for querying incomplete SFTP transfers) */
  oldTerminalSessionIds: string[];
  /** Per-node mapping of old terminal session IDs, keyed by nodeId */
  perNodeOldSessionIds: Map<string, string[]>;
  /** Incomplete SFTP transfers captured BEFORE resetNodeState destroys old sessions */
  incompleteTransfers: Array<{
    oldSessionId: string;
    transfers: IncompleteTransferInfo[];
  }>;
  /** Per-node mapping of old SSH connectionIds for grace period recovery probing */
  oldConnectionIds: Map<string, string>;
  /** IDE state if the IDE was open for a node in this subtree */
  ideSnapshot?: {
    projectPath: string;
    tabPaths: string[];
    connectionId: string;
    /** Dirty file contents captured at snapshot time, keyed by path */
    dirtyContents: Record<string, string>;
  };
};

export type PhaseResult = 'ok' | 'failed' | 'skipped' | 'running';

export type PhaseEvent = {
  phase: ReconnectPhase;
  startedAt: number;
  endedAt?: number;
  result: PhaseResult;
  detail?: string;
};

export type ReconnectJob = {
  nodeId: string;
  nodeName: string;
  status: ReconnectPhase;
  attempt: number;
  maxAttempts: number;
  startedAt: number;
  endedAt?: number;
  error?: string;
  snapshot: ReconnectSnapshot;
  abortController: AbortController;
  restoredCount: number;
  /** Append-only phase event log for time-travel debugging */
  phaseHistory: PhaseEvent[];
};

interface OrchestratorState {
  jobs: Map<string, ReconnectJob>;
  /** Serializable view for React subscribers */
  jobEntries: Array<[string, ReconnectJob]>;
}

interface OrchestratorActions {
  scheduleReconnect: (nodeId: string) => void;
  cancel: (nodeId: string) => void;
  cancelAll: () => void;
  clearCompleted: () => void;
  getJob: (nodeId: string) => ReconnectJob | undefined;
}

// ═══════════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════════

const DEBOUNCE_MS = 500;
const MAX_ATTEMPTS = 5;
const BASE_RETRY_DELAY_MS = 1000;
const MAX_RETRY_DELAY_MS = 15_000;
const BACKOFF_MULTIPLIER = 1.5;

/**
 * Grace Period: 在销毁旧 SSH session 前尝试复用已有连接。
 *
 * 核心价值：如果网络只是短暂中断（WiFi 切换、VPN 重连、短暂抖动），
 * SSH 连接可能仍然存活。通过 grace period 探测，可以无损恢复连接，
 * TUI 应用（yazi、vim、htop）和终端状态得以保留。
 *
 * 流程：
 *   1. link_down → Orchestrator 开始管道
 *   2. Phase 0: Snapshot（捕获旧状态、旧 connectionId）
 *   3. Phase NEW: Grace Period — 每 GRACE_PROBE_INTERVAL_MS 探测一次旧连接
 *      a) 如果探测返回 "alive" → 连接恢复！跳过 ssh-connect，直接标记 done
 *      b) 如果 GRACE_PERIOD_MS 超时 → 连接确认死亡，进入焦土重连
 *   4. Phase 1+: 原有的 ssh-connect → await-terminal → ... 焦土重连流程
 */
const GRACE_PERIOD_MS = 30_000;
const GRACE_PROBE_INTERVAL_MS = 3_000;

/** Max completed/failed/cancelled jobs to retain before auto-eviction */
const MAX_RETAINED_JOBS = 200;
/** Delay (ms) before auto-removing a terminal (done/failed/cancelled) job */
const AUTO_CLEANUP_DELAY_MS = 30_000;
/** Maximum phaseHistory entries per job (ring-buffer style) */
const MAX_PHASE_HISTORY = 64;

/**
 * Adaptive backoff with ±20% jitter.
 * delay = min(BASE × MULTIPLIER^(attempt-1), MAX) × (0.8 ~ 1.2)
 */
function calculateBackoff(attempt: number): number {
  const base = Math.min(
    BASE_RETRY_DELAY_MS * Math.pow(BACKOFF_MULTIPLIER, Math.max(0, attempt - 1)),
    MAX_RETRY_DELAY_MS,
  );
  const jitter = 0.8 + Math.random() * 0.4; // ±20%
  return Math.round(base * jitter);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Module-level state (not reactive — internal bookkeeping)
// ═══════════════════════════════════════════════════════════════════════════════

/** Pending nodeIds accumulated during debounce window */
const pendingNodeIds = new Set<string>();

/** Debounce timer handle */
let debounceTimer: ReturnType<typeof setTimeout> | null = null;

/** Pipeline execution lock */
let isRunning = false;
const MAX_REQUEUE = 120;
const requeueCount = new Map<string, number>();

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/** Sync jobEntries from jobs map so React can subscribe */
function syncEntries(jobs: Map<string, ReconnectJob>): Array<[string, ReconnectJob]> {
  return Array.from(jobs.entries());
}

function toast(
  titleKey: string,
  variant: 'default' | 'success' | 'error' | 'warning' = 'default',
  params?: Record<string, string | number>,
) {
  useToastStore.getState().addToast({
    title: i18n.t(titleKey, params ?? {}),
    variant,
    duration: variant === 'error' ? 8000 : 5000,
  });
}

// ═══════════════════════════════════════════════════════════════════════════════
// Store
// ═══════════════════════════════════════════════════════════════════════════════

export const useReconnectOrchestratorStore = create<OrchestratorState & OrchestratorActions>(
  (set, get) => ({
    // ─── State ───
    jobs: new Map(),
    jobEntries: [],

    // ─── Selectors ───
    getJob: (nodeId: string) => get().jobs.get(nodeId),

    // ─── Actions ───

    scheduleReconnect: (nodeId: string) => {
      console.log(`[Orchestrator] scheduleReconnect(${nodeId})`);

      // Idempotent: skip if job already running for this node
      const existing = get().jobs.get(nodeId);
      if (existing && !isTerminal(existing.status)) {
        console.log(`[Orchestrator] Job already exists for ${nodeId} (${existing.status}), skipping`);
        return;
      }

      pendingNodeIds.add(nodeId);

      // Reset debounce timer
      if (debounceTimer) clearTimeout(debounceTimer);

      debounceTimer = setTimeout(() => {
        debounceTimer = null;
        flushPending();
      }, DEBOUNCE_MS);
    },

    cancel: (nodeId: string) => {
      const jobs = new Map(get().jobs);
      const job = jobs.get(nodeId);

      // Clear from pending debounce set (even if no active job yet)
      pendingNodeIds.delete(nodeId);

      // Also clear descendants from pending set
      const treeStore = useSessionTreeStore.getState();
      const descendants = treeStore.getDescendants(nodeId);
      for (const desc of descendants) {
        pendingNodeIds.delete(desc.id);
      }

      if (!job || isTerminal(job.status)) return;

      job.abortController.abort();
      job.status = 'cancelled';
      job.endedAt = Date.now();
      jobs.set(nodeId, { ...job });

      // Also cancel descendant jobs
      for (const desc of descendants) {
        const dJob = jobs.get(desc.id);
        if (dJob && !isTerminal(dJob.status)) {
          dJob.abortController.abort();
          dJob.status = 'cancelled';
          dJob.endedAt = Date.now();
          jobs.set(desc.id, { ...dJob });
        }
      }

      set({ jobs, jobEntries: syncEntries(jobs) });
      toast('connections.reconnect.cancelled', 'default');
      console.log(`[Orchestrator] Cancelled job for ${nodeId}`);
    },

    cancelAll: () => {
      const jobs = new Map(get().jobs);
      let cancelled = 0;
      for (const [, job] of jobs) {
        if (!isTerminal(job.status)) {
          job.abortController.abort();
          job.status = 'cancelled';
          job.endedAt = Date.now();
          cancelled++;
        }
      }
      if (cancelled > 0) {
        set({ jobs, jobEntries: syncEntries(jobs) });
        toast('connections.reconnect.cancelled', 'default');
      }

      // Also clear pending
      pendingNodeIds.clear();
      if (debounceTimer) {
        clearTimeout(debounceTimer);
        debounceTimer = null;
      }
    },

    clearCompleted: () => {
      const jobs = new Map(get().jobs);
      for (const [nodeId, job] of jobs) {
        if (isTerminal(job.status)) {
          jobs.delete(nodeId);
        }
      }
      set({ jobs, jobEntries: syncEntries(jobs) });
    },
  })
);

// ═══════════════════════════════════════════════════════════════════════════════
// Pipeline Implementation (module-level, not in store to avoid stale closures)
// ═══════════════════════════════════════════════════════════════════════════════

function isTerminal(phase: ReconnectPhase): boolean {
  return phase === 'done' || phase === 'failed' || phase === 'cancelled';
}

function updateJob(nodeId: string, patch: Partial<ReconnectJob>) {
  const store = useReconnectOrchestratorStore.getState();
  const jobs = new Map(store.jobs);
  const job = jobs.get(nodeId);
  if (!job) return;
  const updated = { ...job, ...patch };
  jobs.set(nodeId, updated);
  useReconnectOrchestratorStore.setState({ jobs, jobEntries: syncEntries(jobs) });
}

function getJob(nodeId: string): ReconnectJob | undefined {
  return useReconnectOrchestratorStore.getState().jobs.get(nodeId);
}

/** Record entry into a pipeline phase */
function enterPhase(nodeId: string, phase: ReconnectPhase) {
  const job = getJob(nodeId);
  if (!job) return;
  const history = [...job.phaseHistory, { phase, startedAt: Date.now(), result: 'running' as PhaseResult }];
  // Cap phaseHistory to prevent unbounded growth in long-running retry loops
  const trimmed = history.length > MAX_PHASE_HISTORY ? history.slice(-MAX_PHASE_HISTORY) : history;
  updateJob(nodeId, { status: phase, phaseHistory: trimmed });

  slog({
    component: 'Orchestrator',
    event: 'phase:enter',
    nodeId,
    phase,
  });
}

/** Record exit from the current pipeline phase */
function exitPhase(nodeId: string, result: PhaseResult, detail?: string) {
  const job = getJob(nodeId);
  if (!job) return;
  const history = [...job.phaseHistory];
  let elapsedMs: number | undefined;
  // Find the last 'running' entry and close it
  for (let i = history.length - 1; i >= 0; i--) {
    if (history[i].result === 'running') {
      const endedAt = Date.now();
      elapsedMs = endedAt - history[i].startedAt;
      history[i] = { ...history[i], endedAt, result, detail };
      break;
    }
  }
  updateJob(nodeId, { phaseHistory: history });

  slog({
    component: 'Orchestrator',
    event: 'phase:exit',
    nodeId,
    phase: job.status,
    elapsedMs,
    outcome: result === 'ok' ? 'ok' : result === 'skipped' ? 'skipped' : 'error',
    detail,
  });
}

/**
 * Flush pending nodeIds → group into distinct subtrees → create one job per subtree root.
 *
 * Nodes that share an ancestor already in the set are subsumed by that ancestor's job
 * (reconnectCascade handles descendants). Nodes in unrelated subtrees each get their own job.
 */
function flushPending() {
  if (pendingNodeIds.size === 0) return;

  const nodeIds = Array.from(pendingNodeIds);
  pendingNodeIds.clear();

  const treeStore = useSessionTreeStore.getState();

  const nodes = nodeIds
    .map((id) => treeStore.getNode(id))
    .filter((n): n is NonNullable<typeof n> => n !== undefined);

  if (nodes.length === 0) {
    console.warn('[Orchestrator] No valid nodes in pending set');
    return;
  }

  // Sort shallowest first
  nodes.sort((a, b) => a.depth - b.depth);

  // Determine distinct subtree roots: a node is a root if none of the already-selected
  // roots is its ancestor. We check by walking parentId chains.
  const selectedRoots: Array<typeof nodes[0]> = [];
  const selectedRootIds = new Set<string>();

  for (const node of nodes) {
    // Walk up the parent chain to see if any selected root covers this node
    let coveredByExisting = false;
    let cursor = node;
    while (cursor.parentId) {
      if (selectedRootIds.has(cursor.parentId)) {
        coveredByExisting = true;
        break;
      }
      const parent = treeStore.getNode(cursor.parentId);
      if (!parent) break;
      cursor = parent;
    }
    // Also check if this exact node is already a selected root
    if (selectedRootIds.has(node.id)) coveredByExisting = true;

    if (!coveredByExisting) {
      selectedRoots.push(node);
      selectedRootIds.add(node.id);
    }
  }

  console.log(`[Orchestrator] Flushing ${nodeIds.length} pending -> ${selectedRoots.length} subtree root(s)`);

  const jobs = new Map(useReconnectOrchestratorStore.getState().jobs);
  const newJobIds: string[] = [];

  for (const rootNode of selectedRoots) {
    const rootNodeId = rootNode.id;

    // Idempotent check
    const existing = getJob(rootNodeId);
    if (existing && !isTerminal(existing.status)) {
      console.log(`[Orchestrator] Job already running for root ${rootNodeId}, skipping`);
      continue;
    }

    const job: ReconnectJob = {
      nodeId: rootNodeId,
      nodeName: rootNode.displayName || `${rootNode.username}@${rootNode.host}`,
      status: 'queued',
      attempt: 0,
      maxAttempts: MAX_ATTEMPTS,
      startedAt: Date.now(),
      snapshot: {
        nodeId: rootNodeId,
        snapshotAt: Date.now(),
        forwardRules: [],
        oldTerminalSessionIds: [],
        perNodeOldSessionIds: new Map(),
        incompleteTransfers: [],
        oldConnectionIds: new Map(),
      },
      abortController: new AbortController(),
      restoredCount: 0,
      phaseHistory: [],
    };

    jobs.set(rootNodeId, job);
    newJobIds.push(rootNodeId);
    toast('connections.reconnect.starting', 'default', { name: job.nodeName });
  }

  useReconnectOrchestratorStore.setState({ jobs, jobEntries: syncEntries(jobs) });

  // Start pipelines (sequentially via the isRunning lock)
  for (const id of newJobIds) {
    runPipeline(id);
  }
}

/** Main pipeline runner with retry support */
async function runPipeline(nodeId: string) {
  if (isRunning) {
    const count = (requeueCount.get(nodeId) ?? 0) + 1;
    if (count > MAX_REQUEUE) {
      console.warn(`[Orchestrator] Max re-queue (${MAX_REQUEUE}) reached for ${nodeId}, marking failed`);
      requeueCount.delete(nodeId);
      updateJob(nodeId, { status: 'failed', error: 'Pipeline queue exhausted', endedAt: Date.now() });
      return;
    }
    requeueCount.set(nodeId, count);
    console.log(`[Orchestrator] Pipeline busy, re-queuing ${nodeId} (${count}/${MAX_REQUEUE})`);
    setTimeout(() => runPipeline(nodeId), calculateBackoff(1));
    return;
  }

  isRunning = true;
  requeueCount.delete(nodeId);

  try {
    const job = getJob(nodeId);
    if (!job || isTerminal(job.status)) return;

    const signal = job.abortController.signal;

    // Phase 0: Snapshot
    if (signal.aborted) return markCancelled(nodeId);
    await phaseSnapshot(nodeId);

    // Phase 0.5: Grace Period — 尝试复用旧连接（保留 TUI 应用）
    if (signal.aborted) return markCancelled(nodeId);
    const recovered = await phaseGracePeriod(nodeId);
    if (recovered) {
      // 连接已恢复！跳过焦土重连，TUI 应用得以保留
      updateJob(nodeId, { status: 'done', endedAt: Date.now() });
      toast('connections.reconnect.recovered', 'success');
      console.log(`[Orchestrator] 🎉 Connection RECOVERED during grace period for ${nodeId} — TUI apps preserved`);
      return;
    }

    // Phase 1: SSH Connect (Grace Period 未能恢复 → 焦土重连)
    if (signal.aborted) return markCancelled(nodeId);
    const sshOk = await phaseSshConnect(nodeId);
    if (!sshOk) return; // Already marked failed with retry logic

    // Phase 2: Await Terminal
    if (signal.aborted) return markCancelled(nodeId);
    await phaseAwaitTerminal(nodeId);

    // Phase 3: Restore Forwards
    if (signal.aborted) return markCancelled(nodeId);
    await phaseRestoreForwards(nodeId);

    // Phase 4: Resume Transfers
    if (signal.aborted) return markCancelled(nodeId);
    await phaseResumeTransfers(nodeId);

    // Phase 5: Restore IDE
    if (signal.aborted) return markCancelled(nodeId);
    await phaseRestoreIde(nodeId);

    // Phase 6: Verify Consistency
    if (signal.aborted) return markCancelled(nodeId);
    await phaseVerifyConsistency(nodeId);

    // Done!
    const finalJob = getJob(nodeId);
    updateJob(nodeId, { status: 'done', endedAt: Date.now() });
    toast('connections.reconnect.completed', 'success', {
      count: finalJob?.restoredCount ?? 0,
    });
    console.log(`[Orchestrator] Pipeline done for ${nodeId}, restored ${finalJob?.restoredCount ?? 0} services`);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    console.error(`[Orchestrator] Unexpected pipeline error for ${nodeId}:`, msg);
    exitPhase(nodeId, 'failed', msg);
    updateJob(nodeId, { status: 'failed', error: msg, endedAt: Date.now() });
    toast('connections.reconnect.failed', 'error', { error: msg });
  } finally {
    isRunning = false;
    // Schedule auto-cleanup for terminal jobs
    scheduleAutoCleanup(nodeId);
  }
}

function markCancelled(nodeId: string) {
  exitPhase(nodeId, 'failed', 'cancelled');
  updateJob(nodeId, { status: 'cancelled', endedAt: Date.now() });
  toast('connections.reconnect.cancelled', 'default');
  scheduleAutoCleanup(nodeId);
}

/**
 * Schedule removal of a terminal job after a delay.
 * Also enforces MAX_RETAINED_JOBS hard cap with LRU eviction.
 * Deduplicates: only one pending timer per nodeId+startedAt pair.
 */
const pendingCleanups = new Set<string>();

function scheduleAutoCleanup(nodeId: string) {
  const job = getJob(nodeId);
  if (!job || !isTerminal(job.status)) return;

  // Capture the job's startedAt so the timer only removes *this* job instance.
  const jobStartedAt = job.startedAt;
  const dedupeKey = `${nodeId}:${jobStartedAt}`;

  // Skip if a timer is already pending for this exact job instance
  if (pendingCleanups.has(dedupeKey)) return;
  pendingCleanups.add(dedupeKey);

  setTimeout(() => {
    pendingCleanups.delete(dedupeKey);
    const store = useReconnectOrchestratorStore.getState();
    const jobs = new Map(store.jobs);
    const current = jobs.get(nodeId);
    // Only remove if still terminal AND same job instance (startedAt matches)
    if (current && isTerminal(current.status) && current.startedAt === jobStartedAt) {
      jobs.delete(nodeId);
      useReconnectOrchestratorStore.setState({ jobs, jobEntries: syncEntries(jobs) });
    }
  }, AUTO_CLEANUP_DELAY_MS);

  // Enforce hard cap: evict oldest terminal jobs if over limit
  const store = useReconnectOrchestratorStore.getState();
  const terminalJobs = Array.from(store.jobs.entries())
    .filter(([, j]) => isTerminal(j.status))
    .sort((a, b) => (a[1].endedAt ?? 0) - (b[1].endedAt ?? 0));

  if (terminalJobs.length > MAX_RETAINED_JOBS) {
    const jobs = new Map(store.jobs);
    const toEvict = terminalJobs.length - MAX_RETAINED_JOBS;
    for (let i = 0; i < toEvict; i++) {
      jobs.delete(terminalJobs[i][0]);
    }
    useReconnectOrchestratorStore.setState({ jobs, jobEntries: syncEntries(jobs) });
  }
}

// ─── Phase 0: Snapshot ───────────────────────────────────────────────────────

async function phaseSnapshot(nodeId: string) {
  enterPhase(nodeId, 'snapshot');
  console.log(`[Orchestrator] Phase: snapshot for ${nodeId}`);

  const treeStore = useSessionTreeStore.getState();
  const node = treeStore.getNode(nodeId);
  if (!node) throw new Error(`Node ${nodeId} not found`);

  // Collect all affected nodes (self + descendants)
  const descendants = treeStore.getDescendants(nodeId);
  const allNodes = [node, ...descendants];

  // Collect old terminal session IDs (per-node for deterministic mapping)
  const oldTerminalSessionIds: string[] = [];
  const perNodeOldSessionIds = new Map<string, string[]>();
  for (const n of allNodes) {
    const nodeSessionIds: string[] = [];
    const termIds = treeStore.nodeTerminalMap.get(n.id) || [];
    nodeSessionIds.push(...termIds);
    if (n.terminalSessionId && !termIds.includes(n.terminalSessionId)) {
      nodeSessionIds.push(n.terminalSessionId);
    }
    if (nodeSessionIds.length > 0) {
      perNodeOldSessionIds.set(n.id, nodeSessionIds);
    }
    oldTerminalSessionIds.push(...nodeSessionIds);
  }

  // Snapshot forward rules (BEFORE resetNodeState destroys them)
  // Node-first: query forwards by nodeId via NodeRouter
  const forwardRules: ReconnectSnapshot['forwardRules'] = [];
  for (const n of allNodes) {
    try {
      const rules = await api.nodeListForwards(n.id);
      // Only keep rules that user intended to be running (exclude user-stopped)
      const activeRules = rules.filter((r) => r.status !== 'stopped');
      if (activeRules.length > 0) {
        forwardRules.push({ nodeId: n.id, rules: activeRules });
      }
    } catch (e) {
      // Node may not have forwarding initialized — that's ok, skip
      console.warn(`[Orchestrator] Failed to snapshot forwards for node ${n.id}:`, e);
    }
  }

  // Snapshot incomplete SFTP transfers BEFORE resetNodeState destroys old sessions
  // Node-first: use nodeId to query transfers via node router
  const incompleteTransfers: ReconnectSnapshot['incompleteTransfers'] = [];
  for (const n of allNodes) {
    try {
      const transfers = await nodeSftpListIncompleteTransfers(n.id);
      const resumable = transfers.filter((t) => t.can_resume);
      if (resumable.length > 0) {
        // Store nodeId as oldSessionId for backward compatibility with ReconnectSnapshot type
        incompleteTransfers.push({ oldSessionId: n.id, transfers: resumable });
      }
    } catch (e) {
      // Node SFTP may not be initialized — that's ok
      console.warn(`[Orchestrator] Failed to snapshot incomplete transfers for node ${n.id}:`, e);
    }
  }

  // Snapshot IDE state
  let ideSnapshot: ReconnectSnapshot['ideSnapshot'] | undefined;
  const ideState = useIdeStore.getState();
  if (ideState.nodeId && ideState.project) {
    // Check if IDE's connection belongs to one of the affected nodes
    const ideNodeId = ideState.nodeId;
    const isAffected = allNodes.some((n) => n.id === ideNodeId);
    if (isAffected) {
      const dirtyContents: Record<string, string> = {};
      for (const tab of ideState.tabs) {
        if (tab.isDirty && tab.content !== null) {
          dirtyContents[tab.path] = tab.content;
        }
      }
      ideSnapshot = {
        projectPath: ideState.project.rootPath,
        tabPaths: ideState.tabs.map((t) => t.path),
        connectionId: ideState.nodeId,
        dirtyContents,
      };
      console.log(`[Orchestrator] IDE snapshot: project=${ideSnapshot.projectPath}, tabs=${ideSnapshot.tabPaths.length}, dirty=${Object.keys(dirtyContents).length}`);
    }
  }

  // Snapshot old SSH connectionIds for grace period recovery probing
  const oldConnectionIds = new Map<string, string>();
  for (const n of allNodes) {
    const unifiedNode = treeStore.getNode(n.id);
    if (unifiedNode?.runtime.connectionId) {
      oldConnectionIds.set(n.id, unifiedNode.runtime.connectionId);
    }
  }

  updateJob(nodeId, {
    snapshot: {
      nodeId,
      snapshotAt: Date.now(),
      forwardRules,
      oldTerminalSessionIds,
      perNodeOldSessionIds,
      incompleteTransfers,
      oldConnectionIds,
      ideSnapshot,
    },
  });
  const fwCount = forwardRules.reduce((s, e) => s + e.rules.length, 0);
  const txCount = incompleteTransfers.reduce((s, e) => s + e.transfers.length, 0);
  exitPhase(nodeId, 'ok', `${fwCount} forwards, ${txCount} transfers, ${oldConnectionIds.size} connections, ${ideSnapshot ? 'IDE' : 'no IDE'}`);
}

// ─── Phase 0.5: Grace Period ─────────────────────────────────────────────────

/**
 * Grace Period: 在销毁旧 SSH session 前，反复探测旧连接是否恢复。
 *
 * 核心价值：
 *   - WiFi 切换、VPN 重连、短暂网络抖动 → SSH 连接可能仍然存活
 *   - 如果连接恢复 → 跳过焦土重连，TUI 应用（yazi、vim、htop）和终端缓冲区完整保留
 *   - 如果超时 → 进入 ssh-connect 焦土重连（现有行为）
 *
 * 探测机制：
 *   每 GRACE_PROBE_INTERVAL_MS 对旧 connectionId 发送 SSH keepalive。
 *   后端 `probe_single_connection` 对 LinkDown 连接做 ping：
 *   - 成功 → 自动恢复为 Active，重启心跳，发射 `connected` 事件
 *   - 失败 → 保持 LinkDown
 *
 * @returns true = 连接已恢复（跳过焦土重连），false = 超时（继续焦土重连）
 */
async function phaseGracePeriod(nodeId: string): Promise<boolean> {
  const job = getJob(nodeId);
  if (!job) return false;

  // 获取旧 connectionId（snapshot 阶段已捕获）
  const rootConnectionId = job.snapshot.oldConnectionIds.get(nodeId);
  if (!rootConnectionId) {
    console.log(`[Orchestrator] Grace period: no old connectionId for ${nodeId}, skipping`);
    return false;
  }

  enterPhase(nodeId, 'grace-period');
  console.log(`[Orchestrator] Phase: grace-period for ${nodeId} (max ${GRACE_PERIOD_MS / 1000}s, probe every ${GRACE_PROBE_INTERVAL_MS / 1000}s)`);

  const signal = job.abortController.signal;
  const startedAt = Date.now();
  let probeCount = 0;

  while (Date.now() - startedAt < GRACE_PERIOD_MS) {
    if (signal.aborted) {
      exitPhase(nodeId, 'failed', 'cancelled');
      return false;
    }

    probeCount++;
    const elapsed = Math.round((Date.now() - startedAt) / 1000);

    try {
      slog({
        component: 'Orchestrator',
        event: 'grace:probe',
        nodeId,
        detail: `probe #${probeCount} at ${elapsed}s`,
      });

      const result = await api.probeSingleConnection(rootConnectionId);

      if (result === 'alive') {
        // 连接恢复！后端已自动转为 Active 并重启心跳
        slog({
          component: 'Orchestrator',
          event: 'grace:recovered',
          nodeId,
          outcome: 'ok',
          detail: `recovered after ${probeCount} probes (${elapsed}s)`,
        });
        console.log(`[Orchestrator] ✅ Grace period: connection ${rootConnectionId} RECOVERED after ${probeCount} probes (${elapsed}s)`);

        // 也探测子节点的连接（级联恢复）
        const allConnectionIds = Array.from(job.snapshot.oldConnectionIds.entries());
        let childRecovered = 0;
        for (const [childNodeId, childConnectionId] of allConnectionIds) {
          if (childNodeId === nodeId) continue; // 根节点已恢复
          try {
            const childResult = await api.probeSingleConnection(childConnectionId);
            if (childResult === 'alive') childRecovered++;
          } catch {
            // 子节点恢复失败不影响整体
          }
        }
        if (childRecovered > 0) {
          console.log(`[Orchestrator] Grace period: ${childRecovered} child connection(s) also recovered`);
        }

        // 清除所有受影响节点的 link-down 标记
        const treeStore = useSessionTreeStore.getState();
        treeStore.clearLinkDown(nodeId);
        const descendants = treeStore.getDescendants(nodeId);
        for (const desc of descendants) {
          treeStore.clearLinkDown(desc.id);
        }

        exitPhase(nodeId, 'ok', `recovered after ${probeCount} probes (${elapsed}s)`);
        return true;
      }

      if (result === 'not_found') {
        // 连接已被清理，无需继续等待
        console.log(`[Orchestrator] Grace period: connection ${rootConnectionId} not found, stopping probe`);
        exitPhase(nodeId, 'failed', 'connection not found');
        return false;
      }

      // result === 'dead' or 'not_applicable' → 继续等待
      console.debug(`[Orchestrator] Grace period: probe #${probeCount} → ${result} (${elapsed}s/${GRACE_PERIOD_MS / 1000}s)`);
    } catch (e) {
      console.warn(`[Orchestrator] Grace period: probe #${probeCount} error:`, e);
    }

    // 等待下一次探测
    await sleep(GRACE_PROBE_INTERVAL_MS);
  }

  // 超时 → 连接确认死亡
  const totalElapsed = Math.round((Date.now() - startedAt) / 1000);
  console.log(`[Orchestrator] Grace period expired for ${nodeId} after ${probeCount} probes (${totalElapsed}s) → proceeding to scorched-earth reconnect`);
  slog({
    component: 'Orchestrator',
    event: 'grace:expired',
    nodeId,
    outcome: 'error',
    detail: `${probeCount} probes over ${totalElapsed}s`,
  });
  exitPhase(nodeId, 'failed', `expired after ${probeCount} probes (${totalElapsed}s)`);
  return false;
}

// ─── Phase 1: SSH Connect ────────────────────────────────────────────────────

async function phaseSshConnect(nodeId: string): Promise<boolean> {
  const job = getJob(nodeId);
  if (!job) return false;

  enterPhase(nodeId, 'ssh-connect');
  updateJob(nodeId, { attempt: job.attempt + 1 });
  console.log(`[Orchestrator] Phase: ssh-connect for ${nodeId} (attempt ${job.attempt + 1})`);

  const treeStore = useSessionTreeStore.getState();

  try {
    const reconnected = await treeStore.reconnectCascade(nodeId);
    console.log(`[Orchestrator] SSH reconnect succeeded: ${reconnected.length} nodes`);
    exitPhase(nodeId, 'ok', `${reconnected.length} nodes`);
    toast('connections.reconnect.ssh_restored', 'default');
    return true;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);

    // Non-retryable whitelist: these errors won't resolve by retrying
    const isNonRetryable =
      msg.includes('Authentication failed') ||
      msg.includes('HostKeyMismatch') ||
      msg.includes('host key') ||
      msg.includes('Permission denied') ||
      msg.includes('USER_CANCELLED') ||
      msg.includes('cancelled');

    if (!isNonRetryable && (job.attempt + 1) < job.maxAttempts) {
      const delay = calculateBackoff(job.attempt + 1);
      console.log(`[Orchestrator] Retryable error, will retry in ${delay}ms (attempt ${job.attempt + 1}/${job.maxAttempts})`);
      await sleep(delay);

      // Check if cancelled during sleep
      if (job.abortController.signal.aborted) {
        markCancelled(nodeId);
        return false;
      }

      // Check if node still needs reconnect
      const currentNode = treeStore.getNode(nodeId);
      if (
        currentNode &&
        (currentNode.runtime.status === 'link-down' ||
          currentNode.runtime.status === 'idle' ||
          currentNode.runtime.status === 'error')
      ) {
        return phaseSshConnect(nodeId);
      }
      // Node recovered on its own
      console.log(`[Orchestrator] Node ${nodeId} status changed, skipping retry`);
      exitPhase(nodeId, 'ok', 'recovered on its own');
      return true;
    }

    // Non-retryable or exhausted retries
    console.error(`[Orchestrator] SSH reconnect failed permanently: ${msg}`);
    exitPhase(nodeId, 'failed', msg);
    updateJob(nodeId, { status: 'failed', error: msg, endedAt: Date.now() });
    toast('connections.reconnect.failed', 'error', { error: msg });
    return false;
  }
}

// ─── Phase 2: Await Terminal ─────────────────────────────────────────────────

async function phaseAwaitTerminal(nodeId: string) {
  enterPhase(nodeId, 'await-terminal');
  console.log(`[Orchestrator] Phase: await-terminal for ${nodeId}`);

  const treeStore = useSessionTreeStore.getState();
  const job = getJob(nodeId);
  if (!job) {
    exitPhase(nodeId, 'skipped', 'job missing');
    return;
  }

  const node = treeStore.getNode(nodeId);
  if (!node) {
    exitPhase(nodeId, 'skipped', 'node missing');
    return;
  }

  const { snapshot } = job;

  // Determine which nodes NEED a terminal session for restore phases
  // (nodes that had forwards or incomplete transfers in the snapshot)
  const nodesNeedingSession = new Set<string>();
  for (const entry of snapshot.forwardRules) {
    nodesNeedingSession.add(entry.nodeId);
  }
  for (const entry of snapshot.incompleteTransfers) {
    for (const [nId, oldIds] of snapshot.perNodeOldSessionIds) {
      if (oldIds.includes(entry.oldSessionId)) {
        nodesNeedingSession.add(nId);
      }
    }
  }

  const { useAppStore } = await import('./appStore');

  // Process ALL affected nodes (root + descendants), not just the root.
  // For each node that had open terminal tab(s), create new terminal(s) and
  // patch the pane tree so TerminalView remounts with a valid session.
  const allNodes = [node, ...treeStore.getDescendants(nodeId)];
  let terminalTabsFixed = 0;

  for (const n of allNodes) {
    if (job.abortController.signal.aborted) return;

    const oldSessionIds = snapshot.perNodeOldSessionIds.get(n.id);
    if (!oldSessionIds || oldSessionIds.length === 0) continue;

    const oldSessionIdSet = new Set(oldSessionIds);
    const { tabs } = useAppStore.getState();
    const hasTerminalTab = tabs.some((tab) => {
      if (tab.sessionId && oldSessionIdSet.has(tab.sessionId)) return true;
      if (!tab.rootPane) return false;
      return paneTreeHasAnySession(tab.rootPane, oldSessionIdSet);
    });

    if (!hasTerminalTab) continue;

    // Check if this node has been reconnected (connectNodeWithAncestors should
    // have connected the root; children may still be pending).
    const currentNode = useSessionTreeStore.getState().getNode(n.id);
    if (!currentNode?.runtime.connectionId) {
      console.debug(`[Orchestrator] Node ${n.id} not yet connected, skipping terminal creation`);
      continue;
    }

    try {
      // For split-view: each distinct old sessionId needs its own new terminal
      // to avoid merging two independent shells into one.
      if (oldSessionIds.length > 1) {
        // Multiple terminals — create one per old sessionId
        for (const oldId of oldSessionIds) {
          // Check if this specific old sessionId is still referenced by any pane
          const { tabs: currentTabs } = useAppStore.getState();
          const isReferenced = currentTabs.some((tab) => {
            if (tab.sessionId === oldId) return true;
            if (!tab.rootPane) return false;
            return paneTreeHasAnySession(tab.rootPane, new Set([oldId]));
          });
          if (!isReferenced) continue;

          const newId = await useSessionTreeStore.getState().createTerminalForNode(n.id);
          useAppStore.getState().updatePaneSessionId(oldId, newId);
          console.log(`[Orchestrator] Created terminal for node ${n.id}: ${oldId} → ${newId}`);
        }
      } else {
        // Single terminal — common case
        const newSessionId = await useSessionTreeStore.getState().createTerminalForNode(n.id);
        useAppStore.getState().updatePaneSessionId(oldSessionIds[0], newSessionId);
        console.log(`[Orchestrator] Created terminal for node ${n.id}: ${newSessionId}`);
      }
      terminalTabsFixed++;
    } catch (e) {
      console.warn(`[Orchestrator] Failed to create terminal for node ${n.id}:`, e);
    }
  }

  // For nodes that need a session for forward/transfer restore but have no terminal,
  // explicitly create a terminal session so there's a valid session to bind to.
  for (const n of allNodes) {
    if (job.abortController.signal.aborted) return;

    const currentNode = useSessionTreeStore.getState().getNode(n.id);
    if (currentNode?.terminalSessionId) continue; // already has a session
    if (!nodesNeedingSession.has(n.id)) continue; // doesn't need one

    try {
      console.log(`[Orchestrator] Creating terminal for node ${n.id} (needed for forward/transfer restore)`);
      await useSessionTreeStore.getState().createTerminalForNode(n.id);
    } catch (e) {
      console.warn(`[Orchestrator] Failed to create terminal for node ${n.id}:`, e);
    }
  }
  exitPhase(nodeId, 'ok', `fixed ${terminalTabsFixed} terminal tab(s), ${nodesNeedingSession.size} nodes needed sessions`);
}

// ─── Phase 3: Restore Forwards ──────────────────────────────────────────────

async function phaseRestoreForwards(nodeId: string) {
  enterPhase(nodeId, 'restore-forwards');
  const job = getJob(nodeId);
  if (!job) return;

  const { snapshot } = job;
  if (snapshot.forwardRules.length === 0) {
    console.log(`[Orchestrator] No forwards to restore for ${nodeId}`);
    exitPhase(nodeId, 'skipped', 'no forward rules in snapshot');
    return;
  }

  console.log(`[Orchestrator] Phase: restore-forwards for ${nodeId}`);

  // Node-first: no session mapping needed — nodeId resolves to new terminal session via NodeRouter
  let restored = 0;

  for (const entry of snapshot.forwardRules) {
    // Collect existing live forwards to avoid duplicating or resurrecting user-stopped rules
    const liveForwardKeys = new Set<string>();
    try {
      const live = await api.nodeListForwards(entry.nodeId);
      for (const f of live) {
        liveForwardKeys.add(`${f.forward_type}:${f.bind_address}:${f.bind_port}`);
      }
    } catch {
      // Node may not have forwarding initialized yet — that's fine
    }

    for (const rule of entry.rules) {
      if (job.abortController.signal.aborted) return;

      const key = `${rule.forward_type}:${rule.bind_address}:${rule.bind_port}`;

      // Re-check live forwards right before creation to catch user actions during the loop
      try {
        const freshLive = await api.nodeListForwards(entry.nodeId);
        for (const f of freshLive) {
          liveForwardKeys.add(`${f.forward_type}:${f.bind_address}:${f.bind_port}`);
        }
      } catch {
        // Best-effort; fall back to cached set
      }

      if (liveForwardKeys.has(key)) {
        console.log(`[Orchestrator] Forward already exists: ${key}, skipping`);
        continue;
      }

      try {
        await api.nodeCreateForward({
          node_id: entry.nodeId,
          forward_type: rule.forward_type,
          bind_address: rule.bind_address,
          bind_port: rule.bind_port,
          target_host: rule.target_host,
          target_port: rule.target_port,
          description: rule.description,
        });
        restored++;
        liveForwardKeys.add(key); // track so we don't duplicate within the same batch
        console.log(`[Orchestrator] Restored forward: ${rule.bind_address}:${rule.bind_port} -> ${rule.target_host}:${rule.target_port}`);
      } catch (e) {
        console.warn(`[Orchestrator] Failed to restore forward ${rule.id}:`, e);
        // Continue with next rule
      }
    }
  }

  if (restored > 0) {
    updateJob(nodeId, { restoredCount: (job.restoredCount || 0) + restored });
    console.log(`[Orchestrator] Restored ${restored} forward rules`);
  }
  exitPhase(nodeId, 'ok', `restored ${restored} forward(s)`);
}

// ─── Phase 4: Resume Transfers ──────────────────────────────────────────────

async function phaseResumeTransfers(nodeId: string) {
  enterPhase(nodeId, 'resume-transfers');
  const job = getJob(nodeId);
  if (!job) return;

  const { snapshot } = job;
  if (snapshot.oldTerminalSessionIds.length === 0) {
    console.log(`[Orchestrator] No sessions to check for incomplete transfers`);
    exitPhase(nodeId, 'skipped', 'no old sessions');
    return;
  }

  console.log(`[Orchestrator] Phase: resume-transfers for ${nodeId}`);

  // Use pre-snapshotted incomplete transfers (captured before resetNodeState destroyed old sessions)
  if (snapshot.incompleteTransfers.length === 0) {
    console.log(`[Orchestrator] No incomplete transfers in snapshot`);
    exitPhase(nodeId, 'skipped', 'no incomplete transfers in snapshot');
    return;
  }

  // Ensure SFTP sessions are initialized for all affected nodes before resuming
  const treeStore = useSessionTreeStore.getState();
  const rootNode = treeStore.getNode(nodeId);
  if (rootNode) {
    const descendants = treeStore.getDescendants(nodeId);
    const allNodes = [rootNode, ...descendants];
    for (const n of allNodes) {
      if (job.abortController.signal.aborted) return;
      if (!n.sftpSessionId) {
        try {
          await treeStore.openSftpForNode(n.id);
          console.log(`[Orchestrator] Initialized SFTP for node ${n.id}`);
        } catch (e) {
          console.warn(`[Orchestrator] Failed to init SFTP for node ${n.id}:`, e);
        }
      }
    }
  }

  let resumed = 0;

  for (const entry of snapshot.incompleteTransfers) {
    if (job.abortController.signal.aborted) return;

    // entry.oldSessionId is actually nodeId (set in snapshot phase)
    const entryNodeId = entry.oldSessionId;

    for (const transfer of entry.transfers) {
      if (job.abortController.signal.aborted) return;

      // Re-check this specific transfer's status right before resume
      // to catch user cancellations that happened during the restore loop
      try {
        const freshTransfers = await nodeSftpListIncompleteTransfers(entryNodeId);
        const stillExists = freshTransfers.some(
          (t) => t.transfer_id === transfer.transfer_id && t.can_resume,
        );
        if (!stillExists) {
          console.log(`[Orchestrator] Transfer ${transfer.transfer_id} no longer resumable, skipping`);
          continue;
        }
      } catch {
        // Best-effort; proceed with resume attempt (will fail safely if cancelled)
      }

      try {
        await nodeSftpResumeTransfer(entryNodeId, transfer.transfer_id);
        resumed++;
        console.log(`[Orchestrator] Resumed transfer ${transfer.transfer_id}`);
      } catch (e) {
        console.warn(`[Orchestrator] Failed to resume transfer ${transfer.transfer_id}:`, e);
      }
    }
  }

  if (resumed > 0) {
    updateJob(nodeId, { restoredCount: (job.restoredCount || 0) + resumed });
    console.log(`[Orchestrator] Resumed ${resumed} transfers`);
  }
  exitPhase(nodeId, 'ok', `resumed ${resumed} transfer(s)`);
}

// ─── Phase 5: Restore IDE ────────────────────────────────────────────────────

async function phaseRestoreIde(nodeId: string) {
  enterPhase(nodeId, 'restore-ide');
  const job = getJob(nodeId);
  if (!job || !job.snapshot.ideSnapshot) {
    console.log(`[Orchestrator] No IDE state to restore for ${nodeId}`);
    exitPhase(nodeId, 'skipped', 'no IDE snapshot');
    return;
  }

  console.log(`[Orchestrator] Phase: restore-ide for ${nodeId}`);

  const { ideSnapshot } = job.snapshot;
  // ideSnapshot.connectionId actually stores nodeId (see phaseSnapshot),
  // so use it directly instead of resolving through topologyResolver
  const targetNodeId = ideSnapshot.connectionId;
  const treeStore = useSessionTreeStore.getState();
  const ideNode = treeStore.getNode(targetNodeId);

  if (!ideNode) {
    console.warn(`[Orchestrator] IDE node ${targetNodeId} no longer exists`);
    exitPhase(nodeId, 'skipped', 'IDE node no longer exists');
    return;
  }

  const newConnectionId = ideNode.runtime.connectionId;
  const newSftpSessionId = ideNode.sftpSessionId;

  if (!newConnectionId || !newSftpSessionId) {
    console.warn(`[Orchestrator] IDE node ${targetNodeId} missing connectionId or sftpSessionId, skipping IDE restore`);
    exitPhase(nodeId, 'skipped', 'missing connectionId or sftpSessionId');
    return;
  }

  const ideStore = useIdeStore.getState();

  // Respect user intent: if user opened a different project or closed IDE after snapshot, skip
  if (ideStore.project) {
    if (ideStore.project.rootPath !== ideSnapshot.projectPath) {
      console.log(`[Orchestrator] IDE project changed by user (${ideStore.project.rootPath} != ${ideSnapshot.projectPath}), skipping IDE restore`);
      exitPhase(nodeId, 'skipped', 'user changed project');
      return;
    }
    // Same project already open — no need to restore
    console.log(`[Orchestrator] IDE already has the same project open, skipping IDE restore`);
    exitPhase(nodeId, 'skipped', 'same project already open');
    return;
  }

  // IDE is closed — check if it was explicitly closed by user after the snapshot
  // If ideStore has a lastClosedAt timestamp after snapshot, user intentionally closed it
  if (ideStore.lastClosedAt && ideStore.lastClosedAt > job.snapshot.snapshotAt) {
    console.log(`[Orchestrator] IDE was closed by user after snapshot (${ideStore.lastClosedAt} > ${job.snapshot.snapshotAt}), skipping IDE restore`);
    exitPhase(nodeId, 'skipped', 'user closed IDE after snapshot');
    return;
  }

  try {
    // Re-open project using nodeId (node-first)
    await ideStore.openProject(targetNodeId, ideSnapshot.projectPath);

    // Re-open file tabs
    let openedTabs = 0;
    for (const path of ideSnapshot.tabPaths) {
      if (job.abortController.signal.aborted) return;
      try {
        await ideStore.openFile(path);
        openedTabs++;
      } catch (e) {
        console.warn(`[Orchestrator] Failed to reopen IDE tab ${path}:`, e);
      }
    }

    // Restore dirty contents from snapshot
    const dirtyContents = ideSnapshot.dirtyContents ?? {};
    const dirtyPaths = Object.keys(dirtyContents);
    if (dirtyPaths.length > 0) {
      const currentIdeState = useIdeStore.getState();
      const tabUpdates = currentIdeState.tabs.map(tab => {
        const savedContent = dirtyContents[tab.path];
        if (savedContent !== undefined && savedContent !== tab.originalContent) {
          return { ...tab, content: savedContent, isDirty: true };
        }
        return tab;
      });
      useIdeStore.setState({ tabs: tabUpdates });
      console.log(`[Orchestrator] Restored ${dirtyPaths.length} dirty file(s) from snapshot`);
    }

    if (openedTabs > 0) {
      updateJob(nodeId, { restoredCount: (job.restoredCount || 0) + 1 });
    }
    console.log(`[Orchestrator] IDE restored: project=${ideSnapshot.projectPath}, tabs=${openedTabs}`);
    exitPhase(nodeId, 'ok', `project + ${openedTabs} tab(s)`);
  } catch (e) {
    console.warn(`[Orchestrator] Failed to restore IDE project:`, e);
    exitPhase(nodeId, 'failed', e instanceof Error ? e.message : String(e));
  }
}

// ─── Phase 6: Verify Consistency ────────────────────────────────────────────

/**
 * Post-pipeline consistency verification.
 *
 * After all restore phases complete, query the backend for the *actual* state
 * and compare it against what the frontend believes.  Mismatches are logged as
 * structured warnings (slog) so they surface in dev-tools and future telemetry
 * without blocking the user.
 *
 * Checks performed:
 *   1. Node readiness — backend vs SessionTreeStore
 *   2. Forward count — backend vs snapshot restore expectation
 *   3. SFTP readiness — backend vs SessionTreeStore
 */
async function phaseVerifyConsistency(nodeId: string) {
  enterPhase(nodeId, 'verify');
  const job = getJob(nodeId);
  if (!job) {
    exitPhase(nodeId, 'skipped', 'job missing');
    return;
  }

  const drifts: string[] = [];

  try {
    // 1. Node readiness check
    const backendState = await nodeGetState(nodeId);
    const treeStore = useSessionTreeStore.getState();
    const treeNode = treeStore.getNode(nodeId);

    if (backendState.state.readiness !== 'ready') {
      drifts.push(`readiness: backend=${backendState.state.readiness}, expected=ready`);
    }

    // 2. Forward count consistency
    try {
      const liveForwards = await api.nodeListForwards(nodeId);
      const snapshotForwardEntry = job.snapshot.forwardRules.find(e => e.nodeId === nodeId);
      const expectedActive = snapshotForwardEntry
        ? snapshotForwardEntry.rules.length
        : 0;

      // Allow live >= expected (user may have added more during restore)
      const liveActive = liveForwards.filter(f => f.status === 'active').length;
      if (expectedActive > 0 && liveActive < expectedActive) {
        drifts.push(`forwards: live=${liveActive}, snapshotExpected=${expectedActive}`);
      }
    } catch {
      // Node may not have forwarding — not a drift
    }

    // 3. SFTP readiness (if it was ready before disconnect)
    if (backendState.state.sftpReady !== (treeNode?.sftpSessionId != null)) {
      drifts.push(`sftp: backend=${backendState.state.sftpReady}, tree=${treeNode?.sftpSessionId != null}`);
    }

    // 4. Terminal session existence (only if the node had a terminal before disconnect)
    if (job.snapshot.oldTerminalSessionIds.length > 0 && !treeNode?.terminalSessionId) {
      // Check if there's actually a terminal tab open for this node
      const { tabs } = await import('./appStore').then((m) => m.useAppStore.getState());
      const treeStore2 = useSessionTreeStore.getState();
      const hasOpenTab = tabs.some((tab) => {
        if (!tab.rootPane) return false;
        return paneUsesNode(tab.rootPane, nodeId, treeStore2);
      });
      if (hasOpenTab) {
        drifts.push('terminal: no terminalSessionId in tree (tab still open)');
      }
      // If no terminal tab is open, the terminalSessionId being null is expected
    }

  } catch (e) {
    // nodeGetState failed — backend may still be settling
    drifts.push(`verify-error: ${e instanceof Error ? e.message : String(e)}`);
  }

  if (drifts.length > 0) {
    slog({
      component: 'Orchestrator',
      event: 'consistency-drift',
      nodeId,
      outcome: 'error',
      detail: drifts.join('; '),
      drifts,
    });
    console.warn(`[Orchestrator] Consistency drift for ${nodeId}:`, drifts);
    exitPhase(nodeId, 'ok', `${drifts.length} drift(s) detected`);
  } else {
    slog({
      component: 'Orchestrator',
      event: 'consistency-ok',
      nodeId,
      outcome: 'ok',
    });
    exitPhase(nodeId, 'ok', 'all checks passed');
  }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Utility Functions
// ═══════════════════════════════════════════════════════════════════════════════

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Check if a pane tree references a node (for terminal tab detection)
 */
function paneUsesNode(
  pane: { type: string; sessionId?: string; children?: Array<typeof pane> },
  nodeId: string,
  treeStore: ReturnType<typeof useSessionTreeStore.getState>,
): boolean {
  if (pane.type === 'leaf' && pane.sessionId) {
    // Check if this session belongs to the node
    const termNodeId = treeStore.terminalNodeMap.get(pane.sessionId);
    return termNodeId === nodeId;
  }
  if (pane.children) {
    return pane.children.some((child) => paneUsesNode(child, nodeId, treeStore));
  }
  return false;
}

/**
 * Check if a pane tree contains any sessionId from the given set.
 * Used for snapshot-based tab detection when terminalNodeMap has been cleared.
 */
function paneTreeHasAnySession(
  pane: { type: string; sessionId?: string; children?: Array<typeof pane> },
  sessionIds: Set<string>,
): boolean {
  if (pane.type === 'leaf' && pane.sessionId) {
    return sessionIds.has(pane.sessionId);
  }
  if (pane.children) {
    return pane.children.some((child) => paneTreeHasAnySession(child, sessionIds));
  }
  return false;
}
