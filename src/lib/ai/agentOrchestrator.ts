// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

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

import { useAgentStore } from '../../store/agentStore';
import { useAppStore } from '../../store/appStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useSettingsStore } from '../../store/settingsStore';
import { getProvider } from './providerRegistry';
import { buildAgentSystemPrompt } from './agentSystemPrompt';
import { buildPlannerSystemPrompt } from './agentPlanner';
import { buildReviewerSystemPrompt, buildReviewPrompt, parseReview } from './agentReviewer';
import { getToolsForContext } from './tools';
import { estimateTokens, getModelContextWindow, responseReserve } from './tokenUtils';
import { getActiveCwd, getActivePaneMetadata } from '../terminalRegistry';
import { platform } from '../platform';
import { nodeGetState, nodeAgentStatus } from '../api';
import { api } from '../api';
import i18n from '../../i18n';
import { useToastStore } from '../../hooks/useToast';
import {
  MAX_OUTPUT_BYTES,
  MAX_EMPTY_ROUNDS,
  CONDENSE_AFTER_ROUND,
  CONDENSE_KEEP_RECENT,
  CONTEXT_OVERFLOW_RATIO,
  DEFAULT_REVIEW_INTERVAL,
} from './agentConfig';
import {
  streamCompletion,
  runSingleShot,
  processToolCalls,
  createStep,
} from './roles';
import type { ChatMessage, AiStreamProvider } from './providers';
import type { AgentTask, AgentStep, AgentPlanStep, AgentRoleConfig, AgentReviewerConfig, TabType } from '../../types';
import type { ToolExecutionContext } from './tools';
/** Cache for resolveActiveToolContext — skip IPC if focused node hasn't changed */
let _cachedToolContext: { nodeId: string; context: ToolExecutionContext } | null = null;

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
//
// Cached per focused node — if the user hasn't switched nodes between rounds,
// we reuse the last result and skip both IPC calls (nodeGetState + nodeAgentStatus).
// The cache is invalidated when the focused node changes or the task starts.
// ═══════════════════════════════════════════════════════════════════════════

