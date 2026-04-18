import { beforeEach, describe, expect, it, vi } from 'vitest';

const apiMocks = vi.hoisted(() => ({
  sshDisconnect: vi.fn(),
  sshListConnections: vi.fn(),
  sshSetKeepAlive: vi.fn(),
  closeTerminal: vi.fn(),
  networkStatusChanged: vi.fn().mockResolvedValue(undefined),
}));

const settingsStoreMock = vi.hoisted(() => {
  const state = {
    settings: {
      sidebarUI: {
        collapsed: false,
        activeSection: 'sessions',
      },
      buffer: {
        maxLines: 5000,
      },
      ide: {
        autoSave: false,
        agentMode: 'disabled' as const,
      },
    },
    getIde: vi.fn(() => ({
      autoSave: false,
      agentMode: 'disabled' as const,
    })),
  };

  const store = Object.assign(
    ((selector?: (value: typeof state) => unknown) =>
      selector ? selector(state) : state) as unknown as {
      (selector?: (value: typeof state) => unknown): unknown;
      getState: () => typeof state;
    },
    {
      getState: () => state,
    },
  );

  return { state, store };
});

const sessionTreeStoreMock = vi.hoisted(() => {
  const state = {
    getNodeByTerminalId: vi.fn(),
    purgeTerminalMapping: vi.fn(),
  };

  const store = Object.assign(
    ((selector?: (value: typeof state) => unknown) =>
      selector ? selector(state) : state) as unknown as {
      (selector?: (value: typeof state) => unknown): unknown;
      getState: () => typeof state;
    },
    {
      getState: () => state,
    },
  );

  return { state, store };
});

const localTerminalStoreMock = vi.hoisted(() => {
  const state = {
    getTerminal: vi.fn(),
    backgroundSessions: new Set<string>(),
    refreshTerminals: vi.fn().mockResolvedValue(undefined),
    closeTerminal: vi.fn().mockResolvedValue(undefined),
  };

  const store = Object.assign(
    ((selector?: (value: typeof state) => unknown) =>
      selector ? selector(state) : state) as unknown as {
      (selector?: (value: typeof state) => unknown): unknown;
      getState: () => typeof state;
    },
    {
      getState: () => state,
    },
  );

  return { state, store };
});

const agentServiceMocks = vi.hoisted(() => ({
  invalidateAgentCache: vi.fn(),
  ensureAgent: vi.fn(),
  isAgentReady: vi.fn(),
  readFile: vi.fn(),
  writeFile: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  api: apiMocks,
}));

vi.mock('@/hooks/useToast', () => ({
  useToastStore: {
    getState: () => ({ addToast: vi.fn() }),
  },
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: settingsStoreMock.store,
}));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: sessionTreeStoreMock.store,
}));

vi.mock('@/store/localTerminalStore', () => ({
  useLocalTerminalStore: localTerminalStoreMock.store,
}));

vi.mock('@/lib/agentService', () => agentServiceMocks);

vi.mock('@/lib/topologyResolver', () => ({
  topologyResolver: {
    getNodeId: vi.fn(),
    unregister: vi.fn(),
  },
}));

vi.mock('@/i18n', () => ({
  default: {
    t: (key: string) => key,
  },
}));

import { useAppStore } from '@/store/appStore';
import { useIdeStore } from '@/store/ideStore';
import { topologyResolver } from '@/lib/topologyResolver';
import type { RemoteEnvInfo, SessionInfo, SshConnectionInfo } from '@/types';

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

function resetIdeStore() {
  localStorage.clear();
  useIdeStore.setState({
    nodeId: null,
    terminalSessionId: null,
    project: null,
    tabs: [],
    activeTabId: null,
    treeWidth: 280,
    terminalHeight: 200,
    terminalVisible: false,
    splitDirection: null,
    splitActiveTabId: null,
    expandedPaths: new Set<string>(),
    treeRefreshSignal: {},
    conflictState: null,
    pendingScroll: null,
    cachedProjectPath: null,
    cachedTabPaths: [],
    cachedNodeId: null,
    lastClosedAt: null,
  });
}

