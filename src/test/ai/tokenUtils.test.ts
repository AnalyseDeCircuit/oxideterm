// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect } from 'vitest';
import {
  estimateTokens,
  getModelTokenCoefficient,
  estimateToolDefinitionsTokens,
  getModelContextWindow,
  responseReserve,
  trimHistoryToTokenBudget,
} from '@/lib/ai/tokenUtils';
import type { AiChatMessage } from '@/types';

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

function makeMsg(role: AiChatMessage['role'], content: string): AiChatMessage {
  return { id: crypto.randomUUID(), role, content, timestamp: Date.now() };
}

// ═══════════════════════════════════════════════════════════════════════════
// estimateTokens
// ═══════════════════════════════════════════════════════════════════════════

describe('estimateTokens', () => {
  it('returns 0 for empty string', () => {
    expect(estimateTokens('')).toBe(0);
  });

  it('returns 0 for undefined-ish falsy', () => {
    expect(estimateTokens(undefined as unknown as string)).toBe(0);
  });

  it('estimates English text (1 token ≈ 4 chars)', () => {
    const text = 'a'.repeat(100); // 100 chars → 25 raw tokens × 1.15 margin
    const tokens = estimateTokens(text);
    expect(tokens).toBe(Math.ceil(100 * 0.25 * 1.15)); // 29
  });

  it('weighs CJK characters higher (~1.5 tokens each)', () => {
    const text = '你好世界'; // 4 CJK chars → 6 raw tokens × 1.15
    const tokens = estimateTokens(text);
    expect(tokens).toBe(Math.ceil(4 * 1.5 * 1.15)); // 7
  });

  it('handles mixed CJK + Latin', () => {
    const text = 'hello你好'; // 5 latin + 2 CJK → 5*0.25 + 2*1.5 = 4.25 raw × 1.15
    const tokens = estimateTokens(text);
    expect(tokens).toBe(Math.ceil((5 * 0.25 + 2 * 1.5) * 1.15)); // 5
  });

  it('applies model coefficient for Claude', () => {
    const text = 'a'.repeat(100);
    const base = estimateTokens(text);
    const claude = estimateTokens(text, 'claude-3-opus');
    expect(claude).toBeGreaterThan(base); // coefficient 1.05
  });

  it('applies model coefficient for DeepSeek', () => {
    const text = 'a'.repeat(100);
    const base = estimateTokens(text);
    const ds = estimateTokens(text, 'deepseek-v3');
    expect(ds).toBeGreaterThan(base); // coefficient 1.08
  });

  it('uses 1.0 coefficient for unknown model', () => {
    const text = 'a'.repeat(100);
    const base = estimateTokens(text);
    const unknown = estimateTokens(text, 'some-custom-model');
    expect(unknown).toBe(base);
  });

  it('handles very long strings without crash', () => {
    const text = 'x'.repeat(1_000_000);
    const tokens = estimateTokens(text);
    expect(tokens).toBeGreaterThan(0);
    expect(tokens).toBeLessThan(500_000); // sanity upper bound
  });

  it('handles single character', () => {
    const tokens = estimateTokens('A');
    expect(tokens).toBeGreaterThanOrEqual(1);
  });

  it('handles Japanese hiragana', () => {
    const tokens = estimateTokens('あいうえお');
    expect(tokens).toBe(Math.ceil(5 * 1.5 * 1.15));
  });

  it('handles Korean hangul', () => {
    const tokens = estimateTokens('안녕하세요');
    expect(tokens).toBe(Math.ceil(5 * 1.5 * 1.15));
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// getModelTokenCoefficient
// ═══════════════════════════════════════════════════════════════════════════

describe('getModelTokenCoefficient', () => {
  it('returns 1.0 for GPT-4 models', () => {
    expect(getModelTokenCoefficient('gpt-4o')).toBe(1.0);
    expect(getModelTokenCoefficient('gpt-4-turbo')).toBe(1.0);
  });

  it('returns 1.05 for Claude', () => {
    expect(getModelTokenCoefficient('claude-3-opus')).toBe(1.05);
  });

  it('returns 1.08 for DeepSeek', () => {
    expect(getModelTokenCoefficient('deepseek-v3')).toBe(1.08);
  });

  it('returns 1.0 for unknown models', () => {
    expect(getModelTokenCoefficient('my-local-model')).toBe(1.0);
  });

  it('is case-insensitive', () => {
    expect(getModelTokenCoefficient('CLAUDE-3')).toBe(1.05);
    expect(getModelTokenCoefficient('DeepSeek-R1')).toBe(1.08);
  });

  it('matches o1/o3 models', () => {
    expect(getModelTokenCoefficient('o1-mini')).toBe(1.0);
    expect(getModelTokenCoefficient('o3')).toBe(1.0);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// estimateToolDefinitionsTokens
// ═══════════════════════════════════════════════════════════════════════════

describe('estimateToolDefinitionsTokens', () => {
  it('returns 0 for undefined tools', () => {
    expect(estimateToolDefinitionsTokens(undefined)).toBe(0);
  });

  it('returns 0 for empty array', () => {
    expect(estimateToolDefinitionsTokens([])).toBe(0);
  });

  it('counts tokens for tool definitions', () => {
    const tools = [
      { name: 'list_files', description: 'List files in directory', parameters: { type: 'object' } },
    ];
    const tokens = estimateToolDefinitionsTokens(tools);
    expect(tokens).toBeGreaterThan(10); // base 10 + name + desc + params
  });

  it('scales with multiple tools', () => {
    const oneTool = [{ name: 'a', description: 'b', parameters: {} }];
    const twoTools = [
      { name: 'a', description: 'b', parameters: {} },
      { name: 'c', description: 'd', parameters: {} },
    ];
    expect(estimateToolDefinitionsTokens(twoTools)).toBeGreaterThan(
      estimateToolDefinitionsTokens(oneTool),
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// getModelContextWindow
// ═══════════════════════════════════════════════════════════════════════════

describe('getModelContextWindow', () => {
  it('returns 128000 for gpt-4o', () => {
    expect(getModelContextWindow('gpt-4o')).toBe(128000);
  });

  it('returns 200000 for claude-3', () => {
    expect(getModelContextWindow('claude-3-opus')).toBe(200000);
  });

  it('returns 1048576 for gemini-2', () => {
    expect(getModelContextWindow('gemini-2')).toBe(1048576);
  });

  it('returns 128000 for deepseek-v3', () => {
    expect(getModelContextWindow('deepseek-v3')).toBe(128000);
  });

  it('returns default 8192 for unknown model', () => {
    expect(getModelContextWindow('totally-unknown-model')).toBe(8192);
  });

  it('uses cached context window from provider', () => {
    const cache = { 'my-provider': { 'custom-model': 999999 } };
    expect(getModelContextWindow('custom-model', cache, 'my-provider')).toBe(999999);
  });

  it('falls back to pattern when cache misses', () => {
    const cache = { 'my-provider': { 'other-model': 5000 } };
    expect(getModelContextWindow('gpt-4o', cache, 'my-provider')).toBe(128000);
  });

  it('is case-insensitive for pattern matching', () => {
    expect(getModelContextWindow('GPT-4O')).toBe(128000);
  });

  it('returns correct window for gpt-4.1', () => {
    expect(getModelContextWindow('gpt-4.1')).toBe(1048576);
  });

  it('returns correct window for llama-4', () => {
    expect(getModelContextWindow('llama-4')).toBe(1048576);
  });

  it('returns 200000 for o1/o3', () => {
    expect(getModelContextWindow('o1-mini')).toBe(200000);
    expect(getModelContextWindow('o3')).toBe(200000);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// responseReserve
// ═══════════════════════════════════════════════════════════════════════════

describe('responseReserve', () => {
  it('caps at 4096 for large context windows', () => {
    expect(responseReserve(128000)).toBe(4096);
  });

  it('uses ratio for small context windows', () => {
    // 8192 × 0.15 = 1228.8 → floor = 1228; min(4096, 1228) = 1228
    expect(responseReserve(8192)).toBe(1228);
  });

  it('returns 0 for zero context window', () => {
    expect(responseReserve(0)).toBe(0);
  });

  it('handles tiny context window', () => {
    // 100 × 0.15 = 15 → min(4096, 15) = 15
    expect(responseReserve(100)).toBe(15);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// trimHistoryToTokenBudget
// ═══════════════════════════════════════════════════════════════════════════

describe('trimHistoryToTokenBudget', () => {
  it('returns empty for empty messages', () => {
    const result = trimHistoryToTokenBudget([], 8192, 0, 0);
    expect(result.messages).toEqual([]);
    expect(result.trimmedCount).toBe(0);
  });

  it('keeps all messages when budget is sufficient', () => {
    const msgs = [makeMsg('user', 'hi'), makeMsg('assistant', 'hello')];
    const result = trimHistoryToTokenBudget(msgs, 128000, 100, 100);
    expect(result.messages).toHaveLength(2);
    expect(result.trimmedCount).toBe(0);
  });

  it('trims oldest messages first', () => {
    // Create messages with enough content to exceed a tight budget
    const msgs = Array.from({ length: 20 }, (_, i) =>
      makeMsg(i % 2 === 0 ? 'user' : 'assistant', 'x'.repeat(500)),
    );
    const result = trimHistoryToTokenBudget(msgs, 4096, 500, 500);
    expect(result.trimmedCount).toBeGreaterThan(0);
    expect(result.messages.length).toBeLessThan(msgs.length);
    // Last message is always preserved
    expect(result.messages[result.messages.length - 1]).toBe(msgs[msgs.length - 1]);
  });

  it('keeps at least the last message when budget <= 0', () => {
    const msgs = [makeMsg('user', 'hi'), makeMsg('assistant', 'bye')];
    // systemTokens + contextTokens > contextWindow * HISTORY_BUDGET_RATIO → budget ≤ 0
    const result = trimHistoryToTokenBudget(msgs, 100, 9999, 9999);
    expect(result.messages).toHaveLength(1);
    expect(result.messages[0]).toBe(msgs[msgs.length - 1]);
    expect(result.trimmedCount).toBe(1);
  });

  it('handles single message that exceeds budget', () => {
    const msgs = [makeMsg('user', 'x'.repeat(10000))];
    const result = trimHistoryToTokenBudget(msgs, 100, 0, 0);
    // Always keeps at least the last message
    expect(result.messages).toHaveLength(1);
  });

  it('preserves message order', () => {
    const msgs = [
      makeMsg('user', 'first'),
      makeMsg('assistant', 'second'),
      makeMsg('user', 'third'),
    ];
    const result = trimHistoryToTokenBudget(msgs, 128000, 0, 0);
    expect(result.messages.map(m => m.content)).toEqual(['first', 'second', 'third']);
  });
});
