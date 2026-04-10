// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * AI Token Estimation Utilities
 *
 * Shared token counting and history trimming logic used by:
 * - aiChatStore (dynamic history trimming before API calls)
 * - ContextIndicator (visual token usage display)
 *
 * These are heuristic estimates — actual tokenization varies by model.
 */

import type { AiChatMessage } from '../../types';
import {
  DEFAULT_CONTEXT_WINDOW as DEFAULT_CTX,
  HISTORY_BUDGET_RATIO,
  RESPONSE_RESERVE_RATIO,
  RESPONSE_RESERVE_CAP,
  TOKEN_SAFETY_MARGIN,
} from './constants';

// ═══════════════════════════════════════════════════════════════════════════
// Token Estimation
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Per-model token coefficient table.
 * Different tokenizers produce different token/char ratios.
 * Applied as a multiplier to the base heuristic estimate.
 */
const MODEL_TOKEN_COEFFICIENTS: Array<[RegExp, number]> = [
  // OpenAI cl100k_base / o200k_base — closely matches our default ratio
  [/gpt-4|o[1-9]/, 1.0],
  // Anthropic — slightly more tokens per char for code-heavy content
  [/claude/, 1.05],
  // DeepSeek — BPE tokenizer tends to run slightly higher for Latin
  [/deepseek/, 1.08],
  // Gemini — SentencePiece tokenizer; variable, close to default
  [/gemini/, 1.0],
  // Qwen — tiktoken-compatible; close to OpenAI ratio
  [/qwen/, 1.02],
  // Llama-based — SentencePiece; slightly higher for non-English
  [/llama/, 1.05],
  // Mistral — SentencePiece
  [/mistral/, 1.03],
];

/** Get the token coefficient for a model (defaults to 1.0). */
export function getModelTokenCoefficient(modelId: string): number {
  const lower = modelId.toLowerCase();
  for (const [pattern, coeff] of MODEL_TOKEN_COEFFICIENTS) {
    if (pattern.test(lower)) return coeff;
  }
  return 1.0;
}

/**
 * Rough token estimation (1 token ≈ 4 chars for English, ~1.5 for CJK).
 * A ×1.15 safety margin is applied to compensate for heuristic imprecision.
 * Optional modelId parameter applies per-model token coefficient.
 */
export function estimateTokens(text: string, modelId?: string): number {
  if (!text) return 0;

  // Count CJK characters (Chinese, Japanese, Korean)
  const cjkRegex = /[\u4e00-\u9fff\u3040-\u309f\u30a0-\u30ff\uac00-\ud7af]/g;
  const cjkMatches = text.match(cjkRegex);
  const cjkCount = cjkMatches?.length || 0;

  // Non-CJK characters
  const nonCjkLength = text.length - cjkCount;

  // CJK: ~1.5 tokens per char, Latin: ~0.25 tokens per char (1 token ≈ 4 chars)
  const raw = cjkCount * 1.5 + nonCjkLength * 0.25;
  const coeff = modelId ? getModelTokenCoefficient(modelId) : 1.0;
  return Math.ceil(raw * TOKEN_SAFETY_MARGIN * coeff);
}

/**
 * Estimate tokens consumed by tool definitions (JSON Schema sent to the model).
 * Serializes each tool's name + description + parameters to JSON and counts tokens.
 */
export function estimateToolDefinitionsTokens(tools: Array<{ name: string; description: string; parameters: Record<string, unknown> }> | undefined): number {
  if (!tools || tools.length === 0) return 0;
  let total = 0;
  for (const tool of tools) {
    // Each tool is sent as { type: "function", function: { name, description, parameters } }
    // The wrapper adds ~10 tokens overhead per tool
    total += 10 + estimateTokens(tool.name) + estimateTokens(tool.description) + estimateTokens(JSON.stringify(tool.parameters));
  }
  return total;
}

// ═══════════════════════════════════════════════════════════════════════════
// Context Window Lookup
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Hardcoded context window sizes by model name pattern.
 * Used as fallback when the provider API doesn't return context_length and
 * no user override is set.
 *
 * NOTE: OpenAI and Anthropic /models APIs do NOT return context window info,
 * so this table is the primary source for those providers.
 * Gemini and Ollama return context length via API and are cached separately.
 *
 * Last updated: 2026-04-10
 */
