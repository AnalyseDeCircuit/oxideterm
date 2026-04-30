// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * useAdaptiveRenderer — Dynamic Refresh Rate scheduler for terminal rendering.
 *
 * Replaces the fixed-RAF batching pattern (`pendingDataRef` + `rafIdRef`) with
 * a three-tier adaptive pipeline:
 *
 * | Tier      | Trigger                              | Effective FPS | Benefit                        |
 * |-----------|--------------------------------------|---------------|--------------------------------|
 * | **boost** | High throughput / rapid scroll        | 120Hz+ (RAF)  | Eliminates motion blur          |
 * | **normal**| Standard typing / light output        | 60Hz (RAF)    | Baseline interactive smoothness |
 * | **idle**  | No I/O for 3s, or window hidden/blur  | 1–15Hz (timer)| GPU idle, battery savings       |
 *
 * ## How it works
 *
 * - Callers push data via `scheduleWrite(chunk)`.
 * - In **boost / normal** mode, a single `requestAnimationFrame` collects
 *   pending chunks and writes bounded batches within a frame budget.
 *   On ≥120Hz displays the browser naturally fires RAF at the panel refresh
 *   rate — no extra timers needed.
 * - In **idle** mode, RAF is cancelled and a `setTimeout` loop fires at a
 *   progressively slower interval (66ms → 1000ms) to keep cursors blinking
 *   and late output visible without GPU load.
 * - Transitions are driven by data volume, user input, and Page Visibility /
 *   window focus events.
 *
 * The hook is **framework-agnostic** in its core — it returns a plain object
 * and manages its own listeners.  React is only used for lifecycle cleanup.
 */

import { useRef, useEffect, useCallback, useMemo } from 'react';
import type { Terminal } from '@xterm/xterm';

// ─── Types ────────────────────────────────────────────────────────────

export type RenderTier = 'boost' | 'normal' | 'idle';

export type AdaptiveRendererMode = 'auto' | 'always-60' | 'off';

export type AdaptiveRendererStats = {
  tier: RenderTier;
  /** Measured writes-per-second (rolling window) */
  actualWps: number;
  /** Bytes pending in the current batch */
  pendingBytes: number;
};

// ─── Constants ────────────────────────────────────────────────────────

/** Bytes per frame that triggers boost mode (4 KB) */
const BOOST_BYTES_THRESHOLD = 4096;

/** Consecutive low-volume frames before dropping from boost → normal */
const BOOST_COOLDOWN_FRAMES = 30; // ~500ms at 60fps

/** Milliseconds of silence before transitioning normal → idle */
const IDLE_TIMEOUT_MS = 3_000;

/** Idle tier minimum interval (≈15 fps) */
const IDLE_INTERVAL_MIN_MS = 66;

/** Idle tier maximum interval (1 fps) */
const IDLE_INTERVAL_MAX_MS = 1_000;

/** Idle interval growth factor per tick */
const IDLE_INTERVAL_GROWTH = 1.5;
/** Maximum bytes to concatenate into a single xterm.write batch. */
const MAX_WRITE_BATCH_BYTES = 256 * 1024;

/** After user input, prefer smaller chunks for lower terminal echo latency. */
const INTERACTIVE_PRIORITY_WINDOW_MS = 220;

/** Sustained output threshold where throughput batching beats latency tuning. */
const THROUGHPUT_PENDING_BYTES_THRESHOLD = 512 * 1024;

const INTERACTIVE_WRITE_BATCH_BYTES = 32 * 1024;
const NORMAL_WRITE_BATCH_BYTES = 128 * 1024;
const THROUGHPUT_WRITE_BATCH_BYTES = MAX_WRITE_BATCH_BYTES;

const INTERACTIVE_MAX_BATCHES_PER_FRAME = 1;
const NORMAL_MAX_BATCHES_PER_FRAME = 2;
const THROUGHPUT_MAX_BATCHES_PER_FRAME = 4;

const INTERACTIVE_FRAME_BUDGET_MS = 3;
const NORMAL_FRAME_BUDGET_MS = 5;
const THROUGHPUT_FRAME_BUDGET_MS = 8;

// ─── FlowControl Constants ────────────────────────────────────────────

/** When pending xterm callbacks exceed this, pause accepting new data */
const FLOW_HIGH_WATERMARK = 10;

