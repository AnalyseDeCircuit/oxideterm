import { invoke } from '@tauri-apps/api/core';
import {
  SessionInfo,
  ConnectRequest,
  ConnectionInfo,
  SaveConnectionRequest,
  HealthMetrics,
  FileInfo,
  PreviewContent,
  ForwardRequest,
  ForwardRule,
  ForwardResponse,
  SshHostInfo,
  SshKeyInfo,
  PersistedSessionInfo,
  PersistedForwardInfo,
  TerminalLine,
  BufferStats,
  SearchOptions,
  SearchResult,
  SessionStats,
  QuickHealthCheck,
  IncompleteTransferInfo,
  // Connection pool types
  SshConnectionInfo,
  CreateTerminalRequest,
  CreateTerminalResponse,
  ConnectionPoolConfig,
  ConnectionPoolStats,
  // Host key preflight types (TOFU)
  SshPreflightRequest,
  SshPreflightResponse,
  AcceptHostKeyRequest,
  // Resource profiler types
  ResourceMetrics,
  // Remote environment detection
  RemoteEnvInfo,
  // Oxide-Next Node State types
  NodeStateSnapshot,
} from '../types';
import type { PluginManifest } from '../types/plugin';

// Toggle this for development without a backend
const USE_MOCK = false;

// ---------------------------------------------------------------------------
// In-flight Promise dedup — prevents StrictMode double-mount from issuing
// duplicate IPC calls (especially OS keychain access which can trigger macOS
// permission dialogs).
// ---------------------------------------------------------------------------
const _inflight = new Map<string, Promise<unknown>>();

/** Return the in-flight promise for `key`, or start a new one via `fn`. */
function dedup<T>(key: string, fn: () => Promise<T>): Promise<T> {
  const existing = _inflight.get(key);
  if (existing) return existing as Promise<T>;
  const p = fn().finally(() => _inflight.delete(key));
  _inflight.set(key, p);
  return p;
}

// --- API Implementation ---

