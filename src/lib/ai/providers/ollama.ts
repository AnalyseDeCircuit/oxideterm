// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Ollama Provider Adapter
 *
 * Supports local Ollama instances.
 * Uses the OpenAI-compatible `/v1/chat/completions` endpoint (Ollama >= 0.1.14).
 */

import type { AiStreamProvider, AiRequestConfig, ChatMessage, AiStreamEvent, AiToolDefinition } from '../providers';
import { DEFAULT_CONTEXT_WINDOW, getModelContextWindow } from '../tokenUtils';
import { aiFetch, aiFetchStreaming } from '../aiFetch';

/** Timeout for individual /api/show calls (ms) */
const OLLAMA_SHOW_TIMEOUT = 2000;
/** Max "wild" models to query via API (those not in the static lookup table) */
const MAX_WILD_MODELS_QUERY = 20;

/**
 * Convert AiToolDefinition[] to OpenAI function calling format (shared with Ollama).
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
 * Convert ChatMessage[] to OpenAI-compatible format for Ollama.
 * Transforms tool role messages and assistant tool_calls to the structure
 * expected by Ollama's OpenAI-compatible endpoint.
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

function convertMessages(messages: ChatMessage[]): Array<Record<string, unknown>> {
  return mergeSystemMessages(messages).map((msg) => {
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
      if (msg.reasoning_content !== undefined) {
        assistantMsg.reasoning_content = msg.reasoning_content;
      }
      return assistantMsg;
    }
    return { role: msg.role, content: msg.content };
  });
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

async function readErrorText(reader: ReadableStreamDefaultReader<Uint8Array>): Promise<string> {
  const errDecoder = new TextDecoder();
  let errorText = '';
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      errorText += errDecoder.decode(value, { stream: true });
    }
  } catch { /* stream error */ }
  return errorText;
}

function parseOllamaError(status: number, errorText: string): string {
  if (status === 0 || errorText.includes('ECONNREFUSED')) {
    return 'Cannot connect to Ollama. Make sure Ollama is running (ollama serve).';
  }

  let errorMessage = `Ollama error: ${status}`;
  try {
    const errorJson = JSON.parse(errorText);
    errorMessage = errorJson.error?.message || errorJson.error || errorMessage;
  } catch {
    if (errorText) errorMessage = errorText.slice(0, 200);
  }
  return errorMessage;
}

function isToolChoiceUnsupportedError(message: string): boolean {
  return /tool[_-]?choice|tool_choice|unsupported.*tool|unknown.*tool|unrecognized.*tool|invalid.*tool_choice/i.test(message);
}

