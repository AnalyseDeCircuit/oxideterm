// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Google Gemini Provider Adapter
 *
 * Supports Google's Generative Language API with SSE streaming.
 * Uses the `generateContent` endpoint with `streamGenerateContent`.
 */

import type { AiStreamProvider, AiRequestConfig, ChatMessage, AiStreamEvent, AiToolDefinition } from '../providers';
import { aiFetch, aiFetchStreaming } from '../aiFetch';

/**
 * Convert standard ChatMessage format to Gemini API format.
 * Handles tool role by converting to functionResponse parts.
 */
function convertMessages(messages: ChatMessage[]): {
  systemInstruction: string | undefined;
  contents: Array<{ role: 'user' | 'model'; parts: Array<Record<string, unknown>> }>;
} {
  let systemInstruction: string | undefined;
  const contents: Array<{ role: 'user' | 'model'; parts: Array<Record<string, unknown>> }> = [];

  for (const msg of messages) {
    if (msg.role === 'system') {
      systemInstruction = systemInstruction
        ? `${systemInstruction}\n\n${msg.content}`
        : msg.content;
    } else if (msg.role === 'tool') {
      // Gemini uses functionResponse in a user-role message
      let response: Record<string, unknown> = { output: msg.content };
      try { response = JSON.parse(msg.content); } catch { /* use as string */ }
      contents.push({
        role: 'user',
        parts: [{ functionResponse: { name: msg.tool_name || 'unknown', response } }],
      });
    } else if (msg.role === 'assistant' && msg.tool_calls && msg.tool_calls.length > 0) {
      // Model message with function calls
      const parts: Array<Record<string, unknown>> = [];
      if (msg.content) parts.push({ text: msg.content });
      for (const tc of msg.tool_calls) {
        let args: Record<string, unknown> = {};
        try { args = JSON.parse(tc.arguments); } catch { /* empty */ }
        parts.push({ functionCall: { name: tc.name, args } });
      }
      contents.push({ role: 'model', parts });
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

/**
 * Convert AiToolDefinition[] to Gemini functionDeclarations format.
 */
function convertTools(tools: AiToolDefinition[]): Array<{ functionDeclarations: Array<{ name: string; description: string; parameters: Record<string, unknown> }> }> {
  return [{
    functionDeclarations: tools.map((t) => ({
      name: t.name,
      description: t.description,
      parameters: t.parameters,
    })),
  }];
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
    const url = `${cleanBaseUrl}/models/${encodeURIComponent(config.model)}:streamGenerateContent?alt=sse&key=${encodeURIComponent(config.apiKey)}`;

    const { systemInstruction, contents } = convertMessages(messages);

    const body: Record<string, unknown> = { contents };
    if (systemInstruction) {
      body.system_instruction = { parts: [{ text: systemInstruction }] };
    }
    if (config.maxResponseTokens) {
      body.generationConfig = { maxOutputTokens: config.maxResponseTokens };
    }
    if (config.tools && config.tools.length > 0) {
      body.tools = convertTools(config.tools);
    }

    const { response: statusPromise, body: streamBody } = aiFetchStreaming(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
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
      let errorMessage = `Gemini API error: ${status}`;
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

    const processDataLine = (line: string): AiStreamEvent[] => {
      if (!line.startsWith('data: ')) return [];
      const data = line.slice(6).trim();
      if (!data) return [];

      const events: AiStreamEvent[] = [];

      try {
        const json = JSON.parse(data);
        const candidates = json.candidates;
        if (candidates?.[0]?.content?.parts) {
          for (const part of candidates[0].content.parts) {
            if (part.text) {
              events.push({ type: 'content', content: part.text });
            }
            if (part.functionCall) {
              const callId = `gemini-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
              const args = JSON.stringify(part.functionCall.args || {});
              events.push({ type: 'tool_call_complete', id: callId, name: part.functionCall.name, arguments: args });
            }
          }
        }
      } catch {
        // Ignore parse errors
      }

      return events;
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
          for (const event of processDataLine(line)) {
            yield event;
          }
        }
      }

      if (buffer.trim()) {
        for (const event of processDataLine(buffer.trim())) {
          yield event;
        }
      }
    } finally {
      reader.releaseLock();
    }

    yield { type: 'done' };
  },

  async fetchModels(config: { baseUrl: string; apiKey: string }): Promise<string[]> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    const resp = await aiFetch(
      `${cleanBaseUrl}/v1beta/models?key=${config.apiKey}`
    );
    if (!resp.ok) throw new Error(`Failed to fetch models: ${resp.status}`);
    const data = JSON.parse(resp.body);
    if (!Array.isArray(data.models)) return [];
    return data.models
      .filter((m: { supportedGenerationMethods?: string[] }) =>
        m.supportedGenerationMethods?.includes('generateContent')
      )
      .map((m: { name: string }) => m.name.replace('models/', ''))
      .sort();
  },

  async fetchModelDetails(config: { baseUrl: string; apiKey: string }): Promise<Record<string, number>> {
    const cleanBaseUrl = config.baseUrl.replace(/\/+$/, '');
    const resp = await aiFetch(
      `${cleanBaseUrl}/v1beta/models?key=${config.apiKey}`
    );
    if (!resp.ok) return {};
    const data = JSON.parse(resp.body);
    if (!Array.isArray(data.models)) return {};
    const result: Record<string, number> = {};
    for (const m of data.models) {
      // Gemini returns inputTokenLimit
      const ctx = m.inputTokenLimit;
      const id = m.name?.replace('models/', '') || '';
      if (typeof ctx === 'number' && ctx > 0 && id) {
        result[id] = ctx;
      }
    }
    return result;
  },
};