export const api = {
  listSessions: async (): Promise<SessionInfo[]> => {
    if (USE_MOCK) return [];
    return invoke('list_sessions_v2');
  },

  getSession: async (sessionId: string): Promise<SessionInfo> => {
    if (USE_MOCK) return mockConnect({ host: 'mock', port: 22, username: 'mock', auth_type: 'password' });
    return invoke('get_session', { sessionId });
  },

  getSessionStats: async (): Promise<SessionStats> => {
    if (USE_MOCK) return { total: 0, connected: 0, connecting: 0, error: 0 };
    return invoke('get_session_stats');
  },

  resizeSession: async (sessionId: string, cols: number, rows: number): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('resize_session_v2', { sessionId, cols, rows });
  },

  reorderSessions: async (orderedIds: string[]): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('reorder_sessions', { orderedIds });
  },

  // ============ SSH Connection Pool ============
  
  /**
   * Disconnect an SSH connection (force close)
   */
  sshDisconnect: async (connectionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('ssh_disconnect', { connectionId });
  },

  /**
   * List all SSH connections in the pool
   */
  sshListConnections: async (): Promise<SshConnectionInfo[]> => {
    if (USE_MOCK) return [];
    return invoke('ssh_list_connections');
  },

  /**
   * Set connection keep-alive flag
   */
  sshSetKeepAlive: async (connectionId: string, keepAlive: boolean): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('ssh_set_keep_alive', { connectionId, keepAlive });
  },

  // ============ SSH Host Key Preflight (TOFU) ============

  /**
   * Preflight check for SSH host key (TOFU - Trust On First Use)
   * 
   * Call this before sshConnect to verify the host key status:
   * - 'verified': Host key matches known_hosts, safe to connect
   * - 'unknown': First time connecting, show confirmation dialog
   * - 'changed': Host key changed! Possible MITM, show strong warning
   * - 'error': Connection error during preflight
   */
  sshPreflight: async (request: SshPreflightRequest): Promise<SshPreflightResponse> => {
    if (USE_MOCK) {
      return { status: 'verified' };
    }
    return invoke('ssh_preflight', { request });
  },

  /**
   * Accept a host key after user confirmation
   * 
   * @param request - Contains host, port, fingerprint, and persist flag
   * @param request.persist - true = save to known_hosts, false = trust for session only
   */
  sshAcceptHostKey: async (request: AcceptHostKeyRequest): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('ssh_accept_host_key', { request });
  },

  /**
   * Clear host key cache (for testing or force re-verification)
   */
  sshClearHostKeyCache: async (): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('ssh_clear_host_key_cache');
  },

  /**
   * Get remote environment info for an SSH connection
   * 
   * Returns null if detection is not yet complete or failed.
   * Subscribe to `env:detected:{connectionId}` event for async updates.
   */
  getRemoteEnv: async (connectionId: string): Promise<RemoteEnvInfo | null> => {
    if (USE_MOCK) return null;
    return invoke('get_remote_env', { connectionId });
  },

  /**
   * Get connection pool configuration
   */
  sshGetPoolConfig: async (): Promise<ConnectionPoolConfig> => {
    if (USE_MOCK) {
      return {
        idleTimeoutSecs: 1800,
        maxConnections: 0,
        protectOnExit: true,
      };
    }
    return invoke('ssh_get_pool_config');
  },

  /**
   * Set connection pool configuration
   */
  sshSetPoolConfig: async (config: ConnectionPoolConfig): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('ssh_set_pool_config', { config });
  },

  /**
   * Get connection pool statistics
   * Returns real-time stats for monitoring panel
   */
  sshGetPoolStats: async (): Promise<ConnectionPoolStats> => {
    if (USE_MOCK) {
      return {
        totalConnections: 0,
        activeConnections: 0,
        idleConnections: 0,
        reconnectingConnections: 0,
        linkDownConnections: 0,
        totalTerminals: 0,
        totalSftpSessions: 0,
        totalForwards: 0,
        totalRefCount: 0,
        poolCapacity: 0,
        idleTimeoutSecs: 1800,
      };
    }
    return invoke('ssh_get_pool_stats');
  },

  /**
   * Create a terminal for an existing SSH connection
   */
  createTerminal: async (request: CreateTerminalRequest): Promise<CreateTerminalResponse> => {
    if (USE_MOCK) {
      return {
        sessionId: 'mock-session-id',
        wsUrl: 'ws://localhost:9999',
        port: 9999,
        wsToken: 'mock-token',
        session: {
          id: 'mock-session-id',
          name: 'Mock Terminal',
          host: 'mock.example.com',
          port: 22,
          username: 'mockuser',
          state: 'connected',
          color: '#ff0000',
          uptime_secs: 0,
          auth_type: 'password',
          order: 0,
        }
      };
    }
    return invoke('create_terminal', { request });
  },

  /**
   * Close a terminal (does not disconnect the SSH connection)
   */
  closeTerminal: async (sessionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('close_terminal', { sessionId });
  },

  /**
   * Recreate terminal PTY after connection reconnect
   * Returns new WebSocket URL and token for the existing session
   */
  recreateTerminalPty: async (sessionId: string): Promise<{
    sessionId: string;
    wsUrl: string;
    port: number;
    wsToken: string;
  }> => {
    if (USE_MOCK) {
      return {
        sessionId,
        wsUrl: 'ws://localhost:9999',
        port: 9999,
        wsToken: 'mock-token-refreshed',
      };
    }
    return invoke('recreate_terminal_pty', { sessionId });
  },

  // ============ Session Persistence ============
  restoreSessions: async (): Promise<PersistedSessionInfo[]> => {
    if (USE_MOCK) return [];
    return invoke('restore_sessions');
  },

  listPersistedSessions: async (): Promise<string[]> => {
    if (USE_MOCK) return [];
    return invoke('list_persisted_sessions');
  },

  deletePersistedSession: async (sessionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('delete_persisted_session', { sessionId });
  },

  // ============ Connection Config ============
  getConnections: async (): Promise<ConnectionInfo[]> => {
    if (USE_MOCK) return mockConnections;
    return invoke('get_connections');
  },

  getRecentConnections: async (limit?: number): Promise<ConnectionInfo[]> => {
    if (USE_MOCK) return mockConnections.slice(0, limit || 5);
    return invoke('get_recent_connections', { limit: limit || null });
  },

  getConnectionsByGroup: async (group?: string): Promise<ConnectionInfo[]> => {
    if (USE_MOCK) return mockConnections.filter(c => c.group === group);
    return invoke('get_connections_by_group', { group: group || null });
  },

  searchConnections: async (query: string): Promise<ConnectionInfo[]> => {
    if (USE_MOCK) return mockConnections.filter(c => c.name.includes(query));
    return invoke('search_connections', { query });
  },

  saveConnection: async (request: SaveConnectionRequest): Promise<ConnectionInfo> => {
    if (USE_MOCK) return mockConnections[0];
    return invoke('save_connection', { request });
  },

  deleteConnection: async (id: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('delete_connection', { id });
  },

  markConnectionUsed: async (id: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('mark_connection_used', { id });
  },

  getConnectionPassword: async (id: string): Promise<string> => {
    if (USE_MOCK) return 'mock-password';
    return invoke('get_connection_password', { id });
  },

  /**
   * Get saved connection with credentials for connecting
   * Returns full connection info including passwords from keychain
   */
  getSavedConnectionForConnect: async (id: string): Promise<{
    host: string;
    port: number;
    username: string;
    auth_type: string;
    password?: string;
    key_path?: string;
    passphrase?: string;
    name: string;
    proxy_chain: Array<{
      host: string;
      port: number;
      username: string;
      auth_type: string;
      password?: string;
      key_path?: string;
      passphrase?: string;
    }>;
  }> => {
    if (USE_MOCK) {
      return {
        host: 'mock.example.com',
        port: 22,
        username: 'mockuser',
        auth_type: 'password',
        password: 'mock-password',
        name: 'Mock Connection',
        proxy_chain: [],
      };
    }
    return invoke('get_saved_connection_for_connect', { id });
  },
  
  // ============ Groups ============
  getGroups: async (): Promise<string[]> => {
    if (USE_MOCK) return ['Production', 'Development', 'Testing'];
    return invoke('get_groups');
  },
  
  createGroup: async (name: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('create_group', { name });
  },
  
  deleteGroup: async (name: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('delete_group', { name });
  },

  // ============ SSH Config & Keys ============
  listSshConfigHosts: async (): Promise<SshHostInfo[]> => {
    if (USE_MOCK) return [];
    return invoke('list_ssh_config_hosts');
  },
  
  importSshHost: async (alias: string): Promise<ConnectionInfo> => {
    if (USE_MOCK) throw new Error("Mock import not implemented");
    return invoke('import_ssh_host', { alias });
  },

  getSshConfigPath: async (): Promise<string> => {
    if (USE_MOCK) return '~/.ssh/config';
    return invoke('get_ssh_config_path');
  },
  
  checkSshKeys: async (): Promise<SshKeyInfo[]> => {
    if (USE_MOCK) return mockSshKeys;
    // Backend returns Vec<String> of key paths, transform to SshKeyInfo[]
    const paths: string[] = await invoke('check_ssh_keys');
    return paths.map(path => {
      const name = path.split('/').pop() || path;
      let key_type = 'Unknown';
      if (name.includes('ed25519')) key_type = 'ED25519';
      else if (name.includes('ecdsa')) key_type = 'ECDSA';
      else if (name.includes('rsa')) key_type = 'RSA';
      else if (name.includes('dsa')) key_type = 'DSA';
      return {
        name,
        path,
        key_type,
        has_passphrase: false // Cannot determine without trying to load
      };
    });
  },

  /** Check if SSH Agent is available on the current platform */
  isAgentAvailable: async (): Promise<boolean> => {
    if (USE_MOCK) return false;
    return invoke('is_ssh_agent_available');
  },

  // ============ SFTP Transfer Control ============
  sftpCancelTransfer: async (transferId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('sftp_cancel_transfer', { transferId });
  },

  sftpPauseTransfer: async (transferId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('sftp_pause_transfer', { transferId });
  },

  sftpResumeTransfer: async (transferId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('sftp_resume_transfer', { transferId });
  },

  sftpTransferStats: async (): Promise<{ active: number; queued: number; completed: number }> => {
    if (USE_MOCK) return { active: 0, queued: 0, completed: 0 };
    return invoke('sftp_transfer_stats');
  },

  // SFTP Settings - Update transfer settings (concurrent limit and speed limit)
  sftpUpdateSettings: async (maxConcurrent?: number, speedLimitKbps?: number): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('sftp_update_settings', { maxConcurrent, speedLimitKbps });
  },

  // ============ Port Forwarding ============
  listPortForwards: async (sessionId: string): Promise<ForwardRule[]> => {
    if (USE_MOCK) return [];
    return invoke('list_port_forwards', { sessionId });
  },
  
  createPortForward: async (request: ForwardRequest): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true, forward: { id: 'mock-fwd-id', forward_type: 'local', bind_address: '127.0.0.1', bind_port: 8080, target_host: 'localhost', target_port: 80, status: 'active' } };
    // Backend returns ForwardResponse
    return invoke('create_port_forward', { request });
  },

  stopPortForward: async (sessionId: string, forwardId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('stop_port_forward', { sessionId, forwardId });
  },

  deletePortForward: async (sessionId: string, forwardId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('delete_port_forward', { sessionId, forwardId });
  },

  restartPortForward: async (sessionId: string, forwardId: string): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true, forward: { id: forwardId, forward_type: 'local', bind_address: '127.0.0.1', bind_port: 8080, target_host: 'localhost', target_port: 80, status: 'active' } };
    return invoke('restart_port_forward', { sessionId, forwardId });
  },

  updatePortForward: async (request: {
    session_id: string;
    forward_id: string;
    bind_address?: string;
    bind_port?: number;
    target_host?: string;
    target_port?: number;
    description?: string;
  }): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true };
    return invoke('update_port_forward', { request });
  },

  getPortForwardStats: async (sessionId: string, forwardId: string): Promise<{
    connection_count: number;
    active_connections: number;
    bytes_sent: number;
    bytes_received: number;
  } | null> => {
    if (USE_MOCK) return null;
    return invoke('get_port_forward_stats', { sessionId, forwardId });
  },

  stopAllForwards: async (sessionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('stop_all_forwards', { sessionId });
  },

  forwardJupyter: async (sessionId: string, localPort: number, remotePort: number): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true, forward: { id: 'mock-jupyter', forward_type: 'local', bind_address: '127.0.0.1', bind_port: localPort, target_host: 'localhost', target_port: remotePort, status: 'active' } };
    return invoke('forward_jupyter', { sessionId, localPort, remotePort });
  },

  forwardTensorboard: async (sessionId: string, localPort: number, remotePort: number): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true, forward: { id: 'mock-tensorboard', forward_type: 'local', bind_address: '127.0.0.1', bind_port: localPort, target_host: 'localhost', target_port: remotePort, status: 'active' } };
    return invoke('forward_tensorboard', { sessionId, localPort, remotePort });
  },

  forwardVscode: async (sessionId: string, localPort: number, remotePort: number): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true, forward: { id: 'mock-vscode', forward_type: 'local', bind_address: '127.0.0.1', bind_port: localPort, target_host: 'localhost', target_port: remotePort, status: 'active' } };
    return invoke('forward_vscode', { sessionId, localPort, remotePort });
  },

  // ============ Forward Persistence ============
  listSavedForwards: async (sessionId: string): Promise<PersistedForwardInfo[]> => {
    if (USE_MOCK) return [];
    return invoke('list_saved_forwards', { sessionId });
  },

  setForwardAutoStart: async (forwardId: string, autoStart: boolean): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('set_forward_auto_start', { forwardId, autoStart });
  },

  deleteSavedForward: async (forwardId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('delete_saved_forward', { forwardId });
  },

  // ============ Node-first Port Forwarding (Oxide-Next) ============
  nodeListForwards: async (nodeId: string): Promise<ForwardRule[]> => {
    if (USE_MOCK) return [];
    return invoke('node_list_forwards', { nodeId });
  },

  nodeCreateForward: async (request: {
    node_id: string;
    forward_type: string;
    bind_address: string;
    bind_port: number;
    target_host: string;
    target_port: number;
    description?: string;
    check_health?: boolean;
  }): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true, forward: { id: 'mock-fwd', forward_type: 'local', bind_address: '127.0.0.1', bind_port: 8080, target_host: 'localhost', target_port: 80, status: 'active' } };
    return invoke('node_create_forward', {
      nodeId: request.node_id,
      forwardType: request.forward_type,
      bindAddress: request.bind_address,
      bindPort: request.bind_port,
      targetHost: request.target_host,
      targetPort: request.target_port,
      description: request.description,
      checkHealth: request.check_health,
    });
  },

  nodeStopForward: async (nodeId: string, forwardId: string): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true };
    return invoke('node_stop_forward', { nodeId, forwardId });
  },

  nodeDeleteForward: async (nodeId: string, forwardId: string): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true };
    return invoke('node_delete_forward', { nodeId, forwardId });
  },

  nodeRestartForward: async (nodeId: string, forwardId: string): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true };
    return invoke('node_restart_forward', { nodeId, forwardId });
  },

  nodeUpdateForward: async (request: {
    node_id: string;
    forward_id: string;
    bind_address?: string;
    bind_port?: number;
    target_host?: string;
    target_port?: number;
    description?: string;
  }): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true };
    return invoke('node_update_forward', {
      nodeId: request.node_id,
      forwardId: request.forward_id,
      bindAddress: request.bind_address,
      bindPort: request.bind_port,
      targetHost: request.target_host,
      targetPort: request.target_port,
      description: request.description,
    });
  },

  nodeGetForwardStats: async (nodeId: string, forwardId: string): Promise<{
    connection_count: number;
    active_connections: number;
    bytes_sent: number;
    bytes_received: number;
  } | null> => {
    if (USE_MOCK) return null;
    return invoke('node_get_forward_stats', { nodeId, forwardId });
  },

  nodeStopAllForwards: async (nodeId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('node_stop_all_forwards', { nodeId });
  },

  nodeForwardJupyter: async (nodeId: string, localPort: number, remotePort: number): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true, forward: { id: 'mock-jupyter', forward_type: 'local', bind_address: '127.0.0.1', bind_port: localPort, target_host: 'localhost', target_port: remotePort, status: 'active' } };
    return invoke('node_forward_jupyter', { nodeId, localPort, remotePort });
  },

  nodeForwardTensorboard: async (nodeId: string, localPort: number, remotePort: number): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true, forward: { id: 'mock-tensorboard', forward_type: 'local', bind_address: '127.0.0.1', bind_port: localPort, target_host: 'localhost', target_port: remotePort, status: 'active' } };
    return invoke('node_forward_tensorboard', { nodeId, localPort, remotePort });
  },

  nodeForwardVscode: async (nodeId: string, localPort: number, remotePort: number): Promise<ForwardResponse> => {
    if (USE_MOCK) return { success: true, forward: { id: 'mock-vscode', forward_type: 'local', bind_address: '127.0.0.1', bind_port: localPort, target_host: 'localhost', target_port: remotePort, status: 'active' } };
    return invoke('node_forward_vscode', { nodeId, localPort, remotePort });
  },

  nodeListSavedForwards: async (nodeId: string): Promise<PersistedForwardInfo[]> => {
    if (USE_MOCK) return [];
    return invoke('node_list_saved_forwards', { nodeId });
  },

  // ============ Health Check ============
  getConnectionHealth: async (sessionId: string): Promise<HealthMetrics> => {
    if (USE_MOCK) return mockHealthMetrics;
    return invoke('get_connection_health', { sessionId });
  },

  getQuickHealth: async (sessionId: string): Promise<QuickHealthCheck> => {
    if (USE_MOCK) return { session_id: sessionId, status: 'Healthy', latency_ms: 10, message: 'Connected • 10ms' };
    return invoke('get_quick_health', { sessionId });
  },

  getAllHealthStatus: async (): Promise<Record<string, QuickHealthCheck>> => {
    if (USE_MOCK) return {};
    return invoke('get_all_health_status');
  },

  getHealthForDisplay: async (sessionId: string): Promise<QuickHealthCheck> => {
    if (USE_MOCK) return { session_id: sessionId, status: 'Healthy', latency_ms: 10, message: 'Connected • 10ms' };
    return invoke('get_health_for_display', { sessionId });
  },

  // ============ Resource Profiler ============
  startResourceProfiler: async (connectionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('start_resource_profiler', { connectionId });
  },

  stopResourceProfiler: async (connectionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('stop_resource_profiler', { connectionId });
  },

  getResourceMetrics: async (connectionId: string): Promise<ResourceMetrics | null> => {
    if (USE_MOCK) return null;
    return invoke('get_resource_metrics', { connectionId });
  },

  getResourceHistory: async (connectionId: string): Promise<ResourceMetrics[]> => {
    if (USE_MOCK) return [];
    return invoke('get_resource_history', { connectionId });
  },

  // ============ Network & Reconnect ============
  networkStatusChanged: async (online: boolean): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('network_status_changed', { online });
  },

  /**
   * 主动探测所有活跃 SSH 连接的健康状态。
   *
   * 对每个 Active/Idle 连接发送 SSH keepalive。
   * 死连接会被标记 link_down 并通过 connection_status_changed 事件通知前端。
   *
   * @returns 已死连接的 connection_id 列表
   */
  probeConnections: async (): Promise<string[]> => {
    if (USE_MOCK) return [];
    return invoke('probe_connections');
  },

  /**
   * 探测单个连接的健康状态（支持 LinkDown 恢复）。
   *
   * 如果连接处于 LinkDown 且探测成功，后端自动恢复为 Active 并重启心跳。
   * 用于 Grace Period 机制：在销毁旧 SSH session 前，尝试复用已有连接。
   *
   * @returns "alive" | "dead" | "not_found" | "not_applicable"
   */
  probeSingleConnection: async (connectionId: string): Promise<string> => {
    if (USE_MOCK) return 'not_found';
    return invoke('probe_single_connection', { connectionId });
  },

  cancelReconnect: async (sessionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('cancel_reconnect', { sessionId });
  },

  isReconnecting: async (sessionId: string): Promise<boolean> => {
    if (USE_MOCK) return false;
    return invoke('is_reconnecting', { sessionId });
  },

  // --- Scroll Buffer APIs ---
  
  getScrollBuffer: async (sessionId: string, startLine: number, count: number): Promise<TerminalLine[]> => {
    if (USE_MOCK) return [];
    return invoke('get_scroll_buffer', { sessionId, startLine, count });
  },

  getBufferStats: async (sessionId: string): Promise<BufferStats> => {
    if (USE_MOCK) return { current_lines: 0, total_lines: 0, max_lines: 100000, memory_usage_mb: 0 };
    return invoke('get_buffer_stats', { sessionId });
  },

  clearBuffer: async (sessionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('clear_buffer', { sessionId });
  },

  getAllBufferLines: async (sessionId: string): Promise<import('../types').BufferLinesResponse> => {
    if (USE_MOCK) return { lines: [], total_lines: 0, returned_lines: 0, truncated: false };
    return invoke('get_all_buffer_lines', { sessionId });
  },

  // --- Search APIs ---
  
  searchTerminal: async (sessionId: string, options: SearchOptions): Promise<SearchResult> => {
    if (USE_MOCK) return { matches: [], total_matches: 0, duration_ms: 0 };
    return invoke('search_terminal', { sessionId, options });
  },

  scrollToLine: async (sessionId: string, lineNumber: number, contextLines: number): Promise<TerminalLine[]> => {
    if (USE_MOCK) return [];
    return invoke('scroll_to_line', { sessionId, lineNumber, contextLines });
  },

  // ============ Session Tree (Dynamic Jump Host) ============

  /**
   * 获取扁平化的会话树（用于前端渲染）
   */
  getSessionTree: async (): Promise<import('../types').FlatNode[]> => {
    if (USE_MOCK) return [];
    return invoke('get_session_tree');
  },

  /**
   * 获取会话树摘要信息
   */
  getSessionTreeSummary: async (): Promise<import('../types').SessionTreeSummary> => {
    if (USE_MOCK) return { totalNodes: 0, rootCount: 0, connectedCount: 0, maxDepth: 0 };
    return invoke('get_session_tree_summary');
  },

  /**
   * 添加直连节点（depth=0）
   */
  addRootNode: async (request: import('../types').ConnectServerRequest): Promise<string> => {
    if (USE_MOCK) return 'mock-node-id';
    return invoke('add_root_node', { request });
  },

  /**
   * 从已连接节点钻入新服务器（模式3: 动态钻入）
   */
  treeDrillDown: async (request: import('../types').DrillDownRequest): Promise<string> => {
    if (USE_MOCK) return 'mock-child-node-id';
    return invoke('tree_drill_down', { request });
  },

  // ===== Auto-Route (Auto-generated from Saved Connections) APIs =====

  /**
   * Get topology nodes (auto-generated from saved connections)
   */
  getTopologyNodes: async (): Promise<import('../types').TopologyNodeInfo[]> => {
    if (USE_MOCK) return [];
    return invoke('get_topology_nodes');
  },

  /**
   * Get topology edges
   */
  getTopologyEdges: async (): Promise<import('../types').TopologyEdge[]> => {
    if (USE_MOCK) return [];
    return invoke('get_topology_edges');
  },

  /**
   * Get custom edges overlay config
   */
  getTopologyEdgesOverlay: async (): Promise<import('../types').TopologyEdgesConfig> => {
    if (USE_MOCK) return { customEdges: [], excludedEdges: [] };
    return invoke('get_topology_edges_overlay');
  },

  /**
   * Add a custom edge to topology
   */
  addTopologyEdge: async (from: string, to: string, cost?: number): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('add_topology_edge', { from, to, cost });
  },

  /**
   * Remove a custom edge from topology
   */
  removeTopologyEdge: async (from: string, to: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('remove_topology_edge', { from, to });
  },

  /**
   * Exclude an auto-generated edge
   */
  excludeTopologyEdge: async (from: string, to: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('exclude_topology_edge', { from, to });
  },

  /**
   * Expand auto-route node chain (Mode 2: Static Auto-Route)
   */
  expandAutoRoute: async (request: import('../types').ExpandAutoRouteRequest): Promise<import('../types').ExpandAutoRouteResponse> => {
    if (USE_MOCK) return {
      targetNodeId: 'mock-target-node-id',
      route: [],
      totalCost: 0,
      allNodeIds: ['mock-target-node-id'],
    };
    return invoke('expand_auto_route', { request });
  },

  /**
   * 更新节点状态
   */
  updateTreeNodeState: async (nodeId: string, newState: string, error?: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('update_tree_node_state', { nodeId, newState, error });
  },

  /**
   * 关联 SSH 连接 ID 到节点
   */
  setTreeNodeConnection: async (nodeId: string, connectionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('set_tree_node_connection', { nodeId, connectionId });
  },

  /**
   * 关联终端会话 ID 到节点
   */
  setTreeNodeTerminal: async (nodeId: string, sessionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('set_tree_node_terminal', { nodeId, sessionId });
  },

  /**
   * 关联 SFTP 会话 ID 到节点
   */
  setTreeNodeSftp: async (nodeId: string, sessionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('set_tree_node_sftp', { nodeId, sessionId });
  },

  /**
   * 移除节点（递归移除所有子节点）
   */
  removeTreeNode: async (nodeId: string): Promise<string[]> => {
    if (USE_MOCK) return [nodeId];
    return invoke('remove_tree_node', { nodeId });
  },

  /**
   * 获取节点详情
   */
  getTreeNode: async (nodeId: string): Promise<import('../types').FlatNode | null> => {
    if (USE_MOCK) return null;
    return invoke('get_tree_node', { nodeId });
  },

  /**
   * 获取节点到根的完整路径
   */
  getTreeNodePath: async (nodeId: string): Promise<import('../types').FlatNode[]> => {
    if (USE_MOCK) return [];
    return invoke('get_tree_node_path', { nodeId });
  },

  /**
   * 清空会话树
   */
  clearSessionTree: async (): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('clear_session_tree');
  },

  /**
   * 连接树节点（建立 SSH 连接）
   */
  connectTreeNode: async (request: { nodeId: string; cols?: number; rows?: number }): Promise<{ nodeId: string; sshConnectionId: string; parentConnectionId?: string }> => {
    if (USE_MOCK) {
      return { nodeId: request.nodeId, sshConnectionId: crypto.randomUUID() };
    }
    return invoke('connect_tree_node', { request });
  },

  /**
   * 断开树节点（断开 SSH 连接）
   */
  disconnectTreeNode: async (nodeId: string): Promise<string[]> => {
    if (USE_MOCK) return [nodeId];
    return invoke('disconnect_tree_node', { nodeId });
  },

  /**
   * 展开手工预设链为树节点（不执行连接）
   * 
   * Phase 2.2: 后端降级 - 只展开节点，不建立连接
   * 前端负责通过 connectNodeWithAncestors 进行线性连接
   */
  expandManualPreset: async (
    request: { savedConnectionId: string; hops: Array<{ host: string; port: number; username: string; authType?: string; password?: string; keyPath?: string; passphrase?: string }>; target: { host: string; port: number; username: string; authType?: string; password?: string; keyPath?: string; passphrase?: string } }
  ): Promise<{ targetNodeId: string; pathNodeIds: string[]; chainDepth: number }> => {
    if (USE_MOCK) {
      const mockId = crypto.randomUUID();
      return {
        targetNodeId: mockId,
        pathNodeIds: [mockId],
        chainDepth: request.hops.length + 1,
      };
    }
    return invoke('expand_manual_preset', { request });
  },

  /**
   * 销毁节点关联的所有会话资源（焦土式清理）
   * 
   * Phase 2.1: 前端调用此接口进行彻底清理
   * - 关闭终端和 WebSocket bridges
   * - 关闭 SFTP 会话
   * - 条件性断开 SSH 连接（无剩余引用时）
   * 
   * 幂等性：重复调用不会产生错误
   */
  destroyNodeSessions: async (
    nodeId: string
  ): Promise<{ destroyedTerminals: string[]; sshDisconnected: boolean; sftpClosed: boolean }> => {
    if (USE_MOCK) {
      return {
        destroyedTerminals: [],
        sshDisconnected: false,
        sftpClosed: false,
      };
    }
    return invoke('destroy_node_sessions', { nodeId });
  },

  // ============ AI API Key Commands (Legacy compat → routes to builtin-openai keychain) ============

  /**
   * @deprecated Use setAiProviderApiKey instead. Routes to builtin-openai in OS keychain.
   */
  setAiApiKey: async (apiKey: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('set_ai_api_key', { apiKey });
  },

  /**
   * @deprecated Use getAiProviderApiKey instead. Routes to builtin-openai in OS keychain.
   */
  getAiApiKey: async (): Promise<string | null> => {
    if (USE_MOCK) return null;
    return invoke('get_ai_api_key');
  },

  /**
   * @deprecated Use hasAiProviderApiKey instead. Routes to builtin-openai in OS keychain.
   */
  hasAiApiKey: async (): Promise<boolean> => {
    if (USE_MOCK) return false;
    return invoke('has_ai_api_key');
  },

  /**
   * @deprecated Use deleteAiProviderApiKey instead. Routes to builtin-openai in OS keychain.
   */
  deleteAiApiKey: async (): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('delete_ai_api_key');
  },

  // ============ AI Provider API Key Commands (OS Keychain) ============

  /**
   * Set API key for a specific AI provider — stored in OS keychain
   */
  setAiProviderApiKey: async (providerId: string, apiKey: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('set_ai_provider_api_key', { providerId, apiKey });
  },

  /**
   * Get API key for a specific AI provider — from OS keychain.
   * Deduped: concurrent calls for the same provider share one IPC round-trip.
   */
  getAiProviderApiKey: async (providerId: string): Promise<string | null> => {
    if (USE_MOCK) return null;
    return dedup(`get-ai-key:${providerId}`, () =>
      invoke('get_ai_provider_api_key', { providerId }),
    );
  },

  /**
   * Check if API key exists for a specific AI provider — checks keychain + legacy vault.
   * Deduped: concurrent calls for the same provider share one IPC round-trip.
   */
  hasAiProviderApiKey: async (providerId: string): Promise<boolean> => {
    if (USE_MOCK) return false;
    return dedup(`has-ai-key:${providerId}`, () =>
      invoke('has_ai_provider_api_key', { providerId }),
    );
  },

  /**
   * Delete API key for a specific AI provider — removes from keychain + vault
   */
  deleteAiProviderApiKey: async (providerId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('delete_ai_provider_api_key', { providerId });
  },

  /**
   * List all provider IDs that have stored API keys
   */
  listAiProviderKeys: async (): Promise<string[]> => {
    if (USE_MOCK) return [];
    return invoke('list_ai_provider_keys');
  },

  // ============ Local Terminal (PTY) ============

  /**
   * List available shells on the system
   */
  localListShells: async (): Promise<import('../types').ShellInfo[]> => {
    if (USE_MOCK) {
      return [
        { id: 'zsh', label: 'Zsh', path: '/bin/zsh', args: ['--login'] },
        { id: 'bash', label: 'Bash', path: '/bin/bash', args: ['--login'] },
      ];
    }
    return invoke('local_list_shells');
  },

  /**
   * Get the default shell for the current user
   */
  localGetDefaultShell: async (): Promise<import('../types').ShellInfo> => {
    if (USE_MOCK) {
      return { id: 'zsh', label: 'Zsh', path: '/bin/zsh', args: ['--login'] };
    }
    return invoke('local_get_default_shell');
  },

  /**
   * Create a new local terminal session
   */
  localCreateTerminal: async (request: import('../types').CreateLocalTerminalRequest): Promise<import('../types').CreateLocalTerminalResponse> => {
    if (USE_MOCK) {
      const sessionId = crypto.randomUUID();
      return {
        sessionId,
        info: {
          id: sessionId,
          shell: { id: 'zsh', label: 'Zsh', path: '/bin/zsh', args: ['--login'] },
          cols: request.cols || 80,
          rows: request.rows || 24,
          running: true,
        },
      };
    }
    return invoke('local_create_terminal', { request });
  },

  /**
   * Close a local terminal session
   */
  localCloseTerminal: async (sessionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('local_close_terminal', { sessionId });
  },

  /**
   * Resize a local terminal
   */
  localResizeTerminal: async (sessionId: string, cols: number, rows: number): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('local_resize_terminal', { sessionId, cols, rows });
  },

  /**
   * Write data to a local terminal
   */
  localWriteTerminal: async (sessionId: string, data: number[]): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('local_write_terminal', { sessionId, data });
  },

  /**
   * List all active local terminal sessions
   */
  localListTerminals: async (): Promise<import('../types').LocalTerminalInfo[]> => {
    if (USE_MOCK) return [];
    return invoke('local_list_terminals');
  },

  /**
   * Get info about a specific local terminal session
   */
  localGetTerminalInfo: async (sessionId: string): Promise<import('../types').LocalTerminalInfo | null> => {
    if (USE_MOCK) return null;
    return invoke('local_get_terminal_info', { sessionId });
  },

  /**
   * Clean up dead local terminal sessions
   */
  localCleanupDeadSessions: async (): Promise<string[]> => {
    if (USE_MOCK) return [];
    return invoke('local_cleanup_dead_sessions');
  },

  /**
   * Get available local drives / mounted volumes.
   * Returns structured DriveInfo with path, name, type, and capacity.
   */
  localGetDrives: async (): Promise<import('../components/fileManager/types').DriveInfo[]> => {
    if (USE_MOCK) return [{ path: '/', name: 'System', driveType: 'system', totalSpace: 0, availableSpace: 0, isReadOnly: false }];
    return invoke('local_get_drives');
  },

  // ═══════════════════════════════════════════════════════════════════════════
  // IDE Mode Commands
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Open a project directory and get basic info
   */
  ideOpenProject: async (sessionId: string, path: string): Promise<{
    rootPath: string;
    name: string;
    isGitRepo: boolean;
    gitBranch: string | null;
    fileCount: number;
  }> => {
    if (USE_MOCK) return { rootPath: path, name: 'mock', isGitRepo: false, gitBranch: null, fileCount: 0 };
    return invoke('ide_open_project', { sessionId, path });
  },

  /**
   * Check if a file is editable
   */
  ideCheckFile: async (sessionId: string, path: string): Promise<
    | { type: 'editable'; size: number; mtime: number }
    | { type: 'too_large'; size: number; limit: number }
    | { type: 'binary' }
    | { type: 'not_editable'; reason: string }
  > => {
    if (USE_MOCK) return { type: 'editable', size: 100, mtime: Date.now() / 1000 };
    return invoke('ide_check_file', { sessionId, path });
  },

  /**
   * Batch stat multiple paths
   */
  ideBatchStat: async (sessionId: string, paths: string[]): Promise<Array<{
    size: number;
    mtime: number;
    isDir: boolean;
  } | null>> => {
    if (USE_MOCK) return paths.map(() => null);
    return invoke('ide_batch_stat', { sessionId, paths });
  },

  /**
   * Execute a command on the remote server via SSH exec channel
   */
  ideExecCommand: async (
    connectionId: string,
    command: string,
    cwd?: string,
    timeoutSecs?: number
  ): Promise<{
    stdout: string;
    stderr: string;
    exitCode: number | null;
  }> => {
    if (USE_MOCK) return { stdout: '', stderr: '', exitCode: 0 };
    return invoke('ide_exec_command', { connectionId, command, cwd, timeoutSecs });
  },

  // ═══════════════════════════════════════════════════════════════════
  // Plugin System
  // ═══════════════════════════════════════════════════════════════════

  /** List all installed plugins (scans plugins directory) */
  pluginList: async (): Promise<PluginManifest[]> => {
    if (USE_MOCK) return [];
    return invoke('list_plugins');
  },

  /** Read a file from a plugin's directory (path-traversal protected) */
  pluginReadFile: async (pluginId: string, relativePath: string): Promise<number[]> => {
    if (USE_MOCK) return [];
    return invoke('read_plugin_file', { pluginId, relativePath });
  },

  /** Save plugin configuration (enabled/disabled state) */
  pluginSaveConfig: async (config: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('save_plugin_config', { config });
  },

  /** Load plugin configuration */
  pluginLoadConfig: async (): Promise<string> => {
    if (USE_MOCK) return '{}';
    return invoke('load_plugin_config');
  },

  /** Start the plugin file server (for multi-file packages). Returns the port. */
  pluginStartServer: async (): Promise<number> => {
    if (USE_MOCK) return 0;
    return invoke('start_plugin_server');
  },

  /** Get the plugin file server port, if running. */
  pluginGetServerPort: async (): Promise<number | null> => {
    if (USE_MOCK) return null;
    return invoke('get_plugin_server_port');
  },

  /** Stop the plugin file server gracefully. Returns true if it was running. */
  pluginStopServer: async (): Promise<boolean> => {
    if (USE_MOCK) return false;
    return invoke('stop_plugin_server');
  },

  // ============ Plugin Registry (Remote Installation) ============

  /** Fetch the plugin registry index from a remote URL */
  pluginFetchRegistry: async (url: string): Promise<import('../types/plugin').RegistryIndex> => {
    if (USE_MOCK) return { version: 1, plugins: [] };
    return invoke('fetch_plugin_registry', { url });
  },

  /** Download, verify, and install a plugin from a remote URL */
  pluginInstall: async (
    downloadUrl: string,
    expectedId: string,
    checksum?: string,
  ): Promise<PluginManifest> => {
    if (USE_MOCK) throw new Error('Mock mode does not support plugin installation');
    return invoke('install_plugin', { downloadUrl, expectedId, checksum });
  },

  /** Uninstall a plugin by removing its directory */
  pluginUninstall: async (pluginId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('uninstall_plugin', { pluginId });
  },

  /** Check for available plugin updates */
  pluginCheckUpdates: async (
    registryUrl: string,
    installed: Array<{ id: string; version: string }>,
  ): Promise<import('../types/plugin').RegistryEntry[]> => {
    if (USE_MOCK) return [];
    return invoke('check_plugin_updates', { registryUrl, installed });
  },
};