async function resolveActiveToolContext(): Promise<ToolExecutionContext> {
  const empty: ToolExecutionContext = {
    activeNodeId: null,
    activeAgentAvailable: false,
  };

  try {
    const focusedNodeId = useSessionTreeStore.getState().getFocusedNodeId();
    if (!focusedNodeId) {
      _cachedToolContext = null;
      return empty;
    }

    // Cache hit — same node as last round, skip IPC
    if (_cachedToolContext && _cachedToolContext.nodeId === focusedNodeId) {
      return _cachedToolContext.context;
    }

    // Cache miss — resolve from backend
    const context: ToolExecutionContext = {
      activeNodeId: null,
      activeAgentAvailable: false,
    };

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

    _cachedToolContext = { nodeId: focusedNodeId, context };
    return context;
  } catch (e) {
    console.warn('[AgentOrchestrator] resolveActiveToolContext failed:', e);
    return empty;
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Rebuild LLM messages from prior agent steps (for task resume)
// ═══════════════════════════════════════════════════════════════════════════

function rebuildMessagesFromSteps(messages: ChatMessage[], steps: AgentStep[]): void {
  // Group steps by round for correct message ordering
  const roundMap = new Map<number, AgentStep[]>();
  for (const step of steps) {
    const arr = roundMap.get(step.roundIndex) ?? [];
    arr.push(step);
    roundMap.set(step.roundIndex, arr);
  }

  const sortedRounds = [...roundMap.keys()].sort((a, b) => a - b);
  for (const roundIdx of sortedRounds) {
    const roundSteps = roundMap.get(roundIdx)!;
    for (const step of roundSteps) {
      switch (step.type) {
        case 'plan':
        case 'decision':
          // Assistant text response
          messages.push({ role: 'assistant', content: step.content });
          break;
        case 'tool_call':
          // If this step has a tool result, emit assistant+tool_calls then tool response
          if (step.toolCall?.result) {
            messages.push({
              role: 'assistant',
              content: '',
              tool_calls: [{
                id: step.toolCall.result.toolCallId,
                name: step.toolCall.name,
                arguments: step.toolCall.arguments,
              }],
            });
            const output = step.toolCall.result.success
              ? step.toolCall.result.output.slice(0, MAX_OUTPUT_BYTES)
              : `Error: ${step.toolCall.result.error}`;
            messages.push({
              role: 'tool',
              content: output,
              tool_call_id: step.toolCall.result.toolCallId,
              tool_name: step.toolCall.name,
            });
          }
          break;
        case 'review':
          // Reviewer feedback — inject as assistant context
          messages.push({ role: 'assistant', content: step.content });
          break;
        // observation, error, user_input, verify — skip (already captured in tool results)
      }
    }
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Parse plan from LLM response
// ═══════════════════════════════════════════════════════════════════════════

function parsePlan(text: string): { description: string; steps: AgentPlanStep[] } | null {
  // Try to extract JSON plan from the response
  const jsonMatch = text.match(/```json\s*([\s\S]*?)```/);
  if (jsonMatch) {
    try {
      const parsed = JSON.parse(jsonMatch[1]);
      if (parsed.plan?.steps && Array.isArray(parsed.plan.steps)) {
        return {
          description: parsed.plan.description || '',
          steps: parsed.plan.steps.map((s: unknown) => ({
            description: typeof s === 'string' ? s : String(s),
            status: 'pending' as const,
          })),
        };
      }
    } catch { /* fallthrough */ }
  }

  // Try raw JSON parse
  try {
    const parsed = JSON.parse(text);
    if (parsed.plan?.steps && Array.isArray(parsed.plan.steps)) {
      return {
        description: parsed.plan.description || '',
        steps: parsed.plan.steps.map((s: unknown) => ({
          description: typeof s === 'string' ? s : String(s),
          status: 'pending' as const,
        })),
      };
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
      // Ensure details is always a string — AI may return an object
      const rawDetails = parsed.details;
      const details = typeof rawDetails === 'string'
        ? rawDetails
        : (rawDetails && typeof rawDetails === 'object' ? JSON.stringify(rawDetails, null, 2) : '');
      return {
        status: parsed.status === 'failed' ? 'failed' : 'completed',
        summary: typeof parsed.summary === 'string' ? parsed.summary : String(parsed.summary),
        details,
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
// Helper: Resolve role-specific provider/model config
// Falls back to task default if role is not configured or disabled
// ═══════════════════════════════════════════════════════════════════════════

type ResolvedRoleConfig = {
  provider: AiStreamProvider;
  baseUrl: string;
  model: string;
  apiKey: string;
};

async function resolveRoleConfig(
  roleConfig: AgentRoleConfig | AgentReviewerConfig | undefined,
  fallback: { provider: AiStreamProvider; baseUrl: string; model: string; apiKey: string },
): Promise<ResolvedRoleConfig> {
  if (!roleConfig?.enabled || !roleConfig.providerId || !roleConfig.model) {
    return fallback;
  }

  const settings = useSettingsStore.getState().settings;
  const roleProvider = settings.ai.providers.find(p => p.id === roleConfig.providerId);
  if (!roleProvider || !roleProvider.enabled || !roleProvider.baseUrl) {
    return fallback;
  }

  try {
    const roleAiProvider = getProvider(roleProvider.type);
    const roleApiKey = await getApiKeyForProvider(roleProvider.id, roleProvider.type);
    return {
      provider: roleAiProvider,
      baseUrl: roleProvider.baseUrl,
      model: roleConfig.model,
      apiKey: roleApiKey,
    };
  } catch {
    return fallback;
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Main Entry: Run Agent
// ═══════════════════════════════════════════════════════════════════════════

// Concurrency guard — prevents overlapping runAgent() calls
let _agentRunning = false;

export async function runAgent(task: AgentTask, signal: AbortSignal): Promise<void> {
  if (_agentRunning) {
    throw new Error('Agent task already running');
  }
  _agentRunning = true;
  _cachedToolContext = null; // Reset cache for new task
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

    // ── Resolve role-specific configs ────────────────────────────────────
    const agentRoles = settings.ai.agentRoles;
    const executorFallback = { provider: aiProvider, baseUrl: provider.baseUrl, model: task.model, apiKey };
    const plannerConfig = await resolveRoleConfig(agentRoles?.planner, executorFallback);
    const reviewerRoleConfig = agentRoles?.reviewer;
    const reviewerConfig = await resolveRoleConfig(reviewerRoleConfig, executorFallback);
    const reviewInterval = reviewerRoleConfig?.enabled ? (reviewerRoleConfig.interval ?? DEFAULT_REVIEW_INTERVAL) : 0;

    // ── Tool resolution (refreshed each round to track tab switches) ────
    const disabledToolNames = settings.ai.toolUse?.disabledTools ?? [];
    const disabledSet = new Set(disabledToolNames);

    // Pre-load MCP registry once
    const { useMcpRegistry } = await import('./mcp');

    /** Resolve tools for the current active tab type, merging MCP tools. */
    const resolveTools = () => {
      const appState = useAppStore.getState();
      const activeTab = appState.tabs.find(t => t.id === appState.activeTabId);
      const activeTabType = activeTab?.type ?? null;
      const hasAnySSH = appState.sessions.size > 0;
      let resolved = getToolsForContext(activeTabType, hasAnySSH, disabledSet);
      const mcpTools = useMcpRegistry.getState().getAllMcpToolDefinitions();
      if (mcpTools.length > 0) {
        const filtered = mcpTools.filter(t => !disabledSet.has(t.name));
        if (filtered.length > 0) resolved = [...resolved, ...filtered];
      }
      return resolved;
    };

    let tools = resolveTools();

    // ── Build initial context ────────────────────────────────────────────
    const sessionsDesc = await getSessionsDescription();
    const contextWindow = getModelContextWindow(
      task.model,
      settings.ai.modelContextWindows,
      task.providerId,
    );
    const reserve = responseReserve(contextWindow);

    // ── Conversation history for LLM ─────────────────────────────────────
    const messages: ChatMessage[] = [];

    // Snapshot CWD at task creation, so it won't drift if user switches panes
    const cwd = getActiveCwd();

    // Snapshot environment context (refreshed per-round via snapshotEnvContext)
    const snapshotEnvContext = () => {
      const paneMetadata = getActivePaneMetadata();
      const snap = useAppStore.getState();
      const tab = snap.tabs.find(t => t.id === snap.activeTabId);
      const result = {
        activeTabType: (tab?.type ?? null) as TabType | null,
        terminalType: (paneMetadata?.terminalType ?? null) as 'terminal' | 'local_terminal' | null,
        connectionInfo: undefined as string | undefined,
        remoteEnvDesc: undefined as string | undefined,
        localOS: platform.isMac ? 'macOS' : platform.isWindows ? 'Windows' : 'Linux',
      };
      if (result.terminalType === 'terminal' && paneMetadata?.sessionId) {
        const session = snap.sessions.get(paneMetadata.sessionId);
        if (session?.connectionId) {
          const conn = snap.connections.get(session.connectionId);
          if (conn) {
            result.connectionInfo = `${conn.username}@${conn.host}`;
            if (conn.remoteEnv) {
              const { osType, osVersion, arch, kernel, shell } = conn.remoteEnv;
              const parts: string[] = [osType];
              if (osVersion) parts.push(osVersion);
              if (arch) parts.push(arch);
              if (kernel) parts.push(`kernel ${kernel}`);
              if (shell) parts.push(`shell ${shell}`);
              result.remoteEnvDesc = parts.join(', ');
            }
          }
        }
      }
      return result;
    };

    const envCtx = snapshotEnvContext();
    const { activeTabType, terminalType, connectionInfo, localOS, remoteEnvDesc } = envCtx;

    let systemPrompt = buildAgentSystemPrompt({
      autonomyLevel: task.autonomyLevel,
      maxRounds: task.maxRounds,
      currentRound: 0,
      availableSessions: sessionsDesc,
      activeTabType,
      terminalType,
      connectionInfo,
      localOS,
      remoteEnvDesc,
      cwd: cwd ?? undefined,
    });

    messages.push({ role: 'system', content: systemPrompt });
    messages.push({ role: 'user', content: `Task: ${task.goal}` });

    // ── Resume Path: rebuild messages from prior steps ────────────────────
    let startRound = 0;
    if (task.resumeFromRound != null && task.steps.length > 0) {
      // Rebuild LLM conversation from the preserved steps
      rebuildMessagesFromSteps(messages, task.steps);

      // Inform LLM we're resuming
      const skippedStepDescs = task.plan?.steps
        .filter(s => s.status === 'skipped')
        .map(s => s.description) ?? [];
      let resumeNote = `\n\n[System: This task is being resumed from round ${task.resumeFromRound}. Continue executing the remaining plan steps.]`;
      if (skippedStepDescs.length > 0) {
        resumeNote += `\n[The user has skipped these steps — do NOT execute them: ${skippedStepDescs.join('; ')}]`;
      }
      messages.push({ role: 'user', content: resumeNote });

      startRound = task.resumeFromRound;
      store().setTaskStatus('executing');
    } else if (task.plan) {
      // ── Seeded Plan (reuse from previous task) ─────────────────────────
      // Plan already set by startTask with seedPlan — skip planning phase
      const planStep = createStep(0, 'plan', task.plan.description ?? task.goal);
      store().appendStep(planStep);
      store().updateStep(planStep.id, {
        content: task.plan.description ?? '',
        status: 'completed',
        durationMs: 0,
      });
      messages.push({ role: 'assistant', content: `Plan:\n${task.plan.steps.map((s, i) => `${i + 1}. ${s.description}`).join('\n')}` });
      store().setTaskStatus('executing');
    } else {
      // ── Phase 1: Planning ──────────────────────────────────────────────
      // Use planner role if configured (may use a different, cheaper/faster model)
      const useDedicatedPlanner = !!agentRoles?.planner?.enabled && !!agentRoles.planner.providerId && !!agentRoles.planner.model;

      const planStep = createStep(0, 'plan', '');
      store().appendStep(planStep);
      store().updateStep(planStep.id, { status: 'running' });

      let planText = '';
      let planThinking = '';

      if (useDedicatedPlanner) {
        // Dedicated planner: Use planner-specific prompt (no tools, plan-only)
        const plannerPrompt = buildPlannerSystemPrompt({
          autonomyLevel: task.autonomyLevel,
          maxRounds: task.maxRounds,
          availableSessions: sessionsDesc,
        });
        const plannerMessages: ChatMessage[] = [
          { role: 'system', content: plannerPrompt + (cwd ? `\nCurrent working directory: ${cwd}` : '') },
          { role: 'user', content: `Task: ${task.goal}` },
        ];

        try {
          const result = await runSingleShot(
            { provider: plannerConfig.provider, baseUrl: plannerConfig.baseUrl, model: plannerConfig.model, apiKey: plannerConfig.apiKey },
            plannerMessages,
            signal,
          );
          planText = result.text;
          planThinking = result.thinkingContent;
        } catch (planErr) {
          store().updateStep(planStep.id, {
            status: 'error',
            content: planText || (planErr instanceof Error ? planErr.message : String(planErr)),
            durationMs: Date.now() - planStep.timestamp,
          });
          throw planErr;
        }
      } else {
        // Default: executor model handles planning (existing behavior)
        try {
          const result = await runSingleShot(
            { provider: aiProvider, baseUrl: provider.baseUrl, model: task.model, apiKey },
            messages,
            signal,
          );
          planText = result.text;
          planThinking = result.thinkingContent;
        } catch (planErr) {
          store().updateStep(planStep.id, {
            status: 'error',
            content: planText || (planErr instanceof Error ? planErr.message : String(planErr)),
            durationMs: Date.now() - planStep.timestamp,
          });
          throw planErr;
        }
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
    }

    // ── Phase 2: Execution Loop ──────────────────────────────────────────
    let emptyRoundCount = 0;
    for (let round = startRound; round < task.maxRounds; round++) {
      if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

      // Wait if paused (with 30-minute safety timeout, decoupled from poll loop)
      if (store().activeTask?.status === 'paused') {
        const MAX_PAUSE_MS = 30 * 60 * 1000;
        let pauseTimedOut = false;
        const pauseTimer = setTimeout(() => { pauseTimedOut = true; }, MAX_PAUSE_MS);
        try {
          while (store().activeTask?.status === 'paused') {
            if (pauseTimedOut) {
              store().setTaskSummary('Task auto-cancelled: paused for over 30 minutes.');
              store().setTaskStatus('cancelled');
              showToast('agent.toast.pause_timeout', 'warning');
              return;
            }
            await new Promise(r => setTimeout(r, 200));
            if (signal.aborted) throw new DOMException('Aborted', 'AbortError');
          }
        } finally {
          clearTimeout(pauseTimer);
        }
      }

      store().incrementRound();
      _cachedToolContext = null; // Invalidate per-round to pick up focus changes

      // Refresh tools to pick up tab switches (e.g. after open_tab / open_session_tab)
      tools = resolveTools();

      // Update system prompt with current round (refresh env context for tab switches)
      const roundEnv = snapshotEnvContext();
      const liveAutonomyLevel = useAgentStore.getState().autonomyLevel;
      messages[0] = {
        role: 'system',
        content: buildAgentSystemPrompt({
          autonomyLevel: liveAutonomyLevel,
          maxRounds: task.maxRounds,
          currentRound: round,
          availableSessions: sessionsDesc,
          activeTabType: roundEnv.activeTabType,
          terminalType: roundEnv.terminalType,
          connectionInfo: roundEnv.connectionInfo,
          localOS: roundEnv.localOS,
          remoteEnvDesc: roundEnv.remoteEnvDesc,
          cwd: cwd ?? undefined,
        }),
      };

      // Trim history if needed
      const budget = contextWindow - reserve;
      const trimmed = trimMessages(messages, budget);

      // Stream LLM response via RoleRunner
      const streamResult = await streamCompletion(
        { provider: aiProvider, baseUrl: provider.baseUrl, model: task.model, apiKey },
        trimmed,
        tools,
        signal,
      );

      const { text: responseText, thinkingContent, toolCalls: collectedToolCalls } = streamResult;

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
          // Advance plan to final step so the indicator shows full progress
          const currentPlan = store().activeTask?.plan;
          if (currentPlan) {
            store().setPlan({ ...currentPlan, currentStepIndex: currentPlan.steps.length });
          }
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
          const p = store().activeTask?.plan;
          if (p) store().setPlan({ ...p, currentStepIndex: p.steps.length });
          store().setTaskSummary('Agent stopped: no actionable response after multiple rounds.');
          store().setTaskStatus('completed');
          showToast('agent.toast.no_progress', 'warning');
          return;
        }

        // If no tool calls and no completion, the LLM is asking for input or thinking
        // Continue to next round with the response as context
        continue;
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

      // ── Tool Approval & Execution (delegated to RoleRunner) ──────────
      const toolContext = await resolveActiveToolContext();
      const { results: toolResults, allSucceeded } = await processToolCalls(
        collectedToolCalls,
        round,
        task,
        toolContext,
        signal,
      );

      // Add tool results to conversation
      messages.push(...toolResults);

      // Condense old tool messages to save context
      if (round >= CONDENSE_AFTER_ROUND) {
        condenseToolMessages(messages);
      }

      // Context overflow protection
      const currentTokens = estimateTotalTokens(messages);
      if (currentTokens > contextWindow * CONTEXT_OVERFLOW_RATIO) {
        const p = store().activeTask?.plan;
        if (p) store().setPlan({ ...p, currentStepIndex: p.steps.length });
        store().setTaskSummary('Context window approaching limit. Task stopped to prevent errors.');
        store().setTaskStatus('completed');
        showToast('agent.toast.context_overflow', 'warning');
        return;
      }

      // Advance plan step only if all tools in this round succeeded
      if (allSucceeded && store().activeTask?.plan) {
        store().advancePlanStep();
      }

      // ── Reviewer Check ────────────────────────────────────────────────
      // Invoke the reviewer at configured intervals to audit recent actions
      if (reviewInterval > 0 && round > 0 && ((round + 1) % reviewInterval === 0)) {
        if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

        try {
          const currentSteps = store().activeTask?.steps ?? [];
          // Get recent steps for the reviewer (last reviewInterval rounds)
          const recentSteps = currentSteps.filter(
            s => s.roundIndex > round - reviewInterval && s.roundIndex <= round,
          );

          if (recentSteps.length > 0) {
            const reviewerPrompt = buildReviewerSystemPrompt();
            const reviewContent = buildReviewPrompt(task.goal, recentSteps, round, task.maxRounds);
            const reviewMessages: ChatMessage[] = [
              { role: 'system', content: reviewerPrompt },
              { role: 'user', content: reviewContent },
            ];

            const reviewResult = await runSingleShot(
              { provider: reviewerConfig.provider, baseUrl: reviewerConfig.baseUrl, model: reviewerConfig.model, apiKey: reviewerConfig.apiKey },
              reviewMessages,
              signal,
            );
            const reviewText = reviewResult.text;

            // Record review step
            const reviewStep = createStep(round, 'review', reviewText);
            store().appendStep(reviewStep);
            store().updateStep(reviewStep.id, { status: 'completed' });

            // Parse and act on review
            const review = parseReview(reviewText);
            if (!review) {
              // Record failed parse so user can see the raw response
              store().updateStep(reviewStep.id, { status: 'error', content: `[Review parse failed] ${reviewText.slice(0, 500)}` });
            } else if (review) {
              if (!review.shouldContinue) {
                // Critical issue — stop execution
                const p = store().activeTask?.plan;
                if (p) store().setPlan({ ...p, currentStepIndex: p.steps.length });
                store().setTaskSummary(`Reviewer stopped task: ${review.findings}`);
                store().setTaskStatus('failed');
                showToast('agent.toast.reviewer_stopped', 'warning');
                return;
              }

              // Inject review feedback into executor's conversation
              if (review.assessment !== 'on_track' && review.suggestions.length > 0) {
                const feedbackMsg = `[Review feedback after round ${round + 1}]: ${review.findings}\nSuggestions: ${review.suggestions.join('; ')}`;
                messages.push({ role: 'user', content: feedbackMsg });
              }
            }
          }
        } catch (reviewErr) {
          // Reviewer failure is non-fatal — record as visible error step and continue
          const errMsg = reviewErr instanceof Error ? reviewErr.message : String(reviewErr);
          const errStep = createStep(round, 'error', `Reviewer failed: ${errMsg}`);
          store().appendStep(errStep);
          store().updateStep(errStep.id, { status: 'error' });
        }
      }
    }

    // Max rounds reached
    const p = store().activeTask?.plan;
    if (p) store().setPlan({ ...p, currentStepIndex: p.steps.length });
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
