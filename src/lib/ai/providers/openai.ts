// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * OpenAI Provider Adapter
 *
 * Supports native OpenAI API and any OpenAI-compatible endpoint.
 * Handles SSE streaming with `data: [DONE]` termination.
 */

import type { AiStreamProvider, AiRequestConfig, ChatMessage, AiStreamEvent, AiToolDefinition } from '../providers';
import { aiFetch, aiFetchStreaming } from '../aiFetch';

/**
 * Convert AiToolDefinition[] to OpenAI function calling format.
 */
function convertTools(tools: AiToolDefinition[]): Array<{ type: 'function'; function: { name: string; description: string; parameters: Record<string, unknown> } }> {
  return tools.map((t) => ({
    type: 'function' as const,
    function: {
      name: t.name,
      description: t.description,
      parameters: t.parameters,
    },
  }));
}

/**
 * Convert ChatMessage[] to OpenAI API message format (handles tool role).
 */
function mergeSystemMessages(messages: ChatMessage[]): ChatMessage[] {
  const systemContents: string[] = [];
  const nonSystemMessages: ChatMessage[] = [];

  for (const msg of messages) {
    if (msg.role === 'system') {
      if (msg.content) {
        systemContents.push(msg.content);
      }
      continue;
    }
    nonSystemMessages.push(msg);
  }

  if (systemContents.length === 0) {
    return messages;
  }

  return [
    { role: 'system', content: systemContents.join('\n\n') },
    ...nonSystemMessages,
  ];
}

function shouldPreserveReasoningContent(messages: ChatMessage[], index: number, config: AiRequestConfig): boolean {
  if (config.reasoningProtocol !== 'deepseek') {
    return true;
  }
  let lastUserIndex = -1;
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    if (messages[i]?.role === 'user') {
      lastUserIndex = i;
      break;
    }
  }
  // DeepSeek only needs reasoning_content replay during a tool sub-turn.
  // Older turns can drop it to reduce payload and match their API guidance.
  return index > lastUserIndex;
}

function convertMessages(messages: ChatMessage[], config: AiRequestConfig): Array<Record<string, unknown>> {
  const normalizedMessages = mergeSystemMessages(messages);
  return normalizedMessages.map((msg, index) => {
    if (msg.role === 'tool') {
      return {
        role: 'tool',
        tool_call_id: msg.tool_call_id,
        content: msg.content,
      };
    }
    if (msg.role === 'assistant' && msg.tool_calls && msg.tool_calls.length > 0) {
      const assistantMsg: Record<string, unknown> = {
        role: 'assistant',
        content: msg.content || null,
        tool_calls: msg.tool_calls.map((tc) => ({
          id: tc.id,
          type: 'function',
          function: { name: tc.name, arguments: tc.arguments },
        })),
      };
      // Preserve reasoning_content for thinking models (Kimi K2.5, DeepSeek-R1)
      if (msg.reasoning_content !== undefined && shouldPreserveReasoningContent(normalizedMessages, index, config)) {
        assistantMsg.reasoning_content = msg.reasoning_content;
      }
      return assistantMsg;
    }
    return { role: msg.role, content: msg.content };
  });
}

function applyReasoningOptions(body: Record<string, unknown>, config: AiRequestConfig): void {
  const effort = config.reasoningEffort ?? 'auto';
  if (effort === 'auto') return;

  if (config.reasoningProtocol === 'deepseek') {
    if (effort === 'off') {
      body.thinking = { type: 'disabled' };
      return;
    }
    body.thinking = { type: 'enabled' };
    body.reasoning_effort = effort === 'max' ? 'max' : 'high';
    return;
  }

  if (config.reasoningProtocol === 'openai') {
    if (effort === 'off') {
      body.reasoning_effort = 'minimal';
      return;
    }
    body.reasoning_effort = effort === 'max' ? 'high' : effort;
  }
}

function applyToolChoice(body: Record<string, unknown>, config: AiRequestConfig): void {
  if (!config.tools || config.tools.length === 0 || !config.toolChoice || config.toolChoice === 'auto') {
    return;
  }

  if (config.toolChoice === 'required') {
    body.tool_choice = 'required';
    return;
  }

  body.tool_choice = {
    type: 'function',
    function: { name: config.toolChoice.name },
  };
}

