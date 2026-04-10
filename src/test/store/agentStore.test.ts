import { beforeEach, describe, expect, it, vi } from 'vitest';
import { waitFor } from '@testing-library/react';

const apiMocks = vi.hoisted(() => ({
  agentHistorySaveMeta: vi.fn(),
  agentHistoryUpdateMeta: vi.fn(),
  agentHistoryListMeta: vi.fn(),
  agentHistoryAppendStep: vi.fn(),
  agentHistorySaveSteps: vi.fn(),
  agentHistoryGetSteps: vi.fn(),
  agentHistorySaveCheckpoint: vi.fn(),
  agentHistoryLoadCheckpoint: vi.fn(),
  agentHistoryClearCheckpoint: vi.fn(),
  agentHistoryDelete: vi.fn(),
  agentHistoryClear: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  api: apiMocks,
}));

function resetApiMocks(): void {
  Object.values(apiMocks).forEach((mockFn) => {
    mockFn.mockReset();
    mockFn.mockResolvedValue(undefined);
  });

  apiMocks.agentHistoryListMeta.mockResolvedValue([]);
  apiMocks.agentHistoryGetSteps.mockResolvedValue([]);
  apiMocks.agentHistoryLoadCheckpoint.mockResolvedValue(null);
}

async function loadAgentStore() {
  return import('@/store/agentStore');
}

describe('agentStore task history lifecycle', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    resetApiMocks();
  });

  it('adds completed tasks to history immediately and clears checkpoint', async () => {
    const { useAgentStore } = await loadAgentStore();
    const state = useAgentStore.getState();

    const task = state.startTask('inspect logs', 'provider-1', 'model-1');
    state.setTaskSummary('done');
    state.setTaskStatus('completed');

    expect(useAgentStore.getState().taskHistory[0]).toMatchObject({
      id: task.id,
      status: 'completed',
      summary: 'done',
    });

    await waitFor(() => {
      expect(apiMocks.agentHistorySaveMeta).toHaveBeenCalledTimes(1);
      expect(apiMocks.agentHistoryClearCheckpoint).toHaveBeenCalledTimes(1);
    });
  });

  it('adds cancelled tasks to history immediately and clears checkpoint', async () => {
    const { useAgentStore } = await loadAgentStore();
    const state = useAgentStore.getState();

    const task = state.startTask('restart service', 'provider-1', 'model-1');
    state.cancelTask();

    expect(useAgentStore.getState().taskHistory[0]).toMatchObject({
      id: task.id,
      status: 'cancelled',
    });

    await waitFor(() => {
      expect(apiMocks.agentHistorySaveMeta).toHaveBeenCalledTimes(1);
      expect(apiMocks.agentHistoryClearCheckpoint).toHaveBeenCalledTimes(1);
    });
  });

  it('archives an in-flight task as cancelled before starting a new one', async () => {
    const { useAgentStore } = await loadAgentStore();
    const state = useAgentStore.getState();

    const firstTask = state.startTask('first task', 'provider-1', 'model-1');
    const secondTask = state.startTask('second task', 'provider-1', 'model-1');

    expect(useAgentStore.getState().activeTask?.id).toBe(secondTask.id);
    expect(useAgentStore.getState().taskHistory[0]).toMatchObject({
      id: firstTask.id,
      status: 'cancelled',
    });

    await waitFor(() => {
      expect(apiMocks.agentHistorySaveMeta).toHaveBeenCalledTimes(1);
      expect(apiMocks.agentHistoryClearCheckpoint).toHaveBeenCalledTimes(1);
    });
  });

  it('does not duplicate a task in history when a completed task is archived again on next start', async () => {
    const { useAgentStore } = await loadAgentStore();
    const state = useAgentStore.getState();

    const firstTask = state.startTask('completed task', 'provider-1', 'model-1');
    state.setTaskStatus('completed');

    await waitFor(() => {
      expect(apiMocks.agentHistorySaveMeta).toHaveBeenCalledTimes(1);
    });

    state.startTask('next task', 'provider-1', 'model-1');

    const matching = useAgentStore.getState().taskHistory.filter((meta) => meta.id === firstTask.id);
    expect(matching).toHaveLength(1);
    expect(matching[0]?.status).toBe('completed');
  });

  it('aborts a nonterminal awaiting-approval task before starting a replacement task', async () => {
    const { useAgentStore } = await loadAgentStore();
    const task = useAgentStore.getState().startTask('blocked task', 'provider-1', 'model-1');
    const abort = vi.fn();

    useAgentStore.setState({
      activeTask: { ...task, status: 'awaiting_approval' },
      isRunning: false,
      abortController: { abort } as unknown as AbortController,
    });

    useAgentStore.getState().startTask('replacement task', 'provider-1', 'model-1');

    expect(abort).toHaveBeenCalledTimes(1);
  });

  it('aborts a nonterminal awaiting-approval task before resuming history', async () => {
    const { useAgentStore } = await loadAgentStore();
    const task = useAgentStore.getState().startTask('blocked task', 'provider-1', 'model-1');
    const abort = vi.fn();

    apiMocks.agentHistoryGetSteps.mockResolvedValueOnce([]);

    useAgentStore.setState({
      activeTask: { ...task, status: 'awaiting_approval' },
      isRunning: false,
      abortController: { abort } as unknown as AbortController,
      taskHistory: [{
        id: 'history-task',
        goal: 'resume me',
        status: 'completed',
        autonomyLevel: 'balanced',
        providerId: 'provider-1',
        model: 'model-1',
        currentRound: 0,
        maxRounds: 50,
        createdAt: Date.now(),
        completedAt: Date.now(),
        summary: null,
        error: null,
        stepCount: 0,
        planDescription: null,
        planJson: null,
        contextTabType: null,
      }],
    });

    await useAgentStore.getState().resumeHistoryTask('history-task');

    expect(abort).toHaveBeenCalledTimes(1);
  });
});