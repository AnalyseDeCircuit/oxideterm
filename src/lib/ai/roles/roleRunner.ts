// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Role Runner — Reusable LLM streaming and tool execution loop.
 *
 * Extracted from agentOrchestrator to enable each AgentRoleDefinition
 * to be run through a common engine. The orchestrator composes roles
 * into a pipeline; this module handles single-role execution.
 */

import { useAgentStore, registerApprovalResolver, removeApprovalResolver } from '../../../store/agentStore';
import { useSettingsStore } from '../../../store/settingsStore';
import { useToastStore } from '../../../hooks/useToast';
import { decideToolApproval, executeTool, READ_ONLY_TOOLS, sanitizeToolArguments } from '../tools';
import { MAX_TOOL_CALLS_PER_ROUND, MAX_OUTPUT_BYTES } from '../agentConfig';
import i18n from '../../../i18n';
import type { AiReasoningEffort, AiReasoningProtocol, ChatMessage, AiStreamProvider, AiToolDefinition } from '../providers';
import type { AgentStep, AgentApproval, AgentTask, AiToolResult } from '../../../types';
import type { ToolExecutionContext } from '../tools';
import { sanitizeApiMessages } from '../contextSanitizer';
import { createTurnAccumulator } from '../turnModel/turnAccumulator';
import { createAiDiagnosticEvent, type AiDiagnosticTelemetryBase } from '../turnModel/diagnostics';
import type { AiAssistantTurn, AiDiagnosticEvent, AiToolRound } from '../turnModel/types';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/** Result of a single-shot LLM call (no tools) */
export type SingleShotResult = {
  text: string;
  thinkingContent: string;
  turn: AiAssistantTurn;
};

/** Collected tool call from streaming */
export type CollectedToolCall = {
  id: string;
  name: string;
  arguments: string;
};

/** Result of streaming an LLM completion */
export type StreamResult = {
  text: string;
  thinkingContent: string;
  toolCalls: CollectedToolCall[];
  turn: AiAssistantTurn;
  toolRounds: AiToolRound[];
};

/** Config for an LLM provider call */
export type LLMCallConfig = {
  provider: AiStreamProvider;
  baseUrl: string;
  model: string;
  apiKey: string;
  reasoningEffort?: AiReasoningEffort;
  reasoningProtocol?: AiReasoningProtocol;
};

/** Tool execution entry (approval + result) */
export type ToolCallOutcome = {
  toolCallId: string;
  toolName: string;
  resolution: 'executed' | 'rejected' | 'skipped' | 'error';
  resultMessage: ChatMessage;
};

export type RoleRunnerDiagnosticOptions = {
  conversationId: string;
  turnId?: string;
  roundId?: string;
  logicalRound?: number;
  requestKind?: string;
  telemetryBase?: AiDiagnosticTelemetryBase;
  onEvent?: (event: AiDiagnosticEvent) => void | Promise<void>;
};

async function emitDiagnosticEvent(
  diagnostics: RoleRunnerDiagnosticOptions | undefined,
  type: AiDiagnosticEvent['type'],
  data?: Record<string, unknown>,
  options?: { roundId?: string; timestamp?: number },
): Promise<void> {
  if (!diagnostics?.onEvent) return;

  const base = diagnostics.telemetryBase?.source
    ? {
        ...diagnostics.telemetryBase,
        requestKind: diagnostics.requestKind ?? diagnostics.telemetryBase.requestKind,
      }
    : undefined;

  await diagnostics.onEvent(createAiDiagnosticEvent({
    conversationId: diagnostics.conversationId,
    turnId: diagnostics.turnId,
    roundId: options?.roundId ?? diagnostics.roundId,
    timestamp: options?.timestamp,
    type,
    base,
    data,
  }));
}

function getThinkingContent(turn: AiAssistantTurn): string {
  return turn.parts
    .filter((part): part is Extract<AiAssistantTurn['parts'][number], { type: 'thinking' }> => part.type === 'thinking')
    .map((part) => part.text)
    .join('');
}

function getCollectedToolCalls(toolRounds: readonly AiToolRound[]): CollectedToolCall[] {
  return toolRounds.flatMap((round) => round.toolCalls.map((toolCall) => ({
    id: toolCall.id,
    name: toolCall.name,
    arguments: toolCall.argumentsText,
  })));
}

