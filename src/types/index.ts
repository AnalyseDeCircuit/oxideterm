// Session Types
export type SessionState = 'disconnected' | 'connecting' | 'connected' | 'error' | 'reconnecting';
export type AuthType = 'password' | 'key' | 'default_key' | 'agent' | 'certificate' | 'keyboard_interactive';

// ═══════════════════════════════════════════════════════════════════════════
// SSH Connection Pool Types (New Architecture)
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Connection state in the connection pool
 */
export type SshConnectionState = 
  | 'connecting' 
  | 'active' 
  | 'idle' 
  | 'link_down'      // Heartbeat failed, waiting for reconnect
  | 'reconnecting'   // Attempting to reconnect
  | 'disconnecting' 
  | 'disconnected' 
  | { error: string };

/**
 * SSH connection info from the connection pool
 */
export interface SshConnectionInfo {
  id: string;
  host: string;
  port: number;
  username: string;
  state: SshConnectionState;
  refCount: number;
  keepAlive: boolean;
  createdAt: string;
  lastActive: string;
  terminalIds: string[];
  sftpSessionId?: string;
  forwardIds: string[];
  /** Parent connection ID for tunneled connections */
  parentConnectionId?: string;
}

/**
 * Connection pool configuration
 */
export interface ConnectionPoolConfig {
  idleTimeoutSecs: number;
  maxConnections: number;
  protectOnExit: boolean;
}

/**
 * Connection pool statistics (for monitoring panel)
 */
export interface ConnectionPoolStats {
  /** Total number of connections */
  totalConnections: number;
  /** Active connections (with terminals/SFTP/forwards in use) */
  activeConnections: number;
  /** Idle connections (no users, waiting for timeout) */
  idleConnections: number;
  /** Connections in reconnecting state */
  reconnectingConnections: number;
  /** Connections with link down (waiting for reconnect) */
  linkDownConnections: number;
  /** Total terminal count */
  totalTerminals: number;
  /** Total SFTP session count */
  totalSftpSessions: number;
  /** Total port forward count */
  totalForwards: number;
  /** Total reference count */
  totalRefCount: number;
  /** Pool capacity (0 = unlimited) */
  poolCapacity: number;
  /** Idle timeout in seconds */
  idleTimeoutSecs: number;
}

// ═══════════════════════════════════════════════════════════════════════════
// SSH Host Key Preflight (TOFU - Trust On First Use)
// ═══════════════════════════════════════════════════════════════════════════

/**
 * SSH preflight request - check host key before connecting
 */
export interface SshPreflightRequest {
  host: string;
  port: number;
}

/**
 * Host key status from preflight check
 */
export type HostKeyStatus =
  | { status: 'verified' }
  | { status: 'unknown'; fingerprint: string; keyType: string }
  | { status: 'changed'; expectedFingerprint: string; actualFingerprint: string; keyType: string }
  | { status: 'error'; message: string };

/**
 * SSH preflight response
 */
export type SshPreflightResponse = HostKeyStatus;

/**
 * Accept host key request - trust after user confirmation
 */
export interface AcceptHostKeyRequest {
  host: string;
  port: number;
  fingerprint: string;
  /** true = save to known_hosts, false = trust for session only */
  persist: boolean;
}

// ═══════════════════════════════════════════════════════════════════════════
// Keyboard-Interactive (2FA) Authentication Types
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Individual prompt in a KBI InfoRequest
 */
export interface KbiPrompt {
  /** The prompt text (e.g., "Password:", "Verification code:") */
  prompt: string;
  /** Whether to echo the input (false for passwords/codes) */
  echo: boolean;
}

/**
 * KBI prompt event - emitted when server requests input
 * Event name: "ssh_kbi_prompt"
 */
export interface KbiPromptEvent {
  /** Unique ID for this auth flow (UUID) */
  authFlowId: string;
  /** Optional name from server (often empty) */
  name: string;
  /** Optional instructions from server */
  instructions: string;
  /** Prompts the user must respond to */
  prompts: KbiPrompt[];
}

