import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMutableSelectorStore } from '@/test/helpers/mockStore';

const apiMocks = vi.hoisted(() => ({
  getSavedConnectionForConnect: vi.fn(),
  markConnectionUsed: vi.fn().mockResolvedValue(undefined),
}));

const sessionTreeState = vi.hoisted(() => ({
  nodes: [] as any[],
  selectedNodeId: null as string | null,
  expandManualPreset: vi.fn(),
  connectNodeWithAncestors: vi.fn(),
  createTerminalForNode: vi.fn(),
  addRootNode: vi.fn(),
  getNode: vi.fn(),
}));

const appStoreState = vi.hoisted(() => ({
  tabs: [] as Array<{ id: string; type: string; sessionId?: string }>,
  activeTabId: null as string | null,
}));

vi.mock('@/lib/api', () => ({ api: apiMocks }));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: createMutableSelectorStore(sessionTreeState),
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: createMutableSelectorStore(appStoreState),
}));

import { connectToSaved } from '@/lib/connectToSaved';

describe('connectToSaved', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sessionTreeState.nodes = [];
    sessionTreeState.selectedNodeId = null;
    appStoreState.tabs = [];
    appStoreState.activeTabId = null;
  });

  it('expands proxy chains, connects the chain, and creates a target terminal', async () => {
    apiMocks.getSavedConnectionForConnect.mockResolvedValue({
      host: 'target.example.com',
      port: 22,
      username: 'target',
      auth_type: 'key',
      key_path: '/tmp/key',
      passphrase: 'secret',
      proxy_chain: [
        { host: 'jump.example.com', port: 2222, username: 'jump', auth_type: 'agent' },
      ],
    });
    sessionTreeState.expandManualPreset.mockResolvedValue({ targetNodeId: 'node-target', chainDepth: 2 });
    sessionTreeState.connectNodeWithAncestors.mockResolvedValue(['node-target']);
    sessionTreeState.createTerminalForNode.mockResolvedValue('term-target');
    const createTab = vi.fn();

    await connectToSaved('saved-1', {
      createTab,
      toast: vi.fn(),
      t: (key: string) => key,
    });

    expect(sessionTreeState.expandManualPreset).toHaveBeenCalledWith(expect.objectContaining({
      savedConnectionId: 'saved-1',
      target: expect.objectContaining({ host: 'target.example.com', username: 'target' }),
    }));
    expect(sessionTreeState.connectNodeWithAncestors).toHaveBeenCalledWith('node-target');
    expect(sessionTreeState.createTerminalForNode).toHaveBeenCalledWith('node-target');
    expect(createTab).toHaveBeenCalledWith('terminal', 'term-target');
    expect(apiMocks.markConnectionUsed).toHaveBeenCalledWith('saved-1');
  });

  it('reconnects idle direct nodes and creates a new terminal when none exists', async () => {
    sessionTreeState.nodes = [
      {
        id: 'node-1',
        depth: 0,
        host: 'example.com',
        port: 22,
        username: 'tester',
        runtime: { status: 'idle', terminalIds: [] },
      },
    ];
    sessionTreeState.getNode.mockReturnValue({
      id: 'node-1',
      runtime: { terminalIds: [] },
    });
    apiMocks.getSavedConnectionForConnect.mockResolvedValue({
      name: 'Example',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'agent',
      proxy_chain: [],
    });
    sessionTreeState.connectNodeWithAncestors.mockResolvedValue(['node-1']);
    sessionTreeState.createTerminalForNode.mockResolvedValue('term-1');
    const createTab = vi.fn();

    await connectToSaved('saved-2', {
      createTab,
      toast: vi.fn(),
      t: (key: string) => key,
    });

    expect(sessionTreeState.connectNodeWithAncestors).toHaveBeenCalledWith('node-1');
    expect(sessionTreeState.createTerminalForNode).toHaveBeenCalledWith('node-1');
    expect(createTab).toHaveBeenCalledWith('terminal', 'term-1');
    expect(sessionTreeState.selectedNodeId).toBe('node-1');
  });

  it('activates an existing direct terminal tab instead of opening a duplicate', async () => {
    sessionTreeState.nodes = [
      {
        id: 'node-1',
        depth: 0,
        host: 'example.com',
        port: 22,
        username: 'tester',
        runtime: { status: 'active', terminalIds: ['term-1'] },
      },
    ];
    sessionTreeState.getNode.mockReturnValue({
      id: 'node-1',
      runtime: { terminalIds: ['term-1'] },
    });
    appStoreState.tabs = [{ id: 'tab-1', type: 'terminal', sessionId: 'term-1' }];
    apiMocks.getSavedConnectionForConnect.mockResolvedValue({
      name: 'Example',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'agent',
      proxy_chain: [],
    });
    const createTab = vi.fn();

    await connectToSaved('saved-3', {
      createTab,
      toast: vi.fn(),
      t: (key: string) => key,
    });

    expect(createTab).not.toHaveBeenCalled();
    expect(appStoreState.activeTabId).toBe('tab-1');
  });

  it('suppresses onError for lock-busy style failures', async () => {
    apiMocks.getSavedConnectionForConnect.mockRejectedValue(new Error('CHAIN_LOCK_BUSY'));
    const onError = vi.fn();

    await connectToSaved('saved-4', {
      createTab: vi.fn(),
      toast: vi.fn(),
      t: (key: string) => key,
      onError,
    });

    expect(onError).not.toHaveBeenCalled();
  });
});