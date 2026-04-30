import { beforeEach, describe, expect, it, vi } from 'vitest';

const apiMocks = vi.hoisted(() => ({
  addRootNode: vi.fn(),
  getSessionTree: vi.fn(),
  getTreeNodePath: vi.fn(),
  updateTreeNodeState: vi.fn(),
  disconnectTreeNode: vi.fn(),
  closeTerminal: vi.fn(),
  createTerminal: vi.fn(),
  setTreeNodeTerminal: vi.fn(),
}));

const settingsStoreMock = vi.hoisted(() => {
  const state = {
    settings: {
      buffer: {
        maxLines: 5000,
      },
      terminal: {
        scrollback: 3000,
      },
      treeUI: {
        expandedIds: [] as string[],
        focusedNodeId: null as string | null,
      },
    },
    setTreeExpanded: vi.fn((ids: string[]) => {
      state.settings.treeUI.expandedIds = ids;
    }),
    setFocusedNode: vi.fn((nodeId: string | null) => {
      state.settings.treeUI.focusedNodeId = nodeId;
    }),
    toggleTreeNode: vi.fn((nodeId: string) => {
      const ids = new Set(state.settings.treeUI.expandedIds);
      if (ids.has(nodeId)) {
        ids.delete(nodeId);
      } else {
        ids.add(nodeId);
      }
      state.settings.treeUI.expandedIds = Array.from(ids);
    }),
  };

  return {
    state,
    store: {
      getState: () => state,
    },
  };
});

const appStoreMock = vi.hoisted(() => {
  const state = {
    sessions: new Map<string, { id: string; connectionId?: string; [key: string]: unknown }>(),
    connections: new Map<string, Record<string, unknown>>(),
    tabs: [] as Array<Record<string, unknown>>,
    createTab: vi.fn(),
    closeTab: vi.fn(),
    refreshConnections: vi.fn().mockResolvedValue(undefined),
  };

  return {
    state,
    store: {
      getState: () => state,
      setState: (updater: unknown) => {
        const patch = typeof updater === 'function' ? updater(state) : updater;
        Object.assign(state, patch);
      },
    },
  };
});

const reconnectOrchestratorMock = vi.hoisted(() => ({
  scheduleReconnect: vi.fn(),
  cancel: vi.fn(),
}));

const eventLogMock = vi.hoisted(() => ({
  addEvent: vi.fn(),
  addEntry: vi.fn(),
}));

const topologyResolverMock = vi.hoisted(() => ({
  unregister: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  api: apiMocks,
  nodeSftpInit: vi.fn(),
}));

vi.mock('@/store/settingsStore', () => ({
  deriveBackendHotLines: vi.fn((scrollback: number) => {
    const normalizedScrollback = Number.isFinite(scrollback) ? Math.round(scrollback) : 3000;
    return Math.min(12000, Math.max(5000, normalizedScrollback * 2));
  }),
  useSettingsStore: settingsStoreMock.store,
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: appStoreMock.store,
  tabReferencesAnySession: (
    tab: { sessionId?: string },
    sessionIds: ReadonlySet<string>,
  ) => Boolean(tab.sessionId && sessionIds.has(tab.sessionId)),
  removeConnectionsById: (connections: ReadonlyMap<string, Record<string, unknown>>, connectionIds: Iterable<string>) => {
    const next = new Map(connections);
    for (const connectionId of connectionIds) {
      next.delete(connectionId);
    }
    return next;
  },
  removeTerminalSessionsFromConnections: (
    connections: ReadonlyMap<string, Record<string, unknown>>,
    sessions: ReadonlyMap<string, { id: string; connectionId: string }>,
    sessionIds: Iterable<string>,
  ) => {
    const next = new Map(connections);
    const byConnection = new Map<string, Set<string>>();

    for (const sessionId of sessionIds) {
      const session = sessions.get(sessionId);
      if (!session?.connectionId) {
        continue;
      }

      const ids = byConnection.get(session.connectionId) ?? new Set<string>();
      ids.add(sessionId);
      byConnection.set(session.connectionId, ids);
    }

    for (const [connectionId, removedIds] of byConnection) {
      const connection = next.get(connectionId);
      if (!connection) {
        continue;
      }

      const terminalIds = Array.isArray(connection.terminalIds)
        ? connection.terminalIds.filter(
            (terminalId): terminalId is string => typeof terminalId === 'string' && !removedIds.has(terminalId),
          )
        : [];

      next.set(connectionId, {
        ...connection,
        terminalIds,
        refCount: terminalIds.length,
        state: terminalIds.length > 0 ? 'active' : 'idle',
      });
    }

    return next;
  },
  upsertSyntheticConnection: (
    connections: ReadonlyMap<string, Record<string, unknown>>,
    seed: Record<string, unknown> & { id: string; terminalIds?: string[] },
  ) => {
    const next = new Map(connections);
    const existing = next.get(seed.id) ?? {};
    const terminalIds = Array.from(
      new Set([
        ...(Array.isArray(existing.terminalIds) ? existing.terminalIds : []),
        ...(seed.terminalIds ?? []),
      ]),
    );

    next.set(seed.id, {
      ...existing,
      ...seed,
      terminalIds,
      refCount: seed.refCount ?? terminalIds.length,
    });

    return next;
  },
}));

