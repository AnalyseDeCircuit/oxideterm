/**
 * Ollama Provider Adapter
 *
 * Supports local Ollama instances.
 * Uses the OpenAI-compatible `/v1/chat/completions` endpoint (Ollama >= 0.1.14).
 */

import type { AiStreamProvider, AiRequestConfig, ChatMessage, AiStreamEvent } from '../providers';

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

    let response: Response;
    try {
      response = await fetch(url, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          // Ollama doesn't require auth but we send it if configured
          ...(config.apiKey ? { 'Authorization': `Bearer ${config.apiKey}` } : {}),
        },
        body: JSON.stringify({
          model: config.model,
          messages,
          stream: true,
        }),
        signal,
      });
    } catch (e) {
      yield { type: 'error', message: 'Cannot connect to Ollama. Make sure Ollama is running (ollama serve).' };
      return;
    }

    if (!response.ok) {
      const errorText = await response.text();
      let errorMessage = `Ollama error: ${response.status}`;

      // Special handling for connection refused (Ollama not running)
      if (response.status === 0 || errorText.includes('ECONNREFUSED')) {
        errorMessage = 'Cannot connect to Ollama. Make sure Ollama is running (ollama serve).';
      } else {
        try {
          const errorJson = JSON.parse(errorText);
          errorMessage = errorJson.error?.message || errorJson.error || errorMessage;
        } catch {
          if (errorText) errorMessage = errorText.slice(0, 200);
        }
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
              // Handle DeepSeek-R1 style thinking in Ollama
              const delta = json.choices?.[0]?.delta;
              if (delta?.reasoning_content) {
                yield { type: 'thinking', content: delta.reasoning_content };
              }
              if (delta?.content) {
                yield { type: 'content', content: delta.content };
              }
            } catch {
              // Ignore parse errors
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
    // Try Ollama native /api/tags first
    let resp: Response;
    try {
      resp = await fetch(`${cleanBaseUrl}/api/tags`, {
        headers: config.apiKey ? { 'Authorization': `Bearer ${config.apiKey}` } : {},
      });
    } catch (e) {
      throw new Error('Cannot connect to Ollama. Make sure Ollama is running (ollama serve).');
    }
    if (!resp.ok) throw new Error(`Failed to fetch models: ${resp.status}`);
    const data = await resp.json();
    if (!Array.isArray(data.models)) return [];
    return data.models
      .map((m: { name: string }) => m.name)
      .sort();
  },
};
