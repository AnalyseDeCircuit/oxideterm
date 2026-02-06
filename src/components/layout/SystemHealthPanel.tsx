/**
 * SystemHealthPanel - Sidebar panel for resource profiler metrics
 *
 * Card-based layout matching FocusedNodeList / ConnectionsPanel patterns.
 * Reads from global profilerStore, derives active connection from activeTabId.
 */

import React, { useCallback, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '../../store/appStore';
import { useProfilerStore } from '../../store/profilerStore';
import { cn } from '../../lib/utils';
import { Progress } from '../ui/progress';
import {
  Cpu,
  MemoryStick,
  ArrowDown,
  ArrowUp,
  Activity,
  Gauge,
  Server,
  Wifi,
  WifiOff,
  Power,
} from 'lucide-react';

const SPARKLINE_POINTS = 12;

// ─── Helpers ────────────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function formatRate(bytesPerSec: number): string {
  if (bytesPerSec < 1024) return `${bytesPerSec} B/s`;
  if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
}

function thresholdColor(percent: number | null): string {
  if (percent === null) return 'text-theme-text-muted';
  if (percent < 70) return 'text-emerald-400';
  if (percent < 90) return 'text-amber-400';
  return 'text-red-400';
}

/** Status dot bg color matching SessionTree patterns */
function thresholdDot(percent: number | null): string {
  if (percent === null) return 'bg-theme-text-muted/50';
  if (percent < 70) return 'bg-emerald-500';
  if (percent < 90) return 'bg-amber-500';
  return 'bg-red-500';
}

function rttColor(rtt: number | null | undefined): string {
  if (rtt === null || rtt === undefined) return 'text-theme-text-muted';
  if (rtt < 100) return 'text-emerald-400';
  if (rtt < 300) return 'text-amber-400';
  return 'text-red-400';
}

// ─── Sparkline ──────────────────────────────────────────────────────────────

function Sparkline({
  data,
  width = 100,
  height = 28,
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
      .map((v, i) => `${(i * step).toFixed(1)},${(height - (v / max) * height * 0.85 - height * 0.05).toFixed(1)}`)
      .join(' ');
  }, [data, width, height]);

  if (!points) return null;

  return (
    <svg width="100%" height={height} className={cn('block', className)} viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none">
      <polyline
        points={points}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        opacity={0.6}
      />
    </svg>
  );
}

// ─── Metric Card ────────────────────────────────────────────────────────────

function MetricCard({
  label,
  value,
  icon: Icon,
  colorClass,
  dotClass,
  children,
}: {
  label: string;
  value: React.ReactNode;
  icon: React.FC<{ className?: string }>;
  colorClass?: string;
  dotClass?: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="border border-theme-border/50 rounded-lg p-3 space-y-2 bg-theme-bg-panel">
      <div className="flex items-center justify-between">
        <span className="flex items-center gap-1.5 text-theme-text-muted text-xs">
          <Icon className="w-3.5 h-3.5" />
          {label}
        </span>
        <span className="flex items-center gap-1.5">
          {dotClass && <div className={cn('w-2 h-2 rounded-full', dotClass)} />}
          <span className={cn('text-xs font-mono tabular-nums', colorClass ?? 'text-theme-text')}>
            {value}
          </span>
        </span>
      </div>
      {children}
    </div>
  );
}

// ─── Main Component ─────────────────────────────────────────────────────────

