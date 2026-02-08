/**
 * useNodeSession — React hook wrapping virtualSessionStore
 *
 * Thin wrapper that manages registration lifecycle and provides
 * reactive access to the resolved session for a given nodeId.
 * Components call this instead of touching virtualSessionStore directly.
 *
 * @module hooks/useNodeSession
 */
import { useEffect, useCallback } from 'react';
import {
  useVirtualSessionStore,
  type NodeSessionEntry,
  type NodeSessionState,
} from '../store/virtualSessionStore';

export interface ResolvedSession {
  sessionId: string;
  connectionId: string;
}

/**
 * Given a nodeId, return the latest live {sessionId, connectionId}.
 *
 * Automatically registers/unregisters with virtualSessionStore so
 * the store tracks which nodes need resolution.
 *
 * Returns `resolved: null` while the node has no active session.
 */
export function useNodeSession(nodeId: string | undefined): {
  resolved: ResolvedSession | null;
  /** Bumps when the resolved session changes (e.g., after reconnect) */
  resolveTick: number;
  /** Current state: 'resolving' | 'active' | 'disconnected' | undefined */
  state: NodeSessionState | undefined;
} {
  // Register/unregister lifecycle
  useEffect(() => {
    if (!nodeId) return;
    useVirtualSessionStore.getState().register(nodeId);
    return () => useVirtualSessionStore.getState().unregister(nodeId);
  }, [nodeId]);

  // Subscribe to the specific entry via Zustand React selector
  const entry = useVirtualSessionStore(
    useCallback(
      (s: { entries: Map<string, NodeSessionEntry> }) =>
        nodeId ? s.entries.get(nodeId) : undefined,
      [nodeId],
    ),
  );

  if (!entry || entry.state !== 'active') {
    return {
      resolved: null,
      resolveTick: entry?.generation ?? 0,
      state: nodeId ? (entry?.state ?? 'resolving') : undefined,
    };
  }

  return {
    resolved: {
      sessionId: entry.activeSessionId,
      connectionId: entry.connectionId,
    },
    resolveTick: entry.generation,
    state: entry.state,
  };
}
