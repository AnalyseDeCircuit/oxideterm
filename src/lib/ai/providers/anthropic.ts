// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Anthropic Provider Adapter
 *
 * Supports Anthropic's native Messages API with:
 * - SSE streaming (server-sent events)
 * - Extended thinking (Claude 3.5+ with `thinking` content blocks)
 * - Separate content_block_delta events for text and thinking
 */

import type { AiStreamProvider, AiRequestConfig, ChatMessage, AiStreamEvent, AiToolDefinition } from '../providers';
import { aiFetch, aiFetchStreaming } from '../aiFetch';

/**
 * Convert standard ChatMessage format to Anthropic's Messages API format.
 * System messages are extracted and sent separately.
 * Tool messages are converted to Anthropic's tool_result content blocks.
 */
function convertMessages(messages: ChatMessage[]): {
  system: string | undefined;
  messages: Array<{ role: 'user' | 'assistant'; content: unknown }>;
} {
  let system: string | undefined;
  const converted: Array<{ role: 'user' | 'assistant'; content: unknown }> = [];

  for (const msg of messages) {
    if (msg.role === 'system') {
      system = system ? `${system}\n\n${msg.content}` : msg.content;
    } else if (msg.role === 'tool') {
      // Anthropic wraps tool results in a user message with content blocks
      converted.push({
        role: 'user',
        content: [{
          type: 'tool_result',
          tool_use_id: msg.tool_call_id,
          content: msg.content,
        }],
      });
    } else if (msg.role === 'assistant' && msg.tool_calls && msg.tool_calls.length > 0) {
      // Assistant message with tool use — convert to content blocks
      const contentBlocks: Array<Record<string, unknown>> = [];
      if (msg.content) {
        contentBlocks.push({ type: 'text', text: msg.content });
      }
      for (const tc of msg.tool_calls) {
        let input: Record<string, unknown> = {};
        try { input = JSON.parse(tc.arguments); } catch { /* empty */ }
        contentBlocks.push({
          type: 'tool_use',
          id: tc.id,
          name: tc.name,
          input,
        });
      }
      converted.push({ role: 'assistant', content: contentBlocks });
    } else {
      converted.push({ role: msg.role as 'user' | 'assistant', content: msg.content });
    }
  }

  // Anthropic requires alternating user/assistant messages starting with user
  // Merge consecutive same-role messages
  const merged: Array<{ role: 'user' | 'assistant'; content: unknown }> = [];
  for (const msg of converted) {
    const last = merged[merged.length - 1];
    if (last && last.role === msg.role) {
      // Only merge string content; array content blocks must stay separate
      if (typeof last.content === 'string' && typeof msg.content === 'string') {
        last.content += '\n\n' + msg.content;
      } else {
        // Convert both to arrays and concatenate
        const lastArr = Array.isArray(last.content)
          ? last.content
          : [{ type: 'text', text: last.content as string }];
        const msgArr = Array.isArray(msg.content)
          ? msg.content
          : [{ type: 'text', text: msg.content as string }];
        last.content = [...lastArr, ...msgArr];
      }
    } else {
      merged.push({ ...msg });
    }
  }

  // Ensure first message is from user
  if (merged.length > 0 && merged[0].role !== 'user') {
    merged.unshift({ role: 'user', content: '(Continue from previous context)' });
  }

  return { system, messages: merged };
}

/**
 * Convert AiToolDefinition[] to Anthropic tools format.
 */
function convertTools(tools: AiToolDefinition[]): Array<{ name: string; description: string; input_schema: Record<string, unknown> }> {
  return tools.map((t) => ({
    name: t.name,
    description: t.description,
    input_schema: t.parameters,
  }));
}

