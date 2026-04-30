// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import i18n from '@/i18n';
import { useToastStore } from '@/hooks/useToast';

export type ConnectionTraceStage =
  | 'queued'
  | 'preparing'
  | 'opening_transport'
  | 'ssh_handshake'
  | 'host_key'
  | 'authentication'
  | 'pty'
  | 'shell_ready'
  | 'ready';

export type ConnectionTraceStatus = 'running' | 'ready' | 'failed' | 'cancelled';

export interface ConnectionTraceEvent {
  attemptId: string;
  nodeId?: string;
  stage: ConnectionTraceStage;
  status: ConnectionTraceStatus;
  progress: number;
  elapsedMs: number;
  detail?: string;
  label?: string;
  stepIndex?: number;
  totalSteps?: number;
  mode?: 'connect' | 'reconnect';
}

type ActiveTrace = {
  toastId: string;
  visible: boolean;
  latest: ConnectionTraceEvent;
  showTimer: ReturnType<typeof setTimeout> | null;
  updateTimer: ReturnType<typeof setTimeout> | null;
};

const DISPLAY_DELAY_MS = 1200;
const UPDATE_COALESCE_MS = 300;
const SUCCESS_DISMISS_MS = 1800;

const activeTraces = new Map<string, ActiveTrace>();
let unlistenPromise: Promise<UnlistenFn> | null = null;

function formatElapsed(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return '0.0s';
  if (ms < 10_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.round(ms / 1000)}s`;
}

function getTraceTitle(event: ConnectionTraceEvent): string {
  const label = event.label || event.nodeId || i18n.t('connections.trace.target_unknown');
  const isReconnect = event.mode === 'reconnect';
  if (event.stepIndex && event.totalSteps && event.totalSteps > 1) {
    return i18n.t(
      isReconnect ? 'connections.trace.reconnecting_chain' : 'connections.trace.connecting_chain',
      { current: event.stepIndex, total: event.totalSteps, label },
    );
  }
  return i18n.t(isReconnect ? 'connections.trace.reconnecting' : 'connections.trace.connecting', { label });
}

function getStageText(event: ConnectionTraceEvent): string {
  return event.detail || i18n.t(`connections.trace.stage.${event.stage}`);
}

function flushTrace(trace: ActiveTrace) {
  trace.updateTimer = null;
  const event = trace.latest;
  useToastStore.getState().upsertToast(trace.toastId, {
    title: getTraceTitle(event),
    statusText: getStageText(event),
    progress: event.progress,
    variant: 'default',
    persistent: true,
    duration: 0,
  });
}

function showTrace(attemptId: string) {
  const trace = activeTraces.get(attemptId);
  if (!trace || trace.visible) return;
  if (trace.latest.status !== 'running') return;
  trace.visible = true;
  flushTrace(trace);
}

function clearTraceTimers(trace: ActiveTrace) {
  if (trace.showTimer) {
    clearTimeout(trace.showTimer);
    trace.showTimer = null;
  }
  if (trace.updateTimer) {
    clearTimeout(trace.updateTimer);
    trace.updateTimer = null;
  }
}

function scheduleTraceFlush(trace: ActiveTrace) {
  if (!trace.visible || trace.updateTimer) return;
  trace.updateTimer = setTimeout(() => flushTrace(trace), UPDATE_COALESCE_MS);
}

export function handleConnectionTraceEvent(event: ConnectionTraceEvent) {
  if (!event.attemptId) return;

  let trace = activeTraces.get(event.attemptId);
  if (!trace) {
    trace = {
      toastId: `connection-trace:${event.attemptId}`,
      visible: false,
      latest: event,
      showTimer: setTimeout(() => showTrace(event.attemptId), DISPLAY_DELAY_MS),
      updateTimer: null,
    };
    activeTraces.set(event.attemptId, trace);
  }

  trace.latest = event;

  if (event.status === 'running') {
    scheduleTraceFlush(trace);
    return;
  }

  clearTraceTimers(trace);
  activeTraces.delete(event.attemptId);

  if (event.status === 'ready') {
    if (trace.visible) {
      useToastStore.getState().upsertToast(trace.toastId, {
        title: getTraceTitle(event),
        statusText: i18n.t('connections.trace.connected', { elapsed: formatElapsed(event.elapsedMs) }),
        progress: 100,
        variant: 'success',
        duration: SUCCESS_DISMISS_MS,
        persistent: false,
      });
    }
    return;
  }

  // Failure and cancellation details are handled by the existing connection
  // error toasts + Notification Center. The trace toast only owns progress.
  if (trace.visible) {
    useToastStore.getState().removeToast(trace.toastId);
  }
}

export function initializeConnectionTraceToastManager(): () => void {
  if (!unlistenPromise) {
    unlistenPromise = listen<ConnectionTraceEvent>('connection:trace', (event) => {
      handleConnectionTraceEvent(event.payload);
    });
  }

  return () => {
    void unlistenPromise?.then((unlisten) => unlisten());
    unlistenPromise = null;
    for (const trace of activeTraces.values()) {
      clearTraceTimers(trace);
      if (trace.visible) {
        useToastStore.getState().removeToast(trace.toastId);
      }
    }
    activeTraces.clear();
  };
}