// ═══════════════════════════════════════════════════════════════════════════
// Oxide-Next Node-First API (Phase 3)
//
// 所有函数接受 nodeId 而非 sessionId，后端 NodeRouter 完成 ID 解析。
// 这些是独立的顶级导出，不属于旧 `api` 对象。
// ═══════════════════════════════════════════════════════════════════════════

/** 获取节点状态快照（含 generation，用于初始对齐） */
export const nodeGetState = (nodeId: string): Promise<NodeStateSnapshot> =>
  invoke('node_get_state', { nodeId });

/** 初始化节点 SFTP（首次调用时创建 SFTP 通道） */
export const nodeSftpInit = (nodeId: string): Promise<string> =>
  invoke('node_sftp_init', { nodeId });

/** 列出远程目录 */
export const nodeSftpListDir = (nodeId: string, path: string): Promise<FileInfo[]> =>
  invoke('node_sftp_list_dir', { nodeId, path });

/** 获取远程文件/目录信息 */
export const nodeSftpStat = (nodeId: string, path: string): Promise<FileInfo> =>
  invoke('node_sftp_stat', { nodeId, path });

/** 预览远程文件内容 */
export const nodeSftpPreview = (nodeId: string, path: string, maxSize?: number): Promise<PreviewContent> =>
  invoke('node_sftp_preview', { nodeId, path, maxSize });

