import { useEffect, useState, useCallback, useMemo } from 'react';
import { AppLayout } from './components/layout/AppLayout';
import { Toaster } from './components/ui/toaster';
import { AutoRouteModal } from './components/modals/AutoRouteModal';
import { LocalShellLauncher } from './components/local/LocalShellLauncher';
import { ErrorBoundary } from './components/ErrorBoundary';
import { useNetworkStatus } from './hooks/useNetworkStatus';
import { useConnectionEvents } from './hooks/useConnectionEvents';
import { setupTreeStoreSubscriptions, cleanupTreeStoreSubscriptions } from './store/sessionTreeStore';
import { useLocalTerminalStore } from './store/localTerminalStore';
import { useAppStore } from './store/appStore';
import { useSettingsStore } from './store/settingsStore';
import { useAppShortcuts, ShortcutDefinition, isTerminalReservedKey } from './hooks/useTerminalKeyboard';
import { useSplitPaneShortcuts } from './hooks/useSplitPaneShortcuts';
import { preloadTerminalFonts } from './lib/fontLoader';

function App() {
  // Initialize global event listeners
  // useReconnectEvents 已废弃，由 useConnectionEvents 统一处理连接事件
  useNetworkStatus();
  useConnectionEvents();
  
  // Shell launcher state
  const [shellLauncherOpen, setShellLauncherOpen] = useState(false);
  const { createTerminal, loadShells, shellsLoaded } = useLocalTerminalStore();
  const { createTab, activeTabId, tabs } = useAppStore();

  // Determine if a terminal is currently active
  const isTerminalActive = useMemo(() => {
    if (!activeTabId) return false;
    const activeTab = tabs.find(t => t.id === activeTabId);
    return activeTab?.type === 'terminal' || activeTab?.type === 'local_terminal';
  }, [activeTabId, tabs]);

  // Load shells on mount
  useEffect(() => {
    if (!shellsLoaded) {
      loadShells();
    }
  }, [shellsLoaded, loadShells]);

  // Preload fonts based on user settings (lazy load CJK font)
  useEffect(() => {
    const { settings } = useSettingsStore.getState();
    preloadTerminalFonts(settings.terminal.fontFamily);
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
      // If terminal is active and this is a terminal-reserved key, don't interfere
      if (isTerminalActive && isTerminalReservedKey(e)) {
        // Let the event propagate to the terminal
        return;
      }
      
      // Special case: Cmd+T on Mac should still create new tab even when terminal is active
      // because Cmd+key is not a standard terminal control sequence
      // (Ctrl+T is, but Cmd+T isn't)
      if (e.metaKey && !e.ctrlKey && e.key.toLowerCase() === 't' && !e.shiftKey) {
        e.preventDefault();
        handleCreateLocalTerminal();
        return;
      }
      
      // Cmd+Shift+T on Mac
      if (e.metaKey && !e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 't') {
        e.preventDefault();
        setShellLauncherOpen(true);
        return;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleCreateLocalTerminal, isTerminalActive]);

  // Setup SessionTree state sync
  useEffect(() => {
    setupTreeStoreSubscriptions();
    return () => cleanupTreeStoreSubscriptions();
  }, []);

  return (
    <ErrorBoundary>
      <AppLayout />
      <Toaster />
      <AutoRouteModal />
      <LocalShellLauncher 
        open={shellLauncherOpen} 
        onOpenChange={setShellLauncherOpen} 
      />
    </ErrorBoundary>
  );
}

export default App;
