// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Update Store — Zustand store with persist for resumable updater.
 *
 * Manages the full update lifecycle: check → download → verify → install → restart.
 * Supports resumable downloads via Rust backend, with graceful fallback to legacy plugin.
 */

import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import { api } from '@/lib/api';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { retryWithExponentialBackoff } from '@/lib/retry';
import { useSettingsStore } from '@/store/settingsStore';
import { pushNotification } from '@/lib/notificationCenter';
import i18n from '@/i18n';

// ── Types ───────────────────────────────────────────────────

export type UpdateStage =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'verifying'
  | 'installing'
  | 'ready'
  | 'up-to-date'
  | 'error'
  | 'cancelled';

type ResumableUpdateStatus = {
  taskId: string;
  version: string;
  attempt: number;
  downloadedBytes: number;
  totalBytes: number | null;
  resumable: boolean;
  stage: string;
  status: string;
  errorCode: string | null;
  errorMessage: string | null;
  timestamp: number;
  retryDelayMs: number | null;
  lastHttpStatus: number | null;
  canResumeAfterRestart: boolean;
};

type ResumableEvent = {
  type: 'started' | 'resumed' | 'progress' | 'retrying' | 'verifying' | 'installing' | 'ready' | 'error' | 'cancelled';
} & ResumableUpdateStatus;

type PersistedState = {
  lastCheckedAt: number | null;
  skippedVersion: string | null;
  lastInstalledVersion: string | null;
};

type UpdateState = PersistedState & {
  // Transient state (not persisted)
  stage: UpdateStage;
  newVersion: string | null;
  currentVersion: string | null;
  releaseBody: string | null;
  releaseDate: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  downloadSpeed: number;
  etaSeconds: number | null;
  errorMessage: string | null;
  resumableTaskId: string | null;
  attempt: number;
  retryDelayMs: number | null;

  // Actions
  checkForUpdate: (opts?: { silent?: boolean }) => Promise<void>;
  startDownload: () => Promise<void>;
  cancelDownload: () => Promise<void>;
  restartApp: () => Promise<void>;
  dismiss: () => void;
  skipVersion: (version: string) => void;
  clearSkippedVersion: () => void;
  initAutoUpdateCheck: (delayMs?: number) => void;
  initResumableListeners: () => UnlistenFn;
};

// ── Store ───────────────────────────────────────────────────

let _updateRef: Update | null = null;
let _autoCheckTimer: ReturnType<typeof setTimeout> | null = null;

// Sliding window for download speed calculation (3-second window)
type SpeedSample = { time: number; bytes: number };
let _speedSamples: SpeedSample[] = [];
const SPEED_WINDOW_MS = 3000;

function updateSpeedMetrics(downloadedBytes: number, totalBytes: number | null): { downloadSpeed: number; etaSeconds: number | null } {
  const now = Date.now();
  _speedSamples.push({ time: now, bytes: downloadedBytes });
  // Trim samples older than the window
  const cutoff = now - SPEED_WINDOW_MS;
  _speedSamples = _speedSamples.filter(s => s.time >= cutoff);

  if (_speedSamples.length < 2) {
    return { downloadSpeed: 0, etaSeconds: null };
  }

  const oldest = _speedSamples[0];
  const newest = _speedSamples[_speedSamples.length - 1];
  const deltaMs = newest.time - oldest.time;
  if (deltaMs <= 0) {
    return { downloadSpeed: 0, etaSeconds: null };
  }

  const speed = ((newest.bytes - oldest.bytes) / deltaMs) * 1000; // bytes/sec
  const remaining = totalBytes != null ? totalBytes - downloadedBytes : null;
  const eta = speed > 0 && remaining != null ? remaining / speed : null;

  return { downloadSpeed: Math.max(0, speed), etaSeconds: eta != null ? Math.max(0, eta) : null };
}

function resetSpeedMetrics() {
  _speedSamples = [];
}

type SetFn = (partial: Partial<UpdateState>) => void;
type GetFn = () => UpdateState;

/** Legacy fallback: download via plugin-updater when resumable backend is unavailable */
async function legacyDownload(set: SetFn, get: GetFn) {
  const update = _updateRef;
  if (!update) {
    set({ stage: 'error', errorMessage: 'No update reference available' });
    return;
  }

  resetSpeedMetrics();
  set({ stage: 'downloading', downloadedBytes: 0, totalBytes: null, downloadSpeed: 0, etaSeconds: null });
  try {
    let totalLen = 0;
    let downloaded = 0;
    await update.downloadAndInstall((event) => {
      if (event.event === 'Started') {
        totalLen = event.data.contentLength ?? 0;
        set({ totalBytes: totalLen || null });
      } else if (event.event === 'Progress') {
        downloaded += event.data.chunkLength;
        const metrics = updateSpeedMetrics(downloaded, totalLen || null);
        set({ downloadedBytes: downloaded, ...metrics });
      } else if (event.event === 'Finished') {
        set({ downloadedBytes: totalLen, stage: 'ready', downloadSpeed: 0, etaSeconds: null });
      }
    });
    // Fallback if Finished event didn't fire
    if (get().stage !== 'ready') {
      set({ stage: 'ready', downloadedBytes: totalLen });
    }
  } catch (err) {
    const errorMessage = err instanceof Error ? err.message : String(err);
    set({
      stage: 'error',
      errorMessage,
    });
    pushNotification({
      kind: 'update',
      severity: 'error',
      title: i18n.t('settings_view.help.update_error'),
      body: errorMessage,
      dedupeKey: `update-error:${get().newVersion ?? 'unknown'}`,
    });
  }
}

