/**
 * @oxideterm/plugin-api — Type Definitions for OxideTerm Plugin Development
 *
 * This file provides complete TypeScript type definitions for building
 * OxideTerm plugins. Copy this file into your plugin project for
 * full IntelliSense and type-checking support.
 *
 * Plugin API version: 3
 * Minimum OxideTerm version: >=1.6.2
 *
 * Usage:
 *   1. Copy this file to your plugin project root
 *   2. Reference it in your tsconfig.json:
 *      { "include": ["plugin-api.d.ts", "src/**\/*.ts"] }
 *   3. Import types: import type { PluginContext } from './plugin-api';
 *
 * @see https://github.com/AnalyseDeCircuit/oxideterm/blob/main/docs/reference/PLUGIN_DEVELOPMENT.md
 */

// ═══════════════════════════════════════════════════════════════════════════
// Plugin Manifest (plugin.json)
// ═══════════════════════════════════════════════════════════════════════════

export type PluginTabDef = {
  id: string;
  title: string;
  icon: string;
};

export type PluginSidebarDef = {
  id: string;
  title: string;
  icon: string;
  position: 'top' | 'bottom';
};

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

export type PluginSettingDef = {
  id: string;
  type: 'string' | 'number' | 'boolean' | 'select';
  default: unknown;
  title: string;
  description?: string;
  options?: Array<{ label: string; value: string | number }>;
};

export type PluginTerminalHooksDef = {
  inputInterceptor?: boolean;
  outputProcessor?: boolean;
  shortcuts?: Array<{ key: string; command: string }>;
};

export type ConnectionHookType = 'onConnect' | 'onDisconnect' | 'onReconnect' | 'onLinkDown' | 'onIdle';

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
    apiCommands?: string[];
  };
  locales?: string;
};

// ═══════════════════════════════════════════════════════════════════════════
// Plugin Lifecycle
// ═══════════════════════════════════════════════════════════════════════════

export type PluginModule = {
  activate: (ctx: PluginContext) => void | Promise<void>;
  deactivate?: () => void | Promise<void>;
};

// ═══════════════════════════════════════════════════════════════════════════
// Core Primitives
// ═══════════════════════════════════════════════════════════════════════════

export type Disposable = {
  dispose(): void;
};

export type SshConnectionState =
  | 'connecting'
  | 'active'
  | 'idle'
  | 'link_down'
  | 'reconnecting'
  | 'disconnecting'
  | 'disconnected'
  | { error: string };

// ═══════════════════════════════════════════════════════════════════════════
// Snapshot Types (all deeply frozen at runtime)
// ═══════════════════════════════════════════════════════════════════════════

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

export type IdeFileSnapshot = Readonly<{
  path: string;
  name: string;
  language: string;
  isDirty: boolean;
  isActive: boolean;
  isPinned: boolean;
}>;

export type IdeProjectSnapshot = Readonly<{
  nodeId: string;
  rootPath: string;
  name: string;
  isGitRepo: boolean;
  gitBranch?: string;
}>;

export type AiConversationSnapshot = Readonly<{
  id: string;
  title: string;
  messageCount: number;
  createdAt: number;
  updatedAt: number;
}>;

export type AiMessageSnapshot = Readonly<{
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
}>;

export type ThemeSnapshot = Readonly<{
  name: string;
  isDark: boolean;
}>;

export type PoolStatsSnapshot = Readonly<{
  activeConnections: number;
  totalSessions: number;
}>;

// ═══════════════════════════════════════════════════════════════════════════
// Terminal Hook Types
// ═══════════════════════════════════════════════════════════════════════════

export type TerminalHookContext = {
  sessionId: string;
  nodeId: string;
};

export type InputInterceptor = (data: string, context: TerminalHookContext) => string | null;
export type OutputProcessor = (data: Uint8Array, context: TerminalHookContext) => Uint8Array;

// ═══════════════════════════════════════════════════════════════════════════
// UI Types
// ═══════════════════════════════════════════════════════════════════════════

export type PluginTabProps = {
  tabId: string;
  pluginId: string;
};

export type ContextMenuTarget = 'terminal' | 'sftp' | 'tab' | 'sidebar';

export type ContextMenuItem = {
  label: string;
  icon?: string;
  handler: () => void;
  when?: () => boolean;
};

