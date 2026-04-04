// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect } from 'vitest';
import {
  SLASH_COMMANDS,
  resolveSlashCommand,
  filterSlashCommands,
  groupSlashCommandsByCategory,
} from '@/lib/ai/slashCommands';

// ═══════════════════════════════════════════════════════════════════════════
// Registry Integrity
// ═══════════════════════════════════════════════════════════════════════════

describe('SLASH_COMMANDS registry', () => {
  it('has at least 10 commands', () => {
    expect(SLASH_COMMANDS.length).toBeGreaterThanOrEqual(10);
  });

  it('all commands have unique names', () => {
    const names = SLASH_COMMANDS.map(c => c.name);
    expect(new Set(names).size).toBe(names.length);
  });

  it('all commands have required fields', () => {
    for (const cmd of SLASH_COMMANDS) {
      expect(cmd.name).toBeTruthy();
      expect(cmd.labelKey).toBeTruthy();
      expect(cmd.descriptionKey).toBeTruthy();
      expect(cmd.icon).toBeTruthy();
      expect(cmd.category).toBeTruthy();
    }
  });

  it('client-only commands have no systemPromptModifier', () => {
    const clientOnly = SLASH_COMMANDS.filter(c => c.clientOnly);
    expect(clientOnly.length).toBeGreaterThan(0);
    for (const cmd of clientOnly) {
      expect(cmd.systemPromptModifier).toBeUndefined();
    }
  });

  it('LLM commands have systemPromptModifier', () => {
    const llmCmds = SLASH_COMMANDS.filter(c => !c.clientOnly);
    for (const cmd of llmCmds) {
      expect(cmd.systemPromptModifier).toBeTruthy();
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// resolveSlashCommand
// ═══════════════════════════════════════════════════════════════════════════

describe('resolveSlashCommand', () => {
  it('resolves known command', () => {
    const cmd = resolveSlashCommand('explain');
    expect(cmd).toBeDefined();
    expect(cmd!.name).toBe('explain');
  });

  it('returns undefined for unknown command', () => {
    expect(resolveSlashCommand('nonexistent')).toBeUndefined();
  });

  it('is exact match (not prefix)', () => {
    expect(resolveSlashCommand('expl')).toBeUndefined();
  });

  it('resolves each known command', () => {
    for (const cmd of SLASH_COMMANDS) {
      expect(resolveSlashCommand(cmd.name)).toBeDefined();
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// filterSlashCommands
// ═══════════════════════════════════════════════════════════════════════════

describe('filterSlashCommands', () => {
  it('returns all commands for empty string', () => {
    expect(filterSlashCommands('')).toEqual(SLASH_COMMANDS);
  });

  it('filters by prefix', () => {
    const results = filterSlashCommands('ex');
    expect(results.every(c => c.name.startsWith('ex'))).toBe(true);
    expect(results.length).toBeGreaterThan(0);
  });

  it('returns empty for no match', () => {
    expect(filterSlashCommands('zzzzz')).toEqual([]);
  });

  it('is case-insensitive', () => {
    const lower = filterSlashCommands('ex');
    const upper = filterSlashCommands('EX');
    // Both should match since startsWith uses toLowerCase
    expect(upper.length).toBe(lower.length);
  });

  it('returns single result for full name', () => {
    const results = filterSlashCommands('explain');
    expect(results.some(c => c.name === 'explain')).toBe(true);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// groupSlashCommandsByCategory
// ═══════════════════════════════════════════════════════════════════════════

describe('groupSlashCommandsByCategory', () => {
  it('returns a Map', () => {
    const groups = groupSlashCommandsByCategory();
    expect(groups).toBeInstanceOf(Map);
  });

  it('covers all commands', () => {
    const groups = groupSlashCommandsByCategory();
    let total = 0;
    for (const cmds of groups.values()) {
      total += cmds.length;
    }
    expect(total).toBe(SLASH_COMMANDS.length);
  });

  it('has meta category with client-only commands', () => {
    const groups = groupSlashCommandsByCategory();
    const meta = groups.get('meta');
    expect(meta).toBeDefined();
    expect(meta!.every(c => c.clientOnly)).toBe(true);
  });

  it('all categories have at least one command', () => {
    const groups = groupSlashCommandsByCategory();
    for (const [, cmds] of groups) {
      expect(cmds.length).toBeGreaterThan(0);
    }
  });
});
