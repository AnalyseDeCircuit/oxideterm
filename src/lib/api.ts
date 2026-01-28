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
  ListFilter,
  SessionStats,
  QuickHealthCheck,
  IncompleteTransferInfo,
  // New connection pool types
  SshConnectionInfo,
  SshConnectRequest,
  SshConnectResponse,
  CreateTerminalRequest,
  CreateTerminalResponse,
  ConnectionPoolConfig,
  ConnectionPoolStats,
} from '../types';

// Toggle this for development without a backend
const USE_MOCK = false;

// --- API Implementation ---

export const api = {
  /**
   * @deprecated Use sshConnect() + createTerminal() instead.
   * This legacy API creates a connection AND terminal in one call.
   * The new API separates these concerns for better resource management.
   */
  connect: async (request: ConnectRequest): Promise<SessionInfo> => {
    if (USE_MOCK) return mockConnect(request);
    // Backend returns ConnectResponseV2, extract session info and add ws_token
    // Convert proxy_chain if present
    const proxy_chain = request.proxy_chain;
    const response = await invoke<{ session: SessionInfo; ws_token?: string }>('connect_v2', { request, proxy_chain });
    const session = response.session || response;
    // Add ws_token from response if available
    if (response.ws_token) {
      session.ws_token = response.ws_token;
    }
    return session;
  },

  /**
   * @deprecated Use closeTerminal() instead.
   * This legacy API closes both terminal AND connection.
   * The new API only closes the terminal, leaving the connection for reuse.
   */
  disconnect: async (sessionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('disconnect_v2', { sessionId });
  },

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

  // ============ SSH Connection Pool (New Architecture) ============
  
  /**
   * Establish a new SSH connection (without creating a terminal)
   * Returns connection ID for subsequent operations
   */
  sshConnect: async (request: SshConnectRequest): Promise<SshConnectResponse> => {
    if (USE_MOCK) {
      return {
        connectionId: 'mock-conn-id',
        reused: false,
        connection: {
          id: 'mock-conn-id',
          host: request.host,
          port: request.port,
          username: request.username,
          state: 'active',
          refCount: 0,
          keepAlive: false,
          createdAt: new Date().toISOString(),
          lastActive: new Date().toISOString(),
          terminalIds: [],
          forwardIds: [],
        }
      };
    }
    return invoke('ssh_connect', { request });
  },

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

  // ============ SFTP ============
  sftpInit: async (sessionId: string): Promise<string> => {
    if (USE_MOCK) return '/home/mock';
    return invoke('sftp_init', { sessionId });
  },

  sftpIsInitialized: async (sessionId: string): Promise<boolean> => {
    if (USE_MOCK) return true;
    return invoke('sftp_is_initialized', { sessionId });
  },

  sftpListDir: async (sessionId: string, path: string, filter?: ListFilter): Promise<FileInfo[]> => {
    if (USE_MOCK) return mockFiles;
    return invoke('sftp_list_dir', { sessionId, path, filter: filter || null });
  },

  sftpStat: async (sessionId: string, path: string): Promise<FileInfo> => {
    if (USE_MOCK) return mockFiles[0];
    return invoke('sftp_stat', { sessionId, path });
  },

  sftpPreview: async (sessionId: string, path: string): Promise<PreviewContent> => {
    if (USE_MOCK) return { Text: { data: 'Mock preview', mime_type: 'text/plain', language: null, encoding: 'UTF-8' } };
    return invoke('sftp_preview', { sessionId, path });
  },

  sftpPreviewHex: async (sessionId: string, path: string, offset: number): Promise<PreviewContent> => {
    if (USE_MOCK) return { Hex: { data: '00000000  00 00 00 00 |....|', total_size: 16, offset: 0, chunk_size: 16, has_more: false } };
    return invoke('sftp_preview_hex', { sessionId, path, offset });
  },

  /**
   * Write content to a remote file (IDE Mode)
   * @param encoding Optional target encoding (defaults to "UTF-8")
   * @returns WriteResult containing the new mtime for sync confirmation and file size
   */
  sftpWriteContent: async (
    sessionId: string,
    path: string,
    content: string,
    encoding?: string
  ): Promise<{ mtime: number | null; size: number | null; encoding_used: string }> => {
    if (USE_MOCK) return { mtime: Date.now() / 1000, size: content.length, encoding_used: encoding || 'UTF-8' };
    return invoke('sftp_write_content', { sessionId, path, content, encoding });
  },

  sftpDownload: async (sessionId: string, remotePath: string, localPath: string, transferId?: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('sftp_download', { sessionId, remotePath, localPath, transferId });
  },

  sftpUpload: async (sessionId: string, localPath: string, remotePath: string, transferId?: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('sftp_upload', { sessionId, localPath, remotePath, transferId });
  },

  sftpDelete: async (sessionId: string, path: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('sftp_delete', { sessionId, path });
  },

  sftpDeleteRecursive: async (sessionId: string, path: string): Promise<number> => {
    if (USE_MOCK) return 1;
    return invoke('sftp_delete_recursive', { sessionId, path });
  },

  sftpDownloadDir: async (sessionId: string, remotePath: string, localPath: string): Promise<number> => {
    if (USE_MOCK) return 0;
    return invoke('sftp_download_dir', { sessionId, remotePath, localPath });
  },

  sftpUploadDir: async (sessionId: string, localPath: string, remotePath: string): Promise<number> => {
    if (USE_MOCK) return 0;
    return invoke('sftp_upload_dir', { sessionId, localPath, remotePath });
  },

  sftpMkdir: async (sessionId: string, path: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('sftp_mkdir', { sessionId, path });
  },

  sftpRename: async (sessionId: string, oldPath: string, newPath: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('sftp_rename', { sessionId, oldPath, newPath });
  },

  sftpPwd: async (sessionId: string): Promise<string> => {
    if (USE_MOCK) return '/home/mock';
    return invoke('sftp_pwd', { sessionId });
  },

  sftpCd: async (sessionId: string, path: string): Promise<string> => {
    if (USE_MOCK) return path;
    return invoke('sftp_cd', { sessionId, path });
  },

  sftpClose: async (sessionId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('sftp_close', { sessionId });
  },

  // Transfer Control
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

  // SFTP Resume Transfer - List incomplete transfers
  sftpListIncompleteTransfers: async (sessionId: string): Promise<IncompleteTransferInfo[]> => {
    if (USE_MOCK) return [];
    return invoke('sftp_list_incomplete_transfers', { sessionId });
  },

  // SFTP Resume Transfer - Resume a specific transfer with retry support
  sftpResumeTransferWithRetry: async (sessionId: string, transferId: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('sftp_resume_transfer_with_retry', { sessionId, transferId });
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

  // ============ Network & Reconnect ============
  networkStatusChanged: async (online: boolean): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('network_status_changed', { online });
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

  getAllBufferLines: async (sessionId: string): Promise<TerminalLine[]> => {
    if (USE_MOCK) return [];
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

  /**
   * 展开静态手工预设链（模式1）
   */
  expandManualPreset: async (request: import('../types').ConnectPresetChainRequest): Promise<string> => {
    if (USE_MOCK) return 'mock-target-node-id';
    return invoke('expand_manual_preset', { request });
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
   * 连接手工预设的跳板链（模式1: 静态全手工）
   */
  connectManualPreset: async (
    request: { savedConnectionId: string; hops: Array<{ host: string; port: number; username: string; authType?: string; password?: string; keyPath?: string; passphrase?: string }>; target: { host: string; port: number; username: string; authType?: string; password?: string; keyPath?: string; passphrase?: string } },
    cols?: number,
    rows?: number
  ): Promise<{ targetNodeId: string; targetSshConnectionId: string; connectedNodeIds: string[]; chainDepth: number }> => {
    if (USE_MOCK) {
      return {
        targetNodeId: crypto.randomUUID(),
        targetSshConnectionId: crypto.randomUUID(),
        connectedNodeIds: [crypto.randomUUID()],
        chainDepth: request.hops.length + 1,
      };
    }
    return invoke('connect_manual_preset', { request, cols, rows });
  },

  // ============ AI API Key Commands ============

  /**
   * Set AI API key in local encrypted vault
   * Pass empty string to delete the key
   */
  setAiApiKey: async (apiKey: string): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('set_ai_api_key', { apiKey });
  },

  /**
   * Get AI API key from local encrypted vault
   * Automatically migrates from legacy keychain if needed
   * Returns null if not set
   */
  getAiApiKey: async (): Promise<string | null> => {
    if (USE_MOCK) return null;
    return invoke('get_ai_api_key');
  },

  /**
   * Check if AI API key exists in vault or keychain
   */
  hasAiApiKey: async (): Promise<boolean> => {
    if (USE_MOCK) return false;
    return invoke('has_ai_api_key');
  },

  /**
   * Delete AI API key from all storage locations (vault and keychain)
   */
  deleteAiApiKey: async (): Promise<void> => {
    if (USE_MOCK) return;
    return invoke('delete_ai_api_key');
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
   * Get available local drives (Windows: C:\, D:\, etc. Unix: /)
   */
  localGetDrives: async (): Promise<string[]> => {
    if (USE_MOCK) return ['/'];
    return invoke('local_get_drives');
  },
};


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

const mockFiles: FileInfo[] = [
    { name: 'Documents', path: '/home/user/Documents', file_type: 'Directory', size: 0, modified: Date.now(), permissions: 'drwxr-xr-x' },
    { name: 'Downloads', path: '/home/user/Downloads', file_type: 'Directory', size: 0, modified: Date.now(), permissions: 'drwxr-xr-x' },
    { name: 'project.rs', path: '/home/user/project.rs', file_type: 'File', size: 1024, modified: Date.now(), permissions: '-rw-r--r--' },
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
