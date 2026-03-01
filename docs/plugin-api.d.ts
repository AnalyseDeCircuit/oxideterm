/**
 * OxideTerm Plugin API — Type Declarations (v2.1)
 *
 * Copy this file into your plugin project for full TypeScript support.
 *
 * Usage:
 *   1. Copy this file to your plugin project root
 *   2. Reference it in your tsconfig.json: "include": ["plugin-api.d.ts", "src/**\/*"]
 *   3. Import types via: import type { PluginContext } from './plugin-api'
 *
 * Or use triple-slash directive: /// <reference path="./plugin-api.d.ts" />
 *
 * @version 2.1.0
 * @see https://github.com/AnalyseDeCircuit/oxideterm/blob/main/docs/PLUGIN_DEVELOPMENT.md
 */

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
  description?: string;
  options?: Array<{ label: string; value: string | number }>;
};

/** Terminal hooks contribution */
export type PluginTerminalHooksDef = {
  inputInterceptor?: boolean;
  outputProcessor?: boolean;
  shortcuts?: Array<{ key: string; command: string }>;
};

export type ConnectionHookType = 'onConnect' | 'onDisconnect' | 'onReconnect' | 'onLinkDown' | 'onIdle';

/** The plugin.json manifest */
export type PluginManifest = {
  id: string;
  name: string;
  version: string;
  description?: string;
  author?: string;
  main: string;
  engines?: { oxideterm?: string };
  manifestVersion?: 1 | 2;
  format?: 'bundled' | 'package';
  assets?: string;
  styles?: string[];
  sharedDependencies?: Record<string, string>;
  repository?: string;
  checksum?: string;
  contributes?: {
    tabs?: PluginTabDef[];
    sidebarPanels?: PluginSidebarDef[];
    settings?: PluginSettingDef[];
    terminalHooks?: PluginTerminalHooksDef;
    connectionHooks?: ConnectionHookType[];
    /** Tauri backend commands this plugin may invoke via ctx.api.invoke() */
    apiCommands?: string[];
  };
  locales?: string;
};

// ═══════════════════════════════════════════════════════════════════════════
// Core Types
// ═══════════════════════════════════════════════════════════════════════════

export type Disposable = { dispose(): void };

export type SshConnectionState =
  | 'disconnected' | 'connecting' | 'authenticating'
  | 'active' | 'error' | 'idle' | 'link_down';

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

export type TerminalHookContext = {
  /** @deprecated Use nodeId instead */
  sessionId: string;
  /** Stable node identifier, survives reconnect */
  nodeId: string;
};

export type InputInterceptor = (data: string, context: TerminalHookContext) => string | null;
export type OutputProcessor = (data: Uint8Array, context: TerminalHookContext) => Uint8Array;

export type PluginTabProps = {
  tabId: string;
  pluginId: string;
};

// ═══════════════════════════════════════════════════════════════════════════
// SFTP Types
// ═══════════════════════════════════════════════════════════════════════════

export type PluginFileInfo = Readonly<{
  name: string;
  path: string;
  file_type: 'file' | 'directory' | 'symlink' | 'unknown';
  size: number;
  modified: number | null;
  permissions: string | null;
}>;

// ═══════════════════════════════════════════════════════════════════════════
// Port Forwarding Types
// ═══════════════════════════════════════════════════════════════════════════

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

export type PluginForwardRequest = {
  sessionId: string;
  forwardType: 'local' | 'remote' | 'dynamic';
  bindAddress: string;
  bindPort: number;
  targetHost: string;
  targetPort: number;
  description?: string;
};

// ═══════════════════════════════════════════════════════════════════════════
// Plugin Context API Namespaces
// ═══════════════════════════════════════════════════════════════════════════

/** ctx.connections — read-only connection state */
export type PluginConnectionsAPI = {
  getAll(): ReadonlyArray<ConnectionSnapshot>;
  get(connectionId: string): ConnectionSnapshot | null;
  getState(connectionId: string): SshConnectionState | null;
  getByNode(nodeId: string): ConnectionSnapshot | null;
};

