// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Panel, Group as PanelGroup } from 'react-resizable-panels';
import { Sidebar } from './Sidebar';
import { AiSidebar } from './AiSidebar';
import { ErrorBoundary } from '../ErrorBoundary';
import { ActivityPanel } from './ActivityPanel';
import { TabBar } from './TabBar';
import { useAppStore, getSession } from '../../store/appStore';
import { TerminalView } from '../terminal/TerminalView';
import { LocalTerminalView } from '../terminal/LocalTerminalView';
import { SplitTerminalContainer } from '../terminal/SplitTerminalContainer';
import { Button } from '../ui/button';
import { NewConnectionModal } from '../modals/NewConnectionModal';
import { ConnectionPoolMonitor } from '../connections/ConnectionPoolMonitor';
import { TabActiveProvider } from '../../hooks/useTabActive';
import { ConnectionsPanel } from '../connections/ConnectionsPanel';
import { SystemHealthPanel } from './SystemHealthPanel';
import { PluginStatusBar } from '../plugin/PluginStatusBar';
import { PluginTargetContextMenu } from '../plugin/PluginTargetContextMenu';
import { Plus, Terminal as TerminalIcon } from 'lucide-react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useSettingsStore } from '../../store/settingsStore';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { useActivityStore } from '../../store/activityStore';

import { useTabBgActive } from '../../hooks/useTabBackground';
import { platform } from '../../lib/platform';

// Lazy load non-critical views (only loaded when user opens SFTP/Forwards tab)
const SettingsView = lazy(() => import('../settings/SettingsView').then(m => ({ default: m.SettingsView })));
const TopologyPage = lazy(() => import('../topology/TopologyPage').then(m => ({ default: m.TopologyPage })));
const SFTPView = lazy(() => import('../sftp/SFTPView').then(m => ({ default: m.SFTPView })));
const ForwardsView = lazy(() => import('../forwards/ForwardsView').then(m => ({ default: m.ForwardsView })));
const IdeWorkspace = lazy(() => import('../ide').then(m => ({ default: m.IdeWorkspace })));
const LocalFileManager = lazy(() => import('../fileManager').then(m => ({ default: m.LocalFileManager })));
const SessionManagerPanel = lazy(() => import('../sessionManager').then(m => ({ default: m.SessionManagerPanel })));
const PluginTabRenderer = lazy(() => import('../plugin/PluginTabRenderer').then(m => ({ default: m.PluginTabRenderer })));
const PluginManagerView = lazy(() => import('../plugin/PluginManagerView').then(m => ({ default: m.PluginManagerView })));
const GraphicsView = lazy(() => import('../graphics/GraphicsView').then(m => ({ default: m.GraphicsView })));
const LauncherView = lazy(() => import('../launcher/LauncherView').then(m => ({ default: m.LauncherView })));
const AgentPanel = lazy(() => import('../ai/AgentPanel').then(m => ({ default: m.AgentPanel })));

// Loading fallback for lazy components
const ViewLoader = () => {
  const { t } = useTranslation();
  return (
    <div className="flex items-center justify-center h-full text-theme-text-muted">
      <div className="animate-pulse">{t('layout.loading')}</div>
    </div>
  );
};

// Shown for legacy sftp/ide tabs that have no nodeId binding
const StaleTabBanner = ({ type }: { type: string }) => {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col items-center justify-center h-full text-theme-text-muted gap-3">
      <div className="text-4xl opacity-20">{type === 'sftp' ? '📁' : '💻'}</div>
      <p className="text-sm">{t('layout.stale_tab')}</p>
    </div>
  );
};

// Background image wrapper for non-terminal tabs
const TabBgWrapper: React.FC<{ tabType: string; children: React.ReactNode }> = ({ tabType, children }) => {
  const terminal = useSettingsStore(s => s.settings.terminal);
  const enabledTabs = terminal.backgroundEnabledTabs ?? ['terminal', 'local_terminal'];
  const active = terminal.backgroundEnabled !== false && !!terminal.backgroundImage && enabledTabs.includes(tabType);

  const bgUrl = useMemo(
    () => active && terminal.backgroundImage ? convertFileSrc(terminal.backgroundImage) : null,
    [active, terminal.backgroundImage]
  );

  if (!bgUrl) return <>{children}</>;

  const fit = terminal.backgroundFit || 'cover';

  return (
    <div className="relative h-full w-full">
      <div
        className="absolute inset-0 z-0 pointer-events-none"
        style={{
          backgroundImage: `url(${bgUrl})`,
          backgroundSize: fit === 'tile' ? 'auto' : fit,
          backgroundRepeat: fit === 'tile' ? 'repeat' : 'no-repeat',
          backgroundPosition: 'center',
          opacity: terminal.backgroundOpacity ?? 0.15,
          filter: terminal.backgroundBlur ? `blur(${terminal.backgroundBlur}px)` : undefined,
        }}
      />
      <div className="relative z-[1] h-full w-full">{children}</div>
    </div>
  );
};

