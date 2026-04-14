import { describe, expect, it } from 'vitest';

import { computePromptBudget, determineCompressionLevel } from '@/lib/ai/promptBudget/policy';

describe('promptBudgetPolicy', () => {
  it('computes prompt budget with a safety margin', () => {
    const budget = computePromptBudget({
      contextWindow: 10000,
      responseReserve: 2000,
      systemBudget: 1000,
      safetyMargin: 500,
    });

    expect(budget).toEqual({
      contextWindow: 10000,
      responseReserve: 2000,
      safetyMargin: 500,
      systemBudget: 1000,
      usablePromptBudget: 7500,
      historyBudget: 6500,
    });
  });

  it('prefers level 1 when trimmable history can absorb the overage', () => {
    const decision = determineCompressionLevel({
      contextWindow: 10000,
      responseReserve: 2000,
      systemBudget: 1000,
      historyTokens: 7000,
      safetyMargin: 500,
      trimmableHistoryTokens: 1200,
      canSummarize: true,
      canLookupTranscript: true,
    });

    expect(decision.level).toBe(1);
    expect(decision.overage).toBe(500);
  });

  it('falls back to summary compaction when trimming is insufficient', () => {
    const decision = determineCompressionLevel({
      contextWindow: 10000,
      responseReserve: 2000,
      systemBudget: 1000,
      historyTokens: 7200,
      safetyMargin: 500,
      trimmableHistoryTokens: 100,
      summaryEligibleTokens: 5000,
      canSummarize: true,
    });

    expect(decision.level).toBe(2);
  });

  it('proactively recommends compaction before the prompt is over budget', () => {
    const decision = determineCompressionLevel({
      contextWindow: 10000,
      responseReserve: 2000,
      systemBudget: 1000,
      historyTokens: 6000,
      safetyMargin: 500,
      summaryEligibleTokens: 3000,
      canSummarize: true,
    });

    expect(decision.overage).toBe(0);
    expect(decision.usageRatio).toBeCloseTo(7000 / 7500);
    expect(decision.level).toBe(2);
  });

  it('escalates to transcript lookup near the higher threshold even without overage', () => {
    const decision = determineCompressionLevel({
      contextWindow: 10000,
      responseReserve: 2000,
      systemBudget: 1000,
      historyTokens: 6200,
      safetyMargin: 500,
      summaryEligibleTokens: 3000,
      canSummarize: true,
      canLookupTranscript: true,
    });

    expect(decision.overage).toBe(0);
    expect(decision.usageRatio).toBeCloseTo(7200 / 7500);
    expect(decision.level).toBe(3);
  });

  it('uses tool-loop guardrail as the last fallback when no better compression path exists', () => {
    const decision = determineCompressionLevel({
      contextWindow: 10000,
      responseReserve: 2000,
      systemBudget: 1000,
      historyTokens: 7600,
      safetyMargin: 500,
      trimmableHistoryTokens: 0,
      summaryEligibleTokens: 0,
      canSummarize: false,
      canLookupTranscript: false,
      inToolLoop: true,
    });

    expect(decision.level).toBe(4);
  });
});