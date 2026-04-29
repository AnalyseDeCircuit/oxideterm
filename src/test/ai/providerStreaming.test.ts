import { beforeEach, describe, expect, it, vi } from 'vitest';

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

function getLastRequestBody(): Record<string, unknown> {
  const init = aiFetchStreamingMock.mock.calls.at(-1)?.[1];
  return JSON.parse(init.body);
}

describe('provider streaming EOF handling', () => {
  beforeEach(() => {
    aiFetchMock.mockReset();
    aiFetchStreamingMock.mockReset();
  });

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

  it('openai-compatible payload merges all system messages into the first message', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream(['data: [DONE]']),
    });

    const { openaiCompatibleProvider } = await import('@/lib/ai/providers/openai');
    await collectEvents(openaiCompatibleProvider.streamCompletion({
      baseUrl: 'https://vllm.example.test/v1',
      model: 'qwen-local',
      apiKey: 'key',
      tools: [],
    }, [
      { role: 'system', content: 'Agent role' },
      { role: 'user', content: 'hello' },
      { role: 'system', content: 'Current terminal context' },
      { role: 'assistant', content: 'previous answer' },
      { role: 'system', content: 'Reminder' },
    ], new AbortController().signal));

    const messages = getLastRequestBody().messages as Array<Record<string, unknown>>;
    expect(messages).toEqual([
      { role: 'system', content: 'Agent role\n\nCurrent terminal context\n\nReminder' },
      { role: 'user', content: 'hello' },
      { role: 'assistant', content: 'previous answer' },
    ]);
  });

  it('openai-compatible model listing falls back to /v1 when root returns HTML', async () => {
    aiFetchMock
      .mockResolvedValueOnce({ ok: true, status: 200, body: '<html>not json</html>' })
      .mockResolvedValueOnce({ ok: true, status: 200, body: JSON.stringify({ data: [{ id: 'model-b' }, { id: 'model-a' }] }) });

    const { openaiCompatibleProvider } = await import('@/lib/ai/providers/openai');
    const models = await openaiCompatibleProvider.fetchModels?.({
      baseUrl: 'https://api.kr777.top',
      apiKey: 'key',
    });

    expect(models).toEqual(['model-a', 'model-b']);
    expect(aiFetchMock.mock.calls[0]?.[0]).toBe('https://api.kr777.top/models');
    expect(aiFetchMock.mock.calls[1]?.[0]).toBe('https://api.kr777.top/v1/models');
  });

  it('openai-compatible model listing reports HTML responses as base URL errors', async () => {
    aiFetchMock.mockResolvedValueOnce({ ok: true, status: 200, body: '<html>not json</html>' });

    const { openaiCompatibleProvider } = await import('@/lib/ai/providers/openai');
    await expect(openaiCompatibleProvider.fetchModels?.({
      baseUrl: 'https://api.example.test/v1',
      apiKey: 'key',
    })).rejects.toThrow(/returned HTML instead of JSON.*Base URL.*\/v1/);
  });

  it('openai-compatible streaming falls back to /v1 chat completions for root base URLs', async () => {
    aiFetchStreamingMock
      .mockReturnValueOnce({
        response: Promise.resolve({ ok: false, status: 404 }),
        body: makeStream(['<html>not found</html>']),
      })
      .mockReturnValueOnce({
        response: Promise.resolve({ ok: true, status: 200 }),
        body: makeStream(['data: {"choices":[{"delta":{"content":"ok"}}]}\n\ndata: [DONE]\n\n']),
      });

    const { openaiCompatibleProvider } = await import('@/lib/ai/providers/openai');
    const events = await collectEvents(openaiCompatibleProvider.streamCompletion({
      baseUrl: 'https://api.kr777.top',
      model: 'model-a',
      apiKey: 'key',
      tools: [],
    }, [{ role: 'user', content: 'hi' }], new AbortController().signal));

    expect(events).toContainEqual({ type: 'content', content: 'ok' });
    expect(aiFetchStreamingMock.mock.calls[0]?.[0]).toBe('https://api.kr777.top/chat/completions');
    expect(aiFetchStreamingMock.mock.calls[1]?.[0]).toBe('https://api.kr777.top/v1/chat/completions');
  });

  it('openai-compatible streaming falls back to /v1 when root returns successful HTML', async () => {
    aiFetchStreamingMock
      .mockReturnValueOnce({
        response: Promise.resolve({ ok: true, status: 200 }),
        body: makeStream(['<html>new-api dashboard</html>']),
      })
      .mockReturnValueOnce({
        response: Promise.resolve({ ok: true, status: 200 }),
        body: makeStream(['data: {"choices":[{"delta":{"content":"v1 ok"}}]}\n\ndata: [DONE]\n\n']),
      });

    const { openaiCompatibleProvider } = await import('@/lib/ai/providers/openai');
    const events = await collectEvents(openaiCompatibleProvider.streamCompletion({
      baseUrl: 'https://api.kr777.top',
      model: 'claude-opus-via-new-api',
      apiKey: 'key',
      tools: [],
    }, [{ role: 'user', content: 'hi' }], new AbortController().signal));

    expect(events).toContainEqual({ type: 'content', content: 'v1 ok' });
    expect(aiFetchStreamingMock.mock.calls[0]?.[0]).toBe('https://api.kr777.top/chat/completions');
    expect(aiFetchStreamingMock.mock.calls[1]?.[0]).toBe('https://api.kr777.top/v1/chat/completions');
  });

  it('openai-compatible streaming parses non-SSE JSON responses from gateways', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream([JSON.stringify({ choices: [{ message: { content: 'json ok' } }] })]),
    });

    const { openaiCompatibleProvider } = await import('@/lib/ai/providers/openai');
    const events = await collectEvents(openaiCompatibleProvider.streamCompletion({
      baseUrl: 'https://api.kr777.top/v1',
      model: 'model-a',
      apiKey: 'key',
      tools: [],
    }, [{ role: 'user', content: 'hi' }], new AbortController().signal));

    expect(events).toContainEqual({ type: 'content', content: 'json ok' });
    expect(events.at(-1)).toEqual({ type: 'done' });
  });

  it('ollama OpenAI-compatible payload also merges repeated system messages', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream(['data: [DONE]']),
    });

    const { ollamaProvider } = await import('@/lib/ai/providers/ollama');
    await collectEvents(ollamaProvider.streamCompletion({
      baseUrl: 'http://localhost:11434',
      model: 'qwen-local',
      apiKey: '',
      tools: [],
    }, [
      { role: 'system', content: 'Agent role' },
      { role: 'user', content: 'hello' },
      { role: 'system', content: 'Current terminal context' },
    ], new AbortController().signal));

    const messages = getLastRequestBody().messages as Array<Record<string, unknown>>;
    expect(messages).toEqual([
      { role: 'system', content: 'Agent role\n\nCurrent terminal context' },
      { role: 'user', content: 'hello' },
    ]);
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

  it('openai provider maps required toolChoice to tool_choice', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream(['data: [DONE]']),
    });

    const { openaiProvider } = await import('@/lib/ai/providers/openai');
    await collectEvents(openaiProvider.streamCompletion({
      baseUrl: 'https://example.test',
      model: 'gpt-test',
      apiKey: 'key',
      tools: [{ name: 'local_exec', description: 'Run local command', parameters: { type: 'object', properties: {} } }],
      toolChoice: 'required',
    }, [{ role: 'user', content: 'run pwd' }], new AbortController().signal));

    expect(getLastRequestBody()).toMatchObject({
      tool_choice: 'required',
    });
  });

  it('openai provider maps specific toolChoice to function tool_choice', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream(['data: [DONE]']),
    });

    const { openaiProvider } = await import('@/lib/ai/providers/openai');
    await collectEvents(openaiProvider.streamCompletion({
      baseUrl: 'https://example.test',
      model: 'gpt-test',
      apiKey: 'key',
      tools: [{ name: 'get_settings', description: 'Read settings', parameters: { type: 'object', properties: {} } }],
      toolChoice: { type: 'tool', name: 'get_settings' },
    }, [{ role: 'user', content: 'check settings' }], new AbortController().signal));

    expect(getLastRequestBody()).toMatchObject({
      tool_choice: { type: 'function', function: { name: 'get_settings' } },
    });
  });

  it('openai-compatible provider retries without tool_choice when a gateway rejects it', async () => {
    aiFetchStreamingMock
      .mockReturnValueOnce({
        response: Promise.resolve({ ok: false, status: 400 }),
        body: makeStream(['{"error":{"message":"Unsupported parameter: tool_choice"}}']),
      })
      .mockReturnValueOnce({
        response: Promise.resolve({ ok: true, status: 200 }),
        body: makeStream(['data: [DONE]']),
      });

    const { openaiCompatibleProvider } = await import('@/lib/ai/providers/openai');
    const events = await collectEvents(openaiCompatibleProvider.streamCompletion({
      baseUrl: 'https://gateway.test',
      model: 'gateway-model',
      apiKey: '',
      tools: [{ name: 'local_exec', description: 'Run local command', parameters: { type: 'object', properties: {} } }],
      toolChoice: 'required',
    }, [{ role: 'user', content: 'run pwd' }], new AbortController().signal));

    expect(events.at(-1)).toEqual({ type: 'done' });
    expect(aiFetchStreamingMock).toHaveBeenCalledTimes(2);
    expect(JSON.parse(aiFetchStreamingMock.mock.calls[0]?.[1].body)).toMatchObject({ tool_choice: 'required' });
    expect(JSON.parse(aiFetchStreamingMock.mock.calls[1]?.[1].body)).not.toHaveProperty('tool_choice');
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

  it('anthropic provider maps required toolChoice to any tool choice', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream(['data: {"type":"message_stop"}']),
    });

    const { anthropicProvider } = await import('@/lib/ai/providers/anthropic');
    await collectEvents(anthropicProvider.streamCompletion({
      baseUrl: 'https://example.test',
      model: 'claude-test',
      apiKey: 'key',
      tools: [{ name: 'read_screen', description: 'Read screen', parameters: { type: 'object', properties: {} } }],
      toolChoice: 'required',
    }, [{ role: 'user', content: 'inspect terminal' }], new AbortController().signal));

    expect(getLastRequestBody()).toMatchObject({
      tool_choice: { type: 'any' },
    });
  });

  it('anthropic provider does not force toolChoice while extended thinking is enabled', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream(['data: {"type":"message_stop"}']),
    });

    const { anthropicProvider } = await import('@/lib/ai/providers/anthropic');
    await collectEvents(anthropicProvider.streamCompletion({
      baseUrl: 'https://example.test',
      model: 'claude-test',
      apiKey: 'key',
      maxResponseTokens: 4096,
      reasoningEffort: 'medium',
      reasoningProtocol: 'anthropic',
      tools: [{ name: 'read_screen', description: 'Read screen', parameters: { type: 'object', properties: {} } }],
      toolChoice: 'required',
    }, [{ role: 'user', content: 'inspect terminal' }], new AbortController().signal));

    expect(getLastRequestBody()).toMatchObject({
      thinking: { type: 'enabled', budget_tokens: 2048 },
    });
    expect(getLastRequestBody()).not.toHaveProperty('tool_choice');
  });

  it('deepseek provider maps reasoning off to disabled thinking', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream(['data: [DONE]']),
    });

    const { deepseekProvider } = await import('@/lib/ai/providers/openai');
    await collectEvents(deepseekProvider.streamCompletion({
      baseUrl: 'https://api.deepseek.com',
      model: 'deepseek-reasoner',
      apiKey: 'key',
      tools: [],
      reasoningEffort: 'off',
      reasoningProtocol: 'deepseek',
    }, [{ role: 'user', content: 'hi' }], new AbortController().signal));

    expect(getLastRequestBody()).toMatchObject({
      thinking: { type: 'disabled' },
    });
  });

  it('deepseek provider maps max reasoning to enabled thinking', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream(['data: [DONE]']),
    });

    const { deepseekProvider } = await import('@/lib/ai/providers/openai');
    await collectEvents(deepseekProvider.streamCompletion({
      baseUrl: 'https://api.deepseek.com',
      model: 'deepseek-reasoner',
      apiKey: 'key',
      tools: [],
      reasoningEffort: 'max',
      reasoningProtocol: 'deepseek',
    }, [{ role: 'user', content: 'hi' }], new AbortController().signal));

    expect(getLastRequestBody()).toMatchObject({
      thinking: { type: 'enabled' },
      reasoning_effort: 'max',
    });
  });

  it('deepseek provider preserves reasoning only for the active tool sub-turn', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream(['data: [DONE]']),
    });

    const { deepseekProvider } = await import('@/lib/ai/providers/openai');
    await collectEvents(deepseekProvider.streamCompletion({
      baseUrl: 'https://api.deepseek.com',
      model: 'deepseek-v4-pro',
      apiKey: 'key',
      tools: [],
      reasoningEffort: 'max',
      reasoningProtocol: 'deepseek',
    }, [
      { role: 'user', content: 'old question' },
      {
        role: 'assistant',
        content: '',
        reasoning_content: 'old reasoning',
        tool_calls: [{ id: 'old-call', name: 'read_file', arguments: '{}' }],
      },
      { role: 'user', content: 'current question' },
      {
        role: 'assistant',
        content: '',
        reasoning_content: 'current reasoning',
        tool_calls: [{ id: 'new-call', name: 'read_file', arguments: '{}' }],
      },
      { role: 'tool', content: 'tool result', tool_call_id: 'new-call' },
    ], new AbortController().signal));

    const messages = getLastRequestBody().messages as Array<Record<string, unknown>>;
    expect(messages[1]).not.toHaveProperty('reasoning_content');
    expect(messages[3]).toMatchObject({ reasoning_content: 'current reasoning' });
  });

  it('anthropic provider maps high reasoning to a thinking budget', async () => {
    aiFetchStreamingMock.mockReturnValue({
      response: Promise.resolve({ ok: true, status: 200 }),
      body: makeStream(['data: {"type":"message_stop"}']),
    });

    const { anthropicProvider } = await import('@/lib/ai/providers/anthropic');
    await collectEvents(anthropicProvider.streamCompletion({
      baseUrl: 'https://example.test',
      model: 'claude-test',
      apiKey: 'key',
      tools: [],
      maxResponseTokens: 8192,
      reasoningEffort: 'high',
      reasoningProtocol: 'anthropic',
    }, [{ role: 'user', content: 'hi' }], new AbortController().signal));

    expect(getLastRequestBody()).toMatchObject({
      thinking: { type: 'enabled', budget_tokens: 4096 },
    });
  });
});
