import { describe, expect, it, beforeEach } from 'vitest';

import {
  applyAiResultBindingGuard,
  aiToolResultFactsForMessage,
  buildAiToolExecutionRecord,
  clearAiToolEvidenceLedger,
  extractAiToolResultFacts,
  recordAiToolExecution,
  recordAiToolResultFacts,
} from '@/lib/ai/orchestrator/evidenceBinding';
import type { AiChatMessage, AiToolResult } from '@/types';

function assistantMessage(content: string): AiChatMessage {
  return {
    id: 'assistant-1',
    role: 'assistant',
    content,
    timestamp: 1,
    turn: {
      id: 'assistant-1',
      status: 'complete',
      parts: [{ type: 'text', text: content }],
      toolRounds: [],
      plainTextSummary: content,
    },
  };
}

function toolResult(toolCallId: string, output: string): AiToolResult {
  return {
    toolCallId,
    toolName: 'run_command',
    success: true,
    output,
    envelope: {
      ok: true,
      summary: 'Command completed.',
      output,
      execution: {
        kind: 'terminal',
        target: { id: 'terminal-session:s1', kind: 'terminal-session' },
        exitCode: 0,
        visibleInTerminal: true,
        state: 'output_captured',
      },
      meta: {
        toolName: 'run_command',
        durationMs: 1,
        verified: true,
        runtimeEpoch: 'epoch-test',
      },
    },
  };
}

describe('AI evidence binding', () => {
  beforeEach(() => {
    clearAiToolEvidenceLedger();
  });

  it('extracts output and execution facts from tool results', () => {
    const result = toolResult('tool-1', 'Filesystem Size Used\n/ 468G 72G');
    const record = buildAiToolExecutionRecord({
      conversationId: 'conversation-1',
      assistantMessageId: 'assistant-1',
      toolCallId: result.toolCallId,
      toolName: result.toolName,
      args: { target_id: 'terminal-session:s1', command: 'df -h /' },
      status: 'completed',
      result,
      risk: 'execute',
    });

    const facts = extractAiToolResultFacts(record, result, 42);

    expect(facts.map((fact) => fact.factId)).toEqual(expect.arrayContaining([
      'tool-1.output',
      'tool-1.execution.exit_code',
      'tool-1.execution.visible_in_terminal',
      'tool-1.execution.state',
    ]));
    expect(facts.every((fact) => fact.textHash.startsWith('fnv1a64:'))).toBe(true);
    expect(facts.find((fact) => fact.factId === 'tool-1.output')?.outputPreview).toContain('468G');
  });

  it('accepts structured evidence claims backed by current-turn facts', () => {
    const result = toolResult('tool-1', 'Filesystem Size Used Avail Use%\n/ 468G 72G 373G 17%');
    const record = buildAiToolExecutionRecord({
      conversationId: 'conversation-1',
      assistantMessageId: 'assistant-1',
      toolCallId: result.toolCallId,
      toolName: result.toolName,
      args: {},
      status: 'completed',
      result,
      risk: 'execute',
    });
    recordAiToolExecution(record);
    recordAiToolResultFacts(record, result);
    const message = assistantMessage([
      '磁盘是 468G，已用 72G。',
      '<evidence_claims>{"claims":[{"text":"磁盘是 468G，已用 72G。","evidence":["tool-1.output"],"confidence":"verified"}]}</evidence_claims>',
    ].join('\n'));

    const guarded = applyAiResultBindingGuard(
      message,
      aiToolResultFactsForMessage('conversation-1', 'assistant-1'),
    );

    expect(guarded.guardrail).toBeUndefined();
    expect(guarded.message.content).toBe('磁盘是 468G，已用 72G。');
    expect(guarded.message.turn?.parts.map((part) => part.type)).toEqual(['text', 'claim']);
  });

  it('rejects claims that cite old assistant-turn facts', () => {
    const oldResult = toolResult('old-tool', 'Filesystem Size Used\n/ 468G 72G');
    const oldRecord = buildAiToolExecutionRecord({
      conversationId: 'conversation-1',
      assistantMessageId: 'assistant-old',
      toolCallId: oldResult.toolCallId,
      toolName: oldResult.toolName,
      args: {},
      status: 'completed',
      result: oldResult,
      risk: 'execute',
    });
    recordAiToolResultFacts(oldRecord, oldResult);
    const message = assistantMessage('磁盘是 468G，已用 72G。');

    const guarded = applyAiResultBindingGuard(
      message,
      aiToolResultFactsForMessage('conversation-1', 'assistant-1'),
    );

    expect(guarded.guardrail).toBeDefined();
    expect(guarded.message.turn?.parts[0]).toMatchObject({
      type: 'guardrail',
      code: 'result-binding-required',
    });
  });

  it('keeps unstructured replies when the current turn has tool facts', () => {
    const result = toolResult('tool-1', 'ok');
    const record = buildAiToolExecutionRecord({
      conversationId: 'conversation-1',
      assistantMessageId: 'assistant-1',
      toolCallId: result.toolCallId,
      toolName: result.toolName,
      args: {},
      status: 'completed',
      result,
      risk: 'execute',
    });
    recordAiToolResultFacts(record, result);

    const message = assistantMessage('我已经执行了命令，输出正常。');
    const guarded = applyAiResultBindingGuard(
      message,
      aiToolResultFactsForMessage('conversation-1', 'assistant-1'),
    );

    expect(guarded.guardrail).toBeUndefined();
    expect(guarded.message.content).toBe(message.content);
  });
});
