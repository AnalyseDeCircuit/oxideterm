import React, { useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { X, Terminal, FolderOpen, GitFork, RefreshCw, XCircle, WifiOff, Settings, Activity, Network, Plug, Square } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { cn } from '../../lib/utils';
import { Tab, PaneNode } from '../../types';

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
    default:
      return null;
  }
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

// Format time remaining for reconnect
const formatTimeRemaining = (nextRetry: number): string => {
  const remaining = Math.max(0, nextRetry - Date.now());
  const seconds = Math.ceil(remaining / 1000);
  return `${seconds}s`;
};

export const TabBar = () => {
  const { t } = useTranslation();
  const {
    tabs,
    activeTabId,
    setActiveTab,
    closeTab,
    closeTerminalSession,
    reconnect,
    cancelReconnect,
    sessions,
    networkOnline
  } = useAppStore();
  const [reconnecting, setReconnecting] = React.useState<string | null>(null);
  const [closing, setClosing] = React.useState<string | null>(null);
  // Force re-render for countdown
  const [, setTick] = React.useState(0);

  // Scroll container ref
  const scrollContainerRef = useRef<HTMLDivElement>(null);

  // Handle wheel event - convert vertical scroll to horizontal
  const handleWheel = (e: React.WheelEvent<HTMLDivElement>) => {
    const container = scrollContainerRef.current;
    if (container && e.deltaY !== 0) {
      e.preventDefault();
      container.scrollLeft += e.deltaY;
    }
  };

  // Update countdown every second when there are reconnecting sessions
  React.useEffect(() => {
    const hasReconnecting = tabs.some((tab) => {
      if (!tab.sessionId) return false;
      const session = sessions.get(tab.sessionId);
      return session?.state === 'reconnecting' && session.reconnectNextRetry;
    });

    if (!hasReconnecting) return;

    const interval = setInterval(() => {
      setTick((t) => t + 1);
    }, 1000);

    return () => clearInterval(interval);
  }, [tabs, sessions]);

  const handleReconnect = async (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    setReconnecting(sessionId);
    try {
      await reconnect(sessionId);
    } finally {
      setReconnecting(null);
    }
  };

  const handleCancelReconnect = async (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    await cancelReconnect(sessionId);
  };

  // 关闭 Tab 时释放后端资源
  const handleCloseTab = async (e: React.MouseEvent, tabId: string, sessionId: string | undefined, tabType: string) => {
    e.stopPropagation();

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
      <div
        ref={scrollContainerRef}
        onWheel={handleWheel}
        className="flex-1 min-w-0 h-full overflow-x-auto scrollbar-thin"
      >
        {/* 最内层（渲染层）：inline-flex 让子元素一行排列，不换行 */}
        <div className="inline-flex h-full">
          {tabs.map((tab) => {
            const isActive = tab.id === activeTabId;
            const isManualReconnecting = reconnecting === tab.sessionId;
            const session = tab.sessionId ? sessions.get(tab.sessionId) : undefined;
            const isAutoReconnecting = session?.state === 'reconnecting';
            const reconnectAttempt = session?.reconnectAttempt;
            const reconnectMax = session?.reconnectMaxAttempts;
            const reconnectNextRetry = session?.reconnectNextRetry;
            const showReconnectProgress = isAutoReconnecting && reconnectAttempt !== undefined;

            return (
              // 每个 Tab 必须 flex-shrink-0，防止被挤压
              <div
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={cn(
                  "flex-shrink-0 group flex items-center gap-2 px-3 h-full min-w-[120px] max-w-[240px] border-r border-theme-border cursor-pointer select-none text-sm transition-colors",
                  isActive
                    ? "bg-theme-bg-panel text-theme-text border-t-2 border-t-theme-accent"
                    : "bg-theme-bg text-theme-text-muted hover:bg-theme-bg-hover border-t-2 border-t-transparent",
                  showReconnectProgress && "border-t-amber-500"
                )}
              >
                <TabIcon type={tab.type} />
                <span className="truncate flex-1">{getTabTitle(tab, sessions, t)}</span>

                {/* Reconnect progress indicator */}
                {showReconnectProgress && (
                  <div className="flex items-center gap-1 text-xs text-amber-400">
                    <RefreshCw className="h-3 w-3 animate-spin" />
                    <span>
                      {reconnectAttempt}/{reconnectMax}
                      {reconnectNextRetry && ` (${formatTimeRemaining(reconnectNextRetry)})`}
                    </span>
                    <button
                      onClick={(e) => tab.sessionId && handleCancelReconnect(e, tab.sessionId)}
                      className="hover:bg-theme-bg-hover rounded p-0.5"
                      title={t('tabbar.cancel_reconnect')}
                    >
                      <XCircle className="h-3 w-3" />
                    </button>
                  </div>
                )}

                {/* Normal tab controls */}
                {!showReconnectProgress && (
                  <div className="flex items-center gap-0.5">
                    {/* Refresh button for terminal tabs */}
                    {tab.type === 'terminal' && (
                      <button
                        onClick={(e) => tab.sessionId && handleReconnect(e, tab.sessionId)}
                        disabled={isManualReconnecting}
                        className={cn(
                          "opacity-0 group-hover:opacity-100 hover:bg-theme-bg-hover rounded p-0.5 transition-opacity",
                          isActive && "opacity-100",
                          isManualReconnecting && "opacity-100"
                        )}
                        title={t('tabbar.reconnect')}
                      >
                        <RefreshCw className={cn("h-3 w-3", isManualReconnecting && "animate-spin")} />
                      </button>
                    )}
                    <button
                      onClick={(e) => handleCloseTab(e, tab.id, tab.sessionId, tab.type)}
                      disabled={tab.sessionId ? closing === tab.sessionId : false}
                      className={cn(
                        "opacity-0 group-hover:opacity-100 hover:bg-theme-bg-hover rounded p-0.5 transition-opacity",
                        isActive && "opacity-100",
                        (tab.sessionId && closing === tab.sessionId) && "opacity-100"
                      )}
                      title={t('tabbar.close_tab')}
                    >
                      <X className="h-3 w-3" />
                    </button>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
};
