import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());
const parseUserInputMock = vi.hoisted(() => vi.fn(() => ({
  slashCommand: null,
  participants: [],
  references: [],
  cleanText: '',
})));
const resolveSlashCommandMock = vi.hoisted(() => vi.fn());
const getProviderMock = vi.hoisted(() => vi.fn());
const estimateTokensMock = vi.hoisted(() => vi.fn(() => 100));
const getModelContextWindowMock = vi.hoisted(() => vi.fn(() => 1000));
const responseReserveMock = vi.hoisted(() => vi.fn(() => 256));
const trimHistoryMock = vi.hoisted(() => vi.fn((messages) => ({ messages, trimmedCount: 0 })));
const providerStreamMock = vi.hoisted(() => vi.fn());
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
  gatherSidebarContext: vi.fn(() => null),
  buildContextReminder: vi.fn(() => null),
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
  resolveReferenceType: vi.fn(),
  resolveAllReferences: vi.fn(() => []),
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
import type { AiConversation, AiChatMessage } from '@/types';

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
    streamText('summary text');
    useAiChatStore.setState({
      conversations: [],
      activeConversationId: null,
      isLoading: false,
      isInitialized: true,
      error: null,
      abortController: null,
      trimInfo: null,
      sessionDisabledTools: null,
    });
  });

  it('handles client-only /clear by creating a fresh conversation without streaming', async () => {
    const createConversation = vi.fn().mockResolvedValue('conv-new');
    setConversation([{ id: 'u-1', role: 'user', content: 'hello', timestamp: 1 }]);
    parseUserInputMock.mockReturnValue({
      slashCommand: { name: 'clear' },
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
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_replace_conversation_messages', expect.objectContaining({
      request: expect.objectContaining({ conversationId: 'conv-1' }),
    }));
    expect(invokeMock).toHaveBeenCalledWith('ai_chat_save_message', expect.objectContaining({
      request: expect.objectContaining({ id: 'a-2' }),
    }));
  });
});