// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Plugin Context Factory
 *
 * Builds the frozen PluginContext membrane for each plugin.
 * All API namespaces are Object.freeze'd to prevent mutation.
 * Disposables are tracked for automatic cleanup on unload.
 */

import type {
  PluginManifest,
  PluginContext,
  PluginConnectionsAPI,
  PluginEventsAPI,
  PluginUIAPI,
  PluginTerminalAPI,
  PluginSettingsAPI,
  PluginI18nAPI,
  PluginStorageAPI,
  PluginSyncAPI,
  PluginSecretsAPI,
  PluginBackendAPI,
  PluginAssetsAPI,
  PluginSftpAPI,
  PluginForwardAPI,
  PluginSessionsAPI,
  PluginTransfersAPI,
  PluginProfilerAPI,
  PluginEventLogAPI,
  PluginIdeAPI,
  PluginAiAPI,
  PluginAppAPI,
  PluginFileInfo,
  PluginForwardRule,
  ConnectionSnapshot,
  SavedConnectionSnapshot,
  SessionTreeNodeSnapshot,
  TransferSnapshot,
  ProfilerMetricsSnapshot,
  EventLogEntrySnapshot,
  IdeFileSnapshot,
  IdeProjectSnapshot,
  AiConversationSnapshot,
  AiMessageSnapshot,
  ThemeSnapshot,
  StatusBarItemOptions,
  StatusBarHandle,
  ContextMenuTarget,
  ContextMenuItem,
  ProgressReporter,
  Disposable,
  InputInterceptor,
  OutputProcessor,
  PluginTabProps,
} from '../../types/plugin';
import type {
  ExportPreflightResult,
  ImportPreview,
  ImportResult,
  OxideMetadata,
  SshConnectionState,
} from '../../types';
import { useAppStore } from '../../store/appStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { usePluginStore } from '../../store/pluginStore';
import { createPluginStorage } from './pluginStorage';
import { pluginEventBridge } from './pluginEventBridge';
import { createPluginSettingsManager } from './pluginSettingsManager';
import { createPluginI18nManager } from './pluginI18nManager';
import { toSnapshot } from './pluginUtils';
import { freezeSnapshot } from './pluginSnapshots';
import { createThrottledEmitter } from './pluginThrottledEvents';
import { normalizePluginComboDescriptor, normalizePluginKeyCombo } from './pluginHostUi';
import {
  findPaneBySessionId,
  getTerminalBuffer,
  getTerminalSelection,
  writeToTerminal as registryWriteToTerminal,
} from '../terminalRegistry';
import { getDefaults, getBinding } from '../keybindingRegistry';
import { invoke } from '@tauri-apps/api/core';

// Lazy store imports — loaded on first use to avoid circular deps
let _useTransferStore: typeof import('../../store/transferStore').useTransferStore | null = null;
let _useProfilerStore: typeof import('../../store/profilerStore').useProfilerStore | null = null;
let _useEventLogStore: typeof import('../../store/eventLogStore').useEventLogStore | null = null;
let _useIdeStore: typeof import('../../store/ideStore').useIdeStore | null = null;
let _useAiChatStore: typeof import('../../store/aiChatStore').useAiChatStore | null = null;
let _useSettingsStore: typeof import('../../store/settingsStore').useSettingsStore | null = null;

async function getTransferStore() {
  if (!_useTransferStore) {
    const mod = await import('../../store/transferStore');
    _useTransferStore = mod.useTransferStore;
  }
  return _useTransferStore;
}
async function getProfilerStore() {
  if (!_useProfilerStore) {
    const mod = await import('../../store/profilerStore');
    _useProfilerStore = mod.useProfilerStore;
  }
  return _useProfilerStore;
}
async function getEventLogStore() {
  if (!_useEventLogStore) {
    const mod = await import('../../store/eventLogStore');
    _useEventLogStore = mod.useEventLogStore;
  }
  return _useEventLogStore;
}
async function getIdeStore() {
  if (!_useIdeStore) {
    const mod = await import('../../store/ideStore');
    _useIdeStore = mod.useIdeStore;
  }
  return _useIdeStore;
}
async function getAiChatStore() {
  if (!_useAiChatStore) {
    const mod = await import('../../store/aiChatStore');
    _useAiChatStore = mod.useAiChatStore;
  }
  return _useAiChatStore;
}
async function getSettingsStore() {
  if (!_useSettingsStore) {
    const mod = await import('../../store/settingsStore');
    _useSettingsStore = mod.useSettingsStore;
  }
  return _useSettingsStore;
}

// Pre-warm lazy stores on first context build (non-blocking)
let storesWarmed = false;
function warmStores() {
  if (storesWarmed) return;
  storesWarmed = true;
  void getTransferStore();
  void getProfilerStore();
  void getEventLogStore();
  void getIdeStore();
  void getAiChatStore();
  void getSettingsStore();
}

// ── Module-level asset URL tracking for cleanup ──────────────────────────
const activeAssetUrls = new Map<string, Set<string>>();

/** MIME type map for common asset extensions */
const MIME_MAP: Record<string, string> = {
  png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg',
  gif: 'image/gif', svg: 'image/svg+xml', webp: 'image/webp',
  woff: 'font/woff', woff2: 'font/woff2', ttf: 'font/ttf',
  otf: 'font/otf', ico: 'image/x-icon', json: 'application/json',
  css: 'text/css', js: 'application/javascript',
};

/**
 * Clean up all asset URLs and injected styles for a plugin.
 * Called from pluginLoader.ts during unloadPlugin().
 */
export function cleanupPluginAssets(pluginId: string): void {
  // Remove injected CSS
  document.querySelectorAll(`style[data-plugin="${pluginId}"]`).forEach(el => el.remove());
  // Revoke blob URLs
  const urls = activeAssetUrls.get(pluginId);
  if (urls) {
    for (const url of urls) {
      URL.revokeObjectURL(url);
    }
    activeAssetUrls.delete(pluginId);
  }
}

/**
 * Create a Disposable that calls fn once, tracked automatically.
 */
function createDisposable(pluginId: string, fn: () => void): Disposable {
  let disposed = false;
  const disposable: Disposable = {
    dispose() {
      if (disposed) return;
      disposed = true;
      fn();
    },
  };
  usePluginStore.getState().trackDisposable(pluginId, disposable);
  return disposable;
}

/**
 * Resolve a nodeId to its first terminal sessionId via sessionTreeStore.
 * Returns null if the node has no terminals.
 * Phase 4.5 bridge: used by nodeId-based plugin terminal API.
 */
function resolveNodeToFirstSession(nodeId: string): string | null {
  const node = useSessionTreeStore.getState().getNode(nodeId);
  if (!node) return null;
  const terminalIds = node.runtime.terminalIds;
  return terminalIds.length > 0 ? terminalIds[0] : null;
}