// Empty state with quick actions and shortcut hints
const EmptyState = () => {
  const { t } = useTranslation();
  const { toggleModal, createTab } = useAppStore();
  const createLocalTerminal = useLocalTerminalStore((s) => s.createTerminal);
  const isMac = platform.isMac;

  const handleNewLocalTerminal = async () => {
    try {
      const info = await createLocalTerminal();
      createTab('local_terminal', info.id);
    } catch (err) {
      console.error('Failed to create local terminal:', err);
    }
  };

  const shortcuts = [
    { key: isMac ? '⌘K' : 'Ctrl+K', label: t('command_palette.title') },
    { key: isMac ? '⌘N' : 'Ctrl+N', label: t('layout.empty.new_connection') },
    { key: isMac ? '⌘T' : 'Ctrl+T', label: t('layout.empty.new_local_terminal') },
    { key: isMac ? '⌘/' : 'Ctrl+/', label: t('layout.empty.keyboard_shortcuts') },
  ];

  return (
    <div className="flex flex-col items-center justify-center h-full text-theme-text-muted px-4">
      <div className="w-full max-w-sm space-y-6">
        {/* Header */}
        <div className="text-center select-none">
          <h1 className="text-5xl font-bold tracking-tight text-theme-text empty-brand">
            {t('layout.empty.title')}
            <span className="inline-block w-[3px] h-[0.7em] ml-1.5 translate-y-[0.05em] rounded-sm bg-theme-accent animate-caret" />
          </h1>
        </div>

        {/* Quick Actions */}
        <div className="flex gap-3 justify-center">
          <Button onClick={() => toggleModal('newConnection', true)} className="gap-2">
            <Plus className="h-4 w-4" /> {t('layout.empty.new_connection')}
          </Button>
          <Button variant="outline" onClick={handleNewLocalTerminal} className="gap-2">
            <TerminalIcon className="h-4 w-4" /> {t('layout.empty.new_local_terminal')}
          </Button>
        </div>

        {/* Keyboard shortcut hints */}
        <div className="flex flex-wrap justify-center gap-x-5 gap-y-2 pt-1">
          {shortcuts.map(s => (
            <span key={s.key} className="inline-flex items-center gap-1.5 text-xs text-theme-text-muted">
              <kbd className="px-1.5 py-0.5 rounded-md bg-theme-bg-panel border border-theme-border text-theme-text font-mono text-[11px] leading-tight shadow-sm">{s.key}</kbd>
              <span>{s.label}</span>
            </span>
          ))}
        </div>
      </div>
    </div>
  );
};

