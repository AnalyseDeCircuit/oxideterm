import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

const settingsStoreMock = vi.hoisted(() => ({
  state: {
    settings: {
      ai: {
        toolUse: {
          disabledTools: ['global.read_file'],
        },
      },
    },
  },
  store: {
    getState: () => settingsStoreMock.state,
  },
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  api: {},
  ragSearch: vi.fn(),
  nodeAgentStatus: vi.fn(),
  nodeGetState: vi.fn(),
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: settingsStoreMock.store,
}));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: {
    getState: () => ({}),
  },
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: {
    getState: () => ({ sessions: new Map(), tabs: [] }),
  },
}));

vi.mock('@/lib/sidebarContextProvider', () => ({
  gatherSidebarContext: vi.fn(),
  buildContextReminder: vi.fn(),
}));

vi.mock('@/lib/ai/providerRegistry', () => ({
  getProvider: vi.fn(),
  getProviderReasoningProtocol: () => 'none',
}));

vi.mock('@/lib/ai/tokenUtils', () => ({
  estimateTokens: vi.fn(),
  estimateToolDefinitionsTokens: vi.fn(),
  trimHistoryToTokenBudget: vi.fn(),
  getModelContextWindow: vi.fn(),
  responseReserve: vi.fn(),
}));

vi.mock('@/lib/ai/constants', () => ({
  DEFAULT_SYSTEM_PROMPT: 'system',
  SUGGESTIONS_INSTRUCTION: 'suggestions',
  COMPACTION_TRIGGER_THRESHOLD: 0.9,
}));

vi.mock('@/lib/ai/tools', () => ({
  CONTEXT_FREE_TOOLS: [],
  SESSION_ID_TOOLS: [],
  READ_ONLY_TOOLS: new Set(),
  getToolsForContext: vi.fn(() => []),
  isCommandDenied: vi.fn(() => false),
  hasDeniedCommands: vi.fn(() => false),
  decideToolApproval: vi.fn(() => ({ risk: 'read', autoApprove: false, requiresApproval: true, reason: 'manual' })),
  executeTool: vi.fn(),
}));

vi.mock('@/lib/ai/inputParser', () => ({
  parseUserInput: vi.fn(),
}));

vi.mock('@/lib/ai/slashCommands', () => ({
  resolveSlashCommand: vi.fn(),
  SLASH_COMMANDS: [],
}));

vi.mock('@/lib/ai/participants', () => ({
  PARTICIPANTS: [],
  resolveParticipant: vi.fn(),
  mergeParticipantTools: vi.fn(() => []),
}));

vi.mock('@/lib/ai/references', () => ({
  REFERENCES: [],
  resolveReferenceType: vi.fn(),
  resolveAllReferences: vi.fn(() => []),
}));

vi.mock('@/lib/ai/suggestionParser', () => ({
  parseSuggestions: vi.fn((content: string) => ({
    cleanContent: content,
    suggestions: [],
  })),
}));