function toSavedConnectionSnapshot(connection: {
  id: string;
  name: string;
  group: string | null;
  host: string;
  port: number;
  username: string;
  auth_type: 'password' | 'key' | 'agent' | 'certificate';
  key_path: string | null;
  cert_path: string | null;
  created_at: string;
  last_used_at: string | null;
  color: string | null;
  tags: string[];
  proxy_chain?: Array<{
    host: string;
    port: number;
    username: string;
    auth_type: 'password' | 'key' | 'agent';
    key_path?: string;
  }>;
}): SavedConnectionSnapshot {
  return freezeSnapshot<SavedConnectionSnapshot>({
    id: connection.id,
    name: connection.name,
    group: connection.group,
    host: connection.host,
    port: connection.port,
    username: connection.username,
    auth_type: connection.auth_type,
    key_path: connection.key_path,
    cert_path: connection.cert_path,
    created_at: connection.created_at,
    last_used_at: connection.last_used_at,
    color: connection.color,
    tags: [...connection.tags],
    proxy_chain: (connection.proxy_chain ?? []).map((hop) => ({ ...hop })),
  });
}

function toFrozenExportPreflight(result: ExportPreflightResult): Readonly<ExportPreflightResult> {
  return freezeSnapshot<ExportPreflightResult>({
    ...result,
    missingKeys: result.missingKeys.map(([name, path]) => [name, path]),
  });
}

function toFrozenOxideMetadata(metadata: OxideMetadata): Readonly<OxideMetadata> {
  return freezeSnapshot<OxideMetadata>({
    ...metadata,
    connection_names: [...metadata.connection_names],
  });
}

function toFrozenImportPreview(preview: ImportPreview): Readonly<ImportPreview> {
  return freezeSnapshot<ImportPreview>({
    ...preview,
    unchanged: [...preview.unchanged],
    willRename: preview.willRename.map(([original, renamed]) => [original, renamed]),
    willSkip: [...preview.willSkip],
    willReplace: [...preview.willReplace],
    willMerge: [...preview.willMerge],
  });
}

function toFrozenImportResult(result: ImportResult): Readonly<ImportResult> {
  return freezeSnapshot<ImportResult>({
    ...result,
    errors: [...result.errors],
    renames: result.renames.map(([original, renamed]) => [original, renamed]),
  });
}

/**
 * Build the full PluginContext for a given plugin manifest.
 * Every object in the returned value is deeply frozen.
 */
