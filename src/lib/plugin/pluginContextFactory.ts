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
  PluginBackendAPI,
  PluginAssetsAPI,
  ConnectionSnapshot,
  Disposable,
  InputInterceptor,
  OutputProcessor,
  PluginTabProps,
} from '../../types/plugin';
import type { SshConnectionState } from '../../types';
import { useAppStore } from '../../store/appStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { usePluginStore } from '../../store/pluginStore';
import { createPluginStorage } from './pluginStorage';
import { pluginEventBridge } from './pluginEventBridge';
import { createPluginSettingsManager } from './pluginSettingsManager';
import { createPluginI18nManager } from './pluginI18nManager';
import { toSnapshot } from './pluginUtils';
import {
  findPaneBySessionId,
  getTerminalBuffer,
  getTerminalSelection,
  writeToTerminal as registryWriteToTerminal,
} from '../terminalRegistry';
import { invoke } from '@tauri-apps/api/core';

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
    async showConfirm(opts) {
      return new Promise<boolean>((resolve) => {
        // Emit to PluginConfirmDialog via event bridge; the component
        // renders a themed Radix dialog and resolves the promise.
        pluginEventBridge.emit('plugin:confirm', {
          title: opts.title,
          description: opts.description,
          resolve,
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
    api,
    assets,
  });
}
