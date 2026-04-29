import { beforeEach, describe, expect, it, vi } from 'vitest';

const listAiTargetsMock = vi.hoisted(() => vi.fn());
const getAiTargetMock = vi.hoisted(() => vi.fn());
const ragSearchMock = vi.hoisted(() => vi.fn());

vi.mock('@/lib/ai/capabilities/targets', () => ({
  listAiTargets: listAiTargetsMock,
  getAiTarget: getAiTargetMock,
}));

vi.mock('@/lib/api', () => ({
  ragSearch: ragSearchMock,
  nodeAgentReadFile: vi.fn(),
  nodeAgentWriteFile: vi.fn(),
  nodeSftpDownload: vi.fn(),
  nodeSftpListDir: vi.fn(),
  nodeSftpPreview: vi.fn(),
  nodeSftpStartDirectoryTransfer: vi.fn(),
  nodeSftpUpload: vi.fn(),
  nodeSftpWrite: vi.fn(),
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: { getState: () => ({ tabs: [], activeTabId: null }) },
}));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: { getState: () => ({ nodes: [], selectedNodeId: null }) },
}));

vi.mock('@/store/localTerminalStore', () => ({
  useLocalTerminalStore: { getState: () => ({ terminals: new Map() }) },
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: {
    getState: () => ({
      settings: {
        ai: { enabled: true, toolUse: true },
        terminal: { renderer: 'auto', terminalEncoding: 'utf-8' },
        sftp: { directoryParallelism: 4 },
      },
    }),
  },
}));

vi.mock('@/store/transferStore', () => ({
  useTransferStore: {
    getState: () => ({
      getAllTransfers: () => [],
    }),
  },
}));

vi.mock('@/store/eventLogStore', () => ({
  useEventLogStore: { getState: () => ({ entries: [] }) },
}));

vi.mock('@/lib/terminalRegistry', () => ({
  getAllEntries: () => [],
}));

import { getState, readResource, selectAiTarget, writeResource } from '@/lib/ai/capabilities/resources';

describe('orchestrator resource capability', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('routes knowledge target selection through file-capable targets, not connections or local shell', async () => {
    listAiTargetsMock.mockResolvedValue([
      {
        id: 'rag-index:default',
        kind: 'rag-index',
        label: 'Knowledge base',
        state: 'available',
        capabilities: ['state.list', 'filesystem.search'],
        refs: {},
      },
    ]);

    const result = await selectAiTarget({ query: '插件开发文档', intent: 'knowledge' });

    expect(result.ok).toBe(true);
    expect(listAiTargetsMock).toHaveBeenCalledWith({
      query: '插件开发文档',
      kind: undefined,
      view: 'files',
    });
    expect(result.target).toMatchObject({
      id: 'rag-index:default',
      kind: 'rag-index',
    });
  });

  it('reads RAG resources from rag-index without requiring a path', async () => {
    ragSearchMock.mockResolvedValue([
      {
        docId: 'doc-1',
        chunkId: 'chunk-1',
        title: 'Plugin Development',
        text: 'Use plugin-api.d.ts for OxideTerm plugin APIs.',
        score: 0.91,
      },
    ]);

    const result = await readResource({
      target: {
        id: 'rag-index:default',
        kind: 'rag-index',
        label: 'Knowledge base',
        state: 'available',
        capabilities: ['state.list', 'filesystem.search'],
        refs: {},
      },
      resource: 'rag',
      query: '插件开发文档',
    });

    expect(result.ok).toBe(true);
    expect(ragSearchMock).toHaveBeenCalledWith({
      query: '插件开发文档',
      collectionIds: [],
      topK: 8,
    });
    expect(result.output).toContain('Plugin Development');
  });

  it('rejects command-like text as a target query', async () => {
    const result = await selectAiTarget({ query: 'pwd', intent: 'command' });

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe('command_query_not_target');
    expect(result.nextActions?.[0]).toMatchObject({
      action: 'list_targets',
      args: { view: 'live_sessions' },
    });
    expect(listAiTargetsMock).not.toHaveBeenCalled();
  });

  it('returns a recoverable error for unknown get_state scopes', async () => {
    const result = await getState('nonsense');

    expect(result.ok).toBe(false);
    expect(result.error).toMatchObject({
      code: 'unknown_state_scope',
      recoverable: true,
    });
    expect(result.verified).toBe(false);
  });

  it('rejects non-writable resource kinds', async () => {
    const result = await writeResource({
      target: {
        id: 'settings:app',
        kind: 'settings',
        label: 'Settings',
        state: 'available',
        capabilities: ['settings.read', 'settings.write'],
        refs: {},
      },
      resource: 'directory',
    });

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe('unsupported_resource_write');
    expect(result.nextActions?.[0]?.action).toBe('read_resource');
  });
});
