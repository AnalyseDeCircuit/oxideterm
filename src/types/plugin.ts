/**
 * OxideTerm Plugin System — Type Definitions
 *
 * All types for the runtime dynamic plugin system.
 * Plugins are ESM bundles loaded at runtime via Blob URL + dynamic import().
 */

import type { SshConnectionState } from './index';

// ═══════════════════════════════════════════════════════════════════════════
// Plugin Manifest (plugin.json)
// ═══════════════════════════════════════════════════════════════════════════

/** Tab contribution declared in plugin.json */
export type PluginTabDef = {
  id: string;
  title: string;       // i18n key
  icon: string;        // lucide-react icon name
};

/** Sidebar panel contribution declared in plugin.json */
export type PluginSidebarDef = {
  id: string;
  title: string;       // i18n key
  icon: string;        // lucide-react icon name
  position: 'top' | 'bottom';
};

/** Plugin setting contribution declared in plugin.json */
export type PluginSettingDef = {
  id: string;
  type: 'string' | 'number' | 'boolean' | 'select';
  default: unknown;
  title: string;       // i18n key
  description?: string; // i18n key
  options?: Array<{ label: string; value: string | number }>;
};

/** Terminal hooks contribution declared in plugin.json */
export type PluginTerminalHooksDef = {
  inputInterceptor?: boolean;
  outputProcessor?: boolean;
  shortcuts?: Array<{ key: string; command: string }>;
};

/** Connection lifecycle hooks the plugin subscribes to */
export type ConnectionHookType = 'onConnect' | 'onDisconnect' | 'onReconnect' | 'onLinkDown' | 'onIdle';

/** The plugin.json manifest loaded from disk */
export type PluginManifest = {
  id: string;
  name: string;
  version: string;
  description?: string;
  author?: string;
  main: string;                           // relative path to ESM entry
  engines?: { oxideterm?: string };

  contributes?: {
    tabs?: PluginTabDef[];
    sidebarPanels?: PluginSidebarDef[];
    settings?: PluginSettingDef[];
    terminalHooks?: PluginTerminalHooksDef;
    connectionHooks?: ConnectionHookType[];
    apiCommands?: string[];               // Tauri command whitelist
  };

  locales?: string;                       // relative path to locales dir
};

// ═══════════════════════════════════════════════════════════════════════════
// Plugin Lifecycle
// ═══════════════════════════════════════════════════════════════════════════

/** Plugin runtime state */
export type PluginState = 'inactive' | 'loading' | 'active' | 'error' | 'disabled';

/** Runtime info for a loaded plugin */
export type PluginInfo = {
  manifest: PluginManifest;
  state: PluginState;
  error?: string;
  /** JS module reference (holds activate/deactivate) */
  module?: PluginModule;
};

/** The ESM module a plugin must export */
export type PluginModule = {
  activate: (ctx: PluginContext) => void | Promise<void>;
  deactivate?: () => void | Promise<void>;
};

// ═══════════════════════════════════════════════════════════════════════════
// Disposable
// ═══════════════════════════════════════════════════════════════════════════

/** Returned by every registration call — call dispose() to unregister */
export type Disposable = {
  dispose(): void;
};

// ═══════════════════════════════════════════════════════════════════════════
// Connection Snapshot (frozen, read-only)
// ═══════════════════════════════════════════════════════════════════════════

/** Immutable snapshot of a connection, derived from SshConnectionInfo */
export type ConnectionSnapshot = Readonly<{
  id: string;
  host: string;
  port: number;
  username: string;
  state: SshConnectionState;
  refCount: number;
  keepAlive: boolean;
  createdAt: string;
  lastActive: string;
  terminalIds: readonly string[];
  parentConnectionId?: string;
}>;

// ═══════════════════════════════════════════════════════════════════════════
// Terminal Hook Types
// ═══════════════════════════════════════════════════════════════════════════

/** Context passed to terminal hooks */
export type TerminalHookContext = {
  sessionId: string;
};

/**
 * Input interceptor — receives user keystroke data before it's sent to remote.
 * Return modified string, or null to suppress the input entirely.
 */
export type InputInterceptor = (
  data: string,
  context: TerminalHookContext,
) => string | null;

/**
 * Output processor — receives raw terminal output after arriving from remote.
 * Return modified data (must be same length semantics).
 */
