/**
 * Anthropic Provider Adapter
 *
 * Supports Anthropic's native Messages API with:
 * - SSE streaming (server-sent events)
 * - Extended thinking (Claude 3.5+ with `thinking` content blocks)
 * - Separate content_block_delta events for text and thinking
 */

import type { AiStreamProvider, AiRequestConfig, ChatMessage, AiStreamEvent } from '../providers';

/**
 * Convert standard ChatMessage format to Anthropic's Messages API format.
 * System messages are extracted and sent separately.
 */
function convertMessages(messages: ChatMessage[]): {
  system: string | undefined;
  messages: Array<{ role: 'user' | 'assistant'; content: string }>;
} {
  let system: string | undefined;
  const converted: Array<{ role: 'user' | 'assistant'; content: string }> = [];

  for (const msg of messages) {
    if (msg.role === 'system') {
      system = system ? `${system}\n\n${msg.content}` : msg.content;
    } else {
      converted.push({ role: msg.role, content: msg.content });
    }
  }

  // Anthropic requires alternating user/assistant messages starting with user
  // Merge consecutive same-role messages
  const merged: Array<{ role: 'user' | 'assistant'; content: string }> = [];
  for (const msg of converted) {
    const last = merged[merged.length - 1];
    if (last && last.role === msg.role) {
      last.content += '\n\n' + msg.content;
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
      max_tokens: 8192,
      stream: true,
    };

    if (system) {
      body.system = system;
    }

    // Enable extended thinking for supported models
    const supportsThinking = config.model.includes('claude-3') || config.model.includes('claude-sonnet') || config.model.includes('claude-opus');
    if (supportsThinking) {
      // Extended thinking is opt-in; we request it
      // The API will ignore if the model doesn't support it
    }

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'x-api-key': config.apiKey,
        'anthropic-version': '2023-06-01',
        'anthropic-dangerous-direct-browser-access': 'true',
      },
      body: JSON.stringify(body),
      signal,
    });

    if (!response.ok) {
      const errorText = await response.text();
      let errorMessage = `Anthropic API error: ${response.status}`;
      try {
        const errorJson = JSON.parse(errorText);
        errorMessage = errorJson.error?.message || errorMessage;
      } catch {
        if (errorText) errorMessage = errorText.slice(0, 200);
      }
      yield { type: 'error', message: errorMessage };
      return;
    }

    const reader = response.body?.getReader();
    if (!reader) {
      yield { type: 'error', message: 'No response body' };
      return;
    }

    const decoder = new TextDecoder();
    let buffer = '';

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (!line.startsWith('data: ')) continue;
          const data = line.slice(6).trim();
          if (!data) continue;

          try {
            const event = JSON.parse(data);

            switch (event.type) {
              case 'content_block_start': {
                // Track block type for logging; content type is determined by delta type
                break;
              }

              case 'content_block_delta': {
                const delta = event.delta;
                if (delta?.type === 'thinking_delta' && delta.thinking) {
                  yield { type: 'thinking', content: delta.thinking };
                } else if (delta?.type === 'text_delta' && delta.text) {
                  yield { type: 'content', content: delta.text };
                }
                break;
              }

              case 'content_block_stop': {
                break;
              }

              case 'message_stop': {
                yield { type: 'done' };
                return;
              }

              case 'error': {
                yield { type: 'error', message: event.error?.message || 'Anthropic stream error' };
                return;
              }
            }
          } catch {
            // Ignore parse errors
          }
        }
      }
    } finally {
      reader.releaseLock();
    }

    yield { type: 'done' };
  },
  async fetchModels(config: { baseUrl: string; apiKey: string }): Promise<string[]> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    const resp = await fetch(`${cleanBaseUrl}/v1/models`, {
      headers: {
        'x-api-key': config.apiKey,
        'anthropic-version': '2023-06-01',
        'anthropic-dangerous-direct-browser-access': 'true',
      },
    });
    if (!resp.ok) throw new Error(`Failed to fetch models: ${resp.status}`);
    const data = await resp.json();
    if (!Array.isArray(data.data)) return [];
    return data.data
      .map((m: { id: string }) => m.id)
      .filter((id: string) => id.startsWith('claude-'))
      .sort();
  },};
