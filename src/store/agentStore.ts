/**
 * Agent Store — State management for AI Agent autonomous terminal operations
 *
 * Manages agent task lifecycle: planning → execution → verification.
 * The orchestrator runs in the background independently of UI components.
 */

import { create } from 'zustand';
import type { AgentTask, AgentStep, AgentApproval, AutonomyLevel, AgentTaskStatus, AgentPlan } from '../types';

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/** Max rounds per autonomy level */
export const MAX_ROUNDS: Record<AutonomyLevel, number> = {
  supervised: 20,
  balanced: 50,
  autonomous: 100,
};

// ═══════════════════════════════════════════════════════════════════════════
// Store Interface
// ═══════════════════════════════════════════════════════════════════════════

interface AgentStore {
  // ─── State ──────────────────────────────────────────────────────────────
  /** Currently running or most recent task */
  activeTask: AgentTask | null;
  /** Historical completed tasks */
  taskHistory: AgentTask[];
  /** Default autonomy level for new tasks */
  autonomyLevel: AutonomyLevel;
  /** Whether an agent is currently running */
  isRunning: boolean;
  /** Pending approval requests (UI reads this for approval bar) */
  pendingApprovals: AgentApproval[];
  /** AbortController for the current task */
  abortController: AbortController | null;

  // ─── Task Lifecycle ─────────────────────────────────────────────────────
  /** Start a new agent task */
  startTask: (goal: string, providerId: string, model: string) => AgentTask;
  /** Pause the current task */
  pauseTask: () => void;
  /** Resume a paused task */
  resumeTask: () => void;
  /** Cancel the current task */
  cancelTask: () => void;

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
  /** Resolve all pending approvals */
  resolveAllApprovals: (approved: boolean) => void;
  /** Clear all approvals */
  clearApprovals: () => void;

  // ─── History Management ─────────────────────────────────────────────────
  /** View a historical task (for replay) */
  viewingTask: AgentTask | null;
  /** Set task to view in replay mode */
  setViewingTask: (task: AgentTask | null) => void;
  /** Remove a task from history */
  removeFromHistory: (taskId: string) => void;
  /** Clear all task history */
  clearHistory: () => void;
}

// ═══════════════════════════════════════════════════════════════════════════
// Approval Resolvers (module-level, not in Zustand state)
// ═══════════════════════════════════════════════════════════════════════════

const approvalResolvers = new Map<string, (approved: boolean) => void>();

/** Register a resolver for a pending approval (called by orchestrator) */
export function registerApprovalResolver(
  approvalId: string,
  resolver: (approved: boolean) => void,
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

  // ─── Task Lifecycle ─────────────────────────────────────────────────────

  startTask: (goal, providerId, model) => {
    // Cancel any running task first
    const current = get();
    if (current.isRunning && current.abortController) {
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
        taskHistory: [taskToArchive, ...s.taskHistory].slice(0, 50),
      }));
    }

    const autonomyLevel = get().autonomyLevel;
    const task: AgentTask = {
      id: crypto.randomUUID(),
      goal,
      status: 'planning',
      autonomyLevel,
      providerId,
      model,
      plan: null,
      steps: [],
      currentRound: 0,
      maxRounds: MAX_ROUNDS[autonomyLevel],
      createdAt: Date.now(),
      completedAt: null,
      summary: null,
      error: null,
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

    set({
      activeTask: finishedTask,
      isRunning: false,
      pendingApprovals: [],
      abortController: null,
    });

    // Clear pending resolvers
    clearApprovalResolvers();
  },

  // ─── Settings ───────────────────────────────────────────────────────────

  setAutonomyLevel: (level) => set({ autonomyLevel: level }),

  // ─── Step Management ────────────────────────────────────────────────────

  appendStep: (step) => {
    set((s) => {
      if (!s.activeTask) return s;
      // Cap steps to prevent unbounded growth (keep last 200)
      const MAX_STEPS = 200;
      const existingSteps = s.activeTask.steps;
      const newSteps = existingSteps.length >= MAX_STEPS
        ? [...existingSteps.slice(-(MAX_STEPS - 1)), step]
        : [...existingSteps, step];
      return {
        activeTask: {
          ...s.activeTask,
          steps: newSteps,
        },
      };
    });
  },

  updateStep: (stepId, updates) => {
    set((s) => {
      if (!s.activeTask) return s;
      return {
        activeTask: {
          ...s.activeTask,
          steps: s.activeTask.steps.map((step) =>
            step.id === stepId ? { ...step, ...updates } : step
          ),
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
      return {
        activeTask: {
          ...s.activeTask,
          plan: {
            ...s.activeTask.plan,
            currentStepIndex: s.activeTask.plan.currentStepIndex + 1,
          },
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
      return {
        activeTask: {
          ...s.activeTask,
          status,
          completedAt: finished ? Date.now() : s.activeTask.completedAt,
        },
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
    set((s) => {
      if (!s.activeTask) return s;
      return {
        activeTask: { ...s.activeTask, error, status: 'failed', completedAt: Date.now() },
        isRunning: false,
        abortController: null,
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
    set({ pendingApprovals: [] });
  },

  // ─── History Management ─────────────────────────────────────────────────

  setViewingTask: (task) => {
    set({ viewingTask: task });
  },

  removeFromHistory: (taskId) => {
    set((s) => ({
      taskHistory: s.taskHistory.filter((t) => t.id !== taskId),
      viewingTask: s.viewingTask?.id === taskId ? null : s.viewingTask,
    }));
  },

  clearHistory: () => {
    set({ taskHistory: [], viewingTask: null });
  },
}));
