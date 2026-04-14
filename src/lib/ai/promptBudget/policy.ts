// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { COMPACTION_TRIGGER_THRESHOLD } from '../constants';

export type AiCompressionLevel = 0 | 1 | 2 | 3 | 4;

export interface AiPromptBudget {
  contextWindow: number;
  responseReserve: number;
  safetyMargin: number;
  systemBudget: number;
  usablePromptBudget: number;
  historyBudget: number;
}

export interface AiPromptBudgetInput {
  contextWindow: number;
  responseReserve: number;
  systemBudget: number;
  historyTokens: number;
  safetyMargin?: number;
  trimmableHistoryTokens?: number;
  summaryEligibleTokens?: number;
  canSummarize?: boolean;
  canLookupTranscript?: boolean;
  inToolLoop?: boolean;
  autoCompactThreshold?: number;
  transcriptLookupThreshold?: number;
  toolLoopStopThreshold?: number;
}

export interface AiPromptBudgetDecision {
  level: AiCompressionLevel;
  promptBudget: AiPromptBudget;
  totalPromptTokens: number;
  usageRatio: number;
  overage: number;
}

const DEFAULT_TRANSCRIPT_LOOKUP_THRESHOLD = 0.92;
const DEFAULT_TOOL_LOOP_STOP_THRESHOLD = 0.98;
const DEFAULT_MIN_SAFETY_MARGIN = 128;

export function computePromptBudget(input: Pick<AiPromptBudgetInput, 'contextWindow' | 'responseReserve' | 'systemBudget' | 'safetyMargin'>): AiPromptBudget {
  const safetyMargin = input.safetyMargin ?? Math.max(DEFAULT_MIN_SAFETY_MARGIN, Math.floor(input.contextWindow * 0.02));
  const usablePromptBudget = Math.max(0, input.contextWindow - input.responseReserve - safetyMargin);
  const historyBudget = Math.max(0, usablePromptBudget - input.systemBudget);

  return {
    contextWindow: input.contextWindow,
    responseReserve: input.responseReserve,
    safetyMargin,
    systemBudget: input.systemBudget,
    usablePromptBudget,
    historyBudget,
  };
}

export function determineCompressionLevel(input: AiPromptBudgetInput): AiPromptBudgetDecision {
  const promptBudget = computePromptBudget(input);
  const totalPromptTokens = input.systemBudget + input.historyTokens;
  const overage = Math.max(0, totalPromptTokens - promptBudget.usablePromptBudget);
  const usageRatio = promptBudget.usablePromptBudget > 0
    ? totalPromptTokens / promptBudget.usablePromptBudget
    : Number.POSITIVE_INFINITY;

  const trimmableHistoryTokens = input.trimmableHistoryTokens ?? input.historyTokens;
  const summaryEligibleTokens = input.summaryEligibleTokens ?? input.historyTokens;
  const autoCompactThreshold = input.autoCompactThreshold ?? COMPACTION_TRIGGER_THRESHOLD;
  const transcriptLookupThreshold = input.transcriptLookupThreshold ?? DEFAULT_TRANSCRIPT_LOOKUP_THRESHOLD;
  const toolLoopStopThreshold = input.toolLoopStopThreshold ?? DEFAULT_TOOL_LOOP_STOP_THRESHOLD;

  let level: AiCompressionLevel = 0;

  if (overage <= 0) {
    if (input.inToolLoop && usageRatio >= toolLoopStopThreshold) {
      level = 4;
    } else if (input.canLookupTranscript && usageRatio >= transcriptLookupThreshold) {
      level = 3;
    } else if (input.canSummarize && summaryEligibleTokens > 0 && usageRatio >= autoCompactThreshold) {
      level = 2;
    }
  } else if (trimmableHistoryTokens >= overage && trimmableHistoryTokens > 0) {
    level = 1;
  } else if (input.canSummarize && summaryEligibleTokens > 0 && usageRatio >= autoCompactThreshold) {
    level = 2;
  } else if (input.canLookupTranscript && usageRatio >= transcriptLookupThreshold) {
    level = 3;
  } else if (input.inToolLoop && usageRatio >= toolLoopStopThreshold) {
    level = 4;
  } else if (input.canLookupTranscript) {
    level = 3;
  } else if (input.canSummarize && summaryEligibleTokens > 0) {
    level = 2;
  } else if (input.inToolLoop) {
    level = 4;
  } else {
    level = 1;
  }

  return {
    level,
    promptBudget,
    totalPromptTokens,
    usageRatio,
    overage,
  };
}