// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect, vi, beforeEach } from 'vitest';

// ═══════════════════════════════════════════════════════════════════════════
// calculateBackoff & isTerminal are module-private functions.
// We test them indirectly via the store, or extract the math for testing.
// Since they aren't exported, we replicate the pure math logic here
// to verify correctness of the algorithm itself.
// ═══════════════════════════════════════════════════════════════════════════

// ── Backoff algorithm replicated from reconnectOrchestratorStore.ts ──
const BACKOFF_MULTIPLIER = 1.5;

function calculateBackoff(
  attempt: number,
  baseDelayMs = 1000,
  maxDelayMs = 15_000,
): number {
  const base = Math.min(
    baseDelayMs * Math.pow(BACKOFF_MULTIPLIER, Math.max(0, attempt - 1)),
    maxDelayMs,
  );
  const jitter = 0.8 + Math.random() * 0.4; // ±20%
  return Math.round(base * jitter);
}

type ReconnectPhase =
  | 'pending'
  | 'grace-period'
  | 'snapshot'
  | 'ssh-connect'
  | 'await-terminal'
  | 'restore-forwards'
  | 'resume-transfers'
  | 'restore-ide'
  | 'done'
  | 'failed'
  | 'cancelled';

function isTerminal(phase: ReconnectPhase): boolean {
  return phase === 'done' || phase === 'failed' || phase === 'cancelled';
}

// ═══════════════════════════════════════════════════════════════════════════
// isTerminal
// ═══════════════════════════════════════════════════════════════════════════

describe('isTerminal', () => {
  it('returns true for "done"', () => {
    expect(isTerminal('done')).toBe(true);
  });

  it('returns true for "failed"', () => {
    expect(isTerminal('failed')).toBe(true);
  });

  it('returns true for "cancelled"', () => {
    expect(isTerminal('cancelled')).toBe(true);
  });

  it('returns false for "pending"', () => {
    expect(isTerminal('pending')).toBe(false);
  });

  it('returns false for "ssh-connect"', () => {
    expect(isTerminal('ssh-connect')).toBe(false);
  });

  it('returns false for "grace-period"', () => {
    expect(isTerminal('grace-period')).toBe(false);
  });

  it('returns false for "snapshot"', () => {
    expect(isTerminal('snapshot')).toBe(false);
  });

  it('returns false for all pipeline phases', () => {
    const pipelinePhases: ReconnectPhase[] = [
      'pending', 'grace-period', 'snapshot', 'ssh-connect',
      'await-terminal', 'restore-forwards', 'resume-transfers', 'restore-ide',
    ];
    for (const phase of pipelinePhases) {
      expect(isTerminal(phase)).toBe(false);
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// calculateBackoff
// ═══════════════════════════════════════════════════════════════════════════

describe('calculateBackoff', () => {
  beforeEach(() => {
    vi.spyOn(Math, 'random').mockReturnValue(0.5); // jitter = 0.8 + 0.5*0.4 = 1.0
  });

  it('returns baseDelay for attempt 1', () => {
    const delay = calculateBackoff(1);
    // 1000 * 1.5^0 * 1.0 = 1000
    expect(delay).toBe(1000);
  });

  it('increases exponentially', () => {
    const d1 = calculateBackoff(1);
    const d2 = calculateBackoff(2);
    const d3 = calculateBackoff(3);
    expect(d2).toBeGreaterThan(d1);
    expect(d3).toBeGreaterThan(d2);
  });

  it('caps at maxDelayMs', () => {
    // Very high attempt should be capped
    const delay = calculateBackoff(100, 1000, 15000);
    expect(delay).toBeLessThanOrEqual(Math.round(15000 * 1.2)); // max + max jitter
  });

  it('attempt 2: base * 1.5^1 = 1500', () => {
    const delay = calculateBackoff(2);
    expect(delay).toBe(1500);
  });

  it('attempt 3: base * 1.5^2 = 2250', () => {
    const delay = calculateBackoff(3);
    expect(delay).toBe(2250);
  });

  it('handles attempt 0 same as attempt 1', () => {
    const d0 = calculateBackoff(0);
    const d1 = calculateBackoff(1);
    expect(d0).toBe(d1);
  });

  it('handles negative attempt', () => {
    const delay = calculateBackoff(-5);
    // max(0, -5 - 1) = 0, so 1.5^0 = 1
    expect(delay).toBe(1000);
  });

  it('applies jitter range ±20%', () => {
    vi.spyOn(Math, 'random').mockReturnValue(0.0); // jitter = 0.8
    const low = calculateBackoff(1);
    expect(low).toBe(800);

    vi.spyOn(Math, 'random').mockReturnValue(1.0); // jitter = 1.2
    const high = calculateBackoff(1);
    expect(high).toBe(1200);
  });

  it('respects custom base and max', () => {
    const delay = calculateBackoff(1, 500, 5000);
    expect(delay).toBe(500); // 500 * 1.5^0 * 1.0
  });

  it('caps custom max correctly', () => {
    const delay = calculateBackoff(100, 500, 5000);
    expect(delay).toBeLessThanOrEqual(Math.round(5000 * 1.2));
  });
});
