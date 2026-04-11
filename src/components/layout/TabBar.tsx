// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import React, { useRef, useCallback, useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { X, Terminal, FolderOpen, GitFork, RefreshCw, XCircle, WifiOff, Settings, Activity, Network, Plug, Square, HardDrive, LayoutList, Puzzle, Monitor, Copy, CirclePause, Bot } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useReconnectOrchestratorStore } from '../../store/reconnectOrchestratorStore';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { cn } from '../../lib/utils';
import { Tab, PaneNode } from '../../types';
import { topologyResolver } from '../../lib/topologyResolver';
import { resolvePluginIcon } from '../../lib/plugin/pluginIconResolver';
import { resolvePluginTabTitle } from '../../lib/plugin/pluginI18nManager';
import { selectVisiblePluginContextMenuItems } from '../../lib/plugin/pluginHostUi';
import { ReconnectTimeline } from '../connections/ReconnectTimeline';
import { TabBarTerminalActions } from './TabBarTerminalActions';
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger } from '../ui/context-menu';
import { Tooltip, TooltipTrigger, TooltipContent } from '../ui/tooltip';
import { useConfirm } from '../../hooks/useConfirm';
import { usePluginStore } from '../../store/pluginStore';

/** Count leaf panes in a pane tree */
function countPanes(node: PaneNode): number {
  if (node.type === 'leaf') return 1;
  return node.children.reduce((sum, child) => sum + countPanes(child), 0);
}

const TabIcon = ({ type }: { type: string }) => {
  const iconClass = "h-3.5 w-3.5 opacity-70";
  switch (type) {
    case 'terminal':
      return <Terminal className={iconClass} />;
    case 'local_terminal':
      return <Square className={iconClass} />;
    case 'sftp':
      return <FolderOpen className={iconClass} />;
    case 'forwards':
      return <GitFork className={iconClass} />;
    case 'settings':
      return <Settings className={iconClass} />;
    case 'connection_monitor':
      return <Activity className={iconClass} />;
    case 'connection_pool':
      return <Plug className={iconClass} />;
    case 'topology':
      return <div className="text-[10px]"><Network className={iconClass} /></div>;
    case 'file_manager':
      return <HardDrive className={iconClass} />;
    case 'session_manager':
      return <LayoutList className={iconClass} />;
    case 'plugin_manager':
      return <Puzzle className={iconClass} />;
    case 'graphics':
      return <Monitor className={iconClass} />;
    case 'launcher':
      return <Monitor className={iconClass} />;
    case 'ai_agent':
      return <Bot className={iconClass} />;
    case 'plugin':
      return null; // handled by PluginTabIcon
    default:
      return null;
  }
};

/** Resolve plugin tab icon from manifest icon name */
const PluginTabIcon = ({ iconName }: { iconName: string }) => {
  const Icon = resolvePluginIcon(iconName);
  return <Icon className="h-3.5 w-3.5 opacity-70" />;
};

/** Tiny colored dot indicating connection state */
const ConnectionDot = ({ state }: { state: string }) => {
  let colorClass: string;
  switch (state) {
    case 'active':
      colorClass = 'bg-emerald-400';
      break;
    case 'idle':
      colorClass = 'bg-yellow-400';
      break;
    case 'connecting':
    case 'reconnecting':
      colorClass = 'bg-amber-400 animate-pulse';
      break;
    case 'link_down':
      colorClass = 'bg-red-400 animate-pulse';
      break;
    case 'disconnected':
    case 'disconnecting':
      colorClass = 'bg-theme-text-muted';
      break;
    default:
      // error state (object)
      colorClass = 'bg-red-500';
      break;
  }
  return <span className={cn("inline-block w-1.5 h-1.5 rounded-full shrink-0", colorClass)} />;
};

