import { beforeEach, describe, expect, it, vi } from 'vitest';

const eventMocks = vi.hoisted(() => {
  const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
  const defaultListen = async (eventName: string, callback: (event: { payload: unknown }) => void) => {
    const set = listeners.get(eventName) ?? new Set();
    set.add(callback);
    listeners.set(eventName, set);
    return vi.fn(() => {
      listeners.get(eventName)?.delete(callback);
    });
  };

  return {
    listen: vi.fn(defaultListen),
    clear() {
      listeners.clear();
      this.listen.mockReset();
      this.listen.mockImplementation(defaultListen);
    },
  };
});

const apiMocks = vi.hoisted(() => ({
  nodeAgentDeploy: vi.fn(),
  nodeAgentRemove: vi.fn(),
  nodeAgentStatus: vi.fn(),
  nodeAgentReadFile: vi.fn(),
  nodeAgentWriteFile: vi.fn(),
  nodeAgentListTree: vi.fn(),
  nodeAgentGrep: vi.fn(),
  nodeAgentGitStatus: vi.fn(),
  nodeAgentWatchStart: vi.fn(),
  nodeAgentWatchStop: vi.fn(),
  nodeAgentStartWatchRelay: vi.fn(),
  nodeAgentSymbolIndex: vi.fn(),
  nodeAgentSymbolComplete: vi.fn(),
  nodeAgentSymbolDefinitions: vi.fn(),
  nodeSftpListDir: vi.fn(),
  nodeSftpPreview: vi.fn(),
  nodeSftpWrite: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: eventMocks.listen,
}));

vi.mock('@/lib/api', () => apiMocks);

import { ensureAgent, invalidateAgentCache, readFile, watchDirectory } from '@/lib/agentService';

function resetApiMocks(): void {
  Object.values(apiMocks).forEach((mockFn) => {
    mockFn.mockReset();
  });
}

describe('agentService.ensureAgent', () => {
  beforeEach(() => {
    resetApiMocks();
    eventMocks.clear();
    invalidateAgentCache('node-1');
  });

  it('retries deployment when the ready cache is stale', async () => {
    apiMocks.nodeAgentStatus
      .mockResolvedValueOnce({ type: 'failed', reason: 'channel closed' });
    apiMocks.nodeAgentDeploy
      .mockResolvedValueOnce({ type: 'ready', version: '1.0.0', arch: 'x86_64', pid: 1 })
      .mockResolvedValueOnce({ type: 'ready', version: '1.0.1', arch: 'x86_64', pid: 2 });

    await ensureAgent('node-1');
    const redeployed = await ensureAgent('node-1');

    expect(apiMocks.nodeAgentDeploy).toHaveBeenCalledTimes(2);
    expect(redeployed).toEqual({ type: 'ready', version: '1.0.1', arch: 'x86_64', pid: 2 });
  });

  it('returns manual intervention states without forcing a redeploy', async () => {
    apiMocks.nodeAgentStatus.mockResolvedValueOnce({
      type: 'manualUpdateRequired',
      arch: 'mips64',
      remotePath: '~/.oxideterm/oxideterm-agent',
      currentAgentVersion: '0.12.1',
      currentCompatibilityVersion: 1,
      expectedCompatibilityVersion: 2,
    });
    apiMocks.nodeAgentDeploy.mockResolvedValueOnce({ type: 'ready', version: '1.0.0', arch: 'x86_64', pid: 1 });

    await ensureAgent('node-1');
    const status = await ensureAgent('node-1');

    expect(apiMocks.nodeAgentDeploy).toHaveBeenCalledTimes(1);
    expect(status).toEqual({
      type: 'manualUpdateRequired',
      arch: 'mips64',
      remotePath: '~/.oxideterm/oxideterm-agent',
      currentAgentVersion: '0.12.1',
      currentCompatibilityVersion: 1,
      expectedCompatibilityVersion: 2,
    });
  });
});

