// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Agent Store — State management for AI Agent autonomous terminal operations
 *
 * Manages agent task lifecycle: planning → execution → verification.
 * The orchestrator runs in the background independently of UI components.
 */

import { create } from 'zustand';
import { api } from '../lib/api';
import { MAX_STEPS } from '../lib/ai/agentConfig';
import type { AgentTask, AgentStep, AgentApproval, AutonomyLevel, AgentTaskStatus, AgentPlan, TabType, AgentTaskMeta } from '../types';

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/** Max rounds per autonomy level */
export const MAX_ROUNDS: Record<AutonomyLevel, number> = {
  supervised: 20,
  balanced: 50,
  autonomous: 100,
};

/** Steps persisted per checkpoint interval */
const CHECKPOINT_INTERVAL = 5;

// ═══════════════════════════════════════════════════════════════════════════
// Persistence Helpers
// ═══════════════════════════════════════════════════════════════════════════

/** Build lightweight meta from a full AgentTask */
function buildTaskMeta(task: AgentTask): AgentTaskMeta {
  return {
    id: task.id,
    goal: task.goal,
    status: task.status,
    autonomyLevel: task.autonomyLevel,
    providerId: task.providerId,
    model: task.model,
    currentRound: task.currentRound,
    maxRounds: task.maxRounds,
    createdAt: task.createdAt,
    completedAt: task.completedAt,
    summary: task.summary,
    error: task.error,
    stepCount: task.steps.length,
    planDescription: task.plan?.description ?? null,
    planJson: task.plan ? JSON.stringify(task.plan) : null,
    contextTabType: task.contextTabType ?? null,
  };
}

/** Persist a completed/archived task (meta + all steps) */
async function persistTask(task: AgentTask): Promise<void> {
  const meta = buildTaskMeta(task);
  await api.agentHistorySaveMeta(JSON.stringify(meta));
  if (task.steps.length > 0) {
    const stepsJson = task.steps.map(s => JSON.stringify(s));
    await api.agentHistorySaveSteps(task.id, stepsJson);
  }
}

function upsertTaskHistory(taskHistory: AgentTaskMeta[], task: AgentTask): AgentTaskMeta[] {
  const meta = buildTaskMeta(task);
  return [meta, ...taskHistory.filter((entry) => entry.id !== meta.id)].slice(0, 50);
}

async function persistTaskAndClearCheckpoint(task: AgentTask): Promise<void> {
  await persistTask(task);
  await api.agentHistoryClearCheckpoint();
}

function isTaskTerminal(task: AgentTask | null): boolean {
  if (!task) return true;
  return task.status === 'completed' || task.status === 'failed' || task.status === 'cancelled';
}

