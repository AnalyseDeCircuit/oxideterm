/**
 * Agent Orchestrator — Core execution engine for autonomous AI agent
 *
 * Manages the Plan→Execute→Verify lifecycle:
 * 1. Plan Phase: Sends goal to LLM, parses structured plan
 * 2. Execute Phase: Iterative tool-call loop with approval gating
 * 3. Verify Phase: LLM self-checks and generates summary
 *
 * Runs in the background, driven by agentStore state.
 * Reuses existing toolExecutor and AI providers.
 */

import { useAgentStore, registerApprovalResolver, removeApprovalResolver } from '../../store/agentStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useSettingsStore } from '../../store/settingsStore';
import { getProvider } from './providerRegistry';
import { buildAgentSystemPrompt } from './agentSystemPrompt';
import { getToolsForContext, isCommandDenied, executeTool, READ_ONLY_TOOLS } from './tools';
import { estimateTokens, getModelContextWindow, responseReserve } from './tokenUtils';
import { nodeGetState, nodeAgentStatus } from '../api';
import { api } from '../api';
import i18n from '../../i18n';
import { useToastStore } from '../../hooks/useToast';
import type { ChatMessage } from './providers';
import type { AgentTask, AgentStep, AgentApproval, AiToolResult } from '../../types';
import type { ToolExecutionContext } from './tools';

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

const MAX_TOOL_CALLS_PER_ROUND = 8;
const MAX_OUTPUT_BYTES = 8192;
const MAX_EMPTY_ROUNDS = 3;
const CONDENSE_AFTER_ROUND = 5;
const CONDENSE_KEEP_RECENT = 3;
const CONTEXT_OVERFLOW_RATIO = 0.9;

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Show toast notification from non-React context
// ═══════════════════════════════════════════════════════════════════════════

