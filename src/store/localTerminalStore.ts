import { create } from 'zustand';
import { api } from '../lib/api';
import { useToastStore } from '../hooks/useToast';
import { useSettingsStore } from './settingsStore';
import i18n from '../i18n';
import {
  ShellInfo,
  LocalTerminalInfo,
  CreateLocalTerminalRequest,
} from '../types';

interface LocalTerminalStore {
  // State
  terminals: Map<string, LocalTerminalInfo>;
  shells: ShellInfo[];
  defaultShell: ShellInfo | null;
  shellsLoaded: boolean;

  // Actions
  loadShells: () => Promise<void>;
  createTerminal: (request?: CreateLocalTerminalRequest) => Promise<LocalTerminalInfo>;
  closeTerminal: (sessionId: string) => Promise<void>;
  resizeTerminal: (sessionId: string, cols: number, rows: number) => Promise<void>;
  writeTerminal: (sessionId: string, data: Uint8Array) => Promise<void>;
  refreshTerminals: () => Promise<void>;
  cleanupDeadSessions: () => Promise<string[]>;
  
  // Internal
  updateTerminalState: (sessionId: string, running: boolean) => void;
  removeTerminal: (sessionId: string) => void;
  
  // Computed
  getTerminal: (sessionId: string) => LocalTerminalInfo | undefined;
}

export const useLocalTerminalStore = create<LocalTerminalStore>((set, get) => ({
  terminals: new Map(),
  shells: [],
  defaultShell: null,
  shellsLoaded: false,

  loadShells: async () => {
    try {
      const [shells, defaultShell] = await Promise.all([
        api.localListShells(),
        api.localGetDefaultShell(),
      ]);
      set({ shells, defaultShell, shellsLoaded: true });
    } catch (error) {
      console.error('Failed to load shells:', error);
      useToastStore.getState().addToast({
        title: i18n.t('local_shell.toast.load_shells_failed'),
        description: String(error),
        variant: 'error',
      });
    }
  },

  createTerminal: async (request?: CreateLocalTerminalRequest) => {
    try {
      // Get local terminal settings
      const localSettings = useSettingsStore.getState().settings.localTerminal;
      let { shells, shellsLoaded } = get();
      
      // Ensure shells are loaded before we try to resolve defaultShellId
      if (!shellsLoaded || shells.length === 0) {
        console.debug('[LocalTerminal] Shells not loaded, loading now...');
        await get().loadShells();
        shells = get().shells;
      }
      
      // Resolve shell path from settings if not explicitly provided in request
      let shellPath = request?.shellPath;
      if (!shellPath && localSettings?.defaultShellId) {
        // Find the shell by ID and get its path
        const defaultShell = shells.find(s => s.id === localSettings.defaultShellId);
        if (defaultShell) {
          shellPath = defaultShell.path;
          console.debug(`[LocalTerminal] Using configured default shell: ${defaultShell.label} (${shellPath})`);
        } else {
          console.warn(`[LocalTerminal] Configured defaultShellId "${localSettings.defaultShellId}" not found in available shells`);
        }
      }
      
      // Merge settings into request (request overrides settings)
      // Note: We build the object carefully to ensure shellPath from settings is used
      // when request doesn't explicitly provide one
      const mergedRequest: CreateLocalTerminalRequest = {
        // Shell path: request.shellPath takes precedence, otherwise use resolved shellPath from settings
        shellPath: request?.shellPath ?? shellPath,
        // CWD: request.cwd takes precedence, otherwise use settings
        cwd: request?.cwd ?? localSettings?.defaultCwd ?? undefined,
        // Cols/Rows from request if provided
        cols: request?.cols,
        rows: request?.rows,
        // Profile loading - default true, but can be overridden by settings
        loadProfile: request?.loadProfile ?? localSettings?.loadShellProfile ?? true,
        // Oh My Posh settings
        ohMyPoshEnabled: request?.ohMyPoshEnabled ?? localSettings?.ohMyPoshEnabled ?? false,
        ohMyPoshTheme: request?.ohMyPoshTheme ?? localSettings?.ohMyPoshTheme ?? undefined,
      };
      
      console.debug('[LocalTerminal] Creating terminal with request:', mergedRequest);
      
      const response = await api.localCreateTerminal(mergedRequest);
      
      set((state) => {
        const newTerminals = new Map(state.terminals);
        newTerminals.set(response.sessionId, response.info);
        return { terminals: newTerminals };
      });

      useToastStore.getState().addToast({
        title: i18n.t('local_shell.toast.terminal_created'),
        description: i18n.t('local_shell.toast.using_shell', { shell: response.info.shell.label }),
      });

      return response.info;
    } catch (error) {
      console.error('Failed to create local terminal:', error);
      useToastStore.getState().addToast({
        title: i18n.t('local_shell.toast.create_failed'),
        description: String(error),
        variant: 'error',
      });
      throw error;
    }
  },

  closeTerminal: async (sessionId: string) => {
    try {
      await api.localCloseTerminal(sessionId);
      get().removeTerminal(sessionId);
    } catch (error) {
      console.error('Failed to close local terminal:', error);
      // Still remove from local state even if backend fails
      get().removeTerminal(sessionId);
    }
  },

  resizeTerminal: async (sessionId: string, cols: number, rows: number) => {
    try {
      if (!Number.isFinite(cols) || !Number.isFinite(rows) || cols <= 0 || rows <= 0) {
        return;
      }
      await api.localResizeTerminal(sessionId, cols, rows);
      
      set((state) => {
        const terminal = state.terminals.get(sessionId);
        if (!terminal) return state;
        
        const newTerminals = new Map(state.terminals);
        newTerminals.set(sessionId, { ...terminal, cols, rows });
        return { terminals: newTerminals };
      });
    } catch (error) {
      console.error('Failed to resize local terminal:', error);
    }
  },

  writeTerminal: async (sessionId: string, data: Uint8Array) => {
    try {
      // Convert Uint8Array to number[] for Tauri invoke
      await api.localWriteTerminal(sessionId, Array.from(data));
    } catch (error) {
      console.error('Failed to write to local terminal:', error);
      // Terminal might have closed, update state
      get().updateTerminalState(sessionId, false);
    }
  },

  refreshTerminals: async () => {
    try {
      const terminals = await api.localListTerminals();
      const newTerminals = new Map<string, LocalTerminalInfo>();
      for (const terminal of terminals) {
        newTerminals.set(terminal.id, terminal);
      }
      set({ terminals: newTerminals });
    } catch (error) {
      console.error('Failed to refresh local terminals:', error);
    }
  },

  cleanupDeadSessions: async () => {
    try {
      const removed = await api.localCleanupDeadSessions();
      if (removed.length > 0) {
        set((state) => {
          const newTerminals = new Map(state.terminals);
          for (const id of removed) {
            newTerminals.delete(id);
          }
          return { terminals: newTerminals };
        });
      }
      return removed;
    } catch (error) {
      console.error('Failed to cleanup dead sessions:', error);
      return [];
    }
  },

  updateTerminalState: (sessionId: string, running: boolean) => {
    set((state) => {
      const terminal = state.terminals.get(sessionId);
      if (!terminal) return state;
      
      const newTerminals = new Map(state.terminals);
      newTerminals.set(sessionId, { ...terminal, running });
      return { terminals: newTerminals };
    });
  },

  removeTerminal: (sessionId: string) => {
    set((state) => {
      const newTerminals = new Map(state.terminals);
      newTerminals.delete(sessionId);
      return { terminals: newTerminals };
    });
  },

  getTerminal: (sessionId: string) => {
    return get().terminals.get(sessionId);
  },
}));
