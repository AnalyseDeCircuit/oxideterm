import { describe, expect, it, vi } from 'vitest';

const aiFetchStreamingMock = vi.hoisted(() => vi.fn());
const aiFetchMock = vi.hoisted(() => vi.fn());

vi.mock('@/lib/ai/aiFetch', () => ({
  aiFetch: aiFetchMock,
  aiFetchStreaming: aiFetchStreamingMock,
}));

function makeStream(chunks: string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  return new ReadableStream<Uint8Array>({
    start(controller) {
      for (const chunk of chunks) {
        controller.enqueue(encoder.encode(chunk));
      }
      controller.close();
    },
  });
}

async function collectEvents(generator: AsyncGenerator<unknown>): Promise<unknown[]> {
  const events: unknown[] = [];
  for await (const event of generator) {
    events.push(event);
  }
  return events;
}

describe('provider streaming EOF handling', () => {
  it('openai provider processes the final SSE line without a trailing newline', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream([
        'data: {"choices":[{"delta":{"content":"hello"}}]}',
      ]),
    });

    const { openaiProvider } = await import('@/lib/ai/providers/openai');
    const events = await collectEvents(openaiProvider.streamCompletion({
      baseUrl: 'https://example.test',
      model: 'gpt-test',
      apiKey: 'key',
      tools: [],
    }, [{ role: 'user', content: 'hi' }], new AbortController().signal));

    expect(events).toContainEqual({ type: 'content', content: 'hello' });
    expect(events.at(-1)).toEqual({ type: 'done' });
  });

  it('openai provider flushes pending tool calls when the stream ends without [DONE]', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream([
        'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-1","function":{"name":"read_file","arguments":"{\\"path\\":\\"/tmp/a.txt\\"}"}}]}}]}',
      ]),
    });

    const { openaiProvider } = await import('@/lib/ai/providers/openai');
    const events = await collectEvents(openaiProvider.streamCompletion({
      baseUrl: 'https://example.test',
      model: 'gpt-test',
      apiKey: 'key',
      tools: [],
    }, [{ role: 'user', content: 'hi' }], new AbortController().signal));

    expect(events).toContainEqual({
      type: 'tool_call_complete',
      id: 'call-1',
      name: 'read_file',
      arguments: '{"path":"/tmp/a.txt"}',
    });
    expect(events.at(-1)).toEqual({ type: 'done' });
  });

  it('anthropic provider processes the final content block without a trailing newline', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream([
        'data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"review text"}}',
      ]),
    });

    const { anthropicProvider } = await import('@/lib/ai/providers/anthropic');
    const events = await collectEvents(anthropicProvider.streamCompletion({
      baseUrl: 'https://example.test',
      model: 'claude-test',
      apiKey: 'key',
      tools: [],
    }, [{ role: 'user', content: 'hi' }], new AbortController().signal));

    expect(events).toContainEqual({ type: 'content', content: 'review text' });
    expect(events.at(-1)).toEqual({ type: 'done' });
  });
});