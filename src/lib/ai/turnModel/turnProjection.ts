// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiChatMessage, AiToolCall, AiToolResult } from '../../../types';
import type { AiAssistantTurn, AiTurnPart, AiTurnToolCall } from './types';

export type LegacyProjectedMessageFields = Pick<AiChatMessage, 'content' | 'thinkingContent' | 'toolCalls'>;

function isPartType<TType extends AiTurnPart['type']>(part: AiTurnPart, type: TType): part is Extract<AiTurnPart, { type: TType }> {
  return part.type === type;
}

export function getTurnTextContent(turn: AiAssistantTurn): string {
  return turn.parts
    .filter((part): part is Extract<AiTurnPart, { type: 'text' }> => isPartType(part, 'text'))
    .map((part) => part.text)
    .join('');
}

export function getTurnThinkingContent(turn: AiAssistantTurn): string | undefined {
  const content = turn.parts
    .filter((part): part is Extract<AiTurnPart, { type: 'thinking' }> => isPartType(part, 'thinking'))
    .map((part) => part.text)
    .join('');

  return content || undefined;
}

function mapToolStatus(toolCall: AiTurnToolCall): AiToolCall['status'] {
  if (toolCall.executionState === 'completed') return 'completed';
  if (toolCall.executionState === 'error') return 'error';
  if (toolCall.executionState === 'running') return 'running';
  if (toolCall.approvalState === 'rejected') return 'rejected';
  if (toolCall.approvalState === 'approved') return 'approved';
  if (toolCall.approvalState === 'pending') return 'pending_user_approval';
  return 'pending';
}

function mapStreamingToolCallStatus(part: Extract<AiTurnPart, { type: 'tool_call' }>): AiToolCall['status'] {
  return part.status === 'partial' ? 'pending' : 'running';
}

function collectToolResults(turn: AiAssistantTurn): Map<string, AiToolResult> {
  const results = new Map<string, AiToolResult>();

  for (const part of turn.parts) {
    if (!isPartType(part, 'tool_result')) {
      continue;
    }

    results.set(part.toolCallId, {
      toolCallId: part.toolCallId,
      toolName: part.toolName,
      success: part.success,
      output: part.output,
      error: part.error,
      durationMs: part.durationMs,
      truncated: part.truncated,
    });
  }

  return results;
}

function flattenToolCalls(turn: AiAssistantTurn, results: Map<string, AiToolResult>): AiToolCall[] | undefined {
  const flattened = turn.toolRounds.flatMap((round) =>
    round.toolCalls.map((toolCall) => ({
      id: toolCall.id,
      name: toolCall.name,
      arguments: toolCall.argumentsText,
      status: mapToolStatus(toolCall),
      result: results.get(toolCall.id),
    })),
  );

  const seenToolCallIds = new Set(flattened.map((toolCall) => toolCall.id));

  for (const part of turn.parts) {
    if (!isPartType(part, 'tool_call') || seenToolCallIds.has(part.id)) {
      continue;
    }

    const result = results.get(part.id);

    flattened.push({
      id: part.id,
      name: part.name,
      arguments: part.argumentsText,
      status: result ? (result.success ? 'completed' : 'error') : mapStreamingToolCallStatus(part),
      result,
    });
  }

  return flattened.length > 0 ? flattened : undefined;
}

function getFallbackContent(turn: AiAssistantTurn): string {
  return turn.parts
    .filter((part): part is Extract<AiTurnPart, { type: 'guardrail' | 'warning' | 'error' }> => (
      isPartType(part, 'guardrail') || isPartType(part, 'warning') || isPartType(part, 'error')
    ))
    .map((part) => part.message)
    .join('\n\n');
}

export function projectTurnToLegacyMessageFields(turn: AiAssistantTurn): LegacyProjectedMessageFields {
  const textContent = getTurnTextContent(turn);
  const content = textContent || getFallbackContent(turn);
  const thinkingContent = getTurnThinkingContent(turn);
  const toolCalls = flattenToolCalls(turn, collectToolResults(turn));

  return {
    content,
    thinkingContent,
    toolCalls,
  };
}