/** 清理 SFTP 预览产生的临时文件。传 path 删单个，不传删整个目录 */
export const cleanupSftpPreviewTemp = (path?: string): Promise<void> =>
  invoke('cleanup_sftp_preview_temp', { path: path ?? null });

/** 写入远程文件内容 */
export const nodeSftpWrite = (nodeId: string, path: string, content: string, encoding?: string): Promise<{ mtime: number | null; size: number | null; encodingUsed: string }> =>
  invoke('node_sftp_write', { nodeId, path, content, encoding });

/** 下载远程文件到本地 */
export const nodeSftpDownload = (nodeId: string, remotePath: string, localPath: string, transferId?: string): Promise<void> =>
  invoke('node_sftp_download', { nodeId, remotePath, localPath, transferId });

/** 上传本地文件到远程 */
export const nodeSftpUpload = (nodeId: string, localPath: string, remotePath: string, transferId?: string): Promise<void> =>
  invoke('node_sftp_upload', { nodeId, localPath, remotePath, transferId });

/** 删除远程文件 */
export const nodeSftpDelete = (nodeId: string, path: string): Promise<void> =>
  invoke('node_sftp_delete', { nodeId, path });

/** 递归删除目录 */
export const nodeSftpDeleteRecursive = (nodeId: string, path: string): Promise<number> =>
  invoke('node_sftp_delete_recursive', { nodeId, path });

