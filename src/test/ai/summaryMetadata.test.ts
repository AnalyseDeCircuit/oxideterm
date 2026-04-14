import { describe, expect, it } from 'vitest';

import { normalizePendingSummaries } from '@/lib/ai/turnModel/summaryMetadata';
import type { AiPendingSummary, AiToolRound } from '@/lib/ai/turnModel/types';

describe('normalizePendingSummaries', () => {
  it('attaches the latest pending summary to the matching round and preserves unresolved entries', () => {
    const rounds: AiToolRound[] = [
      { id: 'round-1', round: 1, toolCalls: [] },
      { id: 'round-2', round: 2, toolCalls: [] },
    ];
    const pending: AiPendingSummary[] = [
      { roundId: 'round-1', text: 'old summary' },
      { roundId: 'round-1', text: 'new summary', metadata: { summarizationMode: 'inline' } },
      { roundId: 'missing-round', text: 'unresolved' },
    ];

    const normalized = normalizePendingSummaries(rounds, pending);

    expect(normalized.rounds[0].summary).toBe('new summary');
    expect(normalized.rounds[0].summaryMetadata).toEqual({ summarizationMode: 'inline' });
    expect(normalized.unresolved).toEqual([{ roundId: 'missing-round', text: 'unresolved' }]);
  });
});