/**
 * KBI result event - emitted when auth flow completes
 * Event name: "ssh_kbi_result"
 */
export interface KbiResultEvent {
  authFlowId: string;
  success: boolean;
  error?: string;
  sessionId?: string;
  wsPort?: number;
  wsToken?: string;
}

/**
 * KBI respond request - sent from frontend to backend
 */
export interface KbiRespondRequest {
  authFlowId: string;
  responses: string[];
}

/**
 * KBI cancel request - sent from frontend to backend
 */
export interface KbiCancelRequest {
  authFlowId: string;
}

/**
 * Create terminal request
 */
export interface CreateTerminalRequest {
  connectionId: string;
  cols?: number;
  rows?: number;
  maxBufferLines?: number;
}

/**
 * Create terminal response
 */
export interface CreateTerminalResponse {
  sessionId: string;
  wsUrl: string;
  port: number;
  wsToken: string;
  session: SessionInfo;
}

// ═══════════════════════════════════════════════════════════════════════════
// Global Event Map Extensions (TS 5.8+ strict typing for custom events)
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Settings changed event detail - matches PersistedSettings from SettingsModal
 */
export interface SettingsChangedDetail {
  theme: string;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  cursorStyle: 'block' | 'underline' | 'bar';
  cursorBlink: boolean;
  scrollback: number;
  bufferMaxLines: number;
  bufferSaveOnDisconnect: boolean;
  sidebarCollapsedDefault: boolean;
  defaultUsername: string;
  defaultPort: number;
}

declare global {
  interface WindowEventMap {
    'settings-changed': CustomEvent<SettingsChangedDetail>;
  }
}

// ═══════════════════════════════════════════════════════════════════════════

export interface SessionInfo {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  state: SessionState;
  error?: string;
  ws_url?: string;
  ws_token?: string; // Authentication token for WebSocket connection
  color: string;
  uptime_secs: number;
  order: number; // Tab order
  // Connection pool integration (新架构)
  connectionId?: string; // 关联的 SSH 连接 ID
  // Authentication info for reconnection
  auth_type: AuthType;
  key_path?: string; // Only for key auth (password is never stored)
  // Reconnection state
  reconnectAttempt?: number;
  reconnectMaxAttempts?: number;
  reconnectNextRetry?: number; // timestamp in milliseconds
}

export interface ProxyHopConfig {
  id: string;
  host: string;
  port: number;
  username: string;
  auth_type: 'password' | 'key' | 'default_key' | 'agent';
  password?: string;
  key_path?: string;
  passphrase?: string;
}

export interface BufferConfig {
  max_lines: number;
  save_on_disconnect: boolean;
}

export interface ConnectRequest {
  host: string;
  port: number;
  username: string;
  auth_type: 'password' | 'key' | 'default_key' | 'agent';
  password?: string;
  key_path?: string;
  passphrase?: string;
  cols?: number;
  rows?: number;
  name?: string;
  group?: string;
  proxy_chain?: ProxyHopConfig[];
  buffer_config?: BufferConfig;
}

// Persisted Session Types
export interface PersistedSessionInfo {
  id: string;
  host: string;
  port: number;
  username: string;
  name?: string;
  created_at: string;
  order: number;
}

// Tab Types
export type TabType = 'terminal' | 'sftp' | 'forwards' | 'settings' | 'connection_monitor' | 'connection_pool' | 'topology' | 'local_terminal' | 'ide' | 'file_manager';

// ═══════════════════════════════════════════════════════════════════════════
// Split Pane Types (Layout Tree)
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Terminal type for panes
 */
export type PaneTerminalType = 'terminal' | 'local_terminal';

/**
 * Leaf node: An actual terminal pane
 */
