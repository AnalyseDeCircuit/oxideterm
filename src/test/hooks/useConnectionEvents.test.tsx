import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createSelectorStore } from '@/test/helpers/mockStore';

const tauriEventMocks = vi.hoisted(() => {
  const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();

  return {
    listen: vi.fn(async (eventName: string, callback: (event: { payload: unknown }) => void) => {
      const current = listeners.get(eventName) ?? new Set();
      current.add(callback);
      listeners.set(eventName, current);

      return () => {
        current.delete(callback);
        if (current.size === 0) {
          listeners.delete(eventName);
        }
      };
    }),
    emit<T>(eventName: string, payload: T) {
      for (const callback of listeners.get(eventName) ?? []) {
        callback({ payload });
      }
    },
    count(eventName: string) {
      return listeners.get(eventName)?.size ?? 0;
    },
    clear() {
      listeners.clear();
      this.listen.mockClear();
    },
  };
});

const transferStoreMock = vi.hoisted(() => ({
  interruptTransfersByNode: vi.fn(),
}));

const treeStoreMock = vi.hoisted(() => ({
  markLinkDownBatch: vi.fn(),
  clearLinkDown: vi.fn(),
  setReconnectProgress: vi.fn(),
}));

const orchestratorStoreMock = vi.hoisted(() => ({
  scheduleReconnect: vi.fn(),
}));

const profilerStoreMock = vi.hoisted(() => ({
  removeConnection: vi.fn(),
}));

const topologyResolverMock = vi.hoisted(() => ({
  getNodeId: vi.fn(),
  handleLinkDown: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: tauriEventMocks.listen,
}));

vi.mock('@/store/transferStore', () => ({
  useTransferStore: createSelectorStore(transferStoreMock),
}));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: createSelectorStore(treeStoreMock),
}));

vi.mock('@/store/reconnectOrchestratorStore', () => ({
  useReconnectOrchestratorStore: createSelectorStore(orchestratorStoreMock),
}));

vi.mock('@/store/profilerStore', () => ({
  useProfilerStore: createSelectorStore(profilerStoreMock),
}));

vi.mock('@/lib/topologyResolver', () => ({
  topologyResolver: topologyResolverMock,
}));

vi.mock('@/i18n', () => ({
  default: {
    t: (key: string) => key,
  },
}));

import { useConnectionEvents } from '@/hooks/useConnectionEvents';
import { useAppStore } from '@/store/appStore';
import type { SessionInfo, SshConnectionInfo } from '@/types';

function makeConnection(overrides: Partial<SshConnectionInfo> = {}): SshConnectionInfo {
  return {
    id: 'conn-1',
    host: 'example.com',
    port: 22,
    username: 'tester',
    state: 'idle',
    refCount: 0,
    keepAlive: false,
    createdAt: '2026-04-05T00:00:00Z',
    lastActive: '2026-04-05T00:00:00Z',
    terminalIds: [],
    forwardIds: [],
    ...overrides,
  };
}

function makeSession(overrides: Partial<SessionInfo> = {}): SessionInfo {
  return {
    id: 'session-1',
    name: 'Terminal 1',
    host: 'example.com',
    port: 22,
    username: 'tester',
    state: 'connected',
    color: '#fff',
    uptime_secs: 0,
    order: 0,
    auth_type: 'agent',
    connectionId: 'conn-1',
    ...overrides,
  };
}

function resetAppStore() {
  useAppStore.setState({
    sessions: new Map(),
    connections: new Map(),
    tabs: [],
    activeTabId: null,
    tabHistory: [],
    tabHistoryCursor: -1,
    _isNavigating: false,
    lastNonAgentTabType: null,
    modals: {
      newConnection: false,
      settings: false,
      editConnection: false,
      connectionManager: false,
      autoRoute: false,
    },
    quickConnectData: null,
    savedConnections: [],
    groups: [],
    selectedGroup: null,
    editingConnection: null,
    networkOnline: true,
  });
}