export type StatusBarItemOptions = {
  text: string;
  icon?: string;
  tooltip?: string;
  alignment: 'left' | 'right';
  priority?: number;
  onClick?: () => void;
};

export type StatusBarHandle = {
  update(options: Partial<StatusBarItemOptions>): void;
  dispose(): void;
};

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
// SFTP / Forwarding Types
// ═══════════════════════════════════════════════════════════════════════════

export type PluginFileInfo = Readonly<{
  name: string;
  path: string;
  file_type: 'file' | 'directory' | 'symlink' | 'unknown';
  size: number;
  modified: number | null;
  permissions: string | null;
}>;

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

export type SavedConnectionSnapshot = Readonly<{
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
  tags: readonly string[];
  agent_forwarding: boolean;
  proxy_chain: readonly Readonly<{
    host: string;
    port: number;
    username: string;
    auth_type: 'password' | 'key' | 'agent';
    key_path?: string;
    agent_forwarding?: boolean;
  }>[];
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
  records: readonly SavedConnectionSyncRecord[];
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
  records: readonly SavedForwardSyncRecord[];
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
  code: string;
  message: string;
  applied: boolean;
  normalizedValue?: unknown;
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

export type OxideMetadata = Readonly<{
  exported_at: string;
  exported_by: string;
  description?: string;
  num_connections: number;
  connection_names: readonly string[];
  has_app_settings?: boolean;
  plugin_settings_count?: number;
}>;

export type ImportResult = Readonly<{
  imported: number;
  skipped: number;
  merged: number;
  replaced: number;
  renamed: number;
  errors: readonly string[];
  renames: readonly [string, string][];
  importedAppSettings: boolean;
  skippedAppSettings: boolean;
  importedPluginSettings: number;
  skippedPluginSettings: boolean;
  importedForwards: number;
  skippedForwards: number;
}>;

export type ImportPreviewRecord = Readonly<{
  resource: 'connection';
  name: string;
  action: 'import' | 'rename' | 'skip' | 'replace' | 'merge';
  reasonCode: 'new-connection' | 'name-conflict' | 'name-conflict-skipped' | 'replace-existing' | 'merge-existing';
  targetName?: string;
  targetConnectionId?: string;
  forwardCount: number;
  hasEmbeddedKeys: boolean;
}>;

export type ImportPreviewForward = Readonly<{
  ownerConnectionName: string;
  direction: 'local' | 'remote' | 'dynamic';
  description: string;
}>;

export type ImportPreviewAppSettingsSection = Readonly<{
  id: string;
  fieldKeys: readonly string[];
  fieldValues?: Readonly<Record<string, string>>;
  containsEnvVars?: boolean;
}>;

export type OxideAppSettingsSectionId =
  | 'general'
  | 'terminalAppearance'
  | 'terminalBehavior'
  | 'appearance'
  | 'connections'
  | 'fileAndEditor'
  | 'localTerminal';

export type ImportPreview = Readonly<{
  totalConnections: number;
  unchanged: readonly string[];
  willRename: readonly [string, string][];
  willSkip: readonly string[];
  willReplace: readonly string[];
  willMerge: readonly string[];
  hasEmbeddedKeys: boolean;
  totalForwards: number;
  hasAppSettings: boolean;
  appSettingsFormat?: 'legacy' | 'sectioned';
  appSettingsKeys?: readonly string[];
  appSettingsPreview?: Readonly<Record<string, string>>;
  appSettingsSections?: ReadonlyArray<ImportPreviewAppSettingsSection>;
  pluginSettingsCount: number;
  pluginSettingsByPlugin: Readonly<Record<string, number>>;
  forwardDetails: ReadonlyArray<ImportPreviewForward>;
  records: ReadonlyArray<ImportPreviewRecord>;
}>;

export type ExportPreflightResult = Readonly<{
  totalConnections: number;
  missingKeys: readonly [string, string][];
  connectionsWithKeys: number;
  connectionsWithPasswords: number;
  connectionsWithAgent: number;
  totalKeyBytes: number;
  canExport: boolean;
}>;

export type PluginSyncConflictStrategy = 'rename' | 'skip' | 'replace' | 'merge';

// ═══════════════════════════════════════════════════════════════════════════
// Plugin Context API (passed to activate())
// ═══════════════════════════════════════════════════════════════════════════

export type PluginContext = Readonly<{
  pluginId: string;

  /** Read-only connection state */
  connections: {
    getAll(): ReadonlyArray<ConnectionSnapshot>;
    get(connectionId: string): ConnectionSnapshot | null;
    getState(connectionId: string): SshConnectionState | null;
    getByNode(nodeId: string): ConnectionSnapshot | null;
  };

  /** Lifecycle events + inter-plugin communication */
  events: {
    onConnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
    onDisconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
    onLinkDown(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
    onReconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
    onIdle(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
    onNodeReady(handler: (info: { nodeId: string; connectionId: string }) => void): Disposable;
    onNodeDisconnected(handler: (info: { nodeId: string }) => void): Disposable;
    on(name: string, handler: (data: unknown) => void): Disposable;
    emit(name: string, data: unknown): void;
  };

  /** View registration and user interaction */
  ui: {
    registerTabView(tabId: string, component: React.ComponentType<PluginTabProps>): Disposable;
    registerSidebarPanel(panelId: string, component: React.ComponentType): Disposable;
    registerCommand(id: string, opts: { label: string; icon?: string; shortcut?: string; section?: string }, handler: () => void): Disposable;
    registerContextMenu(target: ContextMenuTarget, items: ContextMenuItem[]): Disposable;
    registerStatusBarItem(options: StatusBarItemOptions): StatusBarHandle;
    registerKeybinding(keybinding: string, handler: () => void): Disposable;
    openTab(tabId: string): void;
    showToast(opts: { title: string; description?: string; variant?: 'default' | 'success' | 'error' | 'warning' }): void;
    showConfirm(opts: { title: string; description: string }): Promise<boolean>;
    showNotification(opts: { title: string; body?: string; severity?: 'info' | 'warning' | 'error' }): void;
    showProgress(title: string): ProgressReporter;
    getLayout(): Readonly<{ sidebarCollapsed: boolean; activeTabId: string | null; tabCount: number }>;
    onLayoutChange(handler: (layout: Readonly<{ sidebarCollapsed: boolean; activeTabId: string | null; tabCount: number }>) => void): Disposable;
  };

  /** Terminal hooks and utilities */
  terminal: {
    registerInputInterceptor(handler: InputInterceptor): Disposable;
    registerOutputProcessor(handler: OutputProcessor): Disposable;
    registerShortcut(command: string, handler: () => void): Disposable;
    getActiveTarget(): PluginActiveTerminalTarget | null;
    writeToActive(text: string): boolean;
    writeToNode(nodeId: string, text: string): void;
    getNodeBuffer(nodeId: string): string | null;
    getNodeSelection(nodeId: string): string | null;
    search(nodeId: string, query: string, options?: { caseSensitive?: boolean; regex?: boolean; wholeWord?: boolean }): Promise<Readonly<{ matches: ReadonlyArray<unknown>; total_matches: number }>>;
    getScrollBuffer(nodeId: string, startLine: number, count: number): Promise<ReadonlyArray<Readonly<{ text: string; lineNumber: number }>>>;
    getBufferSize(nodeId: string): Promise<Readonly<{ currentLines: number; totalLines: number; maxLines: number }>>;
    clearBuffer(nodeId: string): Promise<void>;
  };

  /** Plugin-scoped settings */
  settings: {
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

  /** Plugin-scoped i18n */
  i18n: {
    t(key: string, params?: Record<string, string | number>): string;
    getLanguage(): string;
    onLanguageChange(handler: (lang: string) => void): Disposable;
  };

  /** Plugin-scoped persistent key-value storage */
  storage: {
    get<T>(key: string): T | null;
    set<T>(key: string, value: T): void;
    remove(key: string): void;
  };

  /** Saved-connection sync helpers backed by encrypted .oxide import/export */
  sync: {
    listSavedConnections(): ReadonlyArray<SavedConnectionSnapshot>;
    refreshSavedConnections(): Promise<ReadonlyArray<SavedConnectionSnapshot>>;
    onSavedConnectionsChange(handler: (connections: ReadonlyArray<SavedConnectionSnapshot>) => void): Disposable;
    exportSavedConnectionsSnapshot(): Promise<SavedConnectionsSyncSnapshot>;
    applySavedConnectionsSnapshot(
      snapshot: SavedConnectionsSyncSnapshot,
      options?: { conflictStrategy?: Extract<PluginSyncConflictStrategy, 'skip' | 'replace' | 'merge'> },
    ): Promise<ApplySavedConnectionsSyncSnapshotResult>;
    getLocalSyncMetadata(): Promise<LocalSyncMetadata>;
    preflightExport(connectionIds?: string[], options?: { embedKeys?: boolean }): Promise<ExportPreflightResult>;
    exportOxide(request: {
      connectionIds?: string[];
      password: string;
      description?: string;
      embedKeys?: boolean;
      includeAppSettings?: boolean;
      selectedAppSettingsSections?: readonly OxideAppSettingsSectionId[];
      includeLocalTerminalEnvVars?: boolean;
      includePluginSettings?: boolean;
      selectedPluginIds?: string[];
      selectedForwardIds?: string[];
      onProgress?: (progress: PluginOxideExportProgress) => void;
    }): Promise<Uint8Array>;
    validateOxide(fileData: Uint8Array): Promise<OxideMetadata>;
    previewImport(
      fileData: Uint8Array,
      password: string,
      options?: {
        conflictStrategy?: PluginSyncConflictStrategy;
        onProgress?: (progress: PluginOxideImportProgress) => void;
      },
    ): Promise<ImportPreview>;
    importOxide(
      fileData: Uint8Array,
      password: string,
      options?: {
        selectedNames?: string[];
        conflictStrategy?: PluginSyncConflictStrategy;
        importAppSettings?: boolean;
        selectedAppSettingsSections?: readonly string[];
        importPluginSettings?: boolean;
        selectedPluginIds?: string[];
        importForwards?: boolean;
        onProgress?: (progress: PluginOxideImportProgress) => void;
      },
    ): Promise<ImportResult>;
  };

  /** Plugin-scoped secure secret storage backed by the OS keychain */
  secrets: {
    get(key: string): Promise<string | null>;
    getMany(keys: readonly string[]): Promise<Readonly<Record<string, string | null>>>;
    set(key: string, value: string): Promise<void>;
    has(key: string): Promise<boolean>;
    delete(key: string): Promise<void>;
  };

  /** Restricted backend invocation (whitelist-gated) */
  api: {
    invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  };

  /** Static asset loading */
  assets: {
    loadCSS(relativePath: string): Promise<Disposable>;
    getAssetUrl(relativePath: string): Promise<string>;
    revokeAssetUrl(url: string): void;
  };

  /** Remote file system operations via SFTP */
  sftp: {
    listDir(nodeId: string, path: string): Promise<ReadonlyArray<PluginFileInfo>>;
    stat(nodeId: string, path: string): Promise<PluginFileInfo>;
    readFile(nodeId: string, path: string): Promise<string>;
    writeFile(nodeId: string, path: string, content: string): Promise<void>;
    mkdir(nodeId: string, path: string): Promise<void>;
    delete(nodeId: string, path: string): Promise<void>;
    rename(nodeId: string, oldPath: string, newPath: string): Promise<void>;
  };

  /** Port forwarding management */
  forward: {
    list(sessionId: string): Promise<ReadonlyArray<PluginForwardRule>>;
    listSavedForwards(): ReadonlyArray<SavedForwardSnapshot>;
    onSavedForwardsChange(handler: (items: ReadonlyArray<SavedForwardSnapshot>) => void): Disposable;
    exportSavedForwardsSnapshot(): Promise<SavedForwardsSyncSnapshot>;
    applySavedForwardsSnapshot(snapshot: SavedForwardsSyncSnapshot): Promise<ApplySavedForwardsSyncSnapshotResult>;
    create(request: PluginForwardRequest): Promise<{ success: boolean; forward?: PluginForwardRule; error?: string }>;
    stop(sessionId: string, forwardId: string): Promise<void>;
    stopAll(sessionId: string): Promise<void>;
    getStats(sessionId: string, forwardId: string): Promise<{ connectionCount: number; activeConnections: number; bytesSent: number; bytesReceived: number } | null>;
  };

  /** Session tree (read-only, v3) */
  sessions: {
    getTree(): ReadonlyArray<SessionTreeNodeSnapshot>;
    getActiveNodes(): ReadonlyArray<Readonly<{ nodeId: string; sessionId: string | null; connectionState: string }>>;
    getNodeState(nodeId: string): string | null;
    onTreeChange(handler: (tree: ReadonlyArray<SessionTreeNodeSnapshot>) => void): Disposable;
    onNodeStateChange(nodeId: string, handler: (state: string) => void): Disposable;
  };

  /** SFTP transfer monitoring (v3) */
  transfers: {
    getAll(): ReadonlyArray<TransferSnapshot>;
    getByNode(nodeId: string): ReadonlyArray<TransferSnapshot>;
    onProgress(handler: (transfer: TransferSnapshot) => void): Disposable;
    onComplete(handler: (transfer: TransferSnapshot) => void): Disposable;
    onError(handler: (transfer: TransferSnapshot) => void): Disposable;
  };

  /** Resource profiler (v3) */
  profiler: {
    getMetrics(nodeId: string): ProfilerMetricsSnapshot | null;
    getHistory(nodeId: string, maxPoints?: number): ReadonlyArray<ProfilerMetricsSnapshot>;
    isRunning(nodeId: string): boolean;
    onMetrics(nodeId: string, handler: (metrics: ProfilerMetricsSnapshot) => void): Disposable;
  };

  /** Connection event log (v3) */
  eventLog: {
    getEntries(filter?: { severity?: 'info' | 'warn' | 'error'; category?: 'connection' | 'reconnect' | 'node' }): ReadonlyArray<EventLogEntrySnapshot>;
    onEntry(handler: (entry: EventLogEntrySnapshot) => void): Disposable;
  };

  /** IDE mode (read-only, v3) */
  ide: {
    isOpen(): boolean;
    getProject(): IdeProjectSnapshot | null;
    getOpenFiles(): ReadonlyArray<IdeFileSnapshot>;
    getActiveFile(): IdeFileSnapshot | null;
    onFileOpen(handler: (file: IdeFileSnapshot) => void): Disposable;
    onFileClose(handler: (path: string) => void): Disposable;
    onActiveFileChange(handler: (file: IdeFileSnapshot | null) => void): Disposable;
  };

  /** AI chat (read-only, v3) */
  ai: {
    getConversations(): ReadonlyArray<AiConversationSnapshot>;
    getMessages(conversationId: string): ReadonlyArray<AiMessageSnapshot>;
    getActiveProvider(): Readonly<{ type: string; displayName: string }> | null;
    getAvailableModels(): ReadonlyArray<string>;
    onMessage(handler: (info: Readonly<{ conversationId: string; messageId: string; role: string }>) => void): Disposable;
  };

  /** Application info (v3) */
  app: {
    getTheme(): ThemeSnapshot;
    getSettings(category: 'terminal' | 'appearance' | 'general' | 'buffer' | 'sftp' | 'reconnect'): Readonly<Record<string, unknown>>;
    getVersion(): string;
    getPlatform(): 'macos' | 'windows' | 'linux';
    getLocale(): string;
    onThemeChange(handler: (theme: ThemeSnapshot) => void): Disposable;
    onSettingsChange(category: string, handler: (settings: Readonly<Record<string, unknown>>) => void): Disposable;
    getPoolStats(): Promise<PoolStatsSnapshot>;
    refreshAfterExternalSync(options?: {
      connections?: boolean;
      savedForwards?: boolean;
      settings?: boolean;
    }): Promise<void>;
  };
}>;

// ═══════════════════════════════════════════════════════════════════════════
// Host Shared Modules (window.__OXIDE__)
// ═══════════════════════════════════════════════════════════════════════════

declare global {
  interface Window {
    __OXIDE__?: {
      React: typeof import('react');
      ReactDOM: { createRoot: (container: Element) => { render(element: unknown): void; unmount(): void } };
      zustand: { create: <T>(initializer: (set: (partial: Partial<T>) => void, get: () => T) => T) => () => T };
      lucideIcons: Record<string, React.ForwardRefExoticComponent<React.SVGProps<SVGSVGElement>>>;
      ui: Record<string, React.ComponentType<Record<string, unknown>>>;
      clsx: (...inputs: unknown[]) => string;
      cn: (...inputs: unknown[]) => string;
      useTranslation: (ns?: string) => { t: (key: string, params?: Record<string, unknown>) => string };
      version: string;
      pluginApiVersion: number;
    };
  }
}