export interface PaneLeaf {
  type: 'leaf';
  id: string;                       // Unique pane ID (UUID)
  sessionId: string;                // Associated terminal session
  terminalType: PaneTerminalType;   // SSH or Local terminal
}

/**
 * Split direction for pane groups
 */
export type SplitDirection = 'horizontal' | 'vertical';

/**
 * Group node: A container for multiple panes
 */
export interface PaneGroup {
  type: 'group';
  id: string;                       // Unique group ID (UUID)
  direction: SplitDirection;        // Split direction
  children: PaneNode[];             // Child panes or groups
  sizes?: number[];                 // Percentage sizes for each child (0-100)
}

/**
 * A node in the pane layout tree
 */
export type PaneNode = PaneLeaf | PaneGroup;

/**
 * Maximum number of panes allowed per tab
 */
export const MAX_PANES_PER_TAB = 4;

export interface Tab {
  id: string;
  type: TabType;
  title: string;
  icon?: string;
  
  // Split Pane Support (for terminal/local_terminal tabs)
  rootPane?: PaneNode;              // Layout tree root (null = single pane mode)
  activePaneId?: string;            // Currently focused pane within this tab
  
  // Legacy: Direct session binding (backward compatible, used when rootPane is undefined)
  sessionId?: string;
}

// Connection Config Types

/**
 * Proxy hop info for display (without sensitive credentials)
 * Corresponds to backend ProxyHopInfo
 */
export interface ProxyHopInfo {
  host: string;
  port: number;
  username: string;
  auth_type: 'password' | 'key' | 'agent';
  key_path?: string;
}

export interface ConnectionInfo {
  id: string;
  name: string;
  group: string | null;
  host: string;
  port: number;
  username: string;
  auth_type: 'password' | 'key' | 'agent';
  key_path: string | null;
  created_at: string;
  last_used_at: string | null;
  color: string | null;
  tags: string[];
  proxy_chain?: ProxyHopInfo[];
}

export interface OxideMetadata {
  exported_at: string;
  exported_by: string;
  description?: string;
  num_connections: number;
  connection_names: string[];
}

export interface ImportResult {
  imported: number;
  skipped: number;
  renamed: number;
  errors: string[];
  /** List of name changes: [original_name, new_name][] */
  renames: [string, string][];
}

export interface ImportPreview {
  /** Total number of connections in the file */
  totalConnections: number;
  /** Connections that will be imported without changes */
  unchanged: string[];
  /** Connections that will be renamed: [original_name, new_name][] */
  willRename: [string, string][];
  /** Whether any embedded keys will be extracted */
  hasEmbeddedKeys: boolean;
}

export interface ExportPreflightResult {
  /** Total connections to export */
  totalConnections: number;
  /** Connections with missing private keys: [name, key_path][] */
  missingKeys: [string, string][];
  /** Connections using key authentication */
  connectionsWithKeys: number;
  /** Connections using password authentication */
  connectionsWithPasswords: number;
  /** Connections using SSH agent */
  connectionsWithAgent: number;
  /** Total bytes of key files (if embed_keys is enabled) */
  totalKeyBytes: number;
  /** Whether all connections can be exported */
  canExport: boolean;
}

export interface SaveConnectionRequest {
  id?: string;
  name: string;
  group: string | null;
  host: string;
  port: number;
  username: string;
  auth_type: 'password' | 'key' | 'agent' | 'certificate';
  password?: string;
  key_path?: string;
  cert_path?: string;
  color?: string;
  tags?: string[];
}

// Terminal Config
export interface TerminalConfig {
  themeId: string;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  letterSpacing: number;
  cursorStyle: 'block' | 'underline' | 'bar';
  cursorBlink: boolean;
  cursorWidth: number;
  scrollback: number;
  rightClickSelectsWord: boolean;
  macOptionIsMeta: boolean;
  altClickMovesCursor: boolean;
  bellStyle: 'none' | 'sound' | 'visual' | 'both';
  linkHandler: boolean;
}

