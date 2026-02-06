/**
 * Profiler Store
 *
 * Global state for per-connection resource profiler metrics.
 * Both the sidebar SystemHealthPanel and the terminal PerformanceCapsule
 * read from this single source of truth.
 *
 * Lifecycle: profiler starts when startProfiler() is called (idempotent),
 * stops when SSH disconnects (backend disconnect_rx) or stopProfiler() is called.
 */

import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '../lib/api';
import type { ResourceMetrics } from '../types';

const MAX_HISTORY = 60;
const SPARKLINE_POINTS = 12;

interface ConnectionProfilerState {
  metrics: ResourceMetrics | null;
  history: ResourceMetrics[];
  isRunning: boolean;
  error: string | null;
}

interface ProfilerStore {
  /** Per-connection profiler data */
  connections: Map<string, ConnectionProfilerState>;

  /** Active Tauri event unlisteners (not serialized) */
  _unlisteners: Map<string, UnlistenFn>;

  /** Start profiler for a connection (idempotent) */
  startProfiler: (connectionId: string) => Promise<void>;

  /** Stop profiler for a connection */
  stopProfiler: (connectionId: string) => Promise<void>;

  /** Update metrics from Tauri event (internal) */
  _updateMetrics: (connectionId: string, metrics: ResourceMetrics) => void;

  /** Remove all state for a connection */
  removeConnection: (connectionId: string) => void;

  /** Get sparkline-sized history slice */
  getSparklineHistory: (connectionId: string) => ResourceMetrics[];
}

export const useProfilerStore = create<ProfilerStore>((set, get) => ({
  connections: new Map(),
  _unlisteners: new Map(),

  startProfiler: async (connectionId: string) => {
    const state = get();
    const existing = state.connections.get(connectionId);
    if (existing?.isRunning) return; // idempotent

    try {
      await api.startResourceProfiler(connectionId);

      // Load existing history from backend
      let existingHistory: ResourceMetrics[] = [];
      try {
        existingHistory = await api.getResourceHistory(connectionId);
      } catch {
        // ignore — history may not exist yet
      }

      // Subscribe to Tauri events
      const eventName = `profiler:update:${connectionId}`;
      const unlisten = await listen<ResourceMetrics>(eventName, (event) => {
        get()._updateMetrics(connectionId, event.payload);
      });

      // Store unlisten fn
      const unlisteners = new Map(get()._unlisteners);
      unlisteners.set(connectionId, unlisten);

      // Update state
      const connections = new Map(get().connections);
      connections.set(connectionId, {
        metrics: existingHistory.length > 0
          ? existingHistory[existingHistory.length - 1]
          : null,
        history: existingHistory.slice(-MAX_HISTORY),
        isRunning: true,
        error: null,
      });

      set({ connections, _unlisteners: unlisteners });
    } catch (e) {
      const connections = new Map(get().connections);
      connections.set(connectionId, {
        metrics: null,
        history: [],
        isRunning: false,
        error: String(e),
      });
      set({ connections });
    }
  },

  stopProfiler: async (connectionId: string) => {
    // Unlisten events
    const unlisten = get()._unlisteners.get(connectionId);
    if (unlisten) {
      unlisten();
      const unlisteners = new Map(get()._unlisteners);
      unlisteners.delete(connectionId);
      set({ _unlisteners: unlisteners });
    }

    try {
      await api.stopResourceProfiler(connectionId);
    } catch {
      // idempotent
    }

    const connections = new Map(get().connections);
    const existing = connections.get(connectionId);
    if (existing) {
      connections.set(connectionId, { ...existing, isRunning: false });
      set({ connections });
    }
  },

  _updateMetrics: (connectionId: string, metrics: ResourceMetrics) => {
    const connections = new Map(get().connections);
    const existing = connections.get(connectionId);
    const prevHistory = existing?.history ?? [];
    const newHistory = [...prevHistory, metrics];
    if (newHistory.length > MAX_HISTORY) {
      newHistory.splice(0, newHistory.length - MAX_HISTORY);
    }

    connections.set(connectionId, {
      metrics,
      history: newHistory,
      isRunning: existing?.isRunning ?? true,
      error: null,
    });
    set({ connections });
  },

  removeConnection: (connectionId: string) => {
    // Unlisten
    const unlisten = get()._unlisteners.get(connectionId);
    if (unlisten) unlisten();

    const connections = new Map(get().connections);
    connections.delete(connectionId);
    const unlisteners = new Map(get()._unlisteners);
    unlisteners.delete(connectionId);
    set({ connections, _unlisteners: unlisteners });
  },

  getSparklineHistory: (connectionId: string) => {
    const state = get().connections.get(connectionId);
    if (!state) return [];
    return state.history.slice(-SPARKLINE_POINTS);
  },
}));
