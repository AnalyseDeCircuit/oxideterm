/**
 * AI Provider Abstraction Layer
 *
 * Defines the common interface for all AI streaming providers.
 * Each provider implements this interface to handle API-specific differences.
 */

import type { AiProviderType } from '../../types';

// ═══════════════════════════════════════════════════════════════════════════
// Stream Event Types
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Events emitted by a streaming AI provider
 */
export type AiStreamEvent =
  | { type: 'content'; content: string }
  | { type: 'thinking'; content: string }
  | { type: 'done' }
  | { type: 'error'; message: string };

// ═══════════════════════════════════════════════════════════════════════════
// Provider Interface
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Configuration passed to the provider for each request
 */
export type AiRequestConfig = {
  baseUrl: string;
  model: string;
  apiKey: string;
};

/**
 * A chat message in the standard format
 */
export type ChatMessage = {
  role: 'user' | 'assistant' | 'system';
  content: string;
};

/**
 * Interface that all AI providers must implement
 */
export type AiStreamProvider = {
  /** Provider type identifier */
  readonly type: AiProviderType;

  /** Human-readable name */
  readonly displayName: string;

  /**
   * Stream a chat completion response
   *
   * @param config - API connection config (baseUrl, model, apiKey)
   * @param messages - Conversation messages
   * @param signal - AbortSignal for cancellation
   * @yields AiStreamEvent - Content chunks, thinking blocks, or done/error signals
   */
  streamCompletion(
    config: AiRequestConfig,
    messages: ChatMessage[],
    signal: AbortSignal
  ): AsyncGenerator<AiStreamEvent>;

  /**
   * Fetch available models from the provider's API.
   * Optional — providers without this will use their static model list.
   *
   * @param config - baseUrl and apiKey for auth
   * @returns Array of model ID strings
   */
  fetchModels?(config: { baseUrl: string; apiKey: string }): Promise<string[]>;
};

// ═══════════════════════════════════════════════════════════════════════════
// Default Provider Configurations
// ═══════════════════════════════════════════════════════════════════════════

export type DefaultProviderConfig = {
  type: AiProviderType;
  name: string;
  baseUrl: string;
  defaultModel: string;
  models: string[];
};

/**
 * Built-in provider configurations.
 * Models array is empty by default - users fetch models after configuring API key.
 */
export const DEFAULT_PROVIDERS: DefaultProviderConfig[] = [
  {
    type: 'openai',
    name: 'OpenAI',
    baseUrl: 'https://api.openai.com/v1',
    defaultModel: 'gpt-4o-mini',
    models: [],
  },
  {
    type: 'anthropic',
    name: 'Anthropic',
    baseUrl: 'https://api.anthropic.com',
    defaultModel: 'claude-sonnet-4-20250514',
    models: [],
  },
  {
    type: 'gemini',
    name: 'Google Gemini',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta',
    defaultModel: 'gemini-2.0-flash',
    models: [],
  },
  {
    type: 'ollama',
    name: 'Ollama (Local)',
    baseUrl: 'http://localhost:11434',
    defaultModel: '',
    models: [],
  },
];
