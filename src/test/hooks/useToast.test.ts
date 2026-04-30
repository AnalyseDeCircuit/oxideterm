import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useToastStore } from '../../hooks/useToast';

describe('useToastStore', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useToastStore.getState().clearToasts();
  });

  afterEach(() => {
    useToastStore.getState().clearToasts();
    vi.useRealTimers();
  });

  it('upserts a toast by id instead of adding duplicates', () => {
    const store = useToastStore.getState();

    store.upsertToast('connection-trace:1', {
      title: 'Connecting',
      progress: 20,
      persistent: true,
    });
    store.upsertToast('connection-trace:1', {
      title: 'Connecting',
      statusText: 'Authenticating',
      progress: 70,
      persistent: true,
    });

    expect(useToastStore.getState().toasts).toHaveLength(1);
    expect(useToastStore.getState().toasts[0]).toMatchObject({
      id: 'connection-trace:1',
      statusText: 'Authenticating',
      progress: 70,
    });
  });

  it('does not let an old timer remove a toast after it becomes persistent', () => {
    const store = useToastStore.getState();
    const id = store.addToast({ title: 'Short', duration: 100 });

    store.updateToast(id, { persistent: true, statusText: 'Still running' });
    vi.advanceTimersByTime(150);

    expect(useToastStore.getState().toasts.some((toast) => toast.id === id)).toBe(true);
  });

  it('reschedules removal when a persistent toast finishes', () => {
    const store = useToastStore.getState();

    store.upsertToast('connection-trace:2', {
      title: 'Connecting',
      progress: 50,
      persistent: true,
      duration: 0,
    });
    store.upsertToast('connection-trace:2', {
      title: 'Connected',
      progress: 100,
      persistent: false,
      duration: 200,
    });

    vi.advanceTimersByTime(199);
    expect(useToastStore.getState().toasts).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });
});

