import { beforeEach, describe, expect, it } from 'vitest';
import {
  useNotificationCenterStore,
  type NotificationPush,
} from '@/store/notificationCenterStore';

function getStore() {
  return useNotificationCenterStore.getState();
}

function resetNotificationCenterStore() {
  useNotificationCenterStore.setState({
    items: [],
    filter: { status: 'all', severity: 'all', kind: 'all' },
    unreadCount: 0,
    unreadCriticalCount: 0,
  });
}

function makeNotification(overrides: Partial<NotificationPush> = {}): NotificationPush {
  return {
    kind: 'connection',
    severity: 'info',
    title: 'Connection notice',
    body: 'Connection updated',
    source: { type: 'system' },
    scope: { type: 'global' },
    ...overrides,
  };
}

describe('notificationCenterStore', () => {
  beforeEach(() => {
    resetNotificationCenterStore();
  });

  it('adds a notification as unread and tracks counts', () => {
    getStore().push(makeNotification());

    expect(getStore().items).toHaveLength(1);
    expect(getStore().items[0].status).toBe('unread');
    expect(getStore().unreadCount).toBe(1);
    expect(getStore().unreadCriticalCount).toBe(0);
  });

  it('dedupes by key and refreshes the existing item instead of appending', () => {
    getStore().push(makeNotification({
      dedupeKey: 'connection:node-1',
      title: 'Initial title',
      body: 'Initial body',
      createdAtMs: 100,
    }));

    getStore().push(makeNotification({
      dedupeKey: 'connection:node-1',
      kind: 'security',
      severity: 'error',
      title: 'Updated title',
      body: 'Updated body',
      createdAtMs: 200,
      source: { type: 'agent' },
      scope: { type: 'node', nodeId: 'node-1' },
    }));

    expect(getStore().items).toHaveLength(1);
    expect(getStore().items[0]).toMatchObject({
      kind: 'security',
      severity: 'error',
      title: 'Updated title',
      body: 'Updated body',
      createdAtMs: 200,
      source: { type: 'agent' },
      scope: { type: 'node', nodeId: 'node-1' },
      status: 'unread',
    });
    expect(getStore().unreadCount).toBe(1);
    expect(getStore().unreadCriticalCount).toBe(1);
  });

  it('marks a deduped notification unread again by default when the same issue reoccurs', () => {
    getStore().push(makeNotification({
      dedupeKey: 'connection:node-2',
      severity: 'warning',
      title: 'Transient issue',
    }));

    const firstId = getStore().items[0].id;
    getStore().markRead(firstId);

    expect(getStore().items[0].status).toBe('read');
    expect(getStore().unreadCount).toBe(0);

    getStore().push(makeNotification({
      dedupeKey: 'connection:node-2',
      severity: 'warning',
      title: 'Transient issue happened again',
      createdAtMs: 300,
    }));

    expect(getStore().items).toHaveLength(1);
    expect(getStore().items[0].id).toBe(firstId);
    expect(getStore().items[0].title).toBe('Transient issue happened again');
    expect(getStore().items[0].status).toBe('unread');
    expect(getStore().unreadCount).toBe(1);
  });

  it('preserves read state on dedupe when preserveReadStatusOnDedupe is enabled', () => {
    getStore().push(makeNotification({
      dedupeKey: 'update:v1.2.3',
      kind: 'update',
      title: 'New version available',
    }));

    const notificationId = getStore().items[0].id;
    getStore().markRead(notificationId);

    getStore().push(makeNotification({
      dedupeKey: 'update:v1.2.3',
      kind: 'update',
      title: 'New version available',
      body: 'v1.2.3',
      preserveReadStatusOnDedupe: true,
      createdAtMs: 400,
    }));

    expect(getStore().items).toHaveLength(1);
    expect(getStore().items[0]).toMatchObject({
      id: notificationId,
      status: 'read',
      body: 'v1.2.3',
      createdAtMs: 400,
    });
    expect(getStore().unreadCount).toBe(0);
  });
});