/** Load steps from backend and reconstruct a full AgentTask from meta */
async function loadFullTask(meta: AgentTaskMeta): Promise<AgentTask> {
  const stepsJson = await api.agentHistoryGetSteps(meta.id, 0, meta.stepCount || 500);
  const steps: AgentStep[] = [];
  for (const json of stepsJson) {
    try { steps.push(JSON.parse(json)); } catch { /* skip unparseable */ }
  }
  // Parse plan from meta if available
  let plan: AgentPlan | null = null;
  if (meta.planJson) {
    try { plan = JSON.parse(meta.planJson); } catch { /* skip */ }
  }
  return {
    id: meta.id,
    goal: meta.goal,
    status: meta.status,
    autonomyLevel: meta.autonomyLevel as AutonomyLevel,
    providerId: meta.providerId,
    model: meta.model,
    plan,
    steps,
    currentRound: meta.currentRound,
    maxRounds: meta.maxRounds,
    createdAt: meta.createdAt,
    completedAt: meta.completedAt,
    summary: meta.summary,
    error: meta.error,
    contextTabType: (meta.contextTabType as TabType) ?? null,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// Store Interface
// ═══════════════════════════════════════════════════════════════════════════

interface AgentStore {
  // ─── State ──────────────────────────────────────────────────────────────
  /** Currently running or most recent task */
  activeTask: AgentTask | null;
  /** Historical completed tasks (lightweight metadata only) */
  taskHistory: AgentTaskMeta[];
  /** Default autonomy level for new tasks */
  autonomyLevel: AutonomyLevel;
  /** Whether an agent is currently running */
  isRunning: boolean;
  /** Pending approval requests (UI reads this for approval bar) */
  pendingApprovals: AgentApproval[];
  /** AbortController for the current task */
  abortController: AbortController | null;

  // ─── Task Lifecycle ─────────────────────────────────────────────────────
  /** Start a new agent task. contextTabType inherits tool context from the last active tab. seedPlan reuses a prior plan. */
  startTask: (goal: string, providerId: string, model: string, contextTabType?: TabType | null, seedPlan?: AgentPlan | null) => AgentTask;
  /** Pause the current task */
  pauseTask: () => void;
  /** Resume a paused task */
  resumeTask: () => void;
  /** Cancel the current task */
  cancelTask: () => void;
  /** Resume a historical task from a given round (creates a new task with prior context) */
  resumeHistoryTask: (taskId: string, fromRound?: number) => Promise<AgentTask | null>;

  // ─── Settings ───────────────────────────────────────────────────────────
  /** Set default autonomy level */
  setAutonomyLevel: (level: AutonomyLevel) => void;

  // ─── Step Management (called by orchestrator) ──────────────────────────
  /** Append a new step to the active task */
  appendStep: (step: AgentStep) => void;
  /** Update an existing step */
  updateStep: (stepId: string, updates: Partial<AgentStep>) => void;
  /** Set the task plan */
  setPlan: (plan: AgentPlan) => void;
  /** Update plan's current step index */
  advancePlanStep: () => void;
  /** Skip a plan step at given index (mark as 'skipped') */
  skipPlanStep: (stepIndex: number) => void;
  /** Set task status */
  setTaskStatus: (status: AgentTaskStatus) => void;
  /** Set task summary */
  setTaskSummary: (summary: string) => void;
  /** Set task error */
  setTaskError: (error: string) => void;
  /** Increment round counter */
  incrementRound: () => void;

  // ─── Approval Management ───────────────────────────────────────────────
  /** Add a pending approval */
  addApproval: (approval: AgentApproval) => void;
  /** Resolve a pending approval */
  resolveApproval: (approvalId: string, approved: boolean) => void;
  /** Skip a pending approval (tool skipped, task continues) */
  skipApproval: (approvalId: string) => void;
  /** Resolve all pending approvals */
  resolveAllApprovals: (approved: boolean) => void;
  /** Clear all approvals */
  clearApprovals: () => void;

  // ─── History Management ─────────────────────────────────────────────────
  /** View a historical task (for replay, steps loaded lazily) */
  viewingTask: AgentTask | null;
  /** Whether steps are currently being loaded for viewingTask */
  isLoadingViewingTask: boolean;
  /** Set task to view in replay mode (loads steps from backend) */
  setViewingTask: (meta: AgentTaskMeta | null) => Promise<void>;
  /** Remove a task from history */
  removeFromHistory: (taskId: string) => void;
  /** Clear all task history */
  clearHistory: () => void;
  /** Load task history from persistent storage (call on app init) */
  initHistory: () => Promise<void>;
}

// ═══════════════════════════════════════════════════════════════════════════
// Approval Resolvers (module-level, not in Zustand state)
// ═══════════════════════════════════════════════════════════════════════════

/** Monotonic counter to prevent stale setViewingTask responses */
let viewingTaskRequestId = 0;

const approvalResolvers = new Map<string, (approved: boolean | 'skipped') => void>();

/** Register a resolver for a pending approval (called by orchestrator) */
export function registerApprovalResolver(
  approvalId: string,
  resolver: (approved: boolean | 'skipped') => void,
): void {
  approvalResolvers.set(approvalId, resolver);
}

/** Remove a resolver without invoking it */
export function removeApprovalResolver(approvalId: string): void {
  approvalResolvers.delete(approvalId);
}

/** Reject and clear all pending resolvers (call on task teardown) */
export function clearApprovalResolvers(): void {
  const entries = Array.from(approvalResolvers.entries());
  approvalResolvers.clear();
  for (const [, resolver] of entries) {
    resolver(false);
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Store Implementation
// ═══════════════════════════════════════════════════════════════════════════

export const useAgentStore = create<AgentStore>((set, get) => ({
  // ─── Initial State ────────────────────────────────────────────────────
  activeTask: null,
  taskHistory: [],
  autonomyLevel: 'balanced',
  isRunning: false,
  pendingApprovals: [],
  abortController: null,
  viewingTask: null,
  isLoadingViewingTask: false,

  // ─── Task Lifecycle ─────────────────────────────────────────────────────

  startTask: (goal, providerId, model, contextTabType, seedPlan) => {
    // Cancel any running task first
    const current = get();
    if (!isTaskTerminal(current.activeTask) && current.abortController) {
      current.abortController.abort();
    }

    // Clear old approval resolvers to prevent cross-task pollution
    clearApprovalResolvers();

    // Archive previous task if exists (mark interrupted tasks as cancelled)
    if (current.activeTask) {
      const taskToArchive = (current.activeTask.status === 'executing' || current.activeTask.status === 'planning')
        ? { ...current.activeTask, status: 'cancelled' as const, completedAt: Date.now() }
        : current.activeTask;
      set((s) => ({
        taskHistory: upsertTaskHistory(s.taskHistory, taskToArchive),
      }));
      persistTaskAndClearCheckpoint(taskToArchive).catch((e) => {
        console.warn('[AgentStore] Failed to persist archived task:', e);
      });
    }

    const autonomyLevel = get().autonomyLevel;
    const task: AgentTask = {
      id: crypto.randomUUID(),
      goal,
      status: seedPlan ? 'executing' : 'planning',
      autonomyLevel,
      providerId,
      model,
      plan: seedPlan ? { ...seedPlan, currentStepIndex: 0 } : null,
      steps: [],
      currentRound: 0,
      maxRounds: MAX_ROUNDS[autonomyLevel],
      createdAt: Date.now(),
      completedAt: null,
      summary: null,
      error: null,
      contextTabType: contextTabType ?? null,
    };

    const abortController = new AbortController();

    set({
      activeTask: task,
      isRunning: true,
      pendingApprovals: [],
      abortController,
    });

    return task;
  },

  pauseTask: () => {
    const task = get().activeTask;
    if (!task || task.status !== 'executing') return;
    set({
      activeTask: { ...task, status: 'paused' },
      isRunning: false,
    });
  },

  resumeTask: () => {
    const task = get().activeTask;
    if (!task || task.status !== 'paused') return;
    set({
      activeTask: { ...task, status: 'executing' },
      isRunning: true,
    });
  },

  cancelTask: () => {
    const controller = get().abortController;
    if (controller) controller.abort();

    const task = get().activeTask;
    if (!task) return;

    const finishedTask: AgentTask = {
      ...task,
      status: 'cancelled',
      completedAt: Date.now(),
    };

    set((s) => ({
      activeTask: finishedTask,
      taskHistory: upsertTaskHistory(s.taskHistory, finishedTask),
      isRunning: false,
      pendingApprovals: [],
      abortController: null,
    }));

    persistTaskAndClearCheckpoint(finishedTask).catch((e) => {
      console.warn('[AgentStore] Failed to persist cancelled task:', e);
    });

    // Clear pending resolvers
    clearApprovalResolvers();
  },

  resumeHistoryTask: async (taskId, fromRound) => {
    const current = get();
    // Find the task meta in history
    const sourceMeta = current.taskHistory.find(t => t.id === taskId);
    if (!sourceMeta) return null;

    // Load full task with steps from backend
    let sourceTask: AgentTask;
    try {
      sourceTask = await loadFullTask(sourceMeta);
    } catch (e) {
      console.warn('[AgentStore] Failed to load task steps for resume:', e);
      return null;
    }

    // Cancel any running task first
    if (!isTaskTerminal(current.activeTask) && current.abortController) {
      current.abortController.abort();
    }
    clearApprovalResolvers();

    // Archive current active task if exists
    if (current.activeTask && current.activeTask.id !== taskId) {
      const taskToArchive = (current.activeTask.status === 'executing' || current.activeTask.status === 'planning')
        ? { ...current.activeTask, status: 'cancelled' as const, completedAt: Date.now() }
        : current.activeTask;
      set((s) => ({
        taskHistory: upsertTaskHistory(s.taskHistory, taskToArchive),
      }));
      persistTaskAndClearCheckpoint(taskToArchive).catch((e) => {
        console.warn('[AgentStore] Failed to persist archived task:', e);
      });
    }

    // Determine resume point
    const resumeRound = fromRound ?? (() => {
      // Default: find the last completed step's round
      for (let i = sourceTask.steps.length - 1; i >= 0; i--) {
        if (sourceTask.steps[i].status === 'completed') {
          return sourceTask.steps[i].roundIndex;
        }
      }
      return 0;
    })();

    // Truncate steps to the resume point
    const keptSteps = sourceTask.steps.filter(s => s.roundIndex < resumeRound);

    const autonomyLevel = current.autonomyLevel;
    const newTask: AgentTask = {
      id: crypto.randomUUID(),
      goal: sourceTask.goal,
      status: 'planning',
      autonomyLevel,
      providerId: sourceTask.providerId,
      model: sourceTask.model,
      plan: sourceTask.plan ? {
        ...sourceTask.plan,
        // Keep existing step statuses but reset pending steps after resume point
        steps: sourceTask.plan.steps.map((s, i) =>
          i < sourceTask.plan!.currentStepIndex ? s : { ...s, status: s.status === 'skipped' ? 'skipped' as const : 'pending' as const }
        ),
      } : null,
      steps: keptSteps,
      currentRound: resumeRound,
      maxRounds: MAX_ROUNDS[autonomyLevel],
      createdAt: Date.now(),
      completedAt: null,
      summary: null,
      error: null,
      contextTabType: sourceTask.contextTabType,
      resumeFromRound: resumeRound,
      parentTaskId: sourceTask.id,
    };

    const abortController = new AbortController();

    set({
      activeTask: newTask,
      isRunning: true,
      pendingApprovals: [],
      abortController,
      viewingTask: null,
    });

    return newTask;
  },

  // ─── Settings ───────────────────────────────────────────────────────────

  setAutonomyLevel: (level) => set({ autonomyLevel: level }),

  // ─── Step Management ────────────────────────────────────────────────────

  appendStep: (step) => {
    set((s) => {
      if (!s.activeTask) return s;
      const existingSteps = s.activeTask.steps;
      const stepIndex = existingSteps.length;

      // Persist step incrementally to backend
      api.agentHistoryAppendStep(s.activeTask.id, stepIndex, JSON.stringify(step)).catch((e) => {
        console.warn('[AgentStore] Failed to persist step:', e);
      });

      // Save checkpoint every CHECKPOINT_INTERVAL steps
      if ((stepIndex + 1) % CHECKPOINT_INTERVAL === 0) {
        const updatedTask = { ...s.activeTask, steps: [...existingSteps, step] };
        api.agentHistorySaveCheckpoint(JSON.stringify(updatedTask)).catch((e) => {
          console.warn('[AgentStore] Failed to save checkpoint:', e);
        });
      }

      if (existingSteps.length >= MAX_STEPS) {
        // Add a truncation marker so the user knows earlier steps were dropped
        const marker: AgentStep = {
          id: `truncation-${Date.now()}`,
          roundIndex: step.roundIndex,
          type: 'decision',
          content: `[Earlier steps truncated — only the most recent ${MAX_STEPS} steps are retained]`,
          timestamp: Date.now(),
          status: 'completed',
        };
        const trimmed = existingSteps.slice(-(MAX_STEPS - 2));
        return {
          activeTask: {
            ...s.activeTask,
            steps: [...trimmed, marker, step],
          },
        };
      }
      return {
        activeTask: {
          ...s.activeTask,
          steps: [...existingSteps, step],
        },
      };
    });
  },

  updateStep: (stepId, updates) => {
    set((s) => {
      if (!s.activeTask) return s;
      const steps = s.activeTask.steps;
      const idx = steps.findIndex((step) => step.id === stepId);
      if (idx === -1) return s;
      // Shallow-copy array, splice in the updated step — avoids .map() over all elements
      const newSteps = steps.slice();
      newSteps[idx] = { ...steps[idx], ...updates };
      return {
        activeTask: {
          ...s.activeTask,
          steps: newSteps,
        },
      };
    });
  },

  setPlan: (plan) => {
    set((s) => {
      if (!s.activeTask) return s;
      return {
        activeTask: { ...s.activeTask, plan },
      };
    });
  },

  advancePlanStep: () => {
    set((s) => {
      if (!s.activeTask?.plan) return s;
      const plan = s.activeTask.plan;
      const newSteps = plan.steps.slice();
      // Mark current step as completed
      if (plan.currentStepIndex < newSteps.length) {
        newSteps[plan.currentStepIndex] = { ...newSteps[plan.currentStepIndex], status: 'completed' };
      }
      // Advance past any skipped steps
      let nextIndex = plan.currentStepIndex + 1;
      while (nextIndex < newSteps.length && newSteps[nextIndex].status === 'skipped') {
        nextIndex++;
      }
      return {
        activeTask: {
          ...s.activeTask,
          plan: { ...plan, steps: newSteps, currentStepIndex: nextIndex },
        },
      };
    });
  },

  skipPlanStep: (stepIndex) => {
    set((s) => {
      if (!s.activeTask?.plan) return s;
      const plan = s.activeTask.plan;
      if (stepIndex < 0 || stepIndex >= plan.steps.length) return s;
      if (plan.steps[stepIndex].status !== 'pending') return s;
      const newSteps = plan.steps.slice();
      newSteps[stepIndex] = { ...newSteps[stepIndex], status: 'skipped' };
      return {
        activeTask: {
          ...s.activeTask,
          plan: { ...plan, steps: newSteps },
        },
      };
    });
  },

  setTaskStatus: (status) => {
    const finished = status === 'completed' || status === 'failed' || status === 'cancelled';
    // When task finishes, auto-reject any pending approvals to prevent orphans
    if (finished && get().pendingApprovals.length > 0) {
      clearApprovalResolvers();
    }
    set((s) => {
      if (!s.activeTask) return s;
      const updatedTask = {
        ...s.activeTask,
        status,
        completedAt: finished ? Date.now() : s.activeTask.completedAt,
      };
      if (finished) {
        persistTaskAndClearCheckpoint(updatedTask).catch((e) => {
          console.warn('[AgentStore] Failed to persist task history:', e);
        });
      }
      return {
        activeTask: updatedTask,
        ...(finished ? { taskHistory: upsertTaskHistory(s.taskHistory, updatedTask) } : {}),
        isRunning: !finished && status !== 'paused' && status !== 'awaiting_approval',
        ...(finished ? { pendingApprovals: [] } : {}),
      };
    });
  },

  setTaskSummary: (summary) => {
    set((s) => {
      if (!s.activeTask) return s;
      return { activeTask: { ...s.activeTask, summary } };
    });
  },

  setTaskError: (error) => {
    // Clear pending resolvers to prevent orphans
    clearApprovalResolvers();

    set((s) => {
      if (!s.activeTask) return s;
      const finishedTask = { ...s.activeTask, error, status: 'failed' as const, completedAt: Date.now() };
      persistTaskAndClearCheckpoint(finishedTask).catch((e) => {
        console.warn('[AgentStore] Failed to persist failed task:', e);
      });
      return {
        activeTask: finishedTask,
        taskHistory: upsertTaskHistory(s.taskHistory, finishedTask),
        isRunning: false,
        abortController: null,
        pendingApprovals: [],
      };
    });
  },

  incrementRound: () => {
    set((s) => {
      if (!s.activeTask) return s;
      return {
        activeTask: {
          ...s.activeTask,
          currentRound: s.activeTask.currentRound + 1,
        },
      };
    });
  },

  // ─── Approval Management ───────────────────────────────────────────────

  addApproval: (approval) => {
    set((s) => ({
      pendingApprovals: [...s.pendingApprovals, approval],
    }));
  },

  resolveApproval: (approvalId, approved) => {
    const resolver = approvalResolvers.get(approvalId);
    if (resolver) {
      resolver(approved);
      approvalResolvers.delete(approvalId);
    } else {
      console.warn(`[AgentStore] No resolver found for approval ${approvalId}. Task may have been cancelled.`);
    }

    set((s) => ({
      pendingApprovals: s.pendingApprovals.filter((a) => a.id !== approvalId),
    }));
  },

  skipApproval: (approvalId) => {
    const resolver = approvalResolvers.get(approvalId);
    if (resolver) {
      resolver('skipped');
      approvalResolvers.delete(approvalId);
    } else {
      console.warn(`[AgentStore] No resolver found for approval ${approvalId} (skip). Task may have been cancelled.`);
    }

    set((s) => ({
      pendingApprovals: s.pendingApprovals.filter((a) => a.id !== approvalId),
    }));
  },

  resolveAllApprovals: (approved) => {
    for (const approval of get().pendingApprovals) {
      const resolver = approvalResolvers.get(approval.id);
      if (resolver) {
        resolver(approved);
        approvalResolvers.delete(approval.id);
      }
    }

    set({ pendingApprovals: [] });
  },

  clearApprovals: () => {
    clearApprovalResolvers();
    set({ pendingApprovals: [] });
  },

  // ─── History Management ─────────────────────────────────────────────────

  setViewingTask: async (meta) => {
    if (!meta) {
      viewingTaskRequestId++;
      set({ viewingTask: null, isLoadingViewingTask: false });
      return;
    }
    // Check if active task matches — no need to load from backend
    const active = get().activeTask;
    if (active && active.id === meta.id) {
      viewingTaskRequestId++;
      set({ viewingTask: active, isLoadingViewingTask: false });
      return;
    }
    // Lazy-load steps from backend (guarded against stale responses)
    const requestId = ++viewingTaskRequestId;
    set({ isLoadingViewingTask: true });
    try {
      const fullTask = await loadFullTask(meta);
      // Only apply if this is still the latest request
      if (requestId === viewingTaskRequestId) {
        set({ viewingTask: fullTask, isLoadingViewingTask: false });
      }
    } catch (e) {
      console.warn('[AgentStore] Failed to load task details:', e);
      if (requestId === viewingTaskRequestId) {
        set({ isLoadingViewingTask: false });
      }
    }
  },

  removeFromHistory: (taskId) => {
    set((s) => ({
      taskHistory: s.taskHistory.filter((t) => t.id !== taskId),
      viewingTask: s.viewingTask?.id === taskId ? null : s.viewingTask,
    }));
    api.agentHistoryDelete(taskId).catch((e) => {
      console.warn('[AgentStore] Failed to delete task from backend:', e);
    });
  },

  clearHistory: () => {
    set({ taskHistory: [], viewingTask: null });
    api.agentHistoryClear().catch((e) => {
      console.warn('[AgentStore] Failed to clear history in backend:', e);
    });
  },

  initHistory: async () => {
    try {
      // Load lightweight metadata only (no steps)
      const jsonList = await api.agentHistoryListMeta(50);
      const metas: AgentTaskMeta[] = [];
      for (const json of jsonList) {
        try {
          metas.push(JSON.parse(json) as AgentTaskMeta);
        } catch {
          console.warn('[AgentStore] Skipping unparseable task meta from backend');
        }
      }
      set({ taskHistory: metas });

      // Check for crash-recovery checkpoint
      const checkpoint = await api.agentHistoryLoadCheckpoint();
      if (checkpoint) {
        try {
          const recovered = JSON.parse(checkpoint) as AgentTask;
          console.info('[AgentStore] Recovered checkpoint for task:', recovered.id, recovered.goal);
          // Save recovered task as completed (crashed) and clear checkpoint
          const crashedTask = { ...recovered, status: 'failed' as const, error: 'Task interrupted (app crash recovery)', completedAt: Date.now() };
          const meta = buildTaskMeta(crashedTask);
          set((s) => ({
            taskHistory: [meta, ...s.taskHistory.filter(t => t.id !== meta.id)].slice(0, 50),
          }));
          await persistTask(crashedTask);
          await api.agentHistoryClearCheckpoint();
        } catch {
          console.warn('[AgentStore] Failed to parse checkpoint, clearing');
          await api.agentHistoryClearCheckpoint();
        }
      }
    } catch (e) {
      console.warn('[AgentStore] Failed to load task history from backend:', e);
    }
  },
}));
