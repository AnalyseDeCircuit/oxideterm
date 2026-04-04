import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createSelectorStore } from '@/test/helpers/mockStore';

const apiMocks = vi.hoisted(() => ({
  probeConnections: vi.fn().mockResolvedValue([]),
}));

const slogMock = vi.hoisted(() => vi.fn());

const appStoreState = vi.hoisted(() => ({
  connections: new Map<string, { id: string }>(),
  setNetworkOnline: vi.fn(),
}));

vi.mock('@/lib/api', () => ({ api: apiMocks }));

vi.mock('@/lib/structuredLog', () => ({ slog: slogMock }));

vi.mock('@/store/appStore', () => ({
  useAppStore: createSelectorStore(appStoreState),
}));

import { useNetworkStatus } from '@/hooks/useNetworkStatus';

describe('useNetworkStatus', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    appStoreState.connections = new Map([['conn-1', { id: 'conn-1' }]]);
    Object.defineProperty(navigator, 'onLine', { configurable: true, value: true });
    Object.defineProperty(document, 'hidden', { configurable: true, value: false });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('probes active connections after the browser comes online', async () => {
    renderHook(() => useNetworkStatus());

    act(() => {
      window.dispatchEvent(new Event('online'));
    });

    await vi.advanceTimersByTimeAsync(2000);

    expect(appStoreState.setNetworkOnline).toHaveBeenCalledWith(true);
    expect(apiMocks.probeConnections).toHaveBeenCalledTimes(1);
    expect(slogMock).toHaveBeenCalledWith(expect.objectContaining({ event: 'probe:all_alive' }));
  });

  it('marks offline immediately and does not probe while offline', () => {
    renderHook(() => useNetworkStatus());

    act(() => {
      window.dispatchEvent(new Event('offline'));
    });

    expect(appStoreState.setNetworkOnline).toHaveBeenCalledWith(false);
    expect(apiMocks.probeConnections).not.toHaveBeenCalled();
  });

  it('skips probing when there are no connections', async () => {
    appStoreState.connections = new Map();
    renderHook(() => useNetworkStatus());

    act(() => {
      window.dispatchEvent(new Event('online'));
    });

    await vi.advanceTimersByTimeAsync(2000);

    expect(apiMocks.probeConnections).not.toHaveBeenCalled();
  });

  it('triggers a probe after waking from a long hidden period', async () => {
    renderHook(() => useNetworkStatus());

    Object.defineProperty(document, 'hidden', { configurable: true, value: true });
    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });

    await vi.advanceTimersByTimeAsync(6000);

    Object.defineProperty(document, 'hidden', { configurable: true, value: false });
    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });

    await vi.advanceTimersByTimeAsync(2000);
    expect(apiMocks.probeConnections).toHaveBeenCalledTimes(1);
  });

  it('enforces the minimum probe interval across repeated wake events', async () => {
    renderHook(() => useNetworkStatus());

    act(() => {
      window.dispatchEvent(new Event('online'));
    });
    await vi.advanceTimersByTimeAsync(2000);
    expect(apiMocks.probeConnections).toHaveBeenCalledTimes(1);

    act(() => {
      window.dispatchEvent(new Event('online'));
    });
    await vi.advanceTimersByTimeAsync(2000);
    expect(apiMocks.probeConnections).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(10000);
    act(() => {
      window.dispatchEvent(new Event('online'));
    });
    await vi.advanceTimersByTimeAsync(2000);
    expect(apiMocks.probeConnections).toHaveBeenCalledTimes(2);
  });
});