export const SystemHealthPanel: React.FC = () => {
  const { t } = useTranslation();

  // Derive active connectionId: activeTabId → tab → session → connectionId
  const activeTabId = useAppStore((s) => s.activeTabId);
  const tabs = useAppStore((s) => s.tabs);
  const sessions = useAppStore((s) => s.sessions);

  const activeConnectionId = useMemo(() => {
    if (!activeTabId) return null;
    const tab = tabs.find((tb) => tb.id === activeTabId);
    if (!tab?.sessionId) return null;
    const session = sessions.get(tab.sessionId);
    return session?.connectionId ?? null;
  }, [activeTabId, tabs, sessions]);

  // Read profiler state for active connection
  const connState = useProfilerStore((s) =>
    activeConnectionId ? s.connections.get(activeConnectionId) : undefined
  );
  const startProfiler = useProfilerStore((s) => s.startProfiler);
  const stopProfiler = useProfilerStore((s) => s.stopProfiler);

  const isEnabled = connState?.isEnabled ?? false;
  const metrics = connState?.metrics ?? null;
  const history = connState?.history?.slice(-SPARKLINE_POINTS) ?? [];
  const isRunning = connState?.isRunning ?? false;

  // Auto-start profiler only if not explicitly disabled
  useEffect(() => {
    if (activeConnectionId && connState === undefined) {
      startProfiler(activeConnectionId);
    }
  }, [activeConnectionId, connState, startProfiler]);

  const handleToggle = useCallback(() => {
    if (!activeConnectionId) return;
    if (isEnabled || isRunning) {
      stopProfiler(activeConnectionId);
    } else {
      startProfiler(activeConnectionId);
    }
  }, [activeConnectionId, isEnabled, isRunning, startProfiler, stopProfiler]);

  // Connection info for header
  const activeConnection = useAppStore((s) =>
    activeConnectionId ? s.connections.get(activeConnectionId) : undefined
  );

  const cpuHistory = useMemo(() => history.map((h) => h.cpuPercent), [history]);
  const memHistory = useMemo(() => history.map((h) => h.memoryPercent), [history]);

  const source = metrics?.source ?? 'failed';
  const isRttOnly = source === 'rtt_only' || source === 'failed';

  // ─── No connection state ───
  if (!activeConnectionId) {
    return (
      <div className="flex flex-col items-center justify-center py-8 text-theme-text-muted">
        <WifiOff className="w-8 h-8 mb-2 opacity-30" />
        <span className="text-sm">{t('profiler.panel.no_connection')}</span>
      </div>
    );
  }

  // ─── Disabled state ───
  if (!isEnabled && !isRunning) {
    return (
      <div className="space-y-2 px-2">
        <PanelHeader connection={activeConnection} isRunning={false} onToggle={handleToggle} isEnabled={false} />
        <div className="flex flex-col items-center py-8 text-theme-text-muted">
          <Power className="w-8 h-8 mb-3 opacity-20" />
          <span className="text-sm mb-3">{t('profiler.panel.disabled')}</span>
          <button
            onClick={handleToggle}
            className="px-3 py-1.5 text-xs rounded-md border border-theme-border/50 text-theme-text hover:bg-theme-bg-hover transition-colors"
          >
            {t('profiler.panel.enable')}
          </button>
        </div>
      </div>
    );
  }

  // ─── Waiting for data ───
  if (!metrics && isRunning) {
    return (
      <div className="space-y-2 px-2">
        <PanelHeader connection={activeConnection} isRunning onToggle={handleToggle} isEnabled />
        <div className="flex flex-col items-center py-6 text-theme-text-muted">
          <Activity className="w-5 h-5 animate-pulse mb-2 opacity-50" />
          <span className="text-xs">{t('profiler.panel.sampling')}</span>
        </div>
      </div>
    );
  }

  if (!metrics) {
    return (
      <div className="space-y-2 px-2">
        <PanelHeader connection={activeConnection} isRunning={false} onToggle={handleToggle} isEnabled />
        <div className="flex flex-col items-center py-6 text-theme-text-muted">
          <span className="text-xs opacity-60">{t('profiler.panel.no_data')}</span>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-2 px-2 overflow-y-auto">
      {/* Connection Header */}
      <PanelHeader connection={activeConnection} isRunning={isRunning} onToggle={handleToggle} isEnabled />

      {/* CPU Card */}
      {!isRttOnly && metrics.cpuPercent !== null && (
        <MetricCard
          label={t('profiler.panel.cpu')}
          value={`${metrics.cpuPercent.toFixed(1)}%`}
          icon={Cpu}
          colorClass={thresholdColor(metrics.cpuPercent)}
          dotClass={thresholdDot(metrics.cpuPercent)}
        >
          <Progress value={metrics.cpuPercent} className="h-1.5" />
          {cpuHistory.length >= 2 && (
            <Sparkline data={cpuHistory} className={thresholdColor(metrics.cpuPercent)} />
          )}
        </MetricCard>
      )}

      {/* Memory Card */}
      {!isRttOnly && metrics.memoryUsed !== null && metrics.memoryTotal !== null && (
        <MetricCard
          label={t('profiler.panel.memory')}
          value={`${formatBytes(metrics.memoryUsed)} / ${formatBytes(metrics.memoryTotal)}`}
          icon={MemoryStick}
          colorClass={thresholdColor(metrics.memoryPercent)}
          dotClass={thresholdDot(metrics.memoryPercent)}
        >
          <Progress value={metrics.memoryPercent ?? 0} className="h-1.5" />
          {memHistory.length >= 2 && (
            <Sparkline data={memHistory} className={thresholdColor(metrics.memoryPercent)} />
          )}
        </MetricCard>
      )}

      {/* Network I/O Card */}
      {!isRttOnly && (metrics.netRxBytesPerSec !== null || metrics.netTxBytesPerSec !== null) && (
        <div className="border border-theme-border/50 rounded-lg p-3 bg-theme-bg-panel">
          <div className="flex items-center gap-1.5 text-theme-text-muted text-xs mb-2">
            <Wifi className="w-3.5 h-3.5" />
            {t('profiler.panel.network')}
          </div>
          <div className="flex items-center justify-between text-xs font-mono tabular-nums text-theme-text">
            {metrics.netRxBytesPerSec !== null && (
              <span className="flex items-center gap-1">
                <ArrowDown className="w-3 h-3 text-emerald-400" />
                {formatRate(metrics.netRxBytesPerSec)}
              </span>
            )}
            {metrics.netTxBytesPerSec !== null && (
              <span className="flex items-center gap-1">
                <ArrowUp className="w-3 h-3 text-amber-400" />
                {formatRate(metrics.netTxBytesPerSec)}
              </span>
            )}
          </div>
        </div>
      )}

      {/* Load Averages + RTT — compact row */}
      <div className="grid grid-cols-2 gap-2">
        {!isRttOnly && metrics.loadAvg1 !== null && (
          <div className="border border-theme-border/50 rounded-lg p-3 bg-theme-bg-panel">
            <div className="flex items-center gap-1.5 text-theme-text-muted text-xs mb-1">
              <Gauge className="w-3.5 h-3.5" />
              {t('profiler.panel.load_avg')}
            </div>
            <div className="text-xs font-mono tabular-nums text-theme-text">
              {metrics.loadAvg1?.toFixed(2)} / {metrics.loadAvg5?.toFixed(2)} / {metrics.loadAvg15?.toFixed(2)}
            </div>
          </div>
        )}
        <div className="border border-theme-border/50 rounded-lg p-3 bg-theme-bg-panel">
          <div className="flex items-center gap-1.5 text-theme-text-muted text-xs mb-1">
            <Activity className="w-3.5 h-3.5" />
            {t('profiler.panel.rtt')}
          </div>
          <div className={cn('text-xs font-mono tabular-nums', rttColor(metrics.sshRttMs))}>
            {metrics.sshRttMs !== null ? `${metrics.sshRttMs} ms` : '—'}
          </div>
        </div>
      </div>

      {/* Source footer */}
      <div className="flex items-center justify-between px-1 pt-1">
        <span className="text-[10px] text-theme-text-muted/50">{t('profiler.panel.source')}</span>
        <span className="text-[10px] text-theme-text-muted/50 font-mono">{source}</span>
      </div>
    </div>
  );
};

// ─── Panel Header ───────────────────────────────────────────────────────────

function PanelHeader({
  connection,
  isRunning,
  onToggle,
  isEnabled,
}: {
  connection: { host: string; port: number; username: string } | undefined;
  isRunning?: boolean;
  onToggle?: () => void;
  isEnabled?: boolean;
}) {
  if (!connection) return null;

  return (
    <div className="flex items-center gap-2 border border-theme-border/50 rounded-lg p-3 bg-theme-bg-panel">
      <Server className={cn(
        'w-4 h-4 shrink-0',
        isRunning ? 'text-emerald-400' : 'text-theme-text-muted'
      )} />
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-theme-text truncate">
          {connection.username}@{connection.host}
        </div>
        <div className="text-xs text-theme-text-muted font-mono">
          :{connection.port}
        </div>
      </div>
      {onToggle && (
        <button
          onClick={onToggle}
          className={cn(
            'p-1 rounded transition-colors shrink-0',
            isEnabled
              ? 'text-emerald-400 hover:text-red-400 hover:bg-red-500/10'
              : 'text-theme-text-muted hover:text-emerald-400 hover:bg-emerald-500/10'
          )}
        >
          <Power className="w-3.5 h-3.5" />
        </button>
      )}
      <div className={cn(
        'w-2 h-2 rounded-full shrink-0',
        isRunning ? 'bg-emerald-500 ring-2 ring-emerald-500/20' : 'bg-theme-text-muted/50'
      )} />
    </div>
  );
}
