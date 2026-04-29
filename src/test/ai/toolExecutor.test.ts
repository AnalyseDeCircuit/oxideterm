import { beforeEach, describe, expect, it, vi } from 'vitest';

const settingsState = vi.hoisted(() => ({
  settings: {
    ai: {
      providers: [
        {
          id: 'provider-1',
          name: 'Provider One',
          type: 'openai-compatible',
          enabled: true,
          baseUrl: 'https://example.invalid/v1',
          apiKey: 'secret-key',
        },
      ],
      toolUse: {
        enabled: true,
        autoApproveTools: {},
        disabledTools: [],
      },
      mcpServers: [
        {
          id: 'mcp-1',
          name: 'ops',
          transport: 'stdio',
          command: 'npx',
          args: ['-y', '@modelcontextprotocol/server-filesystem', '--api-key=secret-inline', '--token', 'secret-following'],
          authHeaderName: 'X-API-Key',
          authHeaderMode: 'raw',
          headers: {
            'X-Workspace': 'prod',
            'X-Trace-Token': 'secret-header',
          } as Record<string, string>,
          env: {
            API_TOKEN: 'super-secret',
            DEBUG: '1',
          } as Record<string, string>,
          authToken: 'legacy-secret',
          enabled: true,
          retryOnDisconnect: false,
        },
      ],
    },
    localTerminal: {
      defaultShellId: null,
      recentShellIds: [],
      defaultCwd: null,
      loadShellProfile: true,
      ohMyPoshEnabled: false,
      ohMyPoshTheme: null,
      customEnvVars: {
        SSH_AUTH_SOCK: '/private/tmp/socket',
        INTERNAL_TOKEN: 'very-secret',
      },
    },
    terminal: {
      fontSize: 14,
    },
  },
}));

const localExecCommandMock = vi.hoisted(() => vi.fn());
const nodeIdeExecCommandMock = vi.hoisted(() => vi.fn());
const nodeGetStateMock = vi.hoisted(() => vi.fn());
const nodeAgentStatusMock = vi.hoisted(() => vi.fn());
const nodeAgentReadFileMock = vi.hoisted(() => vi.fn());
const nodeAgentWriteFileMock = vi.hoisted(() => vi.fn());
const nodeSftpPreviewMock = vi.hoisted(() => vi.fn());
const nodeSftpStatMock = vi.hoisted(() => vi.fn());
const nodeSftpWriteMock = vi.hoisted(() => vi.fn());
const sshSetPoolConfigMock = vi.hoisted(() => vi.fn());
const getConnectionsMock = vi.hoisted(() => vi.fn());
const searchConnectionsMock = vi.hoisted(() => vi.fn());
const getAllBufferLinesMock = vi.hoisted(() => vi.fn());
const getBufferStatsMock = vi.hoisted(() => vi.fn());
const getScrollBufferMock = vi.hoisted(() => vi.fn());
const connectToSavedMock = vi.hoisted(() => vi.fn());
const findPaneBySessionIdMock = vi.hoisted(() => vi.fn());
const getTerminalBufferMock = vi.hoisted(() => vi.fn());
const writeToTerminalMock = vi.hoisted(() => vi.fn());
const subscribeTerminalOutputMock = vi.hoisted(() => vi.fn());
const waitForTerminalReadyMock = vi.hoisted(() => vi.fn());
const readScreenMock = vi.hoisted(() => vi.fn());
const beginTerminalCommandMarkMock = vi.hoisted(() => vi.fn());
const createTabMock = vi.hoisted(() => vi.fn());

const sessionTreeState = vi.hoisted(() => ({
  nodes: [] as Array<Record<string, unknown>>,
  getNode: vi.fn(),
  getNodeByTerminalId: vi.fn(),
}));

const appStoreState = vi.hoisted(() => ({
  sessions: new Map(),
  tabs: [] as Array<Record<string, unknown>>,
  activeTabId: null as string | null,
  createTab: createTabMock,
}));

const localTerminalState = vi.hoisted(() => ({
  terminals: new Map<string, Record<string, unknown>>(),
  createTerminal: vi.fn(),
}));

const ideStoreState = vi.hoisted(() => ({
  tabs: [] as Array<Record<string, unknown>>,
  activeTabId: null as string | null,
  activeFileId: null as string | null,
  nodeId: null as string | null,
  project: null as Record<string, unknown> | null,
  openFile: vi.fn(),
  createFile: vi.fn(),
  saveFile: vi.fn(),
  replaceStringInTab: vi.fn(),
  insertTextInTab: vi.fn(),
}));

const mcpRegistryState = vi.hoisted(() => ({
  findServerForTool: vi.fn(),
  callTool: vi.fn(),
  getAllMcpResources: vi.fn(() => []),
  readResource: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  api: {
    localExecCommand: localExecCommandMock,
    sshSetPoolConfig: sshSetPoolConfigMock,
    getConnections: getConnectionsMock,
    searchConnections: searchConnectionsMock,
    getAllBufferLines: getAllBufferLinesMock,
    getBufferStats: getBufferStatsMock,
    getScrollBuffer: getScrollBufferMock,
  },
  ragSearch: vi.fn(),
  nodeIdeExecCommand: nodeIdeExecCommandMock,
  nodeGetState: nodeGetStateMock,
  nodeAgentStatus: nodeAgentStatusMock,
  nodeAgentReadFile: nodeAgentReadFileMock,
  nodeAgentWriteFile: nodeAgentWriteFileMock,
  nodeAgentListTree: vi.fn(),
  nodeAgentGrep: vi.fn(),
  nodeAgentGitStatus: vi.fn(),
  nodeSftpListDir: vi.fn(),
  nodeSftpPreview: nodeSftpPreviewMock,
  nodeSftpStat: nodeSftpStatMock,
  nodeSftpWrite: nodeSftpWriteMock,
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: {
    getState: () => settingsState,
  },
}));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: {
    getState: () => sessionTreeState,
  },
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: {
    getState: () => appStoreState,
  },
}));

vi.mock('@/store/localTerminalStore', () => ({
  useLocalTerminalStore: {
    getState: () => localTerminalState,
  },
}));

vi.mock('@/store/ideStore', () => ({
  useIdeStore: {
    getState: () => ideStoreState,
    setState: (updater: Record<string, unknown> | ((state: typeof ideStoreState) => Record<string, unknown>)) => {
      const next = typeof updater === 'function' ? updater(ideStoreState) : updater;
      Object.assign(ideStoreState, next);
    },
  },
}));

