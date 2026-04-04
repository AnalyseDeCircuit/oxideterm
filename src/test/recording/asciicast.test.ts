// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, it, expect } from 'vitest';
import {
  parseAsciicast,
  serialiseAsciicast,
  mergeAdjacentEvents,
  applyIdleTimeLimit,
} from '@/lib/recording/asciicast';
import type { AsciicastHeader, AsciicastEvent } from '@/lib/recording/types';

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

const baseHeader: AsciicastHeader = {
  version: 2,
  width: 80,
  height: 24,
};

function v2File(header: AsciicastHeader, events: AsciicastEvent[]): string {
  return [JSON.stringify(header), ...events.map(e => JSON.stringify(e))].join('\n');
}

function v1File(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    version: 1,
    width: 80,
    height: 24,
    duration: 10,
    stdout: [
      [0.5, 'hello'],
      [1.0, 'world'],
    ],
    ...overrides,
  });
}

// ═══════════════════════════════════════════════════════════════════════════
// parseAsciicast
// ═══════════════════════════════════════════════════════════════════════════

describe('parseAsciicast', () => {
  // ── v2 ──

  it('parses valid v2 NDJSON', () => {
    const events: AsciicastEvent[] = [
      [0.5, 'o', 'hello'],
      [1.0, 'o', 'world'],
    ];
    const result = parseAsciicast(v2File(baseHeader, events));
    expect(result.header.version).toBe(2);
    expect(result.header.width).toBe(80);
    expect(result.events).toHaveLength(2);
    expect(result.events[0]).toEqual([0.5, 'o', 'hello']);
  });

  it('parses v2 with input events', () => {
    const events: AsciicastEvent[] = [
      [0.1, 'i', 'ls\n'],
      [0.5, 'o', 'file.txt'],
    ];
    const result = parseAsciicast(v2File(baseHeader, events));
    expect(result.events).toHaveLength(2);
    expect(result.events[0][1]).toBe('i');
  });

  it('skips malformed event lines', () => {
    const content = [
      JSON.stringify(baseHeader),
      JSON.stringify([0.5, 'o', 'good']),
      'not json at all',
      JSON.stringify([1.0, 'o', 'also good']),
    ].join('\n');
    const result = parseAsciicast(content);
    expect(result.events).toHaveLength(2);
  });

  it('handles v2 file with header and empty event lines', () => {
    // A v2 header followed by empty lines — events array should be empty
    const content = JSON.stringify(baseHeader) + '\n' + JSON.stringify([0.0, 'o', '']);
    const result = parseAsciicast(content);
    expect(result.header.version).toBe(2);
    expect(result.events).toHaveLength(1);
  });

  // ── v1 ──

  it('parses v1 format and converts to v2', () => {
    const result = parseAsciicast(v1File());
    expect(result.header.version).toBe(2);
    expect(result.events).toHaveLength(2);
    expect(result.events[0]).toEqual([0.5, 'o', 'hello']);
  });

  it('preserves v1 metadata fields', () => {
    const result = parseAsciicast(
      v1File({ command: '/bin/bash', title: 'test recording' }),
    );
    expect(result.header.command).toBe('/bin/bash');
    expect(result.header.title).toBe('test recording');
  });

  it('handles v1 with empty stdout', () => {
    const result = parseAsciicast(v1File({ stdout: [] }));
    expect(result.events).toHaveLength(0);
  });

  // ── Error Cases ──

  it('throws for empty input', () => {
    expect(() => parseAsciicast('')).toThrow('Empty asciicast');
  });

  it('throws for whitespace-only input', () => {
    expect(() => parseAsciicast('   \n  ')).toThrow('Empty asciicast');
  });

  it('throws for wrong v2 version number', () => {
    const header = { ...baseHeader, version: 3 };
    expect(() => parseAsciicast(v2File(header as AsciicastHeader, [[0, 'o', 'x']]))).toThrow();
  });

  it('throws for wrong v1 version number', () => {
    expect(() =>
      parseAsciicast(JSON.stringify({ version: 99, width: 80, height: 24, stdout: [] })),
    ).toThrow();
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// serialiseAsciicast
// ═══════════════════════════════════════════════════════════════════════════

describe('serialiseAsciicast', () => {
  it('produces valid NDJSON', () => {
    const events: AsciicastEvent[] = [[0.5, 'o', 'hello']];
    const output = serialiseAsciicast(baseHeader, events);
    const lines = output.split('\n');
    // Header + 1 event + trailing newline → 3 elements, last is empty
    expect(lines).toHaveLength(3);
    expect(JSON.parse(lines[0])).toMatchObject({ version: 2 });
    expect(JSON.parse(lines[1])).toEqual([0.5, 'o', 'hello']);
  });

  it('ends with newline', () => {
    const output = serialiseAsciicast(baseHeader, []);
    expect(output.endsWith('\n')).toBe(true);
  });

  it('serialises empty events', () => {
    const output = serialiseAsciicast(baseHeader, []);
    const lines = output.trim().split('\n');
    expect(lines).toHaveLength(1); // header only
  });

  it('roundtrips: parse(serialise(h, e)) === {h, e}', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'start'],
      [1.5, 'i', 'x'],
      [2.0, 'o', 'end'],
    ];
    const text = serialiseAsciicast(baseHeader, events);
    const parsed = parseAsciicast(text);
    expect(parsed.header).toMatchObject(baseHeader);
    expect(parsed.events).toEqual(events);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// mergeAdjacentEvents
// ═══════════════════════════════════════════════════════════════════════════

describe('mergeAdjacentEvents', () => {
  it('returns empty for empty input', () => {
    expect(mergeAdjacentEvents([])).toEqual([]);
  });

  it('returns single event unchanged', () => {
    const events: AsciicastEvent[] = [[0.0, 'o', 'hello']];
    expect(mergeAdjacentEvents(events)).toEqual(events);
  });

  it('merges adjacent output events within threshold', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'hel'],
      [0.01, 'o', 'lo'],
    ];
    const result = mergeAdjacentEvents(events);
    expect(result).toHaveLength(1);
    expect(result[0][2]).toBe('hello');
    expect(result[0][0]).toBe(0.0); // keeps first timestamp
  });

  it('does NOT merge events beyond threshold', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'a'],
      [0.1, 'o', 'b'], // 100ms > 16ms threshold
    ];
    const result = mergeAdjacentEvents(events);
    expect(result).toHaveLength(2);
  });

  it('does NOT merge different event types', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'output'],
      [0.001, 'i', 'input'],
    ];
    const result = mergeAdjacentEvents(events);
    expect(result).toHaveLength(2);
  });

  it('respects custom threshold', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'a'],
      [0.05, 'o', 'b'], // 50ms
    ];
    // 50ms threshold → should merge
    expect(mergeAdjacentEvents(events, 100)).toHaveLength(1);
    // 10ms threshold → should NOT merge
    expect(mergeAdjacentEvents(events, 10)).toHaveLength(2);
  });

  it('merges chain of rapid events', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'a'],
      [0.005, 'o', 'b'],
      [0.010, 'o', 'c'],
      [0.015, 'o', 'd'],
    ];
    const result = mergeAdjacentEvents(events);
    expect(result).toHaveLength(1);
    expect(result[0][2]).toBe('abcd');
  });

  it('handles mixed merge/no-merge', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'a'],
      [0.005, 'o', 'b'],   // merge with a
      [1.0, 'o', 'c'],     // too far, new group
      [1.005, 'o', 'd'],   // merge with c
    ];
    const result = mergeAdjacentEvents(events);
    expect(result).toHaveLength(2);
    expect(result[0][2]).toBe('ab');
    expect(result[1][2]).toBe('cd');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// applyIdleTimeLimit
