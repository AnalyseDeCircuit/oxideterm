import { beforeEach, describe, expect, it, vi } from 'vitest';

const appendStepMock = vi.hoisted(() => vi.fn());
const updateStepMock = vi.hoisted(() => vi.fn());
const setTaskStatusMock = vi.hoisted(() => vi.fn());
const addApprovalMock = vi.hoisted(() => vi.fn());
const addToastMock = vi.hoisted(() => vi.fn());
const executeToolMock = vi.hoisted(() => vi.fn());
const isCommandDeniedMock = vi.hoisted(() => vi.fn(() => false));
const hasDeniedCommandsMock = vi.hoisted(() => vi.fn((_toolName?: string, _args?: Record<string, unknown>) => false));

vi.mock('@/store/agentStore', () => ({
  useAgentStore: {
    getState: () => ({
      appendStep: appendStepMock,
      updateStep: updateStepMock,
      setTaskStatus: setTaskStatusMock,
      addApproval: addApprovalMock,
    }),
  },
  registerApprovalResolver: vi.fn(),
  removeApprovalResolver: vi.fn(),
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: {
    getState: () => ({
      settings: {
        ai: {
          toolUse: {
            autoApproveTools: {},
          },
        },
      },
    }),
  },
}));

vi.mock('@/hooks/useToast', () => ({
  useToastStore: {
    getState: () => ({
      addToast: addToastMock,
    }),
  },
}));

vi.mock('@/i18n', () => ({
  default: {
    t: (key: string) => key,
  },
}));

vi.mock('@/lib/ai/tools', () => ({
  executeTool: executeToolMock,
  READ_ONLY_TOOLS: new Set(['read_file', 'list_directory', 'grep_search']),
  isCommandDenied: isCommandDeniedMock,
  hasDeniedCommands: hasDeniedCommandsMock,
  sanitizeToolArguments: (value: unknown) => value,
  decideToolApproval: ({ toolName, args, autoApproveTools, readOnlyTools, autonomyLevel }: {
    toolName: string;
    args?: Record<string, unknown>;
    autoApproveTools?: Record<string, boolean>;
    readOnlyTools?: Set<string>;
    autonomyLevel?: string;
  }) => {
    if (hasDeniedCommandsMock(toolName, args ?? {})) {
      return { risk: 'destructive', autoApprove: false, requiresApproval: true, reason: 'high-risk' };
    }
    if (autonomyLevel === 'supervised') {
      return { risk: 'read', autoApprove: false, requiresApproval: true, reason: 'supervised' };
    }
    if (autoApproveTools?.[toolName] === true || readOnlyTools?.has(toolName) || autonomyLevel === 'autonomous') {
      return { risk: readOnlyTools?.has(toolName) ? 'read' : 'execute-command', autoApprove: true, requiresApproval: false, reason: 'tool-auto-approved' };
    }
    return { risk: 'execute-command', autoApprove: false, requiresApproval: true, reason: 'manual' };
  },
}));

