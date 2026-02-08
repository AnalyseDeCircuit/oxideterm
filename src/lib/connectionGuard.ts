import { useAppStore } from '../store/appStore';
import { maybeDelay } from './faultInjection';
import type { SshConnectionState } from '../types';

const ACTIVE_STATES = new Set<SshConnectionState>(['active', 'idle']);
const BLOCKED_STATES = new Set<SshConnectionState>([
  'connecting',
  'link_down',
  'reconnecting',
  'disconnecting',
  'disconnected',
]);

export const CONNECTION_RECONNECTING = 'CONNECTION_RECONNECTING';
export const CONNECTION_DISCONNECTED = 'CONNECTION_DISCONNECTED';
export const CONNECTION_NOT_FOUND = 'CONNECTION_NOT_FOUND';

function isActiveState(state: SshConnectionState | undefined): boolean {
  if (!state || typeof state === 'object') return false;
  return ACTIVE_STATES.has(state);
}

/** Returns true for error-object states like `{ error: "..." }` */
function isErrorState(state: SshConnectionState | undefined): state is { error: string } {
  return !!state && typeof state === 'object' && 'error' in state;
}

function isBlockedState(state: SshConnectionState | undefined): boolean {
  if (!state) return false;
  if (typeof state === 'object') return true; // { error: string } is also blocked
  return BLOCKED_STATES.has(state);
}

export function isConnectionGuardError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return (
    message === CONNECTION_RECONNECTING ||
    message === CONNECTION_DISCONNECTED ||
    message === CONNECTION_NOT_FOUND
  );
}

export async function waitForConnectionActive(
  connectionId: string,
  timeoutMs = 15000
): Promise<void> {
  // Fault injection: artificial delay before connection check
  await maybeDelay('refreshDelay');

  const current = useAppStore.getState().connections.get(connectionId);
  if (current && isActiveState(current.state)) return;
  if (current && isErrorState(current.state)) throw new Error(CONNECTION_DISCONNECTED);
  if (current && !isBlockedState(current.state)) return;

  if (!current) {
    // Best effort refresh before waiting, covering map propagation lag.
    useAppStore.getState().refreshConnections().catch(() => {
      // Ignore refresh errors and rely on timeout/error signaling below.
    });
  }

  await new Promise<void>((resolve, reject) => {
    let finished = false;
    let unsub: (() => void) | null = null;
    const timer = setTimeout(() => {
      if (finished) return;
      finished = true;
      if (unsub) unsub();
      const latest = useAppStore.getState().connections.get(connectionId);
      if (!latest) {
        reject(new Error(CONNECTION_NOT_FOUND));
        return;
      }
      reject(new Error(CONNECTION_RECONNECTING));
    }, timeoutMs);

    unsub = useAppStore.subscribe((state) => {
        const next = state.connections.get(connectionId);
        if (!next) {
          // Connection record may arrive with a small delay; keep waiting.
          return;
        }

        if (isActiveState(next.state)) {
          if (finished) return;
          finished = true;
          clearTimeout(timer);
          if (unsub) unsub();
          resolve();
          return;
        }

        // Error states are terminal — reject immediately
        if (isErrorState(next.state)) {
          if (finished) return;
          finished = true;
          clearTimeout(timer);
          if (unsub) unsub();
          reject(new Error(CONNECTION_DISCONNECTED));
          return;
        }

        if (next.state === 'disconnected') {
          if (finished) return;
          finished = true;
          clearTimeout(timer);
          if (unsub) unsub();
          reject(new Error(CONNECTION_DISCONNECTED));
        }
      });
  });
}

export async function guardSessionConnection(
  sessionId: string,
  timeoutMs = 15000
): Promise<void> {
  const session = useAppStore.getState().sessions.get(sessionId);
  if (!session?.connectionId) {
    throw new Error(CONNECTION_NOT_FOUND);
  }

  await waitForConnectionActive(session.connectionId, timeoutMs);
}

/**
 * Resolve a stable nodeId to its current live sessionId + connectionId,
 * ensuring the connection is active before returning.
 *
 * Uses virtualSessionStore as the primary source of truth. Falls back
 * to manual resolution when the store has no entry for the nodeId
 * (e.g., called from non-React context before registration).
 *
 * @param capability - Reserved for future per-capability gating.
 */
export async function guardNodeCapability(
  nodeId: string,
  _capability: 'sftp' | 'ide' | 'terminal' = 'sftp',
  timeoutMs = 15000
): Promise<{ sessionId: string; connectionId: string }> {
  // ── Fast path: virtualSessionStore already has an active entry ──
  const { useVirtualSessionStore } = await import('../store/virtualSessionStore');

  const entry = useVirtualSessionStore.getState().getEntry(nodeId);
  if (entry?.state === 'active') {
    await waitForConnectionActive(entry.connectionId, timeoutMs);
    return { sessionId: entry.activeSessionId, connectionId: entry.connectionId };
  }

  // ── If entry exists but not active, wait for store resolution ──
  if (entry) {
    const resolved = await new Promise<{ sessionId: string; connectionId: string }>(
      (resolve, reject) => {
        let finished = false;
        const timer = setTimeout(() => {
          if (finished) return;
          finished = true;
          unsub();
          reject(new Error(CONNECTION_RECONNECTING));
        }, timeoutMs);

        const unsub = useVirtualSessionStore.subscribe((state) => {
          const e = state.entries.get(nodeId);
          if (e?.state === 'active') {
            if (finished) return;
            finished = true;
            clearTimeout(timer);
            unsub();
            resolve({ sessionId: e.activeSessionId, connectionId: e.connectionId });
          }
        });

        // Re-check after subscribing (race condition guard)
        const recheck = useVirtualSessionStore.getState().getEntry(nodeId);
        if (recheck?.state === 'active') {
          if (!finished) {
            finished = true;
            clearTimeout(timer);
            unsub();
            resolve({ sessionId: recheck.activeSessionId, connectionId: recheck.connectionId });
          }
        }
      },
    );

    await waitForConnectionActive(resolved.connectionId, timeoutMs);
    return resolved;
  }

  // ── Fallback: manual resolution (store not initialized) ──
  const { useSessionTreeStore } = await import('../store/sessionTreeStore');

  const terminalIds = useSessionTreeStore.getState().getTerminalsForNode(nodeId);
  if (!terminalIds || terminalIds.length === 0) {
    throw new Error(CONNECTION_NOT_FOUND);
  }

  for (const sessionId of terminalIds) {
    const session = useAppStore.getState().sessions.get(sessionId);
    if (!session?.connectionId) continue;

    try {
      await waitForConnectionActive(session.connectionId, timeoutMs);
      return { sessionId, connectionId: session.connectionId };
    } catch {
      continue;
    }
  }

  // No terminal had an active connection — surface the most useful error
  const firstSessionId = terminalIds[0];
  const firstSession = useAppStore.getState().sessions.get(firstSessionId);
  if (!firstSession?.connectionId) {
    throw new Error(CONNECTION_NOT_FOUND);
  }

  await waitForConnectionActive(firstSession.connectionId, timeoutMs);
  return { sessionId: firstSessionId, connectionId: firstSession.connectionId };
}
