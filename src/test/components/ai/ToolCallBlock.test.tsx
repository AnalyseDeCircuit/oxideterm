import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => {
      if (key === 'ai.tool_use.heading') return 'Tool Calls';
      if (key === 'ai.tool_use.arguments') return 'Arguments';
      if (key === 'ai.tool_use.output') return 'Output';
      if (key === 'ai.tool_use.approval_required') return 'Requires approval';
      if (key === 'ai.tool_use.condensed') return `condensed ${String(options?.count ?? 0)}`;
      if (key === 'ai.tool_use.condensed_label') return 'Earlier calls';
      if (key.startsWith('ai.tool_use.tool_names.')) return key.split('.').at(-1) ?? key;
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
    expect(screen.getByText('read_file')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /read_file/i }));

    expect(screen.getByText('Arguments')).toBeInTheDocument();
    expect(screen.getByText(/"path":\s*"\/tmp\/demo\.txt"/)).toBeInTheDocument();
    expect(screen.getByText('Output')).toBeInTheDocument();
    expect(screen.getByText('file body')).toBeInTheDocument();
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
    expect(screen.getByText('terminal_exec')).toBeInTheDocument();
  });
});