/** 创建远程目录 */
export const nodeSftpMkdir = (nodeId: string, path: string): Promise<void> =>
  invoke('node_sftp_mkdir', { nodeId, path });

/** 重命名远程文件或目录 */
export const nodeSftpRename = (nodeId: string, oldPath: string, newPath: string): Promise<void> =>
  invoke('node_sftp_rename', { nodeId, oldPath, newPath });

/** 递归下载目录 */
export const nodeSftpDownloadDir = (nodeId: string, remotePath: string, localPath: string, transferId?: string): Promise<number> =>
  invoke('node_sftp_download_dir', { nodeId, remotePath, localPath, transferId });

/** 递归上传目录 */
export const nodeSftpUploadDir = (nodeId: string, localPath: string, remotePath: string, transferId?: string): Promise<number> =>
  invoke('node_sftp_upload_dir', { nodeId, localPath, remotePath, transferId });

/** 探测远端是否支持 tar 命令（结果应缓存） */
export const nodeSftpTarProbe = (nodeId: string): Promise<boolean> =>
  invoke('node_sftp_tar_probe', { nodeId });

/** 探测远端 tar 支持的最佳压缩方式（返回 "zstd" | "gzip" | "none"，结果应缓存） */
export const nodeSftpTarCompressionProbe = (nodeId: string): Promise<'zstd' | 'gzip' | 'none'> =>
  invoke('node_sftp_tar_compression_probe', { nodeId });