describe('roleRunner.streamCompletion', () => {
  it('builds a structured assistant turn from streamed provider events', async () => {
    const { streamCompletion } = await import('@/lib/ai/roles');

    const provider = {
      type: 'openai' as const,
      displayName: 'Mock Provider',
      async *streamCompletion() {
        yield { type: 'thinking' as const, content: 'reasoning ' };
        yield { type: 'content' as const, content: 'hello ' };
        yield { type: 'tool_call' as const, id: 'tool-1', name: 'read_file', arguments: '{"path":"/tmp/a"' };
        yield { type: 'tool_call_complete' as const, id: 'tool-1', name: 'read_file', arguments: '{"path":"/tmp/a"}' };
        yield { type: 'content' as const, content: 'world' };
        yield { type: 'done' as const };
      },
    };

    const result = await streamCompletion(
      {
        provider,
        baseUrl: 'https://example.com',
        model: 'mock-model',
        apiKey: 'mock-key',
      },
      [{ role: 'user', content: 'read file' }],
      [],
      new AbortController().signal,
    );

    expect(result.text).toBe('hello world');
    expect(result.thinkingContent).toBe('reasoning ');
    expect(result.toolCalls).toEqual([
      { id: 'tool-1', name: 'read_file', arguments: '{"path":"/tmp/a"}' },
    ]);
    expect(result.turn.plainTextSummary).toBe('hello world');
    expect(result.turn.status).toBe('complete');
    expect(result.turn.parts).toEqual([
      { type: 'thinking', text: 'reasoning ', streaming: false },
      { type: 'text', text: 'hello ' },
      { type: 'tool_call', id: 'tool-1', name: 'read_file', argumentsText: '{"path":"/tmp/a"}', status: 'complete' },
      { type: 'text', text: 'world' },
    ]);
    expect(result.toolRounds).toEqual([
      expect.objectContaining({
        round: 1,
        responseText: 'hello world',
        toolCalls: [
          expect.objectContaining({
            id: 'tool-1',
            name: 'read_file',
            argumentsText: '{"path":"/tmp/a"}',
            executionState: 'pending',
          }),
        ],
      }),
    ]);
  });

  it('runSingleShot keeps the structured turn while preserving legacy text fields', async () => {
    const { runSingleShot } = await import('@/lib/ai/roles');

    const provider = {
      type: 'openai' as const,
      displayName: 'Mock Provider',
      async *streamCompletion() {
        yield { type: 'thinking' as const, content: 'plan' };
        yield { type: 'content' as const, content: 'answer' };
        yield { type: 'done' as const };
      },
    };

    const result = await runSingleShot(
      {
        provider,
        baseUrl: 'https://example.com',
        model: 'mock-model',
        apiKey: 'mock-key',
      },
      [{ role: 'user', content: 'do a plan' }],
      new AbortController().signal,
    );

    expect(result.text).toBe('answer');
    expect(result.thinkingContent).toBe('plan');
    expect(result.turn.plainTextSummary).toBe('answer');
    expect(result.turn.parts).toEqual([
      { type: 'thinking', text: 'plan', streaming: false },
      { type: 'text', text: 'answer' },
    ]);
  });

  it('throws provider errors instead of swallowing them', async () => {
    const { streamCompletion } = await import('@/lib/ai/roles');

    const provider = {
      type: 'openai' as const,
      displayName: 'Mock Provider',
      async *streamCompletion() {
        yield { type: 'content' as const, content: 'partial' };
        yield { type: 'error' as const, message: 'rate limited' };
      },
    };

    await expect(streamCompletion(
      {
        provider,
        baseUrl: 'https://example.com',
        model: 'mock-model',
        apiKey: 'mock-key',
      },
      [{ role: 'user', content: 'fail please' }],
      [],
      new AbortController().signal,
    )).rejects.toThrow('rate limited');
  });

  it('emits shared diagnostic events for requests and completed rounds', async () => {
    const { streamCompletion } = await import('@/lib/ai/roles');
    const onEvent = vi.fn();

    const provider = {
      type: 'openai' as const,
      displayName: 'Mock Provider',
      async *streamCompletion() {
        yield { type: 'content' as const, content: 'answer' };
        yield { type: 'done' as const };
      },
    };

    await streamCompletion(
      {
        provider,
        baseUrl: 'https://example.com',
        model: 'mock-model',
        apiKey: 'mock-key',
      },
      [{ role: 'user', content: 'diagnose' }],
      [],
      new AbortController().signal,
      {
        conversationId: 'lineage-1',
        turnId: 'task-1',
        logicalRound: 2,
        requestKind: 'execute',
        telemetryBase: {
          source: 'agent',
          providerId: 'provider-1',
          model: 'mock-model',
          runId: 'task-1',
        },
        onEvent,
      },
    );

    expect(onEvent).toHaveBeenCalledWith(expect.objectContaining({
      type: 'llm_request',
      conversationId: 'lineage-1',
      turnId: 'task-1',
      data: expect.objectContaining({
        source: 'agent',
        requestKind: 'execute',
        logicalRound: 2,
      }),
    }));
    expect(onEvent).toHaveBeenCalledWith(expect.objectContaining({
      type: 'assistant_round',
      conversationId: 'lineage-1',
      turnId: 'task-1',
      data: expect.objectContaining({
        source: 'agent',
        requestKind: 'execute',
        logicalRound: 2,
        responseLength: 6,
      }),
    }));
  });
});

