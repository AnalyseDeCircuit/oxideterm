// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useEffect, useState, useCallback, useMemo } from 'react';
import { AppLayout } from './components/layout/AppLayout';
import { Toaster } from './components/ui/toaster';
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
import { useAppShortcuts, ShortcutDefinition, isTerminalReservedKey } from './hooks/useTerminalKeyboard';
import { useSplitPaneShortcuts } from './hooks/useSplitPaneShortcuts';
import { preloadTerminalFonts } from './lib/fontLoader';
import { initializePluginSystem } from './lib/plugin/pluginLoader';
import { setupConnectionBridge, setupNodeStateBridge, setupTransferBridge, pluginEventBridge } from './lib/plugin/pluginEventBridge';
import { useToastStore } from './hooks/useToast';
import { useUpdateStore } from './store/updateStore';
import { PluginConfirmDialog } from './components/plugin/PluginConfirmDialog';
import { CommandPalette } from './components/command-palette/CommandPalette';
import { CastPlayer } from './components/terminal/CastPlayer';
import { OnboardingModal } from './components/modals/OnboardingModal';
import { KeyboardShortcutsModal } from './components/modals/KeyboardShortcutsModal';
import { TooltipProvider } from './components/ui/tooltip';
import { useFontSizeHUD } from './components/ui/FontSizeHUD';
import { useRecordingStore } from './store/recordingStore';
import { useCommandPaletteStore } from './store/commandPaletteStore';

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

  // Define app-level shortcuts using the unified keyboard manager
  const appShortcuts: ShortcutDefinition[] = useMemo(() => [
    {
      key: 't',
      ctrl: true,
      shift: false,
      action: handleCreateLocalTerminal,
      description: 'Create new local terminal with default shell',
      // 'never' when terminal is focused - allows Ctrl+T to reach vim/emacs
      // But we still want Cmd+T to work on Mac, so we check platform
      terminalBehavior: 'never' as const,
    },
    {
      key: 't',
      ctrl: true,
      shift: true,
      action: () => setShellLauncherOpen(true),
      description: 'Open shell launcher',
      terminalBehavior: 'always' as const,
    },
    {
      key: 'w',
      ctrl: true,
      shift: false,
      action: () => {
        const id = useAppStore.getState().activeTabId;
        if (id) useAppStore.getState().closeTab(id);
      },
      description: 'Close current tab',
      terminalBehavior: 'always' as const,
    },
    {
      key: 'n',
      ctrl: true,
      shift: false,
      action: () => useAppStore.getState().toggleModal('newConnection', true),
      description: 'New SSH connection',
      terminalBehavior: 'always' as const,
    },
    {
      key: ',',
      ctrl: true,
      shift: false,
      action: () => useAppStore.getState().createTab('settings'),
      description: 'Open settings',
      terminalBehavior: 'always' as const,
    },
    {
      key: '\\',
      ctrl: true,
      shift: false,
      action: () => useSettingsStore.getState().toggleSidebar(),
      description: 'Toggle sidebar',
      terminalBehavior: 'always' as const,
    },
    {
      key: 'k',
      ctrl: true,
      shift: false,
      action: () => setCommandPaletteOpen(true),
      description: 'Open command palette',
      terminalBehavior: 'always' as const,
    },
    {
      key: 'z',
      ctrl: true,
      shift: true,
      action: () => useSettingsStore.getState().toggleZenMode(),
      description: 'Toggle Zen Mode',
      terminalBehavior: 'always' as const,
    },
    {
      key: '=',
      ctrl: true,
      shift: false,
      action: () => {
        const s = useSettingsStore.getState();
        const cur = s.settings.terminal.fontSize;
        const next = Math.min(32, cur + 1);
        if (cur < 32) { s.updateTerminal('fontSize', next); showFontSize(next); }
      },
      description: 'Increase font size',
      terminalBehavior: 'always' as const,
    },
    {
      key: '-',
      ctrl: true,
      shift: false,
      action: () => {
        const s = useSettingsStore.getState();
        const cur = s.settings.terminal.fontSize;
        const next = Math.max(8, cur - 1);
        if (cur > 8) { s.updateTerminal('fontSize', next); showFontSize(next); }
      },
      description: 'Decrease font size',
      terminalBehavior: 'always' as const,
    },
    {
      key: '0',
      ctrl: true,
      shift: false,
      action: () => {
        useSettingsStore.getState().updateTerminal('fontSize', 14);
        showFontSize(14);
      },
      description: 'Reset font size',
      terminalBehavior: 'always' as const,
    },
    {
      key: '/',
      ctrl: true,
      shift: false,
      action: () => setShortcutsModalOpen(true),
      description: 'Show keyboard shortcuts',
      terminalBehavior: 'always' as const,
    },
  ], [handleCreateLocalTerminal]);

  // Use unified keyboard manager for app shortcuts
  // Context: terminal is active, no panels open at app level
  useAppShortcuts(appShortcuts, {
    isTerminalActive,
    isPanelOpen: shellLauncherOpen,
  });

  // Split pane shortcuts (Cmd+Shift+E/D, Cmd+Option+Arrow)
  useSplitPaneShortcuts({ enabled: isTerminalActive });

  // Additional keyboard handling for terminal-reserved keys
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Skip if window lost OS-level focus
      if (!document.hasFocus()) return;

      // If terminal is active and this is a terminal-reserved key, don't interfere
      if (isTerminalActive && isTerminalReservedKey(e)) {
        // Let the event propagate to the terminal
        return;
      }
      
      // ─── Mac Cmd+key shortcuts ───
      // Cmd+key is NOT a standard terminal control sequence, so it's
      // safe to intercept even when the terminal is focused.
      if (e.metaKey && !e.ctrlKey) {
        const key = e.key.toLowerCase();

        // Cmd+T — New local terminal
        if (key === 't' && !e.shiftKey) {
          e.preventDefault();
          handleCreateLocalTerminal();
          return;
        }
        // Cmd+Shift+T — Shell launcher
        if (key === 't' && e.shiftKey) {
          e.preventDefault();
          setShellLauncherOpen(true);
          return;
        }
        // Cmd+W — Close current tab
        if (key === 'w' && !e.shiftKey) {
          e.preventDefault();
          const id = useAppStore.getState().activeTabId;
          if (id) useAppStore.getState().closeTab(id);
          return;
        }
        // Cmd+Shift+W — Close other tabs
        if (key === 'w' && e.shiftKey) {
          e.preventDefault();
          const { tabs, activeTabId, closeTab } = useAppStore.getState();
          if (!activeTabId) return;
          const others = tabs.filter((tab) => tab.id !== activeTabId);
          (async () => {
            for (const tab of others) { await closeTab(tab.id); }
          })();
          return;
        }
        // Cmd+N — New SSH connection
        if (key === 'n' && !e.shiftKey) {
          e.preventDefault();
          useAppStore.getState().toggleModal('newConnection', true);
          return;
        }
        // Cmd+, — Open settings
        if (key === ',' && !e.shiftKey) {
          e.preventDefault();
          useAppStore.getState().createTab('settings');
          return;
        }
        // Cmd+\ — Toggle sidebar
        if (e.key === '\\' && !e.shiftKey) {
          e.preventDefault();
          useSettingsStore.getState().toggleSidebar();
          return;
        }
        // Cmd+= / Cmd+- — Font size zoom
        if ((e.key === '=' || e.key === '+') && !e.shiftKey) {
          e.preventDefault();
          const s = useSettingsStore.getState();
          const cur = s.settings.terminal.fontSize;
          const next = Math.min(32, cur + 1);
          if (cur < 32) { s.updateTerminal('fontSize', next); showFontSize(next); }
          return;
        }
        if (e.key === '-' && !e.shiftKey) {
          e.preventDefault();
          const s = useSettingsStore.getState();
          const cur = s.settings.terminal.fontSize;
          const next = Math.max(8, cur - 1);
          if (cur > 8) { s.updateTerminal('fontSize', next); showFontSize(next); }
          return;
        }
        if (e.key === '0' && !e.shiftKey) {
          e.preventDefault();
          useSettingsStore.getState().updateTerminal('fontSize', 14);
          showFontSize(14);
          return;
        }
        // Cmd+Shift+Z — Zen Mode
        if (key === 'z' && e.shiftKey) {
          e.preventDefault();
          useSettingsStore.getState().toggleZenMode();
          return;
        }
        // Cmd+} / Cmd+{ — Next/Prev tab (Shift+]/Shift+[ on US layout)
        if (e.key === '}') {
          e.preventDefault();
          useAppStore.getState().nextTab();
          return;
        }
        if (e.key === '{') {
          e.preventDefault();
          useAppStore.getState().prevTab();
          return;
        }
        // Cmd+[ — Navigate back
        if (e.key === '[' && !e.shiftKey) {
          e.preventDefault();
          useAppStore.getState().navigateBack();
          return;
        }
        // Cmd+] — Navigate forward
        if (e.key === ']' && !e.shiftKey) {
          e.preventDefault();
          useAppStore.getState().navigateForward();
          return;
        }
        // Cmd+1-9 — Go to tab N
        if (key >= '1' && key <= '9') {
          e.preventDefault();
          useAppStore.getState().goToTab(parseInt(key, 10) - 1);
          return;
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleCreateLocalTerminal, isTerminalActive]);

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
        <AutoRouteModal />
        <OnboardingModal />
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
