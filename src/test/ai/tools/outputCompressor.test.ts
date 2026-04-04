// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect } from 'vitest';
import {
  stripAnsi,
  collapseBlankLines,
  foldDuplicateLines,
  compressOutput,
} from '@/lib/ai/tools/outputCompressor';

// ═══════════════════════════════════════════════════════════════════════════
// stripAnsi
// ═══════════════════════════════════════════════════════════════════════════

describe('stripAnsi', () => {
  it('removes basic colour codes', () => {
    expect(stripAnsi('\x1b[31mERROR\x1b[0m')).toBe('ERROR');
  });

  it('removes 256-colour codes', () => {
    expect(stripAnsi('\x1b[38;5;196mred text\x1b[0m')).toBe('red text');
  });

  it('removes true-colour codes', () => {
    expect(stripAnsi('\x1b[38;2;255;0;0mred\x1b[0m')).toBe('red');
  });

  it('removes cursor movement sequences', () => {
    expect(stripAnsi('\x1b[2J\x1b[H')).toBe('');
  });

  it('returns plain text unchanged', () => {
    expect(stripAnsi('hello world')).toBe('hello world');
  });

  it('handles empty string', () => {
    expect(stripAnsi('')).toBe('');
  });

  it('removes bold/underline/blink codes', () => {
    expect(stripAnsi('\x1b[1m\x1b[4mbold underline\x1b[0m')).toBe('bold underline');
  });

  it('handles mixed ANSI and plain text', () => {
    const input = 'start \x1b[32mgreen\x1b[0m middle \x1b[31mred\x1b[0m end';
    expect(stripAnsi(input)).toBe('start green middle red end');
  });

  it('strips multiple consecutive escape sequences', () => {
    expect(stripAnsi('\x1b[1m\x1b[31m\x1b[4mtext\x1b[0m')).toBe('text');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// collapseBlankLines
// ═══════════════════════════════════════════════════════════════════════════

describe('collapseBlankLines', () => {
  it('collapses 3+ newlines to 2', () => {
    expect(collapseBlankLines('a\n\n\nb')).toBe('a\n\nb');
  });

  it('collapses 5 newlines to 2', () => {
    expect(collapseBlankLines('a\n\n\n\n\nb')).toBe('a\n\nb');
  });

  it('preserves exactly 2 newlines (single blank line)', () => {
    expect(collapseBlankLines('a\n\nb')).toBe('a\n\nb');
  });

  it('preserves single newline', () => {
    expect(collapseBlankLines('a\nb')).toBe('a\nb');
  });

  it('handles empty string', () => {
    expect(collapseBlankLines('')).toBe('');
  });

  it('collapses multiple separated runs', () => {
    expect(collapseBlankLines('a\n\n\nb\n\n\n\nc')).toBe('a\n\nb\n\nc');
  });

  it('handles text with no newlines', () => {
    expect(collapseBlankLines('hello world')).toBe('hello world');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// foldDuplicateLines
// ═══════════════════════════════════════════════════════════════════════════

describe('foldDuplicateLines', () => {
  it('folds 3+ identical consecutive lines', () => {
    const input = 'Processing...\nProcessing...\nProcessing...';
    const result = foldDuplicateLines(input);
    expect(result).toBe('Processing...\n(... repeated ×2)');
  });

  it('folds exactly 3 identical lines (boundary)', () => {
    const input = 'A\nA\nA';
    const result = foldDuplicateLines(input);
    expect(result).toBe('A\n(... repeated ×2)');
  });

  it('does NOT fold 2 identical lines', () => {
    const input = 'A\nA';
    expect(foldDuplicateLines(input)).toBe('A\nA');
  });

  it('does NOT fold 1 line', () => {
    expect(foldDuplicateLines('A')).toBe('A');
  });

  it('folds large run with correct count', () => {
    const lines = Array(10).fill('line').join('\n');
    const result = foldDuplicateLines(lines);
    expect(result).toBe('line\n(... repeated ×9)');
  });

  it('handles multiple separate runs', () => {
    const input = 'A\nA\nA\nB\nB\nB\nB';
    const result = foldDuplicateLines(input);
    expect(result).toBe('A\n(... repeated ×2)\nB\n(... repeated ×3)');
  });

  it('preserves non-duplicate lines', () => {
    const input = 'A\nB\nC';
    expect(foldDuplicateLines(input)).toBe('A\nB\nC');
  });

  it('handles empty string', () => {
    expect(foldDuplicateLines('')).toBe('');
  });

  it('handles mixed duplicate and unique lines', () => {
    const input = 'header\nA\nA\nA\nfooter';
    const result = foldDuplicateLines(input);
    expect(result).toBe('header\nA\n(... repeated ×2)\nfooter');
  });

  it('handles empty lines as duplicates', () => {
    const input = '\n\n\n';
    const result = foldDuplicateLines(input);
    expect(result).toBe('\n(... repeated ×3)');
  });

  it('folds lines with whitespace differences as unique', () => {
    const input = 'A \nA\nA';
    // "A " ≠ "A", so only "A" × 2 (below threshold)
    const result = foldDuplicateLines(input);
    expect(result).toBe('A \nA\nA');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// compressOutput (full pipeline)
// ═══════════════════════════════════════════════════════════════════════════

describe('compressOutput', () => {
  it('applies all stages: ANSI strip + blank collapse + fold + sanitize', () => {
    const input = '\x1b[31mERROR\x1b[0m\n\n\n\nProcessing...\nProcessing...\nProcessing...';
    const result = compressOutput(input);
    expect(result).not.toContain('\x1b[');
    expect(result).not.toMatch(/\n{3,}/);
    expect(result).toContain('(... repeated ×2)');
  });

  it('handles empty string', () => {
    expect(compressOutput('')).toBe('');
  });

  it('redacts secrets via sanitizeForAi', () => {
    const input = 'export API_KEY=sk-1234567890abcdef1234567890abcdef';
    const result = compressOutput(input);
    expect(result).toContain('[REDACTED]');
    expect(result).not.toContain('sk-1234567890abcdef');
  });

  it('preserves normal output', () => {
    const input = 'total 24\ndrwxr-xr-x 2 user group 4096 Jan 1 00:00 .';
    expect(compressOutput(input)).toBe(input);
  });
});
