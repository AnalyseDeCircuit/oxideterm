// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { ToolResultEnvelope } from '../tools/protocol/types';

export type AiConversationTurnStatus = 'streaming' | 'complete' | 'error';

export type AiSummarySource = 'foreground' | 'background';

export type AiSummarizationMode = 'inline' | 'background' | 'manual';

export interface AiTurnSummaryUsage {
  promptTokens?: number;
  completionTokens?: number;
  cachedPromptTokens?: number;
}

export interface AiTurnSummaryMetadata {
  source?: AiSummarySource;
  model?: string;
  summarizationMode?: AiSummarizationMode;
  durationMs?: number;
  contextLengthBefore?: number;
  numRounds?: number;
  numRoundsSinceLastSummarization?: number;
  usage?: AiTurnSummaryUsage;
}

export interface AiPendingSummary {
  roundId: string;
  text: string;
  metadata?: AiTurnSummaryMetadata;
}

export interface AiTurnToolCall {
  id: string;
  name: string;
  argumentsText: string;
  approvalState?: 'pending' | 'approved' | 'rejected';
  executionState?: 'pending' | 'running' | 'completed' | 'error';
}

export interface AiToolRound {
  id: string;
  round: number;
  responseText?: string;
  retryCount?: number;
  timestamp?: number;
  statefulMarker?: string;
  summary?: string;
  summaryMetadata?: AiTurnSummaryMetadata;
  toolCalls: AiTurnToolCall[];
}

export type AiGuardrailCode =
  | 'pseudo-tool-transcript'
  | 'tool-use-disabled'
  | 'tool-context-missing'
  | 'tool-disabled-hard-deny'
  | 'tool-required-no-call'
  | 'tool-budget-limit';

export type AiTurnPart =
  | { type: 'text'; text: string }
  | { type: 'thinking'; text: string; streaming?: boolean }
  | { type: 'tool_call'; id: string; name: string; argumentsText: string; status: 'partial' | 'complete' }
  | { type: 'tool_result'; toolCallId: string; toolName: string; success: boolean; output: string; error?: string; durationMs?: number; truncated?: boolean; envelope?: ToolResultEnvelope }
  | { type: 'guardrail'; code: AiGuardrailCode; message: string; rawText?: string }
  | { type: 'warning'; code: string; message: string }
  | { type: 'error'; message: string };

export interface AiAssistantTurn {
  id: string;
  status: AiConversationTurnStatus;
  parts: AiTurnPart[];
  toolRounds: AiToolRound[];
  plainTextSummary: string;
}

export interface AiConversationTurn {
  id: string;
  requestMessageId: string;
  requestText: string;
  startedAt: number;
  status: AiConversationTurnStatus;
  rounds: AiToolRound[];
  pendingSummaries?: AiPendingSummary[];
}

export interface AiTranscriptReference {
  conversationId: string;
  startEntryId?: string;
  endEntryId?: string;
}

export interface AiSummaryReference {
  kind?: 'round' | 'conversation' | 'compaction';
  roundId?: string;
  transcriptRef?: AiTranscriptReference;
}

export interface AiConversationSessionMetadata {
  conversationId: string;
  firstUserMessage?: string;
  origin?: string;
  providerId?: string;
  providerModel?: string;
  profileId?: string;
  activeParticipant?: string;
  affectedSessionIds?: string[];
  affectedNodeIds?: string[];
  affectedTabIds?: string[];
  lastSummaryRoundId?: string;
  lastSummaryAt?: number;
  lastCompactedUntilEntryId?: string;
  lastBudgetLevel?: 0 | 1 | 2 | 3 | 4;
}

export type AiTranscriptEntryKind =
  | 'user_message'
  | 'assistant_turn_start'
  | 'assistant_part'
  | 'assistant_round'
  | 'tool_call'
  | 'tool_result'
  | 'guardrail'
  | 'assistant_turn_end'
  | 'summary_created';

export interface AiTranscriptEntry {
  id: string;
  conversationId: string;
  turnId?: string;
  parentId?: string | null;
  timestamp: number;
  kind: AiTranscriptEntryKind;
  payload: Record<string, unknown>;
}

export type AiDiagnosticEventType =
  | 'user_message'
  | 'llm_request'
  | 'assistant_round'
  | 'tool_call'
  | 'tool_result'
  | 'guardrail'
  | 'budget_level_changed'
  | 'compaction_started'
  | 'compaction_completed'
  | 'error';

export interface AiDiagnosticEvent {
  id: string;
  conversationId: string;
  turnId?: string;
  roundId?: string;
  timestamp: number;
  type: AiDiagnosticEventType;
  data: Record<string, unknown>;
}