// Get dynamic tab title (non-hook version for use in render)
const getTabTitle = (
  tab: Tab,
  sessions: Map<string, { name: string }>,
  t: (key: string) => string
): string => {
  // For singleton tabs, always use translated title
  switch (tab.type) {
    case 'settings':
      return t('sidebar.panels.settings');
    case 'connection_monitor':
      return t('sidebar.panels.connection_monitor');
    case 'connection_pool':
      return t('sidebar.panels.connection_pool');
    case 'topology':
      return t('sidebar.panels.connection_matrix');
    case 'file_manager':
      return t('fileManager.title');
    case 'session_manager':
      return t('tabs.session_manager');
    case 'graphics':
      return t('graphics.tab_title');
    case 'launcher':
      return t('launcher.tabTitle');
    case 'plugin_manager':
      return t('tabs.plugin_manager');
    case 'plugin':
      if (tab.pluginTabId) {
        return resolvePluginTabTitle(tab.pluginTabId) || tab.title;
      }
      return tab.title;
  }

  // Calculate pane count for terminal tabs with split panes
  const paneCount = tab.rootPane ? countPanes(tab.rootPane) : 1;
  const paneCountSuffix = paneCount > 1 ? ` (${paneCount})` : '';

  // For terminal tabs (may have rootPane instead of sessionId after split)
  if (tab.type === 'terminal' || tab.type === 'local_terminal') {
    // Get session name from sessionId if exists
    if (tab.sessionId) {
      const session = sessions.get(tab.sessionId);
      const sessionName = session?.name || tab.title;

      if (tab.type === 'terminal') {
        return sessionName + paneCountSuffix;
      } else {
        return tab.title + paneCountSuffix;
      }
    }

    // For split panes (sessionId cleared, use tab.title)
    return tab.title + paneCountSuffix;
  }

  // For session-based tabs (SFTP, Forwards)
  if (tab.sessionId) {
    const session = sessions.get(tab.sessionId);
    const sessionName = session?.name || tab.title;

    switch (tab.type) {
      case 'sftp':
        return `${t('sidebar.panels.sftp')}: ${sessionName}`;
      case 'forwards':
        return `${t('sidebar.panels.forwards')}: ${sessionName}`;
    }
  }

  // Fallback to stored title
  return tab.title;
};

// ─── Reconnect Indicator with Hover Timeline ────────────────────────────────

import type { ReconnectJob } from '../../store/reconnectOrchestratorStore';

// ─── TabItem (memoized individual tab) ───────────────────────────────────────

type TabItemProps = {
  tab: Tab;
  tabIndex: number;
  isActive: boolean;
  isBeingDragged: boolean;
  isActuallyDragging: boolean;
  showDropIndicator: boolean;
  closing: string | null;
  tabCount: number;
  tabRefsMap: React.MutableRefObject<Map<string, HTMLDivElement>>;
  onPointerDown: (e: React.PointerEvent, tabId: string, index: number) => void;
  onPointerMove: (e: React.PointerEvent) => void;
  onPointerUp: (e: React.PointerEvent) => void;
  onPointerCancel: () => void;
  onCloseTab: (
    e: React.MouseEvent | null,
    tabId: string,
    sessionId: string | undefined,
    tabType: string,
  ) => Promise<void>;
  onReconnect: (e: React.MouseEvent, sessionId: string) => void;
  onCancelReconnect: (e: React.MouseEvent, nodeId: string) => void;
  onCloseOtherTabs: (keepTabId: string) => void;
  onCloseTabsToRight: (fromIndex: number) => void;
  onCloseAllTabs: () => void;
  onDetachTab: (tabId: string, sessionId: string) => void;
  onSetActiveTab: (tabId: string) => void;
};