async function readErrorText(body: ReadableStream<Uint8Array>): Promise<string> {
  const errReader = body.getReader();
  const errDecoder = new TextDecoder();
  let errorText = '';
  try {
    while (true) {
      const { done, value } = await errReader.read();
      if (done) break;
      errorText += errDecoder.decode(value, { stream: true });
    }
  } catch { /* stream error */ }
  return errorText;
}

function parseOpenAiError(status: number, errorText: string): string {
  let errorMessage = `API error: ${status}`;
  try {
    const errorJson = JSON.parse(errorText);
    errorMessage = errorJson.error?.message || errorJson.message || errorMessage;
  } catch {
    if (errorText) errorMessage = errorText.slice(0, 200);
  }
  return errorMessage;
}

function looksLikeHtmlResponse(body: string): boolean {
  return body.trimStart().startsWith('<');
}

function parseProviderJson(body: string, context: string): unknown {
  try {
    return JSON.parse(body);
  } catch (error) {
    if (looksLikeHtmlResponse(body)) {
      throw new Error(`${context} returned HTML instead of JSON. Check the provider Base URL; OpenAI-compatible endpoints usually end with /v1.`);
    }
    throw new Error(`${context} returned invalid JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function parseOpenAiJsonEvents(body: string, context: string): AiStreamEvent[] {
  const json = parseProviderJson(body, context) as {
    choices?: Array<{
      message?: {
        content?: unknown;
        reasoning_content?: unknown;
        reasoning?: unknown;
        tool_calls?: Array<{ id?: string; function?: { name?: string; arguments?: string } }>;
      };
      delta?: {
        content?: unknown;
        reasoning_content?: unknown;
        reasoning?: unknown;
        tool_calls?: Array<{ id?: string; function?: { name?: string; arguments?: string } }>;
      };
    }>;
  };
  const payload = json.choices?.[0]?.message ?? json.choices?.[0]?.delta;
  const events: AiStreamEvent[] = [];
  const reasoning = payload?.reasoning_content ?? payload?.reasoning;

  if (typeof reasoning === 'string' && reasoning) {
    events.push({ type: 'thinking', content: reasoning });
  }
  if (typeof payload?.content === 'string' && payload.content) {
    events.push({ type: 'content', content: payload.content });
  }
  if (Array.isArray(payload?.tool_calls)) {
    payload.tool_calls.forEach((tc, index) => {
      events.push({
        type: 'tool_call_complete',
        id: tc.id || `call-${index}`,
        name: tc.function?.name || '',
        arguments: tc.function?.arguments || '{}',
      });
    });
  }

  return events;
}

function openAiCompatibleCandidates(baseUrl: string, path: string): string[] {
  const cleanBaseUrl = baseUrl.replace(/\/+$/, '');
  const candidates = [`${cleanBaseUrl}${path}`];
  try {
    const parsed = new URL(cleanBaseUrl);
    const pathname = parsed.pathname.replace(/\/+$/, '');
    if (!/\/v\d+(?:$|\/)/.test(pathname)) {
      candidates.push(`${cleanBaseUrl}/v1${path}`);
    }
  } catch {
    // Keep the direct URL for custom local endpoints that are not URL-parseable.
  }
  return [...new Set(candidates)];
}

async function fetchOpenAiCompatibleJson(config: { baseUrl: string; apiKey: string }, path: string, context: string): Promise<unknown> {
  const headers: Record<string, string> = config.apiKey ? { 'Authorization': `Bearer ${config.apiKey}` } : {};
  const candidates = openAiCompatibleCandidates(config.baseUrl, path);
  const errors: string[] = [];

  for (let index = 0; index < candidates.length; index += 1) {
    const url = candidates[index];
    const resp = await aiFetch(url, { headers });
    const hasFallback = index < candidates.length - 1;

    if (!resp.ok) {
      const message = `${url} returned HTTP ${resp.status}`;
      errors.push(message);
      if (hasFallback && (resp.status === 400 || resp.status === 404 || resp.status === 405 || looksLikeHtmlResponse(resp.body))) {
        continue;
      }
      throw new Error(`${context} failed: ${message}${resp.body ? ` — ${parseOpenAiError(resp.status, resp.body)}` : ''}`);
    }

    try {
      return parseProviderJson(resp.body, context);
    } catch (error) {
      errors.push(`${url}: ${error instanceof Error ? error.message : String(error)}`);
      if (hasFallback && looksLikeHtmlResponse(resp.body)) {
        continue;
      }
      throw error;
    }
  }

  throw new Error(`${context} failed. ${errors.join('; ')}`);
}

function isToolChoiceUnsupportedError(message: string): boolean {
  return /tool[_-]?choice|tool_choice|unsupported.*tool|unknown.*tool|unrecognized.*tool|invalid.*tool_choice/i.test(message);
}

export const openaiProvider: AiStreamProvider = {
  type: 'openai',
  displayName: 'OpenAI',

  async *streamCompletion(
    config: AiRequestConfig,
    messages: ChatMessage[],
    signal: AbortSignal
  ): AsyncGenerator<AiStreamEvent> {
    const body: Record<string, unknown> = {
      model: config.model,
      messages: convertMessages(messages, config),
      stream: true,
      ...(config.maxResponseTokens ? { max_tokens: config.maxResponseTokens } : {}),
    };
    applyReasoningOptions(body, config);

    if (config.tools && config.tools.length > 0) {
      body.tools = convertTools(config.tools);
      applyToolChoice(body, config);
    }

    const headers: Record<string, string> = {
        'Content-Type': 'application/json',
      };
    if (config.apiKey) {
      headers['Authorization'] = `Bearer ${config.apiKey}`;
    }

    const startRequest = (url: string, requestBody: Record<string, unknown>) => aiFetchStreaming(url, {
      method: 'POST',
      headers,
      body: JSON.stringify(requestBody),
      signal,
    });

    let errorMessage = '';
    const urls = openAiCompatibleCandidates(config.baseUrl, '/chat/completions');

    for (let index = 0; index < urls.length; index += 1) {
      const url = urls[index];
      const hasFallback = index < urls.length - 1;
      let streamBody: ReadableStream<Uint8Array> | undefined;
      let statusPromise: Promise<{ ok: boolean; status: number }>;
      let ok = false;
      let status = 0;
      ({ response: statusPromise, body: streamBody } = startRequest(url, body));
      ({ ok, status } = await statusPromise);
      if (!ok) {
        errorMessage = parseOpenAiError(status, await readErrorText(streamBody));
      }

      if (!ok && body.tool_choice && isToolChoiceUnsupportedError(errorMessage)) {
        const fallbackBody = { ...body };
        delete fallbackBody.tool_choice;
        ({ response: statusPromise, body: streamBody } = startRequest(url, fallbackBody));
        ({ ok, status } = await statusPromise);
        if (ok) {
          errorMessage = '';
        } else {
          errorMessage = parseOpenAiError(status, await readErrorText(streamBody));
        }
      }

      if (!ok || !streamBody) {
        if (!hasFallback || !(
          status === 400
          || status === 404
          || status === 405
          || looksLikeHtmlResponse(errorMessage)
        )) {
          yield { type: 'error', message: errorMessage || `No response body (${url})` };
          return;
        }
        continue;
      }

      const reader = streamBody.getReader();
      if (!reader) {
        yield { type: 'error', message: `No response body (${url})` };
        return;
      }

      const decoder = new TextDecoder();
      let buffer = '';
      let rawBody = '';
      let sawMeaningfulEvent = false;
      let sawSseFrame = false;
      let shouldTryFallback = false;
      // Track in-flight tool_calls being assembled across chunks
      const pendingToolCalls = new Map<number, { id: string; name: string; arguments: string }>();

      const processDataLine = (line: string): { events: AiStreamEvent[]; done: boolean } => {
        if (!line.startsWith('data: ')) return { events: [], done: false };
        sawSseFrame = true;
        const data = line.slice(6).trim();
        if (data === '[DONE]') {
          const events: AiStreamEvent[] = [];
          for (const tc of pendingToolCalls.values()) {
            events.push({ type: 'tool_call_complete', id: tc.id, name: tc.name, arguments: tc.arguments });
          }
          pendingToolCalls.clear();
          events.push({ type: 'done' });
          return { events, done: true };
        }

        const events: AiStreamEvent[] = [];
        try {
          const json = JSON.parse(data);
          const delta = json.choices?.[0]?.delta;
          const finishReason = json.choices?.[0]?.finish_reason;
          const reasoning = delta?.reasoning_content ?? delta?.reasoning;

          if (reasoning) {
            events.push({ type: 'thinking', content: reasoning });
          }

          if (delta?.tool_calls) {
            for (const tc of delta.tool_calls) {
              const idx = tc.index ?? 0;
              if (!pendingToolCalls.has(idx)) {
                pendingToolCalls.set(idx, {
                  id: tc.id || '',
                  name: tc.function?.name || '',
                  arguments: '',
                });
              }
              const pending = pendingToolCalls.get(idx)!;
              if (tc.id) pending.id = tc.id;
              if (tc.function?.name) pending.name = tc.function.name;
              if (tc.function?.arguments) {
                pending.arguments += tc.function.arguments;
                events.push({ type: 'tool_call', id: pending.id, name: pending.name, arguments: pending.arguments });
              }
            }
          }

          if (finishReason === 'tool_calls') {
            for (const tc of pendingToolCalls.values()) {
              events.push({ type: 'tool_call_complete', id: tc.id, name: tc.name, arguments: tc.arguments });
            }
            pendingToolCalls.clear();
          }

          const content = delta?.content || '';
          if (content) {
            events.push({ type: 'content', content });
          }
        } catch {
          // Ignore parse errors for partial chunks
        }

        return { events, done: false };
      };

      const emitProcessedEvents = function* (events: AiStreamEvent[]): Generator<AiStreamEvent, boolean> {
        let done = false;
        for (const event of events) {
          if (event.type !== 'done') {
            sawMeaningfulEvent = true;
            yield event;
          } else {
            done = true;
          }
        }
        if (done && sawMeaningfulEvent) {
          yield { type: 'done' };
          return true;
        }
        return false;
      };

      try {
        readLoop: while (true) {
          if (signal.aborted) break;
          const { done, value } = await reader.read();
          if (done) break;

          const chunk = decoder.decode(value, { stream: true });
          if (rawBody.length < 65536) {
            rawBody += chunk;
          }
          buffer += chunk;
          const lines = buffer.split('\n');
          buffer = lines.pop() || '';

          for (const line of lines) {
            const processed = processDataLine(line);
            const finished = yield* emitProcessedEvents(processed.events);
            if (finished) {
              return;
            }
            if (processed.done && !sawMeaningfulEvent) {
              break readLoop;
            }
          }
        }

        if (buffer.trim()) {
          const processed = processDataLine(buffer.trim());
          const finished = yield* emitProcessedEvents(processed.events);
          if (finished) {
            return;
          }
        }

        if (pendingToolCalls.size > 0) {
          sawMeaningfulEvent = true;
          for (const tc of pendingToolCalls.values()) {
            yield { type: 'tool_call_complete', id: tc.id, name: tc.name, arguments: tc.arguments };
          }
          pendingToolCalls.clear();
        }
      } finally {
        reader.releaseLock();
      }

      if (sawMeaningfulEvent) {
        yield { type: 'done' };
        return;
      }
      if (sawSseFrame) {
        yield { type: 'done' };
        return;
      }

      const trimmedBody = rawBody.trim();
      if (hasFallback && (!trimmedBody || looksLikeHtmlResponse(trimmedBody))) {
        shouldTryFallback = true;
      }
      if (shouldTryFallback) {
        continue;
      }
      if (!trimmedBody) {
        yield { type: 'error', message: `Provider returned an empty successful response (${url}).` };
        return;
      }
      if (looksLikeHtmlResponse(trimmedBody)) {
        yield { type: 'error', message: `Provider returned HTML instead of OpenAI SSE. Check the provider Base URL; OpenAI-compatible endpoints usually end with /v1. (${url})` };
        return;
      }
      try {
        const events = parseOpenAiJsonEvents(trimmedBody, 'OpenAI chat response');
        if (events.length === 0) {
          yield { type: 'error', message: `Provider returned a successful response without content or tool calls (${url}).` };
          return;
        }
        for (const event of events) {
          yield event;
        }
        yield { type: 'done' };
        return;
      } catch (error) {
        yield { type: 'error', message: error instanceof Error ? `${error.message} (${url})` : String(error) };
        return;
      }
    }

    yield { type: 'error', message: errorMessage || 'No response body' };
  },

  async fetchModels(config: { baseUrl: string; apiKey: string }): Promise<string[]> {
    const data = await fetchOpenAiCompatibleJson(config, '/models', 'OpenAI model list') as { data?: Array<{ id: string }> };
    if (!Array.isArray(data.data)) return [];
    // Return chat-capable models, sorted alphabetically
    const chatModels = data.data
      .map((m: { id: string }) => m.id)
      .filter((id: string) =>
        /^(gpt-|o[0-9]|chatgpt-)/.test(id) ||
        id.includes('turbo') ||
        id.includes('chat')
      )
      .sort();
    return chatModels.length > 0
      ? chatModels
      : data.data.map((m: { id: string }) => m.id).sort();
  },

  async fetchModelDetails(config: { baseUrl: string; apiKey: string }): Promise<Record<string, number>> {
    let data: { data?: Array<{ id: string; context_window?: number; context_length?: number }> };
    try {
      data = await fetchOpenAiCompatibleJson(config, '/models', 'OpenAI model details') as { data?: Array<{ id: string; context_window?: number; context_length?: number }> };
    } catch {
      return {};
    }
    if (!Array.isArray(data.data)) return {};
    const result: Record<string, number> = {};
    for (const m of data.data) {
      // OpenAI returns context_window on some endpoints, or we can infer from id
      const ctx = m.context_window ?? m.context_length;
      if (typeof ctx === 'number' && ctx > 0) {
        result[m.id] = ctx;
      }
    }
    return result;
  },

  async embedTexts(config: { baseUrl: string; apiKey: string; model: string }, texts: string[]): Promise<number[][]> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    const resp = await aiFetch(`${cleanBaseUrl}/embeddings`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(config.apiKey ? { 'Authorization': `Bearer ${config.apiKey}` } : {}),
      },
      body: JSON.stringify({ model: config.model, input: texts }),
    });
    if (!resp.ok) throw new Error(`Embedding request failed: ${resp.status}`);
    const data = parseProviderJson(resp.body, 'Embedding response') as { data?: Array<{ index: number; embedding: number[] }> };
    if (!Array.isArray(data.data)) throw new Error('Invalid embedding response');
    return data.data
      .sort((a: { index: number }, b: { index: number }) => a.index - b.index)
      .map((d: { embedding: number[] }) => d.embedding);
  },
};

/**
 * OpenAI-compatible provider (reuses the same implementation)
 */
export const openaiCompatibleProvider: AiStreamProvider = {
  ...openaiProvider,
  type: 'openai_compatible',
  displayName: 'OpenAI Compatible',

  async fetchModels(config: { baseUrl: string; apiKey: string }): Promise<string[]> {
    const data = await fetchOpenAiCompatibleJson(config, '/models', 'OpenAI-compatible model list') as {
      data?: Array<{ id?: string; key?: string }>;
      models?: Array<{ id?: string; key?: string }>;
    };
    // OpenAI format: { data: [{ id }] }, LM Studio native: { models: [{ key, display_name }] }
    const models = Array.isArray(data.data) ? data.data : Array.isArray(data.models) ? data.models : [];
    return models.map((m: { id?: string; key?: string }) => m.id ?? m.key ?? '').filter(Boolean).sort();
  },

  async fetchModelDetails(config: { baseUrl: string; apiKey: string }): Promise<Record<string, number>> {
    const data = await fetchOpenAiCompatibleJson(config, '/models', 'OpenAI-compatible model details') as {
      data?: Array<{ id?: string; key?: string; context_window?: number; context_length?: number }>;
      models?: Array<{ id?: string; key?: string; context_window?: number; context_length?: number }>;
    };
    const models = Array.isArray(data.data) ? data.data : Array.isArray(data.models) ? data.models : [];
    const result: Record<string, number> = {};
    for (const m of models) {
      const id = m.id ?? m.key;
      if (!id) continue;
      const ctx = m.context_window ?? m.context_length;
      if (typeof ctx === 'number' && ctx > 0) {
        result[id] = ctx;
      }
    }
    return result;
  },
};

export const deepseekProvider: AiStreamProvider = {
  ...openaiCompatibleProvider,
  type: 'deepseek',
  displayName: 'DeepSeek',
};
