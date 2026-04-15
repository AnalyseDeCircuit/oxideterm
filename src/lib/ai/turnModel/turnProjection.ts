// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiChatMessage, AiToolCall, AiToolResult } from '../../../types';
import type { AiAssistantTurn, AiToolRound, AiTurnPart, AiTurnToolCall } from './types';

export type LegacyProjectedMessageFields = Pick<AiChatMessage, 'content' | 'thinkingContent' | 'toolCalls'>;
export type AiAssistantDisplaySegment =
  | { kind: 'text'; text: string }
  | { kind: 'thinking'; text: string; streaming?: boolean }
  | { kind: 'guardrail'; part: Extract<AiTurnPart, { type: 'guardrail' }> }
  | { kind: 'warning'; part: Extract<AiTurnPart, { type: 'warning' | 'error' }> }
  | {
      kind: 'tool';
      toolParts: Array<Extract<AiTurnPart, { type: 'tool_call' | 'tool_result' }>>;
      toolRounds?: [AiToolRound];
    };

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
  if (toolCall.approvalState === 'rejected') return 'rejected';
  if (toolCall.executionState === 'completed') return 'completed';
  if (toolCall.executionState === 'error') return 'error';
  if (toolCall.executionState === 'running') return 'running';
  if (toolCall.approvalState === 'approved') return 'approved';
  if (toolCall.approvalState === 'pending') return 'pending_user_approval';
  return 'pending';
}

function mapStreamingToolCallStatus(part: Extract<AiTurnPart, { type: 'tool_call' }>): AiToolCall['status'] {
  return part.status === 'partial' ? 'pending' : 'running';
}

function mapLegacyToolCall(toolCall: AiToolCall): AiTurnToolCall {
  return {
    id: toolCall.id,
    name: toolCall.name,
    argumentsText: toolCall.arguments,
    approvalState: toolCall.status === 'pending_user_approval'
      ? 'pending'
      : toolCall.status === 'approved'
        ? 'approved'
        : toolCall.status === 'rejected'
          ? 'rejected'
          : undefined,
    executionState: toolCall.status === 'running'
      ? 'running'
      : toolCall.status === 'completed'
        ? 'completed'
        : toolCall.status === 'error'
          ? 'error'
          : toolCall.status === 'pending'
            ? 'pending'
            : undefined,
  };
}

export function projectLegacyMessageToTurn(
  message: Pick<AiChatMessage, 'id' | 'content' | 'thinkingContent' | 'toolCalls'>,
): AiAssistantTurn {
  const parts: AiAssistantTurn['parts'] = [];

  if (message.thinkingContent) {
    parts.push({ type: 'thinking', text: message.thinkingContent });
  }

  if (message.content) {
    parts.push({ type: 'text', text: message.content });
  }

  for (const toolCall of message.toolCalls ?? []) {
    parts.push({
      type: 'tool_call',
      id: toolCall.id,
      name: toolCall.name,
      argumentsText: toolCall.arguments,
      status: toolCall.status === 'pending' || toolCall.status === 'pending_user_approval' ? 'partial' : 'complete',
    });

    if (toolCall.result) {
      parts.push({
        type: 'tool_result',
        toolCallId: toolCall.id,
        toolName: toolCall.name,
        success: toolCall.result.success,
        output: toolCall.result.output,
        error: toolCall.result.error,
        durationMs: toolCall.result.durationMs,
        truncated: toolCall.result.truncated,
      });
    }
  }

  return {
    id: message.id,
    status: 'complete',
    parts,
    toolRounds: (message.toolCalls?.length ?? 0) > 0
      ? [{
          id: `${message.id}-round-legacy`,
          round: 1,
          toolCalls: (message.toolCalls ?? []).map(mapLegacyToolCall),
        }]
      : [],
    plainTextSummary: message.content,
  };
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

function getToolPartRound(part: Extract<AiTurnPart, { type: 'tool_call' | 'tool_result' }>, toolRounds: AiToolRound[]): AiToolRound | undefined {
  const toolCallId = part.type === 'tool_call' ? part.id : part.toolCallId;
  return toolRounds.find((round) => round.toolCalls.some((toolCall) => toolCall.id === toolCallId));
}

export function buildAssistantDisplaySegments(turn: AiAssistantTurn): AiAssistantDisplaySegment[] {
  const segments: AiAssistantDisplaySegment[] = [];
  let bufferedToolParts: Array<Extract<AiTurnPart, { type: 'tool_call' | 'tool_result' }>> = [];
  let bufferedToolRound: AiToolRound | undefined;

  const flushBufferedToolParts = () => {
    if (bufferedToolParts.length === 0) {
      return;
    }

    segments.push({
      kind: 'tool',
      toolParts: bufferedToolParts,
      toolRounds: bufferedToolRound ? [bufferedToolRound] : undefined,
    });
    bufferedToolParts = [];
    bufferedToolRound = undefined;
  };

  for (const part of turn.parts) {
    if (isPartType(part, 'tool_call') || isPartType(part, 'tool_result')) {
      const nextRound = getToolPartRound(part, turn.toolRounds);
      if (bufferedToolParts.length > 0 && bufferedToolRound?.id !== nextRound?.id) {
        flushBufferedToolParts();
      }

      bufferedToolParts.push(part);
      bufferedToolRound ??= nextRound;
      continue;
    }

    flushBufferedToolParts();

    if (isPartType(part, 'text')) {
      segments.push({ kind: 'text', text: part.text });
      continue;
    }

    if (isPartType(part, 'thinking')) {
      segments.push({ kind: 'thinking', text: part.text, streaming: part.streaming });
      continue;
    }

    if (isPartType(part, 'guardrail')) {
      segments.push({ kind: 'guardrail', part });
      continue;
    }

    if (isPartType(part, 'warning') || isPartType(part, 'error')) {
      segments.push({ kind: 'warning', part });
    }
  }

  flushBufferedToolParts();
  return segments;
}

export function projectTurnToLegacyMessageFields(turn: AiAssistantTurn): LegacyProjectedMessageFields {
  const textContent = getTurnTextContent(turn);
  const fallbackContent = getFallbackContent(turn);
  const content = textContent || fallbackContent;
  const thinkingContent = getTurnThinkingContent(turn);
  const toolCalls = flattenToolCalls(turn, collectToolResults(turn));

  return {
    content,
    thinkingContent,
    toolCalls,
  };
}