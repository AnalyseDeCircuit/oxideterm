import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import {
  buildWriteBatches,
  findCursorControlBoundary,
  getAdaptiveFlushPlan,
  stripZshPromptEolMarks,
  useAdaptiveRenderer,
} from '@/hooks/useAdaptiveRenderer';
import { adaptiveRendererIssue26Fixtures } from '@/test/fixtures/adaptiveRendererIssue26Fixtures';

function textEncoder(input: string): Uint8Array {
  return new TextEncoder().encode(input);
}

function createRendererHarness() {
  const writes: string[] = [];
  const terminal = {
    write: vi.fn((data: Uint8Array, callback?: () => void) => {
      writes.push(new TextDecoder().decode(data));
      callback?.();
    }),
  };

  let rafCallback: FrameRequestCallback | null = null;
  vi.stubGlobal('requestAnimationFrame', vi.fn((cb: FrameRequestCallback) => {
    rafCallback = cb;
    return 1;
  }));
  vi.stubGlobal('cancelAnimationFrame', vi.fn());

  const terminalRef = { current: terminal as never };
  const hook = renderHook(() => useAdaptiveRenderer({ terminalRef, mode: 'auto' }));

  return {
    writes,
    scheduleWrite: hook.result.current.scheduleWrite,
    flushRaf: () => rafCallback?.(16.7),
    hasPendingRaf: () => rafCallback !== null,
  };
}

describe('findCursorControlBoundary', () => {
  it('detects destructive CSI sequences at the start of a chunk', () => {
    expect(findCursorControlBoundary(textEncoder('\x1b[2Kprompt'))).toBe(0);
  });

  it('detects destructive CSI sequences after printable output', () => {
    expect(findCursorControlBoundary(textEncoder('file1\r\nfile2\r\n\x1b[2A\x1b[2K'))).toBe(14);
  });

  it('skips non-destructive CSI sequences and finds a later destructive one', () => {
    expect(findCursorControlBoundary(textEncoder('\x1b[31mred\x1b[0mfile\r\n\x1b[2Kprompt'))).toBe(18);
  });

  it('ignores non-destructive CSI sequences such as SGR color changes', () => {
    expect(findCursorControlBoundary(textEncoder('\x1b[31mred\x1b[0m'))).toBe(-1);
  });
});

