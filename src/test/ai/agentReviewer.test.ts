import { describe, expect, it } from 'vitest';

import { formatReviewFeedback, shouldRunReviewerForRound } from '@/lib/ai/agentReviewer';

describe('agentReviewer helpers', () => {
  it('runs reviewer on the first round when interval is 1', () => {
    expect(shouldRunReviewerForRound(0, 1)).toBe(true);
    expect(shouldRunReviewerForRound(1, 1)).toBe(true);
  });

  it('runs reviewer on matching intervals only', () => {
    expect(shouldRunReviewerForRound(0, 2)).toBe(false);
    expect(shouldRunReviewerForRound(1, 2)).toBe(true);
    expect(shouldRunReviewerForRound(2, 2)).toBe(false);
    expect(shouldRunReviewerForRound(3, 2)).toBe(true);
  });

  it('formats feedback even when findings exist without suggestions', () => {
    expect(formatReviewFeedback({
      assessment: 'needs_correction',
      findings: 'The verification step was skipped.',
      suggestions: [],
    }, 2)).toBe('[Review feedback after round 3]: The verification step was skipped.');
  });

  it('returns null for on-track reviews', () => {
    expect(formatReviewFeedback({
      assessment: 'on_track',
      findings: 'All good.',
      suggestions: ['Keep going'],
    }, 0)).toBeNull();
  });
});