const MODEL_CONTEXT_WINDOWS: Array<[RegExp, number]> = [
  // OpenAI — newest first for priority
  [/gpt-4\.1/, 1048576],
  [/o[3-9][-.]|o[3-9]$/, 200000],
  [/o[1-2][-.]|o[1-2]$/, 200000],
  [/gpt-4o-mini/, 128000],
  [/gpt-4-turbo|gpt-4o/, 128000],
  [/gpt-4-32k/, 32768],
  [/gpt-4(?!o|-)/, 8192],
  [/gpt-3\.5-turbo-16k/, 16384],
  [/gpt-3\.5/, 4096],
  // Anthropic
  [/claude-4|claude-3\.7|claude-3\.6/, 200000],
  [/claude-3|claude-sonnet|claude-opus|claude-haiku/, 200000],
  [/claude-2/, 100000],
  [/claude/, 200000],
  // Google
  [/gemini-2\.5|gemini-2|gemini-1\.5/, 1048576],
  [/gemini/, 128000],
  // Meta — handle both llama3.2 (Ollama) and llama-3.2 (API) formats
  [/llama-?4/, 1048576],
  [/llama-?3\.1|llama-?3\.2|llama-?3\.3/, 128000],
  [/llama-?3/, 8192],
  [/llama/, 4096],
  // Mistral
  [/mistral-large|mistral-medium/, 128000],
  [/mixtral/, 32000],
  [/mistral/, 32000],
  // Alibaba Qwen
  [/qwen-?3|qwen3|qwen-?2\.5|qwen2\.5|qwen-max/, 128000],
  [/qwen/, 32000],
  // DeepSeek
  [/deepseek-v3|deepseek-r1/, 128000],
  [/deepseek/, 128000],
  // Moonshot
  [/moonshot/, 128000],
  // Zhipu GLM
  [/glm-4/, 128000],
  [/glm/, 32000],
  // Baidu ERNIE
  [/ernie/, 8192],
  // ByteDance Doubao
  [/doubao/, 128000],
  // MiniMax
  [/minimax|abab/, 245760],
  // Cohere
  [/command-r/, 128000],
  [/command/, 4096],
  // Yi
  [/yi-large|yi-lightning/, 32000],
  [/yi/, 4000],
];

/** Default context window for unknown models (re-exported from constants) */
const DEFAULT_CONTEXT_WINDOW = DEFAULT_CTX;

/**
 * Try to extract context window size from the model name.
 *
 * Many models encode their context size in their name, e.g.:
 *   moonshot-v1-128k → 131072
 *   doubao-lite-32k  → 32768
 *   chatglm2-6b-32k  → 32768
 *
 * Matches patterns like -128k, _32k, .8k where the number is
 * preceded by a separator (-, _, ., /, :) or start-of-string,
 * and followed by a separator or end-of-string.
 *
 * Returns the largest matched value, or null if none found.
 */
export function extractContextWindowFromModelName(modelId: string): number | null {
  const lower = modelId.toLowerCase();
  const re = /(?:^|[-_./:])(\d+)k(?=$|[-_./:@])/g;
  let best: number | null = null;
  let match: RegExpExecArray | null;

  while ((match = re.exec(lower)) !== null) {
    const n = parseInt(match[1], 10);
    const tokens = n * 1024;
    if (tokens >= 1024 && tokens <= 4 * 1024 * 1024) {
      if (best === null || tokens > best) best = tokens;
    }
  }

  return best;
}

export type ContextWindowSource = 'user' | 'api' | 'pattern' | 'name' | 'default';

/**
 * Get context window size for a model, including the source of the value.
 *
 * Priority:
 *   1. userContextWindows[providerId][modelId]
 *   2. cachedContextWindows[providerId][modelId]
 *   3. MODEL_CONTEXT_WINDOWS pattern matching
 *   4. extractContextWindowFromModelName
 *   5. DEFAULT_CONTEXT_WINDOW
 */
