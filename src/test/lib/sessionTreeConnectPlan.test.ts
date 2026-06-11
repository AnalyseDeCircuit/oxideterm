import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMutableSelectorStore } from '@/test/helpers/mockStore';

const sessionTreeState = vi.hoisted(() => ({
  getNodePath: vi.fn(),
  getRawNode: vi.fn(),
  linkDownNodeIds: new Set<string>(),
}));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: createMutableSelectorStore(sessionTreeState),
}));

import { buildExistingSessionTreeConnectPlan } from '@/lib/sessionTreeConnectPlan';

describe('buildExistingSessionTreeConnectPlan', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sessionTreeState.linkDownNodeIds = new Set();
    sessionTreeState.getRawNode.mockReturnValue(undefined);
  });

  it('skips connected ancestors and preflights the first disconnected child through its parent tunnel', async () => {
    const path = [
      {
        id: 'jump',
        host: 'jump.example.com',
        port: 22,
        state: { status: 'connected' },
        sshConnectionId: 'ssh-jump',
      },
      {
        id: 'target',
        host: 'target.internal',
        port: 2222,
        state: { status: 'pending' },
        sshConnectionId: null,
      },
    ];
    sessionTreeState.getNodePath.mockResolvedValue(path);
    sessionTreeState.getRawNode.mockImplementation((nodeId: string) =>
      path.find((node) => node.id === nodeId),
    );

    await expect(buildExistingSessionTreeConnectPlan('target')).resolves.toEqual({
      targetNodeId: 'target',
      currentIndex: 0,
      steps: [{
        nodeId: 'target',
        host: 'target.internal',
        port: 2222,
        upstreamProxy: undefined,
      }],
    });
  });

  it('keeps the upstream proxy only on a disconnected root hop', async () => {
    const path = [
      {
        id: 'root',
        host: 'root.example.com',
        port: 2222,
        state: { status: 'pending' },
        sshConnectionId: null,
      },
      {
        id: 'child',
        host: 'child.internal',
        port: 22,
        state: { status: 'pending' },
        sshConnectionId: null,
      },
    ];
    const upstreamProxy = {
      protocol: 'socks5' as const,
      host: 'proxy.local',
      port: 1080,
      auth: { type: 'none' as const },
      remoteDns: true,
      noProxy: '',
    };
    sessionTreeState.getNodePath.mockResolvedValue(path);
    sessionTreeState.getRawNode.mockImplementation((nodeId: string) =>
      path.find((node) => node.id === nodeId),
    );

    await expect(buildExistingSessionTreeConnectPlan('child', upstreamProxy)).resolves.toEqual({
      targetNodeId: 'child',
      currentIndex: 0,
      steps: [
        {
          nodeId: 'root',
          host: 'root.example.com',
          port: 2222,
          upstreamProxy,
        },
        {
          nodeId: 'child',
          host: 'child.internal',
          port: 22,
          upstreamProxy: undefined,
        },
      ],
    });
  });
});
