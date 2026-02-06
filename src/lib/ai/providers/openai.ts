/**
 * OpenAI Provider Adapter
 *
 * Supports native OpenAI API and any OpenAI-compatible endpoint.
 * Handles SSE streaming with `data: [DONE]` termination.
 */

import type { AiStreamProvider, AiRequestConfig, ChatMessage, AiStreamEvent } from '../providers';

export const openaiProvider: AiStreamProvider = {
  type: 'openai',
  displayName: 'OpenAI',

  async *streamCompletion(
    config: AiRequestConfig,
    messages: ChatMessage[],
    signal: AbortSignal
  ): AsyncGenerator<AiStreamEvent> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    const url = `${cleanBaseUrl}/chat/completions`;

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${config.apiKey}`,
      },
      body: JSON.stringify({
        model: config.model,
        messages,
        stream: true,
      }),
      signal,
    });

    if (!response.ok) {
      const errorText = await response.text();
      let errorMessage = `API error: ${response.status}`;
      try {
        const errorJson = JSON.parse(errorText);
        errorMessage = errorJson.error?.message || errorJson.message || errorMessage;
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
          if (line.startsWith('data: ')) {
            const data = line.slice(6).trim();
            if (data === '[DONE]') {
              yield { type: 'done' };
              return;
            }

            try {
              const json = JSON.parse(data);
              const content = json.choices?.[0]?.delta?.content || '';
              if (content) {
                yield { type: 'content', content };
              }
            } catch {
              // Ignore parse errors for partial chunks
            }
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
    const resp = await fetch(`${cleanBaseUrl}/models`, {
      headers: { 'Authorization': `Bearer ${config.apiKey}` },
    });
    if (!resp.ok) throw new Error(`Failed to fetch models: ${resp.status}`);
    const data = await resp.json();
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
};

/**
 * OpenAI-compatible provider (reuses the same implementation)
 */
export const openaiCompatibleProvider: AiStreamProvider = {
  ...openaiProvider,
  type: 'openai_compatible',
  displayName: 'OpenAI Compatible',

  async fetchModels(config: { baseUrl: string; apiKey: string }): Promise<string[]> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    const resp = await fetch(`${cleanBaseUrl}/models`, {
      headers: config.apiKey ? { 'Authorization': `Bearer ${config.apiKey}` } : {},
    });
    if (!resp.ok) throw new Error(`Failed to fetch models: ${resp.status}`);
    const data = await resp.json();
    if (!Array.isArray(data.data)) return [];
    return data.data.map((m: { id: string }) => m.id).sort();
  },
};