describe('useAdaptiveRenderer', () => {
  it('strips zsh reverse-video prompt eol markers before writing to xterm', () => {
    const { writes, scheduleWrite, flushRaf } = createRendererHarness();

    scheduleWrite(textEncoder('out\x1b[7m%\x1b[27m\r\nprompt'));
    flushRaf();

    expect(writes).toEqual(['out\x1b[27m\r\nprompt']);
  });

  it('keeps ordinary percent output intact', () => {
    const input = textEncoder('42%\r\n\x1b[31m100%\x1b[0m');

    expect(new TextDecoder().decode(stripZshPromptEolMarks(input))).toBe('42%\r\n\x1b[31m100%\x1b[0m');
  });

  it('flushes printable output before a later destructive cursor-control tail', () => {
    const { writes, scheduleWrite, flushRaf, hasPendingRaf } = createRendererHarness();

    scheduleWrite(textEncoder('file1\r\nfile2\r\n\x1b[2A\x1b[2Kprompt$ '));

    expect(writes).toEqual(['file1\r\nfile2\r\n']);
    expect(hasPendingRaf()).toBe(true);

    flushRaf();

    expect(writes).toEqual([
      'file1\r\nfile2\r\n',
      '\x1b[2A\x1b[2Kprompt$ ',
    ]);
  });

  it('keeps inline redraw sequences in a single write when there is no prior line output', () => {
    const { writes, scheduleWrite, flushRaf, hasPendingRaf } = createRendererHarness();

    scheduleWrite(textEncoder('hello\x1b[1Gworld'));

    expect(writes).toEqual([]);
    expect(hasPendingRaf()).toBe(true);

    flushRaf();

    expect(writes).toEqual(['hello\x1b[1Gworld']);
  });

  it('keeps carriage-return-based single-line redraw in a single write', () => {
    const { writes, scheduleWrite, flushRaf, hasPendingRaf } = createRendererHarness();

    scheduleWrite(textEncoder('42%\r\x1b[2K43%'));

    expect(writes).toEqual([]);
    expect(hasPendingRaf()).toBe(true);

    flushRaf();

    expect(writes).toEqual(['42%\r\x1b[2K43%']);
  });

  it('spreads very large output bursts across multiple animation frames', () => {
    const { writes, scheduleWrite, flushRaf } = createRendererHarness();
    const largeOutput = new Uint8Array((256 * 1024 * 4) + 1);
    largeOutput.fill(97);

    scheduleWrite(largeOutput);
    flushRaf();

    expect(writes).toHaveLength(4);

    flushRaf();

    expect(writes).toHaveLength(5);
  });

  it('flushes a pending single-line chunk before a later redraw chunk arrives', () => {
    const { writes, scheduleWrite, flushRaf, hasPendingRaf } = createRendererHarness();

    scheduleWrite(textEncoder('hello'));
    scheduleWrite(textEncoder('\x1b[1Gworld'));

    expect(writes).toEqual(['hello']);
    expect(hasPendingRaf()).toBe(true);

    flushRaf();

    expect(writes).toEqual(['hello', '\x1b[1Gworld']);
  });

  describe('Issue #26 async prompt redraw regression', () => {
    it.each(adaptiveRendererIssue26Fixtures)('$name', (fixture) => {
      const { writes, scheduleWrite, flushRaf, hasPendingRaf } = createRendererHarness();

      fixture.chunks.forEach((chunk, index) => {
        scheduleWrite(textEncoder(chunk));
        expect(writes).toEqual(fixture.writesAfterChunk[index]);
        expect(hasPendingRaf()).toBe(true);
      });

      flushRaf();

      expect(writes).toEqual(fixture.finalWrites);
    });

  });
}); 

describe('buildWriteBatches', () => {
  it('merges small chunks into a single batch below the limit', () => {
    const batches = buildWriteBatches([
      textEncoder('hello '),
      textEncoder('world'),
    ], 64);

    expect(batches).toHaveLength(1);
    expect(new TextDecoder().decode(batches[0])).toBe('hello world');
  });

  it('splits oversized output into bounded batches', () => {
    const large = textEncoder('a'.repeat(10));
    const batches = buildWriteBatches([large], 4);

    expect(batches.map(batch => batch.length)).toEqual([4, 4, 2]);
    expect(batches.map(batch => new TextDecoder().decode(batch))).toEqual([
      'aaaa',
      'aaaa',
      'aa',
    ]);
  });

  it('preserves chunk order across batch boundaries', () => {
    const batches = buildWriteBatches([
      textEncoder('abc'),
      textEncoder('defg'),
      textEncoder('hij'),
    ], 5);

    expect(batches.map(batch => new TextDecoder().decode(batch))).toEqual([
      'abcde',
      'fghij',
    ]);
  });
});

describe('getAdaptiveFlushPlan', () => {
  it('uses small low-latency batches shortly after user input', () => {
    const plan = getAdaptiveFlushPlan({
      now: 1_100,
      lastUserInputAt: 1_000,
      pendingBytes: 64 * 1024,
      tier: 'boost',
    });

    expect(plan.priority).toBe('interactive');
    expect(plan.maxBatchesPerFrame).toBe(1);
    expect(plan.maxBatchBytes).toBeLessThan(64 * 1024);
  });

  it('switches to throughput batches for sustained output', () => {
    const plan = getAdaptiveFlushPlan({
      now: 1_500,
      lastUserInputAt: 1_000,
      pendingBytes: 768 * 1024,
      tier: 'normal',
    });

    expect(plan.priority).toBe('throughput');
    expect(plan.maxBatchesPerFrame).toBeGreaterThan(1);
  });
});
