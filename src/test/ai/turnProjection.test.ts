import { describe, expect, it } from 'vitest';

import { projectTurnToLegacyMessageFields } from '@/lib/ai/turnModel/turnProjection';
import type { AiAssistantTurn } from '@/lib/ai/turnModel/types';

describe('turnProjection', () => {
  it('projects text, thinking, and tool calls without leaking guardrail raw text', () => {
    const turn: AiAssistantTurn = {
      id: 'turn-1',
      status: 'complete',
      plainTextSummary: 'final answer',
      parts: [
        { type: 'thinking', text: 'first thought ' },
        { type: 'text', text: 'final answer' },
        {
          type: 'guardrail',
          code: 'pseudo-tool-transcript',
          message: 'blocked',
          rawText: '{"name":"terminal_exec"}',
        },
        {
          type: 'tool_result',
          toolCallId: 'call-1',
          toolName: 'terminal_exec',
          success: true,
          output: 'ok',
        },
      ],
      toolRounds: [
        {
          id: 'round-1',
          round: 1,
          toolCalls: [
            {
              id: 'call-1',
              name: 'terminal_exec',
              argumentsText: '{"command":"pwd"}',
              approvalState: 'approved',
              executionState: 'completed',
            },
          ],
        },
      ],
    };

    const projected = projectTurnToLegacyMessageFields(turn);

    expect(projected.content).toBe('final answer');
    expect(projected.thinkingContent).toBe('first thought ');
    expect(projected.toolCalls).toEqual([
      expect.objectContaining({
        id: 'call-1',
        name: 'terminal_exec',
        arguments: '{"command":"pwd"}',
        status: 'completed',
        result: expect.objectContaining({ output: 'ok', success: true }),
      }),
    ]);
  });

  it('falls back to guardrail content when no text parts exist', () => {
    const turn: AiAssistantTurn = {
      id: 'turn-guardrail',
      status: 'error',
      plainTextSummary: 'guardrail blocked the response',
      parts: [
        {
          type: 'guardrail',
          code: 'pseudo-tool-transcript',
          message: 'tool transcript blocked',
          rawText: '{"name":"terminal_exec"}',
        },
      ],
      toolRounds: [],
    };

    const projected = projectTurnToLegacyMessageFields(turn);

    expect(projected.content).toBe('tool transcript blocked');
    expect(projected.thinkingContent).toBeUndefined();
    expect(projected.toolCalls).toBeUndefined();
  });

  it('projects part-level tool calls when rounds are not available yet', () => {
    const turn: AiAssistantTurn = {
      id: 'turn-streaming-call',
      status: 'streaming',
      plainTextSummary: 'running tool',
      parts: [
        {
          type: 'tool_call',
          id: 'call-stream',
          name: 'read_file',
          argumentsText: '{"path":"/tmp/demo.txt"}',
          status: 'complete',
        },
      ],
      toolRounds: [],
    };

    const projected = projectTurnToLegacyMessageFields(turn);

    expect(projected.toolCalls).toEqual([
      expect.objectContaining({
        id: 'call-stream',
        name: 'read_file',
        arguments: '{"path":"/tmp/demo.txt"}',
        status: 'running',
      }),
    ]);
  });

  it('marks part-level tool calls as completed when a tool result already exists', () => {
    const turn: AiAssistantTurn = {
      id: 'turn-part-result',
      status: 'complete',
      plainTextSummary: 'tool finished',
      parts: [
        {
          type: 'tool_call',
          id: 'call-result',
          name: 'read_file',
          argumentsText: '{"path":"/tmp/demo.txt"}',
          status: 'complete',
        },
        {
          type: 'tool_result',
          toolCallId: 'call-result',
          toolName: 'read_file',
          success: true,
          output: 'done',
        },
      ],
      toolRounds: [],
    };

    const projected = projectTurnToLegacyMessageFields(turn);

    expect(projected.toolCalls).toEqual([
      expect.objectContaining({
        id: 'call-result',
        status: 'completed',
        result: expect.objectContaining({ output: 'done', success: true }),
      }),
    ]);
  });

  it('preserves rejected status when a rejected tool call also has a failure result', () => {
    const turn: AiAssistantTurn = {
      id: 'turn-rejected',
      status: 'complete',
      plainTextSummary: 'rejected tool',
      parts: [
        { type: 'text', text: 'tool was rejected' },
        {
          type: 'tool_result',
          toolCallId: 'call-rejected',
          toolName: 'terminal_exec',
          success: false,
          output: '',
          error: 'Tool disabled',
        },
      ],
      toolRounds: [
        {
          id: 'round-rejected',
          round: 1,
          toolCalls: [
            {
              id: 'call-rejected',
              name: 'terminal_exec',
              argumentsText: '{"command":"pwd"}',
              approvalState: 'rejected',
              executionState: 'error',
            },
          ],
        },
      ],
    };

    const projected = projectTurnToLegacyMessageFields(turn);

    expect(projected.toolCalls).toEqual([
      expect.objectContaining({
        id: 'call-rejected',
        arguments: '{"command":"pwd"}',
        status: 'rejected',
      }),
    ]);
  });
});