describe('appStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
    resetIdeStore();
    vi.mocked(topologyResolver.getNodeId).mockReturnValue(undefined);
  });

  it('refreshConnections replaces the current connection map from backend data', async () => {
    apiMocks.sshListConnections.mockResolvedValue([
      makeConnection({ id: 'conn-1', state: 'active' }),
      makeConnection({ id: 'conn-2', host: 'second.example.com', username: 'root' }),
    ]);

    useAppStore.setState({
      connections: new Map([['stale', makeConnection({ id: 'stale' })]]),
    });

    await useAppStore.getState().refreshConnections();

    const connections = useAppStore.getState().connections;
    expect(Array.from(connections.keys())).toEqual(['conn-1', 'conn-2']);
    expect(connections.get('conn-1')?.state).toBe('active');
    expect(connections.get('conn-2')?.host).toBe('second.example.com');
  });

  it('refreshConnections keeps current state when backend refresh fails', async () => {
    apiMocks.sshListConnections.mockRejectedValue(new Error('network down'));
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    useAppStore.setState({
      connections: new Map([['conn-1', makeConnection({ state: 'active' })]]),
    });

    await expect(useAppStore.getState().refreshConnections()).resolves.toBeUndefined();

    expect(useAppStore.getState().connections.get('conn-1')?.state).toBe('active');
    consoleError.mockRestore();
  });

  it('setConnectionKeepAlive updates backend and local connection state', async () => {
    apiMocks.sshSetKeepAlive.mockResolvedValue(undefined);
    useAppStore.setState({
      connections: new Map([['conn-1', makeConnection()]]),
    });

    await useAppStore.getState().setConnectionKeepAlive('conn-1', true);

    expect(apiMocks.sshSetKeepAlive).toHaveBeenCalledWith('conn-1', true);
    expect(useAppStore.getState().connections.get('conn-1')?.keepAlive).toBe(true);
  });

  it('updateConnectionState mutates only the targeted connection', () => {
    useAppStore.setState({
      connections: new Map([
        ['conn-1', makeConnection()],
        ['conn-2', makeConnection({ id: 'conn-2', state: 'active' })],
      ]),
    });

    useAppStore.getState().updateConnectionState('conn-1', 'link_down');

    expect(useAppStore.getState().connections.get('conn-1')?.state).toBe('link_down');
    expect(useAppStore.getState().connections.get('conn-2')?.state).toBe('active');
  });

  it('updateConnectionRemoteEnv attaches remote environment info when the connection exists', () => {
    const remoteEnv: RemoteEnvInfo = {
      osType: 'Linux',
      osVersion: 'Ubuntu 24.04',
      kernel: '6.8.0',
      arch: 'x86_64',
      shell: 'bash',
      detectedAt: 1712345678,
    };

    useAppStore.setState({
      connections: new Map([['conn-1', makeConnection()]]),
    });

    useAppStore.getState().updateConnectionRemoteEnv('conn-1', remoteEnv);

    expect(useAppStore.getState().connections.get('conn-1')?.remoteEnv).toEqual(remoteEnv);
  });

  it('getConnectionForSession resolves through the session connectionId', () => {
    useAppStore.setState({
      connections: new Map([['conn-1', makeConnection({ state: 'active' })]]),
      sessions: new Map([['session-1', makeSession()]]),
    });

    expect(useAppStore.getState().getConnectionForSession('session-1')?.id).toBe('conn-1');
    expect(useAppStore.getState().getConnectionForSession('missing')).toBeUndefined();
  });

  it('disconnectSsh removes the connection and all related terminal tabs and sessions', async () => {
    apiMocks.sshDisconnect.mockResolvedValue(undefined);
    vi.mocked(topologyResolver.getNodeId).mockReturnValue('node-1');
    useAppStore.setState({
      connections: new Map([
        [
          'conn-1',
          makeConnection({
            state: 'active',
            terminalIds: ['session-1'],
            refCount: 1,
          }),
        ],
      ]),
      sessions: new Map([['session-1', makeSession()]]),
      tabs: [
        { id: 'tab-1', type: 'terminal', title: 'Terminal 1', sessionId: 'session-1' },
        { id: 'tab-sftp', type: 'sftp', title: 'SFTP', nodeId: 'node-1' },
        { id: 'tab-2', type: 'settings', title: 'Settings' },
      ],
      activeTabId: 'tab-1',
    });

    await useAppStore.getState().disconnectSsh('conn-1');

    expect(apiMocks.sshDisconnect).toHaveBeenCalledWith('conn-1');
    expect(useAppStore.getState().connections.has('conn-1')).toBe(false);
    expect(useAppStore.getState().sessions.has('session-1')).toBe(false);
    expect(useAppStore.getState().tabs.map((tab) => tab.id)).toEqual(['tab-2']);
    expect(useAppStore.getState().activeTabId).toBe('tab-2');
    expect(topologyResolver.unregister).toHaveBeenCalledWith('node-1');
    expect(sessionTreeStoreMock.state.purgeTerminalMapping).toHaveBeenCalledWith('session-1');
  });

  it('closeTab removes the terminal from the local connection cache after closing ssh terminals', async () => {
    apiMocks.closeTerminal.mockResolvedValue(undefined);

    useAppStore.setState({
      connections: new Map([[
        'conn-1',
        makeConnection({
          state: 'active',
          terminalIds: ['session-1'],
          refCount: 1,
        }),
      ]]),
      sessions: new Map([['session-1', makeSession()]]),
      tabs: [
        { id: 'tab-1', type: 'terminal', title: 'Terminal 1', sessionId: 'session-1' },
      ],
      activeTabId: 'tab-1',
    });

    await useAppStore.getState().closeTab('tab-1');

    expect(apiMocks.closeTerminal).toHaveBeenCalledWith('session-1');
    expect(useAppStore.getState().connections.get('conn-1')).toEqual(
      expect.objectContaining({
        terminalIds: [],
        refCount: 0,
        state: 'idle',
      }),
    );
  });

  it('closeTab clears ideStore when the last IDE tab closes', async () => {
    useIdeStore.setState({
      nodeId: 'node-1',
      project: { rootPath: '/srv/app', name: 'app', isGitRepo: false },
      expandedPaths: new Set(['/srv/app']),
      cachedProjectPath: '/srv/app',
      cachedNodeId: 'node-1',
      tabs: [
        {
          id: 'ide-file-1',
          path: '/srv/app/main.ts',
          name: 'main.ts',
          language: 'typescript',
          content: 'console.log(1)',
          originalContent: 'console.log(1)',
          isDirty: false,
          isLoading: false,
          isPinned: false,
          lastAccessTime: 1,
          contentVersion: 0,
        },
      ],
      activeTabId: 'ide-file-1',
    });

    useAppStore.setState({
      tabs: [
        { id: 'ide-root-1', type: 'ide', title: 'IDE: Remote', nodeId: 'node-1', sessionId: 'session-1' },
      ],
      activeTabId: 'ide-root-1',
    });

    await useAppStore.getState().closeTab('ide-root-1');

    expect(useAppStore.getState().tabs).toEqual([]);
    expect(useIdeStore.getState().project).toBeNull();
    expect(useIdeStore.getState().nodeId).toBeNull();
    expect(useIdeStore.getState().cachedProjectPath).toBeNull();
    expect(agentServiceMocks.invalidateAgentCache).toHaveBeenCalledWith('node-1');
  });

  it('closeTab keeps ideStore when another IDE tab is still open', async () => {
    useIdeStore.setState({
      nodeId: 'node-2',
      project: { rootPath: '/srv/other', name: 'other', isGitRepo: false },
      expandedPaths: new Set(['/srv/other']),
      cachedProjectPath: '/srv/other',
      cachedNodeId: 'node-2',
    });

    useAppStore.setState({
      tabs: [
        { id: 'ide-root-1', type: 'ide', title: 'IDE 1', nodeId: 'node-1', sessionId: 'session-1' },
        { id: 'ide-root-2', type: 'ide', title: 'IDE 2', nodeId: 'node-2', sessionId: 'session-2' },
      ],
      activeTabId: 'ide-root-1',
    });

    await useAppStore.getState().closeTab('ide-root-1');

    expect(useAppStore.getState().tabs.map((tab) => tab.id)).toEqual(['ide-root-2']);
    expect(useIdeStore.getState().project).toEqual({
      rootPath: '/srv/other',
      name: 'other',
      isGitRepo: false,
    });
    expect(agentServiceMocks.invalidateAgentCache).not.toHaveBeenCalled();
  });
});