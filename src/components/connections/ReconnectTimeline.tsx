import React from 'react';
import { useTranslation } from 'react-i18next';
import { CheckCircle2, XCircle, SkipForward, Loader2, Clock } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { ReconnectJob, PhaseEvent, PhaseResult, ReconnectPhase } from '../../store/reconnectOrchestratorStore';

// ─── Constants ───────────────────────────────────────────────────────────────

/** Phases shown in the timeline (excludes meta-states like queued/done/failed/cancelled) */
const PIPELINE_PHASES: ReconnectPhase[] = [
  'snapshot',
  'ssh-connect',
  'await-terminal',
  'restore-forwards',
  'resume-transfers',
  'restore-ide',
];

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.round((ms % 60_000) / 1000);
  return `${m}m${s}s`;
}

function getPhaseI18nKey(phase: ReconnectPhase): string {
  return `connections.reconnect.phase.${phase.replace(/-/g, '_')}`;
}

function getResultI18nKey(result: PhaseResult): string {
  return `connections.reconnect.result.${result}`;
}

const ResultIcon = ({ result }: { result: PhaseResult }) => {
  switch (result) {
    case 'ok':
      return <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />;
    case 'failed':
      return <XCircle className="h-3.5 w-3.5 text-red-400" />;
    case 'skipped':
      return <SkipForward className="h-3.5 w-3.5 text-theme-text-muted" />;
    case 'running':
      return <Loader2 className="h-3.5 w-3.5 text-amber-400 animate-spin" />;
  }
};

// ─── Component ───────────────────────────────────────────────────────────────

type ReconnectTimelineProps = {
  job: ReconnectJob;
};

export const ReconnectTimeline: React.FC<ReconnectTimelineProps> = ({ job }) => {
  const { t } = useTranslation();

  // Build a lookup from phase → event(s). Last event per phase wins for display.
  const phaseEventMap = new Map<ReconnectPhase, PhaseEvent>();
  for (const event of job.phaseHistory) {
    phaseEventMap.set(event.phase, event);
  }

  const totalDuration = (job.endedAt ?? Date.now()) - job.startedAt;
  const isTerminal = job.status === 'done' || job.status === 'failed' || job.status === 'cancelled';

  return (
    <div className="w-64 p-3 space-y-1.5 text-xs">
      {/* Header */}
      <div className="flex items-center justify-between pb-1 border-b border-theme-border">
        <span className="font-medium text-theme-text truncate max-w-[160px]">{job.nodeName}</span>
        <span className="text-theme-text-muted flex items-center gap-1">
          <Clock className="h-3 w-3" />
          {formatDuration(totalDuration)}
        </span>
      </div>

      {/* Phase rows */}
      {PIPELINE_PHASES.map((phase, idx) => {
        const event = phaseEventMap.get(phase);
        const isPending = !event;
        // ssh-connect retries: show attempt count
        const attemptSuffix = phase === 'ssh-connect' && job.attempt > 1
          ? ` (${job.attempt}/${job.maxAttempts})`
          : '';

        return (
          <div key={phase} className="flex items-start gap-2">
            {/* Vertical line connector */}
            <div className="flex flex-col items-center w-4 flex-shrink-0">
              {isPending ? (
                <div className="h-3.5 w-3.5 rounded-full border border-theme-border bg-theme-bg" />
              ) : (
                <ResultIcon result={event.result} />
              )}
              {idx < PIPELINE_PHASES.length - 1 && (
                <div className={cn(
                  "w-px flex-1 min-h-[8px]",
                  isPending ? "bg-theme-border" : "bg-theme-border-strong"
                )} />
              )}
            </div>

            {/* Phase info */}
            <div className="flex-1 min-w-0 pb-1">
              <div className="flex items-center justify-between gap-1">
                <span className={cn(
                  "truncate",
                  isPending ? "text-theme-text-muted" : "text-theme-text"
                )}>
                  {t(getPhaseI18nKey(phase))}{attemptSuffix}
                </span>
                {event?.endedAt && event.startedAt && (
                  <span className="text-theme-text-muted flex-shrink-0">
                    {formatDuration(event.endedAt - event.startedAt)}
                  </span>
                )}
              </div>
              {event?.detail && (
                <div className={cn(
                  "truncate mt-0.5",
                  event.result === 'failed' ? "text-red-400" : "text-theme-text-muted"
                )} title={event.detail}>
                  {event.detail}
                </div>
              )}
            </div>
          </div>
        );
      })}

      {/* Footer: final status */}
      {isTerminal && (
        <div className={cn(
          "pt-1 border-t border-theme-border text-center",
          job.status === 'done' ? "text-emerald-400" :
          job.status === 'failed' ? "text-red-400" :
          "text-theme-text-muted"
        )}>
          {t(getResultI18nKey(job.status === 'done' ? 'ok' : job.status === 'cancelled' ? 'skipped' : 'failed'))}
          {job.restoredCount > 0 && ` · ${job.restoredCount} ${t('connections.reconnect.result.services')}`}
        </div>
      )}
    </div>
  );
};