vi.mock('@/store/reconnectOrchestratorStore', () => ({
  useReconnectOrchestratorStore: {
    getState: () => reconnectOrchestratorMock,
  },
}));

vi.mock('@/store/eventLogStore', () => ({
  useEventLogStore: {
    getState: () => eventLogMock,
  },
}));

vi.mock('@/lib/topologyResolver', () => ({
  topologyResolver: topologyResolverMock,
}));

import { useSessionTreeStore } from '@/store/sessionTreeStore';
import type { FlatNode } from '@/types';

function makeNode(overrides: Partial<FlatNode> = {}): FlatNode {
  return {
    id: 'node-1',
    parentId: null,
    depth: 0,
    host: 'example.com',
    port: 22,
    username: 'tester',
    displayName: 'Example',
    state: { status: 'pending' },
    hasChildren: false,
    isLastChild: true,
    originType: 'direct',
    terminalSessionId: null,
    sftpSessionId: null,
    sshConnectionId: null,
    ...overrides,
  };
}

const initialMethods = (() => {
  const state = useSessionTreeStore.getState();
  return {
    getNodePath: state.getNodePath,
    resetNodeState: state.resetNodeState,
    connectNodeInternal: state.connectNodeInternal,
    fetchTree: state.fetchTree,
  };
})();

function resetSessionTreeStore() {
  useSessionTreeStore.setState({
    rawNodes: [],
    nodes: [],
    selectedNodeId: null,
    isLoading: false,
    error: null,
    summary: null,
    nodeTerminalMap: new Map(),
    terminalNodeMap: new Map(),
    linkDownNodeIds: new Set(),
    reconnectProgress: new Map(),
    disconnectedTerminalCounts: new Map(),
    connectingNodeIds: new Set(),
    isConnectingChain: false,
    ...initialMethods,
  });
}

