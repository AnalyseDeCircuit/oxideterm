// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiToolCall, AiToolResult } from '../../../types';
import type {
  AiAssistantTurn,
  AiConversationTurnStatus,
  AiGuardrailCode,
  AiToolRound,
  AiTurnPart,
  AiTurnToolCall,
} from './types';

export interface TurnAccumulator {
  startRound(roundNumber?: number): AiToolRound;
  setRoundStatefulMarker(roundId: string, marker?: string): void;
  onContent(text: string): void;
  onThinking(text: string): void;
  onToolCallPartial(call: { id: string; name: string; argumentsText: string }): void;
  onToolCallComplete(call: { id: string; name: string; argumentsText: string }): void;
  syncToolCalls(toolCalls: readonly AiToolCall[]): void;
  onToolResult(result: AiToolResult, toolName?: string): void;
  onGuardrail(part: { code: AiGuardrailCode; message: string; rawText?: string }): void;
  onWarning(part: { code: string; message: string }): void;
  onError(message: string): void;
  setStatus(status: AiConversationTurnStatus): void;
  snapshot(): AiAssistantTurn;
}

export interface CreateTurnAccumulatorOptions {
  turnId: string;
  initialStatus?: AiConversationTurnStatus;
}

function clonePart(part: AiTurnPart): AiTurnPart {
  if (part.type === 'tool_result') {
    return { ...part };
  }

  if (part.type === 'tool_call') {
    return { ...part };
  }

  if (part.type === 'guardrail') {
    return { ...part };
  }

  if (part.type === 'warning') {
    return { ...part };
  }

  if (part.type === 'error') {
    return { ...part };
  }

  return { ...part };
}

function cloneRound(round: AiToolRound): AiToolRound {
  return {
    ...round,
    toolCalls: round.toolCalls.map((toolCall) => ({ ...toolCall })),
  };
}

function mapToolCallStatus(toolCall: AiToolCall): Pick<AiTurnToolCall, 'approvalState' | 'executionState'> {
  switch (toolCall.status) {
    case 'pending_user_approval':
      return { approvalState: 'pending' };
    case 'approved':
      return { approvalState: 'approved' };
    case 'rejected':
      return { approvalState: 'rejected' };
    case 'running':
      return { executionState: 'running' };
    case 'completed':
      return { executionState: 'completed' };
    case 'error':
      return { executionState: 'error' };
    case 'pending':
    default:
      return { executionState: 'pending' };
  }
}

