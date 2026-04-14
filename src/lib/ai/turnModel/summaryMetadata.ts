// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiPendingSummary, AiToolRound, AiTurnSummaryMetadata } from './types';

export interface SummaryNormalizationResult {
  rounds: AiToolRound[];
  unresolved: AiPendingSummary[];
}

function mergeSummary(round: AiToolRound, summary: AiPendingSummary): AiToolRound {
  return {
    ...round,
    summary: summary.text,
    summaryMetadata: summary.metadata ?? round.summaryMetadata,
  };
}

export function normalizePendingSummaries(
  rounds: readonly AiToolRound[],
  pendingSummaries: readonly AiPendingSummary[],
): SummaryNormalizationResult {
  const latestSummaryByRoundId = new Map<string, AiPendingSummary>();
  for (const summary of pendingSummaries) {
    latestSummaryByRoundId.set(summary.roundId, summary);
  }

  const seen = new Set<string>();
  const normalizedRounds = rounds.map((round) => {
    const pending = latestSummaryByRoundId.get(round.id);
    if (!pending) {
      return { ...round };
    }

    seen.add(round.id);
    return mergeSummary(round, pending);
  });

  const unresolved = pendingSummaries.filter((summary) => !seen.has(summary.roundId));

  return {
    rounds: normalizedRounds,
    unresolved,
  };
}

export function createSummaryMetadata(
  text: string,
  metadata?: AiTurnSummaryMetadata,
): { summary: string; metadata?: AiTurnSummaryMetadata } {
  return {
    summary: text,
    metadata,
  };
}