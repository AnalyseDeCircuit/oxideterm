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
  if (!current) {
    throw new Error(CONNECTION_NOT_FOUND);
  }

  if (isActiveState(current.state)) return;
  if (!isBlockedState(current.state)) return;

  await new Promise<void>((resolve, reject) => {
    let finished = false;
    const timer = setTimeout(() => {
      if (finished) return;
      finished = true;
      unsub();
      reject(new Error(CONNECTION_RECONNECTING));
    }, timeoutMs);

    const unsub = useAppStore.subscribe((state) => {
        const next = state.connections.get(connectionId);
        if (!next) {
          if (finished) return;
          finished = true;
          clearTimeout(timer);
          unsub();
          reject(new Error(CONNECTION_NOT_FOUND));
          return;
        }

        if (isActiveState(next.state)) {
          if (finished) return;
          finished = true;
          clearTimeout(timer);
          unsub();
          resolve();
          return;
        }

        if (next.state === 'disconnected') {
          if (finished) return;
          finished = true;
          clearTimeout(timer);
          unsub();
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