describe('sessionTreeStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetSessionTreeStore();
    settingsStoreMock.state.settings.treeUI.expandedIds = [];
    settingsStoreMock.state.settings.treeUI.focusedNodeId = null;
    appStoreMock.state.sessions = new Map();
    appStoreMock.state.connections = new Map();
    appStoreMock.state.tabs = [];
    appStoreMock.state.createTab = vi.fn();
    appStoreMock.state.closeTab = vi.fn().mockResolvedValue(undefined);
    appStoreMock.state.refreshConnections = vi.fn().mockResolvedValue(undefined);
  });

  it('acquires and releases node and chain locks idempotently', () => {
    const store = useSessionTreeStore.getState();

    expect(store.acquireConnectLock('node-1')).toBe(true);
    expect(store.acquireConnectLock('node-1')).toBe(false);
    expect(store.isNodeConnecting('node-1')).toBe(true);

    store.releaseConnectLock('node-1');
    store.releaseConnectLock('node-1');
    expect(store.isNodeConnecting('node-1')).toBe(false);

    expect(store.acquireChainLock()).toBe(true);
    expect(store.acquireChainLock()).toBe(false);
    store.releaseChainLock();
    store.releaseChainLock();
    expect(useSessionTreeStore.getState().isConnectingChain).toBe(false);
  });

  it('clears inherited link-down flags but preserves descendants with their own broken connection', () => {
    const root = makeNode({ id: 'root', hasChildren: true });
    const inheritedChild = makeNode({ id: 'child-inherited', parentId: 'root', depth: 1 });
    const ownConnectionChild = makeNode({
      id: 'child-own',
      parentId: 'root',
      depth: 1,
      sshConnectionId: 'conn-child',
      state: { status: 'connected' },
    });

    useSessionTreeStore.setState({ rawNodes: [root, inheritedChild, ownConnectionChild] });
    useSessionTreeStore.getState().rebuildUnifiedNodes();

    useSessionTreeStore.getState().markLinkDownBatch(['root', 'child-inherited', 'child-own']);
    useSessionTreeStore.getState().clearLinkDown('root');

    expect(useSessionTreeStore.getState().linkDownNodeIds).toEqual(new Set(['child-own']));
    expect(useSessionTreeStore.getState().getNode('child-own')?.runtime.status).toBe('link-down');
    expect(useSessionTreeStore.getState().getNode('child-inherited')?.runtime.status).toBe('idle');
  });

  it('syncs backend drift, prunes orphan mappings, and repairs appStore session connection ids', async () => {
    const localNode = makeNode({
      id: 'node-1',
      state: { status: 'connected' },
      terminalSessionId: 'term-1',
      sshConnectionId: 'conn-old',
    });
    const orphanNode = makeNode({ id: 'orphan', parentId: 'node-1', depth: 1 });
    const backendNode = makeNode({
      id: 'node-1',
      state: { status: 'connected' },
      terminalSessionId: 'term-1',
      sshConnectionId: 'conn-new',
    });

    useSessionTreeStore.setState({
      rawNodes: [localNode, orphanNode],
      linkDownNodeIds: new Set(['node-1', 'orphan']),
      nodeTerminalMap: new Map([
        ['node-1', ['term-1']],
        ['orphan', ['term-orphan']],
      ]),
      terminalNodeMap: new Map([
        ['term-1', 'node-1'],
        ['term-orphan', 'orphan'],
      ]),
    });
    useSessionTreeStore.getState().rebuildUnifiedNodes();

    appStoreMock.state.sessions = new Map([
      ['term-1', { id: 'term-1', connectionId: 'conn-old' }],
    ]);

    apiMocks.getSessionTree.mockResolvedValue([backendNode]);

    const report = await useSessionTreeStore.getState().syncFromBackend();

    expect(report.driftCount).toBeGreaterThan(0);
    expect(report.fixed).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ nodeId: 'node-1', field: 'sshConnectionId', backendValue: 'conn-new' }),
        expect.objectContaining({ nodeId: 'orphan', field: 'node', backendValue: null }),
      ]),
    );
    expect(useSessionTreeStore.getState().rawNodes).toEqual([backendNode]);
    expect(useSessionTreeStore.getState().nodeTerminalMap).toEqual(new Map([['node-1', ['term-1']]]));
    expect(useSessionTreeStore.getState().terminalNodeMap).toEqual(new Map([['term-1', 'node-1']]));
    expect(useSessionTreeStore.getState().linkDownNodeIds).toEqual(new Set(['node-1']));
    expect(appStoreMock.state.connections.get('conn-new')).toEqual(
      expect.objectContaining({
        id: 'conn-new',
        host: 'example.com',
        port: 22,
        username: 'tester',
        terminalIds: ['term-1'],
        state: 'active',
      }),
    );
    expect(appStoreMock.state.sessions.get('term-1')?.connectionId).toBe('conn-new');
  });

  it('rehydrates terminal mappings from backend terminalSessionId after frontend state reset', async () => {
    const backendNode = makeNode({
      id: 'node-1',
      state: { status: 'connected' },
      terminalSessionId: 'term-backend',
      sshConnectionId: 'conn-1',
    });

    apiMocks.getSessionTree.mockResolvedValue([backendNode]);

    await useSessionTreeStore.getState().fetchTree();

    expect(useSessionTreeStore.getState().nodeTerminalMap).toEqual(new Map([['node-1', ['term-backend']]]));
    expect(useSessionTreeStore.getState().terminalNodeMap).toEqual(new Map([['term-backend', 'node-1']]));
    expect(useSessionTreeStore.getState().getNodeByTerminalId('term-backend')?.id).toBe('node-1');
    expect(useSessionTreeStore.getState().getNode('node-1')?.runtime.status).toBe('active');
  });

  it('repairs missing terminal mappings during backend sync even when node state did not drift', async () => {
    const backendNode = makeNode({
      id: 'node-1',
      state: { status: 'connected' },
      terminalSessionId: 'term-backend',
      sshConnectionId: 'conn-1',
    });

    useSessionTreeStore.setState({
      rawNodes: [backendNode],
      nodeTerminalMap: new Map(),
      terminalNodeMap: new Map(),
    });
    useSessionTreeStore.getState().rebuildUnifiedNodes();
    apiMocks.getSessionTree.mockResolvedValue([backendNode]);

    const report = await useSessionTreeStore.getState().syncFromBackend();

    expect(report.driftCount).toBe(0);
    expect(useSessionTreeStore.getState().nodeTerminalMap).toEqual(new Map([['node-1', ['term-backend']]]));
    expect(useSessionTreeStore.getState().terminalNodeMap).toEqual(new Map([['term-backend', 'node-1']]));
  });

  it('resetNodeState only performs local cleanup and does not close terminals via backend', async () => {
    const node = makeNode({
      id: 'node-1',
      state: { status: 'connected' },
      sshConnectionId: 'conn-1',
      terminalSessionId: 'term-main',
      sftpSessionId: 'sftp-1',
    });

    useSessionTreeStore.setState({
      rawNodes: [node],
      nodeTerminalMap: new Map([['node-1', ['term-main', 'term-side']]]),
      terminalNodeMap: new Map([
        ['term-main', 'node-1'],
        ['term-side', 'node-1'],
      ]),
      linkDownNodeIds: new Set(['node-1']),
      reconnectProgress: new Map([['node-1', { attempt: 1, maxAttempts: 3, nextRetryMs: 1000 }]]),
    });
    useSessionTreeStore.getState().rebuildUnifiedNodes();

    appStoreMock.state.sessions = new Map([
      ['term-main', { id: 'term-main', connectionId: 'conn-1' }],
      ['term-side', { id: 'term-side', connectionId: 'conn-1' }],
    ]);

    await useSessionTreeStore.getState().resetNodeState('node-1');

    expect(apiMocks.closeTerminal).not.toHaveBeenCalled();
    expect(appStoreMock.state.sessions.size).toBe(0);
    expect(useSessionTreeStore.getState().nodeTerminalMap.size).toBe(0);
    expect(useSessionTreeStore.getState().terminalNodeMap.size).toBe(0);
    expect(useSessionTreeStore.getState().linkDownNodeIds.size).toBe(0);
    expect(useSessionTreeStore.getState().reconnectProgress.size).toBe(0);
    expect(useSessionTreeStore.getState().getRawNode('node-1')?.state.status).toBe('pending');
    expect(useSessionTreeStore.getState().getRawNode('node-1')?.sshConnectionId).toBeNull();
  });

  it('disconnectNode closes node-scoped tabs even when the node has no terminal session', async () => {
    const node = makeNode({
      id: 'node-1',
      state: { status: 'connected' },
      sshConnectionId: 'conn-1',
    });

    useSessionTreeStore.setState({
      rawNodes: [node],
      nodeTerminalMap: new Map(),
      terminalNodeMap: new Map(),
    });
    useSessionTreeStore.getState().rebuildUnifiedNodes();

    appStoreMock.state.tabs = [
      { id: 'sftp-tab', type: 'sftp', nodeId: 'node-1' },
      { id: 'forwards-tab', type: 'forwards', nodeId: 'node-1' },
      { id: 'other-tab', type: 'sftp', nodeId: 'other-node' },
    ];
    appStoreMock.state.connections = new Map([['conn-1', { id: 'conn-1' }]]);
    apiMocks.disconnectTreeNode.mockResolvedValue(undefined);
    apiMocks.getSessionTree.mockResolvedValue([
      makeNode({ id: 'node-1', state: { status: 'pending' } }),
    ]);

    await useSessionTreeStore.getState().disconnectNode('node-1');

    expect(appStoreMock.state.closeTab).toHaveBeenCalledWith('sftp-tab');
    expect(appStoreMock.state.closeTab).toHaveBeenCalledWith('forwards-tab');
    expect(appStoreMock.state.closeTab).not.toHaveBeenCalledWith('other-tab');
    expect(apiMocks.disconnectTreeNode).toHaveBeenCalledWith('node-1');
  });

  it('createTerminalForNode upserts the connection locally before binding the terminal to the node', async () => {
    const node = makeNode({
      id: 'node-1',
      state: { status: 'connected' },
      sshConnectionId: 'conn-1',
    });

    useSessionTreeStore.setState({ rawNodes: [node] });
    useSessionTreeStore.getState().rebuildUnifiedNodes();

    apiMocks.createTerminal.mockResolvedValue({
      sessionId: 'term-1',
      session: { id: 'term-1', name: 'term-1', connectionId: 'conn-1' },
    });
    apiMocks.setTreeNodeTerminal.mockResolvedValue(undefined);

    appStoreMock.state.connections = new Map();

    await expect(useSessionTreeStore.getState().createTerminalForNode('node-1')).resolves.toBe('term-1');

    expect(apiMocks.createTerminal).toHaveBeenCalledWith(expect.objectContaining({
      connectionId: 'conn-1',
      maxBufferLines: 5000,
    }));
    expect(appStoreMock.state.connections.get('conn-1')).toEqual(
      expect.objectContaining({
        id: 'conn-1',
        host: 'example.com',
        port: 22,
        username: 'tester',
        terminalIds: ['term-1'],
        state: 'active',
      }),
    );
    expect(apiMocks.setTreeNodeTerminal).toHaveBeenCalledWith('node-1', 'term-1');
  });

  it('releases all locks when connectNodeWithAncestors fails mid-chain', async () => {
    const root = makeNode({
      id: 'root',
      state: { status: 'connected' },
      sshConnectionId: 'conn-root',
      hasChildren: true,
    });
    const leaf = makeNode({ id: 'leaf', parentId: 'root', depth: 1 });
    const resetNodeState = vi.fn().mockResolvedValue(undefined);
    const connectNodeInternal = vi.fn().mockRejectedValue(new Error('boom'));
    const fetchTree = vi.fn().mockResolvedValue(undefined);

    useSessionTreeStore.setState({
      rawNodes: [root, leaf],
      getNodePath: vi.fn().mockResolvedValue([root, leaf]),
      resetNodeState,
      connectNodeInternal,
      fetchTree,
    });
    useSessionTreeStore.getState().rebuildUnifiedNodes();
    apiMocks.updateTreeNodeState.mockResolvedValue(undefined);

    await expect(useSessionTreeStore.getState().connectNodeWithAncestors('leaf')).rejects.toThrow(
      'CONNECTION_CHAIN_FAILED',
    );

    expect(resetNodeState).toHaveBeenCalledTimes(1);
    expect(resetNodeState).toHaveBeenCalledWith('leaf');
    expect(connectNodeInternal).toHaveBeenCalledWith('leaf', expect.objectContaining({
      attemptId: expect.any(String),
      mode: 'connect',
      stepIndex: 1,
      totalSteps: 1,
      label: 'Example',
    }));
    expect(apiMocks.updateTreeNodeState).toHaveBeenCalledWith('leaf', 'failed', 'boom');
    expect(fetchTree).toHaveBeenCalled();
    expect(useSessionTreeStore.getState().isConnectingChain).toBe(false);
    expect(useSessionTreeStore.getState().connectingNodeIds.size).toBe(0);
  });
});
