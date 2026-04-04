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
import { executeTool, READ_ONLY_TOOLS, isCommandDenied } from '../tools';
import { MAX_TOOL_CALLS_PER_ROUND, MAX_OUTPUT_BYTES } from '../agentConfig';
import i18n from '../../../i18n';
import type { ChatMessage, AiStreamProvider, AiToolDefinition } from '../providers';
import type { AgentStep, AgentApproval, AgentTask, AiToolResult } from '../../../types';
import type { ToolExecutionContext } from '../tools';
import { sanitizeApiMessages } from '../contextSanitizer';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/** Result of a single-shot LLM call (no tools) */
export type SingleShotResult = {
  text: string;
  thinkingContent: string;
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
};

/** Config for an LLM provider call */
export type LLMCallConfig = {
  provider: AiStreamProvider;
  baseUrl: string;
  model: string;
  apiKey: string;
};

/** Tool execution entry (approval + result) */
export type ToolCallOutcome = {
  toolCallId: string;
  toolName: string;
  resolution: 'executed' | 'rejected' | 'skipped' | 'error';
  resultMessage: ChatMessage;
};

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
): Promise<StreamResult> {
  let text = '';
  let thinkingContent = '';
  const toolCallMap = new Map<string, CollectedToolCall>();

  const config = {
    baseUrl: llmConfig.baseUrl,
    model: llmConfig.model,
    apiKey: llmConfig.apiKey,
    tools,
  };

  for await (const event of llmConfig.provider.streamCompletion(config, sanitizeApiMessages(messages), signal)) {
    if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

    switch (event.type) {
      case 'content':
        text += event.content;
        break;
      case 'thinking':
        thinkingContent += event.content;
        break;
      case 'tool_call': {
        if (!event.id) break;
        const existing = toolCallMap.get(event.id);
        if (existing) {
          existing.arguments = event.arguments;
        } else {
          toolCallMap.set(event.id, { id: event.id, name: event.name, arguments: event.arguments });
        }
        break;
      }
      case 'tool_call_complete': {
        if (!event.id) break;
        const existing = toolCallMap.get(event.id);
        if (existing) {
          existing.arguments = event.arguments;
        } else {
          toolCallMap.set(event.id, { id: event.id, name: event.name, arguments: event.arguments });
        }
        break;
      }
      case 'error':
        throw new Error(event.message);
    }
  }

  return {
    text,
    thinkingContent,
    toolCalls: [...toolCallMap.values()],
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
): Promise<SingleShotResult> {
  const result = await streamCompletion(llmConfig, messages, [], signal);
  return { text: result.text, thinkingContent: result.thinkingContent };
}

// ═══════════════════════════════════════════════════════════════════════════
// shouldAutoApprove — approval policy for tool calls
// ═══════════════════════════════════════════════════════════════════════════

export function shouldAutoApprove(
  toolName: string,
  args: Record<string, unknown>,
  autonomyLevel: AgentTask['autonomyLevel'],
): boolean {
  if ((toolName === 'terminal_exec' || toolName === 'local_exec' || toolName === 'batch_exec') &&
      typeof args.command === 'string' && isCommandDenied(args.command)) {
    return false;
  }
  if (toolName === 'batch_exec') {
    if (!Array.isArray(args.commands)) return false;
    for (const cmd of args.commands) {
      if (typeof cmd === 'string' && isCommandDenied(cmd)) return false;
    }
  }

  switch (autonomyLevel) {
    case 'supervised':
      return false;
    case 'balanced': {
      const autoApproveTools = useSettingsStore.getState().settings.ai.toolUse?.autoApproveTools;
      if (autoApproveTools?.[toolName] === true) return true;
      return READ_ONLY_TOOLS.has(toolName);
    }
    case 'autonomous':
      return true;
  }
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
  signal: AbortSignal,
): Promise<{ results: ChatMessage[]; allSucceeded: boolean }> {
  const store = useAgentStore.getState;
  const addToast = useToastStore.getState().addToast;

  // Clamp to max per round
  const clamped = toolCalls.slice(0, MAX_TOOL_CALLS_PER_ROUND);
  const results: ChatMessage[] = [];
  let allSucceeded = true;

  for (const tc of clamped) {
    if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

    let parsedArgs: Record<string, unknown>;
    try {
      parsedArgs = JSON.parse(tc.arguments || '{}');
    } catch {
      const errorStep = createStep(round, 'error', `Malformed tool arguments for ${tc.name}: ${tc.arguments.slice(0, 200)}`);
      store().appendStep(errorStep);
      store().updateStep(errorStep.id, { status: 'error' });
      results.push({
        role: 'tool',
        content: `Error: Invalid JSON arguments for ${tc.name}`,
        tool_call_id: tc.id,
        tool_name: tc.name,
      });
      allSucceeded = false;
      continue;
    }

    // Create step
    const toolStep = createStep(round, 'tool_call', `${tc.name}`, {
      name: tc.name,
      arguments: tc.arguments,
    });
    store().appendStep(toolStep);

    // Check approval
    const autoApprove = shouldAutoApprove(tc.name, parsedArgs, task.autonomyLevel);

    if (!autoApprove) {
      store().updateStep(toolStep.id, { status: 'pending' });
      store().setTaskStatus('awaiting_approval');
      addToast({ title: i18n.t('agent.toast.approval_needed'), variant: 'warning' });

      const approval: AgentApproval = {
        id: crypto.randomUUID(),
        taskId: task.id,
        stepId: toolStep.id,
        toolName: tc.name,
        arguments: tc.arguments,
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
        results.push({
          role: 'tool',
          content: 'User skipped this tool call. Continue with remaining steps.',
          tool_call_id: tc.id,
          tool_name: tc.name,
        });
        continue;
      }

      store().setTaskStatus('executing');
    }

    // Execute tool
    store().updateStep(toolStep.id, { status: 'running' });
    const startTime = Date.now();

    let result: AiToolResult;
    try {
      result = await executeTool(tc.name, parsedArgs, toolContext);
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

  return { results, allSucceeded };
}