function attachRoundResponseText(turn: AiAssistantTurn): AiAssistantTurn {
  if (turn.toolRounds.length !== 1 || !turn.plainTextSummary) {
    return turn;
  }

  const toolRounds = turn.toolRounds.map((round) => (
    !round.responseText
      ? { ...round, responseText: turn.plainTextSummary }
      : round
  ));

  return { ...turn, toolRounds };
}

// ═══════════════════════════════════════════════════════════════════════════
// streamCompletion — shared LLM streaming logic
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Stream a single LLM completion, collecting text, thinking, and tool calls.
 * Used by all roles (planner, executor, reviewer).
 */
export async function streamCompletion(
  llmConfig: LLMCallConfig,
  messages: ChatMessage[],
  tools: AiToolDefinition[],
  signal: AbortSignal,
  diagnostics?: RoleRunnerDiagnosticOptions,
): Promise<StreamResult> {
  const turnId = diagnostics?.turnId ?? crypto.randomUUID();
  const accumulator = createTurnAccumulator({ turnId });

  const config = {
    baseUrl: llmConfig.baseUrl,
    model: llmConfig.model,
    apiKey: llmConfig.apiKey,
    reasoningEffort: llmConfig.reasoningEffort,
    reasoningProtocol: llmConfig.reasoningProtocol,
    tools,
  };

  await emitDiagnosticEvent(diagnostics, 'llm_request', {
    logicalRound: diagnostics?.logicalRound,
    messageCount: messages.length,
    toolDefinitionCount: tools.length,
  });

  for await (const event of llmConfig.provider.streamCompletion(config, sanitizeApiMessages(messages), signal)) {
    if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

    switch (event.type) {
      case 'content':
        accumulator.onContent(event.content);
        break;
      case 'thinking':
        accumulator.onThinking(event.content);
        break;
      case 'tool_call': {
        if (!event.id) break;
        accumulator.onToolCallPartial({ id: event.id, name: event.name, argumentsText: event.arguments });
        break;
      }
      case 'tool_call_complete': {
        if (!event.id) break;
        accumulator.onToolCallComplete({ id: event.id, name: event.name, argumentsText: event.arguments });
        break;
      }
      case 'done':
        break;
      case 'error':
        accumulator.onError(event.message);
        await emitDiagnosticEvent(diagnostics, 'error', {
          logicalRound: diagnostics?.logicalRound,
          message: event.message,
        });
        throw new Error(event.message);
    }
  }

  accumulator.setStatus('complete');
  const turn = attachRoundResponseText(accumulator.snapshot());
  await emitDiagnosticEvent(diagnostics, 'assistant_round', {
    logicalRound: diagnostics?.logicalRound,
    responseLength: turn.plainTextSummary.length,
    thinkingLength: getThinkingContent(turn).length,
    toolCallCount: getCollectedToolCalls(turn.toolRounds).length,
    toolRoundCount: turn.toolRounds.length,
    toolRoundIds: turn.toolRounds.map((round) => round.id),
  }, {
    roundId: turn.toolRounds.length === 1 ? turn.toolRounds[0].id : diagnostics?.roundId,
  });

  return {
    text: turn.plainTextSummary,
    thinkingContent: getThinkingContent(turn),
    toolCalls: getCollectedToolCalls(turn.toolRounds),
    turn,
    toolRounds: turn.toolRounds,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// runSingleShot — one-shot LLM call with no tools (planner, reviewer)
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Run a single LLM call without tool execution. Suitable for planner
 * and reviewer roles that only need text output.
 */
export async function runSingleShot(
  llmConfig: LLMCallConfig,
  messages: ChatMessage[],
  signal: AbortSignal,
  diagnostics?: RoleRunnerDiagnosticOptions,
): Promise<SingleShotResult> {
  const result = await streamCompletion(llmConfig, messages, [], signal, diagnostics);
  return { text: result.text, thinkingContent: result.thinkingContent, turn: result.turn };
}

// ═══════════════════════════════════════════════════════════════════════════
// shouldAutoApprove — approval policy for tool calls
// ═══════════════════════════════════════════════════════════════════════════

export function shouldAutoApprove(
  toolName: string,
  args: Record<string, unknown>,
  autonomyLevel: AgentTask['autonomyLevel'],
): boolean {
  const autoApproveTools = useSettingsStore.getState().settings.ai.toolUse?.autoApproveTools;
  return decideToolApproval({
    toolName,
    args,
    autoApproveTools,
    readOnlyTools: READ_ONLY_TOOLS,
    autonomyLevel,
  }).autoApprove;
}

// ═══════════════════════════════════════════════════════════════════════════
// createStep — helper to create AgentStep objects
// ═══════════════════════════════════════════════════════════════════════════

export function createStep(
  roundIndex: number,
  type: AgentStep['type'],
  content: string,
  toolCall?: AgentStep['toolCall'],
): AgentStep {
  return {
    id: crypto.randomUUID(),
    roundIndex,
    type,
    content,
    toolCall,
    timestamp: Date.now(),
    status: 'pending',
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// processToolCalls — approval gating + execution for collected tool calls
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Process a batch of tool calls: approve → execute → record → return results.
 *
 * This is the core tool execution loop extracted from the orchestrator.
 * It handles approval prompting, execution, step recording, and LLM feedback.
 */
export async function processToolCalls(
  toolCalls: CollectedToolCall[],
  round: number,
  task: AgentTask,
  toolContext: ToolExecutionContext,
  autonomyLevel: AgentTask['autonomyLevel'],
  signal: AbortSignal,
  diagnostics?: RoleRunnerDiagnosticOptions,
): Promise<{ results: ChatMessage[]; allSucceeded: boolean }> {
  const store = useAgentStore.getState;
  const addToast = useToastStore.getState().addToast;

  // Clamp to max per round
  const clamped = toolCalls.slice(0, MAX_TOOL_CALLS_PER_ROUND);
  const dropped = toolCalls.slice(MAX_TOOL_CALLS_PER_ROUND);
  const results: ChatMessage[] = [];
  let allSucceeded = true;
  const overflowContent = dropped.length > 0
    ? `Too many tool calls in one round (${toolCalls.length}). Only the first ${MAX_TOOL_CALLS_PER_ROUND} were executed; retry the remaining work in a later round.`
    : null;

  if (overflowContent) {
    allSucceeded = false;
    const overflowStep = createStep(round, 'error', overflowContent);
    store().appendStep(overflowStep);
    store().updateStep(overflowStep.id, { status: 'error' });
    await emitDiagnosticEvent(diagnostics, 'error', {
      logicalRound: round,
      message: overflowContent,
      toolCallCount: toolCalls.length,
    });
  }

  for (const tc of clamped) {
    if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

    let parsedArgs: Record<string, unknown>;
    try {
      parsedArgs = JSON.parse(tc.arguments || '{}');
    } catch {
      const errorStep = createStep(round, 'error', `Malformed tool arguments for ${tc.name}: ${tc.arguments.slice(0, 200)}`);
      store().appendStep(errorStep);
      store().updateStep(errorStep.id, { status: 'error' });
      await emitDiagnosticEvent(diagnostics, 'error', {
        logicalRound: round,
        message: `Malformed tool arguments for ${tc.name}`,
        toolCallId: tc.id,
        toolName: tc.name,
      });
      results.push({
        role: 'tool',
        content: `Error: Invalid JSON arguments for ${tc.name}`,
        tool_call_id: tc.id,
        tool_name: tc.name,
      });
      allSucceeded = false;
      continue;
    }

    const sanitizedArgsText = JSON.stringify(sanitizeToolArguments(parsedArgs));

    // Create step
    const toolStep = createStep(round, 'tool_call', `${tc.name}`, {
      name: tc.name,
      arguments: sanitizedArgsText,
    });
    store().appendStep(toolStep);
    await emitDiagnosticEvent(diagnostics, 'tool_call', {
      logicalRound: round,
      toolCallId: tc.id,
      toolName: tc.name,
      arguments: sanitizedArgsText,
    });

    // Check approval
    const approvalDecision = decideToolApproval({
      toolName: tc.name,
      args: parsedArgs,
      autoApproveTools: useSettingsStore.getState().settings.ai.toolUse?.autoApproveTools,
      readOnlyTools: READ_ONLY_TOOLS,
      autonomyLevel,
    });
    const autoApprove = approvalDecision.autoApprove;
    let dangerousCommandApproved = false;

    if (!autoApprove) {
      store().updateStep(toolStep.id, { status: 'pending' });
      store().setTaskStatus('awaiting_approval');
      addToast({ title: i18n.t('agent.toast.approval_needed'), variant: 'warning' });

      const approval: AgentApproval = {
        id: crypto.randomUUID(),
        taskId: task.id,
        stepId: toolStep.id,
        toolName: tc.name,
        arguments: sanitizedArgsText,
        status: 'pending',
        reasoning: undefined,
      };

      let approvalAbortHandler: (() => void) | null = null;
      const resolution = await new Promise<'approved' | 'rejected' | 'skipped'>((resolve) => {
        let settled = false;
        // settled flag prevents double-settle if abort fires between settle() and removeApprovalResolver()
        const settle = (value: boolean | 'skipped') => {
          if (settled) return;
          settled = true;
          if (approvalAbortHandler) {
            signal.removeEventListener('abort', approvalAbortHandler);
            approvalAbortHandler = null;
          }
          removeApprovalResolver(approval.id);
          resolve(value === 'skipped' ? 'skipped' : value ? 'approved' : 'rejected');
        };
        approvalAbortHandler = () => settle(false);
        signal.addEventListener('abort', approvalAbortHandler);
        registerApprovalResolver(approval.id, settle);
        store().addApproval(approval);
      });

      if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

      if (resolution === 'rejected') {
        store().updateStep(toolStep.id, { status: 'skipped', content: `${tc.name} (rejected)` });
        store().setTaskStatus('executing');
        await emitDiagnosticEvent(diagnostics, 'tool_result', {
          logicalRound: round,
          toolCallId: tc.id,
          toolName: tc.name,
          success: false,
          error: 'User rejected this tool call.',
        });
        results.push({
          role: 'tool',
          content: 'User rejected this tool call.',
          tool_call_id: tc.id,
          tool_name: tc.name,
        });
        allSucceeded = false;
        continue;
      }

      if (resolution === 'skipped') {
        store().updateStep(toolStep.id, { status: 'skipped', content: `${tc.name} (skipped)` });
        store().setTaskStatus('executing');
        await emitDiagnosticEvent(diagnostics, 'tool_result', {
          logicalRound: round,
          toolCallId: tc.id,
          toolName: tc.name,
          success: false,
          error: 'User skipped this tool call.',
        });
        results.push({
          role: 'tool',
          content: 'User skipped this tool call. Continue with remaining steps.',
          tool_call_id: tc.id,
          tool_name: tc.name,
        });
        continue;
      }

      store().setTaskStatus('executing');
      dangerousCommandApproved = approvalDecision.risk === 'destructive';
    }

    // Execute tool
    store().updateStep(toolStep.id, { status: 'running' });
    const startTime = Date.now();

    let result: AiToolResult;
    try {
      result = await executeTool(tc.name, parsedArgs, toolContext, {
        dangerousCommandApproved,
        abortSignal: signal,
      });
    } catch (err) {
      result = {
        toolCallId: tc.id,
        toolName: tc.name,
        success: false,
        output: '',
        error: err instanceof Error ? err.message : String(err),
      };
    }

    const durationMs = Date.now() - startTime;

    store().updateStep(toolStep.id, {
      status: result.success ? 'completed' : 'error',
      durationMs,
      toolCall: {
        name: tc.name,
        arguments: tc.arguments,
        result,
      },
    });

    if (!result.success) allSucceeded = false;
    await emitDiagnosticEvent(diagnostics, 'tool_result', {
      logicalRound: round,
      toolCallId: result.toolCallId,
      toolName: result.toolName,
      success: result.success,
      error: result.error,
      outputLength: result.output.length,
      durationMs,
    });

    // Add observation step
    const obsContent = result.success
      ? result.output.slice(0, MAX_OUTPUT_BYTES)
      : `Error: ${result.error || 'Unknown error'}`;
    const obsStep = createStep(round, 'observation', obsContent);
    store().appendStep(obsStep);
    store().updateStep(obsStep.id, { status: 'completed' });

    // Feed result back to LLM (truncate large outputs)
    const truncatedOutput = result.success
      ? (result.output.length > MAX_OUTPUT_BYTES ? result.output.slice(0, MAX_OUTPUT_BYTES) + '\n[output truncated]' : result.output)
      : `Error: ${result.error}`;
    results.push({
      role: 'tool',
      content: truncatedOutput,
      tool_call_id: tc.id,
      tool_name: tc.name,
    });
  }

  if (overflowContent) {
    for (const tc of dropped) {
      results.push({
        role: 'tool',
        content: `Error: ${overflowContent}`,
        tool_call_id: tc.id,
        tool_name: tc.name,
      });
    }
  }

  return { results, allSucceeded };
}
