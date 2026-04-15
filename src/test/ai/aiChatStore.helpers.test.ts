import { describe, expect, it } from 'vitest';

import { dtoToConversation, encodeAnchorContent, hydrateStructuredConversation, rebuildConversationFromTranscript } from '@/store/aiChatStore.helpers';
import type { AiConversation } from '@/types';

describe('hydrateStructuredConversation', () => {
  it('prefers turn-derived projection over stale persisted legacy assistant fields', () => {
    const conversation = dtoToConversation({
      id: 'conv-1',
      title: 'Conversation',
      createdAt: 1,
      updatedAt: 2,
      sessionId: null,
      origin: 'sidebar',
      messages: [
        {
          id: 'user-1',
          role: 'user',
          content: 'hello',
          timestamp: 1,
          context: null,
        },
        {
          id: 'assistant-1',
          role: 'assistant',
          content: 'stale projection',
          timestamp: 2,
          context: null,
          toolCalls: [{
            id: 'tool-stale',
            name: 'local_exec',
            arguments: '{"command":"pwd"}',
            status: 'completed',
            result: {
              toolCallId: 'tool-stale',
              toolName: 'local_exec',
              success: true,
              output: 'stale',
            },
          }],
          turn: {
            id: 'assistant-1',
            status: 'complete',
            plainTextSummary: 'fresh projection',
            parts: [
              { type: 'thinking', text: 'inspect state' },
              { type: 'text', text: 'fresh projection' },
              { type: 'tool_call', id: 'tool-fresh', name: 'list_sessions', argumentsText: '{}', status: 'complete' },
              { type: 'tool_result', toolCallId: 'tool-fresh', toolName: 'list_sessions', success: true, output: '[]' },
            ],
            toolRounds: [{
              id: 'round-1',
              round: 1,
              toolCalls: [{
                id: 'tool-fresh',
                name: 'list_sessions',
                argumentsText: '{}',
                executionState: 'completed',
              }],
            }],
          },
        },
      ],
    });

    expect(conversation.messages[1]).toMatchObject({
      content: 'fresh projection',
      thinkingContent: 'inspect state',
      toolCalls: [expect.objectContaining({ id: 'tool-fresh', name: 'list_sessions' })],
    });
  });

  it('preserves existing turn identity and pending summaries while rebuilding assistant projections', () => {
    const conversation: AiConversation = {
      id: 'conv-1',
      title: 'Conversation',
      createdAt: 1,
      updatedAt: 1,
      origin: 'sidebar',
      sessionMetadata: {
        conversationId: 'conv-1',
        firstUserMessage: 'hello',
      },
      messages: [
        { id: 'user-1', role: 'user', content: 'hello', timestamp: 1 },
        {
          id: 'assistant-1',
          role: 'assistant',
          content: 'world',
          thinkingContent: 'thinking',
          timestamp: 2,
        },
      ],
      turns: [
        {
          id: 'turn-existing',
          requestMessageId: 'user-1',
          requestText: 'hello',
          startedAt: 1,
          status: 'streaming',
          rounds: [
            {
              id: 'round-1',
              round: 1,
              retryCount: 2,
              summary: 'existing summary',
              toolCalls: [],
            },
          ],
          pendingSummaries: [{ roundId: 'round-1', text: 'pending' }],
        },
      ],
    };

    const hydrated = hydrateStructuredConversation(conversation);

    expect(hydrated.turns).toEqual([
      expect.objectContaining({
        id: 'turn-existing',
        requestMessageId: 'user-1',
        rounds: [expect.objectContaining({ id: 'round-1', retryCount: 2, summary: 'pending', summaryMetadata: undefined })],
        pendingSummaries: [],
      }),
    ]);
    expect(hydrated.messages[1].turn).toEqual(expect.objectContaining({
      id: 'assistant-1',
      status: 'complete',
      toolRounds: [expect.objectContaining({ id: 'round-1', summary: 'pending' })],
    }));
    expect(hydrated.messages[1].transcriptRef).toEqual({
      conversationId: 'conv-1',
      startEntryId: 'user-1',
      endEntryId: 'assistant-1',
    });
  });

  it('rebuilds stale assistant projection fields from transcript entries', () => {
    const conversation = dtoToConversation({
      id: 'conv-1',
      title: 'Conversation',
      createdAt: 1,
      updatedAt: 2,
      sessionId: null,
      origin: 'sidebar',
      messages: [
        {
          id: 'user-1',
          role: 'user',
          content: 'How do I fix this?',
          timestamp: 10,
          context: null,
        },
        {
          id: 'assistant-1',
          role: 'assistant',
          content: 'stale projection',
          timestamp: 11,
          context: null,
        },
      ],
    });

    const rebuilt = rebuildConversationFromTranscript(conversation, [
      {
        id: 'entry-user',
        conversationId: 'conv-1',
        timestamp: 10,
        kind: 'user_message',
        payload: {
          messageId: 'user-1',
          content: 'How do I fix this?',
        },
      },
      {
        id: 'entry-start',
        conversationId: 'conv-1',
        turnId: 'assistant-1',
        timestamp: 11,
        kind: 'assistant_turn_start',
        payload: {
          messageId: 'assistant-1',
        },
      },
      {
        id: 'entry-part',
        conversationId: 'conv-1',
        turnId: 'assistant-1',
        timestamp: 12,
        kind: 'assistant_part',
        payload: {
          parts: [
            { type: 'thinking', text: 'inspect logs' },
            { type: 'text', text: 'Fresh answer' },
          ],
        },
      },
      {
        id: 'entry-end',
        conversationId: 'conv-1',
        turnId: 'assistant-1',
        timestamp: 13,
        kind: 'assistant_turn_end',
        payload: {
          messageId: 'assistant-1',
          status: 'complete',
          plainTextSummary: 'Fresh answer',
        },
      },
    ]);

    expect(rebuilt.messages[1]).toMatchObject({
      id: 'assistant-1',
      content: 'Fresh answer',
      thinkingContent: 'inspect logs',
      transcriptRef: {
        conversationId: 'conv-1',
        startEntryId: 'entry-start',
        endEntryId: 'entry-end',
      },
    });
  });

  it('preserves transcriptRef and summaryRef when decoding persisted compaction anchors', () => {
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
      origin: 'sidebar',
      messages: [
        {
          id: 'system-1',
          role: 'system',
          content: anchorContent,
          timestamp: 11,
          context: null,
          transcriptRef: {
            conversationId: 'conv-1',
            endEntryId: 'entry-summary',
          },
          summaryRef: {
            kind: 'compaction',
            transcriptRef: {
              conversationId: 'conv-1',
              endEntryId: 'entry-summary',
            },
          },
        },
      ],
    });

    expect(conversation.messages[0]).toMatchObject({
      role: 'system',
      content: 'compacted summary',
      transcriptRef: {
        conversationId: 'conv-1',
        endEntryId: 'entry-summary',
      },
      summaryRef: {
        kind: 'compaction',
      },
    });
  });

  it('reconstructs a compacted projection from transcript when persisted messages are missing', () => {
    const rebuilt = rebuildConversationFromTranscript(dtoToConversation({
      id: 'conv-1',
      title: 'Conversation',
      createdAt: 1,
      updatedAt: 2,
      sessionId: null,
      origin: 'sidebar',
      messages: [],
    }), [
      {
        id: 'entry-user-old',
        conversationId: 'conv-1',
        timestamp: 10,
        kind: 'user_message',
        payload: {
          messageId: 'user-old',
          content: 'Old question',
        },
      },
      {
        id: 'entry-summary',
        conversationId: 'conv-1',
        timestamp: 20,
        kind: 'summary_created',
        payload: {
          messageId: 'anchor-1',
          summaryText: 'Condensed history',
          compactedMessageCount: 2,
          sourceStartEntryId: 'entry-user-old',
          sourceEndEntryId: 'assistant-old',
          compactedUntilMessageId: 'assistant-old',
        },
      },
      {
        id: 'entry-user-new',
        conversationId: 'conv-1',
        timestamp: 30,
        kind: 'user_message',
        payload: {
          messageId: 'user-new',
          content: 'Latest question',
        },
      },
    ]);

    expect(rebuilt.messages).toHaveLength(2);
    expect(rebuilt.messages[0]).toMatchObject({
      id: 'anchor-1',
      role: 'system',
      content: 'Condensed history',
      transcriptRef: {
        conversationId: 'conv-1',
        endEntryId: 'entry-summary',
      },
      summaryRef: {
        kind: 'compaction',
        transcriptRef: {
          conversationId: 'conv-1',
          startEntryId: 'entry-user-old',
          endEntryId: 'assistant-old',
        },
      },
      metadata: {
        type: 'compaction-anchor',
        originalCount: 2,
      },
    });
    expect(rebuilt.messages[1]).toMatchObject({
      id: 'user-new',
      role: 'user',
      content: 'Latest question',
    });
  });

  it('does not reinsert compacted transcript-only messages behind an existing anchor projection', () => {
    const conversation = dtoToConversation({
      id: 'conv-1',
      title: 'Conversation',
      createdAt: 1,
      updatedAt: 2,
      sessionId: null,
      origin: 'sidebar',
      messages: [
        {
          id: 'anchor-1',
          role: 'system',
          content: encodeAnchorContent('Condensed history', {
            type: 'compaction-anchor',
            originalCount: 2,
            compactedAt: 20,
          }),
          timestamp: 20,
          context: null,
        },
      ],
    });

    const rebuilt = rebuildConversationFromTranscript(conversation, [
      {
        id: 'entry-user-old',
        conversationId: 'conv-1',
        timestamp: 10,
        kind: 'user_message',
        payload: {
          messageId: 'user-old',
          content: 'Old question',
        },
      },
      {
        id: 'entry-summary',
        conversationId: 'conv-1',
        timestamp: 20,
        kind: 'summary_created',
        payload: {
          messageId: 'anchor-1',
          summaryText: 'Condensed history',
          compactedMessageCount: 2,
          sourceStartEntryId: 'entry-user-old',
          sourceEndEntryId: 'assistant-old',
          compactedUntilMessageId: 'assistant-old',
        },
      },
      {
        id: 'entry-user-new',
        conversationId: 'conv-1',
        timestamp: 30,
        kind: 'user_message',
        payload: {
          messageId: 'user-new',
          content: 'Latest question',
        },
      },
    ]);

    expect(rebuilt.messages.map((message) => message.id)).toEqual(['anchor-1', 'user-new']);
  });

  it('restores the latest anchor from transcript when the persisted projection lost it', () => {
    const conversation = dtoToConversation({
      id: 'conv-1',
      title: 'Conversation',
      createdAt: 1,
      updatedAt: 2,
      sessionId: null,
      origin: 'sidebar',
      messages: [
        {
          id: 'user-new',
          role: 'user',
          content: 'Latest question',
          timestamp: 30,
          context: null,
        },
      ],
    });

    const rebuilt = rebuildConversationFromTranscript(conversation, [
      {
        id: 'entry-user-old',
        conversationId: 'conv-1',
        timestamp: 10,
        kind: 'user_message',
        payload: {
          messageId: 'user-old',
          content: 'Old question',
        },
      },
      {
        id: 'entry-summary',
        conversationId: 'conv-1',
        timestamp: 20,
        kind: 'summary_created',
        payload: {
          messageId: 'anchor-1',
          summaryText: 'Condensed history',
          compactedMessageCount: 2,
          sourceStartEntryId: 'entry-user-old',
          sourceEndEntryId: 'assistant-old',
          compactedUntilMessageId: 'assistant-old',
        },
      },
      {
        id: 'entry-user-new',
        conversationId: 'conv-1',
        timestamp: 30,
        kind: 'user_message',
        payload: {
          messageId: 'user-new',
          content: 'Latest question',
        },
      },
    ]);

    expect(rebuilt.messages.map((message) => message.id)).toEqual(['anchor-1', 'user-new']);
    expect(rebuilt.messages[0]).toMatchObject({
      role: 'system',
      content: 'Condensed history',
      summaryRef: {
        kind: 'compaction',
        transcriptRef: {
          conversationId: 'conv-1',
          startEntryId: 'entry-user-old',
          endEntryId: 'assistant-old',
        },
      },
    });
  });

  it('reattaches round summary messages via summaryRef during hydration', () => {
    const conversation: AiConversation = {
      id: 'conv-1',
      title: 'Conversation',
      createdAt: 1,
      updatedAt: 1,
      origin: 'sidebar',
      sessionMetadata: {
        conversationId: 'conv-1',
      },
      messages: [
        { id: 'user-1', role: 'user', content: 'hello', timestamp: 1 },
        {
          id: 'assistant-1',
          role: 'assistant',
          content: 'world',
          timestamp: 2,
          turn: {
            id: 'assistant-1',
            status: 'complete',
            parts: [{ type: 'text', text: 'world' }],
            plainTextSummary: 'world',
            toolRounds: [
              {
                id: 'round-1',
                round: 1,
                toolCalls: [],
              },
            ],
          },
        },
        {
          id: 'summary-1',
          role: 'assistant',
          content: 'round one summary',
          timestamp: 3,
          summaryRef: {
            kind: 'round',
            roundId: 'round-1',
          },
        },
      ],
      turns: [
        {
          id: 'turn-existing',
          requestMessageId: 'user-1',
          requestText: 'hello',
          startedAt: 1,
          status: 'complete',
          rounds: [
            {
              id: 'round-1',
              round: 1,
              toolCalls: [],
            },
          ],
          pendingSummaries: [],
        },
      ],
    };

    const hydrated = hydrateStructuredConversation(conversation);

    expect(hydrated.turns?.[0].rounds[0]).toMatchObject({
      id: 'round-1',
      summary: 'round one summary',
    });
  });
});