export const ollamaProvider: AiStreamProvider = {
  type: 'ollama',
  displayName: 'Ollama (Local)',

  async *streamCompletion(
    config: AiRequestConfig,
    messages: ChatMessage[],
    signal: AbortSignal
  ): AsyncGenerator<AiStreamEvent> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    // Use Ollama's OpenAI-compatible endpoint
    const url = `${cleanBaseUrl}/v1/chat/completions`;

    let streamOk: boolean;
    let streamStatus: number;
    let reader: ReadableStreamDefaultReader<Uint8Array>;
    try {
      const body: Record<string, unknown> = {
        model: config.model,
        messages: convertMessages(messages),
        stream: true,
        ...(config.maxResponseTokens ? { max_tokens: config.maxResponseTokens } : {}),
      };
      if (config.tools && config.tools.length > 0) {
        body.tools = convertTools(config.tools);
        applyToolChoice(body, config);
      }

      const headers: Record<string, string> = {
        'Content-Type': 'application/json',
        ...(config.apiKey ? { 'Authorization': `Bearer ${config.apiKey}` } : {}),
      };

      const startRequest = (requestBody: Record<string, unknown>) => aiFetchStreaming(url, {
        method: 'POST',
        headers,
        body: JSON.stringify(requestBody),
        signal,
      });

      let { response: statusPromise, body: streamBody } = startRequest(body);
      const resp = await statusPromise;
      streamOk = resp.ok;
      streamStatus = resp.status;
      reader = streamBody.getReader();
      if (!streamOk && body.tool_choice) {
        const firstErrorMessage = parseOllamaError(streamStatus, await readErrorText(reader));
        if (isToolChoiceUnsupportedError(firstErrorMessage)) {
          const fallbackBody = { ...body };
          delete fallbackBody.tool_choice;
          ({ response: statusPromise, body: streamBody } = startRequest(fallbackBody));
          const fallbackResp = await statusPromise;
          streamOk = fallbackResp.ok;
          streamStatus = fallbackResp.status;
          reader = streamBody.getReader();
        } else {
          yield { type: 'error', message: firstErrorMessage };
          return;
        }
      }
    } catch (e) {
      yield { type: 'error', message: 'Cannot connect to Ollama. Make sure Ollama is running (ollama serve).' };
      return;
    }

    if (!streamOk) {
      yield { type: 'error', message: parseOllamaError(streamStatus, await readErrorText(reader)) };
      return;
    }

    const decoder = new TextDecoder();
    let buffer = '';
    const pendingToolCalls = new Map<number, { id: string; name: string; arguments: string }>();

    const processDataLine = (line: string): { events: AiStreamEvent[]; done: boolean } => {
      if (!line.startsWith('data: ')) return { events: [], done: false };
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

        if (delta?.reasoning_content) {
          events.push({ type: 'thinking', content: delta.reasoning_content });
        }
        if (delta?.tool_calls) {
          for (const tc of delta.tool_calls) {
            const idx = tc.index ?? 0;
            if (!pendingToolCalls.has(idx)) {
              pendingToolCalls.set(idx, { id: tc.id || '', name: tc.function?.name || '', arguments: '' });
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
        if (delta?.content) {
          events.push({ type: 'content', content: delta.content });
        }
      } catch {
        // Ignore parse errors
      }

      return { events, done: false };
    };

    try {
      while (true) {
        if (signal.aborted) break;
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          const processed = processDataLine(line);
          for (const event of processed.events) {
            yield event;
          }
          if (processed.done) {
            return;
          }
        }
      }

      if (buffer.trim()) {
        const processed = processDataLine(buffer.trim());
        for (const event of processed.events) {
          yield event;
        }
        if (processed.done) {
          return;
        }
      }

      if (pendingToolCalls.size > 0) {
        for (const tc of pendingToolCalls.values()) {
          yield { type: 'tool_call_complete', id: tc.id, name: tc.name, arguments: tc.arguments };
        }
        pendingToolCalls.clear();
      }
    } finally {
      reader.releaseLock();
    }

    yield { type: 'done' };
  },

  async fetchModels(config: { baseUrl: string; apiKey: string }): Promise<string[]> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    // Try Ollama native /api/tags first
    let resp: { ok: boolean; status: number; body: string };
    try {
      resp = await aiFetch(`${cleanBaseUrl}/api/tags`, {
        headers: config.apiKey ? { 'Authorization': `Bearer ${config.apiKey}` } : {},
      });
    } catch (e) {
      throw new Error('Cannot connect to Ollama. Make sure Ollama is running (ollama serve).');
    }
    if (!resp.ok) throw new Error(`Failed to fetch models: ${resp.status}`);
    const data = JSON.parse(resp.body);
    if (!Array.isArray(data.models)) return [];
    return data.models
      .map((m: { name: string }) => m.name)
      .sort();
  },

  async fetchModelDetails(config: { baseUrl: string; apiKey: string }): Promise<Record<string, number>> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    // First get all model names
    let resp: { ok: boolean; status: number; body: string };
    try {
      resp = await aiFetch(`${cleanBaseUrl}/api/tags`, {
        headers: config.apiKey ? { 'Authorization': `Bearer ${config.apiKey}` } : {},
      });
    } catch {
      return {};
    }
    if (!resp.ok) return {};
    const data = JSON.parse(resp.body);
    if (!Array.isArray(data.models)) return {};

    const result: Record<string, number> = {};

    // Separate models into "known" (matched by static lookup table) and "wild" (unknown)
    const wildModels: string[] = [];
    for (const m of data.models) {
      const staticCtx = getModelContextWindow(m.name);
      // If static lookup returns the default fallback, it means no match → wild model.
      if (staticCtx !== DEFAULT_CONTEXT_WINDOW) {
        result[m.name] = staticCtx;
      } else {
        wildModels.push(m.name);
      }
    }

    // Only query wild models via API (parallel with timeout, capped)
    const toQuery = wildModels.slice(0, MAX_WILD_MODELS_QUERY);
    if (toQuery.length > 0) {
      const queryResults = await Promise.allSettled(
        toQuery.map(async (name) => {
          try {
            const showResp = await aiFetch(`${cleanBaseUrl}/api/show`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ name }),
              timeoutMs: OLLAMA_SHOW_TIMEOUT,
            });
            if (showResp.ok) {
              const showData = JSON.parse(showResp.body);
              const ctx = showData.model_info?.['general.context_length']
                ?? showData.model_info?.context_length
                ?? showData.parameters?.num_ctx;
              if (typeof ctx === 'number' && ctx > 0) {
                return { name, ctx };
              }
            }
            return null;
          } catch {
            return null;
          }
        })
      );

      for (const r of queryResults) {
        if (r.status === 'fulfilled' && r.value) {
          result[r.value.name] = r.value.ctx;
        }
      }
    }

    return result;
  },

  async embedTexts(config: { baseUrl: string; apiKey: string; model: string }, texts: string[]): Promise<number[][]> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    // Use OpenAI-compatible endpoint if available, fall back to native Ollama
    const resp = await aiFetch(`${cleanBaseUrl}/api/embed`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(config.apiKey ? { 'Authorization': `Bearer ${config.apiKey}` } : {}),
      },
      body: JSON.stringify({ model: config.model, input: texts }),
    });
    if (!resp.ok) throw new Error(`Ollama embedding request failed: ${resp.status}`);
    const data = JSON.parse(resp.body);
    if (Array.isArray(data.embeddings)) return data.embeddings;
    throw new Error('Invalid Ollama embedding response');
  },
};