export type OutputProcessor = (
  data: Uint8Array,
  context: TerminalHookContext,
) => Uint8Array;

// ═══════════════════════════════════════════════════════════════════════════
// PluginContext API Namespace Interfaces
// ═══════════════════════════════════════════════════════════════════════════

/** ctx.connections — read-only connection state */
export type PluginConnectionsAPI = {
  getAll(): ReadonlyArray<ConnectionSnapshot>;
  get(connectionId: string): ConnectionSnapshot | null;
  getState(connectionId: string): SshConnectionState | null;
};

/** ctx.events — lifecycle events + inter-plugin communication */
export type PluginEventsAPI = {
  onConnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onDisconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onLinkDown(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onReconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onIdle(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onSessionCreated(handler: (info: { sessionId: string; connectionId: string }) => void): Disposable;
  onSessionClosed(handler: (info: { sessionId: string }) => void): Disposable;
  /** Inter-plugin events (namespaced automatically as plugin:{pluginId}:{name}) */
  on(name: string, handler: (data: unknown) => void): Disposable;
  emit(name: string, data: unknown): void;
};

/** Props passed to plugin tab components */
export type PluginTabProps = {
  tabId: string;
  pluginId: string;
};

/** ctx.ui — view registration and user interaction */
export type PluginUIAPI = {
  registerTabView(tabId: string, component: React.ComponentType<PluginTabProps>): Disposable;
  registerSidebarPanel(panelId: string, component: React.ComponentType): Disposable;
  openTab(tabId: string): void;
  showToast(opts: {
    title: string;
    description?: string;
    variant?: 'default' | 'success' | 'error' | 'warning';
  }): void;
  showConfirm(opts: { title: string; description: string }): Promise<boolean>;
};

/** ctx.terminal — terminal hooks and utilities */
export type PluginTerminalAPI = {
  registerInputInterceptor(handler: InputInterceptor): Disposable;
  registerOutputProcessor(handler: OutputProcessor): Disposable;
  registerShortcut(command: string, handler: () => void): Disposable;
  writeToTerminal(sessionId: string, text: string): void;
  getBuffer(sessionId: string): string | null;
  getSelection(sessionId: string): string | null;
};

/** ctx.settings — plugin-scoped settings */
export type PluginSettingsAPI = {
  get<T>(key: string): T;
  set<T>(key: string, value: T): void;
  onChange(key: string, handler: (newValue: unknown) => void): Disposable;
};

/** ctx.i18n — plugin-scoped i18n */
export type PluginI18nAPI = {
  t(key: string, params?: Record<string, string | number>): string;
  getLanguage(): string;
  onLanguageChange(handler: (lang: string) => void): Disposable;
};

/** ctx.storage — plugin-scoped persistent KV */
export type PluginStorageAPI = {
  get<T>(key: string): T | null;
  set<T>(key: string, value: T): void;
  remove(key: string): void;
};

/** ctx.api — restricted backend calls */
export type PluginBackendAPI = {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
};

/** The full PluginContext passed to activate() */
export type PluginContext = Readonly<{
  pluginId: string;
  connections: PluginConnectionsAPI;
  events: PluginEventsAPI;
  ui: PluginUIAPI;
  terminal: PluginTerminalAPI;
  settings: PluginSettingsAPI;
  i18n: PluginI18nAPI;
  storage: PluginStorageAPI;
  api: PluginBackendAPI;
}>;

// ═══════════════════════════════════════════════════════════════════════════
// Plugin Configuration (persisted)
// ═══════════════════════════════════════════════════════════════════════════

/** Per-plugin persisted config */
export type PluginConfig = {
  enabled: boolean;
};

/** Global plugin configuration (plugin-config.json) */
export type PluginGlobalConfig = {
  plugins: Record<string, PluginConfig>;
};

// ═══════════════════════════════════════════════════════════════════════════
// Window augmentation for shared modules
// ═══════════════════════════════════════════════════════════════════════════

declare global {
  interface Window {
    __OXIDE__?: {
      React: typeof import('react');
      ReactDOM: { createRoot: typeof import('react-dom/client').createRoot };
      zustand: { create: typeof import('zustand').create };
      lucideReact: typeof import('lucide-react');
      ui: import('../lib/plugin/pluginUIKit').PluginUIKit;
    };
  }
}
