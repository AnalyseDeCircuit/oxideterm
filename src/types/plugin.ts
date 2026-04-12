// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * OxideTerm Plugin System — Type Definitions (v2)
 *
 * All types for the runtime dynamic plugin system.
 * Supports both single-file ESM bundles (v1) and multi-file packages (v2).
 */

import type {
  ConnectionInfo,
  ExportPreflightResult,
  ImportPreview,
  ImportResult,
  OxideMetadata,
  SshConnectionState,
} from './index';
import type { OxideAppSettingsSectionId } from '../store/settingsStore';

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

export type UrlInstallResult = {
  manifest: PluginManifest;
  checksum: string;
  replacedExisting: boolean;
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
  language?: string;
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
  /** v3: Register context menu items for a target area */
  registerContextMenu(target: ContextMenuTarget, items: ContextMenuItem[]): Disposable;
  /** v3: Register a status bar item */
  registerStatusBarItem(options: StatusBarItemOptions): StatusBarHandle;
  /** v3: Register a global keybinding */
  registerKeybinding(keybinding: string, handler: () => void): Disposable;
  /** v3: Show a notification (maps to toast system) */
  showNotification(opts: {
    title: string;
    body?: string;
    severity?: 'info' | 'warning' | 'error';
  }): void;
  /** v3: Show a progress indicator, returns a reporter to update and dismiss */
  showProgress(title: string): ProgressReporter;
  /** v3: Get current layout info (read-only) */
  getLayout(): Readonly<{ sidebarCollapsed: boolean; activeTabId: string | null; tabCount: number }>;
  /** v3: Subscribe to layout changes */
  onLayoutChange(handler: (layout: Readonly<{ sidebarCollapsed: boolean; activeTabId: string | null; tabCount: number }>) => void): Disposable;
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
  /** Get the last focused terminal target across SSH and local terminals */
  getActiveTarget(): PluginActiveTerminalTarget | null;
  /** Write directly to the last focused terminal target */
  writeToActive(text: string): boolean;
  /** Write to terminal by nodeId (stable across reconnects) */
  writeToNode(nodeId: string, text: string): void;
  /** Get terminal buffer by nodeId */
  getNodeBuffer(nodeId: string): string | null;
  /** Get terminal selection by nodeId */
  getNodeSelection(nodeId: string): string | null;
  /** v3: Search terminal buffer */
  search(nodeId: string, query: string, options?: { caseSensitive?: boolean; regex?: boolean; wholeWord?: boolean }): Promise<Readonly<{ matches: ReadonlyArray<unknown>; total_matches: number }>>;
  /** v3: Get scrollback buffer content */
  getScrollBuffer(nodeId: string, startLine: number, count: number): Promise<ReadonlyArray<Readonly<{ text: string; lineNumber: number }>>>;
  /** v3: Get buffer size info */
  getBufferSize(nodeId: string): Promise<Readonly<{ currentLines: number; totalLines: number; maxLines: number }>>;
  /** v3: Clear terminal buffer */
  clearBuffer(nodeId: string): Promise<void>;
};