describe('agentService.watchDirectory', () => {
  beforeEach(() => {
    resetApiMocks();
    eventMocks.clear();
    invalidateAgentCache('node-1');
    apiMocks.nodeAgentStatus.mockResolvedValue({ type: 'ready', version: '1.0.0', arch: 'x86_64', pid: 42 });
    apiMocks.nodeAgentWatchStart.mockResolvedValue(undefined);
    apiMocks.nodeAgentWatchStop.mockResolvedValue(undefined);
    apiMocks.nodeAgentStartWatchRelay.mockResolvedValue(undefined);
  });

  it('treats an already-started watch relay as non-fatal and still subscribes', async () => {
    apiMocks.nodeAgentStartWatchRelay.mockRejectedValueOnce(new Error('Watch relay already started'));

    const unlisten = await watchDirectory('node-1', '/srv/app', vi.fn());

    expect(unlisten).toBeTypeOf('function');
    expect(apiMocks.nodeAgentWatchStart).toHaveBeenCalledWith('node-1', '/srv/app', undefined);
    expect(apiMocks.nodeAgentStartWatchRelay).toHaveBeenCalledTimes(1);
    expect(eventMocks.listen).toHaveBeenCalledWith('agent:watch-event:node-1', expect.any(Function));

    await unlisten?.();
    expect(apiMocks.nodeAgentWatchStop).toHaveBeenCalledWith('node-1', '/srv/app');
  });

  it('starts the backend relay only once per node across multiple watches', async () => {
    const unlistenA = await watchDirectory('node-1', '/srv/app', vi.fn());
    const unlistenB = await watchDirectory('node-1', '/srv/app/src', vi.fn());

    expect(apiMocks.nodeAgentStartWatchRelay).toHaveBeenCalledTimes(1);
    expect(apiMocks.nodeAgentWatchStart).toHaveBeenNthCalledWith(1, 'node-1', '/srv/app', undefined);
    expect(apiMocks.nodeAgentWatchStart).toHaveBeenNthCalledWith(2, 'node-1', '/srv/app/src', undefined);

    await unlistenA?.();
    await unlistenB?.();
  });

  it('cleans up the remote watch if frontend listener setup fails', async () => {
    eventMocks.listen.mockRejectedValueOnce(new Error('listen failed'));

    const result = await watchDirectory('node-1', '/srv/app', vi.fn());

    expect(result).toBeNull();
    expect(apiMocks.nodeAgentWatchStart).toHaveBeenCalledWith('node-1', '/srv/app', undefined);
    expect(apiMocks.nodeAgentWatchStop).toHaveBeenCalledWith('node-1', '/srv/app');
  });

  it('restarts the relay after cache invalidation for the same node', async () => {
    const unlisten = await watchDirectory('node-1', '/srv/app', vi.fn());
    await unlisten?.();

    invalidateAgentCache('node-1');

    await watchDirectory('node-1', '/srv/app', vi.fn());

    expect(apiMocks.nodeAgentStartWatchRelay).toHaveBeenCalledTimes(2);
  });

  it('clears relay readiness after an agent transport failure so redeploy can restore watching', async () => {
    apiMocks.nodeAgentReadFile.mockRejectedValueOnce(new Error('channel closed'));
    apiMocks.nodeSftpPreview.mockResolvedValue({ Text: { data: 'fallback content' } });
    apiMocks.nodeAgentDeploy.mockResolvedValue({ type: 'ready', version: '1.0.1', arch: 'x86_64', pid: 7 });

    const firstUnlisten = await watchDirectory('node-1', '/srv/app', vi.fn());
    expect(apiMocks.nodeAgentStartWatchRelay).toHaveBeenCalledTimes(1);

    await readFile('node-1', '/srv/app/src/main.ts');
    await ensureAgent('node-1');

    const secondUnlisten = await watchDirectory('node-1', '/srv/app', vi.fn());

    expect(apiMocks.nodeAgentStartWatchRelay).toHaveBeenCalledTimes(2);

    await firstUnlisten?.();
    await secondUnlisten?.();
  });
});