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
 * Used as fallback when the provider API doesn't return context_length.
 *
 * NOTE: OpenAI and Anthropic /models APIs do NOT return context window info,
 * so this table is the only source for those providers.
 * Gemini returns inputTokenLimit via API and is cached separately.
 *
 * Last updated: 2026-03-01
 */
const MODEL_CONTEXT_WINDOWS: Array<[RegExp, number]> = [
  // OpenAI — newest first for priority
  [/gpt-4\.1/, 1048576],
  [/o[3-9]-|o[3-9]$/, 200000],
  [/o[1-2]-|o[1-2]$/, 200000],
  [/gpt-4o-mini/, 128000],
  [/gpt-4-turbo|gpt-4o/, 128000],
  [/gpt-4-32k/, 32000],
  [/gpt-4(?!o|-)/, 8192],
  [/gpt-3\.5-turbo-16k/, 16000],
  [/gpt-3\.5/, 4096],
  // Anthropic
  [/claude-4|claude-3\.7|claude-3\.6/, 200000],
  [/claude-3|claude-sonnet|claude-opus|claude-haiku/, 200000],
  [/claude-2/, 100000],
  [/claude/, 100000],
  // Google
  [/gemini-2\.5|gemini-2|gemini-1\.5/, 1048576],
  [/gemini/, 128000],
  // Meta
  [/llama-4/, 1048576],
  [/llama-3\.1|llama-3\.2|llama-3\.3/, 128000],
  [/llama-3/, 8192],
  // Mistral
  [/mistral-large|mistral-medium/, 128000],
  [/mistral/, 32000],
  // Alibaba
  [/qwen-3|qwen3|qwen-2\.5|qwen2\.5|qwen-max/, 128000],
  [/qwen/, 32000],
  // DeepSeek
  [/deepseek-v3|deepseek-r1/, 128000],
  [/deepseek/, 128000],
];

/** Default context window for unknown models (re-exported from constants) */
const DEFAULT_CONTEXT_WINDOW = DEFAULT_CTX;

/**
 * Get context window size for a model.
 *
 * @param modelId - The model identifier string
 * @param cachedContextWindows - Optional provider-scoped cache: { [providerId]: { [modelId]: tokens } }
 * @param providerId - Optional provider id for scoped lookup (prevents cross-provider collisions)
 * @returns Context window size in tokens
 */
export function getModelContextWindow(
  modelId: string,
  cachedContextWindows?: Record<string, Record<string, number>>,
  providerId?: string,
): number {
  // 1. Provider-scoped lookup (most accurate, avoids cross-provider name collisions)
  if (providerId && cachedContextWindows?.[providerId]?.[modelId]) {
    return cachedContextWindows[providerId][modelId];
  }

  // 2. Fallback to pattern matching
  const lower = modelId.toLowerCase();
  for (const [pattern, size] of MODEL_CONTEXT_WINDOWS) {
    if (pattern.test(lower)) return size;
  }

  return DEFAULT_CONTEXT_WINDOW;
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
