/**
 * virtualSessionStore — Single Source of Truth for nodeId → session resolution
 *
 * Maintains the canonical mapping from stable nodeId to the current live
 * {activeSessionId, connectionId, generation, state}. All SFTP/IDE components
 * MUST read from this store; business-layer code MUST NOT maintain its own
 * id-mapping.
 *
 * Auto-subscribes to appStore and sessionTreeStore to stay in sync.
 *
 * @module store/virtualSessionStore
 */
import { create } from 'zustand';
import { useAppStore } from './appStore';
import { useSessionTreeStore } from './sessionTreeStore';
import { invariant, softInvariant } from '../lib/invariant';
import { createScopedLogger } from '../lib/structuredLog';
import type { SshConnectionState } from '../types';

const log = createScopedLogger('VirtualSessionStore');

// ============================================================================
// Types
// ============================================================================

export type NodeSessionState = 'resolving' | 'active' | 'disconnected';

export interface NodeSessionEntry {
  /** Current active terminal session ID for this node */
  activeSessionId: string;
  /** Current connection ID for this node */
  connectionId: string;
  /** Monotonically increasing per nodeId — bumps when resolved session changes */
  generation: number;
  /** Current state of the virtual session */
  state: NodeSessionState;
}

// ============================================================================
// Constants
// ============================================================================

const ACTIVE_STATES = new Set<SshConnectionState>(['active', 'idle']);

// ============================================================================
// Internal bookkeeping (outside Zustand to avoid spurious re-renders)
// ============================================================================

/** Reference counts for observed nodeIds */
const _refCounts = new Map<string, number>();
/** Last resolved sessionId per nodeId — for generation tracking */
const _lastSessionIds = new Map<string, string>();

// ============================================================================
// Store interface
// ============================================================================

interface VirtualSessionStoreState {
  /** nodeId → resolved session entry */
  entries: Map<string, NodeSessionEntry>;

  /** Register interest in a nodeId (ref-counted). Call on component mount. */
  register(nodeId: string): void;
  /** Unregister interest. Call on component unmount. */
  unregister(nodeId: string): void;
  /** Get the entry for a nodeId. */
  getEntry(nodeId: string): NodeSessionEntry | undefined;
  /** Re-resolve a single nodeId. */
  resolveNode(nodeId: string): void;
  /** Re-resolve all registered nodeIds in a single batched update. */
  resolveAll(): void;
}

// ============================================================================
// Core resolution (pure function — reads from upstream stores)
// ============================================================================

function resolveNodeId(
  nodeId: string,
  currentGen: number,
): NodeSessionEntry | undefined {
  const terminalIds = useSessionTreeStore.getState().getTerminalsForNode(nodeId);
  if (!terminalIds || terminalIds.length === 0) return undefined;

  const { sessions, connections } = useAppStore.getState();

  for (const sid of terminalIds) {
    const session = sessions.get(sid);
    if (!session?.connectionId) continue;

    const conn = connections.get(session.connectionId);
    if (!conn) continue;

    const connState = typeof conn.state === 'string' ? conn.state : undefined;
    if (connState && ACTIVE_STATES.has(connState)) {
      // Bump generation when the resolved sessionId rotates
      const lastSid = _lastSessionIds.get(nodeId);
      const gen = lastSid && lastSid !== sid ? currentGen + 1 : currentGen;
      _lastSessionIds.set(nodeId, sid);

      return {
        activeSessionId: sid,
        connectionId: session.connectionId,
        generation: gen,
        state: 'active',
      };
    }
  }

  return undefined;
}

// ============================================================================
// Store implementation
// ============================================================================