export const anthropicProvider: AiStreamProvider = {
  type: 'anthropic',
  displayName: 'Anthropic',

  async *streamCompletion(
    config: AiRequestConfig,
    messages: ChatMessage[],
    signal: AbortSignal
  ): AsyncGenerator<AiStreamEvent> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    const url = `${cleanBaseUrl}/v1/messages`;

    const { system, messages: apiMessages } = convertMessages(messages);

    // Build request body
    const body: Record<string, unknown> = {
      model: config.model,
      messages: apiMessages,
      max_tokens: config.maxResponseTokens ?? 8192,
      stream: true,
    };

    if (system) {
      body.system = system;
    }

    if (config.tools && config.tools.length > 0) {
      body.tools = convertTools(config.tools);
    }

    const { response: statusPromise, body: streamBody } = aiFetchStreaming(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'x-api-key': config.apiKey,
        'anthropic-version': '2023-06-01',
      },
      body: JSON.stringify(body),
      signal,
    });

    const { ok, status } = await statusPromise;

    if (!ok) {
      const errReader = streamBody.getReader();
      const errDecoder = new TextDecoder();
      let errorText = '';
      try {
        while (true) {
          const { done, value } = await errReader.read();
          if (done) break;
          errorText += errDecoder.decode(value, { stream: true });
        }
      } catch { /* stream error */ }
      let errorMessage = `Anthropic API error: ${status}`;
      try {
        const errorJson = JSON.parse(errorText);
        errorMessage = errorJson.error?.message || errorMessage;
      } catch {
        if (errorText) errorMessage = errorText.slice(0, 200);
      }
      yield { type: 'error', message: errorMessage };
      return;
    }

    const reader = streamBody.getReader();
    if (!reader) {
      yield { type: 'error', message: 'No response body' };
      return;
    }

    const decoder = new TextDecoder();
    let buffer = '';
    // Track current tool_use block being assembled
    let currentToolUse: { id: string; name: string; arguments: string } | null = null;

    const processDataLine = (line: string): { events: AiStreamEvent[]; done: boolean } => {
      if (!line.startsWith('data: ')) return { events: [], done: false };
      const data = line.slice(6).trim();
      if (!data) return { events: [], done: false };

      const events: AiStreamEvent[] = [];

      try {
        const event = JSON.parse(data);

        switch (event.type) {
          case 'content_block_start': {
            const block = event.content_block;
            if (block?.type === 'tool_use') {
              currentToolUse = {
                id: block.id || '',
                name: block.name || '',
                arguments: '',
              };
            }
            break;
          }

          case 'content_block_delta': {
            const delta = event.delta;
            if (delta?.type === 'thinking_delta' && delta.thinking) {
              events.push({ type: 'thinking', content: delta.thinking });
            } else if (delta?.type === 'text_delta' && delta.text) {
              events.push({ type: 'content', content: delta.text });
            } else if (delta?.type === 'input_json_delta' && delta.partial_json && currentToolUse) {
              currentToolUse.arguments += delta.partial_json;
              events.push({ type: 'tool_call', id: currentToolUse.id, name: currentToolUse.name, arguments: currentToolUse.arguments });
            }
            break;
          }

          case 'content_block_stop': {
            if (currentToolUse) {
              events.push({ type: 'tool_call_complete', id: currentToolUse.id, name: currentToolUse.name, arguments: currentToolUse.arguments });
              currentToolUse = null;
            }
            break;
          }

          case 'message_stop': {
            events.push({ type: 'done' });
            return { events, done: true };
          }

          case 'error': {
            events.push({ type: 'error', message: event.error?.message || 'Anthropic stream error' });
            return { events, done: true };
          }
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

      if (currentToolUse) {
        yield { type: 'tool_call_complete', id: currentToolUse.id, name: currentToolUse.name, arguments: currentToolUse.arguments };
        currentToolUse = null;
      }
    } finally {
      reader.releaseLock();
    }

    yield { type: 'done' };
  },
  async fetchModels(config: { baseUrl: string; apiKey: string }): Promise<string[]> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    const resp = await aiFetch(`${cleanBaseUrl}/v1/models`, {
      headers: {
        'x-api-key': config.apiKey,
        'anthropic-version': '2023-06-01',
      },
    });
    if (!resp.ok) throw new Error(`Failed to fetch models: ${resp.status}`);
    const data = JSON.parse(resp.body);
    if (!Array.isArray(data.data)) return [];
    return data.data
      .map((m: { id: string }) => m.id)
      .filter((id: string) => id.startsWith('claude-'))
      .sort();
  },

  async fetchModelDetails(config: { baseUrl: string; apiKey: string }): Promise<Record<string, number>> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    const resp = await aiFetch(`${cleanBaseUrl}/v1/models`, {
      headers: {
        'x-api-key': config.apiKey,
        'anthropic-version': '2023-06-01',
      },
    });
    if (!resp.ok) return {};
    const data = JSON.parse(resp.body);
    if (!Array.isArray(data.data)) return {};
    const result: Record<string, number> = {};
    for (const m of data.data) {
      // Anthropic returns context_window or input_token_limit
      const ctx = m.context_window ?? m.input_token_limit;
      if (typeof ctx === 'number' && ctx > 0) {
        result[m.id] = ctx;
      }
    }
    return result;
  },
};
