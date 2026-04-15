// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import {
  useNotificationCenterStore,
  type NotificationItem,
  type NotificationKind,
  type NotificationSeverity,
  type NotificationSource,
} from '../store/notificationCenterStore';

type NotifyOptions = {
  kind: NotificationKind;
  severity: NotificationSeverity;
  title: string;
  body?: string;
  nodeId?: string;
  connectionId?: string;
  dedupeKey?: string;
  source?: NotificationSource;
  preserveReadStatusOnDedupe?: boolean;
};

function inferConnectionKind(text?: string): NotificationKind {
  const normalized = (text ?? '').toLowerCase();
  if (
    normalized.includes('hostkey') ||
    normalized.includes('host key') ||
    normalized.includes('fingerprint') ||
    normalized.includes('man-in-the-middle')
  ) {
    return 'security';
  }

  return 'connection';
}

export function pushNotification(options: NotifyOptions) {
  const {
    kind,
    severity,
    title,
    body,
    nodeId,
    connectionId,
    dedupeKey,
    source,
    preserveReadStatusOnDedupe,
  } = options;

  const scope: NotificationItem['scope'] = nodeId
    ? { type: 'node', nodeId }
    : connectionId
      ? { type: 'connection', connectionId }
      : { type: 'global' };

  useNotificationCenterStore.getState().push({
    kind,
    severity,
    title,
    body,
    source: source ?? { type: 'system' },
    scope,
    dedupeKey,
    preserveReadStatusOnDedupe,
  });
}

export function notifyConnectionIssue(options: Omit<NotifyOptions, 'kind'> & { kind?: NotificationKind }) {
  pushNotification({
    ...options,
    kind: options.kind ?? inferConnectionKind(`${options.title}\n${options.body ?? ''}`),
  });
}