// ═══════════════════════════════════════════════════════════════════════════

describe('applyIdleTimeLimit', () => {
  it('returns empty for empty input', () => {
    expect(applyIdleTimeLimit([], 5)).toEqual([]);
  });

  it('returns single event unchanged', () => {
    const events: AsciicastEvent[] = [[0.0, 'o', 'hello']];
    expect(applyIdleTimeLimit(events, 5)).toEqual(events);
  });

  it('compresses long pauses', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'start'],
      [30.0, 'o', 'end'], // 30s gap → compressed to maxIdle
    ];
    const result = applyIdleTimeLimit(events, 2);
    expect(result[0][0]).toBe(0.0);
    expect(result[1][0]).toBe(2.0); // gap compressed from 30s to 2s
  });

  it('preserves gaps shorter than maxIdle', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'a'],
      [1.0, 'o', 'b'],
    ];
    const result = applyIdleTimeLimit(events, 5);
    expect(result[0][0]).toBe(0.0);
    expect(result[1][0]).toBe(1.0);
  });

  it('accumulates compression across multiple pauses', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'a'],
      [20.0, 'o', 'b'],  // 20s gap → compressed by 18s
      [40.0, 'o', 'c'],  // another 20s gap → compressed by 18s more
    ];
    const result = applyIdleTimeLimit(events, 2);
    expect(result[0][0]).toBe(0.0);
    expect(result[1][0]).toBe(2.0);   // original 20 - 18 shift
    expect(result[2][0]).toBe(4.0);   // original 40 - 36 total shift
  });

  it('handles maxIdle = 0 (returns original)', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'a'],
      [10.0, 'o', 'b'],
    ];
    const result = applyIdleTimeLimit(events, 0);
    expect(result).toEqual(events);
  });

  it('handles negative maxIdle (returns original)', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'a'],
      [10.0, 'o', 'b'],
    ];
    const result = applyIdleTimeLimit(events, -1);
    expect(result).toEqual(events);
  });

  it('does not modify original array', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'a'],
      [30.0, 'o', 'b'],
    ];
    const original = JSON.parse(JSON.stringify(events));
    applyIdleTimeLimit(events, 2);
    expect(events).toEqual(original);
  });

  it('preserves event content and type', () => {
    const events: AsciicastEvent[] = [
      [0.0, 'o', 'hello'],
      [100.0, 'i', 'input'],
    ];
    const result = applyIdleTimeLimit(events, 1);
    expect(result[0][1]).toBe('o');
    expect(result[0][2]).toBe('hello');
    expect(result[1][1]).toBe('i');
    expect(result[1][2]).toBe('input');
  });
});
