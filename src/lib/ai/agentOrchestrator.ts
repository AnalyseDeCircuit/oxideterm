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
import { buildPlannerSystemPrompt, parsePlanResponse } from './agentPlanner';
import { buildReviewerSystemPrompt, buildReviewPrompt, formatReviewFeedback, parseReview, shouldRunReviewerForRound } from './agentReviewer';
import { buildRoundContractPrompt, buildRoundContractSystemPrompt, fallbackRoundContract, formatRoundContractForExecutor, parseRoundContract } from './agentContract';
import { buildHandoffArtifact, formatHandoffForExecutor } from './agentHandoff';
import { finalizeReviewResult, shouldTriggerContextReset } from './agentReviewPolicy';
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
import { parseCompletionResponse } from './structuredOutput';
import {
  streamCompletion,
  runSingleShot,
  processToolCalls,
  createStep,
} from './roles';
import type { ChatMessage, AiStreamProvider } from './providers';
import type {
  AgentReviewResult,
  AgentRoleConfig,
  AgentReviewerConfig,
  AgentRoundContract,
  AgentTask,
  AgentStep,
  TabType,
} from '../../types';
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
    skipFocus: true,
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
      skipFocus: true,
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
// Helper: Parse completion status from LLM response
// ═══════════════════════════════════════════════════════════════════════════

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

function normalizeRoundContract(contract: AgentRoundContract, taskId: string, roundIndex: number): AgentRoundContract {
  return {
    ...contract,
    id: contract.id || crypto.randomUUID(),
    taskId,
    roundIndex,
  };
}

function buildSyntheticReview(summary: string): AgentReviewResult {
  return {
    assessment: 'needs_correction',
    summary,
    blockingFindings: [summary],
    suggestions: ['Re-run the review with a narrower scope and explicit verification.'],
    scorecard: {
      contractAdherence: { score: 3, passed: false, findings: [summary] },
      correctness: { score: 3, passed: false, findings: [summary] },
      safety: { score: 8, passed: true, findings: [] },
      efficiency: { score: 5, passed: true, findings: [] },
      verificationQuality: { score: 2, passed: false, findings: [summary] },
    },
    shouldContinue: true,
  };
}

async function buildContractForRound(options: {
  task: AgentTask;
  round: number;
  recentSteps: AgentStep[];
  executorConfig: ResolvedRoleConfig;
  signal: AbortSignal;
}): Promise<AgentRoundContract> {
  const { task, round, recentSteps, executorConfig, signal } = options;
  const contractMessages: ChatMessage[] = [
    { role: 'system', content: buildRoundContractSystemPrompt() },
    {
      role: 'user',
      content: buildRoundContractPrompt({
        taskId: task.id,
        goal: task.goal,
        roundIndex: round,
        plan: task.plan,
        recentSteps,
        lastReview: task.lastReview,
      }),
    },
  ];

  try {
    const result = await runSingleShot(executorConfig, contractMessages, signal);
    const parsed = parseRoundContract(result.text);
    if (parsed) {
      return normalizeRoundContract(parsed, task.id, round);
    }
  } catch (error) {
    console.warn('[AgentOrchestrator] Contract builder failed, using fallback:', error);
  }

  return fallbackRoundContract({
    taskId: task.id,
    roundIndex: round,
    goal: task.goal,
    plan: task.plan,
    lastReview: task.lastReview,
  });
}

async function runReviewerRound(options: {
  task: AgentTask;
  round: number;
  maxRounds: number;
  contract: AgentRoundContract | null;
  recentSteps: AgentStep[];
  reviewerConfig: ResolvedRoleConfig;
  signal: AbortSignal;
}): Promise<AgentReviewResult> {
  const { task, round, maxRounds, contract, recentSteps, reviewerConfig, signal } = options;

  const reviewMessages: ChatMessage[] = [
    { role: 'system', content: buildReviewerSystemPrompt() },
    { role: 'user', content: buildReviewPrompt(task.goal, contract, recentSteps, round, maxRounds) },
  ];

  const result = await runSingleShot(reviewerConfig, reviewMessages, signal);
  const parsed = parseReview(result.text);
  if (!parsed) {
    return buildSyntheticReview('Reviewer output could not be parsed into a scorecard.');
  }
  return finalizeReviewResult(parsed, task.lastReview);
}

