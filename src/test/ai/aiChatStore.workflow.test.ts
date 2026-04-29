import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AiToolDefinition } from '@/lib/ai/providers';

const invokeMock = vi.hoisted(() => vi.fn());
const parseUserInputMock = vi.hoisted(() => vi.fn(() => ({
  slashCommand: null as { name: string; raw: string } | null,
  participants: [] as { name: string; raw: string }[],
  references: [] as { type: string; value?: string; raw: string }[],
  cleanText: '',
})));
const resolveSlashCommandMock = vi.hoisted(() => vi.fn());
const slashCommandsMock = vi.hoisted(() => [] as Array<{ name: string; descriptionKey: string; clientOnly?: boolean }>);
const resolveParticipantMock = vi.hoisted(() => vi.fn());
const getProviderMock = vi.hoisted(() => vi.fn());
const contextFreeToolsMock = vi.hoisted(() => new Set(['local_exec']));
const sessionIdToolsMock = vi.hoisted(() => new Set<string>());
const getToolsForContextMock = vi.hoisted(() => vi.fn<() => AiToolDefinition[]>(() => []));
const classifyToolObligationMock = vi.hoisted(() => vi.fn(() => ({
  mode: 'none',
  reason: 'test-default',
  intents: [] as string[],
  candidateTools: [] as string[],
})));
const executeToolMock = vi.hoisted(() => vi.fn());
const hasDeniedCommandsMock = vi.hoisted(() => vi.fn((_toolName?: string, _args?: Record<string, unknown>) => false));
const estimateTokensMock = vi.hoisted(() => vi.fn(() => 100));
const getModelContextWindowMock = vi.hoisted(() => vi.fn(() => 1000));
const responseReserveMock = vi.hoisted(() => vi.fn(() => 256));
const trimHistoryMock = vi.hoisted(() => vi.fn((messages) => ({ messages, trimmedCount: 0 })));
const providerStreamMock = vi.hoisted(() => vi.fn());
const gatherSidebarContextMock = vi.hoisted(() => vi.fn<() => unknown>(() => null));
const buildContextReminderMock = vi.hoisted(() => vi.fn<(ctx: unknown) => string | null>(() => null));
const resolveReferenceTypeMock = vi.hoisted(() => vi.fn());
const resolveAllReferencesMock = vi.hoisted(() => vi.fn<(...args: unknown[]) => unknown>(() => []));
const appStoreState = vi.hoisted(() => ({ tabs: [] as Array<{ id: string; type?: string }>, activeTabId: null as string | null, sessions: new Map() }));
const apiMocks = vi.hoisted(() => ({
  getAiProviderApiKey: vi.fn().mockResolvedValue('key-1'),
  getConnections: vi.fn().mockResolvedValue([]),
  localExecCommand: vi.fn().mockResolvedValue({ stdout: 'ok', stderr: '', exitCode: 0, timedOut: false }),
  ragSearch: vi.fn().mockResolvedValue([]),
  nodeAgentStatus: vi.fn().mockResolvedValue({ type: 'ready' }),
  nodeGetState: vi.fn().mockResolvedValue({ state: { readiness: 'ready' } }),
}));
const settingsStoreMock = vi.hoisted(() => ({
  state: {
    settings: {
      ai: {
        enabled: true,
        enabledConfirmed: true,
        baseUrl: 'https://api.example.com/v1',
        model: 'default-model',
        providers: [
          {
            id: 'provider-1',
            type: 'openai_compatible',
            name: 'Mock Provider',
            baseUrl: 'https://api.example.com/v1',
            defaultModel: 'mock-model',
            models: ['mock-model'],
          },
        ],
        activeProviderId: 'provider-1',
        activeModel: 'mock-model',
        contextVisibleLines: 50,
        contextMaxChars: 8000,
        modelContextWindows: { 'provider-1': { 'mock-model': 1000 } },
        modelMaxResponseTokens: {},
        memory: {
          enabled: true,
          content: '',
        },
        toolUse: {
          enabled: false,
          disabledTools: [],
          autoApproveTools: {},
          maxRounds: 10,
        },
      },
    },
  },
  store: {
    getState: () => settingsStoreMock.state,
  },
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

vi.mock('@/lib/api', () => ({
  api: {
    getAiProviderApiKey: apiMocks.getAiProviderApiKey,
    getConnections: apiMocks.getConnections,
    localExecCommand: apiMocks.localExecCommand,
  },
  ragSearch: apiMocks.ragSearch,
  nodeAgentStatus: apiMocks.nodeAgentStatus,
  nodeGetState: apiMocks.nodeGetState,
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: settingsStoreMock.store,
}));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: {
    getState: () => ({
      nodes: [],
      getNodeByTerminalId: vi.fn(),
      getNode: vi.fn(),
    }),
  },
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: {
    getState: () => appStoreState,
  },
}));

vi.mock('@/lib/sidebarContextProvider', () => ({
  gatherSidebarContext: gatherSidebarContextMock,
  buildContextReminder: buildContextReminderMock,
}));

vi.mock('@/lib/ai/providerRegistry', () => ({
  getProvider: getProviderMock,
  getProviderReasoningProtocol: () => 'none',
}));

vi.mock('@/lib/ai/tokenUtils', () => ({
  estimateTokens: estimateTokensMock,
  estimateToolDefinitionsTokens: vi.fn(() => 0),
  trimHistoryToTokenBudget: trimHistoryMock,
  getModelContextWindow: getModelContextWindowMock,
  responseReserve: responseReserveMock,
}));

vi.mock('@/lib/ai/constants', () => ({
  DEFAULT_SYSTEM_PROMPT: 'system',
  SUGGESTIONS_INSTRUCTION: 'suggestions',
  COMPACTION_TRIGGER_THRESHOLD: 0.9,
}));

vi.mock('@/lib/ai/tools', () => ({
  CONTEXT_FREE_TOOLS: contextFreeToolsMock,
  SESSION_ID_TOOLS: sessionIdToolsMock,
  READ_ONLY_TOOLS: new Set(['read_file', 'list_directory', 'grep_search']),
  getToolsForContext: getToolsForContextMock,
  getToolsForPlan: getToolsForContextMock,
  inferToolIntents: vi.fn(() => []),
  classifyToolObligation: classifyToolObligationMock,
  formatToolResultForModel: vi.fn((result) => JSON.stringify({
    ok: result.envelope?.ok ?? result.success,
    summary: result.envelope?.summary ?? result.error ?? result.output,
    output: result.envelope?.output ?? result.output,
    error: result.envelope?.error ?? (result.error ? { message: result.error, recoverable: true } : undefined),
    nextActions: result.envelope?.nextActions,
  })),
  isCommandDenied: vi.fn(() => false),
  hasDeniedCommands: hasDeniedCommandsMock,
  decideToolApproval: ({ toolName, args, autoApproveTools, readOnlyTools }: {
    toolName: string;
    args?: Record<string, unknown>;
    autoApproveTools?: Record<string, boolean>;
    readOnlyTools?: Set<string>;
  }) => {
    if (hasDeniedCommandsMock(toolName, args ?? {})) {
      return { risk: 'destructive', autoApprove: false, requiresApproval: true, reason: 'high-risk' };
    }
    if (autoApproveTools?.[toolName] === true || readOnlyTools?.has(toolName)) {
      return { risk: readOnlyTools?.has(toolName) ? 'read' : 'execute-command', autoApprove: true, requiresApproval: false, reason: 'tool-auto-approved' };
    }
    return { risk: 'execute-command', autoApprove: false, requiresApproval: true, reason: 'manual' };
  },
  executeTool: executeToolMock,
}));

vi.mock('@/lib/ai/inputParser', () => ({
  parseUserInput: parseUserInputMock,
}));

vi.mock('@/lib/ai/slashCommands', () => ({
  resolveSlashCommand: resolveSlashCommandMock,
  SLASH_COMMANDS: slashCommandsMock,
}));

vi.mock('@/lib/ai/participants', () => ({
  PARTICIPANTS: [],
  resolveParticipant: resolveParticipantMock,
}));

vi.mock('@/lib/ai/references', () => ({
  REFERENCES: [],
  resolveReferenceType: resolveReferenceTypeMock,
  resolveAllReferences: resolveAllReferencesMock,
}));

vi.mock('@/lib/ai/suggestionParser', () => ({
  parseSuggestions: vi.fn((content: string) => ({ cleanContent: content, suggestions: [] })),
}));

vi.mock('@/lib/ai/intentDetector', () => ({
  detectIntent: vi.fn(() => ({ confidence: 0, systemHint: null })),
}));

vi.mock('@/lib/ai/contextSanitizer', () => ({
  sanitizeForAi: vi.fn((value: unknown) => value),
  sanitizeApiMessages: vi.fn((value: unknown) => value),
}));

vi.mock('@/i18n', () => ({
  default: {
    t: (key: string) => key,
  },
}));

import { useAiChatStore } from '@/store/aiChatStore';
import { resetAiChatStoreRuntimeState } from '@/store/aiChatStore.runtime';
import type { SidebarContext } from '@/lib/sidebarContextProvider';
import type { AiConversation, AiChatMessage } from '@/types';

const initialAiChatStoreState = useAiChatStore.getInitialState();

function makeConversation(messages: AiChatMessage[] = [], id = 'conv-1'): AiConversation {
  return {
    id,
    title: 'Conversation',
    createdAt: 1,
    updatedAt: 1,
    messages,
    origin: 'sidebar',
  };
}