export function getModelContextWindowInfo(
  modelId: string,
  cachedContextWindows?: Record<string, Record<string, number>>,
  providerId?: string,
  userContextWindows?: Record<string, Record<string, number>>,
): { value: number; source: ContextWindowSource } {
  if (providerId && userContextWindows?.[providerId]?.[modelId]) {
    return { value: userContextWindows[providerId][modelId], source: 'user' };
  }

  if (providerId && cachedContextWindows?.[providerId]?.[modelId]) {
    return { value: cachedContextWindows[providerId][modelId], source: 'api' };
  }

  const lower = modelId.toLowerCase();
  for (const [pattern, size] of MODEL_CONTEXT_WINDOWS) {
    if (pattern.test(lower)) return { value: size, source: 'pattern' };
  }

  const extracted = extractContextWindowFromModelName(lower);
  if (extracted !== null) return { value: extracted, source: 'name' };

  return { value: DEFAULT_CONTEXT_WINDOW, source: 'default' };
}

/**
 * Get context window size for a model.
 *
 * @param modelId - The model identifier string
 * @param cachedContextWindows - Optional provider-scoped API cache: { [providerId]: { [modelId]: tokens } }
 * @param providerId - Optional provider id for scoped lookup (prevents cross-provider collisions)
 * @param userContextWindows - Optional user-configured overrides: { [providerId]: { [modelId]: tokens } }
 * @returns Context window size in tokens
 */
export function getModelContextWindow(
  modelId: string,
  cachedContextWindows?: Record<string, Record<string, number>>,
  providerId?: string,
  userContextWindows?: Record<string, Record<string, number>>,
): number {
  return getModelContextWindowInfo(modelId, cachedContextWindows, providerId, userContextWindows).value;
}

// ═══════════════════════════════════════════════════════════════════════════
// History Trimming
// ═══════════════════════════════════════════════════════════════════════════

/** Reserve tokens for the model's response — adaptive to context window size. */
export function responseReserve(contextWindow: number): number {
  // Cap at RESPONSE_RESERVE_CAP but never more than RESPONSE_RESERVE_RATIO of the window.
  // This prevents the reserve from consuming the entire budget on 4 k–8 k models.
  return Math.min(RESPONSE_RESERVE_CAP, Math.floor(contextWindow * RESPONSE_RESERVE_RATIO));
}

/**
 * Result of trimming conversation history.
 */
export type TrimResult = {
  /** Messages that fit within the token budget (most recent subset). */
  messages: AiChatMessage[];
  /** Number of messages that were dropped from the front. */
  trimmedCount: number;
};

/**
 * Trim conversation history to fit within a token budget.
 *
 * Strategy: keep the most recent messages, dropping oldest first.
 * Always keeps at least the last user message.
 *
 * @param messages - Full conversation history
 * @param contextWindow - Model's context window in tokens
 * @param systemTokens - Tokens consumed by system prompt(s)
 * @param contextTokens - Tokens consumed by terminal context injection
 * @returns TrimResult with the kept messages and count of trimmed messages
 */
export function trimHistoryToTokenBudget(
  messages: AiChatMessage[],
  contextWindow: number,
  systemTokens: number,
  contextTokens: number,
): TrimResult {
  // Budget = HISTORY_BUDGET_RATIO of context window minus fixed overhead
  const budget = Math.floor(contextWindow * HISTORY_BUDGET_RATIO) - systemTokens - contextTokens - responseReserve(contextWindow);

  if (budget <= 0) {
    // Edge case: not enough room, keep only the last message
    if (messages.length > 0) {
      return { messages: [messages[messages.length - 1]], trimmedCount: messages.length - 1 };
    }
    return { messages: [], trimmedCount: 0 };
  }

  // Walk backwards, accumulating tokens
  let accumulated = 0;
  let keepFrom = messages.length;

  for (let i = messages.length - 1; i >= 0; i--) {
    const msg = messages[i];
    const tokens = estimateTokens(msg.content);
    if (accumulated + tokens > budget && i < messages.length - 1) {
      // Would exceed budget, stop here (but always keep at least the last message)
      break;
    }
    accumulated += tokens;
    keepFrom = i;
  }

  return { messages: messages.slice(keepFrom), trimmedCount: keepFrom };
}
