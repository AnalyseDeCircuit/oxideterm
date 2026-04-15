import { describe, expect, it } from 'vitest';

import { createTurnAccumulator } from '@/lib/ai/turnModel/turnAccumulator';

describe('turnAccumulator', () => {
  it('accumulates content, thinking, and tool lifecycle into a structured turn snapshot', () => {
    const accumulator = createTurnAccumulator({ turnId: 'turn-1' });

    accumulator.onThinking('plan ');
    accumulator.onContent('answer');
    accumulator.startRound(1);
    accumulator.onToolCallPartial({
      id: 'call-1',
      name: 'read_file',
      argumentsText: '{"path":"/tmp/demo.txt"}',
    });
    accumulator.onToolCallComplete({
      id: 'call-1',
      name: 'read_file',
      argumentsText: '{"path":"/tmp/demo.txt"}',
    });
    accumulator.syncToolCalls([
      {
        id: 'call-1',
        name: 'read_file',
        arguments: '{"path":"/tmp/demo.txt"}',
        status: 'running',
      },
    ]);
    accumulator.onToolResult({
      toolCallId: 'call-1',
      toolName: 'read_file',
      success: true,
      output: 'done',
    });
    accumulator.syncToolCalls([
      {
        id: 'call-1',
        name: 'read_file',
        arguments: '{"path":"/tmp/demo.txt"}',
        status: 'completed',
        result: {
          toolCallId: 'call-1',
          toolName: 'read_file',
          success: true,
          output: 'done',
        },
      },
    ]);
    accumulator.setStatus('complete');

    const snapshot = accumulator.snapshot();

    expect(snapshot.status).toBe('complete');
    expect(snapshot.parts.map((part) => part.type)).toEqual(['thinking', 'text', 'tool_call', 'tool_result']);
    expect(snapshot.parts[0]).toMatchObject({ type: 'thinking', streaming: false });
    expect(snapshot.parts[2]).toMatchObject({
      type: 'tool_call',
      id: 'call-1',
      status: 'complete',
    });
    expect(snapshot.toolRounds).toEqual([
      expect.objectContaining({
        round: 1,
        toolCalls: [
          expect.objectContaining({
            id: 'call-1',
            executionState: 'completed',
          }),
        ],
      }),
    ]);
    expect(snapshot.plainTextSummary).toBe('answer');
  });

  it('merges partial and complete updates for the same tool call instead of duplicating parts', () => {
    const accumulator = createTurnAccumulator({ turnId: 'turn-2' });

    accumulator.startRound(1);
    accumulator.onToolCallPartial({
      id: 'call-2',
      name: 'terminal_exec',
      argumentsText: '{"command":"pwd"}',
    });
    accumulator.onToolCallComplete({
      id: 'call-2',
      name: 'terminal_exec',
      argumentsText: '{"command":"pwd"}',
    });

    const snapshot = accumulator.snapshot();
    const toolCallParts = snapshot.parts.filter((part) => part.type === 'tool_call');

    expect(toolCallParts).toHaveLength(1);
    expect(toolCallParts[0]).toMatchObject({
      id: 'call-2',
      status: 'complete',
    });
  });

  it('opens a new round when later tool calls belong to a different round number', () => {
    const accumulator = createTurnAccumulator({ turnId: 'turn-3' });

    accumulator.startRound(1);
    accumulator.onToolCallComplete({
      id: 'call-1',
      name: 'read_file',
      argumentsText: '{"path":"/tmp/one"}',
    });
    accumulator.startRound(2);
    accumulator.onToolCallComplete({
      id: 'call-2',
      name: 'read_file',
      argumentsText: '{"path":"/tmp/two"}',
    });

    const snapshot = accumulator.snapshot();

    expect(snapshot.toolRounds).toHaveLength(2);
    expect(snapshot.toolRounds[0]).toMatchObject({ round: 1, toolCalls: [expect.objectContaining({ id: 'call-1' })] });
    expect(snapshot.toolRounds[1]).toMatchObject({ round: 2, toolCalls: [expect.objectContaining({ id: 'call-2' })] });
  });

  it('can set and clear a transient round marker without affecting other round data', () => {
    const accumulator = createTurnAccumulator({ turnId: 'turn-marker' });

    const round = accumulator.startRound(1);
    accumulator.onToolCallComplete({
      id: 'call-marker',
      name: 'local_exec',
      argumentsText: '{"command":"pwd"}',
    });

    accumulator.setRoundStatefulMarker(round.id, 'awaiting-summary');
    expect(accumulator.snapshot().toolRounds[0]).toMatchObject({
      id: round.id,
      statefulMarker: 'awaiting-summary',
      toolCalls: [expect.objectContaining({ id: 'call-marker' })],
    });

    accumulator.setRoundStatefulMarker(round.id, undefined);
    expect(accumulator.snapshot().toolRounds[0]).toMatchObject({
      id: round.id,
      statefulMarker: undefined,
      toolCalls: [expect.objectContaining({ id: 'call-marker' })],
    });
  });

  it('captures error parts and marks the snapshot as error', () => {
    const accumulator = createTurnAccumulator({ turnId: 'turn-4' });

    accumulator.onThinking('partial reasoning');
    accumulator.onError('provider failed');

    const snapshot = accumulator.snapshot();

    expect(snapshot.status).toBe('error');
    expect(snapshot.parts).toEqual([
      { type: 'thinking', text: 'partial reasoning', streaming: true },
      { type: 'error', message: 'provider failed' },
    ]);
  });
});