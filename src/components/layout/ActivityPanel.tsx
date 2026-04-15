// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { Bell, ScrollText } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { EventLogPanel } from './EventLogPanel';
import { NotificationsPanel } from './NotificationsPanel';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { useActivityStore, type ActivityView } from '../../store/activityStore';
import { useNotificationCenterStore } from '../../store/notificationCenterStore';
import { useEventLogStore } from '../../store/eventLogStore';
import { cn } from '../../lib/utils';

type ActivityTabBadgeProps = {
  count?: number;
  tone?: 'default' | 'critical';
};

const ActivityTabBadge = ({ count, tone = 'default' }: ActivityTabBadgeProps) => {
  if (!count) {
    return null;
  }

  return (
    <span
      className={cn(
        'inline-flex min-w-[16px] items-center justify-center rounded-full px-1.5 text-[10px] font-semibold leading-4',
        tone === 'critical'
          ? 'bg-red-500/15 text-red-400'
          : 'bg-theme-bg-hover text-theme-text-muted',
      )}
    >
      {count}
    </span>
  );
};

export const ActivityPanel = () => {
  const { t } = useTranslation();
  const activeView = useActivityStore((s) => s.activeView);
  const setActiveView = useActivityStore((s) => s.setActiveView);
  const notificationUnreadCount = useNotificationCenterStore((s) => s.unreadCount);
  const notificationUnreadCriticalCount = useNotificationCenterStore((s) => s.unreadCriticalCount);
  const eventLogUnreadCount = useEventLogStore((s) => s.unreadCount);
  const eventLogUnreadErrors = useEventLogStore((s) => s.unreadErrors);

  const handleViewChange = (value: string) => {
    setActiveView(value as ActivityView);
  };

  return (
    <Tabs value={activeView} onValueChange={handleViewChange} className="flex h-full flex-col bg-theme-bg">
      <div className="border-b border-theme-border bg-theme-bg-panel px-6 py-4">
        <h1 className="text-2xl font-bold text-theme-text-heading">{t('activity.title')}</h1>
        <p className="mt-2 text-sm text-theme-text-muted">{t('activity.description')}</p>

        <TabsList className="mt-4 h-10 bg-theme-bg px-1.5">
          <TabsTrigger value="notifications" className="gap-2 px-3 text-xs">
            <Bell className="h-3.5 w-3.5" />
            <span>{t('tabs.notifications')}</span>
            <ActivityTabBadge
              count={notificationUnreadCount}
              tone={notificationUnreadCriticalCount > 0 ? 'critical' : 'default'}
            />
          </TabsTrigger>
          <TabsTrigger value="event_log" className="gap-2 px-3 text-xs">
            <ScrollText className="h-3.5 w-3.5" />
            <span>{t('tabs.event_log')}</span>
            <ActivityTabBadge
              count={eventLogUnreadErrors > 0 ? eventLogUnreadErrors : eventLogUnreadCount}
              tone={eventLogUnreadErrors > 0 ? 'critical' : 'default'}
            />
          </TabsTrigger>
        </TabsList>
      </div>

      <TabsContent value="notifications" className="mt-0 flex-1 min-h-0">
        <NotificationsPanel />
      </TabsContent>

      <TabsContent value="event_log" className="mt-0 flex-1 min-h-0">
        <EventLogPanel />
      </TabsContent>
    </Tabs>
  );
};