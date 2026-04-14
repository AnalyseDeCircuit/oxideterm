import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());
const parseUserInputMock = vi.hoisted(() => vi.fn(() => ({
  slashCommand: null as { name: string; raw: string } | null,
  participants: [] as { name: string; raw: string }[],
  references: [] as { type: string; value?: string; raw: string }[],
  cleanText: '',
})));
const resolveSlashCommandMock = vi.hoisted(() => vi.fn());
const getProviderMock = vi.hoisted(() => vi.fn());
const estimateTokensMock = vi.hoisted(() => vi.fn(() => 100));
const getModelContextWindowMock = vi.hoisted(() => vi.fn(() => 1000));
const responseReserveMock = vi.hoisted(() => vi.fn(() => 256));
const trimHistoryMock = vi.hoisted(() => vi.fn((messages) => ({ messages, trimmedCount: 0 })));
const providerStreamMock = vi.hoisted(() => vi.fn());
const gatherSidebarContextMock = vi.hoisted(() => vi.fn<() => unknown>(() => null));
const buildContextReminderMock = vi.hoisted(() => vi.fn<(ctx: unknown) => string | null>(() => null));
const resolveReferenceTypeMock = vi.hoisted(() => vi.fn());
const resolveAllReferencesMock = vi.hoisted(() => vi.fn<(...args: unknown[]) => unknown>(() => []));
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
    getState: () => ({ tabs: [], activeTabId: null, sessions: new Map() }),
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
  CONTEXT_FREE_TOOLS: [],
  SESSION_ID_TOOLS: [],
  getToolsForContext: vi.fn(() => []),
  isCommandDenied: vi.fn(() => false),
  hasDeniedCommands: vi.fn(() => false),
  executeTool: vi.fn(),
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

function makeConversation(messages: AiChatMessage[] = []): AiConversation {
  return {
    id: 'conv-1',
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

describe('aiChatStore workflows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    settingsStoreMock.state.settings.ai.enabled = true;
    parseUserInputMock.mockReturnValue({ slashCommand: null, participants: [], references: [], cleanText: '' });
    resolveSlashCommandMock.mockReturnValue(undefined);
    getProviderMock.mockReturnValue({ streamCompletion: providerStreamMock });
    estimateTokensMock.mockImplementation(() => 100);
    trimHistoryMock.mockImplementation((messages) => ({ messages, trimmedCount: 0 }));
    gatherSidebarContextMock.mockReturnValue(null);
    buildContextReminderMock.mockReturnValue(null);
    resolveReferenceTypeMock.mockReturnValue(undefined);
    resolveAllReferencesMock.mockResolvedValue([]);
    streamText('summary text');
    useAiChatStore.setState({
      ...initialAiChatStoreState,
      conversations: [],
      activeConversationId: null,
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
      },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(3, 'ai_chat_save_message', expect.objectContaining({
      request: expect.objectContaining({ id: 'user-old', role: 'user' }),
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
      { id: 'a-1', role: 'assistant', content: 'answer', timestamp: 2 },
      { id: 'u-2', role: 'user', content: 'follow up', timestamp: 3 },
      { id: 'a-2', role: 'assistant', content: 'more detail', timestamp: 4 },
    ]);

    await useAiChatStore.getState().summarizeConversation();

    expect(providerStreamMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_replace_conversation_messages', expect.objectContaining({
      request: expect.objectContaining({ conversationId: 'conv-1' }),
    }));
    expect(useAiChatStore.getState().conversations[0].messages).toHaveLength(1);
    expect(useAiChatStore.getState().conversations[0].messages[0].content).toContain('Conversation summary');
  });

  it('compactConversation creates a compaction anchor and preserves recent messages', async () => {
    streamText('Merged summary');
    estimateTokensMock.mockImplementation(() => 120);
    setConversation([
      { id: 'u-1', role: 'user', content: 'old question', timestamp: 1 },
      { id: 'a-1', role: 'assistant', content: 'old answer', timestamp: 2 },
      { id: 'u-2', role: 'user', content: 'middle question', timestamp: 3 },
      { id: 'a-2', role: 'assistant', content: 'middle answer', timestamp: 4 },
      { id: 'u-3', role: 'user', content: 'recent question', timestamp: 5 },
      { id: 'a-3', role: 'assistant', content: 'recent answer', timestamp: 6 },
    ]);

    await useAiChatStore.getState().compactConversation('conv-1');

    const compacted = useAiChatStore.getState().conversations[0].messages;
    expect(compacted[0]).toMatchObject({
      role: 'system',
      content: 'Merged summary',
      metadata: expect.objectContaining({ type: 'compaction-anchor', originalCount: 3 }),
    });
    expect(compacted.slice(1).map((message) => message.id)).toEqual(['a-2', 'u-3', 'a-3']);
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_replace_conversation_message_list', expect.objectContaining({
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
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_replace_conversation_message_list', expect.objectContaining({
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