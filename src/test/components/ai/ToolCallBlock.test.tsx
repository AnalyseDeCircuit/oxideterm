import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => {
      const toolNames: Record<string, string> = {
        read_file: 'Read File',
        write_file: 'Write File',
        terminal_exec: 'Execute Command',
      };
      const riskLabels: Record<string, string> = {
        read: 'Read',
      };
      if (key === 'ai.tool_use.heading') return 'Tool Calls';
      if (key === 'ai.tool_use.arguments') return 'Arguments';
      if (key === 'ai.tool_use.output') return 'Output';
      if (key === 'ai.tool_use.summary') return 'Summary';
      if (key === 'ai.tool_use.target') return 'Target';
      if (key === 'ai.tool_use.warnings') return 'Warnings';
      if (key === 'ai.tool_use.structured_data') return 'Structured Data';
      if (key === 'ai.tool_use.raw_output') return 'Output';
      if (key === 'ai.tool_use.show_raw_output') return 'Show full output';
      if (key === 'ai.tool_use.show_more_preview') return 'Show more preview';
      if (key === 'ai.tool_use.output_truncated_with_full') return 'Output compacted; full stored';
      if (key === 'ai.tool_use.output_truncated_no_full') return 'Output compacted; full too large';
      if (key === 'ai.tool_use.output_stats') return `${String(options?.chars ?? 0)} chars, ${String(options?.lines ?? 0)} lines${String(options?.omitted ?? '')}`;
      if (key === 'ai.tool_use.approval_required') return 'Requires approval';
      if (key === 'ai.tool_use.bypass_badge') return 'Bypass';
      if (key === 'ai.tool_use.condensed') return `condensed ${String(options?.count ?? 0)}`;
      if (key === 'ai.tool_use.condensed_label') return 'Earlier calls';
      if (key.startsWith('ai.tool_use.tool_names.')) {
        const name = key.slice('ai.tool_use.tool_names.'.length);
        return toolNames[name] ?? String(options?.defaultValue ?? name);
      }
      if (key.startsWith('ai.tool_use.risk_labels.')) {
        const name = key.slice('ai.tool_use.risk_labels.'.length);
        return riskLabels[name] ?? String(options?.defaultValue ?? name);
      }
      return key;
    },
  }),
}));

vi.mock('@/store/aiChatStore', () => ({
  useAiChatStore: ((selector: (state: { resolveToolApproval: (id: string, approved: boolean) => void }) => unknown) => selector({
    resolveToolApproval: vi.fn(),
  })) as unknown,
}));

vi.mock('@/lib/ai/tools', () => ({
  hasDeniedCommands: vi.fn(() => false),
  sanitizeToolArguments: (value: unknown) => value,
  inferToolRisk: () => 'read',
  fromLegacyToolResult: (result: {
    success: boolean;
    toolName: string;
    output: string;
    error?: string;
    envelope?: unknown;
    durationMs?: number;
    truncated?: boolean;
  }) => result.envelope ?? ({
    ok: result.success,
    summary: result.error ?? result.output.split('\n')[0] ?? result.toolName,
    output: result.output,
    meta: {
      toolName: result.toolName,
      durationMs: result.durationMs ?? 0,
      truncated: result.truncated,
    },
  }),
}));

import { ToolCallBlock } from '@/components/ai/ToolCallBlock';