/** ctx.settings — plugin-scoped settings */
export type PluginSettingsAPI = {
  get<T>(key: string): T;
  set<T>(key: string, value: T): void;
  onChange(key: string, handler: (newValue: unknown) => void): Disposable;
  exportSyncableSettings(): Promise<Readonly<{
    revision: string;
    exportedAt: string;
    payload: SyncableSettingsPayload;
    warnings: ReadonlyArray<SyncableSettingsWarning>;
  }>>;
  applySyncableSettings(payload: SyncableSettingsPayload): Promise<Readonly<ApplySyncableSettingsResult>>;
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

/** Safe saved-connection snapshot for sync/export workflows (no secrets included) */
export type SavedConnectionSnapshot = Readonly<{
  id: ConnectionInfo['id'];
  name: ConnectionInfo['name'];
  group: ConnectionInfo['group'];
  host: ConnectionInfo['host'];
  port: ConnectionInfo['port'];
  username: ConnectionInfo['username'];
  auth_type: ConnectionInfo['auth_type'];
  key_path: ConnectionInfo['key_path'];
  cert_path: ConnectionInfo['cert_path'];
  created_at: ConnectionInfo['created_at'];
  last_used_at: ConnectionInfo['last_used_at'];
  color: ConnectionInfo['color'];
  tags: ReadonlyArray<string>;
  agent_forwarding: boolean;
  proxy_chain: ReadonlyArray<NonNullable<ConnectionInfo['proxy_chain']>[number]>;
}>;

export type SavedConnectionSyncRecord = Readonly<{
  id: string;
  revision: string;
  updatedAt: string;
  deleted: boolean;
  payload?: SavedConnectionSnapshot;
}>;

export type SavedConnectionsSyncSnapshot = Readonly<{
  revision: string;
  exportedAt: string;
  records: ReadonlyArray<SavedConnectionSyncRecord>;
}>;

export type ApplySavedConnectionsSyncSnapshotResult = Readonly<{
  applied: number;
  skipped: number;
  conflicts: number;
}>;

export type SavedForwardSnapshot = Readonly<{
  id: string;
  session_id: string;
  owner_connection_id?: string;
  forward_type: string;
  bind_address: string;
  bind_port: number;
  target_host: string;
  target_port: number;
  auto_start: boolean;
  created_at: string;
  description?: string;
}>;

export type SavedForwardSyncRecord = Readonly<{
  id: string;
  revision: string;
  updatedAt: string;
  deleted: boolean;
  payload?: SavedForwardSnapshot;
}>;

export type SavedForwardsSyncSnapshot = Readonly<{
  revision: string;
  exportedAt: string;
  records: ReadonlyArray<SavedForwardSyncRecord>;
}>;

export type ApplySavedForwardsSyncSnapshotResult = Readonly<{
  applied: number;
  skipped: number;
}>;

export type SyncableSettingsPayload = Readonly<{
  appearance?: Readonly<{
    language?: string;
    uiDensity?: 'compact' | 'comfortable' | 'spacious';
  }>;
  terminal?: Readonly<{
    fontSize?: number;
    theme?: string;
  }>;
  reconnect?: Readonly<{
    autoReconnect?: boolean;
  }>;
}>;

export type SyncableSettingsWarning = Readonly<{
  path: string;
  code:
    | 'unsupported-language'
    | 'invalid-ui-density'
    | 'font-size-clamped'
    | 'invalid-font-size'
    | 'missing-theme'
    | 'invalid-auto-reconnect';
  applied: boolean;
  message: string;
  normalizedValue?: string | number | boolean;
}>;

export type ApplySyncableSettingsResult = Readonly<{
  revision: string;
  appliedPayload: SyncableSettingsPayload;
  warnings: ReadonlyArray<SyncableSettingsWarning>;
}>;

export type LocalSyncMetadata = Readonly<{
  savedConnectionsRevision: string;
  savedConnectionsUpdatedAt: string;
  savedForwardsRevision?: string;
  settingsRevision?: string;
  appSettingsSectionRevisions?: Readonly<Partial<Record<OxideAppSettingsSectionId, string>>>;
  pluginSettingsRevisions?: Readonly<Record<string, string>>;
}>;

/** Built-in conflict strategies for importing encrypted .oxide payloads */
export type PluginSyncConflictStrategy = 'rename' | 'skip' | 'replace' | 'merge';

export type PluginOxideExportProgress = Readonly<{
  /** Discrete stage identifier emitted by a single host export invocation. */
  stage: string;
  /** Completed steps for this export invocation, not a 0-1 ratio. */
  current: number;
  /** Total discrete steps for this export invocation. */
  total: number;
}>;

export type PluginOxideImportProgress = Readonly<{
  /** Discrete stage identifier emitted by a single host import invocation. */
  stage: string;
  /** Completed steps for this import invocation, not a 0-1 ratio. */
  current: number;
  /** Total discrete steps for this import invocation. */
  total: number;
}>;

/** ctx.sync — saved-connection sync and encrypted import/export helpers */
export type PluginSyncAPI = {
  /** Current in-memory snapshot of saved connections */
  listSavedConnections(): ReadonlyArray<SavedConnectionSnapshot>;
  /** Refresh saved connections from backend, then return the updated snapshot */
  refreshSavedConnections(): Promise<ReadonlyArray<SavedConnectionSnapshot>>;
  /** Subscribe to saved-connection list changes */
  onSavedConnectionsChange(handler: (connections: ReadonlyArray<SavedConnectionSnapshot>) => void): Disposable;
  /** Export a structured saved-connection snapshot for plugin-driven sync */
  exportSavedConnectionsSnapshot(): Promise<SavedConnectionsSyncSnapshot>;
  /** Apply a structured saved-connection snapshot using a host-managed conflict strategy */
  applySavedConnectionsSnapshot(
    snapshot: SavedConnectionsSyncSnapshot,
    options?: { conflictStrategy?: Extract<PluginSyncConflictStrategy, 'skip' | 'replace' | 'merge'> },
  ): Promise<ApplySavedConnectionsSyncSnapshotResult>;
  /** Get lightweight local sync metadata for revision comparison */
  getLocalSyncMetadata(): Promise<LocalSyncMetadata>;
  /** Pre-flight export analysis for selected connections (or all if omitted) */
  preflightExport(connectionIds?: string[], options?: { embedKeys?: boolean }): Promise<Readonly<ExportPreflightResult>>;
  /** Export selected connections (or all if omitted) to encrypted .oxide bytes */
  exportOxide(request: {
    connectionIds?: string[];
    password: string;
    description?: string;
    embedKeys?: boolean;
    includeAppSettings?: boolean;
    selectedAppSettingsSections?: OxideAppSettingsSectionId[];
    includeLocalTerminalEnvVars?: boolean;
    includePluginSettings?: boolean;
    selectedPluginIds?: string[];
    selectedForwardIds?: string[];
    onProgress?: (progress: PluginOxideExportProgress) => void;
  }): Promise<Uint8Array>;
  /** Validate a .oxide payload and read metadata without decrypting */
  validateOxide(fileData: Uint8Array): Promise<Readonly<OxideMetadata>>;
  /** Preview import results and detect conflicts before applying */
  previewImport(
    fileData: Uint8Array,
    password: string,
    options?: {
      conflictStrategy?: PluginSyncConflictStrategy;
      onProgress?: (progress: PluginOxideImportProgress) => void;
    },
  ): Promise<Readonly<ImportPreview>>;
  /** Import an encrypted .oxide payload using a host-managed conflict strategy */
  importOxide(
    fileData: Uint8Array,
    password: string,
    options?: {
      selectedNames?: string[];
      conflictStrategy?: PluginSyncConflictStrategy;
      importAppSettings?: boolean;
      selectedAppSettingsSections?: string[];
      importPluginSettings?: boolean;
      selectedPluginIds?: string[];
      importForwards?: boolean;
      onProgress?: (progress: PluginOxideImportProgress) => void;
    },
  ): Promise<Readonly<ImportResult>>;
};

/** ctx.secrets — plugin-scoped secure secret storage backed by OS keychain */
export type PluginSecretsAPI = {
  get(key: string): Promise<string | null>;
  getMany(keys: readonly string[]): Promise<Readonly<Record<string, string | null>>>;
  set(key: string, value: string): Promise<void>;
  has(key: string): Promise<boolean>;
  delete(key: string): Promise<void>;
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
  /** List all saved forwards eligible for sync */
  listSavedForwards(): ReadonlyArray<SavedForwardSnapshot>;
  /** Subscribe to saved-forward changes */
  onSavedForwardsChange(handler: (items: ReadonlyArray<SavedForwardSnapshot>) => void): Disposable;
  /** Export saved forwards as a structured sync snapshot */
  exportSavedForwardsSnapshot(): Promise<SavedForwardsSyncSnapshot>;
  /** Apply a structured saved-forward sync snapshot */
  applySavedForwardsSnapshot(snapshot: SavedForwardsSyncSnapshot): Promise<ApplySavedForwardsSyncSnapshotResult>;
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

// ═══════════════════════════════════════════════════════════════════════════
// Plugin API v3 — New Snapshot Types
// ═══════════════════════════════════════════════════════════════════════════

/** Frozen session tree node for plugin consumption */
export type SessionTreeNodeSnapshot = Readonly<{
  id: string;
  label: string;
  host?: string;
  port?: number;
  username?: string;
  parentId: string | null;
  childIds: readonly string[];
  connectionState: string;
  connectionId: string | null;
  terminalIds: readonly string[];
  sftpSessionId: string | null;
  errorMessage?: string;
}>;

/** Frozen transfer item for plugin consumption */
export type TransferSnapshot = Readonly<{
  id: string;
  nodeId: string;
  name: string;
  localPath: string;
  remotePath: string;
  direction: 'upload' | 'download';
  size: number;
  transferred: number;
  state: 'pending' | 'active' | 'paused' | 'completed' | 'cancelled' | 'error';
  error?: string;
  startTime: number;
  endTime?: number;
}>;

/** Frozen resource metrics for plugin consumption */
export type ProfilerMetricsSnapshot = Readonly<{
  timestampMs: number;
  cpuPercent: number | null;
  memoryUsed: number | null;
  memoryTotal: number | null;
  memoryPercent: number | null;
  loadAvg1: number | null;
  loadAvg5: number | null;
  loadAvg15: number | null;
  cpuCores: number | null;
  netRxBytesPerSec: number | null;
  netTxBytesPerSec: number | null;
  sshRttMs: number | null;
}>;

/** Frozen event log entry for plugin consumption */
export type EventLogEntrySnapshot = Readonly<{
  id: number;
  timestamp: number;
  severity: 'info' | 'warn' | 'error';
  category: 'connection' | 'reconnect' | 'node';
  nodeId?: string;
  connectionId?: string;
  title: string;
  detail?: string;
  source: string;
}>;

/** Frozen IDE file info for plugin consumption */
export type IdeFileSnapshot = Readonly<{
  path: string;
  name: string;
  language: string;
  isDirty: boolean;
  isActive: boolean;
  isPinned: boolean;
}>;

/** Frozen IDE project info for plugin consumption */
export type IdeProjectSnapshot = Readonly<{
  nodeId: string;
  rootPath: string;
  name: string;
  isGitRepo: boolean;
  gitBranch?: string;
}>;

/** Frozen AI conversation summary for plugin consumption */
export type AiConversationSnapshot = Readonly<{
  id: string;
  title: string;
  messageCount: number;
  createdAt: number;
  updatedAt: number;
}>;

/** Frozen AI message for plugin consumption */
export type AiMessageSnapshot = Readonly<{
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
}>;

/** Theme info for plugin consumption */
export type ThemeSnapshot = Readonly<{
  name: string;
  isDark: boolean;
}>;

/** Pool statistics for plugin consumption */
export type PoolStatsSnapshot = Readonly<{
  activeConnections: number;
  totalSessions: number;
}>;

/** Status bar item options */
export type StatusBarItemOptions = {
  text: string;
  icon?: string;
  tooltip?: string;
  alignment: 'left' | 'right';
  priority?: number;
  onClick?: () => void;
};

/** Status bar item handle returned to plugins */
export type StatusBarHandle = {
  update(options: Partial<StatusBarItemOptions>): void;
  dispose(): void;
};

/** Context menu target areas */
export type ContextMenuTarget = 'terminal' | 'sftp' | 'tab' | 'sidebar';

/** Context menu item definition */
export type ContextMenuItem = {
  label: string;
  icon?: string;
  handler: () => void;
  when?: () => boolean;
};

/** Progress reporter passed to showProgress callback */
export type ProgressReporter = {
  report(value: number, total: number, message?: string): void;
};

export type PluginActiveTerminalTarget = Readonly<{
  sessionId: string;
  terminalType: 'terminal' | 'local_terminal';
  nodeId: string | null;
  connectionId: string | null;
  connectionState: string | null;
  label: string | null;
}>;

// ═══════════════════════════════════════════════════════════════════════════
// Plugin API v3 — New Namespace Interfaces
// ═══════════════════════════════════════════════════════════════════════════

/** ctx.sessions — read-only session tree access */
export type PluginSessionsAPI = {
  /** Get a frozen snapshot of the entire session tree */
  getTree(): ReadonlyArray<SessionTreeNodeSnapshot>;
  /** Get active (connected) nodes */
  getActiveNodes(): ReadonlyArray<Readonly<{ nodeId: string; sessionId: string | null; connectionState: string }>>;
  /** Get a single node's state */
  getNodeState(nodeId: string): string | null;
  /** Subscribe to tree structure changes */
  onTreeChange(handler: (tree: ReadonlyArray<SessionTreeNodeSnapshot>) => void): Disposable;
  /** Subscribe to a specific node's state changes */
  onNodeStateChange(nodeId: string, handler: (state: string) => void): Disposable;
};

/** ctx.transfers — SFTP transfer monitoring */
export type PluginTransfersAPI = {
  /** Get all current transfers */
  getAll(): ReadonlyArray<TransferSnapshot>;
  /** Get transfers for a specific node */
  getByNode(nodeId: string): ReadonlyArray<TransferSnapshot>;
  /** Subscribe to transfer progress (throttled 500ms) */
  onProgress(handler: (transfer: TransferSnapshot) => void): Disposable;
  /** Subscribe to transfer completion */
  onComplete(handler: (transfer: TransferSnapshot) => void): Disposable;
  /** Subscribe to transfer errors */
  onError(handler: (transfer: TransferSnapshot) => void): Disposable;
};

/** ctx.profiler — resource monitoring */
export type PluginProfilerAPI = {
  /** Get current metrics for a node */
  getMetrics(nodeId: string): ProfilerMetricsSnapshot | null;
  /** Get historical metrics */
  getHistory(nodeId: string, maxPoints?: number): ReadonlyArray<ProfilerMetricsSnapshot>;
  /** Check if profiler is running for a node */
  isRunning(nodeId: string): boolean;
  /** Subscribe to live metrics (throttled 1s) */
  onMetrics(nodeId: string, handler: (metrics: ProfilerMetricsSnapshot) => void): Disposable;
};

/** ctx.eventLog — connection event log access */
export type PluginEventLogAPI = {
  /** Get log entries with optional filter */
  getEntries(filter?: { severity?: 'info' | 'warn' | 'error'; category?: 'connection' | 'reconnect' | 'node' }): ReadonlyArray<EventLogEntrySnapshot>;
  /** Subscribe to new log entries */
  onEntry(handler: (entry: EventLogEntrySnapshot) => void): Disposable;
};

/** ctx.ide — read-only IDE mode access */
export type PluginIdeAPI = {
  /** Check if IDE mode is active */
  isOpen(): boolean;
  /** Get current project info */
  getProject(): IdeProjectSnapshot | null;
  /** Get list of open files */
  getOpenFiles(): ReadonlyArray<IdeFileSnapshot>;
  /** Get the currently active file */
  getActiveFile(): IdeFileSnapshot | null;
  /** Subscribe to file open events */
  onFileOpen(handler: (file: IdeFileSnapshot) => void): Disposable;
  /** Subscribe to file close events */
  onFileClose(handler: (path: string) => void): Disposable;
  /** Subscribe to active file change */
  onActiveFileChange(handler: (file: IdeFileSnapshot | null) => void): Disposable;
};

/** ctx.ai — read-only AI chat access */
export type PluginAiAPI = {
  /** Get all conversation summaries */
  getConversations(): ReadonlyArray<AiConversationSnapshot>;
  /** Get messages for a conversation (content sanitized) */
  getMessages(conversationId: string): ReadonlyArray<AiMessageSnapshot>;
  /** Get active AI provider info */
  getActiveProvider(): Readonly<{ type: string; displayName: string }> | null;
  /** Get available model IDs */
  getAvailableModels(): ReadonlyArray<string>;
  /** Subscribe to new message events */
  onMessage(handler: (info: Readonly<{ conversationId: string; messageId: string; role: string }>) => void): Disposable;
};

/** ctx.app — application-level read-only info */
export type PluginAppAPI = {
  /** Get current theme info */
  getTheme(): ThemeSnapshot;
  /** Get a read-only snapshot of app settings for a category */
  getSettings(category: 'terminal' | 'appearance' | 'general' | 'buffer' | 'sftp' | 'reconnect'): Readonly<Record<string, unknown>>;
  /** Get application version string */
  getVersion(): string;
  /** Get host platform */
  getPlatform(): 'macos' | 'windows' | 'linux';
  /** Get current UI locale */
  getLocale(): string;
  /** Subscribe to theme changes */
  onThemeChange(handler: (theme: ThemeSnapshot) => void): Disposable;
  /** Subscribe to settings changes in a category */
  onSettingsChange(category: string, handler: (settings: Readonly<Record<string, unknown>>) => void): Disposable;
  /** Get connection pool statistics */
  getPoolStats(): Promise<PoolStatsSnapshot>;
  /** Refresh host stores after a plugin applies external sync results */
  refreshAfterExternalSync(options?: {
    connections?: boolean;
    savedForwards?: boolean;
    settings?: boolean;
  }): Promise<void>;
};

/** The full PluginContext passed to activate() (v3) */
export type PluginContext = Readonly<{
  pluginId: string;
  connections: PluginConnectionsAPI;
  events: PluginEventsAPI;
  ui: PluginUIAPI;
  terminal: PluginTerminalAPI;
  settings: PluginSettingsAPI;
  i18n: PluginI18nAPI;
  storage: PluginStorageAPI;
  sync: PluginSyncAPI;
  secrets: PluginSecretsAPI;
  api: PluginBackendAPI;
  assets: PluginAssetsAPI;
  /** Remote file system operations via SFTP */
  sftp: PluginSftpAPI;
  /** Port forwarding management */
  forward: PluginForwardAPI;
  /** v3: Session tree (read-only) */
  sessions: PluginSessionsAPI;
  /** v3: SFTP transfer monitoring */
  transfers: PluginTransfersAPI;
  /** v3: Resource profiler */
  profiler: PluginProfilerAPI;
  /** v3: Event log */
  eventLog: PluginEventLogAPI;
  /** v3: IDE mode (read-only) */
  ide: PluginIdeAPI;
  /** v3: AI chat (read-only) */
  ai: PluginAiAPI;
  /** v3: Application info */
  app: PluginAppAPI;
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
      /** Plugin API version (3 = current) */
      pluginApiVersion: number;
    };
  }
}
