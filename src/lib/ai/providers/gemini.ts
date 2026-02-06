/**
 * Google Gemini Provider Adapter
 *
 * Supports Google's Generative Language API with SSE streaming.
 * Uses the `generateContent` endpoint with `streamGenerateContent`.
 */

import type { AiStreamProvider, AiRequestConfig, ChatMessage, AiStreamEvent } from '../providers';

/**
 * Convert standard ChatMessage format to Gemini API format.
 */
function convertMessages(messages: ChatMessage[]): {
  systemInstruction: string | undefined;
  contents: Array<{ role: 'user' | 'model'; parts: Array<{ text: string }> }>;
} {
  let systemInstruction: string | undefined;
  const contents: Array<{ role: 'user' | 'model'; parts: Array<{ text: string }> }> = [];

  for (const msg of messages) {
    if (msg.role === 'system') {
      systemInstruction = systemInstruction
        ? `${systemInstruction}\n\n${msg.content}`
        : msg.content;
    } else {
      const role = msg.role === 'assistant' ? 'model' : 'user';
      const last = contents[contents.length - 1];
      // Gemini requires alternating roles
      if (last && last.role === role) {
        last.parts.push({ text: msg.content });
      } else {
        contents.push({ role, parts: [{ text: msg.content }] });
      }
    }
  }

  // Ensure starts with user
  if (contents.length > 0 && contents[0].role !== 'user') {
    contents.unshift({ role: 'user', parts: [{ text: '(Continue)' }] });
  }

  return { systemInstruction, contents };
}

export const geminiProvider: AiStreamProvider = {
  type: 'gemini',
  displayName: 'Google Gemini',

  async *streamCompletion(
    config: AiRequestConfig,
    messages: ChatMessage[],
    signal: AbortSignal
  ): AsyncGenerator<AiStreamEvent> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    // Gemini uses API key as query param, not Bearer token
    const url = `${cleanBaseUrl}/models/${config.model}:streamGenerateContent?alt=sse&key=${config.apiKey}`;

    const { systemInstruction, contents } = convertMessages(messages);

    const body: Record<string, unknown> = { contents };
    if (systemInstruction) {
      body.system_instruction = { parts: [{ text: systemInstruction }] };
    }

    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal,
    });

    if (!response.ok) {
      const errorText = await response.text();
      let errorMessage = `Gemini API error: ${response.status}`;
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
            const json = JSON.parse(data);
            const candidates = json.candidates;
            if (candidates?.[0]?.content?.parts) {
              for (const part of candidates[0].content.parts) {
                if (part.text) {
                  yield { type: 'content', content: part.text };
                }
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
    const resp = await fetch(
      `${cleanBaseUrl}/v1beta/models?key=${config.apiKey}`
    );
    if (!resp.ok) throw new Error(`Failed to fetch models: ${resp.status}`);
    const data = await resp.json();
    if (!Array.isArray(data.models)) return [];
    return data.models
      .filter((m: { supportedGenerationMethods?: string[] }) =>
        m.supportedGenerationMethods?.includes('generateContent')
      )
      .map((m: { name: string }) => m.name.replace('models/', ''))
      .sort();
  },
};