// App Settings
export interface AppSettings {
  sidebarDefaultCollapsed: boolean;
  defaultPort: number;
  defaultUsername: string;
}

// SFTP Types
export type FileType = 'File' | 'Directory' | 'Symlink' | 'Unknown';

export interface FileInfo {
  name: string;
  path: string;
  file_type: FileType;
  size: number;
  modified: number | null;
  permissions: string | null;
}

// SFTP Sort Order
export type SortOrder = 'Name' | 'NameDesc' | 'Size' | 'SizeDesc' | 'Modified' | 'ModifiedDesc' | 'Type' | 'TypeDesc';

// SFTP List Filter
export interface ListFilter {
  show_hidden?: boolean;
  pattern?: string | null;
  sort?: SortOrder;
}

export type PreviewContent =
  | { Text: { 
      data: string; 
      mime_type: string | null; 
      language: string | null;
      /** Detected encoding (e.g., "UTF-8", "GBK", "Shift_JIS") */
      encoding: string;
      /** Detection confidence (0.0 - 1.0) */
      confidence?: number;
      /** Whether file has BOM (Byte Order Mark) */
      has_bom?: boolean;
    } }
  | { Image: { data: string; mime_type: string } }
  | { Video: { data: string; mime_type: string } }
  | { Audio: { data: string; mime_type: string } }
  | { Pdf: { data: string; original_mime: string | null } }
  | { Office: { data: string; mime_type: string } }
  | { Hex: { data: string; total_size: number; offset: number; chunk_size: number; has_more: boolean } }
  | { TooLarge: { size: number; max_size: number; recommend_download: boolean } }
  | { Unsupported: { mime_type: string; reason: string } };

export interface TransferProgress {
  transferred: number;
  total: number;
  percentage: number;
  state: 'Pending' | 'InProgress' | 'Completed' | { Failed: string };
}

// Port Forwarding Types
export type ForwardType = 'local' | 'remote' | 'dynamic';

export interface ForwardRequest {
  session_id: string;
  forward_type: ForwardType;
  bind_address: string;
  bind_port: number;
  target_host: string;
  target_port: number;
  description?: string;
  check_health?: boolean; // Default: true - check port availability before creating forward
}

// Persisted Forward Types
export interface PersistedForwardInfo {
  id: string;
  session_id: string;
  forward_type: string;
  bind_address: string;
  bind_port: number;
  target_host: string;
  target_port: number;
  auto_start: boolean;
  created_at: string;
}

export interface ForwardRule {
  id: string;
  forward_type: ForwardType;
  bind_address: string;
  bind_port: number;
  target_host: string;
  target_port: number;
  status: 'starting' | 'active' | 'stopped' | 'error' | 'suspended';
  description?: string;
}

// Forward Response from backend
export interface ForwardRuleDto {
  id: string;
  forward_type: string;
  bind_address: string;
  bind_port: number;
  target_host: string;
  target_port: number;
  status: string;
  description?: string;
}

export interface ForwardResponse {
  success: boolean;
  forward?: ForwardRuleDto;
  error?: string;
}

// Session Stats
export interface SessionStats {
  total: number;
  connected: number;
  connecting: number;
  error: number;
  max_sessions?: number;
}

// Quick Health Check
export interface QuickHealthCheck {
  session_id: string;
  status: HealthStatus;
  latency_ms: number | null;
  message: string;
}

// Health Types
export interface HealthMetrics {
  session_id: string;
  uptime_secs: number;
  ping_sent: number;
  ping_received: number;
  avg_latency_ms: number | null;
  last_latency_ms: number | null;
  status: 'Healthy' | 'Degraded' | 'Unresponsive' | 'Disconnected' | 'Unknown';
}

export type HealthStatus = 'Healthy' | 'Degraded' | 'Unresponsive' | 'Disconnected' | 'Unknown';