function showToast(i18nKey: string, variant: 'success' | 'error' | 'warning' | 'default' = 'default') {
  useToastStore.getState().addToast({
    title: i18n.t(i18nKey),
    variant,
  });
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Estimate tokens for a ChatMessage (content + reasoning_content)
// ═══════════════════════════════════════════════════════════════════════════

function estimateMessageTokens(msg: ChatMessage): number {
  let tokens = estimateTokens(msg.content ?? '');
  if (msg.reasoning_content) tokens += estimateTokens(msg.reasoning_content);
  if (msg.tool_calls) tokens += estimateTokens(JSON.stringify(msg.tool_calls));
  return tokens;
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Estimate total tokens in message array
// ═══════════════════════════════════════════════════════════════════════════

function estimateTotalTokens(messages: ChatMessage[]): number {
  let total = 0;
  for (const m of messages) total += estimateMessageTokens(m);
  return total;
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Trim ChatMessage[] to fit token budget
// ═══════════════════════════════════════════════════════════════════════════

function trimMessages(messages: ChatMessage[], budgetTokens: number): ChatMessage[] {
  // Always keep the system message (index 0) and the last message
  if (messages.length <= 2) return messages;

  const systemMsg = messages[0];
  const remaining = messages.slice(1);

  let total = estimateMessageTokens(systemMsg);
  const kept: ChatMessage[] = [];

  // Walk backwards, keep most recent messages within budget
  for (let i = remaining.length - 1; i >= 0; i--) {
    const msg = remaining[i];
    const tokens = estimateMessageTokens(msg);
    if (total + tokens > budgetTokens && kept.length > 0) break;
    total += tokens;
    kept.unshift(msg);
  }

  return [systemMsg, ...kept];
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Condense old tool result messages to save context
// ═══════════════════════════════════════════════════════════════════════════

function condenseToolMessages(messages: ChatMessage[]): void {
  const toolIndices: number[] = [];
  for (let i = 0; i < messages.length; i++) {
    if (messages[i].role === 'tool') toolIndices.push(i);
  }
  if (toolIndices.length <= CONDENSE_KEEP_RECENT) return;

  const toCondense = toolIndices.slice(0, -CONDENSE_KEEP_RECENT);
  for (const idx of toCondense) {
    const msg = messages[idx];
    const content = msg.content ?? '';
    if (content.startsWith('[condensed]')) continue;

    const toolName = msg.tool_name || 'tool';
    const firstLine = content.split('\n').find(l => l.trim().length > 0) || '';
    const digest = firstLine.slice(0, 120);
    const isError = content.includes('Error:') || content.includes('"error"');
    messages[idx] = {
      ...msg,
      content: `[condensed] ${toolName} → ${isError ? 'err' : 'ok'}: ${digest}`,
    };
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Resolve active ToolExecutionContext for Agent mode
// ═══════════════════════════════════════════════════════════════════════════

async function resolveActiveToolContext(): Promise<ToolExecutionContext> {
  const context: ToolExecutionContext = {
    activeNodeId: null,
    activeAgentAvailable: false,
  };

  try {
    const focusedNodeId = useSessionTreeStore.getState().getFocusedNodeId();
    if (!focusedNodeId) return context;

    const snapshot = await nodeGetState(focusedNodeId);
    if (snapshot?.state?.readiness === 'ready') {
      context.activeNodeId = focusedNodeId;
      try {
        const agentStatus = await nodeAgentStatus(focusedNodeId);
        context.activeAgentAvailable = agentStatus?.type === 'ready';
      } catch (e) {
        console.warn('[AgentOrchestrator] nodeAgentStatus failed for', focusedNodeId, e);
      }
    }
  } catch (e) {
    console.warn('[AgentOrchestrator] resolveActiveToolContext failed:', e);
  }

  return context;
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Create AgentStep
// ═══════════════════════════════════════════════════════════════════════════

function createStep(
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
// Helper: Parse plan from LLM response
// ═══════════════════════════════════════════════════════════════════════════

function parsePlan(text: string): { description: string; steps: string[] } | null {
  // Try to extract JSON plan from the response
  const jsonMatch = text.match(/```json\s*([\s\S]*?)```/);
  if (jsonMatch) {
    try {
      const parsed = JSON.parse(jsonMatch[1]);
      if (parsed.plan?.steps && Array.isArray(parsed.plan.steps)) {
        return {
          description: parsed.plan.description || '',
          steps: parsed.plan.steps,
        };
      }
    } catch { /* fallthrough */ }
  }

  // Try raw JSON parse
  try {
    const parsed = JSON.parse(text);
    if (parsed.plan?.steps && Array.isArray(parsed.plan.steps)) {
      return { description: parsed.plan.description || '', steps: parsed.plan.steps };
    }
  } catch { /* fallthrough */ }

  return null;
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Parse completion status from LLM response
// ═══════════════════════════════════════════════════════════════════════════

function parseCompletion(text: string): { status: 'completed' | 'failed'; summary: string; details: string } | null {
  const jsonMatch = text.match(/```json\s*([\s\S]*?)```/);
  const toParse = jsonMatch ? jsonMatch[1] : text;
  try {
    const parsed = JSON.parse(toParse);
    if (parsed.status && parsed.summary) {
      return {
        status: parsed.status === 'failed' ? 'failed' : 'completed',
        summary: parsed.summary,
        details: parsed.details || '',
      };
    }
  } catch { /* not a completion response */ }
  return null;
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Get available sessions description
// ═══════════════════════════════════════════════════════════════════════════

async function getSessionsDescription(): Promise<string> {
  try {
    const sessions = await api.listSessions();
    if (!sessions || sessions.length === 0) return '';
    return sessions.map(s =>
      `- Session: ${s.id} (${s.name || s.host}:${s.port}, state: ${s.state})`
    ).join('\n');
  } catch {
    return '';
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Should auto-approve a tool call
// ═══════════════════════════════════════════════════════════════════════════

function shouldAutoApprove(
  toolName: string,
  args: Record<string, unknown>,
  autonomyLevel: AgentTask['autonomyLevel'],
): boolean {
  // Deny-listed commands always need approval regardless of level
  if ((toolName === 'terminal_exec' || toolName === 'local_exec' || toolName === 'batch_exec') &&
      typeof args.command === 'string' && isCommandDenied(args.command)) {
    return false;
  }
  if (toolName === 'batch_exec') {
    if (!Array.isArray(args.commands)) return false; // fail closed
    for (const cmd of args.commands) {
      if (typeof cmd === 'string' && isCommandDenied(cmd)) return false;
    }
  }

  switch (autonomyLevel) {
    case 'supervised':
      return false; // Everything needs approval
    case 'balanced':
      return READ_ONLY_TOOLS.has(toolName);
    case 'autonomous':
      return true; // Only deny-list blocks (handled above)
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Get API key for provider
// ═══════════════════════════════════════════════════════════════════════════

async function getApiKeyForProvider(providerId: string, providerType: string): Promise<string> {
  if (providerType === 'ollama' || providerType === 'openai_compatible') {
    try {
      return (await api.getAiProviderApiKey(providerId)) ?? '';
    } catch {
      return '';
    }
  }
  return (await api.getAiProviderApiKey(providerId)) ?? '';
}

// ═══════════════════════════════════════════════════════════════════════════
// Main Entry: Run Agent
// ═══════════════════════════════════════════════════════════════════════════

// Concurrency guard — prevents overlapping runAgent() calls
let _agentRunning = false;

export async function runAgent(task: AgentTask, signal: AbortSignal): Promise<void> {
  if (_agentRunning) {
    console.warn('[AgentOrchestrator] runAgent() called while another task is running. Ignoring.');
    return;
  }
  _agentRunning = true;
  const store = useAgentStore.getState;

  try {
    // ── Get provider config ──────────────────────────────────────────────
    const settings = useSettingsStore.getState().settings;
    const provider = settings.ai.providers.find(p => p.id === task.providerId);
    if (!provider) throw new Error(`Provider not found: ${task.providerId}`);
    if (!provider.enabled) throw new Error(`Provider is disabled: ${provider.name}`);
    if (!provider.baseUrl) throw new Error(`Provider has no base URL: ${provider.name}`);

    const aiProvider = getProvider(provider.type);
    const apiKey = await getApiKeyForProvider(provider.id, provider.type);

    // ── Get available tools ──────────────────────────────────────────────
    const disabledToolNames = settings.ai.toolUse?.disabledTools ?? [];
    const disabledSet = new Set(disabledToolNames);
    const tools = getToolsForContext(null, true, disabledSet);

    // ── Build initial context ────────────────────────────────────────────
    const sessionsDesc = await getSessionsDescription();
    const contextWindow = getModelContextWindow(task.model);
    const reserve = responseReserve(contextWindow);

    // ── Conversation history for LLM ─────────────────────────────────────
    const messages: ChatMessage[] = [];

    // ── Phase 1: Planning ────────────────────────────────────────────────
    const systemPrompt = buildAgentSystemPrompt({
      autonomyLevel: task.autonomyLevel,
      maxRounds: task.maxRounds,
      currentRound: 0,
      availableSessions: sessionsDesc,
    });

    messages.push({ role: 'system', content: systemPrompt });
    messages.push({ role: 'user', content: `Task: ${task.goal}` });

    // Stream planning response
    const planStep = createStep(0, 'plan', '');
    store().appendStep(planStep);
    store().updateStep(planStep.id, { status: 'running' });

    let planText = '';
    let planThinking = '';
    const planConfig = {
      baseUrl: provider.baseUrl,
      model: task.model,
      apiKey,
      tools,
    };

    try {
      for await (const event of aiProvider.streamCompletion(planConfig, messages, signal)) {
        if (signal.aborted) throw new DOMException('Aborted', 'AbortError');
        if (event.type === 'content') {
          planText += event.content;
        }
        if (event.type === 'thinking') {
          planThinking += event.content;
        }
        if (event.type === 'error') {
          throw new Error(event.message);
        }
      }
    } catch (planErr) {
      // Ensure plan step is marked as error on failure
      store().updateStep(planStep.id, {
        status: 'error',
        content: planText || (planErr instanceof Error ? planErr.message : String(planErr)),
        durationMs: Date.now() - planStep.timestamp,
      });
      throw planErr;
    }

    // Parse plan
    const parsedPlan = parsePlan(planText);
    if (parsedPlan) {
      store().setPlan({
        description: parsedPlan.description,
        steps: parsedPlan.steps,
        currentStepIndex: 0,
      });
    }

    store().updateStep(planStep.id, {
      content: planText,
      status: 'completed',
      durationMs: Date.now() - planStep.timestamp,
    });

    // Include reasoning_content for thinking models (Kimi K2.5, DeepSeek-R1)
    const planAssistantMsg: ChatMessage = { role: 'assistant', content: planText };
    if (planThinking) {
      planAssistantMsg.reasoning_content = planThinking;
    }
    messages.push(planAssistantMsg);
    store().setTaskStatus('executing');

    // ── Phase 2: Execution Loop ──────────────────────────────────────────
    let emptyRoundCount = 0;
    for (let round = 0; round < task.maxRounds; round++) {
      if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

      // Wait if paused (with 30-minute safety timeout)
      const pauseStart = Date.now();
      const MAX_PAUSE_MS = 30 * 60 * 1000;
      while (store().activeTask?.status === 'paused') {
        if (Date.now() - pauseStart > MAX_PAUSE_MS) {
          store().setTaskSummary('Task auto-cancelled: paused for over 30 minutes.');
          store().setTaskStatus('cancelled');
          showToast('agent.toast.pause_timeout', 'warning');
          return;
        }
        await new Promise(r => setTimeout(r, 200));
        if (signal.aborted) throw new DOMException('Aborted', 'AbortError');
      }

      store().incrementRound();

      // Update system prompt with current round
      messages[0] = {
        role: 'system',
        content: buildAgentSystemPrompt({
          autonomyLevel: task.autonomyLevel,
          maxRounds: task.maxRounds,
          currentRound: round,
          availableSessions: sessionsDesc,
        }),
      };

      // Trim history if needed
      const budget = contextWindow - reserve;
      const trimmed = trimMessages(messages, budget);

      // Stream LLM response
      const config = {
        baseUrl: provider.baseUrl,
        model: task.model,
        apiKey,
        tools,
      };

      let responseText = '';
      let thinkingContent = '';
      const collectedToolCalls: Array<{ id: string; name: string; arguments: string }> = [];

      for await (const event of aiProvider.streamCompletion(config, trimmed, signal)) {
        if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

        switch (event.type) {
          case 'content':
            responseText += event.content;
            break;
          case 'thinking':
            thinkingContent += event.content;
            break;
          case 'tool_call':
            collectedToolCalls.push({ id: event.id, name: event.name, arguments: event.arguments });
            break;
          case 'tool_call_complete':
            // Update the tool call with complete arguments
            {
              const existing = collectedToolCalls.find(tc => tc.id === event.id);
              if (existing) existing.arguments = event.arguments;
            }
            break;
          case 'error':
            throw new Error(event.message);
        }
      }

      // Check if LLM returned a completion response (no tool calls)
      if (collectedToolCalls.length === 0) {
        const completion = parseCompletion(responseText);

        // Record the decision/observation
        const decisionStep = createStep(round, 'decision', responseText);
        store().appendStep(decisionStep);
        store().updateStep(decisionStep.id, { status: 'completed' });

        // Include reasoning_content for thinking models (Kimi K2.5, DeepSeek-R1)
        const decisionMsg: ChatMessage = { role: 'assistant', content: responseText };
        if (thinkingContent) {
          decisionMsg.reasoning_content = thinkingContent;
        }
        messages.push(decisionMsg);

        if (completion) {
          // Task is done
          store().setTaskSummary(completion.summary + (completion.details ? `\n\n${completion.details}` : ''));
          store().setTaskStatus(completion.status);
          const variant = completion.status === 'completed' ? 'success' : 'error';
          showToast(variant === 'success' ? 'agent.toast.task_completed' : 'agent.toast.task_failed', variant);
          return;
        }

        // Track consecutive empty rounds (no tool calls and no completion)
        emptyRoundCount++;
        if (emptyRoundCount >= MAX_EMPTY_ROUNDS) {
          store().setTaskSummary('Agent stopped: no actionable response after multiple rounds.');
          store().setTaskStatus('completed');
          showToast('agent.toast.no_progress', 'warning');
          return;
        }

        // If no tool calls and no completion, the LLM is asking for input or thinking
        // Continue to next round with the response as context
        continue;
      }

      // Guard: max tool calls per round
      if (collectedToolCalls.length > MAX_TOOL_CALLS_PER_ROUND) {
        collectedToolCalls.length = MAX_TOOL_CALLS_PER_ROUND;
      }

      // Reset empty round counter — we got tool calls
      emptyRoundCount = 0;

      // Record assistant message with tool calls
      // Include reasoning_content for thinking models (Kimi K2.5, DeepSeek-R1)
      const assistantMsg: ChatMessage = {
        role: 'assistant',
        content: responseText,
        tool_calls: collectedToolCalls,
      };
      if (thinkingContent) {
        assistantMsg.reasoning_content = thinkingContent;
      }
      messages.push(assistantMsg);

      // ── Tool Approval & Execution ────────────────────────────────────
      const toolContext = await resolveActiveToolContext();

      const toolResults: ChatMessage[] = [];

      for (const tc of collectedToolCalls) {
        if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

        let parsedArgs: Record<string, unknown>;
        try {
          parsedArgs = JSON.parse(tc.arguments || '{}');
        } catch {
          // Malformed JSON from LLM — record error and skip
          const errorStep = createStep(round, 'error', `Malformed tool arguments for ${tc.name}: ${tc.arguments.slice(0, 200)}`);
          store().appendStep(errorStep);
          store().updateStep(errorStep.id, { status: 'error' });
          toolResults.push({
            role: 'tool',
            content: `Error: Invalid JSON arguments for ${tc.name}`,
            tool_call_id: tc.id,
            tool_name: tc.name,
          });
          continue;
        }

        // Create step for this tool call
        const toolStep = createStep(round, 'tool_call', `${tc.name}`, {
          name: tc.name,
          arguments: tc.arguments,
        });
        store().appendStep(toolStep);

        // Check approval
        const autoApprove = shouldAutoApprove(tc.name, parsedArgs, task.autonomyLevel);

        if (!autoApprove) {
          // Need user approval
          store().updateStep(toolStep.id, { status: 'pending' });
          store().setTaskStatus('awaiting_approval');
          showToast('agent.toast.approval_needed', 'warning');

          const approval: AgentApproval = {
            id: crypto.randomUUID(),
            taskId: task.id,
            stepId: toolStep.id,
            toolName: tc.name,
            arguments: tc.arguments,
            status: 'pending',
          };

          // Register resolver before exposing approval to the UI to avoid
          // a race where the user clicks approval before the waiter exists.
          let approvalAbortHandler: (() => void) | null = null;
          const approved = await new Promise<boolean>((resolve) => {
            let settled = false;
            const settle = (value: boolean) => {
              if (settled) return;
              settled = true;
              if (approvalAbortHandler) {
                signal.removeEventListener('abort', approvalAbortHandler);
                approvalAbortHandler = null;
              }
              removeApprovalResolver(approval.id);
              resolve(value);
            };
            approvalAbortHandler = () => settle(false);
            signal.addEventListener('abort', approvalAbortHandler);
            registerApprovalResolver(approval.id, settle);
            store().addApproval(approval);
          });

          if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

          if (!approved) {
            store().updateStep(toolStep.id, { status: 'skipped', content: `${tc.name} (rejected)` });
            store().setTaskStatus('executing');
            toolResults.push({
              role: 'tool',
              content: 'User rejected this tool call.',
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
        toolResults.push({
          role: 'tool',
          content: truncatedOutput,
          tool_call_id: tc.id,
          tool_name: tc.name,
        });
      }

      // Add tool results to conversation
      messages.push(...toolResults);

      // Condense old tool messages to save context
      if (round >= CONDENSE_AFTER_ROUND) {
        condenseToolMessages(messages);
      }

      // Context overflow protection
      const currentTokens = estimateTotalTokens(messages);
      if (currentTokens > contextWindow * CONTEXT_OVERFLOW_RATIO) {
        store().setTaskSummary('Context window approaching limit. Task stopped to prevent errors.');
        store().setTaskStatus('completed');
        showToast('agent.toast.context_overflow', 'warning');
        return;
      }

      // Advance plan step only if all tools in this round succeeded
      const allSucceeded = toolResults.every(tr => !tr.content?.startsWith('Error:') && !tr.content?.startsWith('User rejected'));
      if (allSucceeded && store().activeTask?.plan) {
        store().advancePlanStep();
      }
    }

    // Max rounds reached
    store().setTaskSummary('Maximum rounds reached. Task may be incomplete.');
    store().setTaskStatus('completed');
    showToast('agent.toast.max_rounds', 'warning');

  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') {
      // Already handled by cancelTask
      return;
    }
    store().setTaskError(err instanceof Error ? err.message : String(err));
    showToast('agent.toast.task_failed', 'error');
  } finally {
    _agentRunning = false;
  }
}