function setConversation(messages: AiChatMessage[]) {
  useAiChatStore.setState({
    conversations: [makeConversation(messages)],
    activeConversationId: 'conv-1',
    activeGenerationId: null,
    isLoading: false,
    isInitialized: true,
    error: null,
    abortController: null,
    compactionInfo: null,
  });
}

function streamText(content: string) {
  providerStreamMock.mockImplementation(async function* () {
    yield { type: 'content', content };
    yield { type: 'done' };
  });
}

function streamEvents(events: Array<Record<string, unknown>>) {
  providerStreamMock.mockImplementation(async function* () {
    for (const event of events) {
      yield event;
    }
    yield { type: 'done' };
  });
}

async function waitFor(predicate: () => boolean, timeoutMs = 1000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() <= deadline) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  throw new Error('Condition not met in time');
}

describe('aiChatStore workflows', () => {
  beforeEach(() => {
    resetAiChatStoreRuntimeState();
    vi.clearAllMocks();
    invokeMock.mockReset();
    providerStreamMock.mockReset();
    settingsStoreMock.state.settings.ai.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.enabled = false;
    settingsStoreMock.state.settings.ai.toolUse.disabledTools = [];
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = {};
    settingsStoreMock.state.settings.ai.toolUse.maxRounds = 10;
    settingsStoreMock.state.settings.ai.memory = { enabled: true, content: '' };
    parseUserInputMock.mockReturnValue({ slashCommand: null, participants: [], references: [], cleanText: '' });
    resolveSlashCommandMock.mockReturnValue(undefined);
    slashCommandsMock.splice(0, slashCommandsMock.length);
    resolveParticipantMock.mockReturnValue(undefined);
    getProviderMock.mockReturnValue({ streamCompletion: providerStreamMock });
    estimateTokensMock.mockImplementation(() => 100);
    trimHistoryMock.mockImplementation((messages) => ({ messages, trimmedCount: 0 }));
    gatherSidebarContextMock.mockReturnValue(null);
    buildContextReminderMock.mockReturnValue(null);
    resolveReferenceTypeMock.mockReturnValue(undefined);
    resolveAllReferencesMock.mockResolvedValue([]);
    appStoreState.tabs = [];
    appStoreState.activeTabId = null;
    appStoreState.sessions = new Map();
    getToolsForContextMock.mockReset();
    getToolsForContextMock.mockReturnValue([]);
    classifyToolObligationMock.mockReset();
    classifyToolObligationMock.mockReturnValue({
      mode: 'none',
      reason: 'test-default',
      intents: [],
      candidateTools: [],
    });
    executeToolMock.mockReset();
    apiMocks.getConnections.mockReset();
    apiMocks.getConnections.mockResolvedValue([]);
    apiMocks.localExecCommand.mockReset();
    apiMocks.localExecCommand.mockResolvedValue({ stdout: 'ok', stderr: '', exitCode: 0, timedOut: false });
    hasDeniedCommandsMock.mockReset();
    hasDeniedCommandsMock.mockReturnValue(false);
    streamText('summary text');
    useAiChatStore.setState({
      ...initialAiChatStoreState,
      conversations: [],
      activeConversationId: null,
      activeGenerationId: null,
      isLoading: false,
      isInitialized: true,
      error: null,
      abortController: null,
      trimInfo: null,
      compactionInfo: null,
    });
  });

  it('handles client-only /clear by creating a fresh conversation without streaming', async () => {
    const createConversation = vi.fn().mockResolvedValue('conv-new');
    setConversation([{ id: 'u-1', role: 'user', content: 'hello', timestamp: 1 }]);
    parseUserInputMock.mockReturnValue({
      slashCommand: { name: 'clear', raw: '/clear' },
      participants: [],
      references: [],
      cleanText: '',
    });
    resolveSlashCommandMock.mockReturnValue({ name: 'clear', clientOnly: true });
    useAiChatStore.setState({ createConversation: createConversation as never });

    await useAiChatStore.getState().sendMessage('/clear');

    expect(createConversation).toHaveBeenCalledTimes(1);
    expect(providerStreamMock).not.toHaveBeenCalled();
  });

  it('/help lists only the active slash commands', async () => {
    setConversation([]);
    parseUserInputMock.mockReturnValue({
      slashCommand: { name: 'help', raw: '/help' },
      participants: [],
      references: [],
      cleanText: '',
    });
    resolveSlashCommandMock.mockReturnValue({ name: 'help', clientOnly: true });
    slashCommandsMock.push(
      { name: 'explain', descriptionKey: 'ai.slash.explain_desc' },
      { name: 'fix', descriptionKey: 'ai.slash.fix_desc' },
      { name: 'help', descriptionKey: 'ai.slash.help_desc', clientOnly: true },
      { name: 'clear', descriptionKey: 'ai.slash.clear_desc', clientOnly: true },
      { name: 'compact', descriptionKey: 'ai.slash.compact_desc', clientOnly: true },
    );

    await useAiChatStore.getState().sendMessage('/help');

    const assistantMessage = useAiChatStore.getState().conversations[0]?.messages.find(m => m.role === 'assistant');
    expect(assistantMessage?.content).toContain('### /help');
    for (const command of ['explain', 'fix', 'help', 'clear', 'compact']) {
      expect(assistantMessage?.content).toContain(`/${command}`);
    }
    for (const removed of ['tools', 'deploy', 'monitor', 'connect', 'search', 'script', 'optimize']) {
      expect(assistantMessage?.content).not.toContain(`/${removed}`);
    }
    expect(providerStreamMock).not.toHaveBeenCalled();
  });

  it('adds participant intent and target-view hints to the system prompt', async () => {
    setConversation([]);
    parseUserInputMock.mockReturnValue({
      slashCommand: null,
      participants: [{ name: 'knowledge', raw: '@knowledge' }],
      references: [],
      cleanText: '查询插件开发文档',
    });
    resolveParticipantMock.mockReturnValue({
      name: 'knowledge',
      systemPromptModifier: 'Use the knowledge base.',
      intentHint: 'knowledge',
      preferredTargetView: 'files',
    });
    streamText('assistant reply');

    await useAiChatStore.getState().sendMessage('@knowledge 查询插件开发文档');

    const [, providerMessages] = providerStreamMock.mock.calls[0];
    expect(providerMessages).toEqual(expect.arrayContaining([
      expect.objectContaining({
        role: 'system',
        content: expect.stringContaining('@knowledge: Use the knowledge base. (intent=knowledge, preferred_target_view=files)'),
      }),
    ]));
  });

  it('keeps the newer run state when an older aborted run finishes later', async () => {
    setConversation([]);

    providerStreamMock
      .mockImplementationOnce(async function* (_config, _messages, signal: AbortSignal) {
        await new Promise<void>((resolve) => {
          if (signal.aborted) {
            resolve();
            return;
          }
          signal.addEventListener('abort', () => resolve(), { once: true });
        });
        const error = new Error('Aborted');
        error.name = 'AbortError';
        throw error;
      })
      .mockImplementationOnce(async function* (_config, _messages, signal: AbortSignal) {
        yield { type: 'content', content: 'second run' };
        await new Promise<void>((resolve) => {
          if (signal.aborted) {
            resolve();
            return;
          }
          signal.addEventListener('abort', () => resolve(), { once: true });
        });
      });

    const firstRun = useAiChatStore.getState().sendMessage('first run');
    await waitFor(() => useAiChatStore.getState().isLoading);

    useAiChatStore.getState().stopGeneration();

    const secondRun = useAiChatStore.getState().sendMessage('second run');
    await waitFor(() => {
      const conversation = useAiChatStore.getState().conversations[0];
      return conversation?.messages.some((message) => message.role === 'assistant' && message.content === 'second run');
    });

    await firstRun;

    const stateAfterFirstRun = useAiChatStore.getState();
    expect(stateAfterFirstRun.isLoading).toBe(true);
    expect(stateAfterFirstRun.abortController).not.toBeNull();
    expect(stateAfterFirstRun.activeGenerationId).not.toBeNull();

    useAiChatStore.getState().stopGeneration();
    await secondRun;
  });

  it('keeps the newer assistant snapshot after same-message save callbacks resolve out of order', async () => {
    setConversation([]);

    const persistedMessages = new Map<string, Record<string, unknown>>();
    let backendUpdatedAt = 1;
    const pendingSaves: Array<{
      request: Record<string, unknown>;
      resolve: () => void;
    }> = [];

    const applySave = (request: Record<string, unknown>) => {
      const messageId = request.id as string;
      const incomingProjectionUpdatedAt = (request.projectionUpdatedAt as number | undefined) ?? (request.timestamp as number);
      const existing = persistedMessages.get(messageId);
      const existingProjectionUpdatedAt = existing
        ? ((existing.projectionUpdatedAt as number | undefined) ?? (existing.timestamp as number))
        : Number.NEGATIVE_INFINITY;

      if (incomingProjectionUpdatedAt < existingProjectionUpdatedAt) {
        backendUpdatedAt = Math.max(backendUpdatedAt, incomingProjectionUpdatedAt);
        return;
      }

      persistedMessages.set(messageId, {
        id: request.id,
        role: request.role,
        content: request.content,
        timestamp: request.timestamp,
        toolCalls: request.toolCalls,
        context: null,
        turn: request.turn,
        transcriptRef: request.transcriptRef,
        summaryRef: request.summaryRef,
        projectionUpdatedAt: incomingProjectionUpdatedAt,
      });
      backendUpdatedAt = Math.max(backendUpdatedAt, incomingProjectionUpdatedAt);
    };

    invokeMock.mockImplementation((command: string, payload?: { request?: Record<string, unknown>; conversationId?: string }) => {
      switch (command) {
        case 'ai_chat_save_message': {
          const request = payload?.request;
          if (!request) {
            return Promise.resolve(undefined);
          }

          return new Promise<void>((resolve) => {
            pendingSaves.push({
              request,
              resolve: () => {
                applySave(request);
                resolve();
              },
            });
          });
        }
        case 'ai_chat_get_conversation':
          return Promise.resolve({
            id: payload?.conversationId ?? 'conv-1',
            title: 'Conversation',
            createdAt: 1,
            updatedAt: backendUpdatedAt,
            sessionId: null,
            origin: 'sidebar',
            messages: Array.from(persistedMessages.values())
              .sort((left, right) => (left.timestamp as number) - (right.timestamp as number))
              .map(({ projectionUpdatedAt: _projectionUpdatedAt, ...message }) => message),
          });
        case 'ai_chat_get_transcript':
          return Promise.resolve({ entries: [] });
        default:
          return Promise.resolve(undefined);
      }
    });

    const olderMessage: AiChatMessage = {
      id: 'assistant-shared',
      role: 'assistant',
      content: 'older snapshot',
      timestamp: 10,
      turn: {
        id: 'assistant-shared',
        status: 'complete',
        plainTextSummary: 'older snapshot',
        parts: [{ type: 'text', text: 'older snapshot' }],
        toolRounds: [],
      },
    };
    const newerMessage: AiChatMessage = {
      id: 'assistant-shared',
      role: 'assistant',
      content: 'newer snapshot',
      timestamp: 10,
      turn: {
        id: 'assistant-shared',
        status: 'complete',
        plainTextSummary: 'newer snapshot',
        parts: [{ type: 'text', text: 'newer snapshot' }],
        toolRounds: [],
      },
    };

    const olderSavePromise = useAiChatStore.getState()._addMessage('conv-1', olderMessage);
    const newerSavePromise = useAiChatStore.getState()._addMessage('conv-1', newerMessage);

    await waitFor(() => pendingSaves.length === 2);

    const [olderSave, newerSave] = pendingSaves;
    expect(newerSave.request.projectionUpdatedAt as number).toBeGreaterThan(olderSave.request.projectionUpdatedAt as number);

    newerSave.resolve();
    await newerSavePromise;
    olderSave.resolve();
    await olderSavePromise;

    await useAiChatStore.getState()._loadConversation('conv-1');

    const reloadedConversation = useAiChatStore.getState().conversations[0];
    const reloadedMessages = reloadedConversation.messages.filter((message) => message.id === 'assistant-shared');

    expect(reloadedMessages).toHaveLength(1);
    expect(reloadedMessages[0]).toMatchObject({
      content: 'newer snapshot',
      turn: expect.objectContaining({
        plainTextSummary: 'newer snapshot',
      }),
    });
  });

  it('updates approval state on the originating conversation even after switching activeConversationId', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    setConversation([]);
    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'tool_call_complete', id: 'tool-1', name: 'run_command', arguments: JSON.stringify({ target_id: 'local-shell:default', command: 'sudo reboot' }) };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: 'done' };
        yield { type: 'done' };
      });

    const sendPromise = useAiChatStore.getState().sendMessage('needs approval');

    await waitFor(() => {
      const conversation = useAiChatStore.getState().conversations.find((item) => item.id === 'conv-1');
      const assistant = conversation?.messages.find((message) => message.role === 'assistant');
      return assistant?.turn?.toolRounds[0]?.toolCalls[0]?.approvalState === 'pending';
    });

    const currentState = useAiChatStore.getState();
    useAiChatStore.setState({
      conversations: [
        ...currentState.conversations,
        makeConversation([], 'conv-2'),
      ],
      activeConversationId: 'conv-2',
    });

    useAiChatStore.getState().resolveToolApproval('tool-1', true);
    await sendPromise;

    const originalConversation = useAiChatStore.getState().conversations.find((item) => item.id === 'conv-1');
    const originalAssistant = originalConversation?.messages.find((message) => message.role === 'assistant');
    expect(originalAssistant?.toolCalls?.[0]).toMatchObject({
      id: 'tool-1',
      status: 'completed',
    });
    expect(originalAssistant?.turn?.toolRounds[0]?.toolCalls[0]).toMatchObject({
      id: 'tool-1',
      executionState: 'completed',
    });
    expect(useAiChatStore.getState().activeConversationId).toBe('conv-2');
  });

  it('keeps destructive commands pending in default mode even when run_command is auto-approved', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = { run_command: true };
    setConversation([]);
    providerStreamMock.mockImplementationOnce(async function* () {
      yield { type: 'tool_call_complete', id: 'tool-danger-default', name: 'run_command', arguments: JSON.stringify({ target_id: 'local-shell:default', command: 'sudo reboot' }) };
      yield { type: 'done' };
    });

    const sendPromise = useAiChatStore.getState().sendMessage('dangerous command');

    await waitFor(() => {
      const assistant = useAiChatStore.getState().conversations[0]?.messages.find((message) => message.role === 'assistant');
      return assistant?.turn?.toolRounds[0]?.toolCalls[0]?.approvalState === 'pending';
    });

    useAiChatStore.getState().stopGeneration();
    await sendPromise;
    expect(apiMocks.localExecCommand).not.toHaveBeenCalled();
  });

  it('bypasses only destructive command approval for the current conversation when enabled', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = {};
    setConversation([]);
    useAiChatStore.getState().setConversationSafetyMode('conv-1', 'bypass');

    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'tool_call_complete', id: 'tool-danger-bypass', name: 'run_command', arguments: JSON.stringify({ target_id: 'local-shell:default', command: 'sudo reboot' }) };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: 'done' };
        yield { type: 'done' };
      });

    await useAiChatStore.getState().sendMessage('dangerous command');

    expect(apiMocks.localExecCommand).toHaveBeenCalledWith('sudo reboot', undefined, 30, true);
    const assistant = useAiChatStore.getState().conversations[0]?.messages.find((message) => message.role === 'assistant');
    expect(assistant?.turn?.toolRounds[0]?.toolCalls[0]).toMatchObject({
      id: 'tool-danger-bypass',
      approvalState: 'approved',
      executionState: 'completed',
    });
    const toolResultPart = assistant?.turn?.parts.find((part) => part.type === 'tool_result' && part.toolCallId === 'tool-danger-bypass');
    expect(toolResultPart?.type === 'tool_result' ? toolResultPart.envelope?.meta.approvalMode : undefined).toBe('bypass');
  });

  it('auto-approves settings writes independently from file writes', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = { 'write_resource:settings': true };
    setConversation([]);

    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield {
          type: 'tool_call_complete',
          id: 'tool-settings-write',
          name: 'write_resource',
          arguments: JSON.stringify({
            target_id: 'settings:app',
            resource: 'settings',
            section: 'ai',
            key: 'thinkingDefaultExpanded',
            value: true,
            dry_run: true,
          }),
        };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: 'done' };
        yield { type: 'done' };
      });

    await useAiChatStore.getState().sendMessage('dry run settings write');

    const assistant = useAiChatStore.getState().conversations[0]?.messages.find((message) => message.role === 'assistant');
    expect(assistant?.turn?.toolRounds[0]?.toolCalls[0]).toMatchObject({
      id: 'tool-settings-write',
      approvalState: 'approved',
      executionState: 'completed',
    });
  });

  it('does not use settings-write approval for file writes', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = { 'write_resource:settings': true };
    setConversation([]);

    providerStreamMock.mockImplementationOnce(async function* () {
      yield {
        type: 'tool_call_complete',
        id: 'tool-file-write',
        name: 'write_resource',
        arguments: JSON.stringify({
          target_id: 'ssh-node:node-1',
          resource: 'file',
          path: '/tmp/demo.txt',
          content: 'demo',
        }),
      };
      yield { type: 'done' };
    });

    const sendPromise = useAiChatStore.getState().sendMessage('write a file');

    await waitFor(() => {
      const assistant = useAiChatStore.getState().conversations[0]?.messages.find((message) => message.role === 'assistant');
      return assistant?.turn?.toolRounds[0]?.toolCalls[0]?.approvalState === 'pending';
    });

    useAiChatStore.getState().stopGeneration();
    await sendPromise;
  });

  it('stores bypass mode only for the selected conversation', () => {
    setConversation([]);
    useAiChatStore.setState((state) => ({
      conversations: [
        ...state.conversations,
        makeConversation([], 'conv-2'),
      ],
    }));

    useAiChatStore.getState().setConversationSafetyMode('conv-1', 'bypass');

    expect(useAiChatStore.getState().getConversationSafetyMode('conv-1')).toBe('bypass');
    expect(useAiChatStore.getState().getConversationSafetyMode('conv-2')).toBe('default');
  });

  it('keeps the assistant message visible when the provider errors after a tool round', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = { run_command: true };
    setConversation([]);

    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'thinking', content: 'Let me inspect that first.' };
        yield { type: 'tool_call_complete', id: 'tool-keep-1', name: 'run_command', arguments: JSON.stringify({ target_id: 'local-shell:default', command: 'uname -a' }) };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'error', message: 'The engine is currently overloaded, please try again later' };
      });

    await useAiChatStore.getState().sendMessage('inspect the system');

    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
    expect(assistantMessage).toBeDefined();
    expect(assistantMessage?.turn?.toolRounds[0]?.toolCalls[0]).toMatchObject({
      id: 'tool-keep-1',
      executionState: 'completed',
    });
    expect(assistantMessage?.turn?.parts).toEqual(expect.arrayContaining([
      expect.objectContaining({ type: 'thinking', text: 'Let me inspect that first.' }),
      expect.objectContaining({ type: 'error', message: 'The engine is currently overloaded, please try again later' }),
    ]));
    expect(assistantMessage?.turn?.toolRounds[0]?.statefulMarker).toBeUndefined();
    expect(useAiChatStore.getState().error).toBe('The engine is currently overloaded, please try again later');
  });

  it('uses the configured tool-round limit before continuing tool loops', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = { run_command: true };
    settingsStoreMock.state.settings.ai.toolUse.maxRounds = 1;
    setConversation([]);

    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'tool_call_complete', id: 'tool-round-1', name: 'run_command', arguments: JSON.stringify({ target_id: 'local-shell:default', command: 'pwd' }) };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'tool_call_complete', id: 'tool-round-2', name: 'run_command', arguments: JSON.stringify({ target_id: 'local-shell:default', command: 'whoami' }) };
        yield { type: 'done' };
      });

    await useAiChatStore.getState().sendMessage('inspect twice');

    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
    const budgetRound = assistantMessage?.turn?.toolRounds.find((toolRound) => toolRound.toolCalls.some((toolCall) => toolCall.id === 'tool-round-2'));
    expect(providerStreamMock).toHaveBeenCalledTimes(2);
    expect(apiMocks.localExecCommand).toHaveBeenCalledTimes(1);
    expect(budgetRound).toMatchObject({
      toolCalls: expect.arrayContaining([
        expect.objectContaining({
          approvalState: 'rejected',
          executionState: 'error',
          id: 'tool-round-2',
        }),
      ]),
    });
    expect(assistantMessage?.turn?.parts).toEqual(expect.arrayContaining([
      expect.objectContaining({ type: 'guardrail', code: 'tool-budget-limit' }),
    ]));
  });

  it('shows and clears a transient post-tool waiting marker before the follow-up summary arrives', async () => {
    let releaseSummary!: () => void;

    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = { run_command: true };
    setConversation([]);

    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'tool_call_complete', id: 'tool-wait-1', name: 'run_command', arguments: JSON.stringify({ target_id: 'local-shell:default', command: 'pwd' }) };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        await new Promise<void>((resolve) => {
          releaseSummary = resolve;
        });
        yield { type: 'content', content: 'final answer' };
        yield { type: 'done' };
      });

    const sendPromise = useAiChatStore.getState().sendMessage('check the current directory');

    await waitFor(() => {
      const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
      return assistantMessage?.turn?.toolRounds[0]?.statefulMarker === 'awaiting-summary' && typeof releaseSummary === 'function';
    });

    expect(useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant')?.turn?.toolRounds[0]).toMatchObject({
      statefulMarker: 'awaiting-summary',
      toolCalls: [expect.objectContaining({ id: 'tool-wait-1', executionState: 'completed' })],
    });

    releaseSummary();
    await sendPromise;

    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
    expect(assistantMessage?.content).toBe('final answer');
    expect(assistantMessage?.turn?.toolRounds[0]?.statefulMarker).toBeUndefined();
  });

  it('keeps inline post-tool thinking after the tool round instead of collapsing it to the top', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = { run_command: true };
    setConversation([]);

    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'tool_call_complete', id: 'tool-inline-think-1', name: 'run_command', arguments: JSON.stringify({ target_id: 'local-shell:default', command: 'pwd' }) };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: '<thinking>Summarize the tool result first.</thinking>final answer' };
        yield { type: 'done' };
      });

    await useAiChatStore.getState().sendMessage('check the current directory');

    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
    expect(assistantMessage?.turn?.parts.map((part) => part.type)).toEqual([
      'tool_call',
      'tool_result',
      'thinking',
      'text',
    ]);
    expect(assistantMessage?.turn?.parts[2]).toMatchObject({
      type: 'thinking',
      text: 'Summarize the tool result first.',
    });
    expect(assistantMessage?.turn?.parts[3]).toMatchObject({
      type: 'text',
      text: 'final answer',
    });
  });

  it('injects the disabled-tools negative constraint into the system prompt', async () => {
    setConversation([]);
    streamText('plain answer');

    await useAiChatStore.getState().sendMessage('what files are here?');

    const providerMessages = providerStreamMock.mock.calls[0]?.[1];
    expect(providerMessages?.[0]?.role).toBe('system');
    expect(providerMessages?.[0]?.content).toContain('TOOL CALLING IS CURRENTLY DISABLED');
  });

  it('injects saved user memory into the system prompt when enabled', async () => {
    setConversation([]);
    settingsStoreMock.state.settings.ai.memory = {
      enabled: true,
      content: '- Prefer concise Chinese replies.',
    };
    streamText('plain answer');

    await useAiChatStore.getState().sendMessage('hello');

    const providerMessages = providerStreamMock.mock.calls[0]?.[1];
    expect(providerMessages?.[0]?.role).toBe('system');
    expect(providerMessages?.[0]?.content).toContain('## User Memory');
    expect(providerMessages?.[0]?.content).toContain('Prefer concise Chinese replies');
  });

  it('injects operation-focused tool strategy when tool use is enabled', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    setConversation([]);
    streamText('plain answer');

    await useAiChatStore.getState().sendMessage('run a command');

    const providerMessages = providerStreamMock.mock.calls[0]?.[1];
    expect(providerMessages?.[0]?.role).toBe('system');
    expect(providerMessages?.[0]?.content).toContain('## OxideSens Tool System');
    expect(providerMessages?.[0]?.content).toContain('call `select_target` first');
    expect(providerMessages?.[0]?.content).toContain('`connect_target` first, then `run_command`');
    expect(providerMessages?.[0]?.content).toContain('Never open a local terminal and type `ssh user@host`');
    expect(providerMessages?.[0]?.content).toContain('Do not guess passwords, passphrases, sudo prompts, or repeat the command');
    expect(providerMessages?.[0]?.content).toContain('Every action that runs, writes, transfers, or sends input must use an explicit target_id');
  });

  it('marks required tool requests with toolChoice and retries once when no tool call is made', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    const settingsTool = { name: 'open_settings_section', description: 'Open settings', parameters: { type: 'object', properties: {} } };
    getToolsForContextMock.mockReturnValue([settingsTool]);
    classifyToolObligationMock.mockReturnValue({
      mode: 'required',
      reason: 'action-request-needs-tool-result',
      intents: ['settings'],
      candidateTools: ['open_settings_section'],
    });
    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: '已经打开设置。' };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: '我需要调用设置工具后才能确认。' };
        yield { type: 'done' };
      });
    setConversation([]);

    await useAiChatStore.getState().sendMessage('打开设置');

    expect(providerStreamMock).toHaveBeenCalledTimes(2);
    expect(providerStreamMock.mock.calls[0]?.[0]).toMatchObject({ toolChoice: 'required' });
    const retryMessages = providerStreamMock.mock.calls[1]?.[1];
    expect(retryMessages).toEqual(expect.arrayContaining([
      expect.objectContaining({
        role: 'user',
        content: expect.stringContaining('requires real app/tool state'),
      }),
    ]));
    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
    expect(assistantMessage?.content).toBe('我需要调用设置工具后才能确认。');
    expect(assistantMessage?.turn?.parts).toEqual(expect.arrayContaining([
      expect.objectContaining({ type: 'guardrail', code: 'tool-required-no-call' }),
    ]));
  });

  it('does not retry required discovery after list_targets already returned a result', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    apiMocks.getConnections.mockResolvedValue([
      {
        id: 'conn-1',
        name: '家里的主机',
        host: '100.118.61.75',
        port: 22,
        username: 'lipsc',
        group: 'Imported',
      },
    ]);
    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'tool_call_complete', id: 'tool-list-1', name: 'list_targets', arguments: JSON.stringify({ kind: 'all' }) };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: '当前有 1 个远程主机：家里的主机。' };
        yield { type: 'done' };
      });
    setConversation([]);

    await useAiChatStore.getState().sendMessage('现在有哪些远程主机可供连接');

    expect(providerStreamMock).toHaveBeenCalledTimes(2);
    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
    expect(assistantMessage?.content).toBe('当前有 1 个远程主机：家里的主机。');
    expect(assistantMessage?.turn?.parts).not.toEqual(expect.arrayContaining([
      expect.objectContaining({ type: 'guardrail', code: 'tool-required-no-call' }),
    ]));
    expect(assistantMessage?.turn?.toolRounds[0]?.toolCalls[0]).toMatchObject({
      name: 'list_targets',
      executionState: 'completed',
    });
  });

  it('drops pre-required-tool thinking content before retrying', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    const settingsTool = { name: 'open_settings_section', description: 'Open settings', parameters: { type: 'object', properties: {} } };
    getToolsForContextMock.mockReturnValue([settingsTool]);
    classifyToolObligationMock.mockReturnValue({
      mode: 'required',
      reason: 'action-request-needs-tool-result',
      intents: ['settings'],
      candidateTools: ['open_settings_section'],
    });
    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'thinking', content: 'I will pretend settings are already open.' };
        yield { type: 'content', content: '已经打开设置。' };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: '我需要调用设置工具后才能确认。' };
        yield { type: 'done' };
      });
    setConversation([]);

    await useAiChatStore.getState().sendMessage('打开设置');

    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
    expect(providerStreamMock).toHaveBeenCalledTimes(2);
    expect(assistantMessage?.thinkingContent).toBeUndefined();
    expect(assistantMessage?.turn?.parts.some((part) => part.type === 'thinking')).toBe(false);
    expect(assistantMessage?.content).toBe('我需要调用设置工具后才能确认。');
  });

  it('feeds compact structured tool envelopes back to the model', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = { run_command: true };
    apiMocks.localExecCommand.mockResolvedValue({ stdout: '/tmp/project', stderr: '', exitCode: 0, timedOut: false });
    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'tool_call_complete', id: 'tool-env-1', name: 'run_command', arguments: JSON.stringify({ target_id: 'local-shell:default', command: 'pwd' }) };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: 'done' };
        yield { type: 'done' };
      });
    setConversation([]);

    await useAiChatStore.getState().sendMessage('在本地执行 pwd');

    const secondRequestMessages = providerStreamMock.mock.calls[1]?.[1] ?? [];
    const toolMessage = secondRequestMessages.find((message: { role: string; tool_name?: string }) => message.role === 'tool' && message.tool_name === 'run_command');
    expect(toolMessage?.content).toContain('"ok":true');
    expect(toolMessage?.content).toContain('Local command completed');
    expect(toolMessage?.content).toContain('/tmp/project');
  });

  it('stores turn snapshots and session metadata for sidebar sends', async () => {
    setConversation([]);
    appStoreState.activeTabId = 'tab-1';
    streamText('assistant answer');

    await useAiChatStore.getState().sendMessage('first question');

    const conversation = useAiChatStore.getState().conversations[0];
    const userMessage = conversation.messages.find((message) => message.role === 'user');
    const assistantMessage = conversation.messages.find((message) => message.role === 'assistant');

    expect(assistantMessage?.turn?.parts).toEqual([
      expect.objectContaining({ type: 'text', text: 'assistant answer' }),
    ]);
    expect(assistantMessage?.transcriptRef).toEqual({
      conversationId: 'conv-1',
      startEntryId: userMessage?.id,
      endEntryId: assistantMessage?.id,
    });
    expect(conversation.turns?.[0]).toMatchObject({
      requestMessageId: userMessage?.id,
      requestText: 'first question',
      status: 'complete',
    });
    expect(conversation.sessionMetadata).toMatchObject({
      conversationId: 'conv-1',
      firstUserMessage: 'first question',
      origin: 'sidebar',
      providerId: 'provider-1',
      providerModel: 'mock-model',
      affectedTabIds: ['tab-1'],
      lastBudgetLevel: 0,
    });

    expect(invokeMock).toHaveBeenCalledWith('ai_chat_append_transcript_entries', {
      request: expect.objectContaining({
        conversationId: 'conv-1',
        entries: expect.arrayContaining([
          expect.objectContaining({ kind: 'user_message' }),
          expect.objectContaining({ kind: 'assistant_turn_start' }),
        ]),
      }),
    });
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_save_message_with_transcript', {
      request: expect.objectContaining({
        transcriptEntries: expect.arrayContaining([
          expect.objectContaining({ kind: 'assistant_turn_end' }),
          expect.objectContaining({ kind: 'assistant_part' }),
        ]),
      }),
    });
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_append_diagnostic_events', {
      request: expect.objectContaining({
        conversationId: 'conv-1',
        events: expect.arrayContaining([
          expect.objectContaining({ type: 'user_message' }),
          expect.objectContaining({ type: 'budget_level_changed' }),
          expect.objectContaining({ type: 'llm_request' }),
        ]),
      }),
    });
  });

  it('records synthetic rejected tool calls with preserved arguments when tools are disabled', async () => {
    setConversation([]);
    providerStreamMock.mockImplementation(async function* () {
      yield { type: 'tool_call_complete', id: 'tool-disabled-1', name: 'local_exec', arguments: JSON.stringify({ command: 'pwd' }) };
      yield { type: 'done' };
    });

    await useAiChatStore.getState().sendMessage('run pwd');

    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
    expect(assistantMessage?.toolCalls?.[0]).toMatchObject({
      id: 'tool-disabled-1',
      arguments: '{"command":"pwd"}',
      status: 'rejected',
      result: expect.objectContaining({ error: 'Tool execution unavailable: tool use is not enabled.' }),
    });
    expect(assistantMessage?.turn?.parts).toEqual(expect.arrayContaining([
      expect.objectContaining({
        type: 'guardrail',
        code: 'tool-use-disabled',
      }),
    ]));

    expect(invokeMock).toHaveBeenCalledWith('ai_chat_save_message_with_transcript', {
      request: expect.objectContaining({
        message: expect.objectContaining({ conversationId: 'conv-1' }),
        transcriptEntries: expect.arrayContaining([
          expect.objectContaining({ kind: 'tool_call' }),
          expect.objectContaining({ kind: 'tool_result' }),
          expect.objectContaining({ kind: 'guardrail' }),
        ]),
      }),
    });
  });

  it('hard-denies pseudo tool transcripts, retries once, and keeps raw text out of the visible answer', async () => {
    setConversation([]);
    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: '{"name":"terminal_exec","arguments":{"command":"pwd"}}' };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: 'I cannot access tools in this chat, but you can run pwd in your shell to inspect the current directory.' };
        yield { type: 'done' };
      });

    await useAiChatStore.getState().sendMessage('run pwd');

    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
    expect(providerStreamMock).toHaveBeenCalledTimes(2);
    expect(assistantMessage?.content).toBe('I cannot access tools in this chat, but you can run pwd in your shell to inspect the current directory.');
    expect(assistantMessage?.turn?.parts).toEqual(expect.arrayContaining([
      expect.objectContaining({
        type: 'guardrail',
        code: 'tool-disabled-hard-deny',
        rawText: '{"name":"terminal_exec","arguments":{"command":"pwd"}}',
      }),
      expect.objectContaining({
        type: 'text',
        text: 'I cannot access tools in this chat, but you can run pwd in your shell to inspect the current directory.',
      }),
    ]));

    const retryToolMessage = providerStreamMock.mock.calls[1]?.[1]?.find((message: { role: string; tool_name?: string }) => (
      message.role === 'tool' && message.tool_name === 'tool_use_disabled'
    ));
    expect(retryToolMessage?.content).toContain('tool_denied');

    expect(invokeMock).toHaveBeenCalledWith('ai_chat_append_transcript_entries', {
      request: expect.objectContaining({
        conversationId: 'conv-1',
        entries: expect.arrayContaining([
          expect.objectContaining({ kind: 'assistant_round' }),
          expect.objectContaining({ kind: 'tool_call', payload: expect.objectContaining({ syntheticDenied: true }) }),
          expect.objectContaining({ kind: 'tool_result', payload: expect.objectContaining({ syntheticDenied: true }) }),
        ]),
      }),
    });
  });

  it('does not hard-deny pseudo tool shaped JSON when the user explicitly asked for JSON', async () => {
    setConversation([]);
    streamEvents([
      { type: 'content', content: '{"name":"terminal_exec","arguments":{"command":"pwd"}}' },
    ]);

    await useAiChatStore.getState().sendMessage('return a JSON example for terminal_exec');

    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
    expect(providerStreamMock).toHaveBeenCalledTimes(1);
    expect(assistantMessage?.content).toBe('{"name":"terminal_exec","arguments":{"command":"pwd"}}');
    expect(assistantMessage?.turn?.parts.some((part) => part.type === 'guardrail')).toBe(false);
  });

  it('drops pre-hard-deny thinking content before retrying', async () => {
    setConversation([]);
    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'thinking', content: 'I should call a tool.' };
        yield { type: 'content', content: '{"name":"terminal_exec","arguments":{"command":"pwd"}}' };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: 'Tool access is disabled here, so I can only explain what to run.' };
        yield { type: 'done' };
      });

    await useAiChatStore.getState().sendMessage('run pwd');

    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant');
    expect(providerStreamMock).toHaveBeenCalledTimes(2);
    expect(assistantMessage?.thinkingContent).toBeUndefined();
    expect(assistantMessage?.turn?.parts.some((part) => part.type === 'thinking')).toBe(false);
    expect(assistantMessage?.content).toBe('Tool access is disabled here, so I can only explain what to run.');
  });

  it('reuses the existing user message as the request anchor when skipUserMessage is true', async () => {
    setConversation([
      { id: 'u-existing', role: 'user', content: 'persisted prompt', timestamp: 1 },
    ]);
    streamText('regenerated answer');

    await useAiChatStore.getState().sendMessage('persisted prompt', undefined, { skipUserMessage: true });

    const conversation = useAiChatStore.getState().conversations[0];
    const assistantMessage = conversation.messages.find((message) => message.role === 'assistant');

    expect(assistantMessage?.transcriptRef).toEqual({
      conversationId: 'conv-1',
      startEntryId: 'u-existing',
      endEntryId: assistantMessage?.id,
    });
    expect(conversation.turns?.[0]).toMatchObject({
      requestMessageId: 'u-existing',
      requestText: 'persisted prompt',
    });
  });

  it('regenerateLastResponse truncates assistant replies and resends the last user message', async () => {
    const sendMessage = vi.fn().mockResolvedValue(undefined);
    setConversation([
      { id: 'u-1', role: 'user', content: 'first', timestamp: 1 },
      { id: 'a-1', role: 'assistant', content: 'answer', timestamp: 2 },
    ]);
    useAiChatStore.setState({ sendMessage: sendMessage as never });

    await useAiChatStore.getState().regenerateLastResponse();

    expect(useAiChatStore.getState().conversations[0].messages).toEqual([
      { id: 'u-1', role: 'user', content: 'first', timestamp: 1 },
    ]);
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_delete_messages_after', {
      conversationId: 'conv-1',
      afterMessageId: 'u-1',
    });
    expect(sendMessage).toHaveBeenCalledWith('first', undefined, { skipUserMessage: true });
  });

  it('editAndResend rolls back local state when backend cleanup fails', async () => {
    setConversation([
      { id: 'u-1', role: 'user', content: 'original', timestamp: 1 },
      { id: 'a-1', role: 'assistant', content: 'reply', timestamp: 2 },
    ]);
    invokeMock.mockRejectedValueOnce(new Error('delete failed'));
    const sendMessage = vi.fn().mockResolvedValue(undefined);
    useAiChatStore.setState({ sendMessage: sendMessage as never });

    await useAiChatStore.getState().editAndResend('u-1', 'edited');

    expect(sendMessage).not.toHaveBeenCalled();
    expect(useAiChatStore.getState().conversations[0].messages).toHaveLength(2);
    expect(useAiChatStore.getState().error).toBe('ai.message.edit_failed');
  });

  it('switchBranch rebuilds the backend conversation from the selected branch tail', async () => {
    setConversation([
      {
        id: 'user-live',
        role: 'user',
        content: 'new branch',
        timestamp: 10,
        branches: {
          total: 2,
          activeIndex: 1,
          tails: {
            0: [
              { id: 'user-old', role: 'user', content: 'old branch', timestamp: 1 },
              { id: 'assistant-old', role: 'assistant', content: 'old answer', timestamp: 2 },
            ],
          },
        },
      },
      { id: 'assistant-live', role: 'assistant', content: 'new answer', timestamp: 11 },
    ]);

    await useAiChatStore.getState().switchBranch('user-live', 0);

    expect(invokeMock).toHaveBeenNthCalledWith(1, 'ai_chat_delete_conversation', { conversationId: 'conv-1' });
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'ai_chat_create_conversation', {
      request: {
        id: 'conv-1',
        title: 'Conversation',
        sessionId: null,
        origin: 'sidebar',
        sessionMetadata: null,
      },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(3, 'ai_chat_save_message', expect.objectContaining({
      request: expect.objectContaining({ id: 'user-old', role: 'user' }),
    }));
    expect(invokeMock).toHaveBeenNthCalledWith(4, 'ai_chat_save_message', expect.objectContaining({
      request: expect.objectContaining({
        id: 'assistant-old',
        role: 'assistant',
        transcriptRef: {
          conversationId: 'conv-1',
          startEntryId: 'user-old',
          endEntryId: 'assistant-old',
        },
      }),
    }));
    expect(useAiChatStore.getState().conversations[0].messages[0]).toMatchObject({
      id: 'user-old',
      content: 'old branch',
      branches: expect.objectContaining({ activeIndex: 0 }),
    });
  });

  it('summarizeConversation replaces message history with a generated summary', async () => {
    streamText('Conversation summary');
    setConversation([
      { id: 'u-1', role: 'user', content: 'question', timestamp: 1 },
      {
        id: 'a-1', role: 'assistant', content: 'answer', timestamp: 2,
        turn: {
          id: 'turn-a-1',
          status: 'complete',
          parts: [{ type: 'text', text: 'answer' }],
          toolRounds: [{ id: 'round-1', round: 1, toolCalls: [] }],
          plainTextSummary: 'answer',
        },
      },
      { id: 'u-2', role: 'user', content: 'follow up', timestamp: 3 },
      {
        id: 'a-2', role: 'assistant', content: 'more detail', timestamp: 4,
        turn: {
          id: 'turn-a-2',
          status: 'complete',
          parts: [{ type: 'text', text: 'more detail' }],
          toolRounds: [{ id: 'round-2', round: 2, toolCalls: [] }],
          plainTextSummary: 'more detail',
        },
      },
    ]);

    await useAiChatStore.getState().summarizeConversation();

    expect(providerStreamMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_replace_conversation_messages_with_transcript', expect.objectContaining({
      request: expect.objectContaining({ conversationId: 'conv-1' }),
    }));
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_replace_conversation_messages_with_transcript', expect.objectContaining({
      request: expect.objectContaining({
        conversationId: 'conv-1',
        transcriptEntries: expect.arrayContaining([
          expect.objectContaining({ kind: 'summary_created' }),
        ]),
      }),
    }));
    expect(useAiChatStore.getState().conversations[0].messages).toHaveLength(1);
    expect(useAiChatStore.getState().conversations[0].messages[0]).toMatchObject({
      content: expect.stringContaining('Conversation summary'),
      summaryRef: expect.objectContaining({
        kind: 'conversation',
        roundId: 'round-2',
        transcriptRef: expect.objectContaining({
          startEntryId: 'u-1',
          endEntryId: 'a-2',
        }),
      }),
    });
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_replace_conversation_messages_with_transcript', expect.objectContaining({
      request: expect.objectContaining({
        message: expect.objectContaining({
          summaryRef: expect.objectContaining({
            kind: 'conversation',
            roundId: 'round-2',
            transcriptRef: expect.objectContaining({
              startEntryId: 'u-1',
              endEntryId: 'a-2',
            }),
          }),
        }),
      }),
    }));
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_update_conversation', {
      conversationId: 'conv-1',
      title: 'Conversation',
      sessionMetadata: expect.objectContaining({
        conversationId: 'conv-1',
        lastSummaryRoundId: 'round-2',
      }),
    });
  });

  it('compactConversation creates a compaction anchor and preserves recent messages', async () => {
    streamText('Merged summary');
    estimateTokensMock.mockImplementation(() => 120);
    setConversation([
      { id: 'u-1', role: 'user', content: 'old question', timestamp: 1 },
      {
        id: 'a-1', role: 'assistant', content: 'old answer', timestamp: 2,
        turn: {
          id: 'turn-a-1',
          status: 'complete',
          parts: [{ type: 'text', text: 'old answer' }],
          toolRounds: [{ id: 'round-1', round: 1, toolCalls: [] }],
          plainTextSummary: 'old answer',
        },
      },
      { id: 'u-2', role: 'user', content: 'middle question', timestamp: 3 },
      {
        id: 'a-2', role: 'assistant', content: 'middle answer', timestamp: 4,
        turn: {
          id: 'turn-a-2',
          status: 'complete',
          parts: [{ type: 'text', text: 'middle answer' }],
          toolRounds: [{ id: 'round-2', round: 2, toolCalls: [] }],
          plainTextSummary: 'middle answer',
        },
      },
      { id: 'u-3', role: 'user', content: 'recent question', timestamp: 5 },
      {
        id: 'a-3', role: 'assistant', content: 'recent answer', timestamp: 6,
        turn: {
          id: 'turn-a-3',
          status: 'complete',
          parts: [{ type: 'text', text: 'recent answer' }],
          toolRounds: [{ id: 'round-3', round: 3, toolCalls: [] }],
          plainTextSummary: 'recent answer',
        },
      },
    ]);

    await useAiChatStore.getState().compactConversation('conv-1');

    const compacted = useAiChatStore.getState().conversations[0].messages;
    expect(compacted[0]).toMatchObject({
      role: 'system',
      content: 'Merged summary',
      summaryRef: expect.objectContaining({
        kind: 'compaction',
        roundId: 'round-1',
        transcriptRef: expect.objectContaining({
          startEntryId: 'u-1',
          endEntryId: 'u-2',
        }),
      }),
      metadata: expect.objectContaining({ type: 'compaction-anchor', originalCount: 3 }),
    });
    expect(compacted.slice(1).map((message) => message.id)).toEqual(['a-2', 'u-3', 'a-3']);
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_replace_conversation_message_list_with_transcript', expect.objectContaining({
      request: expect.objectContaining({
        conversationId: 'conv-1',
        expectedMessageIds: ['u-1', 'a-1', 'u-2', 'a-2', 'u-3', 'a-3'],
        messages: expect.arrayContaining([
          expect.objectContaining({ id: 'a-2' }),
          expect.objectContaining({ id: 'u-3' }),
          expect.objectContaining({ id: 'a-3' }),
        ]),
      }),
    }));
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_update_conversation', {
      conversationId: 'conv-1',
      title: 'Conversation',
      sessionMetadata: expect.objectContaining({
        conversationId: 'conv-1',
        lastSummaryRoundId: 'round-1',
        lastCompactedUntilEntryId: 'u-2',
      }),
    });
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_replace_conversation_message_list_with_transcript', expect.objectContaining({
      request: expect.objectContaining({
        conversationId: 'conv-1',
        transcriptEntries: expect.arrayContaining([
          expect.objectContaining({ kind: 'summary_created' }),
        ]),
        messages: expect.arrayContaining([
          expect.objectContaining({
            id: compacted[0].id,
            summaryRef: expect.objectContaining({
              kind: 'compaction',
              roundId: 'round-1',
              transcriptRef: expect.objectContaining({
                startEntryId: 'u-1',
                endEntryId: 'u-2',
              }),
            }),
          }),
        ]),
      }),
    }));
    expect(useAiChatStore.getState().conversations[0].sessionMetadata).toMatchObject({
      conversationId: 'conv-1',
      lastSummaryRoundId: 'round-1',
      lastCompactedUntilEntryId: 'u-2',
    });
  });

  it('pre-compacts long history before sending when budget reaches level 2', async () => {
    estimateTokensMock.mockImplementation((text?: string) => {
      if (typeof text === 'string' && text.includes('system')) return 40;
      return 80;
    });
    setConversation([
      { id: 'u-1', role: 'user', content: 'old question', timestamp: 1 },
      { id: 'a-1', role: 'assistant', content: 'old answer', timestamp: 2 },
      { id: 'u-2', role: 'user', content: 'middle question', timestamp: 3 },
      { id: 'a-2', role: 'assistant', content: 'middle answer', timestamp: 4 },
      { id: 'u-3', role: 'user', content: 'recent question', timestamp: 5 },
      { id: 'a-3', role: 'assistant', content: 'recent answer', timestamp: 6 },
    ]);
    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: 'Compacted summary' };
        yield { type: 'done' };
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: 'final answer' };
        yield { type: 'done' };
      });

    await useAiChatStore.getState().sendMessage('new question');

    expect(providerStreamMock).toHaveBeenCalledTimes(2);
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_replace_conversation_message_list_with_transcript', expect.objectContaining({
      request: expect.objectContaining({ conversationId: 'conv-1' }),
    }));

    const providerMessages = providerStreamMock.mock.calls[1]?.[1];
    expect(providerMessages).toEqual(expect.arrayContaining([
      expect.objectContaining({ role: 'system', content: expect.stringContaining('Previous conversation summary:\nCompacted summary') }),
    ]));
  });

  it('falls back to trimmed history when pre-send compaction fails', async () => {
    estimateTokensMock.mockImplementation((text?: string) => {
      if (typeof text === 'string' && text.includes('system')) return 40;
      return 80;
    });
    setConversation([
      { id: 'u-1', role: 'user', content: 'old question', timestamp: 1 },
      { id: 'a-1', role: 'assistant', content: 'old answer', timestamp: 2 },
      { id: 'u-2', role: 'user', content: 'middle question', timestamp: 3 },
      { id: 'a-2', role: 'assistant', content: 'middle answer', timestamp: 4 },
      { id: 'u-3', role: 'user', content: 'recent question', timestamp: 5 },
      { id: 'a-3', role: 'assistant', content: 'recent answer', timestamp: 6 },
    ]);
    providerStreamMock
      .mockImplementationOnce(async function* () {
        throw new Error('compact failed');
      })
      .mockImplementationOnce(async function* () {
        yield { type: 'content', content: 'final answer' };
        yield { type: 'done' };
      });

    await useAiChatStore.getState().sendMessage('new question');

    expect(providerStreamMock).toHaveBeenCalledTimes(2);
    const assistantMessage = useAiChatStore.getState().conversations[0].messages.find((message) => message.role === 'assistant' && message.content === 'final answer');
    expect(assistantMessage?.content).toBe('final answer');
  });

  it('injects transcript lookup reference when an existing summary anchor keeps the prompt near the upper budget threshold', async () => {
    estimateTokensMock.mockImplementation((text?: string) => {
      if (typeof text === 'string' && text.includes('system')) return 40;
      return 90;
    });
    setConversation([
      {
        id: 'anchor-1',
        role: 'system',
        content: 'Compacted summary',
        timestamp: 1,
        summaryRef: {
          kind: 'compaction',
          transcriptRef: { conversationId: 'conv-1', startEntryId: 'u-1', endEntryId: 'a-2' },
        },
        metadata: { type: 'compaction-anchor', originalCount: 4, compactedAt: 1 },
      },
      { id: 'u-3', role: 'user', content: 'recent question', timestamp: 2 },
      { id: 'a-3', role: 'assistant', content: 'recent answer', timestamp: 3 },
      { id: 'u-4', role: 'user', content: 'latest question', timestamp: 4 },
      { id: 'a-4', role: 'assistant', content: 'latest answer', timestamp: 5 },
    ]);
    streamText('final answer');

    await useAiChatStore.getState().sendMessage('new question');

    expect(providerStreamMock).toHaveBeenCalledTimes(1);

    const providerMessages = providerStreamMock.mock.calls[0]?.[1];
    expect(providerMessages).toEqual(expect.arrayContaining([
      expect.objectContaining({ role: 'system', content: expect.stringContaining('Previous conversation summary:\nCompacted summary') }),
      expect.objectContaining({ role: 'system', content: expect.stringContaining('Transcript reference: conversation=conv-1') }),
    ]));

    expect(useAiChatStore.getState().conversations[0].sessionMetadata).toMatchObject({
      conversationId: 'conv-1',
      lastBudgetLevel: 3,
    });
  });

  it('silent compaction refreshes the active conversation immediately', async () => {
    streamText('Merged summary');
    estimateTokensMock.mockImplementation(() => 200);
    setConversation([
      { id: 'u-1', role: 'user', content: 'old question', timestamp: 1 },
      { id: 'a-1', role: 'assistant', content: 'old answer', timestamp: 2 },
      { id: 'u-2', role: 'user', content: 'middle question', timestamp: 3 },
      { id: 'a-2', role: 'assistant', content: 'middle answer', timestamp: 4 },
      { id: 'u-3', role: 'user', content: 'recent question', timestamp: 5 },
      { id: 'a-3', role: 'assistant', content: 'recent answer', timestamp: 6 },
    ]);

    await useAiChatStore.getState().compactConversation('conv-1', { silent: true });

    const state = useAiChatStore.getState();
    expect(state.conversations[0].messages[0]).toMatchObject({
      role: 'system',
      content: 'Merged summary',
      metadata: expect.objectContaining({ type: 'compaction-anchor' }),
    });
    expect(state.conversations[0].messages.length).toBeLessThan(6);
    expect(state.compactionInfo).toMatchObject({
      conversationId: 'conv-1',
      mode: 'silent',
      phase: 'done',
      compactedCount: expect.any(Number),
    });
  });

  it('preserves messages appended while compaction is in flight', async () => {
    let releaseStream!: () => void;
    providerStreamMock.mockImplementation(async function* () {
      await new Promise<void>((resolve) => {
        releaseStream = resolve;
      });
      yield { type: 'content', content: 'Merged summary' };
      yield { type: 'done' };
    });

    estimateTokensMock.mockImplementation(() => 200);
    setConversation([
      { id: 'u-1', role: 'user', content: 'old question', timestamp: 1 },
      { id: 'a-1', role: 'assistant', content: 'old answer', timestamp: 2 },
      { id: 'u-2', role: 'user', content: 'middle question', timestamp: 3 },
      { id: 'a-2', role: 'assistant', content: 'middle answer', timestamp: 4 },
      { id: 'u-3', role: 'user', content: 'recent question', timestamp: 5 },
      { id: 'a-3', role: 'assistant', content: 'recent answer', timestamp: 6 },
    ]);

    const compactPromise = useAiChatStore.getState().compactConversation('conv-1', { silent: true });
    await Promise.resolve();
    await Promise.resolve();

    useAiChatStore.setState((state) => ({
      conversations: state.conversations.map((conversation) => {
        if (conversation.id !== 'conv-1') return conversation;
        return {
          ...conversation,
          messages: [
            ...conversation.messages,
            { id: 'u-4', role: 'user', content: 'new question', timestamp: 7 },
            { id: 'a-4', role: 'assistant', content: 'new answer', timestamp: 8 },
          ],
          sessionMetadata: {
            conversationId: 'conv-1',
            lastBudgetLevel: 4,
            affectedTabIds: ['tab-race'],
          },
          updatedAt: 8,
        };
      }),
    }));

    releaseStream();
    await compactPromise;

    const messages = useAiChatStore.getState().conversations[0].messages;
    expect(messages[0]).toMatchObject({
      role: 'system',
      content: 'Merged summary',
      metadata: expect.objectContaining({ type: 'compaction-anchor' }),
    });
    expect(messages.slice(-2).map((message) => message.id)).toEqual(['u-4', 'a-4']);
    const sessionMetadata = useAiChatStore.getState().conversations[0].sessionMetadata;
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_replace_conversation_message_list_with_transcript', expect.objectContaining({
      request: expect.objectContaining({
        conversationId: 'conv-1',
        expectedMessageIds: ['u-1', 'a-1', 'u-2', 'a-2', 'u-3', 'a-3', 'u-4', 'a-4'],
        messages: expect.arrayContaining([
          expect.objectContaining({ id: 'u-4' }),
          expect.objectContaining({ id: 'a-4' }),
        ]),
      }),
    }));
    expect(sessionMetadata).toMatchObject({
      conversationId: 'conv-1',
      lastBudgetLevel: 4,
      affectedTabIds: ['tab-race'],
    });
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_update_conversation', {
      conversationId: 'conv-1',
      title: 'Conversation',
      sessionMetadata: expect.objectContaining({
        conversationId: 'conv-1',
        lastBudgetLevel: 4,
        affectedTabIds: ['tab-race'],
      }),
    });
  });

  it('gathers sidebar context once and reuses it for context, reminder, and persistence', async () => {
    const sidebarContext: SidebarContext = {
      env: {
        localOS: 'macOS',
        terminalType: 'terminal',
        activeTabType: 'terminal',
        activeNodeId: 'node-1',
        sessionId: 'session-1',
        cwd: '/tmp/project',
        connection: {
          id: 'conn-1',
          host: 'host',
          port: 22,
          username: 'user',
          formatted: 'user@host:22',
        },
        remoteEnv: undefined,
        remoteOSHint: 'Linux',
      },
      terminal: {
        buffer: 'BUFFER BLOCK',
        lineCount: 3,
        selection: 'SELECTED',
        hasSelection: true,
      },
      ide: null,
      sftp: null,
      systemPromptSegment: 'SYSTEM SEGMENT',
      contextBlock: 'CONTEXT BLOCK',
      gatheredAt: 123,
    };
    gatherSidebarContextMock.mockReturnValue(sidebarContext);
    buildContextReminderMock.mockImplementation((ctx: unknown) => {
      expect(ctx).toBe(sidebarContext);
      return 'REMINDER';
    });
    streamText('assistant reply');
    setConversation([
      { id: 'u-1', role: 'user', content: 'earlier question', timestamp: 1 },
      { id: 'a-1', role: 'assistant', content: 'earlier answer', timestamp: 2 },
    ]);

    await useAiChatStore.getState().sendMessage('new question');

    expect(gatherSidebarContextMock).toHaveBeenCalledTimes(1);
    expect(buildContextReminderMock).toHaveBeenCalledTimes(1);

    const currentConversation = useAiChatStore.getState().conversations[0];
    const persistedUserMessage = currentConversation.messages.find((message) => message.content === 'new question');
    expect(persistedUserMessage?.context).toBe('CONTEXT BLOCK');

    const saveCalls = invokeMock.mock.calls.filter(([command]) => command === 'ai_chat_save_message');
    expect(saveCalls[0][1]).toMatchObject({
      request: expect.objectContaining({
        contextSnapshot: {
          sessionId: 'session-1',
          connectionName: 'user@host:22',
          remoteOs: 'Linux',
          cwd: '/tmp/project',
          selection: 'SELECTED',
          bufferTail: 'BUFFER BLOCK',
        },
      }),
    });

    const [, providerMessages] = providerStreamMock.mock.calls[0];
    expect(providerMessages).toEqual(expect.arrayContaining([
      expect.objectContaining({ role: 'system', content: expect.stringContaining('SYSTEM SEGMENT') }),
      expect.objectContaining({ role: 'system', content: expect.stringContaining('CONTEXT BLOCK') }),
      expect.objectContaining({ role: 'system', content: 'REMINDER' }),
    ]));
    const firstNonSystemIndex = providerMessages.findIndex((message: { role: string; content: string }) => message.role !== 'system');
    const reminderIndex = providerMessages.findIndex(
      (message: { role: string; content: string }) => message.role === 'system' && message.content === 'REMINDER',
    );
    expect(reminderIndex).toBeGreaterThanOrEqual(0);
    expect(firstNonSystemIndex).toBeGreaterThan(reminderIndex);
  });

  it('reuses a pre-sampled sidebar context without gathering again', async () => {
    const sidebarContext: SidebarContext = {
      env: {
        localOS: 'macOS',
        terminalType: 'terminal',
        activeTabType: 'terminal',
        activeNodeId: 'node-2',
        sessionId: 'session-2',
        cwd: '/workspace',
        connection: {
          id: 'conn-2',
          host: 'example.com',
          port: 22,
          username: 'dev',
          formatted: 'dev@example.com:22',
        },
        remoteEnv: undefined,
        remoteOSHint: 'Ubuntu',
      },
      terminal: {
        buffer: 'BUFFER FROM SNAPSHOT',
        lineCount: 2,
        selection: null,
        hasSelection: false,
      },
      ide: null,
      sftp: null,
      systemPromptSegment: 'PRE-SAMPLED SEGMENT',
      contextBlock: 'BUFFER FROM SNAPSHOT',
      gatheredAt: 456,
    };
    gatherSidebarContextMock.mockImplementation(() => {
      throw new Error('should not gather again');
    });
    buildContextReminderMock.mockImplementation((ctx: unknown) => {
      expect(ctx).toBe(sidebarContext);
      return 'PRE-SAMPLED REMINDER';
    });
    streamText('assistant reply');
    setConversation([
      { id: 'u-1', role: 'user', content: 'older question', timestamp: 1 },
      { id: 'a-1', role: 'assistant', content: 'older answer', timestamp: 2 },
    ]);

    await useAiChatStore.getState().sendMessage('new question', 'EXPLICIT BUFFER', { sidebarContext });

    expect(gatherSidebarContextMock).not.toHaveBeenCalled();
    const currentConversation = useAiChatStore.getState().conversations[0];
    const userMessage = currentConversation.messages.find((message) => message.content === 'new question');
    expect(userMessage?.context).toBe('EXPLICIT BUFFER');

    const saveCalls = invokeMock.mock.calls.filter(([command]) => command === 'ai_chat_save_message');
    expect(saveCalls[0][1]).toMatchObject({
      request: expect.objectContaining({
        contextSnapshot: {
          sessionId: 'session-2',
          connectionName: 'dev@example.com:22',
          remoteOs: 'Ubuntu',
          cwd: '/workspace',
          selection: null,
          bufferTail: 'BUFFER FROM SNAPSHOT',
        },
      }),
    });

    const [, providerMessages] = providerStreamMock.mock.calls[0];
    expect(providerMessages).toEqual(expect.arrayContaining([
      expect.objectContaining({ role: 'system', content: expect.stringContaining('PRE-SAMPLED SEGMENT') }),
      expect.objectContaining({ role: 'system', content: expect.stringContaining('EXPLICIT BUFFER') }),
      expect.objectContaining({ role: 'system', content: 'PRE-SAMPLED REMINDER' }),
    ]));
    const firstNonSystemIndex = providerMessages.findIndex((message: { role: string; content: string }) => message.role !== 'system');
    const reminderIndex = providerMessages.findIndex(
      (message: { role: string; content: string }) => message.role === 'system' && message.content === 'PRE-SAMPLED REMINDER',
    );
    expect(reminderIndex).toBeGreaterThanOrEqual(0);
    expect(firstNonSystemIndex).toBeGreaterThan(reminderIndex);
  });

  it('captures default sidebar context before async reference resolution starts', async () => {
    const firstSidebarContext: SidebarContext = {
      env: {
        localOS: 'macOS',
        terminalType: 'terminal',
        activeTabType: 'terminal',
        activeNodeId: 'node-3',
        sessionId: 'session-3',
        cwd: '/first',
        connection: {
          id: 'conn-3',
          host: 'first-host',
          port: 22,
          username: 'first',
          formatted: 'first@first-host:22',
        },
        remoteEnv: undefined,
        remoteOSHint: 'Debian',
      },
      terminal: {
        buffer: 'FIRST BUFFER',
        lineCount: 1,
        selection: null,
        hasSelection: false,
      },
      ide: null,
      sftp: null,
      systemPromptSegment: 'FIRST SEGMENT',
      contextBlock: 'FIRST CONTEXT',
      gatheredAt: 100,
    };
    const secondSidebarContext: SidebarContext = {
      ...firstSidebarContext,
      env: {
        ...firstSidebarContext.env,
        cwd: '/second',
      },
      terminal: {
        ...firstSidebarContext.terminal,
        buffer: 'SECOND BUFFER',
      },
      systemPromptSegment: 'SECOND SEGMENT',
      contextBlock: 'SECOND CONTEXT',
      gatheredAt: 200,
    };

    let referenceResolutionStarted = false;
    gatherSidebarContextMock.mockImplementation(() => {
      expect(referenceResolutionStarted).toBe(false);
      return referenceResolutionStarted ? secondSidebarContext : firstSidebarContext;
    });
    resolveReferenceTypeMock.mockReturnValue({ type: 'selection' });
    resolveAllReferencesMock.mockImplementation(async () => {
      referenceResolutionStarted = true;
      return 'REFERENCE BLOCK';
    });
    parseUserInputMock.mockReturnValue({
      slashCommand: null,
      participants: [],
      references: [{ type: 'selection', raw: '#selection' }],
      cleanText: 'question with reference',
    });
    buildContextReminderMock.mockImplementation((ctx: unknown) => {
      expect(ctx).toBe(firstSidebarContext);
      return 'REFERENCE REMINDER';
    });
    streamText('assistant reply');
    setConversation([
      { id: 'u-1', role: 'user', content: 'older question', timestamp: 1 },
      { id: 'a-1', role: 'assistant', content: 'older answer', timestamp: 2 },
    ]);

    await useAiChatStore.getState().sendMessage('question with reference');

    expect(gatherSidebarContextMock).toHaveBeenCalledTimes(1);
    const currentConversation = useAiChatStore.getState().conversations[0];
    const userMessage = currentConversation.messages.find((message) => message.content === 'question with reference');
    expect(userMessage?.context).toBe('FIRST CONTEXT\n\nREFERENCE BLOCK');

    const [, providerMessages] = providerStreamMock.mock.calls[0];
    expect(providerMessages).toEqual(expect.arrayContaining([
      expect.objectContaining({ role: 'system', content: expect.stringContaining('FIRST SEGMENT') }),
      expect.objectContaining({ role: 'system', content: expect.stringContaining('FIRST CONTEXT\n\nREFERENCE BLOCK') }),
      expect.objectContaining({ role: 'system', content: 'REFERENCE REMINDER' }),
    ]));
  });
});
