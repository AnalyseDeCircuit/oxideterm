// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect } from 'vitest';
import { parseUserInput, getTokenAtCursor } from '@/lib/ai/inputParser';

// ═══════════════════════════════════════════════════════════════════════════
// parseUserInput
// ═══════════════════════════════════════════════════════════════════════════

describe('parseUserInput', () => {
  // ── Basic / Slash Commands ──

  it('returns null slashCommand for plain text', () => {
    const result = parseUserInput('hello world');
    expect(result.slashCommand).toBeNull();
    expect(result.cleanText).toBe('hello world');
    expect(result.rawText).toBe('hello world');
  });

  it('extracts slash command at start', () => {
    const result = parseUserInput('/explain what is nginx');
    expect(result.slashCommand).toEqual({ name: 'explain', raw: '/explain ' });
    expect(result.cleanText).toBe('what is nginx');
  });

  it('ignores slash command mid-text', () => {
    const result = parseUserInput('please /explain this');
    expect(result.slashCommand).toBeNull();
    expect(result.cleanText).toBe('please /explain this');
  });

  it('handles slash command with no trailing text', () => {
    const result = parseUserInput('/help');
    expect(result.slashCommand).toEqual({ name: 'help', raw: '/help' });
    expect(result.cleanText).toBe('');
  });

  it('handles slash command with underscores', () => {
    const result = parseUserInput('/my_command some text');
    expect(result.slashCommand?.name).toBe('my_command');
  });

  it('rejects slash commands with uppercase', () => {
    const result = parseUserInput('/Explain this');
    expect(result.slashCommand).toBeNull();
  });

  // ── Participants ──

  it('extracts single @participant', () => {
    const result = parseUserInput('show logs @terminal');
    expect(result.participants).toEqual([{ name: 'terminal', raw: '@terminal' }]);
    expect(result.cleanText).toBe('show logs');
  });

  it('extracts multiple @participants', () => {
    const result = parseUserInput('@terminal @sftp list files');
    expect(result.participants).toHaveLength(2);
    expect(result.participants[0].name).toBe('terminal');
    expect(result.participants[1].name).toBe('sftp');
  });

  it('deduplicates repeated @participants', () => {
    const result = parseUserInput('@terminal do thing @terminal');
    expect(result.participants).toHaveLength(1);
  });

  // ── References ──

  it('extracts #reference without value', () => {
    const result = parseUserInput('show me #buffer');
    expect(result.references).toEqual([{ type: 'buffer', value: undefined, raw: '#buffer' }]);
  });

  it('extracts #reference with colon value', () => {
    const result = parseUserInput('read #file:/etc/nginx.conf');
    expect(result.references).toEqual([
      { type: 'file', value: '/etc/nginx.conf', raw: '#file:/etc/nginx.conf' },
    ]);
  });

  it('extracts multiple references', () => {
    const result = parseUserInput('#buffer #pane:2 what happened');
    expect(result.references).toHaveLength(2);
    expect(result.references[0].type).toBe('buffer');
    expect(result.references[1]).toEqual({ type: 'pane', value: '2', raw: '#pane:2' });
  });

  // ── Combined ──

  it('extracts slash + participant + reference together', () => {
    const result = parseUserInput('/explain @terminal #buffer what is this');
    expect(result.slashCommand?.name).toBe('explain');
    expect(result.participants[0].name).toBe('terminal');
    expect(result.references[0].type).toBe('buffer');
    expect(result.cleanText).toBe('what is this');
  });

  // ── Edge Cases ──

  it('handles empty string', () => {
    const result = parseUserInput('');
    expect(result.slashCommand).toBeNull();
    expect(result.participants).toEqual([]);
    expect(result.references).toEqual([]);
    expect(result.cleanText).toBe('');
    expect(result.rawText).toBe('');
  });

  it('handles only whitespace', () => {
    const result = parseUserInput('   ');
    expect(result.cleanText).toBe('');
  });

  it('handles CJK text', () => {
    const result = parseUserInput('/explain 这是什么');
    expect(result.slashCommand?.name).toBe('explain');
    expect(result.cleanText).toBe('这是什么');
  });

  it('handles emoji in text', () => {
    const result = parseUserInput('🚀 deploy @terminal');
    expect(result.participants[0].name).toBe('terminal');
    expect(result.cleanText).toBe('🚀 deploy');
  });

  it('collapses multiple spaces after token removal', () => {
    const result = parseUserInput('show   @terminal   logs');
    expect(result.cleanText).toBe('show logs');
  });

  it('preserves rawText untouched', () => {
    const original = '/explain @terminal #buffer show stuff';
    const result = parseUserInput(original);
    expect(result.rawText).toBe(original);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// getTokenAtCursor
// ═══════════════════════════════════════════════════════════════════════════

describe('getTokenAtCursor', () => {
  it('detects slash at start', () => {
    const result = getTokenAtCursor('/expl', 5);
    expect(result.type).toBe('slash');
    expect(result.partial).toBe('expl');
    expect(result.start).toBe(0);
  });

  it('detects @participant mid-text', () => {
    const result = getTokenAtCursor('hello @ter', 10);
    expect(result.type).toBe('participant');
    expect(result.partial).toBe('ter');
  });

  it('detects #reference', () => {
    const result = getTokenAtCursor('show #buf', 9);
    expect(result.type).toBe('reference');
    expect(result.partial).toBe('buf');
  });

  it('returns null type for plain text', () => {
    const result = getTokenAtCursor('hello world', 5);
    expect(result.type).toBeNull();
  });

  it('handles cursor at position 0', () => {
    const result = getTokenAtCursor('hello', 0);
    expect(result.type).toBeNull();
    expect(result.partial).toBe('');
  });

  it('handles cursor past end of text', () => {
    const result = getTokenAtCursor('hi', 10);
    expect(result.type).toBeNull();
  });

  it('does not detect slash mid-text as slash command', () => {
    const result = getTokenAtCursor('foo /bar', 8);
    expect(result.type).toBeNull();
  });

  it('detects just the @ prefix', () => {
    const result = getTokenAtCursor('hello @', 7);
    expect(result.type).toBe('participant');
    expect(result.partial).toBe('');
  });

  it('detects just the # prefix', () => {
    const result = getTokenAtCursor('hello #', 7);
    expect(result.type).toBe('reference');
    expect(result.partial).toBe('');
  });

  it('handles empty text', () => {
    const result = getTokenAtCursor('', 0);
    expect(result.type).toBeNull();
    expect(result.partial).toBe('');
  });
});