vi.mock('@/lib/ai/intentDetector', () => ({
  detectIntent: vi.fn(),
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
import { updateToolCallStatusInConversations } from '@/store/aiChatStore.runtime';
import {
  condenseToolMessages,
  decodeAnchorContent,
  dtoToConversation,
  encodeAnchorContent,
  generateTitle,
  parseThinkingContent,
} from '@/store/aiChatStore.helpers';

describe('aiChatStore helpers', () => {
  beforeEach(() => {
    resetAiChatStoreRuntimeState();
    vi.clearAllMocks();
    settingsStoreMock.state.settings.ai.toolUse.disabledTools = ['global.read_file'];
    useAiChatStore.setState({ sessionDisabledTools: null });
  });

  it('generates compact titles from the first user message', () => {
    expect(generateTitle('  hello\nworld  ')).toBe('hello world');
    expect(generateTitle('x'.repeat(40))).toBe(`${'x'.repeat(30)}...`);
  });

  it('extracts thinking blocks while leaving the visible response content intact', () => {
    const parsed = parseThinkingContent(
      '<thinking>step one</thinking>Visible answer<thinking>step two</thinking>',
    );

    expect(parsed).toEqual({
      content: 'Visible answer',
      thinkingContent: 'step one\n\nstep two',
    });
  });

  it('updates structured-only tool approval state and keeps conversation turns in sync', () => {
    const updated = updateToolCallStatusInConversations([
      {
        id: 'conv-1',
        title: 'Conversation',
        createdAt: 1,
        updatedAt: 1,
        origin: 'sidebar',
        messages: [
          { id: 'u-1', role: 'user', content: 'run it', timestamp: 1 },
          {
            id: 'a-1',
            role: 'assistant',
            content: '',
            timestamp: 2,
            turn: {
              id: 'a-1',
              status: 'streaming',
              plainTextSummary: '',
              parts: [],
              toolRounds: [{
                id: 'round-1',
                round: 1,
                toolCalls: [{
                  id: 'tool-1',
                  name: 'local_exec',
                  argumentsText: '{"command":"pwd"}',
                  approvalState: 'pending',
                }],
              }],
            },
          },
        ],
      },
    ], 'conv-1', 'a-1', 'tool-1', 'approved');

    const conversation = updated[0];
    const assistant = conversation.messages[1];

    expect(assistant.role).toBe('assistant');
    if (assistant.role !== 'assistant') {
      throw new Error('assistant message missing');
    }

    expect(assistant.turn?.toolRounds[0]?.toolCalls[0]).toMatchObject({
      id: 'tool-1',
      approvalState: 'approved',
    });
    expect(conversation.turns?.[0]?.rounds[0]?.toolCalls[0]).toMatchObject({
      id: 'tool-1',
      approvalState: 'approved',
    });
  });

  it('round-trips compaction anchor metadata through encoded content', () => {
    const encoded = encodeAnchorContent('summary', {
      type: 'compaction-anchor',
      originalCount: 12,
      compactedAt: 123,
    });

    expect(decodeAnchorContent(encoded)).toEqual({
      content: 'summary',
      metadata: {
        type: 'compaction-anchor',
        originalCount: 12,
        compactedAt: 123,
      },
    });
  });

  it('re-hydrates persisted assistant thinking and system anchors from backend dto data', () => {
    const anchorContent = encodeAnchorContent('compacted summary', {
      type: 'compaction-anchor',
      originalCount: 4,
      compactedAt: 456,
    });

    const conversation = dtoToConversation({
      id: 'conv-1',
      title: 'Conversation',
      createdAt: 1,
      updatedAt: 2,
      sessionId: null,
      messages: [
        {
          id: 'assistant-1',
          role: 'assistant',
          content: '<thinking>internal plan</thinking>Visible answer',
          timestamp: 10,
          context: null,
        },
        {
          id: 'system-1',
          role: 'system',
          content: anchorContent,
          timestamp: 11,
          context: null,
        },
      ],
    });

    expect(conversation.messages[0]).toMatchObject({
      role: 'assistant',
      content: 'Visible answer',
      thinkingContent: 'internal plan',
    });
    expect(conversation.messages[1]).toMatchObject({
      role: 'system',
      content: 'compacted summary',
      metadata: {
        type: 'compaction-anchor',
        originalCount: 4,
        compactedAt: 456,
      },
    });
  });

  it('condenses older successful tool results but preserves recent and error outputs', () => {
    const messages = Array.from({ length: 7 }, (_, index) => ({
      role: 'tool',
      tool_name: `tool-${index}`,
      content: `line 1\nline 2\nline 3\nline 4\nline 5 ${index}`,
    }));
    messages[1].content = JSON.stringify({ error: 'boom' });

    condenseToolMessages(messages as never);

    expect(messages[0].content.startsWith('[condensed] tool-0')).toBe(true);
    expect(messages[1].content).toBe(JSON.stringify({ error: 'boom' }));
    expect(messages[6].content).toContain('line 1');
  });

  it('prefers session-level disabled tools over global settings', () => {
    expect(Array.from(useAiChatStore.getState().getEffectiveDisabledTools())).toEqual(['global.read_file']);

    useAiChatStore.getState().setSessionDisabledTools(['session.run_terminal']);

    expect(Array.from(useAiChatStore.getState().getEffectiveDisabledTools())).toEqual(['session.run_terminal']);
  });

  it('initializes by loading conversation metadata and the first conversation body', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({
        conversations: [
          {
            id: 'conv-1',
            title: 'Loaded conversation',
            createdAt: 1,
            updatedAt: 2,
            messageCount: 1,
            origin: 'sidebar',
          },
        ],
      })
      .mockResolvedValueOnce({
        id: 'conv-1',
        title: 'Loaded conversation',
        createdAt: 1,
        updatedAt: 2,
        sessionId: null,
        origin: 'sidebar',
        messages: [
          {
            id: 'msg-1',
            role: 'assistant',
            content: 'Hello from backend',
            timestamp: 3,
            context: null,
          },
        ],
      })
      .mockResolvedValueOnce({
        entries: [],
      });

    useAiChatStore.setState({
      conversations: [],
      activeConversationId: null,
      isInitialized: false,
    });

    await useAiChatStore.getState().init();

    expect(useAiChatStore.getState().isInitialized).toBe(true);
    expect(useAiChatStore.getState().activeConversationId).toBe('conv-1');
    expect(useAiChatStore.getState().conversations[0]).toMatchObject({
      id: 'conv-1',
      title: 'Loaded conversation',
      messages: [{ id: 'msg-1', content: 'Hello from backend' }],
    });
  });

  it('surfaces a database lock initialization error instead of falling back silently', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(
      'Failed to initialize AI chat store at /tmp/chat_history.redb: Database error: Database already open. Cannot acquire lock.'
    );

    useAiChatStore.setState({
      conversations: [],
      activeConversationId: null,
      isInitialized: false,
      isInitializing: false,
      initializationError: null,
      error: null,
    });

    await useAiChatStore.getState().init();

    expect(useAiChatStore.getState().isInitialized).toBe(true);
    expect(useAiChatStore.getState().conversations).toEqual([]);
    expect(useAiChatStore.getState().activeConversationId).toBeNull();
    expect(useAiChatStore.getState().error).toBeNull();
    expect(useAiChatStore.getState().initializationError).toEqual({
      messageKey: 'ai.chat.database_locked',
      canRetry: true,
    });
  });

  it('allows retrying initialization after a recoverable failure', async () => {
    vi.mocked(invoke)
      .mockRejectedValueOnce(
        'Failed to initialize AI chat store at /tmp/chat_history.redb: Database error: Database already open. Cannot acquire lock.'
      )
      .mockResolvedValueOnce({
        conversations: [],
      });

    useAiChatStore.setState({
      conversations: [],
      activeConversationId: null,
      isInitialized: false,
      isInitializing: false,
      initializationError: null,
      error: null,
    });

    await useAiChatStore.getState().init();
    useAiChatStore.getState().retryInit();

    expect(useAiChatStore.getState().isInitialized).toBe(false);
    expect(useAiChatStore.getState().isInitializing).toBe(true);
    expect(useAiChatStore.getState().initializationError).toBeNull();
    expect(useAiChatStore.getState().error).toBeNull();

    await useAiChatStore.getState().init();

    expect(useAiChatStore.getState().isInitialized).toBe(true);
    expect(useAiChatStore.getState().error).toBeNull();
    expect(useAiChatStore.getState().conversations).toEqual([]);
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('keeps the memory-only fallback for non-fatal init failures', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('transport temporarily unavailable');

    useAiChatStore.setState({
      conversations: [],
      activeConversationId: null,
      isInitialized: false,
      isInitializing: false,
      initializationError: null,
      error: null,
    });

    await useAiChatStore.getState().init();

    expect(useAiChatStore.getState().isInitialized).toBe(true);
    expect(useAiChatStore.getState().initializationError).toBeNull();
    expect(useAiChatStore.getState().error).toBeNull();
    expect(useAiChatStore.getState().conversations).toEqual([]);
  });

  it('prefers transcript-rebuilt assistant content during initialization', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({
        conversations: [
          {
            id: 'conv-1',
            title: 'Loaded conversation',
            createdAt: 1,
            updatedAt: 2,
            messageCount: 2,
            origin: 'sidebar',
          },
        ],
      })
      .mockResolvedValueOnce({
        id: 'conv-1',
        title: 'Loaded conversation',
        createdAt: 1,
        updatedAt: 2,
        sessionId: null,
        origin: 'sidebar',
        messages: [
          {
            id: 'user-1',
            role: 'user',
            content: 'Question',
            timestamp: 3,
            context: null,
          },
          {
            id: 'assistant-1',
            role: 'assistant',
            content: 'stale projection',
            timestamp: 4,
            context: null,
          },
        ],
      })
      .mockResolvedValueOnce({
        entries: [
          {
            id: 'entry-user',
            conversationId: 'conv-1',
            timestamp: 3,
            kind: 'user_message',
            payload: {
              messageId: 'user-1',
              content: 'Question',
            },
          },
          {
            id: 'entry-start',
            conversationId: 'conv-1',
            turnId: 'assistant-1',
            timestamp: 4,
            kind: 'assistant_turn_start',
            payload: {
              messageId: 'assistant-1',
            },
          },
          {
            id: 'entry-part',
            conversationId: 'conv-1',
            turnId: 'assistant-1',
            timestamp: 5,
            kind: 'assistant_part',
            payload: {
              parts: [{ type: 'text', text: 'Fresh from transcript' }],
            },
          },
          {
            id: 'entry-end',
            conversationId: 'conv-1',
            turnId: 'assistant-1',
            timestamp: 6,
            kind: 'assistant_turn_end',
            payload: {
              messageId: 'assistant-1',
              status: 'complete',
              plainTextSummary: 'Fresh from transcript',
            },
          },
        ],
      });

    useAiChatStore.setState({
      conversations: [],
      activeConversationId: null,
      isInitialized: false,
    });

    await useAiChatStore.getState().init();

    expect(useAiChatStore.getState().conversations[0].messages[1]).toMatchObject({
      id: 'assistant-1',
      content: 'Fresh from transcript',
      transcriptRef: {
        conversationId: 'conv-1',
        startEntryId: 'entry-start',
        endEntryId: 'entry-end',
      },
    });
  });

  it('replays tool and thinking order from transcript during initialization', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({
        conversations: [
          {
            id: 'conv-1',
            title: 'Loaded conversation',
            createdAt: 1,
            updatedAt: 2,
            messageCount: 2,
            origin: 'sidebar',
          },
        ],
      })
      .mockResolvedValueOnce({
        id: 'conv-1',
        title: 'Loaded conversation',
        createdAt: 1,
        updatedAt: 2,
        sessionId: null,
        origin: 'sidebar',
        messages: [
          {
            id: 'user-1',
            role: 'user',
            content: 'Question',
            timestamp: 3,
            context: null,
          },
          {
            id: 'assistant-1',
            role: 'assistant',
            content: 'stale projection',
            timestamp: 4,
            context: null,
          },
        ],
      })
      .mockResolvedValueOnce({
        entries: [
          {
            id: 'entry-user',
            conversationId: 'conv-1',
            timestamp: 3,
            kind: 'user_message',
            payload: {
              messageId: 'user-1',
              content: 'Question',
            },
          },
          {
            id: 'entry-start',
            conversationId: 'conv-1',
            turnId: 'assistant-1',
            timestamp: 4,
            kind: 'assistant_turn_start',
            payload: {
              messageId: 'assistant-1',
            },
          },
          {
            id: 'entry-round-1',
            conversationId: 'conv-1',
            turnId: 'assistant-1',
            timestamp: 5,
            kind: 'assistant_round',
            payload: {
              round: 1,
              roundId: 'round-1',
              toolCallIds: ['tool-1'],
            },
          },
          {
            id: 'entry-tool-call',
            conversationId: 'conv-1',
            turnId: 'assistant-1',
            timestamp: 6,
            kind: 'tool_call',
            payload: {
              id: 'tool-1',
              name: 'local_exec',
              argumentsText: '{"command":"pwd"}',
              roundId: 'round-1',
            },
          },
          {
            id: 'entry-tool-result',
            conversationId: 'conv-1',
            turnId: 'assistant-1',
            timestamp: 7,
            kind: 'tool_result',
            payload: {
              toolCallId: 'tool-1',
              toolName: 'local_exec',
              success: true,
              output: '/tmp',
              roundId: 'round-1',
            },
          },
          {
            id: 'entry-part',
            conversationId: 'conv-1',
            turnId: 'assistant-1',
            timestamp: 8,
            kind: 'assistant_part',
            payload: {
              parts: [
                { type: 'thinking', text: 'Use the tool result.' },
                { type: 'text', text: 'Current directory is /tmp.' },
              ],
            },
          },
          {
            id: 'entry-end',
            conversationId: 'conv-1',
            turnId: 'assistant-1',
            timestamp: 9,
            kind: 'assistant_turn_end',
            payload: {
              messageId: 'assistant-1',
              status: 'complete',
              plainTextSummary: 'Current directory is /tmp.',
            },
          },
        ],
      });

    useAiChatStore.setState({
      conversations: [],
      activeConversationId: null,
      isInitialized: false,
    });

    await useAiChatStore.getState().init();

    const assistantMessage = useAiChatStore.getState().conversations[0].messages[1];
    expect(assistantMessage).toMatchObject({
      id: 'assistant-1',
      content: 'Current directory is /tmp.',
      transcriptRef: {
        conversationId: 'conv-1',
        startEntryId: 'entry-start',
        endEntryId: 'entry-end',
      },
    });
    expect(assistantMessage.turn?.parts.map((part) => part.type)).toEqual([
      'tool_call',
      'tool_result',
      'thinking',
      'text',
    ]);
    expect(assistantMessage.turn?.parts[2]).toMatchObject({
      type: 'thinking',
      text: 'Use the tool result.',
    });
    expect(assistantMessage.turn?.parts[3]).toMatchObject({
      type: 'text',
      text: 'Current directory is /tmp.',
    });
    expect(assistantMessage.turn?.toolRounds[0]).toMatchObject({
      id: 'round-1',
      toolCalls: [expect.objectContaining({ id: 'tool-1', executionState: 'completed' })],
    });
  });
});