/** When pending callbacks drop below this, resume accepting data */
const FLOW_LOW_WATERMARK = 5;

/** Maximum bytes to buffer while flow-controlled before dropping */
const FLOW_MAX_BACKPRESSURE_BYTES = 8 * 1024 * 1024; // 8 MB

// ─── Cursor Control Detection ─────────────────────────────────────────

/**
 * Find the first destructive CSI cursor-control / erase sequence in a chunk.
 *
 * Async prompt themes (spaceship-zsh, starship) often emit printable output
 * and then immediately redraw the prompt area with cursor-up + erase-line.
 * If both land in one network chunk, batching them into a single `term.write()`
 * lets the destructive sequence wipe the fresh command output before the browser
 * paints it. Returning the boundary lets callers flush the printable prefix
 * first, then apply the prompt redraw in a later write.
 *
 * Detected final bytes (after CSI params):
 *   A = Cursor Up,  B = Cursor Down,  H/f = Cursor Position,
 *   G = Cursor Horizontal Absolute,  J = Erase in Display,  K = Erase in Line
 *
 * Returns the byte offset of the first matching ESC[ sequence, or -1 if none.
 */
export function findCursorControlBoundary(data: Uint8Array): number {
  if (data.length < 3) return -1;

  for (let start = 0; start <= data.length - 3; start++) {
    // CSI = ESC [ (0x1b 0x5b)
    if (data[start] !== 0x1b || data[start + 1] !== 0x5b) continue;

    // Scan for the final byte (first byte in 0x40-0x7E after optional params)
    for (let i = start + 2; i < Math.min(data.length, start + 12); i++) {
      const b = data[i];
      // Parameter bytes: 0-9 ; < = > ? (0x30–0x3F)
      if (b >= 0x30 && b <= 0x3f) continue;
      // Intermediate bytes: SP ! " # … / (0x20–0x2F)
      if (b >= 0x20 && b <= 0x2f) continue;
      // Final byte — check if it's cursor movement or erase
      //   A(0x41) B(0x42) G(0x47) H(0x48) J(0x4A) K(0x4B) f(0x66)
      if (
        b === 0x41 || b === 0x42 || b === 0x47 || b === 0x48 ||
        b === 0x4a || b === 0x4b || b === 0x66
      )
        return start;

      break;
    }
  }

  return -1;
}

function findTrailingPartialCursorControlStart(data: Uint8Array): number {
  if (data.length === 0) return -1;

  const searchStart = Math.max(0, data.length - 12);
  for (let start = data.length - 1; start >= searchStart; start--) {
    if (data[start] !== 0x1b) continue;

    if (start === data.length - 1) {
      return start;
    }

    if (data[start + 1] !== 0x5b) {
      continue;
    }

    let validPartial = true;
    for (let i = start + 2; i < data.length; i++) {
      const b = data[i];
      if (b >= 0x30 && b <= 0x3f) continue;
      if (b >= 0x20 && b <= 0x2f) continue;
      validPartial = false;
      break;
    }

    if (validPartial) {
      return start;
    }
  }

  return -1;
}

function concatUint8Arrays(left: Uint8Array, right: Uint8Array): Uint8Array {
  const combined = new Uint8Array(left.length + right.length);
  combined.set(left, 0);
  combined.set(right, left.length);
  return combined;
}

function isInverseSgr(params: string): boolean {
  const parts = params.length === 0 ? ['0'] : params.split(';');
  return parts.includes('7');
}

function sgrResetEnd(data: Uint8Array, offset: number): number {
  if (data[offset] !== 0x1b || data[offset + 1] !== 0x5b) return -1;
  let i = offset + 2;
  while (i < data.length && data[i] !== 0x6d) {
    const byte = data[i];
    if (!((byte >= 0x30 && byte <= 0x3f) || byte === 0x20)) return -1;
    i += 1;
  }
  if (i >= data.length) return -1;
  const params = new TextDecoder().decode(data.subarray(offset + 2, i)).replace(/\s/g, '');
  const parts = params.length === 0 ? ['0'] : params.split(';');
  return parts.includes('0') || parts.includes('27') ? i + 1 : -1;
}