export function buildPluginContext(manifest: PluginManifest): PluginContext {
  const pluginId = manifest.id;
  const store = usePluginStore.getState;
  const storage = createPluginStorage(pluginId);
  const settingsManager = createPluginSettingsManager(pluginId, manifest);
  const i18nManager = createPluginI18nManager(pluginId);

  // Pre-warm lazy store imports
  warmStores();

  // ── ctx.connections ───────────────────────────────────────────────
  const connections: PluginConnectionsAPI = Object.freeze({
    getAll(): ReadonlyArray<ConnectionSnapshot> {
      const conns = useAppStore.getState().connections;
      return Object.freeze(Array.from(conns.values()).map(toSnapshot));
    },
    get(connectionId: string): ConnectionSnapshot | null {
      const conn = useAppStore.getState().connections.get(connectionId);
      return conn ? toSnapshot(conn) : null;
    },
    getState(connectionId: string): SshConnectionState | null {
      const conn = useAppStore.getState().connections.get(connectionId);
      return conn ? conn.state : null;
    },
    getByNode(nodeId: string): ConnectionSnapshot | null {
      const node = useSessionTreeStore.getState().getNode(nodeId);
      const connectionId = node?.runtime.connectionId;
      if (!connectionId) return null;
      const conn = useAppStore.getState().connections.get(connectionId);
      return conn ? toSnapshot(conn) : null;
    },
  });

  // ── ctx.events ────────────────────────────────────────────────────
  const events: PluginEventsAPI = Object.freeze({
    onConnect(handler) {
      const unsub = pluginEventBridge.on('connection:connect', (snapshot) => {
        try { handler(snapshot as ConnectionSnapshot); } catch { /* swallow */ }
      });
      return createDisposable(pluginId, unsub);
    },
    onDisconnect(handler) {
      const unsub = pluginEventBridge.on('connection:disconnect', (snapshot) => {
        try { handler(snapshot as ConnectionSnapshot); } catch { /* swallow */ }
      });
      return createDisposable(pluginId, unsub);
    },
    onLinkDown(handler) {
      const unsub = pluginEventBridge.on('connection:link_down', (snapshot) => {
        try { handler(snapshot as ConnectionSnapshot); } catch { /* swallow */ }
      });
      return createDisposable(pluginId, unsub);
    },
    onReconnect(handler) {
      const unsub = pluginEventBridge.on('connection:reconnect', (snapshot) => {
        try { handler(snapshot as ConnectionSnapshot); } catch { /* swallow */ }
      });
      return createDisposable(pluginId, unsub);
    },
    onIdle(handler) {
      const unsub = pluginEventBridge.on('connection:idle', (snapshot) => {
        try { handler(snapshot as ConnectionSnapshot); } catch { /* swallow */ }
      });
      return createDisposable(pluginId, unsub);
    },
    onNodeReady(handler) {
      const unsub = pluginEventBridge.on('node:ready', (info) => {
        try { handler(info as { nodeId: string; connectionId: string }); } catch { /* swallow */ }
      });
      return createDisposable(pluginId, unsub);
    },
    onNodeDisconnected(handler) {
      const unsub = pluginEventBridge.on('node:disconnected', (info) => {
        try { handler(info as { nodeId: string }); } catch { /* swallow */ }
      });
      return createDisposable(pluginId, unsub);
    },
    on(name, handler) {
      const namespacedName = `plugin:${pluginId}:${name}`;
      const unsub = pluginEventBridge.on(namespacedName, (data) => {
        try { handler(data); } catch { /* swallow */ }
      });
      return createDisposable(pluginId, unsub);
    },
    emit(name, data) {
      const namespacedName = `plugin:${pluginId}:${name}`;
      pluginEventBridge.emit(namespacedName, data);
    },
  });

  // ── ctx.ui ────────────────────────────────────────────────────────
  const declaredTabs = new Set(manifest.contributes?.tabs?.map((t) => t.id) ?? []);
  const declaredPanels = new Set(manifest.contributes?.sidebarPanels?.map((p) => p.id) ?? []);

  const ui: PluginUIAPI = Object.freeze({
    registerTabView(tabId: string, component: React.ComponentType<PluginTabProps>) {
      if (!declaredTabs.has(tabId)) {
        throw new Error(`Tab "${tabId}" not declared in plugin manifest contributes.tabs`);
      }
      store().registerTabView(pluginId, tabId, component);
      return createDisposable(pluginId, () => {
        const tabViews = new Map(usePluginStore.getState().tabViews);
        tabViews.delete(`${pluginId}:${tabId}`);
        usePluginStore.setState({ tabViews });
      });
    },
    registerSidebarPanel(panelId: string, component: React.ComponentType) {
      if (!declaredPanels.has(panelId)) {
        throw new Error(`Sidebar panel "${panelId}" not declared in plugin manifest contributes.sidebarPanels`);
      }
      const panelDef = manifest.contributes?.sidebarPanels?.find((p) => p.id === panelId);
      store().registerSidebarPanel(
        pluginId,
        panelId,
        component,
        panelDef?.title ?? panelId,
        panelDef?.icon ?? 'Puzzle',
        (panelDef?.position ?? 'bottom') as 'top' | 'bottom',
      );
      return createDisposable(pluginId, () => {
        const sidebarPanels = new Map(usePluginStore.getState().sidebarPanels);
        sidebarPanels.delete(`${pluginId}:${panelId}`);
        usePluginStore.setState({ sidebarPanels });
      });
    },
    openTab(tabId: string) {
      if (!declaredTabs.has(tabId)) {
        throw new Error(`Tab "${tabId}" not declared in plugin manifest contributes.tabs`);
      }
      const tabDef = manifest.contributes?.tabs?.find((t) => t.id === tabId);
      const compositeKey = `${pluginId}:${tabId}`;
      const { tabs } = useAppStore.getState();
      const existing = tabs.find((t) => t.pluginTabId === compositeKey);
      if (existing) {
        useAppStore.setState({ activeTabId: existing.id });
      } else {
        const newTab = {
          id: crypto.randomUUID(),
          type: 'plugin' as const,
          title: tabDef?.title ?? tabId,
          icon: tabDef?.icon ?? 'Puzzle',
          pluginTabId: compositeKey,
        };
        useAppStore.setState((state) => ({
          tabs: [...state.tabs, newTab],
          activeTabId: newTab.id,
        }));
      }
    },
    showToast(opts) {
      // Use the toast system from useToast hook via a global event
      pluginEventBridge.emit('plugin:toast', opts);
    },
    registerCommand(id: string, opts: { label: string; icon?: string; shortcut?: string; section?: string }, handler: () => void) {
      store().registerCommand(pluginId, { id, label: opts.label, icon: opts.icon, shortcut: opts.shortcut, section: opts.section, handler });
      return createDisposable(pluginId, () => {
        const commands = new Map(usePluginStore.getState().commands);
        commands.delete(`${pluginId}:${id}`);
        usePluginStore.setState({ commands });
      });
    },
    async showConfirm(opts) {
      return new Promise<boolean>((resolve) => {
        pluginEventBridge.emit('plugin:confirm', {
          title: opts.title,
          description: opts.description,
          resolve,
        });
      });
    },
    // v3 UI extension methods
    showNotification(opts: { title: string; body?: string; severity?: 'info' | 'warning' | 'error' }) {
      pluginEventBridge.emit('plugin:toast', {
        message: opts.title,
        description: opts.body,
        variant: opts.severity === 'error' ? 'destructive' : opts.severity === 'warning' ? 'warning' : 'default',
      });
    },
    showProgress(title: string): ProgressReporter {
      let _progress = 0;
      let _total = 100;
      let _disposed = false;
      const id = crypto.randomUUID();
      pluginEventBridge.emit('plugin:progress:start', { id, title, pluginId });
      return Object.freeze({
        report(value: number, total: number, message?: string) {
          if (_disposed) return;
          _total = total || 100;
          _progress = Math.min(_total, value);
          pluginEventBridge.emit('plugin:progress:update', { id, progress: Math.round((_progress / _total) * 100), message });
        },
      }) as ProgressReporter;
    },
    getLayout() {
      const state = useAppStore.getState();
      return Object.freeze({
        sidebarCollapsed: state.sidebarCollapsed ?? false,
        activeTabId: state.activeTabId ?? null,
        tabCount: state.tabs?.length ?? 0,
      });
    },
    onLayoutChange(handler) {
      let prev = JSON.stringify(ui.getLayout());
      const unsub = useAppStore.subscribe((state) => {
        const layout = { sidebarCollapsed: state.sidebarCollapsed ?? false, activeTabId: state.activeTabId ?? null, tabCount: state.tabs?.length ?? 0 };
        const key = JSON.stringify(layout);
        if (key !== prev) { prev = key; try { handler(Object.freeze(layout)); } catch { /* swallow */ } }
      });
      return createDisposable(pluginId, unsub);
    },
    registerContextMenu(target: ContextMenuTarget, items: ContextMenuItem[]) {
      const key = `${pluginId}:${target}:${crypto.randomUUID()}`;
      usePluginStore.setState((state) => ({
        contextMenuItems: new Map(state.contextMenuItems ?? new Map()).set(key, { pluginId, target, items }),
      }));
      return createDisposable(pluginId, () => {
        usePluginStore.setState((state) => {
          const m = new Map(state.contextMenuItems ?? new Map());
          m.delete(key);
          return { contextMenuItems: m };
        });
      });
    },
    registerStatusBarItem(options: StatusBarItemOptions): StatusBarHandle {
      const id = `${pluginId}:${(options as StatusBarItemOptions & { id?: string }).id ?? crypto.randomUUID()}`;
      let currentOptions: StatusBarItemOptions = {
        ...options,
        alignment: options.alignment ?? 'left',
      };
      const updateBar = (opts: StatusBarItemOptions) => {
        currentOptions = {
          ...opts,
          alignment: opts.alignment ?? 'left',
        };
        usePluginStore.setState((state) => ({
          statusBarItems: new Map(state.statusBarItems ?? new Map()).set(id, { pluginId, ...currentOptions }),
        }));
      };
      updateBar(currentOptions);
      const disposable = createDisposable(pluginId, () => {
        usePluginStore.setState((state) => {
          const m = new Map(state.statusBarItems ?? new Map());
          m.delete(id);
          return { statusBarItems: m };
        });
      });
      return Object.freeze({
        update(newOpts: Partial<StatusBarItemOptions>) { updateBar({ ...currentOptions, ...newOpts }); },
        dispose: disposable.dispose,
      });
    },
    registerKeybinding(keybinding: string, handler: () => void) {
      const key = `${pluginId}:${keybinding}`;
      const normalizedKey = normalizePluginKeyCombo(keybinding);
      const conflictingBuiltIn = getDefaults().find((definition) => {
        const combo = getBinding(definition.id);
        if (!combo) return false;
        return normalizePluginComboDescriptor(combo) === normalizedKey;
      });
      if (conflictingBuiltIn) {
        console.warn(
          `[PluginContextFactory] Plugin "${pluginId}" registered keybinding "${keybinding}" ` +
          `which conflicts with built-in action "${conflictingBuiltIn.id}". Built-in shortcuts keep priority.`,
        );
      }

      const conflictingPlugin = Array.from(usePluginStore.getState().keybindings.values()).find((entry) => entry.normalizedKey === normalizedKey);
      if (conflictingPlugin) {
        console.warn(
          `[PluginContextFactory] Plugin "${pluginId}" registered keybinding "${keybinding}" ` +
          `which conflicts with plugin "${conflictingPlugin.pluginId}". Earlier registrations keep priority.`,
        );
      }

      usePluginStore.setState((state) => ({
        keybindings: new Map(state.keybindings ?? new Map()).set(key, { pluginId, keybinding, normalizedKey, handler }),
      }));
      return createDisposable(pluginId, () => {
        usePluginStore.setState((state) => {
          const m = new Map(state.keybindings ?? new Map());
          m.delete(key);
          return { keybindings: m };
        });
      });
    },
  });

  // ── ctx.terminal ──────────────────────────────────────────────────
  const declaredShortcuts = new Map(
    (manifest.contributes?.terminalHooks?.shortcuts ?? []).map((s) => [s.command, s.key]),
  );

  const terminal: PluginTerminalAPI = Object.freeze({
    registerInputInterceptor(handler: InputInterceptor) {
      if (!manifest.contributes?.terminalHooks?.inputInterceptor) {
        throw new Error('inputInterceptor not declared in manifest contributes.terminalHooks');
      }
      store().registerInputInterceptor(pluginId, handler);
      return createDisposable(pluginId, () => {
        usePluginStore.setState((state) => ({
          inputInterceptors: state.inputInterceptors.filter((e) => e.handler !== handler),
        }));
      });
    },
    registerOutputProcessor(handler: OutputProcessor) {
      if (!manifest.contributes?.terminalHooks?.outputProcessor) {
        throw new Error('outputProcessor not declared in manifest contributes.terminalHooks');
      }
      store().registerOutputProcessor(pluginId, handler);
      return createDisposable(pluginId, () => {
        usePluginStore.setState((state) => ({
          outputProcessors: state.outputProcessors.filter((e) => e.handler !== handler),
        }));
      });
    },
    registerShortcut(command: string, handler: () => void) {
      const key = declaredShortcuts.get(command);
      if (!key) {
        throw new Error(`Shortcut command "${command}" not declared in manifest contributes.terminalHooks.shortcuts`);
      }
      store().registerShortcut(pluginId, command, key, handler);
      return createDisposable(pluginId, () => {
        const normalizedKey = key.toLowerCase().split('+').sort().join('+');
        const shortcuts = new Map(usePluginStore.getState().shortcuts);
        shortcuts.delete(normalizedKey);
        usePluginStore.setState({ shortcuts });
      });
    },
    writeToNode(nodeId: string, text: string) {
      const sessionId = resolveNodeToFirstSession(nodeId);
      if (!sessionId) {
        console.warn(`[PluginContext] writeToNode: no terminal session found for node "${nodeId}"`);
        return;
      }
      const paneId = findPaneBySessionId(sessionId);
      if (!paneId) {
        console.warn(`[PluginContext] writeToNode: no pane found for session "${sessionId}"`);
        return;
      }
      if (!registryWriteToTerminal(paneId, text)) {
        console.warn(`[PluginContext] writeToNode: no writer registered for pane "${paneId}"`);
      }
    },
    getNodeBuffer(nodeId: string): string | null {
      const sessionId = resolveNodeToFirstSession(nodeId);
      if (!sessionId) return null;
      const paneId = findPaneBySessionId(sessionId);
      if (!paneId) return null;
      return getTerminalBuffer(paneId) ?? null;
    },
    getNodeSelection(nodeId: string): string | null {
      const sessionId = resolveNodeToFirstSession(nodeId);
      if (!sessionId) return null;
      const paneId = findPaneBySessionId(sessionId);
      if (!paneId) return null;
      return getTerminalSelection(paneId) ?? null;
    },
    // v3 terminal search & buffer methods
    async search(nodeId: string, query: string, options?: { caseSensitive?: boolean; regex?: boolean; wholeWord?: boolean }) {
      const sessionId = resolveNodeToFirstSession(nodeId);
      if (!sessionId) return Object.freeze({ matches: [], total_matches: 0 });
      const result = await invoke<{ matches: unknown[]; total_matches: number; duration_ms: number }>('search_terminal', {
        sessionId,
        options: { query, case_sensitive: options?.caseSensitive ?? false, regex: options?.regex ?? false, whole_word: options?.wholeWord ?? false },
      });
      return Object.freeze({ matches: Object.freeze(result.matches), total_matches: result.total_matches });
    },
    async getScrollBuffer(nodeId: string, startLine: number, count: number) {
      const sessionId = resolveNodeToFirstSession(nodeId);
      if (!sessionId) return Object.freeze([]);
      const lines = await invoke<{ text: string; line_number: number }[]>('get_scroll_buffer', { sessionId, startLine, count });
      return Object.freeze(lines.map((l) => Object.freeze({ text: l.text, lineNumber: l.line_number })));
    },
    async getBufferSize(nodeId: string) {
      const sessionId = resolveNodeToFirstSession(nodeId);
      if (!sessionId) return Object.freeze({ currentLines: 0, totalLines: 0, maxLines: 0 });
      const stats = await invoke<{ current_lines: number; total_lines: number; max_lines: number; memory_usage_mb: number }>('get_buffer_stats', { sessionId });
      return Object.freeze({ currentLines: stats.current_lines, totalLines: stats.total_lines, maxLines: stats.max_lines });
    },
    async clearBuffer(nodeId: string) {
      const sessionId = resolveNodeToFirstSession(nodeId);
      if (!sessionId) return;
      await invoke('clear_buffer', { sessionId });
    },
  });

  // ── ctx.settings ──────────────────────────────────────────────────
  const settings: PluginSettingsAPI = Object.freeze({
    get<T>(key: string): T {
      return settingsManager.get<T>(key);
    },
    set<T>(key: string, value: T): void {
      settingsManager.set(key, value);
    },
    onChange(key: string, handler: (newValue: unknown) => void) {
      const unsub = settingsManager.onChange(key, handler);
      return createDisposable(pluginId, unsub);
    },
  });

  // ── ctx.i18n ──────────────────────────────────────────────────────
  const i18n: PluginI18nAPI = Object.freeze({
    t(key: string, params?: Record<string, string | number>): string {
      return i18nManager.t(key, params);
    },
    getLanguage(): string {
      return i18nManager.getLanguage();
    },
    onLanguageChange(handler: (lang: string) => void) {
      const unsub = i18nManager.onLanguageChange(handler);
      return createDisposable(pluginId, unsub);
    },
  });

  // ── ctx.storage ───────────────────────────────────────────────────
  const storageApi: PluginStorageAPI = Object.freeze({
    get<T>(key: string): T | null {
      return storage.get<T>(key);
    },
    set<T>(key: string, value: T): void {
      storage.set(key, value);
    },
    remove(key: string): void {
      storage.remove(key);
    },
  });

  const getSavedConnectionSnapshots = (): ReadonlyArray<SavedConnectionSnapshot> => {
    return Object.freeze(useAppStore.getState().savedConnections.map(toSavedConnectionSnapshot));
  };

  const refreshSavedConnectionSnapshots = async (): Promise<ReadonlyArray<SavedConnectionSnapshot>> => {
    await useAppStore.getState().loadSavedConnections();
    return getSavedConnectionSnapshots();
  };

  const resolveExportConnectionIds = async (connectionIds?: string[]): Promise<string[]> => {
    if (connectionIds && connectionIds.length > 0) {
      return [...connectionIds];
    }
    const currentConnections = useAppStore.getState().savedConnections;
    if (currentConnections.length > 0) {
      return currentConnections.map((connection) => connection.id);
    }
    const snapshots = await refreshSavedConnectionSnapshots();
    return snapshots.map((connection) => connection.id);
  };

  const sync: PluginSyncAPI = Object.freeze({
    listSavedConnections(): ReadonlyArray<SavedConnectionSnapshot> {
      return getSavedConnectionSnapshots();
    },
    async refreshSavedConnections(): Promise<ReadonlyArray<SavedConnectionSnapshot>> {
      return refreshSavedConnectionSnapshots();
    },
    onSavedConnectionsChange(handler) {
      let previous = useAppStore.getState().savedConnections;
      const unsub = useAppStore.subscribe((state) => {
        if (state.savedConnections !== previous) {
          previous = state.savedConnections;
          try { handler(getSavedConnectionSnapshots()); } catch { /* swallow */ }
        }
      });
      return createDisposable(pluginId, unsub);
    },
    async preflightExport(connectionIds?: string[], options?: { embedKeys?: boolean }): Promise<Readonly<ExportPreflightResult>> {
      const effectiveIds = await resolveExportConnectionIds(connectionIds);
      const result = await invoke<ExportPreflightResult>('preflight_export', {
        connectionIds: effectiveIds,
        embedKeys: options?.embedKeys ?? null,
      });
      return toFrozenExportPreflight(result);
    },
    async exportOxide(request): Promise<Uint8Array> {
      const effectiveIds = await resolveExportConnectionIds(request.connectionIds);
      const fileData = await invoke<number[]>('export_to_oxide', {
        connectionIds: effectiveIds,
        password: request.password,
        description: request.description ?? null,
        embedKeys: request.embedKeys ?? null,
      });
      return new Uint8Array(fileData);
    },
    async validateOxide(fileData: Uint8Array): Promise<Readonly<OxideMetadata>> {
      const metadata = await invoke<OxideMetadata>('validate_oxide_file', {
        fileData: Array.from(fileData),
      });
      return toFrozenOxideMetadata(metadata);
    },
    async previewImport(fileData: Uint8Array, password: string, options): Promise<Readonly<ImportPreview>> {
      const preview = await invoke<ImportPreview>('preview_oxide_import', {
        fileData: Array.from(fileData),
        password,
        conflictStrategy: options?.conflictStrategy ?? null,
      });
      return toFrozenImportPreview(preview);
    },
    async importOxide(fileData: Uint8Array, password: string, options): Promise<Readonly<ImportResult>> {
      const result = await invoke<ImportResult>('import_from_oxide', {
        fileData: Array.from(fileData),
        password,
        selectedNames: options?.selectedNames ?? null,
        conflictStrategy: options?.conflictStrategy ?? null,
      });

      await useAppStore.getState().loadSavedConnections();
      return toFrozenImportResult(result);
    },
  });

  const secrets: PluginSecretsAPI = Object.freeze({
    async get(key: string): Promise<string | null> {
      return invoke<string | null>('get_plugin_secret', {
        pluginId,
        key,
      });
    },
    async set(key: string, value: string): Promise<void> {
      await invoke('set_plugin_secret', {
        pluginId,
        key,
        value,
      });
    },
    async has(key: string): Promise<boolean> {
      return invoke<boolean>('has_plugin_secret', {
        pluginId,
        key,
      });
    },
    async delete(key: string): Promise<void> {
      await invoke('delete_plugin_secret', {
        pluginId,
        key,
      });
    },
  });

  // ── ctx.api ───────────────────────────────────────────────────────
  //
  // SECURITY NOTE: This whitelist is advisory, not a hard boundary.
  // Plugins run in the same JS context and can theoretically import
  // @tauri-apps/api/core directly to bypass this check. True sandboxing
  // would require an iframe or WebWorker with a message-passing bridge.
  // The whitelist is defense-in-depth: it prevents accidental misuse
  // and makes intentional abuse explicit (detectable via code review).
  //
  const allowedCommands = new Set(manifest.contributes?.apiCommands ?? []);

  const api: PluginBackendAPI = Object.freeze({
    async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
      if (!allowedCommands.has(command)) {
        console.warn(
          `[PluginContext] Plugin "${pluginId}" attempted to invoke undeclared command "${command}". ` +
          `Declare it in manifest contributes.apiCommands to allow.`,
        );
        throw new Error(`Command "${command}" not whitelisted in manifest contributes.apiCommands`);
      }
      return invoke(command, args);
    },
  });

  // ── ctx.assets ──────────────────────────────────────────────────
  const assets: PluginAssetsAPI = Object.freeze({
    async loadCSS(relativePath: string): Promise<Disposable> {
      const cssPath = relativePath.replace(/^\.\//, '');
      const fileBytes: number[] = await invoke('read_plugin_file', {
        pluginId, relativePath: cssPath,
      });
      const cssText = new TextDecoder().decode(new Uint8Array(fileBytes));

      const styleEl = document.createElement('style');
      styleEl.setAttribute('data-plugin', pluginId);
      styleEl.setAttribute('data-path', cssPath);
      styleEl.textContent = cssText;
      document.head.appendChild(styleEl);

      return createDisposable(pluginId, () => { styleEl.remove(); });
    },

    async getAssetUrl(relativePath: string): Promise<string> {
      const assetPath = relativePath.replace(/^\.\//, '');
      const fileBytes: number[] = await invoke('read_plugin_file', {
        pluginId, relativePath: assetPath,
      });

      const ext = assetPath.split('.').pop()?.toLowerCase() ?? '';
      const mime = MIME_MAP[ext] ?? 'application/octet-stream';
      const blob = new Blob([new Uint8Array(fileBytes)], { type: mime });
      const url = URL.createObjectURL(blob);

      if (!activeAssetUrls.has(pluginId)) {
        activeAssetUrls.set(pluginId, new Set());
      }
      activeAssetUrls.get(pluginId)!.add(url);
      return url;
    },

    revokeAssetUrl(url: string): void {
      URL.revokeObjectURL(url);
      activeAssetUrls.get(pluginId)?.delete(url);
    },
  });

  // ── ctx.sftp ───────────────────────────────────────────────────────
  //
  // Wraps node_sftp_* backend commands. Each method requires a nodeId
  // (not sessionId) so it survives reconnects. The plugin must have an
  // active connection on the node — backend enforces State Gating.
  //
  const sftp: PluginSftpAPI = Object.freeze({
    async listDir(nodeId: string, path: string): Promise<ReadonlyArray<PluginFileInfo>> {
      const items = await invoke<PluginFileInfo[]>('node_sftp_list_dir', { nodeId, path });
      return Object.freeze(items.map((i) => Object.freeze(i)));
    },
    async stat(nodeId: string, path: string): Promise<PluginFileInfo> {
      const info: PluginFileInfo = await invoke('node_sftp_stat', { nodeId, path });
      return Object.freeze(info);
    },
    async readFile(nodeId: string, path: string): Promise<string> {
      // Use preview with text extraction — safe and size-limited
      const preview = await invoke<{ Text?: { data: string } }>('node_sftp_preview', { nodeId, path });
      if (preview && typeof preview === 'object' && 'Text' in preview) {
        return (preview as { Text: { data: string } }).Text.data;
      }
      throw new Error('File is not a text file or exceeds size limit');
    },
    async writeFile(nodeId: string, path: string, content: string): Promise<void> {
      await invoke('node_sftp_write', { nodeId, path, content });
    },
    async mkdir(nodeId: string, path: string): Promise<void> {
      await invoke('node_sftp_mkdir', { nodeId, path });
    },
    async delete(nodeId: string, path: string): Promise<void> {
      await invoke('node_sftp_delete', { nodeId, path });
    },
    async rename(nodeId: string, oldPath: string, newPath: string): Promise<void> {
      await invoke('node_sftp_rename', { nodeId, oldPath, newPath });
    },
  });

  // ── ctx.forward ───────────────────────────────────────────────────
  //
  // Wraps port forwarding backend commands. Uses sessionId (not nodeId)
  // because forwarding is bound to the SSH session lifecycle.
  //
  const forward: PluginForwardAPI = Object.freeze({
    async list(sessionId: string): Promise<ReadonlyArray<PluginForwardRule>> {
      const rules = await invoke<PluginForwardRule[]>('list_port_forwards', { sessionId });
      return Object.freeze(rules.map((r) => Object.freeze(r)));
    },
    async create(request) {
      const backendRequest = {
        session_id: request.sessionId,
        forward_type: request.forwardType,
        bind_address: request.bindAddress,
        bind_port: request.bindPort,
        target_host: request.targetHost,
        target_port: request.targetPort,
        description: request.description,
      };
      const resp = await invoke<{ success: boolean; forward?: PluginForwardRule; error?: string }>(
        'create_port_forward', { request: backendRequest },
      );
      return Object.freeze({
        success: resp.success,
        forward: resp.forward ? Object.freeze(resp.forward) : undefined,
        error: resp.error,
      });
    },
    async stop(sessionId: string, forwardId: string): Promise<void> {
      await invoke('stop_port_forward', { sessionId, forwardId });
    },
    async stopAll(sessionId: string): Promise<void> {
      await invoke('stop_all_forwards', { sessionId });
    },
    async getStats(sessionId: string, forwardId: string) {
      const stats = await invoke<{
        connection_count: number;
        active_connections: number;
        bytes_sent: number;
        bytes_received: number;
      } | null>('get_port_forward_stats', { sessionId, forwardId });
      if (!stats) return null;
      return Object.freeze({
        connectionCount: stats.connection_count,
        activeConnections: stats.active_connections,
        bytesSent: stats.bytes_sent,
        bytesReceived: stats.bytes_received,
      });
    },
  });

  // ── ctx.sessions (v3) ──────────────────────────────────────────────
  //
  // Read-only session tree access. Data is snapshots frozen on read.
  //
  const sessions: PluginSessionsAPI = Object.freeze({
    getTree(): ReadonlyArray<SessionTreeNodeSnapshot> {
      const nodes = useSessionTreeStore.getState().nodes;
      const childMap = new Map<string, string[]>();
      for (const n of nodes) {
        if (n.parentId) {
          const siblings = childMap.get(n.parentId) ?? [];
          siblings.push(n.id);
          childMap.set(n.parentId, siblings);
        }
      }
      return Object.freeze(nodes.map((n) => {
        return freezeSnapshot<SessionTreeNodeSnapshot>({
          id: n.id,
          label: n.displayName || `${n.username}@${n.host}`,
          host: n.host,
          port: n.port,
          username: n.username,
          parentId: n.parentId,
          childIds: Object.freeze(childMap.get(n.id) ?? []),
          connectionState: n.runtime.status,
          connectionId: n.runtime.connectionId ?? null,
          terminalIds: Object.freeze([...n.runtime.terminalIds]),
          sftpSessionId: n.runtime.sftpSessionId,
          errorMessage: n.runtime.errorMessage,
        });
      }));
    },
    getActiveNodes() {
      const nodes = useSessionTreeStore.getState().nodes;
      return Object.freeze(
        nodes
          .filter((n) => n.runtime.status === 'active' || n.runtime.status === 'connected')
          .map((n) => Object.freeze({
            nodeId: n.id,
            sessionId: n.runtime.terminalIds[0] ?? null,
            connectionState: n.runtime.status,
          })),
      );
    },
    getNodeState(nodeId: string): string | null {
      const node = useSessionTreeStore.getState().getNode(nodeId);
      return node ? node.runtime.status : null;
    },
    onTreeChange(handler) {
      const unsub = useSessionTreeStore.subscribe(
        (state) => state.nodes,
        () => {
          try { handler(sessions.getTree()); } catch { /* swallow */ }
        },
      );
      return createDisposable(pluginId, unsub);
    },
    onNodeStateChange(nodeId: string, handler) {
      let prevStatus: string | null = null;
      const unsub = useSessionTreeStore.subscribe(
        (state) => state.getNode(nodeId)?.runtime.status ?? null,
        (status) => {
          if (status !== prevStatus) {
            prevStatus = status;
            try { handler(status ?? 'idle'); } catch { /* swallow */ }
          }
        },
      );
      return createDisposable(pluginId, unsub);
    },
  });

  // ── ctx.transfers (v3) ────────────────────────────────────────────
  //
  // SFTP transfer monitoring with throttled progress events.
  //
  function toTransferSnapshot(t: { id: string; nodeId: string; name: string; localPath: string; remotePath: string; direction: string; size: number; transferred: number; state: string; error?: string; startTime: number; endTime?: number }): TransferSnapshot {
    return freezeSnapshot<TransferSnapshot>({
      id: t.id,
      nodeId: t.nodeId,
      name: t.name,
      localPath: t.localPath,
      remotePath: t.remotePath,
      direction: t.direction as 'upload' | 'download',
      size: t.size,
      transferred: t.transferred,
      state: t.state as TransferSnapshot['state'],
      error: t.error,
      startTime: t.startTime,
      endTime: t.endTime,
    });
  }

  const transfers: PluginTransfersAPI = Object.freeze({
    getAll(): ReadonlyArray<TransferSnapshot> {
      if (!_useTransferStore) return Object.freeze([]);
      const items = _useTransferStore.getState().transfers;
      return Object.freeze(Array.from(items.values()).map(toTransferSnapshot));
    },
    getByNode(nodeId: string): ReadonlyArray<TransferSnapshot> {
      if (!_useTransferStore) return Object.freeze([]);
      const items = _useTransferStore.getState().transfers;
      return Object.freeze(
        Array.from(items.values())
          .filter((t) => t.nodeId === nodeId)
          .map(toTransferSnapshot),
      );
    },
    onProgress(handler) {
      const throttled = createThrottledEmitter<TransferSnapshot>(500, handler);
      let unsub: (() => void) | null = null;
      void getTransferStore().then((store) => {
        unsub = store.subscribe((state) => {
          for (const t of state.transfers.values()) {
            if (t.state === 'active') throttled.push(toTransferSnapshot(t));
          }
        });
      });
      return createDisposable(pluginId, () => { unsub?.(); throttled.dispose(); });
    },
    onComplete(handler) {
      let unsub: (() => void) | null = null;
      let prevStates = new Map<string, string>();
      void getTransferStore().then((store) => {
        prevStates = new Map(Array.from(store.getState().transfers.entries()).map(([k, v]) => [k, v.state]));
        unsub = store.subscribe((state) => {
          const transfers = state.transfers;
          for (const [id, t] of transfers) {
            if (t.state === 'completed' && prevStates.get(id) !== 'completed') {
              try { handler(toTransferSnapshot(t)); } catch { /* swallow */ }
            }
          }
          prevStates = new Map(Array.from(transfers.entries()).map(([k, v]) => [k, v.state]));
        });
      });
      return createDisposable(pluginId, () => { unsub?.(); });
    },
    onError(handler) {
      let unsub: (() => void) | null = null;
      let prevStates = new Map<string, string>();
      void getTransferStore().then((store) => {
        prevStates = new Map(Array.from(store.getState().transfers.entries()).map(([k, v]) => [k, v.state]));
        unsub = store.subscribe((state) => {
          const transfers = state.transfers;
          for (const [id, t] of transfers) {
            if (t.state === 'error' && prevStates.get(id) !== 'error') {
              try { handler(toTransferSnapshot(t)); } catch { /* swallow */ }
            }
          }
          prevStates = new Map(Array.from(transfers.entries()).map(([k, v]) => [k, v.state]));
        });
      });
      return createDisposable(pluginId, () => { unsub?.(); });
    },
  });

  // ── ctx.profiler (v3) ─────────────────────────────────────────────
  //
  // Resource monitoring with throttled metrics push.
  //
  function toMetricsSnapshot(m: { timestampMs: number; cpuPercent: number | null; memoryUsed: number | null; memoryTotal: number | null; memoryPercent: number | null; loadAvg1: number | null; loadAvg5: number | null; loadAvg15: number | null; cpuCores: number | null; netRxBytesPerSec: number | null; netTxBytesPerSec: number | null; sshRttMs: number | null }): ProfilerMetricsSnapshot {
    return freezeSnapshot<ProfilerMetricsSnapshot>({
      timestampMs: m.timestampMs,
      cpuPercent: m.cpuPercent,
      memoryUsed: m.memoryUsed,
      memoryTotal: m.memoryTotal,
      memoryPercent: m.memoryPercent,
      loadAvg1: m.loadAvg1,
      loadAvg5: m.loadAvg5,
      loadAvg15: m.loadAvg15,
      cpuCores: m.cpuCores,
      netRxBytesPerSec: m.netRxBytesPerSec,
      netTxBytesPerSec: m.netTxBytesPerSec,
      sshRttMs: m.sshRttMs,
    });
  }

  const profiler: PluginProfilerAPI = Object.freeze({
    getMetrics(nodeId: string): ProfilerMetricsSnapshot | null {
      if (!_useProfilerStore) return null;
      const connState = _useProfilerStore.getState().connections.get(nodeId);
      return connState?.metrics ? toMetricsSnapshot(connState.metrics) : null;
    },
    getHistory(nodeId: string, maxPoints?: number): ReadonlyArray<ProfilerMetricsSnapshot> {
      if (!_useProfilerStore) return Object.freeze([]);
      const connState = _useProfilerStore.getState().connections.get(nodeId);
      if (!connState) return Object.freeze([]);
      const history = maxPoints ? connState.history.slice(-maxPoints) : connState.history;
      return Object.freeze(history.map(toMetricsSnapshot));
    },
    isRunning(nodeId: string): boolean {
      if (!_useProfilerStore) return false;
      const connState = _useProfilerStore.getState().connections.get(nodeId);
      return connState?.isRunning ?? false;
    },
    onMetrics(nodeId: string, handler) {
      const throttled = createThrottledEmitter<ProfilerMetricsSnapshot>(1000, handler);
      let unsub: (() => void) | null = null;
      let prevTs = 0;
      void getProfilerStore().then((store) => {
        unsub = store.subscribe((state) => {
          const metrics = state.connections.get(nodeId)?.metrics ?? null;
          if (metrics && metrics.timestampMs !== prevTs) {
            prevTs = metrics.timestampMs;
            throttled.push(toMetricsSnapshot(metrics));
          }
        });
      });
      return createDisposable(pluginId, () => { unsub?.(); throttled.dispose(); });
    },
  });

  // ── ctx.eventLog (v3) ─────────────────────────────────────────────
  //
  // Read-only event log access.
  //
  function toEventLogSnapshot(e: { id: number; timestamp: number; severity: string; category: string; nodeId?: string; connectionId?: string; title: string; detail?: string; source: string }): EventLogEntrySnapshot {
    return freezeSnapshot<EventLogEntrySnapshot>({
      id: e.id,
      timestamp: e.timestamp,
      severity: e.severity as EventLogEntrySnapshot['severity'],
      category: e.category as EventLogEntrySnapshot['category'],
      nodeId: e.nodeId,
      connectionId: e.connectionId,
      title: e.title,
      detail: e.detail,
      source: e.source,
    });
  }

  const eventLog: PluginEventLogAPI = Object.freeze({
    getEntries(filter?) {
      if (!_useEventLogStore) return Object.freeze([]);
      let entries = _useEventLogStore.getState().entries;
      if (filter?.severity) entries = entries.filter((e) => e.severity === filter.severity);
      if (filter?.category) entries = entries.filter((e) => e.category === filter.category);
      return Object.freeze(entries.map(toEventLogSnapshot));
    },
    onEntry(handler) {
      let unsub: (() => void) | null = null;
      let prevLength = 0;
      void getEventLogStore().then((store) => {
        prevLength = store.getState().entries.length;
        unsub = store.subscribe((state) => {
          const entries = state.entries;
          if (entries.length > prevLength) {
            for (let i = prevLength; i < entries.length; i++) {
              try { handler(toEventLogSnapshot(entries[i])); } catch { /* swallow */ }
            }
          }
          prevLength = entries.length;
        });
      });
      return createDisposable(pluginId, () => { unsub?.(); });
    },
  });

  // ── ctx.ide (v3) ──────────────────────────────────────────────────
  //
  // Read-only IDE mode access.
  //
  function toIdeFileSnapshot(tab: { path: string; name: string; language: string; isDirty: boolean; isPinned: boolean }, isActive: boolean): IdeFileSnapshot {
    return freezeSnapshot<IdeFileSnapshot>({
      path: tab.path,
      name: tab.name,
      language: tab.language,
      isDirty: tab.isDirty,
      isActive,
      isPinned: tab.isPinned,
    });
  }

  const ide: PluginIdeAPI = Object.freeze({
    isOpen(): boolean {
      return _useIdeStore ? _useIdeStore.getState().nodeId !== null : false;
    },
    getProject(): IdeProjectSnapshot | null {
      if (!_useIdeStore) return null;
      const state = _useIdeStore.getState();
      if (!state.project || !state.nodeId) return null;
      return freezeSnapshot<IdeProjectSnapshot>({
        nodeId: state.nodeId,
        rootPath: state.project.rootPath,
        name: state.project.name,
        isGitRepo: state.project.isGitRepo,
        gitBranch: state.project.gitBranch,
      });
    },
    getOpenFiles(): ReadonlyArray<IdeFileSnapshot> {
      if (!_useIdeStore) return Object.freeze([]);
      const state = _useIdeStore.getState();
      return Object.freeze(
        state.tabs.map((tab) => toIdeFileSnapshot(tab, tab.id === state.activeTabId)),
      );
    },
    getActiveFile(): IdeFileSnapshot | null {
      if (!_useIdeStore) return null;
      const state = _useIdeStore.getState();
      const tab = state.tabs.find((t) => t.id === state.activeTabId);
      return tab ? toIdeFileSnapshot(tab, true) : null;
    },
    onFileOpen(handler) {
      let unsub: (() => void) | null = null;
      let prevTabIds = new Set<string>();
      void getIdeStore().then((store) => {
        prevTabIds = new Set(store.getState().tabs.map((t) => t.id));
        unsub = store.subscribe(
          (state) => state.tabs,
          (tabs) => {
            const activeId = _useIdeStore?.getState().activeTabId ?? null;
            for (const tab of tabs) {
              if (!prevTabIds.has(tab.id)) {
                try { handler(toIdeFileSnapshot(tab, tab.id === activeId)); } catch { /* swallow */ }
              }
            }
            prevTabIds = new Set(tabs.map((t) => t.id));
          },
        );
      });
      return createDisposable(pluginId, () => { unsub?.(); });
    },
    onFileClose(handler) {
      let unsub: (() => void) | null = null;
      let prevTabIds = new Set<string>();
      let prevTabPaths = new Map<string, string>();
      void getIdeStore().then((store) => {
        const tabs = store.getState().tabs;
        prevTabIds = new Set(tabs.map((t) => t.id));
        prevTabPaths = new Map(tabs.map((t) => [t.id, t.path]));
        unsub = store.subscribe(
          (state) => state.tabs,
          (tabs) => {
            const currentIds = new Set(tabs.map((t) => t.id));
            for (const id of prevTabIds) {
              if (!currentIds.has(id)) {
                const path = prevTabPaths.get(id);
                if (path) { try { handler(path); } catch { /* swallow */ } }
              }
            }
            prevTabIds = currentIds;
            prevTabPaths = new Map(tabs.map((t) => [t.id, t.path]));
          },
        );
      });
      return createDisposable(pluginId, () => { unsub?.(); });
    },
    onActiveFileChange(handler) {
      let unsub: (() => void) | null = null;
      void getIdeStore().then((store) => {
        unsub = store.subscribe(
          (state) => state.activeTabId,
          () => {
            const state = _useIdeStore?.getState();
            if (!state) return;
            const tab = state.tabs.find((t) => t.id === state.activeTabId);
            try { handler(tab ? toIdeFileSnapshot(tab, true) : null); } catch { /* swallow */ }
          },
        );
      });
      return createDisposable(pluginId, () => { unsub?.(); });
    },
  });

  // ── ctx.ai (v3) ───────────────────────────────────────────────────
  //
  // Read-only AI chat access. Message content is provided as-is but
  // plugins should treat terminal buffer context as sensitive data.
  //
  const ai: PluginAiAPI = Object.freeze({
    getConversations(): ReadonlyArray<AiConversationSnapshot> {
      if (!_useAiChatStore) return Object.freeze([]);
      const conversations = _useAiChatStore.getState().conversations;
      return Object.freeze(conversations.map((c) => freezeSnapshot<AiConversationSnapshot>({
        id: c.id,
        title: c.title,
        messageCount: c.messages.length,
        createdAt: c.createdAt,
        updatedAt: c.updatedAt,
      })));
    },
    getMessages(conversationId: string): ReadonlyArray<AiMessageSnapshot> {
      if (!_useAiChatStore) return Object.freeze([]);
      const conv = _useAiChatStore.getState().conversations.find((c) => c.id === conversationId);
      if (!conv) return Object.freeze([]);
      return Object.freeze(conv.messages.map((m) => freezeSnapshot<AiMessageSnapshot>({
        id: m.id,
        role: m.role,
        content: m.content,
        timestamp: m.timestamp,
      })));
    },
    getActiveProvider(): Readonly<{ type: string; displayName: string }> | null {
      if (!_useSettingsStore) return null;
      const s = _useSettingsStore.getState().settings;
      const providerId = s.ai.activeProviderId;
      if (!providerId) return null;
      const provider = s.ai.providers?.find((p: { id: string; type: string; name: string }) => p.id === providerId);
      if (!provider) return null;
      return Object.freeze({ type: provider.type, displayName: provider.name });
    },
    getAvailableModels(): ReadonlyArray<string> {
      if (!_useSettingsStore) return Object.freeze([]);
      const s = _useSettingsStore.getState().settings;
      const providerId = s.ai.activeProviderId;
      if (!providerId || !s.ai.modelContextWindows) return Object.freeze([]);
      const models = s.ai.modelContextWindows[providerId];
      return models ? Object.freeze(Object.keys(models)) : Object.freeze([]);
    },
    onMessage(handler) {
      let unsub: (() => void) | null = null;
      let prevMessageCounts = new Map<string, number>();
      void getAiChatStore().then((store) => {
        prevMessageCounts = new Map(store.getState().conversations.map((c: { id: string; messages: unknown[] }) => [c.id, c.messages.length]));
        unsub = store.subscribe((state) => {
          const conversations = state.conversations as Array<{ id: string; messages: Array<{ id: string; role: string }> }>;
          for (const conv of conversations) {
            const prevCount = prevMessageCounts.get(conv.id) ?? 0;
            if (conv.messages.length > prevCount) {
              const newMsg = conv.messages[conv.messages.length - 1];
              try {
                handler(Object.freeze({
                  conversationId: conv.id,
                  messageId: newMsg.id,
                  role: newMsg.role,
                }));
              } catch { /* swallow */ }
            }
          }
          prevMessageCounts = new Map(conversations.map((c) => [c.id, c.messages.length]));
        });
      });
      return createDisposable(pluginId, () => { unsub?.(); });
    },
  });

  // ── ctx.app (v3) ──────────────────────────────────────────────────
  //
  // Application-level read-only information.
  //
  const app: PluginAppAPI = Object.freeze({
    getTheme(): ThemeSnapshot {
      if (!_useSettingsStore) return Object.freeze({ name: 'default', isDark: true });
      const terminal = _useSettingsStore.getState().settings.terminal;
      const theme = terminal?.theme ?? 'default';
      // Dark detection: themes without "light" in name are assumed dark
      return Object.freeze({ name: theme, isDark: !theme.toLowerCase().includes('light') });
    },
    getSettings(category: 'terminal' | 'appearance' | 'general' | 'buffer' | 'sftp' | 'reconnect'): Readonly<Record<string, unknown>> {
      if (!_useSettingsStore) return Object.freeze({});
      const s = _useSettingsStore.getState().settings;
      const section = s[category as keyof typeof s];
      return section && typeof section === 'object' ? freezeSnapshot({ ...(section as unknown as Record<string, unknown>) }) : Object.freeze({});
    },
    getVersion(): string {
      return window.__OXIDE__?.version ?? '0.0.0';
    },
    getPlatform(): 'macos' | 'windows' | 'linux' {
      const ua = navigator.userAgent.toLowerCase();
      if (ua.includes('mac')) return 'macos';
      if (ua.includes('win')) return 'windows';
      return 'linux';
    },
    getLocale(): string {
      if (!_useSettingsStore) return 'en';
      return _useSettingsStore.getState().settings.general?.language ?? 'en';
    },
    onThemeChange(handler) {
      let unsub: (() => void) | null = null;
      void getSettingsStore().then((store) => {
        unsub = store.subscribe(
          (state) => state.settings.terminal?.theme,
          (theme) => {
            if (theme) {
              try { handler(Object.freeze({ name: theme, isDark: !theme.toLowerCase().includes('light') })); } catch { /* swallow */ }
            }
          },
        );
      });
      return createDisposable(pluginId, () => { unsub?.(); });
    },
    onSettingsChange(category, handler) {
      let unsub: (() => void) | null = null;
      void getSettingsStore().then((store) => {
        unsub = store.subscribe(
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (state) => (state.settings as any)[category],
          (section) => {
            if (section) {
              try { handler(freezeSnapshot({ ...section } as Record<string, unknown>)); } catch { /* swallow */ }
            }
          },
        );
      });
      return createDisposable(pluginId, () => { unsub?.(); });
    },
    async getPoolStats() {
      try {
        const stats = await invoke<{ active_connections: number; total_sessions: number }>('ssh_get_pool_stats');
        return Object.freeze({
          activeConnections: stats.active_connections,
          totalSessions: stats.total_sessions,
        });
      } catch {
        return Object.freeze({ activeConnections: 0, totalSessions: 0 });
      }
    },
  });

  // ── Build final frozen context ────────────────────────────────────
  return Object.freeze({
    pluginId,
    connections,
    events,
    ui,
    terminal,
    settings,
    i18n,
    storage: storageApi,
    sync,
    secrets,
    api,
    assets,
    sftp,
    forward,
    // v3 namespaces
    sessions,
    transfers,
    profiler,
    eventLog,
    ide,
    ai,
    app,
  });
}
