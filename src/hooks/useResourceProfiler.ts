/**
 * useResourceProfiler Hook
 *
 * Thin wrapper around profilerStore for component-level usage.
 * Auto-starts profiler when connectionId is provided, reads from global store.
 *
 * Key behaviors:
 * - Auto-starts profiler when connectionId is provided and enabled
 * - Reads metrics/history from global profilerStore (single source of truth)
 * - Multiple components can read the same connection's data without duplication
 */

import { useEffect, useCallback } from 'react';
import { useProfilerStore } from '../store/profilerStore';
import type { ResourceMetrics } from '../types';

export type UseResourceProfilerResult = {
  /** Latest resource metrics snapshot */
  metrics: ResourceMetrics | null;
  /** Historical metrics for sparkline (last 12 points) */
  history: ResourceMetrics[];
  /** Whether the profiler is actively running */
  isRunning: boolean;
  /** Any error message */
  error: string | null;
  /** Manually start profiling */
  start: () => Promise<void>;
  /** Manually stop profiling */
  stop: () => Promise<void>;
};

const SPARKLINE_POINTS = 12;

export function useResourceProfiler(
  connectionId: string | null,
  enabled = true
): UseResourceProfilerResult {
  const connState = useProfilerStore((s) =>
    connectionId ? s.connections.get(connectionId) : undefined
  );

  const startProfiler = useProfilerStore((s) => s.startProfiler);
  const stopProfiler = useProfilerStore((s) => s.stopProfiler);

  const start = useCallback(async () => {
    if (connectionId) await startProfiler(connectionId);
  }, [connectionId, startProfiler]);

  const stop = useCallback(async () => {
    if (connectionId) await stopProfiler(connectionId);
  }, [connectionId, stopProfiler]);

  // Auto-start when connectionId is provided
  useEffect(() => {
    if (!connectionId || !enabled) return;
    startProfiler(connectionId);
  }, [connectionId, enabled, startProfiler]);

  const history = connState?.history?.slice(-SPARKLINE_POINTS) ?? [];

  return {
    metrics: connState?.metrics ?? null,
    history,
    isRunning: connState?.isRunning ?? false,
    error: connState?.error ?? null,
    start,
    stop,
  };
}
