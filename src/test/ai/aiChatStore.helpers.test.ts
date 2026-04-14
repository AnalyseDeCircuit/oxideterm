import { describe, expect, it } from 'vitest';

import { hydrateStructuredConversation } from '@/store/aiChatStore.helpers';
import type { AiConversation } from '@/types';

describe('hydrateStructuredConversation', () => {
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
        rounds: [expect.objectContaining({ id: 'round-1', retryCount: 2, summary: 'existing summary' })],
        pendingSummaries: [{ roundId: 'round-1', text: 'pending' }],
      }),
    ]);
    expect(hydrated.messages[1].turn).toEqual(expect.objectContaining({
      id: 'assistant-1',
      status: 'complete',
    }));
    expect(hydrated.messages[1].transcriptRef).toEqual({
      conversationId: 'conv-1',
      startEntryId: 'user-1',
      endEntryId: 'assistant-1',
    });
  });
});