// Resource Profiler Types
export type MetricsSource = 'full' | 'partial' | 'rtt_only' | 'failed';

export type ResourceMetrics = {
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
  source: MetricsSource;
};

// SSH Types
export interface SshHostInfo {
    alias: string;
    hostname: string;
    user: string | null;
    port: number;
    identity_file: string | null;
}

export interface SshKeyInfo {
  name: string;
  path: string;
  key_type: string;
  has_passphrase: boolean;
}

// Scroll Buffer Types
export interface TerminalLine {
  text: string;
  timestamp: number;
}

export interface BufferStats {
  current_lines: number;
  total_lines: number;
  max_lines: number;
  memory_usage_mb: number;
}

// Search Types
export interface SearchOptions {
  query: string;
  case_sensitive: boolean;
  regex: boolean;
  whole_word: boolean;
}

export interface SearchMatch {
  line_number: number;
  column_start: number;
  column_end: number;
  matched_text: string;
  line_content: string;
}

export interface SearchResult {
  matches: SearchMatch[];
  total_matches: number;
  duration_ms: number;
  /** Error message if regex is invalid */
  error?: string;
}

// SFTP Resume Transfer Types
export type TransferStatusType = 'Active' | 'Paused' | 'Failed' | 'Completed' | 'Cancelled';
export type TransferType = 'Upload' | 'Download';

/**
 * Stored transfer progress from persistent storage
 * Corresponds to backend StoredTransferProgress
 */
export interface StoredTransferProgress {
  transfer_id: string;
  transfer_type: TransferType;
  source_path: string;
  destination_path: string;
  transferred_bytes: number;
  total_bytes: number;
  status: TransferStatusType;
  last_updated: string; // ISO datetime
  session_id: string;
  error?: string;
}

/**
 * Incomplete transfer info for UI display
 */
export interface IncompleteTransferInfo {
  transfer_id: string;
  transfer_type: TransferType;
  source_path: string;
  destination_path: string;
  transferred_bytes: number;
  total_bytes: number;
  status: TransferStatusType;
  session_id: string;
  error?: string;
  progress_percent: number;
  can_resume: boolean;
}

// ═══════════════════════════════════════════════════════════════════════════
// Session Tree Types (Dynamic Jump Host)
// ═══════════════════════════════════════════════════════════════════════════

/**
 * 节点状态 (原有类型，用于后端兼容)
 */
export type TreeNodeState = 
  | { status: 'pending' }
  | { status: 'connecting' }
  | { status: 'connected' }
  | { status: 'disconnected' }
  | { status: 'failed'; error: string };

/**
 * 统一节点状态 (前端扩展)
 * NodeState = f(ConnectionStatus, TerminalSessionCount)
 */
export type UnifiedNodeStatus = 
  | 'idle'         // 灰色 - 未连接
  | 'connecting'   // 蓝色脉冲 - 正在连接
  | 'connected'    // 绿色空心 - 已连接无终端
  | 'active'       // 绿色实心 - 已连接有终端
  | 'link-down'    // 橙色 - 父节点断开
  | 'error';       // 红色 - 连接失败

/**
 * 节点运行时状态 (非持久化)
 * 作为 Single Source of Truth 的核心数据结构
 */
export interface NodeRuntimeState {
  /** 临时挂载的 SSH 连接句柄 (后端生成) */
  connectionId: string | null;
  /** 计算后的统一状态 */
  status: UnifiedNodeStatus;
  /** 关联的终端会话ID列表 */
  terminalIds: string[];
  /** SFTP 会话ID */
  sftpSessionId: string | null;
  /** 错误信息 */
  errorMessage?: string;
  /** 上次连接时间 */
  lastConnectedAt?: number;
}

/**
 * 扩展的 FlatNode - 包含运行时状态
 * 用于 UI 渲染的完整节点信息
 */
