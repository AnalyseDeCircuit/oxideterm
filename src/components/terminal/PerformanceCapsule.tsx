/**
 * PerformanceCapsule - Glassmorphism resource metrics overlay
 *
 * Displays CPU / Memory / Network / RTT in a compact floating capsule
 * positioned at the top-right of the terminal view.
 *
 * Features:
 * - Mini sparkline (SVG polyline, last 12 samples)
 * - Color-coded thresholds: green(<70%), amber(70-90%), red(>90%)
 * - Click to expand detailed view
 * - Graceful degradation for RttOnly/Failed sources
 */

import React, { useState, useMemo } from 'react';
import { cn } from '../../lib/utils';
import { useTranslation } from 'react-i18next';
import type { ResourceMetrics } from '../../types';
import {
  Activity,
  Cpu,
  MemoryStick,
  ArrowDown,
  ArrowUp,
  ChevronDown,
  ChevronUp,
} from 'lucide-react';

type PerformanceCapsuleProps = {
  metrics: ResourceMetrics | null;
  history: ResourceMetrics[];
  isRunning: boolean;
  sshRttMs?: number | null;
};

/** Format bytes to human-readable string */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)}GB`;
}

/** Format bytes/sec to rate string */
function formatRate(bytesPerSec: number): string {
  if (bytesPerSec < 1024) return `${bytesPerSec}B/s`;
  if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)}KB/s`;
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)}MB/s`;
}

/** Get color class based on percentage threshold */
function getThresholdColor(percent: number | null): string {
  if (percent === null) return 'text-zinc-400';
  if (percent < 70) return 'text-emerald-400';
  if (percent < 90) return 'text-amber-400';
  return 'text-red-400';
}

/** Get color for RTT (ms) */
function getRttColor(rtt: number | null | undefined): string {
  if (rtt === null || rtt === undefined) return 'text-zinc-400';
  if (rtt < 100) return 'text-emerald-400';
  if (rtt < 300) return 'text-amber-400';
  return 'text-red-400';
}

/** Mini sparkline SVG from history values */
function MiniSparkline({
  data,
  width = 48,
  height = 16,
  className,
}: {
  data: (number | null)[];
  width?: number;
  height?: number;
  className?: string;
}) {
  const points = useMemo(() => {
    const valid = data.filter((v): v is number => v !== null);
    if (valid.length < 2) return '';

    const max = Math.max(...valid, 1);
    const step = width / (valid.length - 1);

    return valid
      .map((v, i) => `${(i * step).toFixed(1)},${(height - (v / max) * height).toFixed(1)}`)
      .join(' ');
  }, [data, width, height]);

  if (!points) return null;

  return (
    <svg
      width={width}
      height={height}
      className={cn('inline-block', className)}
      viewBox={`0 0 ${width} ${height}`}
    >
      <polyline
        points={points}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        opacity={0.7}
      />
    </svg>
  );
}

export const PerformanceCapsule: React.FC<PerformanceCapsuleProps> = ({
  metrics,
  history,
  isRunning,
  sshRttMs,
}) => {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);

  // Merge SSH RTT from HealthTracker if not in metrics
  const rtt = metrics?.sshRttMs ?? sshRttMs ?? null;

  const cpuHistory = useMemo(
    () => history.map((h) => h.cpuPercent),
    [history]
  );

  if (!isRunning && !metrics) return null;

  const source = metrics?.source ?? 'failed';
  const isRttOnly = source === 'rtt_only' || source === 'failed';

  return (
    <div
      className={cn(
        'absolute top-2 right-2 z-10 select-none transition-all duration-200',
        'bg-black/50 backdrop-blur-md border border-white/10 rounded-lg shadow-lg',
        'text-xs font-mono',
        expanded ? 'min-w-[260px]' : ''
      )}
    >
      {/* Compact row */}
      <div
        className="flex items-center gap-2.5 px-3 py-1.5 cursor-pointer hover:bg-white/5 rounded-lg"
        onClick={() => setExpanded(!expanded)}
        title={t('capsule.click_expand', 'Click to expand')}
      >
        {/* CPU */}
        {!isRttOnly && metrics?.cpuPercent !== null && metrics?.cpuPercent !== undefined && (
          <span className={cn('flex items-center gap-1', getThresholdColor(metrics.cpuPercent))}>
            <Cpu className="w-3 h-3" />
            <span>{metrics.cpuPercent.toFixed(0)}%</span>
          </span>
        )}

        {/* Sparkline */}
        {!isRttOnly && cpuHistory.length >= 2 && (
          <MiniSparkline
            data={cpuHistory}
            className={getThresholdColor(metrics?.cpuPercent ?? null)}
          />
        )}

        {/* Memory */}
        {!isRttOnly && metrics?.memoryUsed !== null && metrics?.memoryTotal !== null &&
          metrics?.memoryUsed !== undefined && metrics?.memoryTotal !== undefined && (
          <span className={cn('flex items-center gap-1', getThresholdColor(metrics.memoryPercent ?? null))}>
            <MemoryStick className="w-3 h-3" />
            <span>
              {formatBytes(metrics.memoryUsed)}/{formatBytes(metrics.memoryTotal)}
            </span>
          </span>
        )}

        {/* Network rates */}
        {!isRttOnly && (metrics?.netRxBytesPerSec !== null || metrics?.netTxBytesPerSec !== null) && (
          <span className="flex items-center gap-1 text-sky-400">
            {metrics?.netRxBytesPerSec !== null && metrics?.netRxBytesPerSec !== undefined && (
              <span className="flex items-center gap-0.5">
                <ArrowDown className="w-2.5 h-2.5" />
                {formatRate(metrics.netRxBytesPerSec)}
              </span>
            )}
            {metrics?.netTxBytesPerSec !== null && metrics?.netTxBytesPerSec !== undefined && (
              <span className="flex items-center gap-0.5">
                <ArrowUp className="w-2.5 h-2.5" />
                {formatRate(metrics.netTxBytesPerSec)}
              </span>
            )}
          </span>
        )}

        {/* RTT */}
        {rtt !== null && (
          <span className={cn('flex items-center gap-1', getRttColor(rtt))}>
            <Activity className="w-3 h-3" />
            <span>{rtt}ms</span>
          </span>
        )}

        {/* RttOnly / no-data indicator */}
        {isRttOnly && rtt === null && !metrics && (
          <span className="text-zinc-500 italic">{t('capsule.sampling', 'Sampling...')}</span>
        )}
        {isRttOnly && rtt === null && metrics && (
          <span className="text-zinc-500 italic">{t('capsule.no_data', 'No data')}</span>
        )}

        {/* Expand/collapse icon */}
        {expanded ? (
          <ChevronUp className="w-3 h-3 text-zinc-500 ml-auto" />
        ) : (
          <ChevronDown className="w-3 h-3 text-zinc-500 ml-auto" />
        )}
      </div>

      {/* Expanded details */}
      {expanded && metrics && (
        <div className="px-3 pb-2 pt-1 border-t border-white/10 space-y-1.5">
          {/* Load averages */}
          {metrics.loadAvg1 !== null && (
            <div className="flex justify-between text-zinc-400">
              <span>{t('detail.load_avg', 'Load Avg')}</span>
              <span className="text-zinc-200">
                {metrics.loadAvg1?.toFixed(2)} / {metrics.loadAvg5?.toFixed(2)} / {metrics.loadAvg15?.toFixed(2)}
              </span>
            </div>
          )}

          {/* CPU cores */}
          {metrics.cpuCores !== null && (
            <div className="flex justify-between text-zinc-400">
              <span>{t('detail.cpu_cores', 'CPU Cores')}</span>
              <span className="text-zinc-200">{metrics.cpuCores}</span>
            </div>
          )}

          {/* Memory detail */}
          {metrics.memoryPercent !== null && (
            <div className="flex justify-between text-zinc-400">
              <span>{t('detail.memory', 'Memory')}</span>
              <span className={getThresholdColor(metrics.memoryPercent)}>
                {metrics.memoryPercent.toFixed(1)}%
              </span>
            </div>
          )}

          {/* Data source */}
          <div className="flex justify-between text-zinc-500 text-[10px] pt-1 border-t border-white/5">
            <span>{t('detail.source', 'Source')}</span>
            <span>{source}</span>
          </div>
        </div>
      )}
    </div>
  );
};