export function stripZshPromptEolMarks(data: Uint8Array): Uint8Array {
  if (data.length < 8) return data;

  let changed = false;
  const chunks: Uint8Array[] = [];
  let last = 0;

  for (let start = 0; start < data.length - 7; start += 1) {
    if (data[start] !== 0x1b || data[start + 1] !== 0x5b) continue;

    let sgrEnd = start + 2;
    while (sgrEnd < data.length && data[sgrEnd] !== 0x6d) {
      const byte = data[sgrEnd];
      if (!((byte >= 0x30 && byte <= 0x3f) || byte === 0x20)) break;
      sgrEnd += 1;
    }
    if (sgrEnd >= data.length || data[sgrEnd] !== 0x6d) continue;

    const markerIndex = sgrEnd + 1;
    const marker = data[markerIndex];
    if (marker !== 0x25 && marker !== 0x23) continue; // zsh uses % for users, # for root.

    const params = new TextDecoder().decode(data.subarray(start + 2, sgrEnd)).replace(/\s/g, '');
    if (!isInverseSgr(params)) continue;

    const resetEnd = sgrResetEnd(data, markerIndex + 1);
    if (resetEnd < 0) continue;

    // zsh's default PROMPT_EOL_MARK is a reverse-video "%/#" inserted when
    // the previous output did not end with a newline. It is a shell warning,
    // not command output. Strip only this exact SGR marker shape so ordinary
    // percent signs and progress output remain untouched.
    if (start > last) chunks.push(data.subarray(last, start));
    chunks.push(data.subarray(markerIndex + 1, resetEnd));
    last = resetEnd;
    start = resetEnd - 1;
    changed = true;
  }

  if (!changed) return data;
  if (last < data.length) chunks.push(data.subarray(last));
  return concatManyUint8Arrays(chunks);
}

function concatManyUint8Arrays(chunks: Uint8Array[]): Uint8Array {
  if (chunks.length === 0) return new Uint8Array();
  if (chunks.length === 1) return chunks[0];
  const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const combined = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    combined.set(chunk, offset);
    offset += chunk.length;
  }
  return combined;
}

export function buildWriteBatches(
  chunks: Uint8Array[],
  maxBatchBytes: number = MAX_WRITE_BATCH_BYTES,
): Uint8Array[] {
  if (chunks.length === 0) return [];

  const batches: Uint8Array[] = [];
  let pendingChunks: Uint8Array[] = [];
  let pendingBytes = 0;

  const flushPending = () => {
    if (pendingChunks.length === 0) return;
    if (pendingChunks.length === 1) {
      batches.push(pendingChunks[0]);
    } else {
      const combined = new Uint8Array(pendingBytes);
      let offset = 0;
      for (const chunk of pendingChunks) {
        combined.set(chunk, offset);
        offset += chunk.length;
      }
      batches.push(combined);
    }
    pendingChunks = [];
    pendingBytes = 0;
  };

  for (const chunk of chunks) {
    let offset = 0;

    while (offset < chunk.length) {
      const remainingChunkBytes = chunk.length - offset;

      if (pendingBytes === 0 && remainingChunkBytes >= maxBatchBytes) {
        batches.push(chunk.subarray(offset, offset + maxBatchBytes));
        offset += maxBatchBytes;
        continue;
      }

      const remainingBatchBytes = maxBatchBytes - pendingBytes;
      if (remainingBatchBytes === 0) {
        flushPending();
        continue;
      }

      const sliceLength = Math.min(remainingChunkBytes, remainingBatchBytes);
      pendingChunks.push(chunk.subarray(offset, offset + sliceLength));
      pendingBytes += sliceLength;
      offset += sliceLength;

      if (pendingBytes >= maxBatchBytes) {
        flushPending();
      }
    }
  }

  flushPending();
  return batches;
}

export type AdaptiveFlushPlan = {
  priority: 'interactive' | 'normal' | 'throughput';
  maxBatchBytes: number;
  maxBatchesPerFrame: number;
  frameBudgetMs: number;
};