export const useUpdateStore = create<UpdateState>()(
  persist(
    (set, get) => ({
      // Persisted
      lastCheckedAt: null,
      skippedVersion: null,
      lastInstalledVersion: null,

      // Transient
      stage: 'idle' as UpdateStage,
      newVersion: null,
      currentVersion: null,
      releaseBody: null,
      releaseDate: null,
      downloadedBytes: 0,
      totalBytes: null,
      downloadSpeed: 0,
      etaSeconds: null,
      errorMessage: null,
      resumableTaskId: null,
      attempt: 0,
      retryDelayMs: null,

      // ── Check ───────────────────────────────────────────

      checkForUpdate: async (opts) => {
        const silent = opts?.silent ?? false;
        set({ stage: 'checking', errorMessage: null });

        const channel = useSettingsStore.getState().settings.general.updateChannel;

        try {
          // For beta channel, use Rust-side channel-aware check.
          // For stable, use the plugin's check() (keeps _updateRef for legacy fallback).
          if (channel === 'beta') {
            const result = await retryWithExponentialBackoff(
              () => api.updateCheckWithChannel('beta'),
              { maxRetries: 2, baseDelayMs: 2000 },
            );
            _updateRef = null; // No plugin Update object for beta
            if (result) {
              const { skippedVersion } = get();
              if (silent && skippedVersion === result.version) {
                set({ stage: 'idle', lastCheckedAt: Date.now() });
                return;
              }
              set({
                stage: 'available',
                newVersion: result.version,
                currentVersion: result.currentVersion,
                releaseBody: result.body ?? null,
                releaseDate: result.date ?? null,
                lastCheckedAt: Date.now(),
              });
              pushNotification({
                kind: 'update',
                severity: 'info',
                title: i18n.t('settings_view.help.update_available'),
                body: `v${result.version}`,
                dedupeKey: `update-available:${result.version}`,
                preserveReadStatusOnDedupe: true,
              });
            } else {
              set({ stage: 'up-to-date', lastCheckedAt: Date.now() });
            }
          } else {
            const update = await retryWithExponentialBackoff(
              () => check(),
              { maxRetries: 2, baseDelayMs: 2000 },
            );

            if (update) {
              _updateRef = update;
              const { skippedVersion } = get();
              if (silent && skippedVersion === update.version) {
                set({ stage: 'idle', lastCheckedAt: Date.now() });
                return;
              }
              set({
                stage: 'available',
                newVersion: update.version,
                currentVersion: update.currentVersion,
                releaseBody: update.body ?? null,
                releaseDate: update.date ?? null,
                lastCheckedAt: Date.now(),
              });
              pushNotification({
                kind: 'update',
                severity: 'info',
                title: i18n.t('settings_view.help.update_available'),
                body: `v${update.version}`,
                dedupeKey: `update-available:${update.version}`,
                preserveReadStatusOnDedupe: true,
              });
            } else {
              _updateRef = null;
              set({ stage: 'up-to-date', lastCheckedAt: Date.now() });
            }
          }
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          // 404 / network errors / dev mode: treat as up-to-date
          if (silent && /404|not found|fetch|network|endpoint/i.test(msg)) {
            set({ stage: 'idle', lastCheckedAt: Date.now() });
            return;
          }
          if (/404|not found|fetch|network|endpoint/i.test(msg)) {
            set({ stage: 'up-to-date', lastCheckedAt: Date.now() });
          } else {
            set({ stage: 'error', errorMessage: msg, lastCheckedAt: Date.now() });
            pushNotification({
              kind: 'update',
              severity: 'error',
              title: i18n.t('settings_view.help.update_error'),
              body: msg,
              dedupeKey: `update-check-error:${msg}`,
            });
          }
        }
      },

      // ── Download (resumable backend) ────────────────────

      startDownload: async () => {
        const { newVersion } = get();
        if (!newVersion) return;

        resetSpeedMetrics();
        set({
          stage: 'downloading',
          downloadedBytes: 0,
          totalBytes: null,
          downloadSpeed: 0,
          etaSeconds: null,
          errorMessage: null,
          attempt: 1,
          retryDelayMs: null,
        });

        try {
          const channel = useSettingsStore.getState().settings.general.updateChannel;
          const taskId = await api.updateStartResumableInstall(newVersion, channel !== 'stable' ? channel : undefined);
          set({ resumableTaskId: taskId });
          // Progress will be tracked via event listener
        } catch (err) {
          // Beta channel has no legacy fallback (no plugin Update reference)
          if (!_updateRef) {
            set({
              stage: 'error',
              errorMessage: err instanceof Error ? err.message : String(err),
            });
            return;
          }
          // Resumable backend not available — fallback to legacy plugin
          console.warn('[update] Resumable install failed, falling back to legacy:', err);
          await legacyDownload(set, get);
        }
      },

      // ── Cancel ──────────────────────────────────────────

      cancelDownload: async () => {
        const { resumableTaskId } = get();
        try {
          if (resumableTaskId) {
            await api.updateCancelResumableInstall(resumableTaskId);
          }
        } catch {
          // Ignore cancel errors
        }
        _speedSamples = [];
        set({
          stage: 'idle',
          resumableTaskId: null,
          downloadedBytes: 0,
          totalBytes: null,
          errorMessage: null,
          downloadSpeed: 0,
          etaSeconds: null,
        });
      },

      // ── Restart ─────────────────────────────────────────

      restartApp: async () => {
        await relaunch();
      },

      // ── UI actions ──────────────────────────────────────

      dismiss: () => {
        set({ stage: 'idle', errorMessage: null });
      },

      skipVersion: (version: string) => {
        set({ skippedVersion: version, stage: 'idle' });
      },

      clearSkippedVersion: () => {
        set({ skippedVersion: null });
      },

      // ── Auto-check on startup ───────────────────────────

      initAutoUpdateCheck: (delayMs = 8000) => {
        if (_autoCheckTimer) clearTimeout(_autoCheckTimer);
        _autoCheckTimer = setTimeout(() => {
          get().checkForUpdate({ silent: true });
          _autoCheckTimer = null;
        }, delayMs);
      },

      // ── Resumable event listener ────────────────────────

      initResumableListeners: () => {
        let unlisten: UnlistenFn | null = null;

        const setup = async () => {
          unlisten = await listen<ResumableEvent>('update:resumable-event', (event) => {
            const payload = event.payload;

            switch (payload.type) {
              case 'started':
              case 'resumed':
                set({
                  stage: 'downloading',
                  resumableTaskId: payload.taskId,
                  downloadedBytes: payload.downloadedBytes,
                  totalBytes: payload.totalBytes,
                  attempt: payload.attempt,
                });
                break;

              case 'progress': {
                const metrics = updateSpeedMetrics(payload.downloadedBytes, payload.totalBytes);
                set({
                  downloadedBytes: payload.downloadedBytes,
                  totalBytes: payload.totalBytes,
                  ...metrics,
                });
                break;
              }

              case 'retrying':
                set({
                  attempt: payload.attempt,
                  retryDelayMs: payload.retryDelayMs,
                });
                break;

              case 'verifying':
                set({ stage: 'verifying' });
                break;

              case 'installing':
                set({ stage: 'installing' });
                break;

              case 'ready':
                resetSpeedMetrics();
                set({
                  stage: 'ready',
                  downloadedBytes: payload.downloadedBytes,
                  totalBytes: payload.totalBytes,
                  downloadSpeed: 0,
                  etaSeconds: null,
                });
                pushNotification({
                  kind: 'update',
                  severity: 'info',
                  title: i18n.t('settings_view.help.ready_to_restart'),
                  body: get().newVersion ? `v${get().newVersion}` : undefined,
                  dedupeKey: `update-ready:${get().newVersion ?? 'unknown'}`,
                  preserveReadStatusOnDedupe: true,
                });
                break;

              case 'error':
                set({
                  stage: 'error',
                  errorMessage: payload.errorMessage || 'Unknown error',
                });
                pushNotification({
                  kind: 'update',
                  severity: 'error',
                  title: i18n.t('settings_view.help.update_error'),
                  body: payload.errorMessage || 'Unknown error',
                  dedupeKey: `update-error:${payload.errorCode ?? get().newVersion ?? 'unknown'}`,
                });
                break;

              case 'cancelled':
                resetSpeedMetrics();
                set({
                  stage: 'idle',
                  resumableTaskId: null,
                  downloadedBytes: 0,
                  totalBytes: null,
                  downloadSpeed: 0,
                  etaSeconds: null,
                });
                break;
            }
          });
        };

        setup();

        return () => {
          unlisten?.();
        };
      },
    }),
    {
      name: 'oxide-update-store',
      storage: createJSONStorage(() => localStorage),
      partialize: (state): PersistedState => ({
        lastCheckedAt: state.lastCheckedAt,
        skippedVersion: state.skippedVersion,
        lastInstalledVersion: state.lastInstalledVersion,
      }),
    },
  ),
);
