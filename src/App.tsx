// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useEffect, useState, useCallback, useMemo } from 'react';
import { AppLayout } from './components/layout/AppLayout';
import { Toaster } from './components/ui/toaster';
import { UpdateNotification } from './components/layout/UpdateNotification';
import { AutoRouteModal } from './components/modals/AutoRouteModal';
import { LocalShellLauncher } from './components/local/LocalShellLauncher';
import { ErrorBoundary } from './components/ErrorBoundary';
import { useNetworkStatus } from './hooks/useNetworkStatus';
import { useConnectionEvents } from './hooks/useConnectionEvents';
import { useCliEvents } from './hooks/useCliEvents';
import { useEventLogCapture } from './hooks/useEventLogCapture';
import { setupTreeStoreSubscriptions, cleanupTreeStoreSubscriptions } from './store/sessionTreeStore';
import { useLocalTerminalStore } from './store/localTerminalStore';
import { useAppStore } from './store/appStore';
import { useSettingsStore } from './store/settingsStore';
import { useSplitPaneActions } from './hooks/useSplitPaneShortcuts';
import { useKeybindingDispatcher } from './hooks/useKeybindingDispatcher';
import type { ActionId } from './lib/keybindingRegistry';
import { preloadTerminalFonts } from './lib/fontLoader';
import { initializePluginSystem } from './lib/plugin/pluginLoader';
import { setupConnectionBridge, setupNodeStateBridge, setupTransferBridge, pluginEventBridge } from './lib/plugin/pluginEventBridge';
import { useToastStore } from './hooks/useToast';
import { useUpdateStore } from './store/updateStore';
import { PluginConfirmDialog } from './components/plugin/PluginConfirmDialog';
import { PluginProgressHost } from './components/plugin/PluginProgressHost';
import { CommandPalette } from './components/command-palette/CommandPalette';
import { CastPlayer } from './components/terminal/CastPlayer';
import { OnboardingModal } from './components/modals/OnboardingModal';
import { KeyboardShortcutsModal } from './components/modals/KeyboardShortcutsModal';
import { GlobalKbiDialog } from './components/modals/GlobalKbiDialog';
import { TooltipProvider } from './components/ui/tooltip';
import { useFontSizeHUD } from './components/ui/FontSizeHUD';
import { useRecordingStore } from './store/recordingStore';
import { useCommandPaletteStore } from './store/commandPaletteStore';
import { useBroadcastStore } from './store/broadcastStore';
import { useActivityStore } from './store/activityStore';
import { initNotificationListener, teardownNotificationListener } from './store/notificationCenterStore';

