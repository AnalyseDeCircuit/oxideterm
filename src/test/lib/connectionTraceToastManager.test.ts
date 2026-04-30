import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useToastStore } from '../../hooks/useToast';
import {
  handleConnectionTraceEvent,
  type ConnectionTraceEvent,
} from '../../lib/connectionTraceToastManager';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}));

function traceEvent(patch: Partial<ConnectionTraceEvent> = {}): ConnectionTraceEvent {
  return {
    attemptId: 'attempt-1',
    nodeId: 'node-1',
    label: 'example',
    stage: 'preparing',
    status: 'running',
    progress: 15,
    elapsedMs: 100,
    ...patch,
  };
}

describe('connectionTraceToastManager', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useToastStore.getState().clearToasts();
  });

  afterEach(() => {
    useToastStore.getState().clearToasts();
    vi.useRealTimers();
  });

  it('does not show a toast for a fast successful connection', () => {
    handleConnectionTraceEvent(traceEvent());
    vi.advanceTimersByTime(500);
    handleConnectionTraceEvent(traceEvent({ stage: 'ready', status: 'ready', progress: 100, elapsedMs: 700 }));
    vi.advanceTimersByTime(1200);

    expect(useToastStore.getState().toasts).toHaveLength(0);
  });

  it('shows one progress toast for a slow connection and updates it in place', () => {
    handleConnectionTraceEvent(traceEvent({ stage: 'opening_transport', progress: 28 }));
    vi.advanceTimersByTime(1200);

    expect(useToastStore.getState().toasts).toHaveLength(1);
    expect(useToastStore.getState().toasts[0]).toMatchObject({
      id: 'connection-trace:attempt-1',
      progress: 28,
      persistent: true,
    });

    handleConnectionTraceEvent(traceEvent({ stage: 'authentication', progress: 72 }));
    vi.advanceTimersByTime(300);

    expect(useToastStore.getState().toasts).toHaveLength(1);
    expect(useToastStore.getState().toasts[0].progress).toBe(72);
  });

  it('turns a visible trace toast into a short success toast', () => {
    handleConnectionTraceEvent(traceEvent());
    vi.advanceTimersByTime(1200);
    handleConnectionTraceEvent(traceEvent({ stage: 'ready', status: 'ready', progress: 100, elapsedMs: 2345 }));

    expect(useToastStore.getState().toasts).toHaveLength(1);
    expect(useToastStore.getState().toasts[0]).toMatchObject({
      variant: 'success',
      persistent: false,
      progress: 100,
    });

    vi.advanceTimersByTime(1800);
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });

  it('removes a visible progress toast on failure without adding an error toast', () => {
    handleConnectionTraceEvent(traceEvent());
    vi.advanceTimersByTime(1200);
    expect(useToastStore.getState().toasts).toHaveLength(1);

    handleConnectionTraceEvent(traceEvent({ status: 'failed', progress: 100, detail: 'boom' }));

    expect(useToastStore.getState().toasts).toHaveLength(0);
  });
});

