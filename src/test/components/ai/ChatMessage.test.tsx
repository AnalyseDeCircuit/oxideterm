import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(),
}));

vi.mock('@/lib/markdownRenderer', () => ({
  markdownStyles: '',
  renderMarkdown: (content: string) => `<p>${content}</p>`,
  renderMathInElement: vi.fn(),
}));

vi.mock('@/hooks/useMermaid', () => ({
  useMermaid: vi.fn(),
}));

vi.mock('@/components/ai/ThinkingBlock', () => ({
  ThinkingBlock: ({ content }: { content: string }) => <div data-testid="thinking-block">{content}</div>,
}));

vi.mock('@/components/ai/ToolCallBlock', () => ({
  ToolCallBlock: ({ toolRounds }: { toolRounds?: Array<{ id: string }> }) => (
    <div data-testid="tool-call-block">{toolRounds?.[0]?.id ?? 'part-level'}</div>
  ),
}));

vi.mock('@/components/ai/GuardrailBlock', () => ({
  GuardrailBlock: ({ part }: { part: { message: string } }) => <div data-testid="guardrail-block">{part.message}</div>,
}));

vi.mock('@/components/ai/WarningBlock', () => ({
  WarningBlock: ({ part }: { part: { message: string } }) => <div data-testid="warning-block">{part.message}</div>,
}));

import { ChatMessage } from '@/components/ai/ChatMessage';
import type { AiChatMessage } from '@/types';

describe('ChatMessage', () => {
  it('renders assistant messages from turn-first fields before legacy fallbacks', () => {
    const message: AiChatMessage = {
      id: 'assistant-1',
      role: 'assistant',
      content: 'legacy content',
      thinkingContent: 'legacy thinking',
      timestamp: 1,
      turn: {
        id: 'assistant-1',
        status: 'complete',
        plainTextSummary: 'turn text',
        toolRounds: [
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
        ],
        parts: [
          { type: 'thinking', text: 'turn thinking' },
          { type: 'guardrail', code: 'tool-use-disabled', message: 'guardrail message' },
          { type: 'warning', code: 'tool-budget', message: 'warning message' },
          { type: 'text', text: 'turn text' },
        ],
      },
    };

    render(<ChatMessage message={message} />);

    expect(screen.getByTestId('thinking-block')).toHaveTextContent('turn thinking');
    expect(screen.getByTestId('guardrail-block')).toHaveTextContent('guardrail message');
    expect(screen.getByTestId('warning-block')).toHaveTextContent('warning message');
    expect(screen.getByTestId('tool-call-block')).toHaveTextContent('1');
    expect(screen.getByText('turn text')).toBeInTheDocument();
    expect(screen.queryByText('legacy content')).not.toBeInTheDocument();
    expect(screen.queryByText('legacy thinking')).not.toBeInTheDocument();
  });

  it('falls back to legacy content when a turn has no text or structured feedback parts', () => {
    const message: AiChatMessage = {
      id: 'assistant-2',
      role: 'assistant',
      content: 'legacy fallback content',
      timestamp: 2,
      turn: {
        id: 'assistant-2',
        status: 'complete',
        plainTextSummary: 'legacy fallback content',
        toolRounds: [],
        parts: [],
      },
    };

    render(<ChatMessage message={message} />);

    expect(screen.getByText('legacy fallback content')).toBeInTheDocument();
  });

  it('does not duplicate structured feedback through legacy content fallback', () => {
    const message: AiChatMessage = {
      id: 'assistant-3',
      role: 'assistant',
      content: 'guardrail only',
      timestamp: 3,
      turn: {
        id: 'assistant-3',
        status: 'complete',
        plainTextSummary: 'guardrail only',
        toolRounds: [],
        parts: [
          { type: 'guardrail', code: 'tool-use-disabled', message: 'guardrail only' },
        ],
      },
    };

    render(<ChatMessage message={message} />);

    expect(screen.getAllByText('guardrail only')).toHaveLength(1);
  });

  it('shows tool blocks for part-level tool calls before rounds are materialized', () => {
    const message: AiChatMessage = {
      id: 'assistant-4',
      role: 'assistant',
      content: '',
      timestamp: 4,
      turn: {
        id: 'assistant-4',
        status: 'streaming',
        plainTextSummary: '',
        toolRounds: [],
        parts: [
          {
            type: 'tool_call',
            id: 'tool-1',
            name: 'terminal_exec',
            argumentsText: '{"command":"pwd"}',
            status: 'complete',
          },
        ],
      },
    };

    render(<ChatMessage message={message} />);

    expect(screen.getByTestId('tool-call-block')).toHaveTextContent('part-level');
  });

  it('renders tool blocks in chronological order between text segments', () => {
    const message: AiChatMessage = {
      id: 'assistant-5',
      role: 'assistant',
      content: 'legacy content',
      timestamp: 5,
      turn: {
        id: 'assistant-5',
        status: 'complete',
        plainTextSummary: 'before middle after',
        toolRounds: [
          {
            id: 'round-1',
            round: 1,
            toolCalls: [
              { id: 'tool-1', name: 'read_file', argumentsText: '{"path":"/tmp/a"}', executionState: 'completed' },
            ],
          },
          {
            id: 'round-2',
            round: 2,
            toolCalls: [
              { id: 'tool-2', name: 'read_file', argumentsText: '{"path":"/tmp/b"}', executionState: 'completed' },
            ],
          },
        ],
        parts: [
          { type: 'text', text: 'before text' },
          { type: 'tool_call', id: 'tool-1', name: 'read_file', argumentsText: '{"path":"/tmp/a"}', status: 'complete' },
          { type: 'tool_result', toolCallId: 'tool-1', toolName: 'read_file', success: true, output: 'A' },
          { type: 'text', text: 'middle text' },
          { type: 'tool_call', id: 'tool-2', name: 'read_file', argumentsText: '{"path":"/tmp/b"}', status: 'complete' },
          { type: 'tool_result', toolCallId: 'tool-2', toolName: 'read_file', success: true, output: 'B' },
          { type: 'text', text: 'after text' },
        ],
      },
    };

    const { container } = render(<ChatMessage message={message} />);

    const before = screen.getByText('before text');
    const middle = screen.getByText('middle text');
    const after = screen.getByText('after text');
    const [firstToolBlock, secondToolBlock] = screen.getAllByTestId('tool-call-block');

    const orderedText = container.textContent ?? '';
    expect(orderedText.indexOf('before text')).toBeLessThan(orderedText.indexOf('round-1'));
    expect(orderedText.indexOf('round-1')).toBeLessThan(orderedText.indexOf('middle text'));
    expect(orderedText.indexOf('middle text')).toBeLessThan(orderedText.indexOf('round-2'));
    expect(orderedText.indexOf('round-2')).toBeLessThan(orderedText.indexOf('after text'));

    expect(before.compareDocumentPosition(firstToolBlock) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(firstToolBlock.compareDocumentPosition(middle) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(middle.compareDocumentPosition(secondToolBlock) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(secondToolBlock.compareDocumentPosition(after) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});