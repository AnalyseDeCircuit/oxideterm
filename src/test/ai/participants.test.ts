// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect } from 'vitest';
import {
  PARTICIPANTS,
  resolveParticipant,
  filterParticipants,
  mergeParticipantTools,
} from '@/lib/ai/participants';

// ═══════════════════════════════════════════════════════════════════════════
// Registry Integrity
// ═══════════════════════════════════════════════════════════════════════════

describe('PARTICIPANTS registry', () => {
  it('has at least 5 participants', () => {
    expect(PARTICIPANTS.length).toBeGreaterThanOrEqual(5);
  });

  it('all participants have unique names', () => {
    const names = PARTICIPANTS.map(p => p.name);
    expect(new Set(names).size).toBe(names.length);
  });

  it('all participants have required fields', () => {
    for (const p of PARTICIPANTS) {
      expect(p.name).toBeTruthy();
      expect(p.labelKey).toBeTruthy();
      expect(p.descriptionKey).toBeTruthy();
      expect(p.icon).toBeTruthy();
      expect(Array.isArray(p.includeTools)).toBe(true);
      expect(p.systemPromptModifier).toBeTruthy();
    }
  });

  it('all includeTools are non-empty strings', () => {
    for (const p of PARTICIPANTS) {
      for (const tool of p.includeTools) {
        expect(typeof tool).toBe('string');
        expect(tool.length).toBeGreaterThan(0);
      }
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// resolveParticipant
// ═══════════════════════════════════════════════════════════════════════════

describe('resolveParticipant', () => {
  it('resolves known participant', () => {
    const p = resolveParticipant('terminal');
    expect(p).toBeDefined();
    expect(p!.name).toBe('terminal');
  });

  it('returns undefined for unknown participant', () => {
    expect(resolveParticipant('nonexistent')).toBeUndefined();
  });

  it('resolves every participant in registry', () => {
    for (const p of PARTICIPANTS) {
      expect(resolveParticipant(p.name)).toBeDefined();
    }
  });

  it('is exact match', () => {
    expect(resolveParticipant('term')).toBeUndefined();
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// filterParticipants
// ═══════════════════════════════════════════════════════════════════════════

describe('filterParticipants', () => {
  it('returns all for empty string', () => {
    expect(filterParticipants('')).toEqual(PARTICIPANTS);
  });

  it('filters by prefix', () => {
    const results = filterParticipants('ter');
    expect(results.length).toBeGreaterThan(0);
    expect(results.every(p => p.name.startsWith('ter'))).toBe(true);
  });

  it('returns empty for no match', () => {
    expect(filterParticipants('zzzz')).toEqual([]);
  });

  it('is case-insensitive', () => {
    const lower = filterParticipants('ter');
    const upper = filterParticipants('TER');
    expect(upper.length).toBe(lower.length);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// mergeParticipantTools
// ═══════════════════════════════════════════════════════════════════════════

describe('mergeParticipantTools', () => {
  it('returns empty set for empty names', () => {
    expect(mergeParticipantTools([])).toEqual(new Set());
  });

  it('returns tools for single participant', () => {
    const tools = mergeParticipantTools(['terminal']);
    expect(tools.size).toBeGreaterThan(0);
    expect(tools.has('terminal_exec')).toBe(true);
  });

  it('merges tools from multiple participants', () => {
    const tools = mergeParticipantTools(['terminal', 'sftp']);
    // Should have tools from both
    expect(tools.has('terminal_exec')).toBe(true);
    expect(tools.has('sftp_list_dir')).toBe(true);
  });

  it('deduplicates tools', () => {
    const single = mergeParticipantTools(['terminal']);
    const double = mergeParticipantTools(['terminal', 'terminal']);
    expect(double.size).toBe(single.size);
  });

  it('ignores unknown participant names', () => {
    const tools = mergeParticipantTools(['terminal', 'nonexistent']);
    const expected = mergeParticipantTools(['terminal']);
    expect(tools.size).toBe(expected.size);
  });

  it('handles all participants combined', () => {
    const allNames = PARTICIPANTS.map(p => p.name);
    const tools = mergeParticipantTools(allNames);
    // Should be the union of all tools
    const expectedCount = new Set(PARTICIPANTS.flatMap(p => p.includeTools)).size;
    expect(tools.size).toBe(expectedCount);
  });
});
