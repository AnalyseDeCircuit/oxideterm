/**
 * OxideTerm Plugin Store
 *
 * Central state management for the plugin system.
 * Holds plugin registry, UI component registrations, terminal hooks, and disposables.
 */

import { create } from 'zustand';
import type {
  PluginInfo,
  PluginManifest,
  PluginState,
  PluginModule,
  Disposable,
  InputInterceptor,
  OutputProcessor,
  PluginTabProps,
  RegistryEntry,
  InstallState,
  PluginCommandEntry,
} from '../types/plugin';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/** Registered tab view component */
type TabViewEntry = {
  pluginId: string;
  tabId: string;
  component: React.ComponentType<PluginTabProps>;
};

/** Registered sidebar panel component */
type SidebarPanelEntry = {
  pluginId: string;
  panelId: string;
  component: React.ComponentType;
  title: string;
  icon: string;
  position: 'top' | 'bottom';
};

/** Registered input interceptor */
type InputInterceptorEntry = {
  pluginId: string;
  handler: InputInterceptor;
};

/** Registered output processor */
type OutputProcessorEntry = {
  pluginId: string;
  handler: OutputProcessor;
};

/** Registered shortcut */
type ShortcutEntry = {
  pluginId: string;
  command: string;
  key: string;
  handler: () => void;
};

/** Plugin install progress info */
type InstallProgress = {
  state: InstallState;
  error?: string;
};

/** A single plugin log entry */
export type PluginLogEntry = {
  timestamp: number;
  level: 'info' | 'warn' | 'error';
  message: string;
};

// ═══════════════════════════════════════════════════════════════════════════
// Store Interface
// ═══════════════════════════════════════════════════════════════════════════

interface PluginStore {
  // ── State ────────────────────────────────────────────────────────────
  /** All discovered/loaded plugins */
  plugins: Map<string, PluginInfo>;
  /** Plugin tab view components: key = "pluginId:tabId" */
  tabViews: Map<string, TabViewEntry>;
  /** Plugin sidebar panels: key = "pluginId:panelId" */
  sidebarPanels: Map<string, SidebarPanelEntry>;
  /** Input interceptors in registration order */
  inputInterceptors: InputInterceptorEntry[];
  /** Output processors in registration order */
  outputProcessors: OutputProcessorEntry[];
  /** Shortcuts: key = normalized key combo (e.g. "ctrl+shift+a") */
  shortcuts: Map<string, ShortcutEntry>;
  /** Plugin commands for command palette: key = "pluginId:commandId" */
  commands: Map<string, PluginCommandEntry>;
  /** Context menu items: key = "pluginId:target:uuid" */
  contextMenuItems: Map<string, { pluginId: string; target: string; items: { label: string; handler: () => void; icon?: string }[] }>;
  /** Status bar items: key = "pluginId:itemId" */
  statusBarItems: Map<string, { pluginId: string; text: string; tooltip?: string; icon?: string; onClick?: () => void; priority?: number; alignment?: 'left' | 'right' }>;
  /** Global keybindings: key = "pluginId:keybinding" */
  keybindings: Map<string, { pluginId: string; keybinding: string; handler: () => void }>;
  /** Disposables per plugin: key = pluginId */
  disposables: Map<string, Disposable[]>;

  // ── Registry State (Remote Installation) ───────────────────────────
  /** Plugins available in the remote registry */
  registryEntries: RegistryEntry[];
  /** Installation progress per plugin: key = pluginId */
  installProgress: Map<string, InstallProgress>;
  /** Plugins with available updates */
  availableUpdates: RegistryEntry[];

  /** Plugin runtime logs: key = pluginId */
  pluginLogs: Map<string, PluginLogEntry[]>;

  // ── Plugin Lifecycle ────────────────────────────────────────────────
  /** Register a discovered plugin (initially inactive) */
  registerPlugin: (manifest: PluginManifest) => void;
  /** Update plugin state */
  setPluginState: (pluginId: string, state: PluginState, error?: string) => void;
  /** Store the loaded JS module reference */
  setPluginModule: (pluginId: string, module: PluginModule) => void;
  /** Remove a plugin from the registry entirely */
  removePlugin: (pluginId: string) => void;