/** Tar 流式上传目录（远端需支持 tar） */
export const nodeSftpTarUpload = (nodeId: string, localPath: string, remotePath: string, transferId?: string, compression?: 'zstd' | 'gzip' | 'none'): Promise<number> =>
  invoke('node_sftp_tar_upload', { nodeId, localPath, remotePath, transferId, compression });

/** Tar 流式下载目录（远端需支持 tar） */
export const nodeSftpTarDownload = (nodeId: string, remotePath: string, localPath: string, transferId?: string, compression?: 'zstd' | 'gzip' | 'none'): Promise<number> =>
  invoke('node_sftp_tar_download', { nodeId, remotePath, localPath, transferId, compression });

/** 十六进制预览 */
export const nodeSftpPreviewHex = (nodeId: string, path: string, offset: number): Promise<PreviewContent> =>
  invoke('node_sftp_preview_hex', { nodeId, path, offset });

/** 列出未完成的传输 */
export const nodeSftpListIncompleteTransfers = (nodeId: string): Promise<IncompleteTransferInfo[]> =>
  invoke('node_sftp_list_incomplete_transfers', { nodeId });

/** 恢复传输（带重试） */
export const nodeSftpResumeTransfer = (nodeId: string, transferId: string): Promise<void> =>
  invoke('node_sftp_resume_transfer', { nodeId, transferId });