function finishTask(summary: string, status: 'completed' | 'failed'): void {
  const store = useAgentStore.getState;
  const plan = store().activeTask?.plan;
  if (plan) {
    store().setPlan({ ...plan, currentStepIndex: plan.steps.length });
  }
  store().setTaskSummary(summary);
  store().setTaskStatus(status);
  showToast(status === 'completed' ? 'agent.toast.task_completed' : 'agent.toast.task_failed', status === 'completed' ? 'success' : 'error');
}

// ═══════════════════════════════════════════════════════════════════════════
// Main Entry: Run Agent
// ═══════════════════════════════════════════════════════════════════════════

// Concurrency guard — prevents overlapping runAgent() calls
let _agentRunning = false;

async function executeTask(task: AgentTask, signal: AbortSignal): Promise<{ nextTask: AgentTask; nextSignal: AbortSignal } | null> {
  const store = useAgentStore.getState;
  const settings = useSettingsStore.getState().settings;
  const provider = settings.ai.providers.find((entry) => entry.id === task.providerId);
  if (!provider) throw new Error(`Provider not found: ${task.providerId}`);
  if (!provider.enabled) throw new Error(`Provider is disabled: ${provider.name}`);
  if (!provider.baseUrl) throw new Error(`Provider has no base URL: ${provider.name}`);

  const aiProvider = getProvider(provider.type);
  const apiKey = await getApiKeyForProvider(provider.id, provider.type);

  const agentRoles = settings.ai.agentRoles;
  const executorConfig = { provider: aiProvider, baseUrl: provider.baseUrl, model: task.model, apiKey };
  const plannerConfig = await resolveRoleConfig(agentRoles?.planner, executorConfig);
  const reviewerRoleConfig = agentRoles?.reviewer;
  const reviewerConfig = await resolveRoleConfig(reviewerRoleConfig, executorConfig);
  const reviewInterval = reviewerRoleConfig?.enabled ? (reviewerRoleConfig.interval ?? DEFAULT_REVIEW_INTERVAL) : 0;

  const disabledToolNames = settings.ai.toolUse?.disabledTools ?? [];
  const disabledSet = new Set(disabledToolNames);
  const { useMcpRegistry } = await import('./mcp');

  const resolveTools = () => {
    const appState = useAppStore.getState();
    const activeTab = appState.tabs.find((entry) => entry.id === appState.activeTabId);
    const activeTabType = activeTab?.type ?? null;
    const hasAnySSH = appState.sessions.size > 0;
    let resolved = getToolsForContext(activeTabType, hasAnySSH, disabledSet);
    const mcpTools = useMcpRegistry.getState().getAllMcpToolDefinitions();
    if (mcpTools.length > 0) {
      const filtered = mcpTools.filter((entry) => !disabledSet.has(entry.name));
      if (filtered.length > 0) resolved = [...resolved, ...filtered];
    }
    return resolved;
  };

  let tools = resolveTools();
  const sessionsDesc = await getSessionsDescription();
  const contextWindow = getModelContextWindow(
    task.model,
    settings.ai.modelContextWindows,
    task.providerId,
    settings.ai.userContextWindows,
  );
  const reserve = responseReserve(contextWindow);
  const messages: ChatMessage[] = [];
  const cwd = getActiveCwd();

  const snapshotEnvContext = () => {
    const paneMetadata = getActivePaneMetadata();
    const snap = useAppStore.getState();
    const tab = snap.tabs.find((entry) => entry.id === snap.activeTabId);
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

  const initialEnv = snapshotEnvContext();
  messages.push({
    role: 'system',
    content: buildAgentSystemPrompt({
      autonomyLevel: task.autonomyLevel,
      maxRounds: task.maxRounds,
      currentRound: 0,
      availableSessions: sessionsDesc,
      activeTabType: initialEnv.activeTabType,
      terminalType: initialEnv.terminalType,
      connectionInfo: initialEnv.connectionInfo,
      localOS: initialEnv.localOS,
      remoteEnvDesc: initialEnv.remoteEnvDesc,
      cwd: cwd ?? undefined,
    }),
  });
  messages.push({ role: 'user', content: `Task: ${task.goal}` });

  if (task.handoffFromTaskId && task.lineageArtifacts.length > 0) {
    messages.push({ role: 'user', content: formatHandoffForExecutor(task.lineageArtifacts[task.lineageArtifacts.length - 1]) });
  }

  let startRound = 0;
  if (task.resumeFromRound != null && task.steps.length > 0) {
    rebuildMessagesFromSteps(messages, task.steps);
    const skippedStepDescs = task.plan?.steps
      .filter((step) => step.status === 'skipped')
      .map((step) => step.description) ?? [];
    let resumeNote = `\n\n[System: This task is being resumed from round ${task.resumeFromRound}. Continue executing the remaining plan steps.]`;
    if (skippedStepDescs.length > 0) {
      resumeNote += `\n[The user has skipped these steps — do NOT execute them: ${skippedStepDescs.join('; ')}]`;
    }
    messages.push({ role: 'user', content: resumeNote });
    startRound = task.resumeFromRound;
    store().setTaskStatus('executing');
  } else if (task.plan) {
    const planStep = createStep(0, 'plan', task.plan.description ?? task.goal);
    store().appendStep(planStep);
    store().updateStep(planStep.id, {
      content: task.plan.description ?? '',
      status: 'completed',
      durationMs: 0,
    });
    messages.push({ role: 'assistant', content: `Plan:\n${task.plan.steps.map((step, index) => `${index + 1}. ${step.description}`).join('\n')}` });
    store().setTaskStatus('executing');
  } else {
    const useDedicatedPlanner = !!agentRoles?.planner?.enabled && !!agentRoles.planner.providerId && !!agentRoles.planner.model;
    const planStep = createStep(0, 'plan', '');
    store().appendStep(planStep);
    store().updateStep(planStep.id, { status: 'running' });

    let planText = '';
    let planThinking = '';

    const planningMessages: ChatMessage[] = useDedicatedPlanner
      ? [
          {
            role: 'system',
            content: buildPlannerSystemPrompt({
              autonomyLevel: task.autonomyLevel,
              maxRounds: task.maxRounds,
              availableSessions: sessionsDesc,
            }) + (cwd ? `\nCurrent working directory: ${cwd}` : ''),
          },
          { role: 'user', content: `Task: ${task.goal}` },
        ]
      : messages;

    try {
      const result = await runSingleShot(
        useDedicatedPlanner ? plannerConfig : executorConfig,
        planningMessages,
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

    const parsedPlan = parsePlanResponse(planText);
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

    const planAssistantMsg: ChatMessage = { role: 'assistant', content: planText };
    if (planThinking) {
      planAssistantMsg.reasoning_content = planThinking;
    }
    messages.push(planAssistantMsg);
    store().setTaskStatus('executing');
  }

  let emptyRoundCount = 0;
  for (let round = startRound; round < task.maxRounds; round++) {
    if (signal.aborted) throw new DOMException('Aborted', 'AbortError');

    if (store().activeTask?.status === 'paused') {
      const maxPauseMs = 30 * 60 * 1000;
      let pauseTimedOut = false;
      const pauseTimer = setTimeout(() => {
        pauseTimedOut = true;
      }, maxPauseMs);
      try {
        while (store().activeTask?.status === 'paused') {
          if (pauseTimedOut) {
            store().setTaskSummary('Task auto-cancelled: paused for over 30 minutes.');
            store().setTaskStatus('cancelled');
            showToast('agent.toast.pause_timeout', 'warning');
            return null;
          }
          await new Promise((resolve) => setTimeout(resolve, 200));
          if (signal.aborted) throw new DOMException('Aborted', 'AbortError');
        }
      } finally {
        clearTimeout(pauseTimer);
      }
    }

    store().incrementRound();
    _cachedToolContext = null;
    tools = resolveTools();

    const liveTask = store().activeTask ?? task;
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

    const contractContextSteps = liveTask.steps.filter((step) => step.roundIndex >= Math.max(0, round - 1));
    const contractStep = createStep(round, 'contract', '');
    store().appendStep(contractStep);
    store().updateStep(contractStep.id, { status: 'running' });

    const contract = await buildContractForRound({
      task: liveTask,
      round,
      recentSteps: contractContextSteps,
      executorConfig,
      signal,
    });
    store().setActiveContract(contract);
    store().updateStep(contractStep.id, {
      status: 'completed',
      durationMs: Date.now() - contractStep.timestamp,
      content: JSON.stringify(contract, null, 2),
    });
    messages.push({ role: 'user', content: formatRoundContractForExecutor(contract) });

    const budget = contextWindow - reserve;
    const trimmed = trimMessages(messages, budget);
    const streamResult = await streamCompletion(executorConfig, trimmed, tools, signal);
    const responseText = streamResult.turn.plainTextSummary;
    const thinkingContent = streamResult.turn.parts
      .filter((part): part is Extract<typeof streamResult.turn.parts[number], { type: 'thinking' }> => part.type === 'thinking')
      .map((part) => part.text)
      .join('');
    const collectedToolCalls = streamResult.toolRounds.flatMap((toolRound) => toolRound.toolCalls.map((toolCall) => ({
      id: toolCall.id,
      name: toolCall.name,
      arguments: toolCall.argumentsText,
    })));

    const responseMessage: ChatMessage = { role: 'assistant', content: responseText };
    if (thinkingContent) {
      responseMessage.reasoning_content = thinkingContent;
    }

    if (collectedToolCalls.length === 0) {
      const completion = parseCompletionResponse(responseText);
      const decisionStep = createStep(round, 'decision', responseText);
      store().appendStep(decisionStep);
      store().updateStep(decisionStep.id, { status: 'completed' });
      messages.push(responseMessage);

      if (completion) {
        const currentSteps = store().activeTask?.steps ?? [];
        const recentSteps = currentSteps.filter((step) => step.roundIndex >= Math.max(0, round - Math.max(1, reviewInterval)) && step.roundIndex <= round);
        const reviewStep = createStep(round, 'review', '');
        store().appendStep(reviewStep);
        store().updateStep(reviewStep.id, { status: 'running' });

        const review = await runReviewerRound({
          task: store().activeTask ?? liveTask,
          round,
          maxRounds: task.maxRounds,
          contract,
          recentSteps,
          reviewerConfig,
          signal,
        });
        store().setLastReview(review);
        store().updateStep(reviewStep.id, {
          status: review.assessment === 'critical_failure' ? 'error' : 'completed',
          durationMs: Date.now() - reviewStep.timestamp,
          content: JSON.stringify(review, null, 2),
        });

        if (review.assessment === 'pass') {
          finishTask(completion.summary + (completion.details ? `\n\n${completion.details}` : ''), completion.status);
          return null;
        }

        if (review.assessment === 'critical_failure') {
          finishTask(review.summary, 'failed');
          showToast('agent.toast.reviewer_stopped', 'warning');
          return null;
        }

        if (review.assessment === 'reset_required') {
          const handoffArtifact = buildHandoffArtifact({
            task: store().activeTask ?? liveTask,
            reason: review.summary,
          });
          const nextTask = await store().createHandoffTask(handoffArtifact);
          if (!nextTask) return null;
          const nextSignal = useAgentStore.getState().abortController?.signal;
          if (!nextSignal) return null;
          return { nextTask, nextSignal };
        }

        const feedbackMsg = formatReviewFeedback(review, round);
        if (feedbackMsg) {
          messages.push({ role: 'user', content: feedbackMsg });
        }
        continue;
      }

      emptyRoundCount++;
      if (emptyRoundCount >= MAX_EMPTY_ROUNDS) {
        const plan = store().activeTask?.plan;
        if (plan) store().setPlan({ ...plan, currentStepIndex: plan.steps.length });
        store().setTaskSummary('Agent stopped: no actionable response after multiple rounds.');
        store().setTaskStatus('completed');
        showToast('agent.toast.no_progress', 'warning');
        return null;
      }
      continue;
    }

    emptyRoundCount = 0;
    responseMessage.tool_calls = collectedToolCalls;
    messages.push(responseMessage);

    const toolContext = await resolveActiveToolContext();
    const { results: toolResults, allSucceeded } = await processToolCalls(
      collectedToolCalls,
      round,
      store().activeTask ?? liveTask,
      toolContext,
      signal,
    );
    messages.push(...toolResults);

    if (round >= CONDENSE_AFTER_ROUND) {
      condenseToolMessages(messages);
    }

    const currentTokens = estimateTotalTokens(messages);
    if (shouldTriggerContextReset(currentTokens, contextWindow, CONTEXT_OVERFLOW_RATIO)) {
      const contextHandoff = buildHandoffArtifact({
        task: store().activeTask ?? liveTask,
        reason: 'Context window approaching limit. Resetting with a fresh handoff.',
      });
      const nextTask = await store().createHandoffTask(contextHandoff);
      if (!nextTask) return null;
      const nextSignal = useAgentStore.getState().abortController?.signal;
      if (!nextSignal) return null;
      return { nextTask, nextSignal };
    }

    let review: AgentReviewResult | null = null;
    if (shouldRunReviewerForRound(round, reviewInterval)) {
      try {
        const currentSteps = store().activeTask?.steps ?? [];
        const recentSteps = currentSteps.filter((step) => step.roundIndex > round - reviewInterval && step.roundIndex <= round);
        if (recentSteps.length > 0) {
          const reviewStep = createStep(round, 'review', '');
          store().appendStep(reviewStep);
          store().updateStep(reviewStep.id, { status: 'running' });

          review = await runReviewerRound({
            task: store().activeTask ?? liveTask,
            round,
            maxRounds: task.maxRounds,
            contract,
            recentSteps,
            reviewerConfig,
            signal,
          });
          store().setLastReview(review);
          store().updateStep(reviewStep.id, {
            status: review.assessment === 'critical_failure' ? 'error' : 'completed',
            durationMs: Date.now() - reviewStep.timestamp,
            content: JSON.stringify(review, null, 2),
          });

          if (review.assessment === 'critical_failure') {
            finishTask(review.summary, 'failed');
            showToast('agent.toast.reviewer_stopped', 'warning');
            return null;
          }

          if (review.assessment === 'reset_required') {
            const handoffArtifact = buildHandoffArtifact({
              task: store().activeTask ?? liveTask,
              reason: review.summary,
            });
            const nextTask = await store().createHandoffTask(handoffArtifact);
            if (!nextTask) return null;
            const nextSignal = useAgentStore.getState().abortController?.signal;
            if (!nextSignal) return null;
            return { nextTask, nextSignal };
          }

          const feedbackMsg = formatReviewFeedback(review, round);
          if (feedbackMsg) {
            messages.push({ role: 'user', content: feedbackMsg });
          }
        }
      } catch (reviewErr) {
        const errMsg = reviewErr instanceof Error ? reviewErr.message : String(reviewErr);
        const errStep = createStep(round, 'error', `Reviewer failed: ${errMsg}`);
        store().appendStep(errStep);
        store().updateStep(errStep.id, { status: 'error' });
      }
    }

    if (allSucceeded && (!review || review.assessment === 'pass') && store().activeTask?.plan) {
      store().advancePlanStep();
    }
  }

  const finalPlan = store().activeTask?.plan;
  if (finalPlan) store().setPlan({ ...finalPlan, currentStepIndex: finalPlan.steps.length });
  store().setTaskSummary('Maximum rounds reached. Task may be incomplete.');
  store().setTaskStatus('completed');
  showToast('agent.toast.max_rounds', 'warning');
  return null;
}

export async function runAgent(task: AgentTask, signal: AbortSignal): Promise<void> {
  if (_agentRunning) {
    throw new Error('Agent task already running');
  }

  _agentRunning = true;
  _cachedToolContext = null;

  try {
    let currentTask: AgentTask | null = task;
    let currentSignal: AbortSignal | null = signal;

    while (currentTask && currentSignal) {
      const next = await executeTask(currentTask, currentSignal);
      if (!next) break;
      currentTask = next.nextTask;
      currentSignal = next.nextSignal;
      _cachedToolContext = null;
    }
  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') {
      return;
    }
    useAgentStore.getState().setTaskError(err instanceof Error ? err.message : String(err));
    showToast('agent.toast.task_failed', 'error');
  } finally {
    _agentRunning = false;
  }
}
