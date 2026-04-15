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
const getProviderMock = vi.hoisted(() => vi.fn());
const contextFreeToolsMock = vi.hoisted(() => new Set(['local_exec']));
const sessionIdToolsMock = vi.hoisted(() => new Set<string>());
const getToolsForContextMock = vi.hoisted(() => vi.fn<() => AiToolDefinition[]>(() => []));
const executeToolMock = vi.hoisted(() => vi.fn());
const hasDeniedCommandsMock = vi.hoisted(() => vi.fn(() => false));
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
        toolUse: {
          enabled: false,
          disabledTools: [],
          autoApproveTools: {},
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
  api: { getAiProviderApiKey: apiMocks.getAiProviderApiKey },
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
  getToolsForContext: getToolsForContextMock,
  isCommandDenied: vi.fn(() => false),
  hasDeniedCommands: hasDeniedCommandsMock,
  executeTool: executeToolMock,
}));

vi.mock('@/lib/ai/inputParser', () => ({
  parseUserInput: parseUserInputMock,
}));

vi.mock('@/lib/ai/slashCommands', () => ({
  resolveSlashCommand: resolveSlashCommandMock,
  SLASH_COMMANDS: [],
}));

vi.mock('@/lib/ai/participants', () => ({
  PARTICIPANTS: [],
  resolveParticipant: vi.fn(),
  mergeParticipantTools: vi.fn(() => new Set()),
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

async function waitFor(predicate: () => boolean, attempts = 40) {
  for (let attempt = 0; attempt < attempts; attempt++) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  throw new Error('Condition not met in time');
}

describe('aiChatStore workflows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    settingsStoreMock.state.settings.ai.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.enabled = false;
    settingsStoreMock.state.settings.ai.toolUse.disabledTools = [];
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = {};
    parseUserInputMock.mockReturnValue({ slashCommand: null, participants: [], references: [], cleanText: '' });
    resolveSlashCommandMock.mockReturnValue(undefined);
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
    executeToolMock.mockReset();
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
      sessionDisabledTools: null,
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

  it('updates approval state on the originating conversation even after switching activeConversationId', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    setConversation([]);
    getToolsForContextMock.mockReturnValue([
      { name: 'local_exec', description: 'Run a local command', parameters: {} },
    ]);
    hasDeniedCommandsMock.mockReturnValue(true);
    executeToolMock.mockResolvedValue({
      toolCallId: 'tool-1',
      toolName: 'local_exec',
      success: true,
      output: 'ok',
    });

    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'tool_call_complete', id: 'tool-1', name: 'local_exec', arguments: JSON.stringify({ command: 'sudo reboot' }) };
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
      return assistant?.toolCalls?.[0]?.status === 'pending_user_approval';
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

  it('keeps the assistant message visible when the provider errors after a tool round', async () => {
    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = { local_exec: true };
    setConversation([]);
    getToolsForContextMock.mockReturnValue([
      { name: 'local_exec', description: 'Run a local command', parameters: {} },
    ]);
    hasDeniedCommandsMock.mockReturnValue(false);
    executeToolMock.mockResolvedValue({
      toolCallId: 'tool-keep-1',
      toolName: 'local_exec',
      success: true,
      output: 'ok',
    });

    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'thinking', content: 'Let me inspect that first.' };
        yield { type: 'tool_call_complete', id: 'tool-keep-1', name: 'local_exec', arguments: JSON.stringify({ command: 'uname -a' }) };
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

  it('shows and clears a transient post-tool waiting marker before the follow-up summary arrives', async () => {
    let releaseSummary!: () => void;

    settingsStoreMock.state.settings.ai.toolUse.enabled = true;
    settingsStoreMock.state.settings.ai.toolUse.autoApproveTools = { local_exec: true };
    setConversation([]);
    getToolsForContextMock.mockReturnValue([
      { name: 'local_exec', description: 'Run a local command', parameters: {} },
    ]);
    executeToolMock.mockResolvedValue({
      toolCallId: 'tool-wait-1',
      toolName: 'local_exec',
      success: true,
      output: 'pwd',
    });

    providerStreamMock
      .mockImplementationOnce(async function* () {
        yield { type: 'tool_call_complete', id: 'tool-wait-1', name: 'local_exec', arguments: JSON.stringify({ command: 'pwd' }) };
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

  it('injects the disabled-tools negative constraint into the system prompt', async () => {
    setConversation([]);
    streamText('plain answer');

    await useAiChatStore.getState().sendMessage('what files are here?');

    const providerMessages = providerStreamMock.mock.calls[0]?.[1];
    expect(providerMessages?.[0]?.role).toBe('system');
    expect(providerMessages?.[0]?.content).toContain('TOOL CALLING IS CURRENTLY DISABLED');
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