export function getAdaptiveFlushPlan(params: {
  now: number;
  lastUserInputAt: number;
  pendingBytes: number;
  tier: RenderTier;
}): AdaptiveFlushPlan {
  const recentlyInteractive =
    params.lastUserInputAt > 0 && params.now - params.lastUserInputAt <= INTERACTIVE_PRIORITY_WINDOW_MS;

  if (recentlyInteractive && params.pendingBytes < THROUGHPUT_PENDING_BYTES_THRESHOLD) {
    return {
      priority: 'interactive',
      maxBatchBytes: INTERACTIVE_WRITE_BATCH_BYTES,
      maxBatchesPerFrame: INTERACTIVE_MAX_BATCHES_PER_FRAME,
      frameBudgetMs: INTERACTIVE_FRAME_BUDGET_MS,
    };
  }

  if (params.tier === 'boost' || params.pendingBytes >= THROUGHPUT_PENDING_BYTES_THRESHOLD) {
    return {
      priority: 'throughput',
      maxBatchBytes: THROUGHPUT_WRITE_BATCH_BYTES,
      maxBatchesPerFrame: THROUGHPUT_MAX_BATCHES_PER_FRAME,
      frameBudgetMs: THROUGHPUT_FRAME_BUDGET_MS,
    };
  }

  return {
    priority: 'normal',
    maxBatchBytes: NORMAL_WRITE_BATCH_BYTES,
    maxBatchesPerFrame: NORMAL_MAX_BATCHES_PER_FRAME,
    frameBudgetMs: NORMAL_FRAME_BUDGET_MS,
  };
}

function hasLineBreak(data: Uint8Array): boolean {
  for (const byte of data) {
    if (byte === 0x0a) return true;
  }
  return false;
}

// ─── Hook ─────────────────────────────────────────────────────────────

type UseAdaptiveRendererOptions = {
  /** Ref to the xterm Terminal instance (null until mounted) */
  terminalRef: React.RefObject<Terminal | null>;
  /** Feature mode from settings */
  mode: AdaptiveRendererMode;
};

export type AdaptiveRendererHandle = {
  /**
   * Enqueue a chunk for the next batched `terminal.write()`.
   * Call this instead of `terminal.write()` directly.
   */
  scheduleWrite: (data: Uint8Array) => void;
  /** Signal that the user pressed a key / interacted */
  notifyUserInput: () => void;
  /** Current tier (for UI / profiler) */
  getTier: () => RenderTier;
  /** Get stats snapshot */
  getStats: () => AdaptiveRendererStats;
};

