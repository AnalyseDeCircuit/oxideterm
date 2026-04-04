// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect } from 'vitest';
import { parseSuggestions, hasPartialSuggestionsBlock } from '@/lib/ai/suggestionParser';

// ═══════════════════════════════════════════════════════════════════════════
// parseSuggestions
// ═══════════════════════════════════════════════════════════════════════════

describe('parseSuggestions', () => {
  // ── No Suggestions ──

  it('returns original content when no suggestions block', () => {
    const result = parseSuggestions('Hello, here is my response.');
    expect(result.cleanContent).toBe('Hello, here is my response.');
    expect(result.suggestions).toEqual([]);
  });

  it('handles empty string', () => {
    const result = parseSuggestions('');
    expect(result.cleanContent).toBe('');
    expect(result.suggestions).toEqual([]);
  });

  // ── Valid Suggestions ──

  it('extracts single suggestion', () => {
    const content = `Here is my answer.
<suggestions>
<s icon="Zap">Run the deploy script</s>
</suggestions>`;
    const result = parseSuggestions(content);
    expect(result.cleanContent).toBe('Here is my answer.');
    expect(result.suggestions).toEqual([{ icon: 'Zap', text: 'Run the deploy script' }]);
  });

  it('extracts multiple suggestions', () => {
    const content = `Analysis complete.
<suggestions>
<s icon="Zap">Run the deploy script</s>
<s icon="Search">Show container logs</s>
<s icon="Bug">Debug the connection timeout</s>
</suggestions>`;
    const result = parseSuggestions(content);
    expect(result.cleanContent).toBe('Analysis complete.');
    expect(result.suggestions).toHaveLength(3);
    expect(result.suggestions[0]).toEqual({ icon: 'Zap', text: 'Run the deploy script' });
    expect(result.suggestions[1]).toEqual({ icon: 'Search', text: 'Show container logs' });
    expect(result.suggestions[2]).toEqual({ icon: 'Bug', text: 'Debug the connection timeout' });
  });

  // ── Max Cap ──

  it('caps at 5 suggestions', () => {
    const items = Array.from(
      { length: 8 },
      (_, i) => `<s icon="Zap">Suggestion ${i}</s>`,
    ).join('\n');
    const content = `Response.\n<suggestions>\n${items}\n</suggestions>`;
    const result = parseSuggestions(content);
    expect(result.suggestions).toHaveLength(5);
  });

  // ── Filtering ──

  it('skips empty text suggestions', () => {
    const content = `Response.
<suggestions>
<s icon="Zap"></s>
<s icon="Search">Valid one</s>
</suggestions>`;
    const result = parseSuggestions(content);
    expect(result.suggestions).toHaveLength(1);
    expect(result.suggestions[0].text).toBe('Valid one');
  });

  it('skips suggestions with text > 200 chars', () => {
    const longText = 'a'.repeat(201);
    const content = `Response.
<suggestions>
<s icon="Zap">${longText}</s>
<s icon="Search">Short</s>
</suggestions>`;
    const result = parseSuggestions(content);
    expect(result.suggestions).toHaveLength(1);
    expect(result.suggestions[0].text).toBe('Short');
  });

  it('skips suggestions with icon > 30 chars', () => {
    const longIcon = 'A'.repeat(31);
    const content = `Response.
<suggestions>
<s icon="${longIcon}">Text</s>
<s icon="Search">Valid</s>
</suggestions>`;
    const result = parseSuggestions(content);
    expect(result.suggestions).toHaveLength(1);
    expect(result.suggestions[0].text).toBe('Valid');
  });

  // ── Edge Cases ──

  it('ignores suggestions block mid-content (only at end)', () => {
    const content = `<suggestions>
<s icon="Zap">First</s>
</suggestions>
More text after.`;
    const result = parseSuggestions(content);
    // Block must be at the end
    expect(result.suggestions).toEqual([]);
    expect(result.cleanContent).toBe(content);
  });

  it('trims whitespace from cleanContent', () => {
    const content = `Response with trailing space   
<suggestions>
<s icon="Zap">Do thing</s>
</suggestions>`;
    const result = parseSuggestions(content);
    expect(result.cleanContent).toBe('Response with trailing space');
  });

  it('handles empty suggestions block', () => {
    const content = `Response.
<suggestions>
</suggestions>`;
    const result = parseSuggestions(content);
    expect(result.cleanContent).toBe('Response.');
    expect(result.suggestions).toEqual([]);
  });

  it('trims icon and text whitespace', () => {
    const content = `Response.
<suggestions>
<s icon="  Search  ">  Trim me  </s>
</suggestions>`;
    const result = parseSuggestions(content);
    expect(result.suggestions[0]).toEqual({ icon: 'Search', text: 'Trim me' });
  });

  it('handles multiline suggestion text', () => {
    const content = `Response.
<suggestions>
<s icon="Zap">Line 1
Line 2</s>
</suggestions>`;
    const result = parseSuggestions(content);
    expect(result.suggestions[0].text).toBe('Line 1\nLine 2');
  });

  it('keeps text at exactly 200 chars', () => {
    const exactText = 'b'.repeat(200);
    const content = `Response.\n<suggestions>\n<s icon="Zap">${exactText}</s>\n</suggestions>`;
    const result = parseSuggestions(content);
    expect(result.suggestions).toHaveLength(1);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// hasPartialSuggestionsBlock
// ═══════════════════════════════════════════════════════════════════════════

describe('hasPartialSuggestionsBlock', () => {
  it('returns false for no suggestions', () => {
    expect(hasPartialSuggestionsBlock('Hello world')).toBe(false);
  });

  it('returns true for open tag without close', () => {
    expect(hasPartialSuggestionsBlock('Response\n<suggestions>\n<s icon="Zap">...')).toBe(true);
  });

  it('returns false for complete block', () => {
    expect(
      hasPartialSuggestionsBlock('Response\n<suggestions>\n<s icon="Zap">X</s>\n</suggestions>'),
    ).toBe(false);
  });

  it('returns false for empty string', () => {
    expect(hasPartialSuggestionsBlock('')).toBe(false);
  });
});
