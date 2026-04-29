// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect } from 'vitest';
import {
  PARTICIPANTS,
  resolveParticipant,
  filterParticipants,
} from '@/lib/ai/participants';

const ACTIVE_PARTICIPANTS = ['terminal', 'sftp', 'ide', 'local', 'settings', 'knowledge'];
const REMOVED_PARTICIPANTS = ['connection', 'system'];

// ═══════════════════════════════════════════════════════════════════════════
// Registry Integrity
// ═══════════════════════════════════════════════════════════════════════════

describe('PARTICIPANTS registry', () => {
  it('only exposes the supported participant set', () => {
    expect(PARTICIPANTS.map(p => p.name)).toEqual(ACTIVE_PARTICIPANTS);
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
      expect(p.systemPromptModifier).toBeTruthy();
      expect(p.intentHint).toBeTruthy();
      expect(p.preferredTargetView).toBeTruthy();
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

  it('does not resolve removed broad participants', () => {
    for (const name of REMOVED_PARTICIPANTS) {
      expect(resolveParticipant(name)).toBeUndefined();
    }
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