const TabItem = React.memo<TabItemProps>(({
  tab, tabIndex, isActive, isBeingDragged, isActuallyDragging, showDropIndicator,
  closing, tabCount, tabRefsMap,
  onPointerDown, onPointerMove, onPointerUp, onPointerCancel,
  onCloseTab, onReconnect, onCancelReconnect, onCloseOtherTabs,
  onCloseTabsToRight, onCloseAllTabs, onDetachTab, onSetActiveTab,
}) => {
  const { t } = useTranslation();
  const pluginContextMenuRegistry = usePluginStore((state) => state.contextMenuItems);
  const pluginContextMenuItems = React.useMemo(
    () => selectVisiblePluginContextMenuItems(pluginContextMenuRegistry, 'tab'),
    [pluginContextMenuRegistry],
  );

  // Read session/connection state on demand (not subscribed — avoids re-renders on connection:update)
  const sessions = useAppStore.getState().sessions;
  const connections = useAppStore.getState().connections;
  const session = tab.sessionId ? sessions.get(tab.sessionId) : undefined;
  const connectionId = session?.connectionId;
  const connectionState = connectionId ? connections.get(connectionId)?.state : undefined;

  // Look up orchestrator job — read via hook to ensure reactivity for reconnect progress
  const nodeId = connectionId ? topologyResolver.getNodeId(connectionId) : undefined;
  const orchestratorGetJob = useReconnectOrchestratorStore(s => s.getJob);
  const orchJob = nodeId ? orchestratorGetJob(nodeId) : undefined;
  const isOrchestratorActive = orchJob && orchJob.status !== 'done' && orchJob.status !== 'failed' && orchJob.status !== 'cancelled';
  const isManualReconnecting = !!isOrchestratorActive;
  const showReconnectProgress = !!isOrchestratorActive;

  return (
    <ContextMenu>
    <ContextMenuTrigger asChild>
    <div
      ref={(el) => {
        if (el) tabRefsMap.current.set(tab.id, el);
        else tabRefsMap.current.delete(tab.id);
      }}
      onPointerDown={(e) => onPointerDown(e, tab.id, tabIndex)}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerCancel}
      onMouseDown={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          void onCloseTab(e, tab.id, tab.sessionId, tab.type);
        }
      }}
      onClick={() => {
        if (!isActuallyDragging) onSetActiveTab(tab.id);
      }}
      className={cn(
        "flex-shrink-0 group flex items-center gap-2 px-3 h-full min-w-[120px] max-w-[240px] border-r border-theme-border cursor-pointer select-none text-sm transition-[color,background-color,border-color,box-shadow] duration-150",
        isActive
          ? "bg-theme-bg-panel text-theme-text border-t-2 border-t-theme-accent shadow-[inset_0_1px_0_var(--theme-accent)]"
          : "bg-theme-bg text-theme-text-muted hover:bg-theme-bg-hover hover:text-theme-text border-t-2 border-t-transparent",
        showReconnectProgress && "border-t-amber-500",
        isBeingDragged && isActuallyDragging && "opacity-50",
        showDropIndicator && "border-l-2 border-l-theme-accent"
      )}
      style={isBeingDragged && isActuallyDragging ? { cursor: 'grabbing' } : undefined}
    >
      {tab.type === 'plugin' && tab.icon ? <PluginTabIcon iconName={tab.icon} /> : <TabIcon type={tab.type} />}
      {tab.type === 'terminal' && connectionState && (
        <ConnectionDot state={typeof connectionState === 'string' ? connectionState : 'error'} />
      )}
      <span className="truncate flex-1">{getTabTitle(tab, sessions, t)}</span>

      {/* Reconnect progress indicator with hover timeline */}
      {showReconnectProgress && orchJob && nodeId && (
        <ReconnectIndicator
          job={orchJob}
          nodeId={nodeId}
          onCancel={onCancelReconnect}
          t={t}
        />
      )}

      {/* Normal tab controls */}
      {!showReconnectProgress && (
        <div className="flex items-center gap-0.5">
          {/* Refresh button for terminal tabs */}
          {tab.type === 'terminal' && (
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={(e) => tab.sessionId && onReconnect(e, tab.sessionId)}
                  disabled={isManualReconnecting}
                  className={cn(
                    "opacity-0 group-hover:opacity-100 hover:bg-theme-bg-hover rounded-md p-0.5 transition-opacity",
                    isActive && "opacity-100",
                    isManualReconnecting && "opacity-100"
                  )}
                >
                  <RefreshCw className={cn("h-3 w-3", isManualReconnecting && "animate-spin")} />
                </button>
              </TooltipTrigger>
              <TooltipContent side="bottom">{t('tabbar.reconnect')}</TooltipContent>
            </Tooltip>
          )}
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                  onClick={(e) => void onCloseTab(e, tab.id, tab.sessionId, tab.type)}
                disabled={tab.sessionId ? closing === tab.sessionId : false}
                className={cn(
                  "opacity-0 group-hover:opacity-100 hover:bg-theme-bg-hover rounded-md p-0.5 transition-opacity",
                  isActive && "opacity-100",
                  (tab.sessionId && closing === tab.sessionId) && "opacity-100"
                )}
              >
                <X className="h-3 w-3" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="bottom">{t('tabbar.close_tab')}</TooltipContent>
          </Tooltip>
        </div>
      )}
    </div>
    </ContextMenuTrigger>
    <ContextMenuContent>
      <ContextMenuItem onSelect={() => {
        const currentSessions = useAppStore.getState().sessions;
        const title = getTabTitle(tab, currentSessions, t);
        navigator.clipboard.writeText(title);
      }}>
        <Copy className="h-3.5 w-3.5 mr-2" />
        {t('tabbar.copy_title')}
      </ContextMenuItem>
      {/* Send to Background — only for local terminal tabs */}
      {tab.type === 'local_terminal' && tab.sessionId && (
        <>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => onDetachTab(tab.id, tab.sessionId!)}>
            <CirclePause className="h-3.5 w-3.5 mr-2" />
            {t('tabbar.send_to_background')}
          </ContextMenuItem>
        </>
      )}
      {pluginContextMenuItems.length > 0 && (
        <>
          <ContextMenuSeparator />
          {pluginContextMenuItems.map((item) => {
            const Icon = item.icon ? resolvePluginIcon(item.icon) : null;
            return (
              <ContextMenuItem key={item.key} onSelect={item.handler}>
                {Icon && <Icon className="h-3.5 w-3.5 mr-2" />}
                {item.label}
              </ContextMenuItem>
            );
          })}
        </>
      )}
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void onCloseTab(null, tab.id, tab.sessionId, tab.type)}>
        {t('tabbar.close_tab')}
      </ContextMenuItem>
      <ContextMenuItem
        onSelect={() => onCloseOtherTabs(tab.id)}
        disabled={tabCount <= 1}
      >
        {t('tabbar.close_other_tabs')}
      </ContextMenuItem>
      <ContextMenuItem
        onSelect={() => onCloseTabsToRight(tabIndex)}
        disabled={tabIndex >= tabCount - 1}
      >
        {t('tabbar.close_tabs_to_right')}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => onCloseAllTabs()}>
        {t('tabbar.close_all_tabs')}
      </ContextMenuItem>
    </ContextMenuContent>
    </ContextMenu>
  );
});
TabItem.displayName = 'TabItem';

