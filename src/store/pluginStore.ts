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
  /** Disposables per plugin: key = pluginId */
  disposables: Map<string, Disposable[]>;

  // ── Registry State (Remote Installation) ───────────────────────────
  /** Plugins available in the remote registry */
  registryEntries: RegistryEntry[];
  /** Installation progress per plugin: key = pluginId */
  installProgress: Map<string, InstallProgress>;
  /** Plugins with available updates */
  availableUpdates: RegistryEntry[];

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
  disposables: new Map(),
  registryEntries: [],
  installProgress: new Map(),
  availableUpdates: [],

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

      const disposables = new Map(prev.disposables);
      disposables.delete(pluginId);

      return { tabViews, sidebarPanels, inputInterceptors, outputProcessors, shortcuts, disposables };
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
}));
