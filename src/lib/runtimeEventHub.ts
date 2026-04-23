// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { NodeStateEvent, ResourceMetrics } from '../types';

export interface ConnectionStatusEvent {
  connection_id: string;
  status: 'connected' | 'link_down' | 'reconnecting' | 'disconnected';
  affected_children: string[];
  timestamp: number;
}

export interface EnvDetectedEvent {
  connectionId: string;
  osType: string;
  osVersion?: string;
  kernel?: string;
  arch?: string;
  shell?: string;
  detectedAt: number;
}

export interface ForwardRuntimeEvent {
  type: 'statusChanged' | 'statsUpdated' | 'sessionSuspended';
  forward_id?: string;
  session_id: string;
  status?: 'starting' | 'active' | 'stopped' | 'error' | 'suspended';
  error?: string;
  stats?: {
    connection_count: number;
    active_connections: number;
    bytes_sent: number;
    bytes_received: number;
  };
  forward_ids?: string[];
}

export interface ProfilerUpdateEvent {
  connectionId: string;
  metrics: ResourceMetrics;
}

type RuntimeEventMap = {
  connectionStatusChanged: ConnectionStatusEvent;
  envDetected: EnvDetectedEvent;
  nodeState: NodeStateEvent;
  forwardEvent: ForwardRuntimeEvent;
  profilerUpdate: ProfilerUpdateEvent;
};

type EventKey = keyof RuntimeEventMap;
type RuntimeEventHandler<K extends EventKey> = (payload: RuntimeEventMap[K]) => void;

class RuntimeEventHub {
  private handlers = new Map<EventKey, Set<(payload: unknown) => void>>();
  private unlisteners = new Map<EventKey, UnlistenFn>();
  private startPromises = new Map<EventKey, Promise<void>>();

  subscribe<K extends EventKey>(event: K, handler: RuntimeEventHandler<K>): () => void {
    let set = this.handlers.get(event);
    if (!set) {
      set = new Set();
      this.handlers.set(event, set);
    }
    set.add(handler as (payload: unknown) => void);
    void this.ensureStarted(event);

    return () => {
      const current = this.handlers.get(event);
      if (current) {
        current.delete(handler as (payload: unknown) => void);
        if (current.size === 0) {
          this.handlers.delete(event);
        }
      }
      if (!this.handlers.has(event)) {
        void this.teardown(event);
      }
    };
  }

  private emit<K extends EventKey>(event: K, payload: RuntimeEventMap[K]): void {
    const set = this.handlers.get(event);
    if (!set || set.size === 0) return;

    for (const handler of set) {
      try {
        handler(payload);
      } catch (error) {
        console.error(`[RuntimeEventHub] handler for ${event} failed:`, error);
      }
    }
  }

  private getListenerFactory<K extends EventKey>(event: K): Promise<UnlistenFn> {
    switch (event) {
      case 'connectionStatusChanged':
        return listen<ConnectionStatusEvent>('connection_status_changed', (tauriEvent) => {
          this.emit('connectionStatusChanged', tauriEvent.payload);
        });
      case 'envDetected':
        return listen<EnvDetectedEvent>('env:detected', (tauriEvent) => {
          this.emit('envDetected', tauriEvent.payload);
        });
      case 'nodeState':
        return listen<NodeStateEvent>('node:state', (tauriEvent) => {
          this.emit('nodeState', tauriEvent.payload);
        });
      case 'forwardEvent':
        return listen<ForwardRuntimeEvent>('forward-event', (tauriEvent) => {
          this.emit('forwardEvent', tauriEvent.payload);
        });
      case 'profilerUpdate':
        return listen<ProfilerUpdateEvent>('profiler:update', (tauriEvent) => {
          this.emit('profilerUpdate', tauriEvent.payload);
        });
    }
  }

  private async ensureStarted<K extends EventKey>(event: K): Promise<void> {
    if (this.unlisteners.has(event)) return;
    const existing = this.startPromises.get(event);
    if (existing) return existing;

    const startPromise = this.getListenerFactory(event)
      .then((unlisten) => {
        this.unlisteners.set(event, unlisten);
      })
      .catch((error) => {
        console.error(`[RuntimeEventHub] Failed to initialize ${event} listener:`, error);
        throw error;
      })
      .finally(() => {
        this.startPromises.delete(event);
      });

    this.startPromises.set(event, startPromise);
    await startPromise;

    if (!this.handlers.has(event)) {
      await this.teardown(event);
    }
  }

  private async teardown<K extends EventKey>(event: K): Promise<void> {
    const pending = this.startPromises.get(event);
    if (pending) {
      await pending.catch(() => undefined);
    }

    const unlisten = this.unlisteners.get(event);
    if (!unlisten) return;

    try {
      unlisten();
    } catch (error) {
      console.error(`[RuntimeEventHub] Failed to unlisten ${event}:`, error);
    }
    this.unlisteners.delete(event);
  }

  async resetForTests(): Promise<void> {
    this.handlers.clear();
    const events = Array.from(this.unlisteners.keys());
    await Promise.all(events.map((event) => this.teardown(event)));
    this.startPromises.clear();
  }
}

export const runtimeEventHub = new RuntimeEventHub();

export function resetRuntimeEventHubForTests(): Promise<void> {
  return runtimeEventHub.resetForTests();
}