export function useAdaptiveRenderer(opts: UseAdaptiveRendererOptions): AdaptiveRendererHandle {
  const { terminalRef, mode } = opts;

  // Track mode in a ref so callbacks always see the latest value
  const modeRef = useRef(mode);
  modeRef.current = mode;

  // ── Internal state (refs for zero-render-cost) ────────────────────

  const pendingRef = useRef<Uint8Array[]>([]);
  const rafIdRef = useRef<number | null>(null);
  const timerIdRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const tierRef = useRef<RenderTier>('normal');

  // Boost cooldown counter: counts consecutive low-volume frames
  const boostCooldownRef = useRef(0);

  // Idle transition timer
  const idleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Current idle interval (grows over time)
  const idleIntervalRef = useRef(IDLE_INTERVAL_MIN_MS);
  const lastUserInputAtRef = useRef(0);

  // WPS tracking (rolling window)
  const writeTimestampsRef = useRef<number[]>([]);

  // Track whether we're "alive" (component mounted)
  const mountedRef = useRef(true);
  const flushRef = useRef<() => void>(() => {});

  // ── FlowControl state ─────────────────────────────────────────────

  const pendingCallbacksRef = useRef(0);
  const blockedRef = useRef(false);
  const backpressureBytesRef = useRef(0);
  const backpressureQueueRef = useRef<Uint8Array[]>([]);

  // ── Helpers ───────────────────────────────────────────────────────

  const scheduleDeferredFlush = useCallback(() => {
    if (!mountedRef.current || pendingRef.current.length === 0) return;

    if (tierRef.current === 'idle' || document.hidden) {
      if (timerIdRef.current === null) {
        timerIdRef.current = setTimeout(() => {
          timerIdRef.current = null;
          if (!mountedRef.current) return;
          flushRef.current();
        }, IDLE_INTERVAL_MIN_MS);
      }
      return;
    }

    if (rafIdRef.current === null) {
      rafIdRef.current = requestAnimationFrame(() => {
        rafIdRef.current = null;
        if (!mountedRef.current) return;
        flushRef.current();
      });
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  /** Write pending chunks using frame-budgeted batches. */
  const flush = useCallback(() => {
    const term = terminalRef.current;
    const pending = pendingRef.current;
    if (!term || pending.length === 0) return;
    pendingRef.current = [];
    const now = performance.now();
    const pendingBytes = pending.reduce((sum, chunk) => sum + chunk.length, 0);
    const plan = getAdaptiveFlushPlan({
      now,
      lastUserInputAt: lastUserInputAtRef.current,
      pendingBytes,
      tier: tierRef.current,
    });
    const batches = buildWriteBatches(pending, plan.maxBatchBytes);
    const deferredBatches: Uint8Array[] = [];
    const frameStart = performance.now();
    let writtenThisFrame = 0;

    for (const batch of batches) {
      const overBatchLimit = writtenThisFrame >= plan.maxBatchesPerFrame;
      const overFrameBudget =
        writtenThisFrame > 0 && performance.now() - frameStart >= plan.frameBudgetMs;

      if (overBatchLimit || overFrameBudget) {
        deferredBatches.push(batch);
        continue;
      }

      // FlowControl: track pending xterm callbacks
      pendingCallbacksRef.current++;
      writtenThisFrame++;
      term.write(batch, () => {
        pendingCallbacksRef.current--;
        // If below low watermark and was blocked, drain backpressure queue
        if (blockedRef.current && pendingCallbacksRef.current <= FLOW_LOW_WATERMARK) {
          blockedRef.current = false;
          // Move backpressure queue into pending and schedule a flush
          if (backpressureQueueRef.current.length > 0) {
            pendingRef.current.push(...backpressureQueueRef.current);
            backpressureQueueRef.current = [];
            backpressureBytesRef.current = 0;
            if (rafIdRef.current === null && mountedRef.current) {
              rafIdRef.current = requestAnimationFrame(() => {
                rafIdRef.current = null;
                if (!mountedRef.current) return;
                flush();
              });
            }
          }
        }
      });
    }

    if (deferredBatches.length > 0) {
      pendingRef.current = deferredBatches.concat(pendingRef.current);
      scheduleDeferredFlush();
    }

    // Enter blocked state if above high watermark
    if (pendingCallbacksRef.current >= FLOW_HIGH_WATERMARK) {
      blockedRef.current = true;
    }

    // Track WPS
    const writeNow = performance.now();
    writeTimestampsRef.current.push(writeNow);
    // Keep only last 2 seconds of timestamps
    const cutoff = writeNow - 2_000;
    while (writeTimestampsRef.current.length > 0 && writeTimestampsRef.current[0] < cutoff) {
      writeTimestampsRef.current.shift();
    }
  }, [terminalRef, scheduleDeferredFlush]);
  flushRef.current = flush;

  // ── Tier transition logic ─────────────────────────────────────────

  const cancelIdle = useCallback(() => {
    if (timerIdRef.current !== null) {
      clearTimeout(timerIdRef.current);
      timerIdRef.current = null;
    }
    idleIntervalRef.current = IDLE_INTERVAL_MIN_MS;
  }, []);

  const cancelRaf = useCallback(() => {
    if (rafIdRef.current !== null) {
      cancelAnimationFrame(rafIdRef.current);
      rafIdRef.current = null;
    }
  }, []);

  const resetIdleTimer = useCallback(() => {
    if (idleTimerRef.current !== null) {
      clearTimeout(idleTimerRef.current);
    }
    idleTimerRef.current = setTimeout(() => {
      if (!mountedRef.current) return;
      enterIdle();
    }, IDLE_TIMEOUT_MS);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const enterIdle = useCallback(() => {
    if (tierRef.current === 'idle') return;
    tierRef.current = 'idle';
    cancelRaf();
    idleIntervalRef.current = IDLE_INTERVAL_MIN_MS;
    scheduleIdleTick();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const scheduleIdleTick = useCallback(() => {
    if (timerIdRef.current !== null) return;
    timerIdRef.current = setTimeout(() => {
      timerIdRef.current = null;
      if (!mountedRef.current) return;

      flush();

      if (tierRef.current === 'idle') {
        // Grow interval towards max
        idleIntervalRef.current = Math.min(
          idleIntervalRef.current * IDLE_INTERVAL_GROWTH,
          IDLE_INTERVAL_MAX_MS,
        );
        scheduleIdleTick();
      }
    }, idleIntervalRef.current);
  }, [flush]); // eslint-disable-line react-hooks/exhaustive-deps

  const exitIdle = useCallback(() => {
    if (tierRef.current !== 'idle') return;
    cancelIdle();
    tierRef.current = 'normal';
    boostCooldownRef.current = 0;
  }, [cancelIdle]);

  /** Evaluate whether the current frame should be boost or stay normal */
  const evaluateBoost = useCallback((bytesThisFrame: number) => {
    if (bytesThisFrame >= BOOST_BYTES_THRESHOLD) {
      tierRef.current = 'boost';
      boostCooldownRef.current = 0;
    } else if (tierRef.current === 'boost') {
      boostCooldownRef.current++;
      if (boostCooldownRef.current >= BOOST_COOLDOWN_FRAMES) {
        tierRef.current = 'normal';
        boostCooldownRef.current = 0;
      }
    }
  }, []);

  // ── scheduleWrite ─────────────────────────────────────────────────

  const scheduleWrite = useCallback(
    (data: Uint8Array) => {
      if (!terminalRef.current || !mountedRef.current) return;

      let nextData = stripZshPromptEolMarks(data);

      const currentMode = modeRef.current;

      // Mode: 'off' — direct write, no batching
      if (currentMode === 'off') {
        terminalRef.current.write(nextData);
        return;
      }

      // FlowControl: if blocked, divert to backpressure queue
      if (blockedRef.current) {
        if (backpressureBytesRef.current < FLOW_MAX_BACKPRESSURE_BYTES) {
          backpressureQueueRef.current.push(nextData);
          backpressureBytesRef.current += nextData.length;
        } else {
          console.warn(
            `[AdaptiveRenderer] Backpressure queue full (${(backpressureBytesRef.current / 1024 / 1024).toFixed(1)}MB), dropping ${nextData.length} bytes`,
          );
        }
        return;
      }

      const lastPending = pendingRef.current[pendingRef.current.length - 1];
      if (lastPending) {
        const partialStart = findTrailingPartialCursorControlStart(lastPending);
        if (partialStart >= 0) {
          const mergeStart = findCursorControlBoundary(lastPending) === 0 ? 0 : partialStart;
          const pendingPrefix = lastPending.subarray(0, mergeStart);
          const pendingSuffix = lastPending.subarray(mergeStart);
          const merged = concatUint8Arrays(pendingSuffix, nextData);

          if (findCursorControlBoundary(merged) === 0) {
            pendingRef.current.pop();
            if (pendingPrefix.length > 0) {
              pendingRef.current.push(pendingPrefix);
            }
            nextData = merged;
          }
        }
      }

      // If currently idle AND the page is hidden (or window has no focus),
      // stay in idle — just push data and let the idle timer flush it.
      // This prevents waking to RAF mode under continuous remote output
      // while the tab is backgrounded, which would accumulate large pending
      // buffers since RAF is throttled/suspended in background tabs.
      if (tierRef.current === 'idle' && document.hidden) {
        pendingRef.current.push(nextData);
        // Reset idle interval to min so data gets flushed soon
        idleIntervalRef.current = IDLE_INTERVAL_MIN_MS;
        // Ensure an idle tick is scheduled
        scheduleIdleTick();
        return;
      }

      // Wake from idle if needed (only when page is visible)
      if (tierRef.current === 'idle') {
        exitIdle();
      }

      // ── Cursor-control split (Phase 3 — async-prompt defence) ────
      // Split chunks that contain a destructive cursor-control sequence
      // anywhere inside them, not just at byte 0. This covers the common
      // spaceship-zsh case where command output and prompt redraw are emitted
      // in the same SSH/WebSocket payload.
      const controlBoundary = findCursorControlBoundary(nextData);
      if (controlBoundary >= 0) {
        const prefix = controlBoundary > 0 ? nextData.subarray(0, controlBoundary) : null;
        const shouldSplit = pendingRef.current.length > 0 || (prefix !== null && hasLineBreak(prefix));

        if (shouldSplit && prefix !== null && prefix.length > 0) {
          pendingRef.current.push(prefix);
        }

        if (shouldSplit && pendingRef.current.length > 0) {
          flush();
          // Cancel the current RAF — we just flushed everything, and the new
          // cursor-control tail should be written in a fresh RAF frame.
          cancelRaf();
          pendingRef.current.push(nextData.subarray(controlBoundary));
        } else {
          pendingRef.current.push(nextData);
        }
      } else {
        pendingRef.current.push(nextData);
      }

      // Reset idle countdown
      resetIdleTimer();

      // Mode: 'always-60' — simple RAF, no boost evaluation
      if (currentMode === 'always-60') {
        if (rafIdRef.current === null) {
          rafIdRef.current = requestAnimationFrame(() => {
            rafIdRef.current = null;
            if (!mountedRef.current) return;
            flush();
          });
        }
        return;
      }

      // Mode: 'auto' — adaptive RAF with boost evaluation
      if (rafIdRef.current === null) {
        rafIdRef.current = requestAnimationFrame(() => {
          rafIdRef.current = null;
          if (!mountedRef.current) return;

          // Calculate bytes in this frame for boost decision
          const totalBytes = pendingRef.current.reduce((s, c) => s + c.length, 0);
          evaluateBoost(totalBytes);

          flush();
        });
      }
    },
    [terminalRef, flush, exitIdle, resetIdleTimer, evaluateBoost, scheduleIdleTick, cancelRaf],
  );

  // ── notifyUserInput ───────────────────────────────────────────────

  const notifyUserInput = useCallback(() => {
    lastUserInputAtRef.current = performance.now();
    if (tierRef.current === 'idle') {
      exitIdle();
    }
    resetIdleTimer();
  }, [exitIdle, resetIdleTimer]);

  // ── Visibility / focus listeners ──────────────────────────────────

  useEffect(() => {
    if (modeRef.current === 'off') return;

    const handleVisibilityChange = () => {
      if (modeRef.current === 'off') return;
      if (document.hidden) {
        // Page hidden → enter idle immediately
        if (idleTimerRef.current !== null) {
          clearTimeout(idleTimerRef.current);
          idleTimerRef.current = null;
        }
        enterIdle();
      } else {
        // Page visible → exit idle, resume normal
        exitIdle();
        resetIdleTimer();
        // If there's pending data, kick a RAF
        if (pendingRef.current.length > 0 && rafIdRef.current === null) {
          rafIdRef.current = requestAnimationFrame(() => {
            rafIdRef.current = null;
            if (!mountedRef.current) return;
            flush();
          });
        }
      }
    };

    const handleWindowBlur = () => {
      // Start a fast-track to idle when the user switches apps (3s still applies)
      resetIdleTimer();
    };

    const handleWindowFocus = () => {
      if (tierRef.current === 'idle') {
        exitIdle();
        resetIdleTimer();
      }
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);
    window.addEventListener('blur', handleWindowBlur);
    window.addEventListener('focus', handleWindowFocus);

    return () => {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      window.removeEventListener('blur', handleWindowBlur);
      window.removeEventListener('focus', handleWindowFocus);
    };
  }, [enterIdle, exitIdle, resetIdleTimer, flush]);

  // ── Cleanup on unmount ────────────────────────────────────────────

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      cancelRaf();
      cancelIdle();
      if (idleTimerRef.current !== null) {
        clearTimeout(idleTimerRef.current);
        idleTimerRef.current = null;
      }
      pendingRef.current = [];
    };
  }, [cancelRaf, cancelIdle]);

  // ── Public API ────────────────────────────────────────────────────

  const getTier = useCallback(() => tierRef.current, []);

  const getStats = useCallback((): AdaptiveRendererStats => {
    const now = performance.now();
    const cutoff = now - 1_000;
    const recentWrites = writeTimestampsRef.current.filter((t) => t >= cutoff);
    return {
      tier: tierRef.current,
      actualWps: recentWrites.length,
      pendingBytes: pendingRef.current.reduce((s, c) => s + c.length, 0),
    };
  }, []);

  return useMemo(
    () => ({ scheduleWrite, notifyUserInput, getTier, getStats }),
    [scheduleWrite, notifyUserInput, getTier, getStats],
  );
}