const ReconnectIndicator = ({
  job,
  nodeId,
  onCancel,
  t,
}: {
  job: ReconnectJob;
  nodeId: string;
  onCancel: (e: React.MouseEvent, nodeId: string) => void;
  t: (key: string) => string;
}) => {
  const [showTimeline, setShowTimeline] = React.useState(false);
  const containerRef = React.useRef<HTMLDivElement>(null);

  // Per-indicator countdown tick — only this component re-renders every second
  const [, setTick] = React.useState(0);
  React.useEffect(() => {
    if (job.status === 'done' || job.status === 'failed' || job.status === 'cancelled') return;
    const interval = setInterval(() => setTick(t => t + 1), 1000);
    return () => clearInterval(interval);
  }, [job.status]);

  return (
    <div
      ref={containerRef}
      className="relative flex items-center gap-1 text-xs text-amber-400"
      onMouseEnter={() => setShowTimeline(true)}
      onMouseLeave={() => setShowTimeline(false)}
    >
      <RefreshCw className="h-3 w-3 animate-spin" />
      <span>
        {t(`connections.reconnect.phase.${job.status.replace(/-/g, '_')}`)}
        {job.attempt > 1 && ` (${job.attempt}/${job.maxAttempts})`}
      </span>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={(e) => onCancel(e, nodeId)}
            className="hover:bg-theme-bg-hover rounded-md p-0.5"
          >
            <XCircle className="h-3 w-3" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom">{t('tabbar.cancel_reconnect')}</TooltipContent>
      </Tooltip>

      {/* Hover popover with timeline */}
      {showTimeline && (
        <div
          className="absolute top-full right-0 mt-1 z-50 bg-theme-bg-elevated border border-theme-border rounded-md shadow-xl"
          onClick={(e) => e.stopPropagation()}
        >
          <ReconnectTimeline job={job} />
        </div>
      )}
    </div>
  );
};