export const AppLayout = () => {
  const { t } = useTranslation();
  const { tabs, activeTabId, setActivePaneId, closePane } = useAppStore();
  const monitorBgActive = useTabBgActive('connection_monitor');
  const zenMode = useSettingsStore((s) => s.settings.sidebarUI.zenMode);


  // Zen mode hint — show briefly on enter
  const [showZenHint, setShowZenHint] = useState(false);
  useEffect(() => {
    if (zenMode) {
      setShowZenHint(true);
      const timer = setTimeout(() => setShowZenHint(false), 2500);
      return () => clearTimeout(timer);
    } else {
      setShowZenHint(false);
    }
  }, [zenMode]);

  // After zen mode toggled, TabBar/Sidebar appear/disappear changing the terminal
  // container dimensions. Dispatch a resize event after DOM reflow so that
  // xterm.js FitAddon recalculates cols/rows correctly.
  useEffect(() => {
    const raf = requestAnimationFrame(() => {
      window.dispatchEvent(new Event('resize'));
    });
    return () => cancelAnimationFrame(raf);
  }, [zenMode]);

  const isMac = platform.isMac;

  // Handlers for split pane interactions
  const handlePaneFocus = useCallback((tabId: string, paneId: string) => {
    setActivePaneId(tabId, paneId);
  }, [setActivePaneId]);

  const handlePaneClose = useCallback((tabId: string, paneId: string) => {
    closePane(tabId, paneId);
  }, [closePane]);

  useEffect(() => {
    const legacyTabs = tabs.filter((tab) => tab.type === 'notifications' || tab.type === 'event_log');
    if (legacyTabs.length === 0) {
      return;
    }

    const activeLegacyTab = legacyTabs.find((tab) => tab.id === activeTabId) ?? legacyTabs[0];
    useActivityStore.getState().setActiveView(activeLegacyTab.type === 'event_log' ? 'event_log' : 'notifications');

    useAppStore.setState((state) => {
      const currentLegacyTabs = state.tabs.filter((tab) => tab.type === 'notifications' || tab.type === 'event_log');
      if (currentLegacyTabs.length === 0) {
        return state;
      }

      const nonLegacyTabs = state.tabs.filter((tab) => tab.type !== 'notifications' && tab.type !== 'event_log');
      const existingActivityTab = nonLegacyTabs.find((tab) => tab.type === 'activity');
      const wasLegacyTabActive = !!state.activeTabId && currentLegacyTabs.some((tab) => tab.id === state.activeTabId);

      if (existingActivityTab) {
        return {
          ...state,
          tabs: nonLegacyTabs,
          activeTabId: wasLegacyTabActive ? existingActivityTab.id : state.activeTabId,
        };
      }

      const sourceTab = currentLegacyTabs.find((tab) => tab.id === state.activeTabId) ?? currentLegacyTabs[0];
      const activityTab = {
        ...sourceTab,
        type: 'activity' as const,
        title: t('tabs.activity'),
        icon: '🔔',
      };

      return {
        ...state,
        tabs: [...nonLegacyTabs, activityTab],
        activeTabId: wasLegacyTabActive ? activityTab.id : state.activeTabId,
      };
    });
  }, [activeTabId, tabs, t]);



  return (
    <div className="flex flex-col h-screen w-screen bg-theme-bg text-theme-text overflow-hidden">
      {/* Modals */}
      <NewConnectionModal />

      {/* Main horizontal layout */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
      {/* Sidebar — hidden in zen mode */}
      {!zenMode && <Sidebar />}

      {/* Main Content */}
      <div className="flex-1 flex flex-col min-w-0 min-h-0">
        {!zenMode && <TabBar />}

        <PanelGroup orientation="vertical" className="flex-1">
          <Panel minSize={30}>
            <div className="h-full relative bg-theme-bg overflow-hidden">
          {tabs.length === 0 ? (
            <EmptyState />
          ) : (
            <>
              {tabs.map(tab => {
                const isActive = tab.id === activeTabId;
                return (
                <div
                  key={tab.id}
                  className={`absolute inset-0 ${isActive ? 'z-10 block' : 'z-0 hidden'}`}
                >
                <TabActiveProvider value={isActive}>
                  {/* Terminal tabs: Support split panes via rootPane, fallback to single terminal */}
                  {(tab.type === 'terminal' || tab.type === 'local_terminal') && (
                    <PluginTargetContextMenu target="terminal">
                      <div className="relative h-full w-full group/terminal">
                        {tab.rootPane ? (
                          // Split pane mode - use recursive container
                          <SplitTerminalContainer
                            key={`split-${tab.id}`}
                            tabId={tab.id}
                            rootPane={tab.rootPane}
                            activePaneId={tab.activePaneId}
                            onPaneFocus={(paneId) => handlePaneFocus(tab.id, paneId)}
                            onPaneClose={(paneId) => handlePaneClose(tab.id, paneId)}
                          />
                        ) : (
                          // Legacy single pane mode (backward compatible)
                          // Key includes ws_url to force remount when backend assigns new port
                          tab.sessionId && (
                            tab.type === 'terminal'
                              ? <TerminalView key={`${tab.sessionId}-${getSession(tab.sessionId)?.ws_url ?? ''}`} sessionId={tab.sessionId} tabId={tab.id} isActive={tab.id === activeTabId} />
                              : <LocalTerminalView key={tab.sessionId} sessionId={tab.sessionId} tabId={tab.id} isActive={tab.id === activeTabId} />
                          )
                        )}
                      </div>
                    </PluginTargetContextMenu>
                  )}
                  {tab.type === 'sftp' && (
                    <TabBgWrapper tabType="sftp">
                      {tab.nodeId ? (
                        <Suspense fallback={<ViewLoader />}>
                          <SFTPView 
                            key={`sftp-${tab.nodeId}`}
                            nodeId={tab.nodeId}
                          />
                        </Suspense>
                      ) : (
                        <StaleTabBanner type="sftp" />
                      )}
                    </TabBgWrapper>
                  )}
                  {tab.type === 'forwards' && tab.nodeId && (
                    <TabBgWrapper tabType="forwards">
                      <Suspense fallback={<ViewLoader />}>
                        <ForwardsView 
                          key={`forwards-${tab.nodeId}`} 
                          nodeId={tab.nodeId} 
                        />
                      </Suspense>
                    </TabBgWrapper>
                  )}
                  {tab.type === 'settings' && (
                    <TabBgWrapper tabType="settings">
                      <SettingsView />
                    </TabBgWrapper>
                  )}
                  {tab.type === 'connection_monitor' && (
                    <TabBgWrapper tabType="connection_monitor">
                      <div className={`h-full w-full p-8 overflow-auto ${monitorBgActive ? '' : 'bg-theme-bg'}`} data-bg-active={monitorBgActive || undefined}>
                        <div className="max-w-5xl mx-auto space-y-8">
                          <div>
                            <h2 className="text-2xl font-bold mb-6 text-theme-text">{t('layout.connection_monitor.title')}</h2>
                            <ConnectionPoolMonitor />
                          </div>
                          <div>
                            <h2 className="text-xl font-bold mb-4 text-theme-text">{t('sidebar.panels.system_health')}</h2>
                            <SystemHealthPanel />
                          </div>
                        </div>
                      </div>
                    </TabBgWrapper>
                  )}
                  {tab.type === 'connection_pool' && (
                    <TabBgWrapper tabType="connection_pool">
                      <ConnectionsPanel />
                    </TabBgWrapper>
                  )}
                  {tab.type === 'topology' && (
                    <TabBgWrapper tabType="topology">
                      <TopologyPage />
                    </TabBgWrapper>
                  )}
                  {tab.type === 'ide' && (
                    <TabBgWrapper tabType="ide">
                      {tab.nodeId ? (
                        <Suspense fallback={<ViewLoader />}>
                          <IdeWorkspace
                            key={`ide-${tab.nodeId}`}
                            nodeId={tab.nodeId}
                            rootPath="~"
                          />
                        </Suspense>
                      ) : (
                        <StaleTabBanner type="ide" />
                      )}
                    </TabBgWrapper>
                  )}
                  {tab.type === 'file_manager' && (
                    <TabBgWrapper tabType="file_manager">
                      <Suspense fallback={<ViewLoader />}>
                        <LocalFileManager />
                      </Suspense>
                    </TabBgWrapper>
                  )}
                  {tab.type === 'session_manager' && (
                    <TabBgWrapper tabType="session_manager">
                      <Suspense fallback={<ViewLoader />}>
                        <SessionManagerPanel />
                      </Suspense>
                    </TabBgWrapper>
                  )}
                  {tab.type === 'plugin' && tab.pluginTabId && (
                    <Suspense fallback={<ViewLoader />}>
                      <PluginTabRenderer pluginTabId={tab.pluginTabId} tab={tab} />
                    </Suspense>
                  )}
                  {tab.type === 'plugin_manager' && (
                    <TabBgWrapper tabType="plugin_manager">
                      <Suspense fallback={<ViewLoader />}>
                        <PluginManagerView />
                      </Suspense>
                    </TabBgWrapper>
                  )}
                  {tab.type === 'graphics' && (
                    <Suspense fallback={<ViewLoader />}>
                      <GraphicsView />
                    </Suspense>
                  )}
                  {tab.type === 'launcher' && (
                    <TabBgWrapper tabType="launcher">
                      <Suspense fallback={<ViewLoader />}>
                        <LauncherView />
                      </Suspense>
                    </TabBgWrapper>
                  )}
                  {tab.type === 'ai_agent' && (
                    <TabBgWrapper tabType="ai_agent">
                      <ErrorBoundary>
                        <Suspense fallback={<ViewLoader />}>
                          <AgentPanel />
                        </Suspense>
                      </ErrorBoundary>
                    </TabBgWrapper>
                  )}
                  {tab.type === 'activity' && (
                    <TabBgWrapper tabType="activity">
                      <ActivityPanel />
                    </TabBgWrapper>
                  )}
                </TabActiveProvider>
                </div>
                );
              })}
            </>
          )}
            </div>
          </Panel>


        </PanelGroup>
        <PluginStatusBar />
      </div>

      {/* AI Sidebar - Right side (hidden in zen mode) */}
      {!zenMode && <AiSidebar />}
      </div>

      {/* Zen mode hint overlay */}
      {showZenHint && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 px-4 py-2 rounded-md bg-theme-bg-elevated/90 border border-theme-border text-sm text-theme-text-muted backdrop-blur-sm animate-in fade-in slide-in-from-bottom-2 duration-300">
          {isMac ? t('zen_mode.hint') : t('zen_mode.hint_other')}
        </div>
      )}
    </div>
  );
};