  // ── UI Registrations ────────────────────────────────────────────────
  /** Register a tab view component */
  registerTabView: (pluginId: string, tabId: string, component: React.ComponentType<PluginTabProps>) => void;
  /** Register a sidebar panel component */
  registerSidebarPanel: (
    pluginId: string,
    panelId: string,
    component: React.ComponentType,
    title: string,
    icon: string,
    position: 'top' | 'bottom',
  ) => void;

  // ── Terminal Hooks ──────────────────────────────────────────────────
  /** Register an input interceptor */
  registerInputInterceptor: (pluginId: string, handler: InputInterceptor) => void;
  /** Register an output processor */
  registerOutputProcessor: (pluginId: string, handler: OutputProcessor) => void;
  /**
   * Register a keyboard shortcut.
   * Keys are normalized: lowercased, sorted (e.g. "ctrl+shift+k").
   * On macOS, Cmd(⌘) is treated as Ctrl — plugins declare "Ctrl+X" and
   * it matches both Cmd+X on macOS and Ctrl+X on Windows/Linux.
   */
  registerShortcut: (pluginId: string, command: string, key: string, handler: () => void) => void;

  // ── Command Palette Commands ─────────────────────────────────────
  /** Register a command for the command palette */
  registerCommand: (pluginId: string, entry: Omit<PluginCommandEntry, 'pluginId'>) => void;

  // ── Disposable Tracking ─────────────────────────────────────────────
  /** Track a disposable for a plugin (auto-cleanup on unload) */
  trackDisposable: (pluginId: string, disposable: Disposable) => void;

  // ── Cleanup ─────────────────────────────────────────────────────────
  /** Clean up all registrations and disposables for a plugin */
  cleanupPlugin: (pluginId: string) => void;

  // ── Queries ─────────────────────────────────────────────────────────
  /** Get plugin info by ID */
  getPlugin: (pluginId: string) => PluginInfo | undefined;
  /** Get all active plugins */
  getActivePlugins: () => PluginInfo[];
  /** Find a tab view by composite key */
  getTabView: (pluginTabId: string) => TabViewEntry | undefined;
  /** Find shortcut handler by normalized key combo */
  getShortcutHandler: (key: string) => (() => void) | undefined;

  // ── Registry Actions ───────────────────────────────────────────────
  /** Set registry entries from remote fetch */
  setRegistryEntries: (entries: RegistryEntry[]) => void;
  /** Set install progress for a plugin */
  setInstallProgress: (pluginId: string, state: InstallState, error?: string) => void;
  /** Clear install progress for a plugin */
  clearInstallProgress: (pluginId: string) => void;
  /** Set available updates */
  setAvailableUpdates: (updates: RegistryEntry[]) => void;
  /** Check if a plugin has an update available */
  hasUpdate: (pluginId: string) => boolean;

  // ── Log Actions ────────────────────────────────────────────────────
  /** Append a log entry for a plugin (capped at 200 per plugin) */
  addPluginLog: (pluginId: string, level: PluginLogEntry['level'], message: string) => void;
  /** Clear all logs for a plugin */
  clearPluginLogs: (pluginId: string) => void;
}

// ═══════════════════════════════════════════════════════════════════════════
// Store Implementation
// ═══════════════════════════════════════════════════════════════════════════