/** IDE: 打开项目 */
export const nodeIdeOpenProject = (nodeId: string, path: string): Promise<{
  rootPath: string; name: string; isGitRepo: boolean; gitBranch: string | null; fileCount: number;
}> =>
  invoke('node_ide_open_project', { nodeId, path });

/** IDE: 执行远程命令 */
export const nodeIdeExecCommand = (nodeId: string, command: string, cwd?: string, timeoutSecs?: number): Promise<{
  stdout: string; stderr: string; exitCode: number | null;
}> =>
  invoke('node_ide_exec_command', { nodeId, command, cwd, timeoutSecs });

/** IDE: 检查文件是否可编辑 */
export const nodeIdeCheckFile = (nodeId: string, path: string): Promise<
  | { type: 'editable'; size: number; mtime: number }
  | { type: 'too_large'; size: number; limit: number }
  | { type: 'binary' }
  | { type: 'not_editable'; reason: string }
> =>
  invoke('node_ide_check_file', { nodeId, path });

/** IDE: 批量 stat 多个路径 */
export const nodeIdeBatchStat = (nodeId: string, paths: string[]): Promise<
  Array<{ size: number; mtime: number; isDir: boolean } | null>
> =>
  invoke('node_ide_batch_stat', { nodeId, paths });

/** 获取节点终端 WebSocket 端点 */
export const nodeTerminalUrl = (nodeId: string): Promise<{ wsPort: number; wsToken: string; sessionId: string }> =>
  invoke('node_terminal_url', { nodeId });