describe('ToolCallBlock', () => {
  it('renders tool calls derived from turn rounds when legacy toolCalls are absent', () => {
    render(
      <ToolCallBlock
        toolRounds={[
          {
            id: 'round-1',
            round: 1,
            toolCalls: [
              {
                id: 'tool-1',
                name: 'read_file',
                argumentsText: '{"path":"/tmp/demo.txt"}',
                executionState: 'completed',
              },
            ],
          },
        ]}
        turnParts={[
          {
            type: 'tool_result',
            toolCallId: 'tool-1',
            toolName: 'read_file',
            success: true,
            output: 'file body',
          },
        ]}
      />,
    );

    expect(screen.getByText('Tool Calls (1)')).toBeInTheDocument();
    expect(screen.getByText('Read File')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Read File/i }));

    expect(screen.getByText('Arguments')).toBeInTheDocument();
    expect(screen.getAllByText(/"path":\s*"\/tmp\/demo\.txt"/).length).toBeGreaterThan(0);
    expect(screen.getByText('Output')).toBeInTheDocument();
    expect(screen.getAllByText('file body').length).toBeGreaterThan(0);
  });

  it('renders envelope summary, badges, warnings, and structured data', () => {
    render(
      <ToolCallBlock
        toolCalls={[
          {
            id: 'tool-envelope',
            name: 'write_file',
            arguments: '{"path":"/tmp/demo.txt","content":"new"}',
            status: 'completed',
            result: {
              toolCallId: 'tool-envelope',
              toolName: 'write_file',
              success: true,
              output: 'raw output'.repeat(200),
              durationMs: 25,
              envelope: {
                ok: true,
                summary: 'Written 3 bytes to /tmp/demo.txt',
                output: 'raw output'.repeat(200),
                rawOutput: 'raw output'.repeat(200),
                warnings: ['unconditional overwrite'],
                data: { path: '/tmp/demo.txt', size: 3, contentHash: 'hash-1' },
                meta: {
                  toolName: 'write_file',
                  capability: 'filesystem.write',
                  targetId: 'ssh-node:node-1',
                  durationMs: 25,
                },
              },
            },
          },
        ]}
      />,
    );

    expect(screen.getByText('filesystem.write')).toBeInTheDocument();
    expect(screen.getByText(/Written 3 bytes/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Write File/i }));

    expect(screen.getByText('Warnings')).toBeInTheDocument();
    expect(screen.getByText('unconditional overwrite')).toBeInTheDocument();
    expect(screen.getByText('Structured Data')).toBeInTheDocument();
    expect(screen.getByText(/"contentHash":\s*"hash-1"/)).toBeInTheDocument();
    expect(screen.getByText('Show full output')).toBeInTheDocument();
  });

  it('uses envelope rawOutput when expanding full output', () => {
    render(
      <ToolCallBlock
        toolCalls={[
          {
            id: 'tool-long',
            name: 'terminal_exec',
            arguments: '{"command":"ls -la"}',
            status: 'completed',
            result: {
              toolCallId: 'tool-long',
              toolName: 'terminal_exec',
              success: true,
              output: 'HEAD\n[output truncated]\nTAIL',
              truncated: true,
              envelope: {
                ok: true,
                summary: 'Command completed',
                output: 'HEAD\n[output truncated]\nTAIL',
                rawOutput: 'HEAD\nfull middle output\nTAIL',
                outputPreview: {
                  strategy: 'head_tail',
                  charCount: 28,
                  lineCount: 3,
                  omittedChars: 9,
                  rawOutputStored: true,
                },
                meta: {
                  toolName: 'terminal_exec',
                  durationMs: 1,
                  truncated: true,
                },
              },
            },
          },
        ]}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /Execute Command/i }));
    expect(screen.queryByText(/full middle output/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByText('Show full output'));
    expect(screen.getByText(/full middle output/)).toBeInTheDocument();
    expect(screen.getByText(/Output compacted; full stored/)).toBeInTheDocument();
  });

  it('shows a prominent bypass badge for bypass-approved tool results', () => {
    render(
      <ToolCallBlock
        toolCalls={[
          {
            id: 'tool-bypass',
            name: 'terminal_exec',
            arguments: '{"command":"sudo reboot"}',
            status: 'completed',
            result: {
              toolCallId: 'tool-bypass',
              toolName: 'terminal_exec',
              success: true,
              output: 'ok',
              durationMs: 1,
              envelope: {
                ok: true,
                summary: 'Command completed',
                output: 'ok',
                meta: {
                  toolName: 'terminal_exec',
                  approvalMode: 'bypass',
                  durationMs: 1,
                },
              },
            },
          },
        ]}
      />,
    );

    expect(screen.getByText('Bypass')).toBeInTheDocument();
  });

  it('renders part-level tool calls when rounds are not available yet', () => {
    render(
      <ToolCallBlock
        turnParts={[
          {
            type: 'tool_call',
            id: 'tool-partial',
            name: 'terminal_exec',
            argumentsText: '{"command":"pwd"}',
            status: 'complete',
          },
        ]}
      />,
    );

    expect(screen.getByText('Tool Calls (1)')).toBeInTheDocument();
    expect(screen.getByText('Execute Command')).toBeInTheDocument();
  });

  it('formats dynamic MCP tool names instead of exposing raw internal ids', () => {
    render(
      <ToolCallBlock
        toolCalls={[
          {
            id: 'tool-mcp',
            name: 'mcp::filesystem::read_file',
            arguments: '{"path":"/tmp/demo.txt"}',
            status: 'completed',
            result: {
              toolCallId: 'tool-mcp',
              toolName: 'mcp::filesystem::read_file',
              success: true,
              output: 'ok',
              durationMs: 1,
            },
          },
        ]}
      />,
    );

    expect(screen.getByText('MCP: filesystem / Read File')).toBeInTheDocument();
    expect(screen.queryByText('mcp::filesystem::read_file')).not.toBeInTheDocument();
  });
});