function App() {
  // Initialize global event listeners
  // useReconnectEvents 已废弃，由 useConnectionEvents 统一处理连接事件
  useNetworkStatus();
  useConnectionEvents();
  useCliEvents();
  useEventLogCapture();
  
  // Recording player modal state
  const playerModal = useRecordingStore((s) => s.playerModal);
  const closePlayer = useRecordingStore((s) => s.closePlayer);

  // Shell launcher state
  const [shellLauncherOpen, setShellLauncherOpen] = useState(false);
  const { createTerminal } = useLocalTerminalStore();
  const createTab = useAppStore(s => s.createTab);
  
  // Command palette state (lifted to store so other components can trigger it)
  const commandPaletteOpen = useCommandPaletteStore((s) => s.open);
  const setCommandPaletteOpen = useCommandPaletteStore((s) => s.setOpen);

  // Keyboard shortcuts modal state
  const [shortcutsModalOpen, setShortcutsModalOpen] = useState(false);

  // Font size HUD
  const { showFontSize, FontSizeHUD } = useFontSizeHUD();

  // Determine if a terminal is currently active (inline selector avoids subscribing to full tabs array)
  const isTerminalActive = useAppStore(s => {
    if (!s.activeTabId) return false;
    const tab = s.tabs.find(t => t.id === s.activeTabId);
    return tab?.type === 'terminal' || tab?.type === 'local_terminal';
  });

  // Preload fonts based on user settings (lazy load CJK font)
  // Delayed 500ms to let Tauri window and PTY initialize first
  useEffect(() => {
    const timer = setTimeout(() => {
      const { settings } = useSettingsStore.getState();
      preloadTerminalFonts(settings.terminal.fontFamily);
    }, 500);
    
    return () => clearTimeout(timer);
  }, []);

  // Initialize plugin system (discover + load enabled plugins)
  useEffect(() => {
    const bridgeCleanup = setupConnectionBridge(useAppStore);
    const transferBridgeCleanup = setupTransferBridge();
    let nodeStateBridgeCleanup: (() => void) | undefined;
    let nodeStateBridgeResolved = false;

    const bridgePromise = setupNodeStateBridge().then(cleanup => {
      nodeStateBridgeCleanup = cleanup;
      nodeStateBridgeResolved = true;
    });

    // Wire plugin toast events → app toast system
    const toastCleanup = pluginEventBridge.on('plugin:toast', (data) => {
      const opts = data as { title?: string; message?: string; description?: string; variant?: string; duration?: number };
      useToastStore.getState().addToast({
        title: opts.title ?? opts.message ?? 'Plugin',
        description: opts.description,
        variant: (opts.variant as 'success' | 'error' | 'warning' | 'default') ?? 'default',
        duration: opts.duration,
      });
    });

    initializePluginSystem().catch(err => {
      console.error('Failed to initialize plugin system:', err);
    });
    initNotificationListener().catch(err => {
      console.error('Failed to initialize notification listener:', err);
    });
    return () => {
      bridgeCleanup();
      transferBridgeCleanup();
      if (nodeStateBridgeResolved) {
        nodeStateBridgeCleanup?.();
      } else {
        // setupNodeStateBridge() 尚未 resolve，等待完成后再清理
        bridgePromise.then(() => nodeStateBridgeCleanup?.());
      }
      toastCleanup();
      teardownNotificationListener();
    };
  }, []);

  // Load agent task history from backend persistence
  useEffect(() => {
    (async () => {
      // Auto-connect enabled MCP servers
      try {
        const { useMcpRegistry } = await import('./lib/ai/mcp');
        await useMcpRegistry.getState().connectAll();
      } catch (e) {
        console.warn('Failed to auto-connect MCP servers:', e);
      }

      try {
        const { useAgentStore } = await import('./store/agentStore');
        await useAgentStore.getState().initHistory();
      } catch (e) {
        console.warn('Failed to load agent history:', e);
        try {
          const { useToastStore } = await import('./hooks/useToast');
          useToastStore.getState().addToast({
            title: 'Agent history failed to load',
            variant: 'warning',
          });
        } catch (toastErr) {
          console.error('Failed to show agent history warning:', toastErr);
        }
      }
    })();
  }, []);

  // Sync SFTP settings to backend on app startup
  useEffect(() => {
    const syncSftpSettings = async () => {
      const { settings } = useSettingsStore.getState();
      const sftp = settings.sftp;
      if (sftp) {
        const { api } = await import('./lib/api');
        try {
          await api.sftpUpdateSettings(
            sftp.maxConcurrentTransfers,
            sftp.speedLimitEnabled ? sftp.speedLimitKBps : 0
          );
        } catch (err) {
          console.error('Failed to sync SFTP settings on startup:', err);
        }
      }
    };
    syncSftpSettings();
  }, []);

  // Sync AI provider config to backend for CLI server access
  useEffect(() => {
    const syncAiProviders = async () => {
      const { settings } = useSettingsStore.getState();
      const ai = settings.ai;
      if (ai?.providers) {
        const { api } = await import('./lib/api');
        try {
          await api.syncAiProviders(
            ai.providers.map(p => ({
              id: p.id,
              type: p.type,
              baseUrl: p.baseUrl,
              defaultModel: p.defaultModel,
              enabled: p.enabled,
            })),
            ai.activeProviderId,
          );
        } catch (err) {
          console.error('Failed to sync AI providers on startup:', err);
        }
      }
    };
    syncAiProviders();

    // Re-sync when settings change
    const unsub = useSettingsStore.subscribe(
      (state) => state.settings.ai,
      (ai) => {
        if (ai?.providers) {
          import('./lib/api').then(({ api }) => {
            api.syncAiProviders(
              ai.providers.map(p => ({
                id: p.id,
                type: p.type,
                baseUrl: p.baseUrl,
                defaultModel: p.defaultModel,
                enabled: p.enabled,
              })),
              ai.activeProviderId,
            ).catch(err => console.error('Failed to sync AI providers:', err));
          });
        }
      },
    );
    return unsub;
  }, []);

  // Initialize terminal background: re-grant asset scope & reconcile stored path
  useEffect(() => {
    const initBg = async () => {
      const { invoke } = await import('@tauri-apps/api/core');
      try {
        const allPaths = await invoke<string[]>('init_terminal_background');
        const { settings, updateTerminal } = useSettingsStore.getState();
        const storedPath = settings.terminal.backgroundImage;

        if (storedPath && !allPaths.includes(storedPath)) {
          // Stored path points to a file that no longer exists — clear setting
          updateTerminal('backgroundImage', null);
        }
      } catch (err) {
        console.error('Failed to init terminal background:', err);
      }
    };
    initBg();
  }, []);
  
  // Handle creating local terminal with default shell
  const handleCreateLocalTerminal = useCallback(async () => {
    try {
      const info = await createTerminal();
      createTab('local_terminal', info.id);
    } catch (err) {
      console.error('Failed to create local terminal:', err);
    }
  }, [createTerminal, createTab]);

  // ── Unified Keybinding Dispatcher ──
  // Single capture-phase handler replacing useAppShortcuts + native fallback + useSplitPaneShortcuts.
  // All key matching goes through keybindingRegistry.matchAction().
  const { handleSplit, handleClosePane, handleNavigate, getPaneCount } = useSplitPaneActions();

  const actionHandlers = useMemo((): Partial<Record<ActionId, () => void>> => ({
    // ── Global actions ──
    'app.newTerminal': handleCreateLocalTerminal,
    'app.shellLauncher': () => setShellLauncherOpen(true),
    'app.closeTab': () => {
      const id = useAppStore.getState().activeTabId;
      if (id) useAppStore.getState().closeTab(id);
    },
    'app.closeOtherTabs': () => {
      const { tabs, activeTabId } = useAppStore.getState();
      if (!activeTabId) return;
      const tab = tabs.find(t => t.id === activeTabId);
      const isTermTab = tab?.type === 'terminal' || tab?.type === 'local_terminal';
      if (isTermTab) {
        // In terminal tab, this binding doubles as split pane close
        if (getPaneCount(activeTabId) > 1) {
          handleClosePane();
        }
        // Single pane terminal: do nothing (preserves existing behavior)
        return;
      }
      // Non-terminal tab: close other tabs
      const others = tabs.filter(t => t.id !== activeTabId);
      const { closeTab } = useAppStore.getState();
      (async () => {
        for (const t of others) await closeTab(t.id);
      })();
    },
    'app.newConnection': () => useAppStore.getState().toggleModal('newConnection', true),
    'app.settings': () => useAppStore.getState().createTab('settings'),
    'app.toggleSidebar': () => useSettingsStore.getState().toggleSidebar(),
    'app.commandPalette': () => setCommandPaletteOpen(true),
    'app.zenMode': () => useSettingsStore.getState().toggleZenMode(),
    'app.fontIncrease': () => {
      const s = useSettingsStore.getState();
      const cur = s.settings.terminal.fontSize;
      const next = Math.min(32, cur + 1);
      if (cur < 32) { s.updateTerminal('fontSize', next); showFontSize(next); }
    },
    'app.fontDecrease': () => {
      const s = useSettingsStore.getState();
      const cur = s.settings.terminal.fontSize;
      const next = Math.max(8, cur - 1);
      if (cur > 8) { s.updateTerminal('fontSize', next); showFontSize(next); }
    },
    'app.fontReset': () => {
      useSettingsStore.getState().updateTerminal('fontSize', 14);
      showFontSize(14);
    },
    'app.showShortcuts': () => setShortcutsModalOpen(true),
    'app.nextTab': () => useAppStore.getState().nextTab(),
    'app.prevTab': () => useAppStore.getState().prevTab(),
    'app.navBack': () => useAppStore.getState().navigateBack(),
    'app.navForward': () => useAppStore.getState().navigateForward(),
    'app.goToTab1': () => useAppStore.getState().goToTab(0),
    'app.goToTab2': () => useAppStore.getState().goToTab(1),
    'app.goToTab3': () => useAppStore.getState().goToTab(2),
    'app.goToTab4': () => useAppStore.getState().goToTab(3),
    'app.goToTab5': () => useAppStore.getState().goToTab(4),
    'app.goToTab6': () => useAppStore.getState().goToTab(5),
    'app.goToTab7': () => useAppStore.getState().goToTab(6),
    'app.goToTab8': () => useAppStore.getState().goToTab(7),
    'app.goToTab9': () => useAppStore.getState().goToTab(8),
    // ── Palette actions ──
    'palette.eventLog': () => {
      useActivityStore.getState().setActiveView('event_log');
      useAppStore.getState().createTab('activity');
    },
    'palette.aiSidebar': () => useSettingsStore.getState().toggleAiSidebar(),
    'palette.broadcast': () => useBroadcastStore.getState().toggle(),
    // ── Split actions ──
    'split.horizontal': () => handleSplit('horizontal'),
    'split.vertical': () => handleSplit('vertical'),
    'split.closePane': () => handleClosePane(),
    'split.navLeft': () => handleNavigate('left'),
    'split.navRight': () => handleNavigate('right'),
  }), [handleCreateLocalTerminal, handleSplit, handleClosePane, handleNavigate, getPaneCount]);

  useKeybindingDispatcher(actionHandlers, {
    isTerminalActive,
    isPanelOpen: shellLauncherOpen,
  });

  // Listen for split commands dispatched by the Command Palette
  useEffect(() => {
    const handleSplitEvent = (e: Event) => {
      const detail = (e as CustomEvent<{ direction: 'horizontal' | 'vertical' }>).detail;
      if (detail?.direction) {
        handleSplit(detail.direction);
      }
    };
    window.addEventListener('oxideterm:split', handleSplitEvent);
    return () => window.removeEventListener('oxideterm:split', handleSplitEvent);
  }, [handleSplit]);

  // Startup update check — silent, fires once after 8s
  useEffect(() => {
    useUpdateStore.getState().initAutoUpdateCheck(8000);
    const unlisten = useUpdateStore.getState().initResumableListeners();
    return unlisten;
  }, []);

  // Post-update: re-install CLI companion on Windows (copy-based, not symlink)
  useEffect(() => {
    (async () => {
      try {
        const { getVersion } = await import('@tauri-apps/api/app');
        const currentVersion = await getVersion();
        const store = useUpdateStore.getState();
        const lastVersion = store.lastInstalledVersion;

        if (lastVersion && lastVersion !== currentVersion) {
          // Version changed — check if CLI is installed and re-install (fixes Windows copy staleness)
          const { api } = await import('./lib/api');
          const status = await api.cliGetStatus();
          if (status.installed && status.bundled) {
            await api.cliInstall();
          }
        }

        // Always persist current version
        useUpdateStore.setState({ lastInstalledVersion: currentVersion });
      } catch {
        // Non-critical — don't block startup
      }
    })();
  }, []);

  // Setup SessionTree state sync
  useEffect(() => {
    setupTreeStoreSubscriptions();
    return () => cleanupTreeStoreSubscriptions();
  }, []);

  return (
    <ErrorBoundary>
      <TooltipProvider delayDuration={300} skipDelayDuration={100}>
        <AppLayout />
        <Toaster />
        <UpdateNotification />
        <PluginProgressHost />
        <AutoRouteModal />
        <OnboardingModal />
        <GlobalKbiDialog />
        <PluginConfirmDialog />
        <CommandPalette open={commandPaletteOpen} onOpenChange={setCommandPaletteOpen} onOpenShortcuts={() => setShortcutsModalOpen(true)} />
        <KeyboardShortcutsModal open={shortcutsModalOpen} onOpenChange={setShortcutsModalOpen} />
        <LocalShellLauncher 
          open={shellLauncherOpen} 
          onOpenChange={setShellLauncherOpen} 
        />
        {playerModal.open && playerModal.content && (
          <CastPlayer
            content={playerModal.content}
            fileName={playerModal.fileName}
            onClose={closePlayer}
          />
        )}
        <FontSizeHUD />
      </TooltipProvider>
    </ErrorBoundary>
  );
}

export default App;