// --- Mock Data Helpers ---

const mockConnect = async (req: ConnectRequest): Promise<SessionInfo> => {
  await new Promise(r => setTimeout(r, 500));
  return {
    id: crypto.randomUUID(),
    name: req.name || req.host,
    host: req.host,
    port: req.port,
    username: req.username,
    state: 'connected',
    color: '#3b82f6',
    uptime_secs: 0,
    order: 0,
    auth_type: req.auth_type,
    key_path: req.key_path,
  };
};

const mockConnections: ConnectionInfo[] = [
  { id: '1', name: 'Production DB', group: 'Production', host: '10.0.0.1', port: 22, username: 'admin', auth_type: 'key', key_path: '~/.ssh/id_rsa', created_at: '2023-09-01', last_used_at: '2023-10-01', color: null, tags: [] },
  { id: '2', name: 'Dev Server', group: 'Development', host: 'localhost', port: 2222, username: 'user', auth_type: 'password', key_path: null, created_at: '2023-09-15', last_used_at: '2023-10-02', color: null, tags: [] },
];

const mockSshKeys: SshKeyInfo[] = [
  { name: 'id_rsa', path: '/Users/mock/.ssh/id_rsa', key_type: 'RSA', has_passphrase: true },
  { name: 'id_ed25519', path: '/Users/mock/.ssh/id_ed25519', key_type: 'ED25519', has_passphrase: false },
];

const mockHealthMetrics: HealthMetrics = {
  session_id: 'mock',
  uptime_secs: 120,
  ping_sent: 10,
  ping_received: 10,
  avg_latency_ms: 15,
  last_latency_ms: 12,
  status: 'Healthy'
};