describe('useConnectionEvents', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriEventMocks.clear();
    resetAppStore();
    topologyResolverMock.getNodeId.mockReturnValue('node-1');
    topologyResolverMock.handleLinkDown.mockReturnValue(['node-1', 'node-child']);

    useAppStore.setState({
      connections: new Map([['conn-1', makeConnection()]]),
      closeTab: vi.fn().mockResolvedValue(undefined),
      refreshConnections: vi.fn().mockResolvedValue(undefined),
    });
  });

  afterEach(() => {
    tauriEventMocks.clear();
  });

  it('registers both backend listeners and unsubscribes on unmount', async () => {
    const { unmount } = renderHook(() => useConnectionEvents());

    await waitFor(() => expect(tauriEventMocks.listen).toHaveBeenCalledTimes(2));
    expect(tauriEventMocks.count('connection_status_changed')).toBe(1);
    expect(tauriEventMocks.count('env:detected')).toBe(1);

    unmount();

    expect(tauriEventMocks.count('connection_status_changed')).toBe(0);
    expect(tauriEventMocks.count('env:detected')).toBe(0);
  });

  it('handles link_down by updating state, marking nodes, scheduling reconnect, and interrupting transfers', async () => {
    renderHook(() => useConnectionEvents());

    await waitFor(() => expect(tauriEventMocks.listen).toHaveBeenCalledTimes(2));

    tauriEventMocks.emit('connection_status_changed', {
      connection_id: 'conn-1',
      status: 'link_down',
      affected_children: ['child-conn-1'],
      timestamp: Date.now(),
    });

    expect(useAppStore.getState().connections.get('conn-1')?.state).toBe('link_down');
    expect(topologyResolverMock.handleLinkDown).toHaveBeenCalledWith('conn-1', ['child-conn-1']);
    expect(treeStoreMock.markLinkDownBatch).toHaveBeenCalledWith(['node-1', 'node-child']);
    expect(orchestratorStoreMock.scheduleReconnect).toHaveBeenCalledWith('node-1');
    expect(transferStoreMock.interruptTransfersByNode).toHaveBeenCalledWith(
      'node-1',
      'connections.events.connection_lost_reconnecting',
    );
  });

  it('handles connected and env:detected events through appStore and session tree side effects', async () => {
    renderHook(() => useConnectionEvents());

    await waitFor(() => expect(tauriEventMocks.listen).toHaveBeenCalledTimes(2));

    tauriEventMocks.emit('connection_status_changed', {
      connection_id: 'conn-1',
      status: 'connected',
      affected_children: [],
      timestamp: Date.now(),
    });

    expect(useAppStore.getState().connections.get('conn-1')?.state).toBe('active');
    expect(treeStoreMock.clearLinkDown).toHaveBeenCalledWith('node-1');
    expect(treeStoreMock.setReconnectProgress).toHaveBeenCalledWith('node-1', null);

    tauriEventMocks.emit('env:detected', {
      connectionId: 'conn-1',
      osType: 'Linux',
      osVersion: 'Ubuntu 24.04',
      kernel: '6.8.0',
      arch: 'x86_64',
      shell: 'bash',
      detectedAt: 1712345678,
    });

    expect(useAppStore.getState().connections.get('conn-1')?.remoteEnv).toEqual({
      osType: 'Linux',
      osVersion: 'Ubuntu 24.04',
      kernel: '6.8.0',
      arch: 'x86_64',
      shell: 'bash',
      detectedAt: 1712345678,
    });
  });

  it('handles reconnecting status without scheduling a new reconnect job', async () => {
    renderHook(() => useConnectionEvents());

    await waitFor(() => expect(tauriEventMocks.listen).toHaveBeenCalledTimes(2));

    tauriEventMocks.emit('connection_status_changed', {
      connection_id: 'conn-1',
      status: 'reconnecting',
      affected_children: [],
      timestamp: Date.now(),
    });

    expect(useAppStore.getState().connections.get('conn-1')?.state).toBe('reconnecting');
    expect(orchestratorStoreMock.scheduleReconnect).not.toHaveBeenCalled();
    expect(treeStoreMock.markLinkDownBatch).not.toHaveBeenCalled();
  });

  it('handles disconnected by closing related tabs, interrupting transfers, refreshing connections, and cleaning profiler listeners', async () => {
    const closeTab = vi.fn().mockResolvedValue(undefined);
    const refreshConnections = vi.fn().mockResolvedValue(undefined);

    useAppStore.setState({
      sessions: new Map([['session-1', makeSession()]]),
      tabs: [
        { id: 'tab-terminal', type: 'terminal', title: 'Terminal', sessionId: 'session-1' },
        { id: 'tab-sftp', type: 'sftp', title: 'SFTP', nodeId: 'node-1', sessionId: 'session-1' },
      ],
      closeTab,
      refreshConnections,
    });

    renderHook(() => useConnectionEvents());

    await waitFor(() => expect(tauriEventMocks.listen).toHaveBeenCalledTimes(2));

    tauriEventMocks.emit('connection_status_changed', {
      connection_id: 'conn-1',
      status: 'disconnected',
      affected_children: [],
      timestamp: Date.now(),
    });

    await waitFor(() => {
      expect(closeTab).toHaveBeenCalledWith('tab-terminal');
      expect(closeTab).toHaveBeenCalledWith('tab-sftp');
    });

    expect(transferStoreMock.interruptTransfersByNode).toHaveBeenCalledWith(
      'node-1',
      'connections.events.connection_closed',
    );
    expect(refreshConnections).toHaveBeenCalledTimes(1);
    expect(profilerStoreMock.removeConnection).toHaveBeenCalledWith('conn-1');
    expect(useAppStore.getState().connections.get('conn-1')?.state).toBe('disconnected');
  });
});