/** ctx.events — lifecycle events + inter-plugin communication */
export type PluginEventsAPI = {
  onConnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onDisconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onLinkDown(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onReconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onIdle(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onNodeReady(handler: (info: { nodeId: string; connectionId: string }) => void): Disposable;
  onNodeDisconnected(handler: (info: { nodeId: string }) => void): Disposable;
  /** Inter-plugin events (namespaced as plugin:{pluginId}:{name}) */
  on(name: string, handler: (data: unknown) => void): Disposable;
  emit(name: string, data: unknown): void;
};

/** ctx.ui — view registration and user interaction */
export type PluginUIAPI = {
  registerTabView(tabId: string, component: React.ComponentType<PluginTabProps>): Disposable;
  registerSidebarPanel(panelId: string, component: React.ComponentType): Disposable;
  /** Register a command in the global command palette */
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

/** ctx.terminal — terminal hooks and utilities */
export type PluginTerminalAPI = {
  registerInputInterceptor(handler: InputInterceptor): Disposable;
  registerOutputProcessor(handler: OutputProcessor): Disposable;
  registerShortcut(command: string, handler: () => void): Disposable;
  writeToNode(nodeId: string, text: string): void;
  getNodeBuffer(nodeId: string): string | null;
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

/** ctx.api — restricted backend calls (requires apiCommands in manifest) */
export type PluginBackendAPI = {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
};

/** ctx.assets — static asset loading */
export type PluginAssetsAPI = {
  loadCSS(relativePath: string): Promise<Disposable>;
  getAssetUrl(relativePath: string): Promise<string>;
  revokeAssetUrl(url: string): void;
};

/** ctx.sftp — remote file system operations */
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

/** ctx.forward — port forwarding management */
export type PluginForwardAPI = {
  /** List all active forwards for a session */
  list(sessionId: string): Promise<ReadonlyArray<PluginForwardRule>>;
  /** Create a new port forward */
  create(request: PluginForwardRequest): Promise<{
    success: boolean;
    forward?: PluginForwardRule;
    error?: string;
  }>;
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

// ═══════════════════════════════════════════════════════════════════════════
// The Full Plugin Context
// ═══════════════════════════════════════════════════════════════════════════

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
  sftp: PluginSftpAPI;
  forward: PluginForwardAPI;
}>;

// ═══════════════════════════════════════════════════════════════════════════
// Plugin Module Export Interface
// ═══════════════════════════════════════════════════════════════════════════

/** Your plugin must default-export (or named-export) this shape */
export type PluginModule = {
  activate: (ctx: PluginContext) => void | Promise<void>;
  deactivate?: () => void | Promise<void>;
};

// ═══════════════════════════════════════════════════════════════════════════
// Host Shared Modules (window.__OXIDE__)
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Shared modules available at runtime via window.__OXIDE__.
 * Plugins should externalize these in their bundler config to avoid
 * duplicate React instances and hooks crashes.
 *
 * Available modules:
 *   - React          — window.__OXIDE__.React
 *   - ReactDOM       — window.__OXIDE__.ReactDOM  (createRoot only)
 *   - zustand        — window.__OXIDE__.zustand    (create only)
 *   - lucideIcons    — window.__OXIDE__.lucideIcons (icon map)
 *   - lucideReact    — window.__OXIDE__.lucideReact (full module with Proxy fallback)
 *   - clsx           — window.__OXIDE__.clsx
 *   - cn             — window.__OXIDE__.cn          (Tailwind-merge + clsx)
 *   - useTranslation — window.__OXIDE__.useTranslation (react-i18next)
 *   - ui             — window.__OXIDE__.ui          (OxideTerm UI Kit)
 *
 * UI Kit components (window.__OXIDE__.ui.*):
 *   Layout:    Stack, Grid
 *   Container: Card, Stat
 *   Form:      Button, Input, Checkbox, Select, Toggle
 *   Display:   Text, Badge, Separator, IconText, KV, EmptyState, ListItem
 *   Feedback:  Progress, Alert, Spinner
 *   Data:      Table, CodeBlock
 *   Composite: Tabs, Header
 */

// ═══════════════════════════════════════════════════════════════════════════
// Available apiCommands (for manifest contributes.apiCommands)
// ═══════════════════════════════════════════════════════════════════════════

/**
 * The following Tauri backend commands can be invoked by plugins via
 * ctx.api.invoke(command, args). Each must be declared in the plugin
 * manifest under contributes.apiCommands.
 *
 * NOTE: ctx.sftp and ctx.forward provide typed wrappers for the most
 * common SFTP/forwarding operations. Use ctx.api.invoke only for
 * commands not covered by those namespaces.
 *
 * ── Connection & Session ─────────────────────────────────────────────
 *   "list_connections"             — List all active connections
 *   "get_connection_health"       — Get health metrics for a connection
 *   "quick_health_check"          — Quick connectivity check
 *
 * ── SFTP (low-level, prefer ctx.sftp instead) ───────────────────────
 *   "node_sftp_init"              — Initialize SFTP channel for a node
 *   "node_sftp_list_dir"          — List remote directory
 *   "node_sftp_stat"              — Get file/dir metadata
 *   "node_sftp_preview"           — Preview file content (text/image/hex)
 *   "node_sftp_write"             — Write file content
 *   "node_sftp_mkdir"             — Create directory
 *   "node_sftp_delete"            — Delete file
 *   "node_sftp_delete_recursive"  — Recursively delete directory
 *   "node_sftp_rename"            — Rename/move file
 *   "node_sftp_download"          — Download file to local
 *   "node_sftp_upload"            — Upload file to remote
 *   "node_sftp_download_dir"      — Download directory recursively
 *   "node_sftp_upload_dir"        — Upload directory recursively
 *   "node_sftp_tar_probe"         — Check if remote supports tar
 *   "node_sftp_tar_upload"        — Tar-stream upload (faster)
 *   "node_sftp_tar_download"      — Tar-stream download (faster)
 *
 * ── Port Forwarding (low-level, prefer ctx.forward instead) ─────────
 *   "list_port_forwards"          — List forwards for a session
 *   "create_port_forward"         — Create a new forward
 *   "stop_port_forward"           — Stop a forward
 *   "delete_port_forward"         — Delete a forward rule
 *   "restart_port_forward"        — Restart a stopped forward
 *   "update_port_forward"         — Update forward parameters
 *   "get_port_forward_stats"      — Get forward traffic stats
 *   "stop_all_forwards"           — Stop all forwards for a session
 *
 * ── SFTP Transfer Queue ──────────────────────────────────────────────
 *   "sftp_cancel_transfer"        — Cancel a queued/active transfer
 *   "sftp_pause_transfer"         — Pause an active transfer
 *   "sftp_resume_transfer"        — Resume a paused transfer
 *   "sftp_transfer_stats"         — Get transfer queue statistics
 *
 * ── System ───────────────────────────────────────────────────────────
 *   "get_app_version"             — Get OxideTerm version
 *   "get_system_info"             — Get OS/platform info
 */
