import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMutableSelectorStore } from '@/test/helpers/mockStore';

const apiMocks = vi.hoisted(() => ({
  getSavedConnectionForConnect: vi.fn(),
  markConnectionUsed: vi.fn().mockResolvedValue(undefined),
  preflightTreeNode: vi.fn(),
}));

const sessionTreeState = vi.hoisted(() => ({
  nodes: [] as any[],
  selectedNodeId: null as string | null,
  expandManualPreset: vi.fn(),
  connectNode: vi.fn(),
  createTerminalForNode: vi.fn(),
  addRootNode: vi.fn(),
  getNode: vi.fn(),
  removeNode: vi.fn(),
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
    apiMocks.preflightTreeNode.mockResolvedValue({ status: 'verified' });
  });

  it('expands proxy chains, connects each hop, and creates a target terminal', async () => {
    apiMocks.getSavedConnectionForConnect.mockResolvedValue({
      host: 'target.example.com',
      port: 22,
      username: 'target',
      auth_type: 'key',
      key_path: '/tmp/key',
      passphrase: 'secret',
      agent_forwarding: true,
      proxy_chain: [
        {
          host: 'jump.example.com',
          port: 2222,
          username: 'jump',
          auth_type: 'agent',
          agent_forwarding: true,
        },
      ],
    });
    sessionTreeState.expandManualPreset.mockResolvedValue({
      targetNodeId: 'node-target',
      pathNodeIds: ['node-jump', 'node-target'],
      chainDepth: 2,
    });
    sessionTreeState.connectNode.mockResolvedValue(undefined);
    sessionTreeState.createTerminalForNode.mockResolvedValue('term-target');
    const createTab = vi.fn();

    const result = await connectToSaved('saved-1', {
      createTab,
      toast: vi.fn(),
      t: (key: string) => key,
    });

    expect(sessionTreeState.expandManualPreset).toHaveBeenCalledWith(expect.objectContaining({
      savedConnectionId: 'saved-1',
      target: expect.objectContaining({
        host: 'target.example.com',
        username: 'target',
        agentForwarding: true,
      }),
      hops: [
        expect.objectContaining({
          host: 'jump.example.com',
          agentForwarding: true,
        }),
      ],
    }));
    expect(apiMocks.preflightTreeNode).toHaveBeenNthCalledWith(1, 'node-jump');
    expect(apiMocks.preflightTreeNode).toHaveBeenNthCalledWith(2, 'node-target');
    expect(sessionTreeState.connectNode).toHaveBeenNthCalledWith(1, 'node-jump');
    expect(sessionTreeState.connectNode).toHaveBeenNthCalledWith(2, 'node-target');
    expect(sessionTreeState.createTerminalForNode).toHaveBeenCalledWith('node-target');
    expect(createTab).toHaveBeenCalledWith('terminal', 'term-target');
    expect(apiMocks.markConnectionUsed).toHaveBeenCalledWith('saved-1');
    expect(result).toEqual({ nodeId: 'node-target', sessionId: 'term-target' });
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
      agent_forwarding: true,
      proxy_chain: [],
    });
    sessionTreeState.connectNode.mockResolvedValue(undefined);
    sessionTreeState.createTerminalForNode.mockResolvedValue('term-1');
    const createTab = vi.fn();

    await connectToSaved('saved-2', {
      createTab,
      toast: vi.fn(),
      t: (key: string) => key,
    });

    expect(apiMocks.preflightTreeNode).toHaveBeenCalledWith('node-1');
    expect(sessionTreeState.connectNode).toHaveBeenCalledWith('node-1');
    expect(sessionTreeState.createTerminalForNode).toHaveBeenCalledWith('node-1');
    expect(createTab).toHaveBeenCalledWith('terminal', 'term-1');
    expect(sessionTreeState.selectedNodeId).toBe('node-1');
  });

  it('passes agentForwarding when creating a new direct root node', async () => {
    apiMocks.getSavedConnectionForConnect.mockResolvedValue({
      name: 'Forwarded',
      host: 'forwarded.example.com',
      port: 22,
      username: 'alice',
      auth_type: 'agent',
      agent_forwarding: true,
      proxy_chain: [],
    });
    sessionTreeState.addRootNode.mockResolvedValue('node-forwarded');
    sessionTreeState.connectNode.mockResolvedValue(undefined);
    sessionTreeState.createTerminalForNode.mockResolvedValue('term-forwarded');

    await connectToSaved('saved-forwarded', {
      createTab: vi.fn(),
      toast: vi.fn(),
      t: (key: string) => key,
    });

    expect(sessionTreeState.addRootNode).toHaveBeenCalledWith(expect.objectContaining({
      host: 'forwarded.example.com',
      username: 'alice',
      agentForwarding: true,
    }));
    expect(apiMocks.preflightTreeNode).toHaveBeenCalledWith('node-forwarded');
    expect(sessionTreeState.connectNode).toHaveBeenCalledWith('node-forwarded');
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
      agent_forwarding: false,
      proxy_chain: [],
    });
    const createTab = vi.fn();

    const result = await connectToSaved('saved-3', {
      createTab,
      toast: vi.fn(),
      t: (key: string) => key,
    });

    expect(createTab).not.toHaveBeenCalled();
    expect(appStoreState.activeTabId).toBe('tab-1');
    expect(result).toEqual({ nodeId: 'node-1', sessionId: 'term-1' });
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

  it('requests a password prompt instead of attempting a fresh connection when password is missing', async () => {
    apiMocks.getSavedConnectionForConnect.mockResolvedValue({
      name: 'Missing Password',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'password',
      agent_forwarding: false,
      proxy_chain: [],
    });
    const onError = vi.fn();

    await connectToSaved('saved-missing-password', {
      createTab: vi.fn(),
      toast: vi.fn(),
      t: (key: string) => key,
      onError,
    });

    expect(onError).toHaveBeenCalledWith('saved-missing-password', 'missing-password');
    expect(sessionTreeState.addRootNode).not.toHaveBeenCalled();
    expect(sessionTreeState.connectNode).not.toHaveBeenCalled();
  });

  it('treats an explicitly empty saved password as a valid password auth value', async () => {
    apiMocks.getSavedConnectionForConnect.mockResolvedValue({
      name: 'Empty Password',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'password',
      password: '',
      agent_forwarding: false,
      proxy_chain: [],
    });
    const onError = vi.fn();

    await connectToSaved('saved-empty-password', {
      createTab: vi.fn(),
      toast: vi.fn(),
      t: (key: string) => key,
      onError,
    });

    expect(onError).not.toHaveBeenCalled();
    expect(sessionTreeState.addRootNode).toHaveBeenCalledWith(expect.objectContaining({
      authType: 'password',
      password: '',
    }));
    expect(sessionTreeState.connectNode).toHaveBeenCalled();
  });

  it('reuses an active node without prompting for password when a terminal already exists', async () => {
    sessionTreeState.nodes = [
      {
        id: 'node-active',
        depth: 0,
        host: 'example.com',
        port: 22,
        username: 'tester',
        runtime: { status: 'active', terminalIds: ['term-1'] },
      },
    ];
    sessionTreeState.getNode.mockReturnValue({
      id: 'node-active',
      runtime: { terminalIds: ['term-1'] },
    });
    appStoreState.tabs = [{ id: 'tab-1', type: 'terminal', sessionId: 'term-1' }];
    apiMocks.getSavedConnectionForConnect.mockResolvedValue({
      name: 'Active Password Conn',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'password',
      agent_forwarding: false,
      proxy_chain: [],
    });
    const onError = vi.fn();

    await connectToSaved('saved-active-password', {
      createTab: vi.fn(),
      toast: vi.fn(),
      t: (key: string) => key,
      onError,
    });

    expect(onError).not.toHaveBeenCalled();
    expect(appStoreState.activeTabId).toBe('tab-1');
  });

  it('blocks unsupported proxy-hop keyboard-interactive auth before expanding the chain', async () => {
    apiMocks.getSavedConnectionForConnect.mockResolvedValue({
      name: 'Jump Target',
      host: 'target.example.com',
      port: 22,
      username: 'target-user',
      auth_type: 'agent',
      agent_forwarding: false,
      proxy_chain: [
        {
          host: 'jump-1.example.com',
          port: 22,
          username: 'jump1',
          auth_type: 'keyboard_interactive',
          agent_forwarding: false,
        },
      ],
    } as never);
    const toast = vi.fn();
    const onError = vi.fn();

    await connectToSaved('saved-kbi-hop', {
      createTab: vi.fn(),
      toast,
      t: (key: string) => key,
      onError,
    });

    expect(toast).toHaveBeenCalledWith(expect.objectContaining({
      title: 'connections.toast.proxy_chain_invalid',
      description: 'connections.toast.proxy_hop_kbi_unsupported',
      variant: 'error',
    }));
    expect(onError).toHaveBeenCalledWith('saved-kbi-hop', 'connect-failed');
    expect(sessionTreeState.expandManualPreset).not.toHaveBeenCalled();
  });

  it('surfaces a host key challenge for a later proxy hop without opening a terminal yet', async () => {
    const onHostKeyChallenge = vi.fn();

    apiMocks.getSavedConnectionForConnect.mockResolvedValue({
      host: 'target.example.com',
      port: 22,
      username: 'target',
      auth_type: 'agent',
      agent_forwarding: false,
      proxy_chain: [
        {
          host: 'jump.example.com',
          port: 2222,
          username: 'jump',
          auth_type: 'agent',
          agent_forwarding: false,
        },
      ],
    });
    sessionTreeState.expandManualPreset.mockResolvedValue({
      targetNodeId: 'node-target',
      pathNodeIds: ['node-jump', 'node-target'],
      chainDepth: 2,
    });
    apiMocks.preflightTreeNode
      .mockResolvedValueOnce({ status: 'verified' })
      .mockResolvedValueOnce({
        status: 'unknown',
        fingerprint: 'SHA256:target',
        keyType: 'ssh-ed25519',
      });

    const result = await connectToSaved('saved-hop-hostkey', {
      createTab: vi.fn(),
      toast: vi.fn(),
      t: (key: string) => key,
      onHostKeyChallenge,
    });

    expect(result).toBeNull();
    expect(sessionTreeState.connectNode).toHaveBeenCalledTimes(1);
    expect(sessionTreeState.connectNode).toHaveBeenCalledWith('node-jump');
    expect(sessionTreeState.createTerminalForNode).not.toHaveBeenCalled();
    expect(onHostKeyChallenge).toHaveBeenCalledWith(expect.objectContaining({
      host: 'target.example.com',
      port: 22,
      status: expect.objectContaining({ status: 'unknown' }),
    }));
  });
});