describe('roleRunner.processToolCalls', () => {
  beforeEach(() => {
    appendStepMock.mockReset();
    updateStepMock.mockReset();
    setTaskStatusMock.mockReset();
    addApprovalMock.mockReset();
    addToastMock.mockReset();
    executeToolMock.mockReset();
    isCommandDeniedMock.mockReset();
    isCommandDeniedMock.mockReturnValue(false);
    hasDeniedCommandsMock.mockReset();
    hasDeniedCommandsMock.mockReturnValue(false);
  });

  it('returns synthetic tool errors for calls beyond the per-round limit', async () => {
    const { processToolCalls } = await import('@/lib/ai/roles');
    const { MAX_TOOL_CALLS_PER_ROUND } = await import('@/lib/ai/agentConfig');

    executeToolMock.mockResolvedValue({
      toolCallId: 'ok',
      toolName: 'read_file',
      success: true,
      output: 'content',
    });

    const toolCalls = Array.from({ length: MAX_TOOL_CALLS_PER_ROUND + 2 }, (_, idx) => ({
      id: `tool-${idx}`,
      name: 'read_file',
      arguments: JSON.stringify({ path: `/tmp/${idx}.txt` }),
    }));

    const result = await processToolCalls(
      toolCalls,
      0,
      {
        id: 'task-1',
        goal: 'inspect files',
        status: 'executing',
        autonomyLevel: 'balanced',
        providerId: 'provider-1',
        model: 'model-1',
        plan: null,
        steps: [],
        currentRound: 0,
        maxRounds: 10,
        createdAt: Date.now(),
        completedAt: null,
        summary: null,
        error: null,
        lineageId: 'lineage-1',
        resetCount: 0,
        activeContract: null,
        lastReview: null,
        handoffFromTaskId: null,
        lineageArtifacts: [],
      },
      { activeNodeId: null, activeAgentAvailable: false, skipFocus: true },
      'balanced',
      new AbortController().signal,
    );

    expect(executeToolMock).toHaveBeenCalledTimes(MAX_TOOL_CALLS_PER_ROUND);
    expect(result.results).toHaveLength(toolCalls.length);
    expect(result.results.slice(-2)).toEqual([
      expect.objectContaining({ tool_call_id: `tool-${MAX_TOOL_CALLS_PER_ROUND}`, content: expect.stringContaining('Too many tool calls in one round') }),
      expect.objectContaining({ tool_call_id: `tool-${MAX_TOOL_CALLS_PER_ROUND + 1}`, content: expect.stringContaining('Too many tool calls in one round') }),
    ]);
    expect(result.allSucceeded).toBe(false);
    expect(appendStepMock).toHaveBeenCalledWith(expect.objectContaining({ type: 'error' }));
  });

  it('auto-approves read-only tools in balanced mode', async () => {
    const { shouldAutoApprove } = await import('@/lib/ai/roles');

    expect(shouldAutoApprove('read_file', { path: '/tmp/demo.txt' }, 'balanced')).toBe(true);
  });

  it('uses the live autonomy level for auto-approval instead of the stale task snapshot', async () => {
    const { processToolCalls } = await import('@/lib/ai/roles');

    executeToolMock.mockResolvedValue({
      toolCallId: 'write-1',
      toolName: 'write_file',
      success: true,
      output: 'written',
    });

    const result = await processToolCalls(
      [{ id: 'tool-1', name: 'write_file', arguments: JSON.stringify({ path: '/tmp/demo.txt', content: 'hello' }) }],
      0,
      {
        id: 'task-1',
        goal: 'update file',
        status: 'executing',
        autonomyLevel: 'supervised',
        providerId: 'provider-1',
        model: 'model-1',
        plan: null,
        steps: [],
        currentRound: 0,
        maxRounds: 10,
        createdAt: Date.now(),
        completedAt: null,
        summary: null,
        error: null,
        lineageId: 'lineage-1',
        resetCount: 0,
        activeContract: null,
        lastReview: null,
        handoffFromTaskId: null,
        lineageArtifacts: [],
      },
      { activeNodeId: null, activeAgentAvailable: false, skipFocus: true },
      'autonomous',
      new AbortController().signal,
    );

    expect(addApprovalMock).not.toHaveBeenCalled();
    expect(addToastMock).not.toHaveBeenCalled();
    expect(executeToolMock).toHaveBeenCalledOnce();
    expect(result.allSucceeded).toBe(true);
  });

  it('never auto-approves deny-listed command payloads', async () => {
    const { shouldAutoApprove } = await import('@/lib/ai/roles');
    hasDeniedCommandsMock.mockReturnValue(true);

    expect(shouldAutoApprove('terminal_exec', { command: 'sudo reboot' }, 'autonomous')).toBe(false);
    expect(shouldAutoApprove('batch_exec', { commands: ['echo ok', 'sudo reboot'] }, 'balanced')).toBe(false);
  });
});
