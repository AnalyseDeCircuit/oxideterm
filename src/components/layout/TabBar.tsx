import React, { useRef, useCallback, useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { X, Terminal, FolderOpen, GitFork, RefreshCw, XCircle, WifiOff, Settings, Activity, Network, Plug, Square, HardDrive, LayoutList, Puzzle, Monitor } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useReconnectOrchestratorStore } from '../../store/reconnectOrchestratorStore';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { cn } from '../../lib/utils';
import { Tab, PaneNode } from '../../types';
import { topologyResolver } from '../../lib/topologyResolver';
import { resolvePluginIcon } from '../../lib/plugin/pluginIconResolver';
import { ReconnectTimeline } from '../connections/ReconnectTimeline';
import { TabBarTerminalActions } from './TabBarTerminalActions';
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger } from '../ui/context-menu';
import { Tooltip, TooltipTrigger, TooltipContent } from '../ui/tooltip';

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
      colorClass = 'bg-zinc-500';
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
            className="hover:bg-theme-bg-hover rounded p-0.5"
          >
            <XCircle className="h-3 w-3" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom">{t('tabbar.cancel_reconnect')}</TooltipContent>
      </Tooltip>

      {/* Hover popover with timeline */}
      {showTimeline && (
        <div
          className="absolute top-full right-0 mt-1 z-50 bg-theme-bg-panel border border-theme-border rounded-lg shadow-xl"
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
  const {
    tabs,
    activeTabId,
    setActiveTab,
    closeTab,
    closeTerminalSession,
    moveTab,
    sessions,
    connections,
    networkOnline
  } = useAppStore();
  const orchestratorGetJob = useReconnectOrchestratorStore((s) => s.getJob);
  const orchestratorScheduleReconnect = useReconnectOrchestratorStore((s) => s.scheduleReconnect);
  const orchestratorCancel = useReconnectOrchestratorStore((s) => s.cancel);
  const [closing, setClosing] = React.useState<string | null>(null);

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
    const session = sessions.get(sessionId);
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

  // 关闭 Tab 时释放后端资源
  const handleCloseTab = async (e: React.MouseEvent | null, tabId: string, sessionId: string | undefined, tabType: string) => {
    e?.stopPropagation();

    // Handle local terminal tabs
    if (tabType === 'local_terminal' && sessionId) {
      setClosing(sessionId);
      try {
        const { closeTerminal } = useLocalTerminalStore.getState();
        await closeTerminal(sessionId);
      } catch (error) {
        console.error('Failed to close local terminal session:', error);
      } finally {
        setClosing(null);
      }
      closeTab(tabId);
      return;
    }

    // 如果是终端 Tab，尝试调用新的 closeTerminalSession
    if (tabType === 'terminal' && sessionId) {
      setClosing(sessionId);
      try {
        // 检查 session 是否使用新的连接池架构
        const session = sessions.get(sessionId);
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
      } finally {
        setClosing(null);
      }
    }

    // 总是移除 Tab（即使后端调用失败）
    closeTab(tabId);
  };

  const handleCloseOtherTabs = async (keepTabId: string) => {
    const tabsToClose = tabs.filter(tab => tab.id !== keepTabId);
    for (const tab of tabsToClose) {
      await handleCloseTab(null, tab.id, tab.sessionId, tab.type);
    }
  };

  const handleCloseTabsToRight = async (fromIndex: number) => {
    const tabsToClose = tabs.slice(fromIndex + 1);
    for (const tab of tabsToClose) {
      await handleCloseTab(null, tab.id, tab.sessionId, tab.type);
    }
  };

  const handleCloseAllTabs = async () => {
    for (const tab of [...tabs]) {
      await handleCloseTab(null, tab.id, tab.sessionId, tab.type);
    }
  };

  return (
    // 最外层（限制层）：w-full + overflow-hidden 限制总宽度
    <div className="w-full h-9 overflow-hidden bg-theme-bg border-b border-theme-border flex items-center">
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
          className="h-full overflow-x-auto scrollbar-thin"
        >
        {/* 最内层（渲染层）：inline-flex 让子元素一行排列，不换行 */}
        <div className="inline-flex h-full">
          {tabs.map((tab) => {
            const isActive = tab.id === activeTabId;
            const session = tab.sessionId ? sessions.get(tab.sessionId) : undefined;
            
            // Look up orchestrator job for this tab's node
            const connectionId = session?.connectionId;
            const connectionState = connectionId ? connections.get(connectionId)?.state : undefined;
            const nodeId = connectionId ? topologyResolver.getNodeId(connectionId) : undefined;
            const orchJob = nodeId ? orchestratorGetJob(nodeId) : undefined;
            const isOrchestratorActive = orchJob && orchJob.status !== 'done' && orchJob.status !== 'failed' && orchJob.status !== 'cancelled';
            const isManualReconnecting = !!isOrchestratorActive;
            const showReconnectProgress = !!isOrchestratorActive;

            const tabIndex = tabs.indexOf(tab);
            const isBeingDragged = dragState?.tabId === tab.id;
            const currentDropTarget = dropTargetIndex.current;
            const showDropIndicator = isActuallyDragging && currentDropTarget === tabIndex && dragState?.fromIndex !== tabIndex;

            return (
              // 每个 Tab 必须 flex-shrink-0，防止被挤压
              <ContextMenu key={tab.id}>
              <ContextMenuTrigger asChild>
              <div
                ref={(el) => {
                  if (el) tabRefsMap.current.set(tab.id, el);
                  else tabRefsMap.current.delete(tab.id);
                }}
                onPointerDown={(e) => handlePointerDown(e, tab.id, tabIndex)}
                onPointerMove={handlePointerMove}
                onPointerUp={handlePointerUp}
                onPointerCancel={handlePointerCancel}
                onMouseDown={(e) => {
                  if (e.button === 1) {
                    e.preventDefault();
                    handleCloseTab(e, tab.id, tab.sessionId, tab.type);
                  }
                }}
                onClick={() => {
                  if (!isActuallyDragging) setActiveTab(tab.id);
                }}
                className={cn(
                  "flex-shrink-0 group flex items-center gap-2 px-3 h-full min-w-[120px] max-w-[240px] border-r border-theme-border cursor-pointer select-none text-sm transition-colors",
                  isActive
                    ? "bg-theme-bg-panel text-theme-text border-t-2 border-t-theme-accent"
                    : "bg-theme-bg text-theme-text-muted hover:bg-theme-bg-hover border-t-2 border-t-transparent",
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
                    onCancel={handleCancelReconnect}
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
                            onClick={(e) => tab.sessionId && handleReconnect(e, tab.sessionId)}
                            disabled={isManualReconnecting}
                            className={cn(
                              "opacity-0 group-hover:opacity-100 hover:bg-theme-bg-hover rounded p-0.5 transition-opacity",
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
                          onClick={(e) => handleCloseTab(e, tab.id, tab.sessionId, tab.type)}
                          disabled={tab.sessionId ? closing === tab.sessionId : false}
                          className={cn(
                            "opacity-0 group-hover:opacity-100 hover:bg-theme-bg-hover rounded p-0.5 transition-opacity",
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
                <ContextMenuItem onSelect={() => handleCloseTab(null, tab.id, tab.sessionId, tab.type)}>
                  {t('tabbar.close_tab')}
                </ContextMenuItem>
                <ContextMenuItem
                  onSelect={() => handleCloseOtherTabs(tab.id)}
                  disabled={tabs.length <= 1}
                >
                  {t('tabbar.close_other_tabs')}
                </ContextMenuItem>
                <ContextMenuItem
                  onSelect={() => handleCloseTabsToRight(tabIndex)}
                  disabled={tabIndex >= tabs.length - 1}
                >
                  {t('tabbar.close_tabs_to_right')}
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuItem onSelect={() => handleCloseAllTabs()}>
                  {t('tabbar.close_all_tabs')}
                </ContextMenuItem>
              </ContextMenuContent>
              </ContextMenu>
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
    </div>
  );
};