export interface UnifiedFlatNode extends FlatNode {
  /** 运行时状态 (前端管理) */
  runtime: NodeRuntimeState;
  /** 是否展开 */
  isExpanded: boolean;
  /** 连接线指示器 */
  lineGuides: boolean[];
}

/**
 * 节点来源类型
 */
export type TreeNodeOriginType = 
  | 'manual_preset'  // 模式1: 静态全手工
  | 'auto_route'     // 模式2: 静态自动计算
  | 'drill_down'     // 模式3: 动态钻入
  | 'direct'         // 直接连接
  | 'restored';      // 从配置恢复

/**
 * 扁平化节点 - 用于前端渲染
 */
export interface FlatNode {
  id: string;
  parentId: string | null;
  depth: number;
  host: string;
  port: number;
  username: string;
  displayName: string | null;
  state: TreeNodeState;
  hasChildren: boolean;
  isLastChild: boolean;
  originType: TreeNodeOriginType;
  terminalSessionId: string | null;
  sftpSessionId: string | null;
  sshConnectionId: string | null;
}

/**
 * 会话树摘要
 */
export interface SessionTreeSummary {
  totalNodes: number;
  rootCount: number;
  connectedCount: number;
  maxDepth: number;
}

/**
 * 连接服务器请求
 */
export interface ConnectServerRequest {
  host: string;
  port: number;
  username: string;
  authType?: 'password' | 'key' | 'agent' | 'certificate' | 'keyboard_interactive';
  password?: string;
  keyPath?: string;
  certPath?: string;
  passphrase?: string;
  displayName?: string;
}

/**
 * 钻入请求
 */
export interface DrillDownRequest {
  parentNodeId: string;
  host: string;
  port: number;
  username: string;
  authType?: 'password' | 'key' | 'agent' | 'certificate';
  password?: string;
  keyPath?: string;
  certPath?: string;
  passphrase?: string;
  displayName?: string;
}

/**
 * 跳板机信息
 */
export interface HopInfo {
  host: string;
  port: number;
  username: string;
  authType?: 'password' | 'key' | 'agent' | 'certificate';
  password?: string;
  keyPath?: string;
  certPath?: string;
  passphrase?: string;
}

/**
 * 预设链连接请求
 */
export interface ConnectPresetChainRequest {
  savedConnectionId: string;
  hops: HopInfo[];
  target: HopInfo;
}

/**
 * 连接树节点请求
 */
export interface ConnectTreeNodeRequest {
  nodeId: string;
  cols?: number;
  rows?: number;
}

/**
 * 连接树节点响应
 */
export interface ConnectTreeNodeResponse {
  nodeId: string;
  sshConnectionId: string;
  parentConnectionId?: string;
}

/**
 * 连接手工预设响应
 */
export interface ConnectManualPresetResponse {
  /** 目标节点 ID */
  targetNodeId: string;
  /** 目标节点的 SSH 连接 ID */
  targetSshConnectionId: string;
  /** 所有已连接的节点 ID（从根到目标） */
  connectedNodeIds: string[];
  /** 链的深度（跳板数量 + 1） */
  chainDepth: number;
}

// ===== Auto-Route (Auto-generated from Saved Connections) =====

/**
 * Topology node info (auto-generated from saved connections)
 */
export interface TopologyNodeInfo {
  /** Node ID (same as saved connection ID) */
  id: string;
  /** Display name */
  displayName?: string;
  /** Host address */
  host: string;
  /** SSH port */
  port: number;
  /** Username */
  username: string;
  /** Auth type */
  authType: "password" | "key" | "agent";
  /** Is local node (start point) */
  isLocal: boolean;
  /** Neighbor nodes (reachable next hops) */
  neighbors: string[];
  /** Tags */
  tags?: string[];
  /** Reference to saved connection ID */
  savedConnectionId?: string;
}

/**
 * Topology edge (reachability)
 */