vi.mock('@/store/pluginStore', () => ({
  usePluginStore: {
    getState: () => ({ plugins: [] }),
  },
}));

vi.mock('@/store/eventLogStore', () => ({
  useEventLogStore: {
    getState: () => ({ entries: [] }),
  },
}));

vi.mock('@/store/transferStore', () => ({
  useTransferStore: {
    getState: () => ({ queue: [], history: [] }),
  },
}));

vi.mock('@/store/recordingStore', () => ({
  useRecordingStore: {
    getState: () => ({ activeRecording: null }),
  },
}));

vi.mock('@/store/broadcastStore', () => ({
  useBroadcastStore: {
    getState: () => ({ enabled: false, sessionIds: [] }),
  },
}));

vi.mock('@/lib/terminalRegistry', () => ({
  beginTerminalCommandMark: beginTerminalCommandMarkMock,
  findPaneBySessionId: findPaneBySessionIdMock,
  getTerminalBuffer: getTerminalBufferMock,
  writeToTerminal: writeToTerminalMock,
  subscribeTerminalOutput: subscribeTerminalOutputMock,
  waitForTerminalReady: waitForTerminalReadyMock,
  readScreen: readScreenMock,
}));

vi.mock('@/lib/connectToSaved', () => ({
  connectToSaved: connectToSavedMock,
}));

vi.mock('@/lib/ai/providerRegistry', () => ({
  getProvider: vi.fn(),
  getProviderReasoningProtocol: () => 'none',
}));

vi.mock('@/lib/ai/tools/outputCompressor', () => ({
  compressOutput: (value: string) => value,
}));

vi.mock('@/lib/ai/contextSanitizer', () => ({
  sanitizeConnectionInfo: (value: unknown) => value,
}));

vi.mock('@/lib/ai/mcp', () => ({
  useMcpRegistry: {
    getState: () => mcpRegistryState,
  },
}));

import { executeTool } from '@/lib/ai/tools/toolExecutor';

