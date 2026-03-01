/**
 * OxideTerm Plugin System — Type Definitions (v2)
 *
 * All types for the runtime dynamic plugin system.
 * Supports both single-file ESM bundles (v1) and multi-file packages (v2).
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

  // ── v2 Package Fields ─────────────────────────────────────────────────
  /** Manifest schema version (1 = legacy single-file, 2 = package) */
  manifestVersion?: 1 | 2;
  /** Plugin format: 'bundled' (single ESM, default) or 'package' (multi-file) */
  format?: 'bundled' | 'package';
  /** Static assets directory (relative path) */
  assets?: string;
  /** CSS files to auto-load on activation (relative paths) */
  styles?: string[];
  /** Shared dependencies the plugin expects from the host */
  sharedDependencies?: Record<string, string>;
  /** Plugin repository URL (for update checking) */
  repository?: string;
  /** SHA-256 checksum of the plugin package */
  checksum?: string;

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
  /** @deprecated Use nodeId instead. Will be removed in next major version. */
  sessionId: string;
  /** Stable node identifier, survives reconnect. */
  nodeId: string;
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
  /** Phase 4.5: resolve node to connection snapshot */
  getByNode(nodeId: string): ConnectionSnapshot | null;
};

/** ctx.events — lifecycle events + inter-plugin communication */
export type PluginEventsAPI = {
  onConnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onDisconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onLinkDown(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onReconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onIdle(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  /** Phase 4.5: Node becomes ready (connected + capabilities available) */
  onNodeReady(handler: (info: { nodeId: string; connectionId: string }) => void): Disposable;
  /** Phase 4.5: Node disconnected */
  onNodeDisconnected(handler: (info: { nodeId: string }) => void): Disposable;
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
  registerCommand(id: string, opts: {
    label: string;
    icon?: string;
    shortcut?: string;
    section?: string;
  }, handler: () => void): Disposable;
  openTab(tabId: string): void;
  showToast(opts: {
    title: string;
    description?: string;
    variant?: 'default' | 'success' | 'error' | 'warning';
  }): void;
  showConfirm(opts: { title: string; description: string }): Promise<boolean>;
};

/** Plugin command entry stored in pluginStore */
export type PluginCommandEntry = {
  pluginId: string;
  id: string;
  label: string;
  icon?: string;
  shortcut?: string;
  section?: string;
  handler: () => void;
};

/** ctx.terminal — terminal hooks and utilities */
export type PluginTerminalAPI = {
  registerInputInterceptor(handler: InputInterceptor): Disposable;
  registerOutputProcessor(handler: OutputProcessor): Disposable;
  registerShortcut(command: string, handler: () => void): Disposable;
  /** Write to terminal by nodeId (stable across reconnects) */
  writeToNode(nodeId: string, text: string): void;
  /** Get terminal buffer by nodeId */
  getNodeBuffer(nodeId: string): string | null;
  /** Get terminal selection by nodeId */
  getNodeSelection(nodeId: string): string | null;
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

/** ctx.assets — static asset loading (v2) */
export type PluginAssetsAPI = {
  /** Load a CSS file from the plugin directory. Returns a Disposable to remove it. */
  loadCSS(relativePath: string): Promise<Disposable>;
  /** Get a Blob URL for a binary asset (image, font, etc.) */
  getAssetUrl(relativePath: string): Promise<string>;
  /** Revoke a previously created asset URL */
  revokeAssetUrl(url: string): void;
};

// ═══════════════════════════════════════════════════════════════════════════
// SFTP File Info (subset safe for plugins)
// ═══════════════════════════════════════════════════════════════════════════

/** File information returned by SFTP operations */
export type PluginFileInfo = Readonly<{
  name: string;
  path: string;
  file_type: 'file' | 'directory' | 'symlink' | 'unknown';
  size: number;
  modified: number | null;
  permissions: string | null;
}>;

/** ctx.sftp — remote file system operations (requires node with active SFTP session) */
export type PluginSftpAPI = {
  /** List directory contents on the remote host */
  listDir(nodeId: string, path: string): Promise<ReadonlyArray<PluginFileInfo>>;
  /** Get file/directory metadata */
  stat(nodeId: string, path: string): Promise<PluginFileInfo>;
  /** Read a remote text file (max 10 MB, returns UTF-8 string) */
  readFile(nodeId: string, path: string): Promise<string>;
  /** Write text content to a remote file */
  writeFile(nodeId: string, path: string, content: string): Promise<void>;
  /** Create a directory on the remote host */
  mkdir(nodeId: string, path: string): Promise<void>;
  /** Delete a remote file */
  delete(nodeId: string, path: string): Promise<void>;
  /** Rename or move a remote file/directory */
  rename(nodeId: string, oldPath: string, newPath: string): Promise<void>;
};

// ═══════════════════════════════════════════════════════════════════════════
// Port Forward Info (subset safe for plugins)
// ═══════════════════════════════════════════════════════════════════════════

/** Port forward rule snapshot for plugins */
export type PluginForwardRule = Readonly<{
  id: string;
  forward_type: 'local' | 'remote' | 'dynamic';
  bind_address: string;
  bind_port: number;
  target_host: string;
  target_port: number;
  status: string;
  description?: string;
}>;

/** Request to create a port forward */
export type PluginForwardRequest = {
  sessionId: string;
  forwardType: 'local' | 'remote' | 'dynamic';
  bindAddress: string;
  bindPort: number;
  targetHost: string;
  targetPort: number;
  description?: string;
};

/** ctx.forward — port forwarding management */
export type PluginForwardAPI = {
  /** List all active forwards for a session */
  list(sessionId: string): Promise<ReadonlyArray<PluginForwardRule>>;
  /** Create a new port forward */
  create(request: PluginForwardRequest): Promise<{ success: boolean; forward?: PluginForwardRule; error?: string }>;
  /** Stop a port forward */
  stop(sessionId: string, forwardId: string): Promise<void>;
  /** Stop all forwards for a session */
  stopAll(sessionId: string): Promise<void>;
  /** Get traffic stats for a forward */
  getStats(sessionId: string, forwardId: string): Promise<{
    connectionCount: number;
    activeConnections: number;
    bytesSent: number;
    bytesReceived: number;
  } | null>;
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
  assets: PluginAssetsAPI;
  /** Remote file system operations via SFTP */
  sftp: PluginSftpAPI;
  /** Port forwarding management */
  forward: PluginForwardAPI;
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
  /** Plugin registry URL */
  registryUrl?: string;
  /** Whether to check for updates on startup */
  autoCheckUpdates?: boolean;
  /** Last update check timestamp (ISO 8601) */
  lastUpdateCheck?: string;
};

// ═══════════════════════════════════════════════════════════════════════════
// Plugin Registry (Remote Installation)
// ═══════════════════════════════════════════════════════════════════════════

/** A plugin entry from the remote registry index */
export type RegistryEntry = {
  id: string;
  name: string;
  description?: string;
  author?: string;
  version: string;
  minOxidetermVersion?: string;
  downloadUrl: string;
  checksum?: string;
  size?: number;
  tags?: string[];
  homepage?: string;
  updatedAt?: string;
};

/** The registry index fetched from a remote URL */
export type RegistryIndex = {
  version: number;
  plugins: RegistryEntry[];
};

/** Installation progress state */
export type InstallState = 'idle' | 'downloading' | 'extracting' | 'installing' | 'done' | 'error';

// ═══════════════════════════════════════════════════════════════════════════
// Window augmentation for shared modules
// ═══════════════════════════════════════════════════════════════════════════

declare global {
  interface Window {
    __OXIDE__?: {
      React: typeof import('react');
      ReactDOM: { createRoot: typeof import('react-dom/client').createRoot };
      zustand: { create: typeof import('zustand').create };
      lucideIcons: Record<string, import('react').ForwardRefExoticComponent<import('react').SVGProps<SVGSVGElement>>>;
      /** @deprecated Use lucideIcons instead. Kept for backward compatibility with existing plugins. */
      lucideReact: typeof import('lucide-react');
      ui: import('../lib/plugin/pluginUIKit').PluginUIKit;
      /** clsx — lightweight className string builder */
      clsx: typeof import('clsx').clsx;
      /** cn — Tailwind-merge + clsx helper (project utility) */
      cn: (...inputs: import('clsx').ClassValue[]) => string;
      /** useTranslation — i18next React hook for host-level translations */
      useTranslation: typeof import('react-i18next').useTranslation;
      /** Host application version */
      version: string;
      /** Plugin API version (2 = current) */
      pluginApiVersion: number;
    };
  }
}
