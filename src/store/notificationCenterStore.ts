// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Notification Center Store — Phase 1
 *
 * In-memory notification inbox with unread state, filtering, and deduplication.
 * Receives notifications via Tauri `notification:push` events.
 *
 * Phase 2 will add backend persistence for unresolved/security notifications.
 */

import { create } from 'zustand';
import { listen } from '@tauri-apps/api/event';

// ============================================================================
// Types
// ============================================================================

export type NotificationKind =
  | 'connection'
  | 'security'
  | 'transfer'
  | 'update'
  | 'health'
  | 'plugin'
  | 'agent';

export type NotificationSeverity = 'info' | 'warning' | 'error' | 'critical';

export type NotificationSource =
  | { type: 'system' }
  | { type: 'plugin'; pluginId: string }
  | { type: 'agent' };

export type NotificationScope =
  | { type: 'global' }
  | { type: 'node'; nodeId: string }
  | { type: 'connection'; connectionId: string };

export type NotificationActionKind =
  | 'retryConnection'
  | 'openSessionManager'
  | 'openEventLog'
  | 'openSettings'
  | 'openSavedConnection'
  | 'acceptHostKey'
  | 'dismiss';

export type NotificationActionVariant = 'primary' | 'secondary' | 'danger';

export type NotificationAction = {
  id: string;
  label: string;
  kind: NotificationActionKind;
  variant: NotificationActionVariant;
};

export type NotificationStatus = 'unread' | 'read' | 'dismissed';

export type NotificationItem = {
  id: string;
  createdAtMs: number;
  kind: NotificationKind;
  severity: NotificationSeverity;
  title: string;
  body?: string;
  source: NotificationSource;
  status: NotificationStatus;
  scope: NotificationScope;
  actions: NotificationAction[];
  dedupeKey?: string;
};

export type NotificationFilter = {
  status: 'all' | 'unread';
  severity: 'all' | NotificationSeverity;
  kind: 'all' | NotificationKind;
};

// ============================================================================
// Constants
// ============================================================================

const MAX_ITEMS = 200;

// ============================================================================
// Store
// ============================================================================

/** Minimal payload for pushing a notification. id, createdAtMs, status auto-generated. */
export type NotificationPush = Omit<NotificationItem, 'id' | 'createdAtMs' | 'status' | 'actions'> & {
  id?: string;
  createdAtMs?: number;
  actions?: NotificationAction[];
};

type NotificationCenterState = {
  items: NotificationItem[];
  filter: NotificationFilter;

  // Derived counts (computed in actions for performance)
  unreadCount: number;
  unreadCriticalCount: number;

  // Actions
  push: (item: NotificationPush) => void;
  markRead: (id: string) => void;
  markAllRead: () => void;
  dismiss: (id: string) => void;
  dismissAll: () => void;
  setFilter: (filter: Partial<NotificationFilter>) => void;
  clear: () => void;
};

export const useNotificationCenterStore = create<NotificationCenterState>((set) => ({
  items: [],
  filter: { status: 'all', severity: 'all', kind: 'all' },
  unreadCount: 0,
  unreadCriticalCount: 0,

  push: (incoming) => set((state) => {
    const item: NotificationItem = {
      ...incoming,
      id: incoming.id ?? crypto.randomUUID(),
      createdAtMs: incoming.createdAtMs ?? Date.now(),
      actions: incoming.actions ?? [],
      status: 'unread',
    };

    let newItems: NotificationItem[];

    // Dedupe: update existing record if dedupeKey matches
    if (item.dedupeKey) {
      const existingIdx = state.items.findIndex(
        (n) => n.dedupeKey === item.dedupeKey && n.status !== 'dismissed',
      );
      if (existingIdx !== -1) {
        newItems = [...state.items];
        const existing = newItems[existingIdx];
        newItems[existingIdx] = {
          ...existing,
          createdAtMs: item.createdAtMs,
          title: item.title,
          body: item.body,
          severity: item.severity,
          actions: item.actions,
          status: 'unread',
        };
        return {
          items: newItems,
          ...recount(newItems),
        };
      }
    }

    newItems = [...state.items, item];

    // Ring buffer
    if (newItems.length > MAX_ITEMS) {
      newItems = newItems.slice(newItems.length - MAX_ITEMS);
    }

    return {
      items: newItems,
      ...recount(newItems),
    };
  }),

  markRead: (id) => set((state) => {
    const newItems = state.items.map((n) =>
      n.id === id && n.status === 'unread' ? { ...n, status: 'read' as const } : n,
    );
    return { items: newItems, ...recount(newItems) };
  }),

  markAllRead: () => set((state) => {
    const newItems = state.items.map((n) =>
      n.status === 'unread' ? { ...n, status: 'read' as const } : n,
    );
    return { items: newItems, unreadCount: 0, unreadCriticalCount: 0 };
  }),

  dismiss: (id) => set((state) => {
    const newItems = state.items.filter((n) => n.id !== id);
    return { items: newItems, ...recount(newItems) };
  }),

  dismissAll: () => set({ items: [], unreadCount: 0, unreadCriticalCount: 0 }),

  setFilter: (partial) => set((state) => ({
    filter: { ...state.filter, ...partial },
  })),

  clear: () => set({ items: [], unreadCount: 0, unreadCriticalCount: 0 }),
}));

// ============================================================================
// Helpers
// ============================================================================

function recount(items: NotificationItem[]) {
  let unreadCount = 0;
  let unreadCriticalCount = 0;
  for (const n of items) {
    if (n.status === 'unread') {
      unreadCount++;
      if (n.severity === 'critical' || n.severity === 'error') {
        unreadCriticalCount++;
      }
    }
  }
  return { unreadCount, unreadCriticalCount };
}

// ============================================================================
// Tauri event listener (call once at app startup)
// ============================================================================

let _unlisten: (() => void) | null = null;
let _initPromise: Promise<void> | null = null;

export function initNotificationListener(): Promise<void> {
  if (!_initPromise) {
    _initPromise = (async () => {
      _unlisten = await listen<NotificationPush>('notification:push', (event) => {
        useNotificationCenterStore.getState().push(event.payload);
      });
    })();
  }
  return _initPromise;
}

export function teardownNotificationListener() {
  _unlisten?.();
  _unlisten = null;
  _initPromise = null;
}