export function createTurnAccumulator(options: CreateTurnAccumulatorOptions): TurnAccumulator {
  let status: AiConversationTurnStatus = options.initialStatus ?? 'streaming';
  const parts: AiTurnPart[] = [];
  const toolRounds: AiToolRound[] = [];
  const toolCallPartIndex = new Map<string, number>();
  const toolResultPartIndex = new Map<string, number>();
  let nextRoundNumber = 1;
  let openRoundId: string | null = null;

  const getTextSummary = (): string => parts
    .filter((part): part is Extract<AiTurnPart, { type: 'text' }> => part.type === 'text')
    .map((part) => part.text)
    .join('');

  const getOpenRound = (): AiToolRound => {
    if (openRoundId) {
      const existing = toolRounds.find((round) => round.id === openRoundId);
      if (existing) {
        return existing;
      }
    }

    const round: AiToolRound = {
      id: `${options.turnId}-round-${nextRoundNumber}`,
      round: nextRoundNumber,
      timestamp: Date.now(),
      toolCalls: [],
    };

    nextRoundNumber += 1;
    openRoundId = round.id;
    toolRounds.push(round);
    return round;
  };

  const findRoundForToolCall = (toolCallId: string): AiToolRound | undefined => {
    return toolRounds.find((round) => round.toolCalls.some((toolCall) => toolCall.id === toolCallId));
  };

  const upsertToolCallPart = (call: { id: string; name: string; argumentsText: string }, callStatus: 'partial' | 'complete') => {
    const existingIndex = toolCallPartIndex.get(call.id);
    if (existingIndex !== undefined) {
      const existing = parts[existingIndex];
      if (existing?.type === 'tool_call') {
        parts[existingIndex] = {
          ...existing,
          name: call.name,
          argumentsText: call.argumentsText,
          status: callStatus,
        };
      }
      return;
    }

    toolCallPartIndex.set(call.id, parts.length);
    parts.push({
      type: 'tool_call',
      id: call.id,
      name: call.name,
      argumentsText: call.argumentsText,
      status: callStatus,
    });
  };

  const upsertRoundToolCall = (call: { id: string; name: string; argumentsText: string }, patch?: Partial<AiTurnToolCall>) => {
    const round = findRoundForToolCall(call.id) ?? getOpenRound();
    const index = round.toolCalls.findIndex((toolCall) => toolCall.id === call.id);
    if (index === -1) {
      round.toolCalls.push({
        id: call.id,
        name: call.name,
        argumentsText: call.argumentsText,
        ...patch,
      });
      return;
    }

    round.toolCalls[index] = {
      ...round.toolCalls[index],
      name: call.name,
      argumentsText: call.argumentsText || round.toolCalls[index].argumentsText,
      ...patch,
    };
  };

  const appendTextPart = (type: 'text' | 'thinking', text: string) => {
    if (!text) return;

    const lastPart = parts[parts.length - 1];
    if (lastPart?.type === type) {
      parts[parts.length - 1] = {
        ...lastPart,
        text: lastPart.text + text,
      } as Extract<AiTurnPart, { type: typeof type }>;
      return;
    }

    if (type === 'thinking') {
      parts.push({ type: 'thinking', text, streaming: status === 'streaming' });
      return;
    }

    parts.push({ type: 'text', text });
  };

  return {
    startRound(roundNumber) {
      if (typeof roundNumber === 'number' && roundNumber > 0) {
        const currentOpenRound = openRoundId
          ? toolRounds.find((round) => round.id === openRoundId)
          : undefined;

        if (currentOpenRound?.round === roundNumber) {
          return currentOpenRound;
        }

        const existing = toolRounds.find((round) => round.round === roundNumber);
        if (existing) {
          openRoundId = existing.id;
          return existing;
        }

        openRoundId = null;
        if (roundNumber >= nextRoundNumber) {
          nextRoundNumber = roundNumber;
        }
      }

      return getOpenRound();
    },

    setRoundStatefulMarker(roundId, marker) {
      const round = toolRounds.find((candidate) => candidate.id === roundId);
      if (!round) {
        return;
      }

      round.statefulMarker = marker;
    },

    onContent(text) {
      appendTextPart('text', text);
    },

    onThinking(text) {
      appendTextPart('thinking', text);
    },

    onToolCallPartial(call) {
      upsertToolCallPart(call, 'partial');
      upsertRoundToolCall(call, { executionState: 'pending' });
    },

    onToolCallComplete(call) {
      upsertToolCallPart(call, 'complete');
      upsertRoundToolCall(call);
    },

    syncToolCalls(toolCalls) {
      for (const toolCall of toolCalls) {
        upsertRoundToolCall(
          { id: toolCall.id, name: toolCall.name, argumentsText: toolCall.arguments },
          mapToolCallStatus(toolCall),
        );
      }
    },

    onToolResult(result, toolName) {
      const existingIndex = toolResultPartIndex.get(result.toolCallId);
      const normalized: Extract<AiTurnPart, { type: 'tool_result' }> = {
        type: 'tool_result',
        toolCallId: result.toolCallId,
        toolName: toolName ?? result.toolName,
        success: result.success,
        output: result.output,
        error: result.error,
        durationMs: result.durationMs,
        truncated: result.truncated,
      };

      if (existingIndex !== undefined) {
        parts[existingIndex] = normalized;
      } else {
        toolResultPartIndex.set(result.toolCallId, parts.length);
        parts.push(normalized);
      }

      upsertRoundToolCall(
        { id: result.toolCallId, name: toolName ?? result.toolName, argumentsText: '' },
        { executionState: result.success ? 'completed' : 'error' },
      );
    },

    onGuardrail(part) {
      parts.push({ type: 'guardrail', ...part });
    },

    onWarning(part) {
      parts.push({ type: 'warning', ...part });
    },

    onError(message) {
      parts.push({ type: 'error', message });
      status = 'error';
    },

    setStatus(nextStatus) {
      status = nextStatus;
      for (let index = 0; index < parts.length; index += 1) {
        const part = parts[index];
        if (part?.type === 'thinking' && part.streaming !== (nextStatus === 'streaming')) {
          parts[index] = { ...part, streaming: nextStatus === 'streaming' };
        }
      }
    },

    snapshot() {
      return {
        id: options.turnId,
        status,
        parts: parts.map(clonePart),
        toolRounds: toolRounds.map(cloneRound),
        plainTextSummary: getTextSummary(),
      };
    },
  };
}