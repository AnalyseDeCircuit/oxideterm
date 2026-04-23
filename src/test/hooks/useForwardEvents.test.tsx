import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resetRuntimeEventHubForTests } from '@/lib/runtimeEventHub';

const tauriForwardMocks = vi.hoisted(() => {
  const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
  let deferred = false;
  let pendingResolvers: Array<(unlisten: () => void) => void> = [];

  return {
    listen: vi.fn((eventName: string, callback: (event: { payload: unknown }) => void) => {
      const unlisten = vi.fn(() => {
        listeners.get(eventName)?.delete(callback);
      });

      if (deferred) {
        return new Promise<() => void>((resolve) => {
          pendingResolvers.push(() => {
            const current = listeners.get(eventName) ?? new Set();
            current.add(callback);
            listeners.set(eventName, current);
            resolve(unlisten);
          });
        });
      }

      const current = listeners.get(eventName) ?? new Set();
      current.add(callback);
      listeners.set(eventName, current);
      return Promise.resolve(unlisten);
    }),
    emit<T>(eventName: string, payload: T) {
      for (const callback of listeners.get(eventName) ?? []) {
        callback({ payload });
      }
    },
    count(eventName: string) {
      return listeners.get(eventName)?.size ?? 0;
    },
    setDeferred(value: boolean) {
      deferred = value;
    },
    resolvePending() {
      const resolvers = pendingResolvers;
      pendingResolvers = [];
      resolvers.forEach((resolve) => resolve(() => undefined));
    },
    clear() {
      listeners.clear();
      deferred = false;
      pendingResolvers = [];
      this.listen.mockClear();
    },
  };
});

vi.mock('@tauri-apps/api/event', () => ({
  listen: tauriForwardMocks.listen,
}));

import { useForwardEvents } from '@/hooks/useForwardEvents';

describe('useForwardEvents', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    tauriForwardMocks.clear();
    await resetRuntimeEventHubForTests();
  });

  afterEach(async () => {
    await resetRuntimeEventHubForTests();
    tauriForwardMocks.clear();
  });

  it('registers the backend listener and unsubscribes on unmount', async () => {
    const { unmount } = renderHook(() => useForwardEvents({}));

    await waitFor(() => expect(tauriForwardMocks.listen).toHaveBeenCalledTimes(1));
    expect(tauriForwardMocks.count('forward-event')).toBe(1);

    unmount();

    expect(tauriForwardMocks.count('forward-event')).toBe(0);
  });

  it('filters by session id and dispatches the correct callbacks', async () => {
    const onStatusChanged = vi.fn();
    const onStatsUpdated = vi.fn();
    const onSessionSuspended = vi.fn();

    renderHook(() =>
      useForwardEvents({
        sessionId: 'session-1',
        onStatusChanged,
        onStatsUpdated,
        onSessionSuspended,
      }),
    );

    await waitFor(() => expect(tauriForwardMocks.listen).toHaveBeenCalledTimes(1));

    tauriForwardMocks.emit('forward-event', {
      type: 'statusChanged',
      session_id: 'other',
      forward_id: 'f-ignored',
      status: 'error',
    });

    tauriForwardMocks.emit('forward-event', {
      type: 'statusChanged',
      session_id: 'session-1',
      forward_id: 'f-1',
      status: 'active',
    });

    tauriForwardMocks.emit('forward-event', {
      type: 'statsUpdated',
      session_id: 'session-1',
      forward_id: 'f-1',
      stats: {
        connection_count: 2,
        active_connections: 1,
        bytes_sent: 10,
        bytes_received: 20,
      },
    });

    tauriForwardMocks.emit('forward-event', {
      type: 'sessionSuspended',
      session_id: 'session-1',
      forward_ids: ['f-1', 'f-2'],
    });

    expect(onStatusChanged).toHaveBeenCalledOnce();
    expect(onStatusChanged).toHaveBeenCalledWith('f-1', 'active', undefined);
    expect(onStatsUpdated).toHaveBeenCalledWith('f-1', {
      connection_count: 2,
      active_connections: 1,
      bytes_sent: 10,
      bytes_received: 20,
    });
    expect(onSessionSuspended).toHaveBeenCalledWith(['f-1', 'f-2']);
  });

  it('cleans up listeners even if unmounted before listen resolves', async () => {
    tauriForwardMocks.setDeferred(true);
    const { unmount } = renderHook(() => useForwardEvents({}));

    expect(tauriForwardMocks.listen).toHaveBeenCalledTimes(1);
    unmount();
    tauriForwardMocks.resolvePending();
    await waitFor(() => expect(tauriForwardMocks.count('forward-event')).toBe(0));
  });

  it('does not register a listener while disabled', () => {
    renderHook(() => useForwardEvents({ enabled: false }));

    expect(tauriForwardMocks.listen).not.toHaveBeenCalled();
    expect(tauriForwardMocks.count('forward-event')).toBe(0);
  });

  it('registers and tears down the listener when enabled toggles', async () => {
    const { rerender } = renderHook(
      ({ enabled }) => useForwardEvents({ enabled }),
      { initialProps: { enabled: false } },
    );

    expect(tauriForwardMocks.count('forward-event')).toBe(0);

    rerender({ enabled: true });
    await waitFor(() => expect(tauriForwardMocks.count('forward-event')).toBe(1));

    rerender({ enabled: false });
    await waitFor(() => expect(tauriForwardMocks.count('forward-event')).toBe(0));
  });
});