export interface TopologyEdge {
  /** Source node ID ("local" = local machine) */
  from: string;
  /** Target node ID */
  to: string;
  /** Cost (hop count, latency, etc.) */
  cost: number;
}

/**
 * Custom edges overlay config (user-editable)
 */
export interface TopologyEdgesConfig {
  /** User-defined custom edges */
  customEdges: TopologyEdge[];
  /** Edges to exclude from auto-generation */
  excludedEdges: TopologyEdge[];
}

/**
 * Expand auto-route request
 */
export interface ExpandAutoRouteRequest {
  /** Target node ID (topology node id) */
  targetId: string;
  /** Optional display name override */
  displayName?: string;
}

/**
 * Expand auto-route response
 */
export interface ExpandAutoRouteResponse {
  /** Target node ID (in SessionTree) */
  targetNodeId: string;
  /** Computed route path (intermediate hop node IDs) */
  route: string[];
  /** Total route cost */
  totalCost: number;
  /** All expanded node IDs (from root to target) */
  allNodeIds: string[];
}

// ═══════════════════════════════════════════════════════════════════════════
// Local Terminal Types
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Information about a detected shell on the system
 */
export interface ShellInfo {
  /** Unique identifier (e.g., "zsh", "bash", "powershell") */
  id: string;
  /** Human-readable label (e.g., "Zsh", "Bash", "PowerShell") */
  label: string;
  /** Full path to the shell executable */
  path: string;
  /** Default arguments (e.g., ["--login"]) */
  args: string[];
}

/**
 * Local terminal session info
 */
export interface LocalTerminalInfo {
  /** Unique session ID */
  id: string;
  /** Shell being used */
  shell: ShellInfo;
  /** Terminal columns */
  cols: number;
  /** Terminal rows */
  rows: number;
  /** Whether the session is running */
  running: boolean;
}

/**
 * Request to create a local terminal
 */
export interface CreateLocalTerminalRequest {
  /** Shell path (optional, uses default if not specified) */
  shellPath?: string;
  /** Terminal columns */
  cols?: number;
  /** Terminal rows */
  rows?: number;
  /** Working directory (optional) */
  cwd?: string;
  /** Whether to load shell profile (default: true) */
  loadProfile?: boolean;
  /** Enable Oh My Posh prompt theme (Windows) */
  ohMyPoshEnabled?: boolean;
  /** Path to Oh My Posh theme file */
  ohMyPoshTheme?: string;
}

/**
 * Response from creating a local terminal
 */
export interface CreateLocalTerminalResponse {
  /** Session ID */
  sessionId: string;
  /** Session info */
  info: LocalTerminalInfo;
}

// ═══════════════════════════════════════════════════════════════════════════
// AI Chat Types
// ═══════════════════════════════════════════════════════════════════════════

/**
 * A single message in an AI conversation
 */
export interface AiChatMessage {
  /** Unique message ID */
  id: string;
  /** Message role */
  role: 'user' | 'assistant' | 'system';
  /** Message content */
  content: string;
  /** Unix timestamp (ms) */
  timestamp: number;
  /** Terminal context attached to this message */
  context?: string;
  /** Whether the message is being streamed */
  isStreaming?: boolean;
  /** Thinking content from extended thinking models (Anthropic) */
  thinkingContent?: string;
  /** Whether the thinking block is expanded in UI */
  isThinkingExpanded?: boolean;
  /** Whether thinking is currently streaming */
  isThinkingStreaming?: boolean;
}

/**
 * A conversation containing multiple messages
 */
export interface AiConversation {
  /** Unique conversation ID */
  id: string;
  /** Conversation title (auto-generated or user-defined) */
  title: string;
  /** Messages in the conversation */
  messages: AiChatMessage[];
  /** Creation timestamp */
  createdAt: number;
  /** Last update timestamp */
  updatedAt: number;
  /** Associated terminal session ID (optional) */
  sessionId?: string;
}
