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
          env: {
            API_TOKEN: 'super-secret',
            DEBUG: '1',
          },
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
const hasDeniedCommandsMock = vi.hoisted(() => vi.fn(() => false));

vi.mock('@/lib/api', () => ({
  api: {
    localExecCommand: localExecCommandMock,
  },
  ragSearch: vi.fn(),
  nodeIdeExecCommand: vi.fn(),
  nodeGetState: vi.fn(),
  nodeAgentStatus: vi.fn(),
  nodeAgentReadFile: vi.fn(),
  nodeAgentWriteFile: vi.fn(),
  nodeAgentListTree: vi.fn(),
  nodeAgentGrep: vi.fn(),
  nodeAgentGitStatus: vi.fn(),
  nodeSftpListDir: vi.fn(),
  nodeSftpPreview: vi.fn(),
  nodeSftpStat: vi.fn(),
  nodeSftpWrite: vi.fn(),
}));

vi.mock('@/lib/ai/tools/toolDefinitions', async () => {
  const actual = await vi.importActual<typeof import('@/lib/ai/tools/toolDefinitions')>('@/lib/ai/tools/toolDefinitions');
  return {
    ...actual,
    hasDeniedCommands: hasDeniedCommandsMock,
  };
});

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: {
    getState: () => settingsState,
  },
}));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: {
    getState: () => ({ nodes: [] }),
  },
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: {
    getState: () => ({ sessions: new Map(), tabs: [], createTab: vi.fn() }),
  },
}));

vi.mock('@/store/localTerminalStore', () => ({
  useLocalTerminalStore: {
    getState: () => ({ terminals: new Map(), createTerminal: vi.fn() }),
  },
}));

vi.mock('@/store/ideStore', () => ({
  useIdeStore: {
    getState: () => ({ openFiles: [], activeFileId: null, nodeId: null }),
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
  findPaneBySessionId: vi.fn(),
  getTerminalBuffer: vi.fn(),
  writeToTerminal: vi.fn(),
  subscribeTerminalOutput: vi.fn(),
  readScreen: vi.fn(),
}));

vi.mock('@/lib/ai/providerRegistry', () => ({
  getProvider: vi.fn(),
}));

vi.mock('@/lib/ai/tools/outputCompressor', () => ({
  compressOutput: (value: string) => value,
}));

vi.mock('@/lib/ai/contextSanitizer', () => ({
  sanitizeConnectionInfo: (value: unknown) => value,
}));

import { executeTool } from '@/lib/ai/tools/toolExecutor';

describe('toolExecutor get_settings sanitization', () => {
  beforeEach(() => {
    localExecCommandMock.mockReset();
    hasDeniedCommandsMock.mockReset();
    hasDeniedCommandsMock.mockReturnValue(false);
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
        args: ['-y', '@modelcontextprotocol/server-filesystem', '--api-key=[redacted]', '--token', '[redacted]'],
        env: {
          API_TOKEN: '[redacted]',
          DEBUG: '[redacted]',
        },
        enabled: true,
        retryOnDisconnect: false,
        hasLegacyAuthToken: true,
      },
    ]);
    expect(parsed.mcpServers[0]).not.toHaveProperty('authToken');
  });

  it('preserves explicit empty MCP env objects while still redacting values', async () => {
    settingsState.settings.ai.mcpServers[0].env = {};

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
    hasDeniedCommandsMock.mockReturnValue(true);
    localExecCommandMock.mockResolvedValue({ stdout: 'ok', stderr: '', exitCode: 0, timedOut: false });

    const result = await executeTool('local_exec', { command: 'sudo reboot', timeout_secs: 5 }, { activeNodeId: null, activeAgentAvailable: false });

    expect(result.success).toBe(true);
    expect(localExecCommandMock).toHaveBeenCalledWith('sudo reboot', undefined, 5, true);
  });
});