export const useVirtualSessionStore = create<VirtualSessionStoreState>()(
  (set, get) => ({
    entries: new Map(),

    register(nodeId: string) {
      const count = (_refCounts.get(nodeId) ?? 0) + 1;
      _refCounts.set(nodeId, count);

      if (count === 1) {
        // First observer — ensure upstream subscriptions and resolve immediately
        ensureUpstreamSubscriptions();
        get().resolveNode(nodeId);
      }
    },

    unregister(nodeId: string) {
      const count = (_refCounts.get(nodeId) ?? 1) - 1;
      if (count <= 0) {
        _refCounts.delete(nodeId);
        _lastSessionIds.delete(nodeId);
        const next = new Map(get().entries);
        next.delete(nodeId);
        set({ entries: next });
      } else {
        _refCounts.set(nodeId, count);
      }
    },

    getEntry(nodeId: string) {
      return get().entries.get(nodeId);
    },

    resolveNode(nodeId: string) {
      if (!_refCounts.has(nodeId)) return;

      const prev = get().entries.get(nodeId);
      const entry = resolveNodeId(nodeId, prev?.generation ?? 0);

      if (entry) {
        // ── Invariant: generation must be monotonically increasing ──
        invariant(
          entry.generation >= (prev?.generation ?? 0),
          'generation must be monotonic',
          { nodeId, newGen: entry.generation, prevGen: prev?.generation },
        );

        // ── Invariant: resolved connectionId must exist in connections map ──
        softInvariant(
          useAppStore.getState().connections.has(entry.connectionId),
          'resolved connectionId must exist in connections map',
          { nodeId, connectionId: entry.connectionId },
        );

        if (
          prev?.activeSessionId !== entry.activeSessionId ||
          prev?.connectionId !== entry.connectionId ||
          prev?.state !== entry.state
        ) {
          log('resolve:changed', {
            nodeId,
            sessionId: entry.activeSessionId,
            connectionId: entry.connectionId,
            generation: entry.generation,
            outcome: 'ok',
            detail: prev ? `${prev.state}→${entry.state}` : 'initial',
          });
          const next = new Map(get().entries);
          next.set(nodeId, entry);
          set({ entries: next });
        }
      } else if (!prev || prev.state !== 'disconnected') {
        log('resolve:disconnected', {
          nodeId,
          generation: prev?.generation ?? 0,
          outcome: 'error',
          detail: 'no active session found',
        });
        // Mark as disconnected, preserve generation for recovery
        const next = new Map(get().entries);
        next.set(nodeId, {
          activeSessionId: prev?.activeSessionId ?? '',
          connectionId: prev?.connectionId ?? '',
          generation: prev?.generation ?? 0,
          state: 'disconnected',
        });
        set({ entries: next });
      }
    },

    resolveAll() {
      const current = get().entries;
      const next = new Map(current);
      let changed = false;

      for (const nodeId of _refCounts.keys()) {
        const prev = current.get(nodeId);
        const entry = resolveNodeId(nodeId, prev?.generation ?? 0);

        if (entry) {
          if (
            prev?.activeSessionId !== entry.activeSessionId ||
            prev?.connectionId !== entry.connectionId ||
            prev?.state !== entry.state
          ) {
            next.set(nodeId, entry);
            changed = true;
          }
        } else if (!prev || prev.state !== 'disconnected') {
          next.set(nodeId, {
            activeSessionId: prev?.activeSessionId ?? '',
            connectionId: prev?.connectionId ?? '',
            generation: prev?.generation ?? 0,
            state: 'disconnected',
          });
          changed = true;
        }
      }

      if (changed) {
        set({ entries: next });
      }
    },
  }),
);

// ============================================================================
// Upstream subscriptions (lazy — initialized on first register)
// ============================================================================

let _subscribed = false;

function ensureUpstreamSubscriptions() {
  if (_subscribed) return;
  _subscribed = true;

  // Re-resolve when sessions or connections change
  let prevSessions = useAppStore.getState().sessions;
  let prevConnections = useAppStore.getState().connections;

  useAppStore.subscribe((state) => {
    if (state.sessions !== prevSessions || state.connections !== prevConnections) {
      prevSessions = state.sessions;
      prevConnections = state.connections;
      useVirtualSessionStore.getState().resolveAll();
    }
  });

  // Re-resolve when nodeTerminalMap changes
  let prevNodeTerminalMap = useSessionTreeStore.getState().nodeTerminalMap;

  useSessionTreeStore.subscribe((state) => {
    if (state.nodeTerminalMap !== prevNodeTerminalMap) {
      prevNodeTerminalMap = state.nodeTerminalMap;
      useVirtualSessionStore.getState().resolveAll();
    }
  });
}
