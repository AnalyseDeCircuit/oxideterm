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
  getToolsForContext: vi.fn(() => []),
  isCommandDenied: vi.fn(() => false),
  hasDeniedCommands: vi.fn(() => false),
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
});