export const usePluginStore = create<PluginStore>((set, get) => ({
  // ── Initial State ───────────────────────────────────────────────────
  plugins: new Map(),
  tabViews: new Map(),
  sidebarPanels: new Map(),
  inputInterceptors: [],
  outputProcessors: [],
  shortcuts: new Map(),
  commands: new Map(),
  contextMenuItems: new Map(),
  statusBarItems: new Map(),
  keybindings: new Map(),
  disposables: new Map(),
  registryEntries: [],
  installProgress: new Map(),
  availableUpdates: [],
  pluginLogs: new Map(),

  // ── Plugin Lifecycle ────────────────────────────────────────────────

  registerPlugin: (manifest) => {
    set((state) => {
      const plugins = new Map(state.plugins);
      plugins.set(manifest.id, { manifest, state: 'inactive' });
      return { plugins };
    });
  },

  setPluginState: (pluginId, pluginState, error) => {
    set((state) => {
      const plugins = new Map(state.plugins);
      const existing = plugins.get(pluginId);
      if (!existing) return state;
      plugins.set(pluginId, { ...existing, state: pluginState, error });
      return { plugins };
    });
  },

  setPluginModule: (pluginId, module) => {
    set((state) => {
      const plugins = new Map(state.plugins);
      const existing = plugins.get(pluginId);
      if (!existing) return state;
      plugins.set(pluginId, { ...existing, module });
      return { plugins };
    });
  },

  removePlugin: (pluginId) => {
    get().cleanupPlugin(pluginId);
    set((state) => {
      const plugins = new Map(state.plugins);
      plugins.delete(pluginId);
      return { plugins };
    });
  },

  // ── UI Registrations ────────────────────────────────────────────────

  registerTabView: (pluginId, tabId, component) => {
    const compositeKey = `${pluginId}:${tabId}`;
    set((state) => {
      const tabViews = new Map(state.tabViews);
      tabViews.set(compositeKey, { pluginId, tabId, component });
      return { tabViews };
    });
  },

  registerSidebarPanel: (pluginId, panelId, component, title, icon, position) => {
    const compositeKey = `${pluginId}:${panelId}`;
    set((state) => {
      const sidebarPanels = new Map(state.sidebarPanels);
      sidebarPanels.set(compositeKey, { pluginId, panelId, component, title, icon, position });
      return { sidebarPanels };
    });
  },

  // ── Terminal Hooks ──────────────────────────────────────────────────

  registerInputInterceptor: (pluginId, handler) => {
    set((state) => ({
      inputInterceptors: [...state.inputInterceptors, { pluginId, handler }],
    }));
  },

  registerOutputProcessor: (pluginId, handler) => {
    set((state) => ({
      outputProcessors: [...state.outputProcessors, { pluginId, handler }],
    }));
  },

  registerShortcut: (pluginId, command, key, handler) => {
    const normalizedKey = key.toLowerCase().split('+').sort().join('+');
    set((state) => {
      const shortcuts = new Map(state.shortcuts);
      shortcuts.set(normalizedKey, { pluginId, command, key, handler });
      return { shortcuts };
    });
  },

  registerCommand: (pluginId, entry) => {
    const compositeKey = `${pluginId}:${entry.id}`;
    set((state) => {
      const commands = new Map(state.commands);
      commands.set(compositeKey, { ...entry, pluginId });
      return { commands };
    });
  },

  // ── Disposable Tracking ─────────────────────────────────────────────

  trackDisposable: (pluginId, disposable) => {
    set((state) => {
      const disposables = new Map(state.disposables);
      const existing = disposables.get(pluginId) ?? [];
      disposables.set(pluginId, [...existing, disposable]);
      return { disposables };
    });
  },

  // ── Cleanup ─────────────────────────────────────────────────────────

  cleanupPlugin: (pluginId) => {
    const state = get();

    // 1. Dispose all tracked disposables.
    //    Some disposables have real side effects beyond store removal (e.g. event
    //    unsubscription from pluginEventBridge, DOM style element removal).
    //    Their individual store-removal callbacks are redundant with the bulk
    //    removal below, but harmless — we keep both for defense-in-depth.
    const pluginDisposables = state.disposables.get(pluginId) ?? [];
    for (const d of pluginDisposables) {
      try { d.dispose(); } catch { /* swallow */ }
    }

    // 2. Bulk removal — single setState for efficiency.
    //    This is the authoritative cleanup; the individual disposal above may
    //    have already removed some entries, which is safe (idempotent).
    set((prev) => {
      const tabViews = new Map(prev.tabViews);
      for (const [key, entry] of tabViews) {
        if (entry.pluginId === pluginId) tabViews.delete(key);
      }

      const sidebarPanels = new Map(prev.sidebarPanels);
      for (const [key, entry] of sidebarPanels) {
        if (entry.pluginId === pluginId) sidebarPanels.delete(key);
      }

      const inputInterceptors = prev.inputInterceptors.filter((e) => e.pluginId !== pluginId);
      const outputProcessors = prev.outputProcessors.filter((e) => e.pluginId !== pluginId);

      const shortcuts = new Map(prev.shortcuts);
      for (const [key, entry] of shortcuts) {
        if (entry.pluginId === pluginId) shortcuts.delete(key);
      }

      const commands = new Map(prev.commands);
      for (const [key, entry] of commands) {
        if (entry.pluginId === pluginId) commands.delete(key);
      }

      const contextMenuItems = new Map(prev.contextMenuItems);
      for (const [key, entry] of contextMenuItems) {
        if (entry.pluginId === pluginId) contextMenuItems.delete(key);
      }

      const statusBarItems = new Map(prev.statusBarItems);
      for (const [key, entry] of statusBarItems) {
        if (entry.pluginId === pluginId) statusBarItems.delete(key);
      }

      const keybindings = new Map(prev.keybindings);
      for (const [key, entry] of keybindings) {
        if (entry.pluginId === pluginId) keybindings.delete(key);
      }

      const disposables = new Map(prev.disposables);
      disposables.delete(pluginId);

      const pluginLogs = new Map(prev.pluginLogs);
      pluginLogs.delete(pluginId);

      return { tabViews, sidebarPanels, inputInterceptors, outputProcessors, shortcuts, commands, contextMenuItems, statusBarItems, keybindings, disposables, pluginLogs };
    });
  },

  // ── Queries ─────────────────────────────────────────────────────────

  getPlugin: (pluginId) => {
    return get().plugins.get(pluginId);
  },

  getActivePlugins: () => {
    const result: PluginInfo[] = [];
    for (const info of get().plugins.values()) {
      if (info.state === 'active') result.push(info);
    }
    return result;
  },

  getTabView: (pluginTabId) => {
    return get().tabViews.get(pluginTabId);
  },

  getShortcutHandler: (key) => {
    const normalizedKey = key.toLowerCase().split('+').sort().join('+');
    return get().shortcuts.get(normalizedKey)?.handler;
  },

  // ── Registry Actions ───────────────────────────────────────────────

  setRegistryEntries: (entries) => {
    set({ registryEntries: entries });
  },

  setInstallProgress: (pluginId, state, error) => {
    set((prev) => {
      const installProgress = new Map(prev.installProgress);
      installProgress.set(pluginId, { state, error });
      return { installProgress };
    });
  },

  clearInstallProgress: (pluginId) => {
    set((prev) => {
      const installProgress = new Map(prev.installProgress);
      installProgress.delete(pluginId);
      return { installProgress };
    });
  },

  setAvailableUpdates: (updates) => {
    set({ availableUpdates: updates });
  },

  hasUpdate: (pluginId) => {
    return get().availableUpdates.some((u) => u.id === pluginId);
  },

  // ── Log Actions ────────────────────────────────────────────────────

  addPluginLog: (pluginId, level, message) => {
    set((prev) => {
      const pluginLogs = new Map(prev.pluginLogs);
      const existing = pluginLogs.get(pluginId) ?? [];
      const entry: PluginLogEntry = { timestamp: Date.now(), level, message };
      // Keep last 200 entries
      const updated = [...existing, entry].slice(-200);
      pluginLogs.set(pluginId, updated);
      return { pluginLogs };
    });
  },

  clearPluginLogs: (pluginId) => {
    set((prev) => {
      const pluginLogs = new Map(prev.pluginLogs);
      pluginLogs.delete(pluginId);
      return { pluginLogs };
    });
  },
}));