export const TabBar = () => {
  const { t } = useTranslation();
  // Individual selectors — only re-render when the specific slice changes
  const tabs = useAppStore(s => s.tabs);
  const activeTabId = useAppStore(s => s.activeTabId);
  const networkOnline = useAppStore(s => s.networkOnline);
  // Stable function refs — never change, won't cause re-renders
  const setActiveTab = useAppStore(s => s.setActiveTab);
  const closeTab = useAppStore(s => s.closeTab);
  const closeTerminalSession = useAppStore(s => s.closeTerminalSession);
  const moveTab = useAppStore(s => s.moveTab);
  const orchestratorScheduleReconnect = useReconnectOrchestratorStore((s) => s.scheduleReconnect);
  const orchestratorCancel = useReconnectOrchestratorStore((s) => s.cancel);
  const [closing, setClosing] = React.useState<string | null>(null);
  const { confirm, ConfirmDialog } = useConfirm();

  // ── Pointer-based tab drag reorder (Tauri intercepts HTML5 DnD) ──────────
  const [dragState, setDragState] = useState<{
    tabId: string;
    fromIndex: number;
    startX: number;
    currentX: number;
    tabRects: DOMRect[];
  } | null>(null);
  const tabRefsMap = useRef<Map<string, HTMLDivElement>>(new Map());
  const dropTargetIndex = useRef<number | null>(null);

  // Compute drop target from current pointer X
  const computeDropTarget = useCallback((clientX: number, state: NonNullable<typeof dragState>) => {
    const { tabRects, fromIndex } = state;
    for (let i = 0; i < tabRects.length; i++) {
      const rect = tabRects[i];
      const mid = rect.left + rect.width / 2;
      if (clientX < mid) {
        return i <= fromIndex ? i : i;
      }
    }
    return tabRects.length - 1;
  }, []);

  const handlePointerDown = useCallback((e: React.PointerEvent, tabId: string, index: number) => {
    // Only left button, ignore buttons inside tab (close, reconnect)
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest('button')) return;

    // Capture pointer for tracking
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);

    // Snapshot all tab rects
    const rects: DOMRect[] = [];
    tabs.forEach((tab) => {
      const el = tabRefsMap.current.get(tab.id);
      if (el) rects.push(el.getBoundingClientRect());
      else rects.push(new DOMRect());
    });

    setDragState({
      tabId,
      fromIndex: index,
      startX: e.clientX,
      currentX: e.clientX,
      tabRects: rects,
    });
    dropTargetIndex.current = index;
  }, [tabs]);

  const handlePointerMove = useCallback((e: React.PointerEvent) => {
    if (!dragState) return;
    e.preventDefault();
    const newX = e.clientX;
    const target = computeDropTarget(newX, dragState);
    dropTargetIndex.current = target;
    setDragState(prev => prev ? { ...prev, currentX: newX } : null);
  }, [dragState, computeDropTarget]);

  const handlePointerUp = useCallback(() => {
    if (!dragState) return;
    const toIndex = dropTargetIndex.current;
    if (toIndex !== null && toIndex !== dragState.fromIndex) {
      moveTab(dragState.fromIndex, toIndex);
    }
    setDragState(null);
    dropTargetIndex.current = null;
  }, [dragState, moveTab]);

  // Also reset on pointer cancel
  const handlePointerCancel = useCallback(() => {
    setDragState(null);
    dropTargetIndex.current = null;
  }, []);

  // Is a given tab the current drop target (but not the source)?
  const isDragging = dragState !== null;
  const dragDelta = dragState ? dragState.currentX - dragState.startX : 0;
  const isActuallyDragging = isDragging && Math.abs(dragDelta) > 4;

  // Scroll container ref
  const scrollContainerRef = useRef<HTMLDivElement>(null);

  // Track scroll overflow for fade indicators
  const [scrollOverflow, setScrollOverflow] = useState<{ left: boolean; right: boolean }>({ left: false, right: false });

  const updateScrollOverflow = useCallback(() => {
    const el = scrollContainerRef.current;
    if (!el) return;
    const hasOverflow = el.scrollWidth > el.clientWidth + 1; // +1 for rounding
    setScrollOverflow({
      left: hasOverflow && el.scrollLeft > 2,
      right: hasOverflow && el.scrollLeft + el.clientWidth < el.scrollWidth - 2,
    });
  }, []);

  useEffect(() => {
    const el = scrollContainerRef.current;
    if (!el) return;
    el.addEventListener('scroll', updateScrollOverflow, { passive: true });
    const ro = new ResizeObserver(updateScrollOverflow);
    ro.observe(el);
    updateScrollOverflow();
    return () => {
      el.removeEventListener('scroll', updateScrollOverflow);
      ro.disconnect();
    };
  }, [updateScrollOverflow]);

  // Re-check overflow when tabs change
  useEffect(() => {
    updateScrollOverflow();
  }, [tabs.length, updateScrollOverflow]);

  // Scroll active tab into view when it changes
  useEffect(() => {
    if (!activeTabId) return;
    const el = tabRefsMap.current.get(activeTabId);
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', inline: 'nearest', block: 'nearest' });
    }
  }, [activeTabId]);

  // Handle wheel event - convert vertical scroll to horizontal
  const handleWheel = (e: React.WheelEvent<HTMLDivElement>) => {
    const container = scrollContainerRef.current;
    if (container && e.deltaY !== 0) {
      e.preventDefault();
      container.scrollLeft += e.deltaY;
    }
  };

  const handleReconnect = async (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    // 从 session 获取 connectionId，再通过 topologyResolver 获取 nodeId
    const currentSessions = useAppStore.getState().sessions;
    const session = currentSessions.get(sessionId);
    const connectionId = session?.connectionId;
    const nodeId = connectionId ? topologyResolver.getNodeId(connectionId) : undefined;
    
    if (nodeId) {
      // 委托给 orchestrator
      orchestratorScheduleReconnect(nodeId);
    } else {
      console.warn(`[TabBar] Cannot reconnect session ${sessionId}: no associated tree node`);
    }
  };

  const handleCancelReconnect = (e: React.MouseEvent, nodeId: string) => {
    e.stopPropagation();
    orchestratorCancel(nodeId);
  };

  const isRemoteTerminalTab = useCallback((tabType: string, sessionId?: string) => {
    if (tabType !== 'terminal' || !sessionId) {
      return false;
    }

    const session = useAppStore.getState().sessions.get(sessionId);
    return !!session?.connectionId;
  }, []);

  // 关闭 Tab 时释放后端资源
  const handleCloseTab = async (
    e: React.MouseEvent | null,
    tabId: string,
    sessionId: string | undefined,
    tabType: string,
    options?: { skipConfirm?: boolean },
  ) => {
    e?.stopPropagation();
    let shouldCloseTab = true;

    if (!options?.skipConfirm && isRemoteTerminalTab(tabType, sessionId)) {
      const confirmed = await confirm({
        title: t('tabbar.confirm_close_terminal_title'),
        description: t('tabbar.confirm_close_terminal_desc'),
        variant: 'danger',
      });
      if (!confirmed) {
        return;
      }
    }

    // Handle local terminal tabs
    if (tabType === 'local_terminal' && sessionId) {
      // Check for child processes — offer "Send to Background" if active
      try {
        const hasChildren = await useLocalTerminalStore.getState().checkChildProcesses(sessionId);
        if (hasChildren) {
          const userChoice = await confirm({
            title: t('tabbar.child_process_warning'),
            variant: 'danger',
          });
          if (!userChoice) {
            return;
          }
        }
      } catch {
        // If check fails, proceed with close anyway
      }

      setClosing(sessionId);
      try {
        const { closeTerminal } = useLocalTerminalStore.getState();
        await closeTerminal(sessionId);
      } catch (error) {
        console.error('Failed to close local terminal session:', error);
        shouldCloseTab = false;
      } finally {
        setClosing(null);
      }
      if (shouldCloseTab) {
        closeTab(tabId);
      }
      return;
    }

    // 如果是终端 Tab，尝试调用新的 closeTerminalSession
    if (tabType === 'terminal' && sessionId) {
      setClosing(sessionId);
      try {
        // 检查 session 是否使用新的连接池架构
        const session = useAppStore.getState().sessions.get(sessionId);
        if (session?.connectionId) {
          // 使用新 API 释放终端（会减少连接引用计数）
          await closeTerminalSession(sessionId);
        }

        // 同步到 sessionTreeStore：清理终端映射
        const { terminalNodeMap, closeTerminalForNode } = useSessionTreeStore.getState();
        const nodeId = terminalNodeMap.get(sessionId);
        if (nodeId) {
          await closeTerminalForNode(nodeId, sessionId);
        }
      } catch (error) {
        console.error('Failed to close terminal session:', error);
        shouldCloseTab = false;
      } finally {
        setClosing(null);
      }

      if (shouldCloseTab) {
        closeTab(tabId);
      }
      return;
    }

    closeTab(tabId);
  };

  const handleCloseOtherTabs = async (keepTabId: string) => {
    const tabsToClose = tabs.filter(tab => tab.id !== keepTabId);
    const remoteTerminalCount = tabsToClose.filter((tab) => isRemoteTerminalTab(tab.type, tab.sessionId)).length;
    if (remoteTerminalCount > 0) {
      const confirmed = await confirm({
        title: t('tabbar.confirm_close_other_title'),
        description: t('tabbar.confirm_close_other_desc', { count: tabsToClose.length }),
        variant: 'danger',
      });
      if (!confirmed) {
        return;
      }
    }

    for (const tab of tabsToClose) {
      await handleCloseTab(null, tab.id, tab.sessionId, tab.type, {
        skipConfirm: remoteTerminalCount > 0 && isRemoteTerminalTab(tab.type, tab.sessionId),
      });
    }
  };

  const handleCloseTabsToRight = async (fromIndex: number) => {
    const tabsToClose = tabs.slice(fromIndex + 1);
    const remoteTerminalCount = tabsToClose.filter((tab) => isRemoteTerminalTab(tab.type, tab.sessionId)).length;
    if (remoteTerminalCount > 0) {
      const confirmed = await confirm({
        title: t('tabbar.confirm_close_tabs_to_right_title'),
        description: t('tabbar.confirm_close_tabs_to_right_desc', { count: tabsToClose.length }),
        variant: 'danger',
      });
      if (!confirmed) {
        return;
      }
    }

    for (const tab of tabsToClose) {
      await handleCloseTab(null, tab.id, tab.sessionId, tab.type, {
        skipConfirm: remoteTerminalCount > 0 && isRemoteTerminalTab(tab.type, tab.sessionId),
      });
    }
  };

  const handleCloseAllTabs = async () => {
    const remoteTerminalCount = tabs.filter((tab) => isRemoteTerminalTab(tab.type, tab.sessionId)).length;
    if (remoteTerminalCount > 0) {
      const confirmed = await confirm({
        title: t('tabbar.confirm_close_all_title'),
        description: t('tabbar.confirm_close_all_desc', { count: tabs.length }),
        variant: 'danger',
      });
      if (!confirmed) {
        return;
      }
    }

    for (const tab of [...tabs]) {
      await handleCloseTab(null, tab.id, tab.sessionId, tab.type, {
        skipConfirm: remoteTerminalCount > 0 && isRemoteTerminalTab(tab.type, tab.sessionId),
      });
    }
  };

  const handleDetachTab = async (tabId: string, sessionId: string) => {
    try {
      await useLocalTerminalStore.getState().detachTerminal(sessionId);
      closeTab(tabId);
    } catch (error) {
      console.error('Failed to detach local terminal:', error);
    }
  };

  return (
    // 最外层（限制层）：w-full + overflow-hidden 限制总宽度
    <div
      className="w-full h-9 overflow-hidden bg-theme-bg border-b border-theme-border flex items-center"
    >

      {/* Network status indicator - 固定不滚动 */}
      {!networkOnline && (
        <div className="flex-shrink-0 flex items-center gap-1.5 px-3 h-full border-r border-theme-border bg-amber-900/30 text-amber-400 text-xs">
          <WifiOff className="h-3.5 w-3.5" />
          <span>{t('tabbar.offline')}</span>
        </div>
      )}

      {/* 中间层（滚动层）：flex-1 + min-w-0 强制收缩 + overflow-x-auto 触发滚动 */}
      <div className="relative flex-1 min-w-0 h-full">
        {/* Left fade — more tabs off-screen to the left */}
        {scrollOverflow.left && (
          <div className="absolute left-0 top-0 bottom-0 w-6 z-10 pointer-events-none bg-gradient-to-r from-theme-bg to-transparent" />
        )}
        <div
          ref={scrollContainerRef}
          onWheel={handleWheel}
          className="h-full overflow-x-auto scrollbar-thin scroll-smooth"
        >
        {/* 最内层（渲染层）：inline-flex 让子元素一行排列，不换行 */}
        <div className="inline-flex h-full">
          {tabs.map((tab, tabIndex) => {
            const isBeingDragged = dragState?.tabId === tab.id;
            const currentDropTarget = dropTargetIndex.current;
            const showDropIndicator = isActuallyDragging && currentDropTarget === tabIndex && dragState?.fromIndex !== tabIndex;

            return (
              <TabItem
                key={tab.id}
                tab={tab}
                tabIndex={tabIndex}
                isActive={tab.id === activeTabId}
                isBeingDragged={!!isBeingDragged}
                isActuallyDragging={isActuallyDragging}
                showDropIndicator={!!showDropIndicator}
                closing={closing}
                tabCount={tabs.length}
                tabRefsMap={tabRefsMap}
                onPointerDown={handlePointerDown}
                onPointerMove={handlePointerMove}
                onPointerUp={handlePointerUp}
                onPointerCancel={handlePointerCancel}
                onCloseTab={handleCloseTab}
                onReconnect={handleReconnect}
                onCancelReconnect={handleCancelReconnect}
                onCloseOtherTabs={handleCloseOtherTabs}
                onCloseTabsToRight={handleCloseTabsToRight}
                onCloseAllTabs={handleCloseAllTabs}
                onDetachTab={handleDetachTab}
                onSetActiveTab={setActiveTab}
              />
            );
          })}
        </div>
      </div>
        {/* Right fade — more tabs off-screen to the right */}
        {scrollOverflow.right && (
          <div className="absolute right-0 top-0 bottom-0 w-6 z-10 pointer-events-none bg-gradient-to-l from-theme-bg to-transparent" />
        )}
      </div>

      {/* Right-fixed area: terminal-specific actions (recording, cast) */}
      {(() => {
        const activeTab = tabs.find(tab => tab.id === activeTabId);
        if (activeTab && (activeTab.type === 'terminal' || activeTab.type === 'local_terminal')) {
          return <TabBarTerminalActions activeTab={activeTab} />;
        }
        return null;
      })()}
      {ConfirmDialog}
    </div>
  );
};