describe('toolExecutor get_settings sanitization', () => {
  beforeEach(() => {
    localExecCommandMock.mockReset();
    nodeIdeExecCommandMock.mockReset();
    nodeGetStateMock.mockReset();
    nodeGetStateMock.mockResolvedValue({ state: { readiness: 'ready', sftpCwd: '/' } });
    nodeAgentStatusMock.mockReset();
    nodeAgentStatusMock.mockResolvedValue({ type: 'error' });
    nodeAgentReadFileMock.mockReset();
    nodeAgentWriteFileMock.mockReset();
    nodeSftpPreviewMock.mockReset();
    nodeSftpStatMock.mockReset();
    nodeSftpWriteMock.mockReset();
    sshSetPoolConfigMock.mockReset();
    sshSetPoolConfigMock.mockResolvedValue(undefined);
    getConnectionsMock.mockReset();
    getConnectionsMock.mockResolvedValue([]);
    searchConnectionsMock.mockReset();
    searchConnectionsMock.mockResolvedValue([]);
    getAllBufferLinesMock.mockReset();
    getAllBufferLinesMock.mockRejectedValue(new Error('no backend buffer'));
    getBufferStatsMock.mockReset();
    getBufferStatsMock.mockRejectedValue(new Error('no backend buffer stats'));
    getScrollBufferMock.mockReset();
    getScrollBufferMock.mockRejectedValue(new Error('no backend scroll buffer'));
    connectToSavedMock.mockReset();
    findPaneBySessionIdMock.mockReset();
    getTerminalBufferMock.mockReset();
    writeToTerminalMock.mockReset();
    subscribeTerminalOutputMock.mockReset();
    subscribeTerminalOutputMock.mockImplementation(() => () => {});
    waitForTerminalReadyMock.mockReset();
    waitForTerminalReadyMock.mockResolvedValue({ ready: true, state: null });
    readScreenMock.mockReset();
    createTabMock.mockReset();
    sessionTreeState.nodes = [];
    sessionTreeState.getNode.mockReset();
    sessionTreeState.getNodeByTerminalId.mockReset();
    appStoreState.tabs = [];
    appStoreState.activeTabId = null;
    localTerminalState.terminals = new Map();
    localTerminalState.createTerminal.mockReset();
    ideStoreState.tabs = [];
    ideStoreState.activeTabId = null;
    ideStoreState.activeFileId = null;
    ideStoreState.nodeId = null;
    ideStoreState.project = null;
    ideStoreState.openFile.mockReset();
    ideStoreState.createFile.mockReset();
    ideStoreState.saveFile.mockReset();
    ideStoreState.replaceStringInTab.mockReset();
    ideStoreState.insertTextInTab.mockReset();
    mcpRegistryState.findServerForTool.mockReset();
    mcpRegistryState.callTool.mockReset();
    mcpRegistryState.getAllMcpResources.mockReset();
    mcpRegistryState.getAllMcpResources.mockReturnValue([]);
    mcpRegistryState.readResource.mockReset();
    settingsState.settings.ai.mcpServers[0].env = {
      API_TOKEN: 'super-secret',
      DEBUG: '1',
    };
    settingsState.settings.ai.mcpServers[0].authToken = 'legacy-secret';
    settingsState.settings.localTerminal.customEnvVars = {
      SSH_AUTH_SOCK: '/private/tmp/socket',
      INTERNAL_TOKEN: 'very-secret',
    };
  });

  it('redacts MCP env values and legacy auth token metadata in ai settings', async () => {
    const result = await executeTool('get_settings', { section: 'ai' }, { activeNodeId: null, activeAgentAvailable: false });

    expect(result.success).toBe(true);
    const parsed = JSON.parse(result.output);

    expect(parsed.providers).toEqual([
      {
        id: 'provider-1',
        name: 'Provider One',
        type: 'openai-compatible',
        enabled: true,
      },
    ]);
    expect(parsed.providers[0]).not.toHaveProperty('baseUrl');
    expect(parsed.providers[0]).not.toHaveProperty('apiKey');

    expect(parsed.mcpServers).toEqual([
      {
        id: 'mcp-1',
        name: 'ops',
        transport: 'stdio',
        command: 'npx',
        authHeaderName: 'X-API-Key',
        authHeaderMode: 'raw',
        args: ['-y', '@modelcontextprotocol/server-filesystem', '--api-key=[redacted]', '--token', '[redacted]'],
        env: {
          API_TOKEN: '[redacted]',
          DEBUG: '[redacted]',
        },
        headers: {
          'X-Workspace': '[redacted]',
          'X-Trace-Token': '[redacted]',
        },
        enabled: true,
        retryOnDisconnect: false,
        hasLegacyAuthToken: true,
      },
    ]);
    expect(parsed.mcpServers[0]).not.toHaveProperty('authToken');
  });

  it('preserves explicit empty MCP env objects while still redacting values', async () => {
    settingsState.settings.ai.mcpServers[0].env = {} as Record<string, string>;

    const result = await executeTool('get_settings', { section: 'ai' }, { activeNodeId: null, activeAgentAvailable: false });

    expect(result.success).toBe(true);
    const parsed = JSON.parse(result.output);
    expect(parsed.mcpServers[0].env).toEqual({});
  });

  it('redacts local terminal custom env vars in section and full settings output', async () => {
    const sectionResult = await executeTool('get_settings', { section: 'localTerminal' }, { activeNodeId: null, activeAgentAvailable: false });
    const fullResult = await executeTool('get_settings', {}, { activeNodeId: null, activeAgentAvailable: false });

    expect(sectionResult.success).toBe(true);
    expect(fullResult.success).toBe(true);

    const sectionParsed = JSON.parse(sectionResult.output);
    const fullParsed = JSON.parse(fullResult.output);

    const expectedEnv = {
      INTERNAL_TOKEN: '[redacted]',
      SSH_AUTH_SOCK: '[redacted]',
    };

    expect(sectionParsed.customEnvVars).toEqual(expectedEnv);
    expect(fullParsed.localTerminal.customEnvVars).toEqual(expectedEnv);
  });

  it('passes explicit dangerous-command approval through local_exec', async () => {
    localExecCommandMock.mockResolvedValue({ stdout: 'ok', stderr: '', exitCode: 0, timedOut: false });

    const result = await executeTool(
      'local_exec',
      { command: 'sudo reboot', timeout_secs: 5 },
      { activeNodeId: null, activeAgentAvailable: false },
      { dangerousCommandApproved: true },
    );

    expect(result.success).toBe(true);
    expect(localExecCommandMock).toHaveBeenCalledWith('sudo reboot', undefined, 5, true);
  });

  it('does not mark local_exec as approved unless caller passes explicit approval', async () => {
    localExecCommandMock.mockResolvedValue({ stdout: 'ok', stderr: '', exitCode: 0, timedOut: false });

    await executeTool(
      'local_exec',
      { command: 'sudo reboot', timeout_secs: 5 },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(localExecCommandMock).toHaveBeenCalledWith('sudo reboot', undefined, 5, false);
  });

  it('reads terminal buffer through paged backend APIs instead of full-buffer fetch', async () => {
    getBufferStatsMock.mockResolvedValue({ current_lines: 1200, total_lines: 1200, max_lines: 100000, memory_usage_mb: 2 });
    getScrollBufferMock.mockResolvedValue([
      { text: 'tail line 1' },
      { text: 'tail line 2' },
      { text: 'tail line 3' },
    ]);

    const result = await executeTool(
      'get_terminal_buffer',
      { session_id: 'session-1', max_lines: 3 },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('tail line 1');
    expect(getBufferStatsMock).toHaveBeenCalledWith('session-1');
    expect(getScrollBufferMock).toHaveBeenCalledWith('session-1', 1197, 3);
    expect(getAllBufferLinesMock).not.toHaveBeenCalled();
  });

  it('prefers rendered terminal buffer for open sessions', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');
    getTerminalBufferMock.mockReturnValue('decoded line 1\ndecoded line 2');
    getBufferStatsMock.mockResolvedValue({ current_lines: 1200, total_lines: 1200, max_lines: 100000, memory_usage_mb: 2 });

    const result = await executeTool(
      'get_terminal_buffer',
      { session_id: 'session-1', max_lines: 1 },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('decoded line 2');
    expect(result.output).not.toContain('decoded line 1');
    expect(getBufferStatsMock).not.toHaveBeenCalled();
  });

  it('uses activeSessionId fallback for get_terminal_buffer when session_id is omitted', async () => {
    getBufferStatsMock.mockResolvedValue({ current_lines: 1000, total_lines: 1000, max_lines: 100000, memory_usage_mb: 2 });
    getScrollBufferMock.mockResolvedValue([
      { text: 'active line 1' },
      { text: 'active line 2' },
    ]);

    const result = await executeTool(
      'get_terminal_buffer',
      { max_lines: 2 },
      { activeNodeId: null, activeAgentAvailable: false, activeSessionId: 'active-session', activeTerminalType: 'local_terminal' },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('active line 1');
    expect(getBufferStatsMock).toHaveBeenCalledWith('active-session');
    expect(getScrollBufferMock).toHaveBeenCalledWith('active-session', 998, 2);
  });

  it('uses activeSessionId fallback for terminal_exec when session_id is omitted', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');
    getBufferStatsMock.mockResolvedValue({ current_lines: 0, total_lines: 0, max_lines: 100000, memory_usage_mb: 0 });
    writeToTerminalMock.mockReturnValue(true);

    const result = await executeTool(
      'terminal_exec',
      { command: 'pwd', await_output: false },
      { activeNodeId: null, activeAgentAvailable: false, activeSessionId: 'active-session', activeTerminalType: 'local_terminal' },
    );

    expect(result.success).toBe(true);
    expect(writeToTerminalMock).toHaveBeenCalledWith('pane-1', 'pwd\r');
    expect(result.output).toContain('active-session');

    writeToTerminalMock.mockClear();
    findPaneBySessionIdMock.mockClear();
  });

  it('blocks manual ssh in a visible terminal when a matching saved connection exists', async () => {
    searchConnectionsMock.mockResolvedValue([
      { id: 'saved-1', host: '192.168.31.192', port: 22, username: 'lipsc', name: 'Home Local', group: 'Home' },
    ]);
    findPaneBySessionIdMock.mockReturnValue('pane-1');
    getBufferStatsMock.mockResolvedValue({ current_lines: 0, total_lines: 0, max_lines: 100000, memory_usage_mb: 0 });
    writeToTerminalMock.mockReturnValue(true);

    const result = await executeTool(
      'terminal_exec',
      { command: 'ssh lipsc@192.168.31.192', await_output: false },
      { activeNodeId: null, activeAgentAvailable: false, activeSessionId: 'active-session', activeTerminalType: 'local_terminal' },
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain('saved connection');
    expect(writeToTerminalMock).not.toHaveBeenCalled();
    expect(result.envelope?.nextActions?.[0]).toMatchObject({
      tool: 'connect_saved_session',
      args: { connection_id: 'saved-1' },
      priority: 'recommended',
    });
  });

  it('allows manual ssh when no saved connection matches', async () => {
    searchConnectionsMock.mockResolvedValue([]);
    findPaneBySessionIdMock.mockReturnValue('pane-1');
    getBufferStatsMock.mockResolvedValue({ current_lines: 0, total_lines: 0, max_lines: 100000, memory_usage_mb: 0 });
    writeToTerminalMock.mockReturnValue(true);

    const result = await executeTool(
      'terminal_exec',
      { command: 'ssh someone@203.0.113.10', await_output: false },
      { activeNodeId: null, activeAgentAvailable: false, activeSessionId: 'active-session', activeTerminalType: 'local_terminal' },
    );

    expect(result.success).toBe(true);
    expect(writeToTerminalMock).toHaveBeenCalledWith('pane-1', 'ssh someone@203.0.113.10\r');
  });

  it('lists unified targets for SSH, local terminal, and tab routing', async () => {
    sessionTreeState.nodes = [
      {
        id: 'node-1',
        host: 'example.com',
        username: 'root',
        port: 22,
        runtime: {
          status: 'connected',
          connectionId: 'conn-1',
          terminalIds: ['term-1'],
          sftpSessionId: 'sftp-1',
        },
      },
    ];
    localTerminalState.terminals = new Map([
      ['local-1', { running: true, shell: { label: 'Zsh' } }],
    ]);
    appStoreState.activeTabId = 'tab-ssh';
    appStoreState.tabs = [
      {
        id: 'tab-ssh',
        type: 'terminal',
        title: 'SSH',
        nodeId: 'node-1',
        sessionId: 'term-1',
      },
    ];

    const result = await executeTool(
      'list_targets',
      {},
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('ssh-node:node-1');
    expect(result.output).toContain('terminal-session:term-1');
    expect(result.output).toContain('terminal-session:local-1');

    const payload = JSON.parse(result.output.slice(result.output.indexOf('JSON:') + 'JSON:'.length));
    expect(payload.targets).toEqual(expect.arrayContaining([
      expect.objectContaining({ id: 'ssh-node:node-1', kind: 'ssh-node' }),
      expect.objectContaining({ id: 'terminal-session:term-1', kind: 'terminal-session' }),
      expect.objectContaining({ id: 'tab:tab-ssh', kind: 'app-tab', active: true }),
    ]));
  });

  it('lists capabilities for a selected discovered target', async () => {
    sessionTreeState.nodes = [
      {
        id: 'node-1',
        host: 'example.com',
        username: 'root',
        port: 22,
        runtime: {
          status: 'connected',
          connectionId: 'conn-1',
          terminalIds: ['term-1'],
        },
      },
    ];

    const result = await executeTool(
      'list_capabilities',
      { target_id: 'ssh-node:node-1' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('command.run on ssh-node:node-1');
    expect(result.output).toContain('filesystem.read on ssh-node:node-1');

    const payload = JSON.parse(result.output.slice(result.output.indexOf('JSON:') + 'JSON:'.length));
    expect(payload.capabilities).toEqual(expect.arrayContaining([
      expect.objectContaining({ targetId: 'ssh-node:node-1', capability: 'command.run' }),
      expect.objectContaining({ targetId: 'ssh-node:node-1', capability: 'filesystem.read' }),
    ]));
  });

  it('resolves saved connection targets before opening a manual shell', async () => {
    searchConnectionsMock.mockResolvedValue([
      { id: 'saved-1', host: '192.168.31.192', port: 22, username: 'lipsc', name: '家里的主机本地', group: 'Home' },
    ]);

    const result = await executeTool(
      'resolve_target',
      { query: '家里的主机本地', intent: 'connection' },
      { activeNodeId: null, activeAgentAvailable: false, activeSessionId: 'local-1', activeTerminalType: 'local_terminal', requireExplicitTarget: true },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('saved-connection:saved-1');
    expect(result.envelope?.targets?.[0]).toMatchObject({
      id: 'saved-connection:saved-1',
      kind: 'saved-connection',
    });
    expect(result.envelope?.nextActions?.[0]).toMatchObject({
      tool: 'connect_saved_session',
      args: { connection_id: 'saved-1' },
    });
  });

  it('rejects broad connection discovery through resolve_target with list guidance', async () => {
    const result = await executeTool(
      'resolve_target',
      { query: '看看现在有哪些远程主机可供链接', intent: 'connection' },
      { activeNodeId: null, activeAgentAvailable: false, activeSessionId: 'local-1', activeTerminalType: 'local_terminal', requireExplicitTarget: true },
    );

    expect(result.success).toBe(false);
    expect(result.envelope?.error?.code).toBe('target_query_too_broad');
    expect(result.envelope?.nextActions?.[0]?.tool).toBe('list_saved_connections');
    expect(searchConnectionsMock).not.toHaveBeenCalled();
  });

  it('lists capabilities for a resolved saved-connection target', async () => {
    const result = await executeTool(
      'list_capabilities',
      { target_id: 'saved-connection:saved-1' },
      { activeNodeId: null, activeAgentAvailable: false, requireExplicitTarget: true },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('navigation.open on saved-connection:saved-1');
    expect(result.output).toContain('state.list on saved-connection:saved-1');
  });

  it('rejects terminal_exec on saved-connection targets with connect guidance', async () => {
    const result = await executeTool(
      'terminal_exec',
      { target_id: 'saved-connection:saved-1', command: 'docker ps' },
      { activeNodeId: null, activeAgentAvailable: false, requireExplicitTarget: true },
    );

    expect(result.success).toBe(false);
    expect(result.envelope?.error?.code).toBe('saved_connection_not_connected');
    expect(result.envelope?.nextActions?.[0]).toMatchObject({
      tool: 'connect_saved_session',
      args: { connection_id: 'saved-1' },
    });
  });

  it('requires an explicit target for terminal_exec in target-first mode', async () => {
    const result = await executeTool(
      'terminal_exec',
      { command: 'pwd' },
      { activeNodeId: 'node-1', activeAgentAvailable: true, requireExplicitTarget: true },
    );

    expect(result.success).toBe(false);
    expect(result.envelope?.error?.code).toBe('target_required');
    expect(result.envelope?.nextActions?.[0]?.tool).toBe('resolve_target');
  });

  it('maps terminal_exec target_id to direct SSH node execution', async () => {
    sessionTreeState.nodes = [
      { id: 'node-1', host: 'example.com', username: 'root', port: 22, runtime: { status: 'connected', terminalIds: [] } },
    ];
    nodeIdeExecCommandMock.mockResolvedValue({ stdout: '/home/root\n', stderr: '', exitCode: 0 });

    const result = await executeTool(
      'terminal_exec',
      { target_id: 'ssh-node:node-1', command: 'pwd' },
      { activeNodeId: null, activeAgentAvailable: false, requireExplicitTarget: true },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('/home/root');
    expect(nodeIdeExecCommandMock).toHaveBeenCalledWith('node-1', 'pwd', undefined, 30);
    expect(result.envelope?.meta.targetId).toBe('ssh-node:node-1');
  });

  it('maps terminal_exec target_id to visible terminal session execution', async () => {
    sessionTreeState.nodes = [
      { id: 'node-1', host: 'example.com', username: 'root', port: 22, runtime: { status: 'connected', terminalIds: ['term-1'] } },
    ];
    findPaneBySessionIdMock.mockReturnValue('pane-1');
    getBufferStatsMock.mockResolvedValue({ current_lines: 0, total_lines: 0, max_lines: 100000, memory_usage_mb: 0 });
    writeToTerminalMock.mockReturnValue(true);

    const result = await executeTool(
      'terminal_exec',
      { target_id: 'terminal-session:term-1', command: 'pwd', await_output: false },
      { activeNodeId: null, activeAgentAvailable: false, requireExplicitTarget: true },
    );

    expect(result.success).toBe(true);
    expect(writeToTerminalMock).toHaveBeenCalledWith('pane-1', 'pwd\r');
    expect(result.envelope?.meta.targetId).toBe('terminal-session:term-1');
  });

  it('returns file read metadata in the structured envelope', async () => {
    nodeAgentReadFileMock.mockResolvedValue({
      content: 'hello\n',
      hash: 'agent-hash-1',
      size: 6,
      mtime: 123,
      encoding: 'utf-8',
    });

    const result = await executeTool(
      'read_file',
      { path: '/tmp/a.txt' },
      { activeNodeId: 'node-1', activeAgentAvailable: true },
    );

    expect(result.success).toBe(true);
    expect(result.output).toBe('hello\n');
    expect(result.envelope?.data).toMatchObject({
      path: '/tmp/a.txt',
      contentHash: 'agent-hash-1',
      size: 6,
      mtime: 123,
    });
  });

  it('rejects write_file when expected hash no longer matches', async () => {
    nodeAgentReadFileMock.mockResolvedValue({
      content: 'old',
      hash: 'current-hash',
      size: 3,
      mtime: 10,
      encoding: 'utf-8',
    });

    const result = await executeTool(
      'write_file',
      { path: '/tmp/a.txt', content: 'new', expected_hash: 'stale-hash' },
      { activeNodeId: 'node-1', activeAgentAvailable: true },
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain('expected hash stale-hash');
    expect(result.envelope?.error).toMatchObject({
      code: 'expected_hash_mismatch',
      recoverable: true,
    });
    expect(nodeAgentWriteFileMock).not.toHaveBeenCalled();
  });

  it('supports write_file dry run without writing', async () => {
    nodeAgentReadFileMock.mockResolvedValue({
      content: 'old',
      hash: 'old-hash',
      size: 3,
      mtime: 10,
      encoding: 'utf-8',
    });

    const result = await executeTool(
      'write_file',
      { path: '/tmp/a.txt', content: 'new content', expected_hash: 'old-hash', dry_run: true },
      { activeNodeId: 'node-1', activeAgentAvailable: true },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('"dryRun": true');
    expect(result.envelope?.data).toMatchObject({
      path: '/tmp/a.txt',
      dryRun: true,
    });
    expect(nodeAgentWriteFileMock).not.toHaveBeenCalled();
  });

  it('marks legacy write_file without expected hash as an unconditional overwrite', async () => {
    nodeAgentWriteFileMock.mockResolvedValue({
      hash: 'new-hash',
      size: 3,
      mtime: 11,
      atomic: true,
    });

    const result = await executeTool(
      'write_file',
      { path: '/tmp/a.txt', content: 'new' },
      { activeNodeId: 'node-1', activeAgentAvailable: true },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('new-hash');
    expect(result.envelope?.warnings?.[0]).toContain('unconditional overwrite');
    expect(nodeAgentWriteFileMock).toHaveBeenCalledWith('node-1', '/tmp/a.txt', 'new', undefined);
  });

  it('appends write_file content by reading the existing text first', async () => {
    nodeAgentReadFileMock.mockResolvedValue({
      content: 'old',
      hash: 'old-hash',
      size: 3,
      mtime: 10,
      encoding: 'utf-8',
    });
    nodeAgentWriteFileMock.mockResolvedValue({
      hash: 'new-hash',
      size: 7,
      mtime: 11,
      atomic: true,
    });

    const result = await executeTool(
      'write_file',
      { path: '/tmp/a.txt', content: '\nnew', append: true },
      { activeNodeId: 'node-1', activeAgentAvailable: true },
    );

    expect(result.success).toBe(true);
    expect(nodeAgentWriteFileMock).toHaveBeenCalledWith('node-1', '/tmp/a.txt', 'old\nnew', undefined);
    expect(result.envelope?.data).toMatchObject({
      path: '/tmp/a.txt',
      size: 7,
    });
  });

  it('rejects sftp_write_file create_only when the target already exists', async () => {
    nodeSftpStatMock.mockResolvedValue({
      name: 'a.txt',
      path: '/tmp/a.txt',
      file_type: 'File',
      size: 3,
      modified: 10,
      permissions: null,
    });

    const result = await executeTool(
      'sftp_write_file',
      { path: '/tmp/a.txt', content: 'new', create_only: true },
      { activeNodeId: 'node-1', activeAgentAvailable: false },
    );

    expect(result.success).toBe(false);
    expect(result.envelope?.error).toMatchObject({
      code: 'file_exists',
      recoverable: true,
    });
    expect(nodeSftpWriteMock).not.toHaveBeenCalled();
  });

  it('requires ide_replace_string to match exactly once', async () => {
    ideStoreState.tabs = [
      {
        id: 'tab-1',
        path: '/repo/a.ts',
        name: 'a.ts',
        language: 'typescript',
        isDirty: false,
        content: 'const a = 1;\nconst a = 1;\n',
      },
    ];

    const result = await executeTool(
      'ide_replace_string',
      { tab_id: 'tab-1', old_string: 'const a = 1;', new_string: 'const a = 2;' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(false);
    expect(result.envelope?.error).toMatchObject({
      code: 'replace_string_not_unique',
      recoverable: true,
    });
    expect(ideStoreState.replaceStringInTab).not.toHaveBeenCalled();
  });
});

describe('toolExecutor regressions', () => {
  beforeEach(() => {
    findPaneBySessionIdMock.mockReset();
    getTerminalBufferMock.mockReset();
    writeToTerminalMock.mockReset();
    subscribeTerminalOutputMock.mockReset();
    subscribeTerminalOutputMock.mockImplementation(() => () => {});
    waitForTerminalReadyMock.mockReset();
    waitForTerminalReadyMock.mockResolvedValue({ ready: true, state: null });
    getBufferStatsMock.mockReset();
    getBufferStatsMock.mockRejectedValue(new Error('no backend buffer stats'));
    getScrollBufferMock.mockReset();
    getScrollBufferMock.mockRejectedValue(new Error('no backend scroll buffer'));
  });

  it('captures fast terminal_exec session output when data arrives immediately after write', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');

    let currentLines = 0;
    const renderedLines = [
      { text: '=== 终端已就绪 ===' },
      { text: '/Users/dominical' },
      { text: 'Shell: /bin/zsh' },
      { text: 'dominical@macbook %' },
    ];

    let outputListener: (() => void) | undefined;
    subscribeTerminalOutputMock.mockImplementation((_sessionId: string, listener: () => void) => {
      outputListener = listener;
      return () => {
        outputListener = undefined;
      };
    });

    getBufferStatsMock.mockImplementation(async () => ({
      current_lines: currentLines,
      total_lines: currentLines,
      max_lines: 100000,
      memory_usage_mb: 1,
    }));
    getScrollBufferMock.mockImplementation(async (_sessionId: string, startLine: number, count: number) => (
      renderedLines.slice(startLine, startLine + count)
    ));

    writeToTerminalMock.mockImplementation(() => {
      currentLines = renderedLines.length;
      outputListener?.();
      return true;
    });

    const result = await executeTool(
      'terminal_exec',
      { session_id: 'session-1', command: 'echo ready && pwd && echo "Shell: $SHELL"' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('=== 终端已就绪 ===');
    expect(result.output).toContain('/Users/dominical');
    expect(result.output).toContain('Shell: /bin/zsh');
    expect(subscribeTerminalOutputMock.mock.invocationCallOrder[0]).toBeLessThan(writeToTerminalMock.mock.invocationCallOrder[0]);
  });

  it('captures terminal_exec output from the output notification path', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');

    let currentLines = 0;
    const renderedLines = [
      { text: 'pwd' },
      { text: '/Users/dominical/Documents/OxideTerm' },
      { text: 'dominical@macbook %' },
    ];

    let outputListener: (() => void) | undefined;
    subscribeTerminalOutputMock.mockImplementation((_sessionId: string, listener: () => void) => {
      outputListener = listener;
      return () => {
        outputListener = undefined;
      };
    });

    getBufferStatsMock.mockImplementation(async () => ({
      current_lines: currentLines,
      total_lines: currentLines,
      max_lines: 100000,
      memory_usage_mb: 1,
    }));
    getScrollBufferMock.mockImplementation(async (_sessionId: string, startLine: number, count: number) => (
      renderedLines.slice(startLine, startLine + count)
    ));

    writeToTerminalMock.mockImplementation(() => {
      currentLines = renderedLines.length;
      outputListener?.();
      return true;
    });

    const result = await executeTool(
      'terminal_exec',
      { session_id: 'session-1', command: 'pwd' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('/Users/dominical/Documents/OxideTerm');
    expect(result.output).not.toContain('No new output after');
  });

  it('waits for terminal readiness before writing terminal_exec input', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');
    waitForTerminalReadyMock.mockResolvedValue({
      ready: false,
      state: {
        sessionId: 'session-1',
        terminalType: 'local_terminal',
        writerReady: true,
        frontendOutputListenerReady: false,
        renderBufferReady: true,
        backendBufferReady: true,
        updatedAt: Date.now(),
      },
      reason: 'timeout',
    });

    const result = await executeTool(
      'terminal_exec',
      { session_id: 'session-1', command: 'pwd' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain('Terminal session is not ready for interactive input yet');
    expect(result.error).toContain('output listener');
    expect(writeToTerminalMock).not.toHaveBeenCalled();
  });

  it('waits for local terminal readiness before reporting terminal_exec readiness', async () => {
    localTerminalState.createTerminal.mockResolvedValue({
      id: 'local-session-1',
      shell: { label: 'Zsh' },
      running: true,
      cols: 80,
      rows: 24,
    });
    waitForTerminalReadyMock.mockResolvedValue({
      ready: true,
      state: {
        sessionId: 'local-session-1',
        terminalType: 'local_terminal',
        writerReady: true,
        frontendOutputListenerReady: true,
        renderBufferReady: true,
        backendBufferReady: true,
        updatedAt: Date.now(),
      },
    });

    const result = await executeTool(
      'open_local_terminal',
      {},
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(createTabMock).toHaveBeenCalledWith('local_terminal', 'local-session-1', undefined);
    expect(waitForTerminalReadyMock).toHaveBeenCalledWith('local-session-1', { timeoutMs: 3000 });
    expect(result.output).toContain('Terminal is ready for terminal_exec');
  });

  it('returns rendered terminal_exec output when backend buffer text is mojibake', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');

    let currentLines = 1;
    let renderedBuffer = 'prompt %';
    const backendLines = [
      { text: 'prompt %' },
      { text: '�������' },
      { text: 'prompt %' },
    ];

    let outputListener: (() => void) | undefined;
    subscribeTerminalOutputMock.mockImplementation((_sessionId: string, listener: () => void) => {
      outputListener = listener;
      return () => {
        outputListener = undefined;
      };
    });

    getTerminalBufferMock.mockImplementation(() => renderedBuffer);
    getBufferStatsMock.mockImplementation(async () => ({
      current_lines: currentLines,
      total_lines: currentLines,
      max_lines: 100000,
      memory_usage_mb: 1,
    }));
    getScrollBufferMock.mockImplementation(async (_sessionId: string, startLine: number, count: number) => (
      backendLines.slice(startLine, startLine + count)
    ));

    writeToTerminalMock.mockImplementation(() => {
      currentLines = backendLines.length;
      renderedBuffer = 'prompt %\n中文输出\nprompt %';
      outputListener?.();
      return true;
    });

    const result = await executeTool(
      'terminal_exec',
      { session_id: 'session-1', command: 'echo 中文' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('中文输出');
    expect(result.output).not.toContain('�������');
  });

  it('returns rendered current-line output when terminal_exec waits for sudo password', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');

    let renderedBuffer = 'dominical@macbook %';
    let outputListener: (() => void) | undefined;
    subscribeTerminalOutputMock.mockImplementation((_sessionId: string, listener: () => void) => {
      outputListener = listener;
      return () => {
        outputListener = undefined;
      };
    });

    getTerminalBufferMock.mockImplementation(() => renderedBuffer);
    getBufferStatsMock.mockResolvedValue({
      current_lines: 1,
      total_lines: 1,
      max_lines: 100000,
      memory_usage_mb: 1,
    });
    getScrollBufferMock.mockResolvedValue([{ text: 'dominical@macbook %' }]);

    writeToTerminalMock.mockImplementation(() => {
      renderedBuffer = 'dominical@macbook % sudo fastfetch [sudo] password for dominical:';
      outputListener?.();
      return true;
    });

    const result = await executeTool(
      'terminal_exec',
      { session_id: 'session-1', command: 'sudo fastfetch' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('[sudo] password for dominical:');
    expect(result.output).not.toContain('No new output after');
  });

  it('encodes terminal shortcut combos in send_keys instead of sending literal text', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');

    let currentLines = 0;
    const renderedLines = [
      { text: 'combo handled' },
      { text: 'dominical@macbook %' },
    ];

    let outputListener: (() => void) | undefined;
    subscribeTerminalOutputMock.mockImplementation((_sessionId: string, listener: () => void) => {
      outputListener = listener;
      return () => {
        outputListener = undefined;
      };
    });

    getBufferStatsMock.mockImplementation(async () => ({
      current_lines: currentLines,
      total_lines: currentLines,
      max_lines: 100000,
      memory_usage_mb: 1,
    }));
    getScrollBufferMock.mockImplementation(async (_sessionId: string, startLine: number, count: number) => (
      renderedLines.slice(startLine, startLine + count)
    ));

    const writes: string[] = [];
    writeToTerminalMock.mockImplementation((_paneId: string, data: string) => {
      writes.push(data);
      currentLines = renderedLines.length;
      outputListener?.();
      return true;
    });

    const result = await executeTool(
      'send_keys',
      {
        session_id: 'session-1',
        keys: ['Ctrl+C', 'Cmd+K', 'Shift+Tab', 'Ctrl+Shift+Left'],
        delay_ms: 10,
      },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(writes).toEqual(['\x03', '\x1bk', '\x1b[Z', '\x1b[1;6D']);
    expect(result.output).toContain('[Ctrl+C]');
    expect(result.output).toContain('[Cmd+K]');
    expect(subscribeTerminalOutputMock.mock.invocationCallOrder[0]).toBeLessThan(writeToTerminalMock.mock.invocationCallOrder[0]);
  });

  it('keeps send_keys subscribed until delayed terminal output arrives', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');

    let currentLines = 0;
    const renderedLines = [
      { text: 'delayed key response' },
      { text: 'dominical@macbook %' },
    ];

    let outputListener: (() => void) | undefined;
    subscribeTerminalOutputMock.mockImplementation((_sessionId: string, listener: () => void) => {
      outputListener = listener;
      return () => {
        outputListener = undefined;
      };
    });

    getBufferStatsMock.mockImplementation(async () => ({
      current_lines: currentLines,
      total_lines: currentLines,
      max_lines: 100000,
      memory_usage_mb: 1,
    }));
    getScrollBufferMock.mockImplementation(async (_sessionId: string, startLine: number, count: number) => (
      renderedLines.slice(startLine, startLine + count)
    ));

    writeToTerminalMock.mockImplementation(() => {
      setTimeout(() => {
        currentLines = renderedLines.length;
        outputListener?.();
      }, 20);
      return true;
    });

    const result = await executeTool(
      'send_keys',
      {
        session_id: 'session-1',
        keys: ['Enter'],
        delay_ms: 10,
      },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('delayed key response');
  });

  it('captures delayed terminal output after send_mouse input', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');
    readScreenMock.mockReturnValue({ cols: 80, rows: 24 });

    let currentLines = 0;
    const renderedLines = [
      { text: 'mouse click handled' },
      { text: 'dominical@macbook %' },
    ];

    let outputListener: (() => void) | undefined;
    subscribeTerminalOutputMock.mockImplementation((_sessionId: string, listener: () => void) => {
      outputListener = listener;
      return () => {
        outputListener = undefined;
      };
    });

    getBufferStatsMock.mockImplementation(async () => ({
      current_lines: currentLines,
      total_lines: currentLines,
      max_lines: 100000,
      memory_usage_mb: 1,
    }));
    getScrollBufferMock.mockImplementation(async (_sessionId: string, startLine: number, count: number) => (
      renderedLines.slice(startLine, startLine + count)
    ));

    const writes: string[] = [];
    writeToTerminalMock.mockImplementation((_paneId: string, data: string) => {
      writes.push(data);
      setTimeout(() => {
        currentLines = renderedLines.length;
        outputListener?.();
      }, 20);
      return true;
    });

    const result = await executeTool(
      'send_mouse',
      {
        session_id: 'session-1',
        action: 'click',
        x: 5,
        y: 3,
        button: 'left',
      },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('mouse click handled');
    expect(writes).toEqual(['\x1b[<0;5;3M\x1b[<0;5;3m']);
    expect(subscribeTerminalOutputMock.mock.invocationCallOrder[0]).toBeLessThan(writeToTerminalMock.mock.invocationCallOrder[0]);
  });

  it('cancels await_terminal_output when abort signal fires', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');
    getTerminalBufferMock.mockReturnValue('existing line');
    const abortController = new AbortController();

    const promise = executeTool(
      'await_terminal_output',
      { session_id: 'session-1', timeout_secs: 30 },
      { activeNodeId: null, activeAgentAvailable: false },
      { abortSignal: abortController.signal },
    );

    abortController.abort();

    await expect(promise).resolves.toMatchObject({
      success: false,
      error: 'Generation was stopped.',
    });
  });

  it('stops batch_exec before sending later commands after abort', async () => {
    findPaneBySessionIdMock.mockReturnValue('pane-1');
    getTerminalBufferMock.mockReturnValue('buffer');
    const abortController = new AbortController();
    writeToTerminalMock.mockImplementation((_paneId: string, data: string) => {
      if (data === 'first\r') {
        abortController.abort();
      }
      return true;
    });

    const result = await executeTool(
      'batch_exec',
      { session_id: 'session-1', commands: ['first', 'second'] },
      { activeNodeId: null, activeAgentAvailable: false },
      { abortSignal: abortController.signal },
    );

    expect(writeToTerminalMock).toHaveBeenCalledTimes(1);
    expect(result).toMatchObject({
      success: false,
      error: 'Generation was stopped.',
    });
  });

  it('surfaces grep fallback execution errors instead of reporting no matches', async () => {
    sessionTreeState.nodes = [{ id: 'node-1' }];
    nodeIdeExecCommandMock.mockResolvedValue({ stdout: '', stderr: 'Permission denied', exitCode: 2 });

    const result = await executeTool(
      'grep_search',
      { pattern: 'secret', path: '/root', node_id: 'node-1' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result).toMatchObject({
      success: false,
      error: 'Permission denied',
    });
  });

  it('rejects unsupported keepalive_interval_secs in set_pool_config', async () => {
    const result = await executeTool(
      'set_pool_config',
      { keepalive_interval_secs: 30 },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result).toMatchObject({
      success: false,
      error: 'keepalive_interval_secs is not supported by the current connection pool backend.',
    });
    expect(sshSetPoolConfigMock).not.toHaveBeenCalled();
  });

  it('uses the exact connectToSaved result when reporting connect_saved_session metadata', async () => {
    connectToSavedMock.mockResolvedValue({ nodeId: 'node-2', sessionId: 'term-2' });
    sessionTreeState.getNode.mockReturnValue({
      id: 'node-2',
      host: 'example.com',
      port: 22,
      username: 'alice',
      runtime: { status: 'active' },
    });

    const result = await executeTool(
      'connect_saved_session',
      { connection_id: 'saved-1' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toContain('"session_id": "term-2"');
    expect(result.output).toContain('"node_id": "node-2"');
    expect(result.envelope?.nextActions?.some((action) => action.tool === 'terminal_exec')).toBe(true);
  });

  it('connects a unique saved connection match through the workflow tool', async () => {
    searchConnectionsMock.mockResolvedValue([
      { id: 'saved-1', host: 'example.com', port: 22, username: 'alice', name: 'Prod', group: 'Servers' },
    ]);
    connectToSavedMock.mockResolvedValue({ nodeId: 'node-2', sessionId: 'term-2' });
    sessionTreeState.getNode.mockReturnValue({
      id: 'node-2',
      host: 'example.com',
      port: 22,
      username: 'alice',
      runtime: { status: 'active' },
    });

    const result = await executeTool(
      'connect_saved_connection_by_query',
      { query: 'prod' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(searchConnectionsMock).toHaveBeenCalledWith('prod');
    expect(connectToSavedMock).toHaveBeenCalledWith('saved-1', expect.any(Object));
    expect(result.toolName).toBe('connect_saved_connection_by_query');
    expect(result.output).toContain('Matched saved connection');
  });

  it('returns disambiguation when saved connection workflow has multiple matches', async () => {
    connectToSavedMock.mockClear();
    searchConnectionsMock.mockResolvedValue([
      { id: 'saved-1', host: 'one.example.com', port: 22, username: 'alice', name: 'Prod One', group: 'Servers' },
      { id: 'saved-2', host: 'two.example.com', port: 22, username: 'bob', name: 'Prod Two', group: 'Servers' },
    ]);

    const result = await executeTool(
      'connect_saved_connection_by_query',
      { query: 'prod' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(connectToSavedMock).not.toHaveBeenCalled();
    expect(result.envelope?.disambiguation?.options).toHaveLength(2);
    expect(result.envelope?.nextActions?.[0]?.tool).toBe('connect_saved_connection_by_query');
  });

  it('opens settings section with next-action guidance', async () => {
    const result = await executeTool(
      'open_settings_section',
      { section: 'sftp' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(createTabMock).toHaveBeenCalledWith('settings', undefined, undefined);
    expect(result.output).toContain('directoryParallelism');
    expect(result.envelope?.nextActions?.map((action) => action.tool)).toContain('get_settings');
    expect(result.envelope?.nextActions?.map((action) => action.tool)).toContain('update_setting');
  });

  it('executes dynamic MCP tools without requiring an SSH node context', async () => {
    mcpRegistryState.findServerForTool.mockReturnValue({
      server: { config: { id: 'mcp-1', name: 'filesystem' } },
      originalName: 'read_file',
    });
    mcpRegistryState.callTool.mockResolvedValue({
      isError: false,
      content: [{ type: 'text', text: 'hello from mcp' }],
    });

    const result = await executeTool(
      'mcp::filesystem::read_file',
      { path: '/tmp/example.txt' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    expect(result.output).toBe('hello from mcp');
    expect(mcpRegistryState.callTool).toHaveBeenCalledWith('mcp-1', 'read_file', { path: '/tmp/example.txt' });
  });

  it('reports ide_create_file as partial success when post-create content setup fails', async () => {
    ideStoreState.nodeId = 'node-ide';
    ideStoreState.createFile.mockResolvedValue(undefined);
    ideStoreState.openFile.mockRejectedValue(new Error('open failed'));

    const result = await executeTool(
      'ide_create_file',
      { path: '/tmp/new-file.txt', content: 'hello' },
      { activeNodeId: null, activeAgentAvailable: false },
    );

    expect(result.success).toBe(true);
    const parsed = JSON.parse(result.output);
    expect(parsed.path).toBe('/tmp/new-file.txt');
    expect(parsed.warning).toContain('File was created');
  });
});
