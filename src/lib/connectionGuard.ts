import { useAppStore } from '../store/appStore';
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

function isBlockedState(state: SshConnectionState | undefined): boolean {
  if (!state || typeof state === 'object') return false;
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
  const current = useAppStore.getState().connections.get(connectionId);
  if (current && isActiveState(current.state)) return;
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

        if (!isBlockedState(next.state)) {
          if (finished) return;
          finished = true;
          clearTimeout(timer);
          if (unsub) unsub();
          resolve();
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
