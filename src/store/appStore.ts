import { create } from 'zustand';
import { api } from '../lib/api';
import { useToastStore } from '../hooks/useToast';
import { topologyResolver } from '../lib/topologyResolver';
import { useSettingsStore, type SidebarSection } from './settingsStore';
import i18n from '../i18n';
import { 
  SessionInfo, 
  Tab, 
  ConnectRequest, 
  TabType,
  SessionState,
  ConnectionInfo,
  SshConnectionInfo,
  SshConnectionState,
  SshConnectRequest,
  ConnectPresetChainRequest,
  PaneNode,
  PaneLeaf,
  SplitDirection,
  PaneTerminalType,
  MAX_PANES_PER_TAB,
} from '../types';

interface ModalsState {
  newConnection: boolean;
  settings: boolean;
  editConnection: boolean;
  connectionManager: boolean; // 新增：连接管理面板
  autoRoute: boolean; // 自动路由选择器
}

// Re-export SidebarSection from settingsStore for backwards compatibility
export type { SidebarSection };

interface AppStore {
  // State
  sessions: Map<string, SessionInfo>;
  connections: Map<string, SshConnectionInfo>; // 新增：连接池状态
  tabs: Tab[];
  activeTabId: string | null;
  // sidebarCollapsed 和 sidebarActiveSection 已迁移至 settingsStore
  // 使用 getter 保持向后兼容
  readonly sidebarCollapsed: boolean;
  readonly sidebarActiveSection: SidebarSection;
  modals: ModalsState;
  savedConnections: ConnectionInfo[];
  groups: string[];
  selectedGroup: string | null;
  editingConnection: ConnectionInfo | null;
  networkOnline: boolean;
  reconnectPendingSessionId: string | null; // Session awaiting password for reconnect

  // Actions - Sessions (legacy, still working)
  connect: (request: ConnectRequest) => Promise<string>;
  disconnect: (sessionId: string) => Promise<void>;
  reconnect: (sessionId: string) => Promise<void>;
  reconnectWithPassword: (sessionId: string, password: string) => Promise<void>;
  cancelReconnectDialog: () => void;
  cancelReconnect: (sessionId: string) => Promise<void>;
  updateSessionState: (sessionId: string, state: SessionState, error?: string) => void;
  
  // Actions - Connection Pool (新 API)
  connectSsh: (request: SshConnectRequest) => Promise<string>;
  disconnectSsh: (connectionId: string) => Promise<void>;
  createTerminalSession: (connectionId: string, cols?: number, rows?: number) => Promise<SessionInfo>;
  closeTerminalSession: (sessionId: string) => Promise<void>;
  /**
   * Force-remove a terminal session locally (no backend call).
   * Used when backend no longer recognizes the session.
   */
  purgeTerminalSession: (sessionId: string) => void;
  refreshConnections: () => Promise<void>;
  setConnectionKeepAlive: (connectionId: string, keepAlive: boolean) => Promise<void>;
  
  // Actions - Network
  setNetworkOnline: (online: boolean) => void;
  
  // Actions - Tabs
  createTab: (type: TabType, sessionId?: string) => void;
  /**
   * 关闭标签页并执行完整的清理
   * 
   * 清理步骤：
   * 1. 从 UI 移除 Tab（乐观更新）
   * 2. 从 sessions Map 移除 session
   * 3. 通知 sessionTreeStore 清理映射
   * 4. 调用后端 closeTerminal
   * 5. 检查并可能断开 SSH 连接
   */
  closeTab: (tabId: string) => Promise<void>;
  setActiveTab: (tabId: string) => void;
  
  // Actions - Split Panes
  splitPane: (tabId: string, direction: SplitDirection, newSessionId: string, newTerminalType: PaneTerminalType) => void;
  closePane: (tabId: string, paneId: string) => void;
  setActivePaneId: (tabId: string, paneId: string) => void;
  getPaneCount: (tabId: string) => number;
  
  // Actions - UI
  toggleSidebar: () => void;
  setSidebarSection: (section: SidebarSection) => void;
  toggleModal: (modal: keyof ModalsState, isOpen: boolean) => void;
  
  // Actions - Connections & Groups
  loadSavedConnections: () => Promise<void>;
  loadGroups: () => Promise<void>;
  setSelectedGroup: (group: string | null) => void;
  connectToSaved: (connectionId: string) => Promise<void>;
  openConnectionEditor: (connectionId: string) => void;
  
  // Actions - Connection status updates (from backend events)
  updateConnectionState: (connectionId: string, state: SshConnectionState) => void;
  
  // Computed (Helper methods)
  getSession: (sessionId: string) => SessionInfo | undefined;
  getConnection: (connectionId: string) => SshConnectionInfo | undefined;
  getConnectionForSession: (sessionId: string) => SshConnectionInfo | undefined;
}

// Key for localStorage persistence
// NOTE: oxide-ui-state localStorage key is DEPRECATED
// Sidebar state is now managed by settingsStore (oxide-settings-v2)
// This key will be cleaned up in a future version

// Load persisted UI state from localStorage
// NOTE: We don't persist tabs/activeTabId because sessions are memory-only.
// NOTE: sidebarCollapsed/sidebarActiveSection have been migrated to settingsStore
function loadPersistedUIState(): { tabs: Tab[]; activeTabId: string | null } {
  // Just return defaults - sidebar state is loaded from settingsStore
  return {
    tabs: [],
    activeTabId: null,
  };
}

// Save UI state to localStorage
// NOTE: This is now a NO-OP as sidebar state is managed by settingsStore
// Keeping the function signature for backwards compatibility
// eslint-disable-next-line @typescript-eslint/no-unused-vars
export function saveUIState(): void {
  // NO-OP: Sidebar state is now automatically persisted by settingsStore
  // This function is kept for backwards compatibility but does nothing
}

const persistedState = loadPersistedUIState();

export const useAppStore = create<AppStore>((set, get) => ({
  sessions: new Map(),
  connections: new Map(), // 新增：连接池状态
  tabs: persistedState.tabs,
  activeTabId: persistedState.activeTabId,
  // Sidebar state is now delegated to settingsStore
  // These getters provide backwards compatibility for components that read from appStore
  get sidebarCollapsed() {
    return useSettingsStore.getState().settings.sidebarUI.collapsed;
  },
  get sidebarActiveSection() {
    return useSettingsStore.getState().settings.sidebarUI.activeSection;
  },
  reconnectPendingSessionId: null,
  modals: {
    newConnection: false,
    settings: false,
    editConnection: false,
    connectionManager: false,
    autoRoute: false, // 自动路由
  },
  savedConnections: [],
  groups: [],
  selectedGroup: null,
  editingConnection: null,
  networkOnline: true,

  /** @deprecated Use connectSsh() + createTerminalSession() instead */
  connect: async (request: ConnectRequest) => {
    try {
      // 🔄 迁移到新 API: sshConnect + createTerminal
      const connResponse = await api.sshConnect({
        host: request.host,
        port: request.port,
        username: request.username,
        authType: request.auth_type,
        password: request.password,
        keyPath: request.key_path,
        passphrase: request.passphrase,
        name: request.name,
      });

      // 更新连接池状态
      set((state) => {
        const newConnections = new Map(state.connections);
        newConnections.set(connResponse.connectionId, connResponse.connection);
        return { connections: newConnections };
      });

      // 创建终端
      const termResponse = await api.createTerminal({
        connectionId: connResponse.connectionId,
        cols: request.cols,
        rows: request.rows,
      });

      // 合并 ws_token 到 session
      const sessionInfo = { ...termResponse.session, ws_token: termResponse.wsToken };
      
      set((state) => {
        const newSessions = new Map(state.sessions);
        newSessions.set(sessionInfo.id, sessionInfo);
        
        // 更新连接的 terminalIds
        const newConnections = new Map(state.connections);
        const conn = newConnections.get(connResponse.connectionId);
        if (conn) {
          newConnections.set(connResponse.connectionId, {
            ...conn,
            terminalIds: [...conn.terminalIds, sessionInfo.id],
            refCount: conn.refCount + 1,
            state: 'active',
          });
        }
        
        return { sessions: newSessions, connections: newConnections };
      });

      // Open terminal tab by default
      get().createTab('terminal', sessionInfo.id);
      
      return sessionInfo.id;
    } catch (error) {
      console.error('Connection failed:', error);
      throw error;
    }
  },

  // ═══════════════════════════════════════════════════════════════════════════
  // Connection Pool Actions (新架构)
  // ═══════════════════════════════════════════════════════════════════════════

  connectSsh: async (request: SshConnectRequest) => {
    try {
      const response = await api.sshConnect(request);
      
      // 更新连接池状态
      set((state) => {
        const newConnections = new Map(state.connections);
        newConnections.set(response.connectionId, response.connection);
        return { connections: newConnections };
      });
      
      console.log(`SSH connected: ${response.connectionId} (reused: ${response.reused})`);
      return response.connectionId;
    } catch (error) {
      console.error('SSH connection failed:', error);
      throw error;
    }
  },

  disconnectSsh: async (connectionId: string) => {
    try {
      await api.sshDisconnect(connectionId);
      
      set((state) => {
        const newConnections = new Map(state.connections);
        newConnections.delete(connectionId);
        
        // 关闭所有关联的终端 Tab
        const connection = state.connections.get(connectionId);
        const terminalIds = connection?.terminalIds || [];
        const newSessions = new Map(state.sessions);
        const newTabs = state.tabs.filter(t => {
          if (t.sessionId && terminalIds.includes(t.sessionId)) {
            newSessions.delete(t.sessionId);
            return false;
          }
          return true;
        });
        
        let newActiveId = state.activeTabId;
        if (state.activeTabId && !newTabs.find(t => t.id === state.activeTabId)) {
          newActiveId = newTabs.length > 0 ? newTabs[newTabs.length - 1].id : null;
        }

        return { 
          connections: newConnections,
          sessions: newSessions,
          tabs: newTabs,
          activeTabId: newActiveId
        };
      });
    } catch (error) {
      console.error('SSH disconnect failed:', error);
      throw error;
    }
  },

  createTerminalSession: async (connectionId: string, cols?: number, rows?: number) => {
    try {
      const response = await api.createTerminal({
        connectionId,
        cols,
        rows,
      });
      
      // 更新 sessions 和 connections
      set((state) => {
        const newSessions = new Map(state.sessions);
        newSessions.set(response.sessionId, response.session);
        
        // 更新连接的 terminalIds
        const newConnections = new Map(state.connections);
        const connection = newConnections.get(connectionId);
        if (connection) {
          newConnections.set(connectionId, {
            ...connection,
            terminalIds: [...connection.terminalIds, response.sessionId],
            refCount: connection.refCount + 1,
            state: 'active',
          });
        }
        
        return { sessions: newSessions, connections: newConnections };
      });
      
      // 创建终端 Tab
      get().createTab('terminal', response.sessionId);
      
      return response.session;
    } catch (error) {
      console.error('Create terminal failed:', error);
      throw error;
    }
  },

  closeTerminalSession: async (sessionId: string) => {
    try {
      await api.closeTerminal(sessionId);
      
      set((state) => {
        const newSessions = new Map(state.sessions);
        const session = newSessions.get(sessionId);
        newSessions.delete(sessionId);
        
        // 更新连接的引用计数
        const newConnections = new Map(state.connections);
        if (session?.connectionId) {
          const connection = newConnections.get(session.connectionId);
          if (connection) {
            const newTerminalIds = connection.terminalIds.filter(id => id !== sessionId);
            newConnections.set(session.connectionId, {
              ...connection,
              terminalIds: newTerminalIds,
              refCount: Math.max(0, connection.refCount - 1),
              state: newTerminalIds.length === 0 ? 'idle' : 'active',
            });
          }
        }
        
        return { sessions: newSessions, connections: newConnections };
      });
    } catch (error) {
      console.error('Close terminal failed:', error);
      throw error;
    }
  },

  purgeTerminalSession: (sessionId: string) => {
    set((state) => {
      const newSessions = new Map(state.sessions);
      const session = newSessions.get(sessionId);
      if (!session) return state;
      newSessions.delete(sessionId);

      // Update connections map
      const newConnections = new Map(state.connections);
      if (session.connectionId) {
        const connection = newConnections.get(session.connectionId);
        if (connection) {
          const newTerminalIds = connection.terminalIds.filter(id => id !== sessionId);
          newConnections.set(session.connectionId, {
            ...connection,
            terminalIds: newTerminalIds,
            refCount: Math.max(0, connection.refCount - 1),
            state: newTerminalIds.length === 0 ? 'idle' : connection.state,
          });
        }
      }

      // Update tabs (legacy + split panes)
      const updatedTabs: Tab[] = [];
      let newActiveId = state.activeTabId;

      for (const tab of state.tabs) {
        // Legacy single-pane tabs
        if (!tab.rootPane) {
          if (tab.sessionId === sessionId) {
            if (newActiveId === tab.id) {
              newActiveId = null;
            }
            continue; // Drop the tab
          }
          updatedTabs.push(tab);
          continue;
        }

        // Split-pane tabs
        const result = removePanesBySessionId(tab.rootPane, sessionId);
        if (!result.removed) {
          updatedTabs.push(tab);
          continue;
        }

        // If no panes left, drop tab
        if (!result.node) {
          if (newActiveId === tab.id) {
            newActiveId = null;
          }
          continue;
        }

        // If only one pane left, simplify to single pane mode
        if (result.node.type === 'leaf') {
          updatedTabs.push({
            ...tab,
            rootPane: undefined,
            activePaneId: result.node.id,
            sessionId: result.node.sessionId,
            type: result.node.terminalType,
          });
          continue;
        }

        // Keep split pane mode
        const activePaneId = result.newActivePaneId || tab.activePaneId;
        updatedTabs.push({
          ...tab,
          rootPane: result.node,
          activePaneId,
        });
      }

      // Fix activeTabId if it was removed
      if (newActiveId === null && updatedTabs.length > 0) {
        newActiveId = updatedTabs[updatedTabs.length - 1].id;
      }

      return {
        sessions: newSessions,
        connections: newConnections,
        tabs: updatedTabs,
        activeTabId: newActiveId,
      };
    });

    // Also purge terminal mapping in sessionTreeStore (local only)
    void import('./sessionTreeStore')
      .then(({ useSessionTreeStore }) => {
        useSessionTreeStore.getState().purgeTerminalMapping(sessionId);
      })
      .catch(() => {
        // ignore
      });
  },

  refreshConnections: async () => {
    try {
      const connectionsList = await api.sshListConnections();
      set(() => {
        const newConnections = new Map<string, SshConnectionInfo>();
        for (const conn of connectionsList) {
          newConnections.set(conn.id, conn);
        }
        return { connections: newConnections };
      });
    } catch (error) {
      console.error('Refresh connections failed:', error);
    }
  },

  setConnectionKeepAlive: async (connectionId: string, keepAlive: boolean) => {
    try {
      await api.sshSetKeepAlive(connectionId, keepAlive);
      
      set((state) => {
        const newConnections = new Map(state.connections);
        const connection = newConnections.get(connectionId);
        if (connection) {
          newConnections.set(connectionId, { ...connection, keepAlive });
        }
        return { connections: newConnections };
      });
    } catch (error) {
      console.error('Set keep alive failed:', error);
      throw error;
    }
  },

  // ═══════════════════════════════════════════════════════════════════════════

  /** @deprecated Use closeTerminalSession() instead */
  disconnect: async (sessionId: string) => {
    try {
      // 🔄 迁移到新 API: closeTerminal
      await api.closeTerminal(sessionId);
      
      set((state) => {
        const newSessions = new Map(state.sessions);
        const session = newSessions.get(sessionId);
        newSessions.delete(sessionId);
        
        // 更新连接的 terminalIds
        const newConnections = new Map(state.connections);
        if (session?.connectionId) {
          const conn = newConnections.get(session.connectionId);
          if (conn) {
            const newTerminalIds = conn.terminalIds.filter(id => id !== sessionId);
            newConnections.set(session.connectionId, {
              ...conn,
              terminalIds: newTerminalIds,
              refCount: Math.max(0, conn.refCount - 1),
              state: newTerminalIds.length === 0 ? 'idle' : 'active',
            });
          }
        }
        
        // Close associated tabs
        const newTabs = state.tabs.filter(t => t.sessionId !== sessionId);
        let newActiveId = state.activeTabId;
        
        if (state.activeTabId && !newTabs.find(t => t.id === state.activeTabId)) {
          newActiveId = newTabs.length > 0 ? newTabs[newTabs.length - 1].id : null;
        }

        return { 
          sessions: newSessions,
          connections: newConnections,
          tabs: newTabs,
          activeTabId: newActiveId
        };
      });
    } catch (error) {
      console.error('Disconnect failed:', error);
    }
  },

  reconnect: async (sessionId: string) => {
    const session = get().sessions.get(sessionId);
    if (!session) {
      console.warn(`[AppStore] createTab(${type}) missing session ${sessionId}, attempting to hydrate`);
      api.getSession(sessionId)
        .then((fetched) => {
          set((state) => {
            const newSessions = new Map(state.sessions);
            newSessions.set(sessionId, fetched);
            return { sessions: newSessions };
          });
          // Retry tab creation after hydration
          get().createTab(type, sessionId);
        })
        .catch((error) => {
          console.error(`[AppStore] Failed to hydrate session ${sessionId} for ${type} tab:`, error);
        });
      return;
    }

    // Password auth requires user to re-enter password
    if (session.auth_type === 'password') {
      set({ reconnectPendingSessionId: sessionId });
      return;
    }

    // Update state to connecting
    get().updateSessionState(sessionId, 'connecting');

    try {
      // Disconnect existing session first
      // 🔄 迁移到新 API: closeTerminal
      await api.closeTerminal(sessionId).catch((err) => {
        console.warn(`[Reconnect] Failed to close old terminal ${sessionId}:`, err);
        useToastStore.getState().addToast({
          title: i18n.t('connections.toast.close_terminal_failed'),
          description: String(err),
          variant: 'warning',
        });
      });
      
      // Determine auth_type for reconnection:
      // - 'agent' -> use agent
      // - 'key' with key_path -> use key with the specific path
      // - 'key' without key_path -> use default_key (fallback)
      // - 'default_key' -> use default_key
      let reconnectAuthType: 'key' | 'default_key' | 'agent' = 'default_key';
      let reconnectKeyPath: string | undefined = undefined;

      if (session.auth_type === 'agent') {
        reconnectAuthType = 'agent';
      } else if (session.auth_type === 'key' && session.key_path) {
        reconnectAuthType = 'key';
        reconnectKeyPath = session.key_path;
      }
      // else: default_key fallback

      // Reconnect with saved authentication info
      // 🔄 迁移到新 API: sshConnect + createTerminal
      const connResponse = await api.sshConnect({
        host: session.host,
        port: session.port,
        username: session.username,
        authType: reconnectAuthType,
        keyPath: reconnectKeyPath,
        name: session.name,
      });
      
      const termResponse = await api.createTerminal({
        connectionId: connResponse.connectionId,
      });
      
      const newSession = { ...termResponse.session, ws_token: termResponse.wsToken };

      // Update session map with new session info but keep same sessionId in tabs
      set((state) => {
        const newSessions = new Map(state.sessions);
        newSessions.delete(sessionId); // Remove old
        newSessions.set(newSession.id, newSession); // Add new
        
        // Update tabs to point to new session
        const newTabs = state.tabs.map(tab => 
          tab.sessionId === sessionId 
            ? { ...tab, sessionId: newSession.id }
            : tab
        );
        
        return { sessions: newSessions, tabs: newTabs };
      });

      console.log(`Reconnected session: ${sessionId} -> ${newSession.id}`);
    } catch (error) {
      console.error('Reconnect failed:', error);
      get().updateSessionState(sessionId, 'error', String(error));
    }
  },

  reconnectWithPassword: async (sessionId: string, password: string) => {
    const session = get().sessions.get(sessionId);
    if (!session) {
      set({ reconnectPendingSessionId: null });
      return;
    }

    // Clear pending state
    set({ reconnectPendingSessionId: null });

    // Update state to connecting
    get().updateSessionState(sessionId, 'connecting');

    try {
      // Disconnect existing session first
      // 🔄 迁移到新 API: closeTerminal
      await api.closeTerminal(sessionId).catch((err) => {
        console.warn(`[ReconnectWithPassword] Failed to close old terminal ${sessionId}:`, err);
        useToastStore.getState().addToast({
          title: i18n.t('connections.toast.close_terminal_failed'),
          description: String(err),
          variant: 'warning',
        });
      });
      
      // Reconnect with password
      // 🔄 迁移到新 API: sshConnect + createTerminal
      const connResponse = await api.sshConnect({
        host: session.host,
        port: session.port,
        username: session.username,
        authType: 'password',
        password,
        name: session.name,
      });
      
      const termResponse = await api.createTerminal({
        connectionId: connResponse.connectionId,
      });
      
      const newSession = { ...termResponse.session, ws_token: termResponse.wsToken };

      // Update session map with new session info
      set((state) => {
        const newSessions = new Map(state.sessions);
        newSessions.delete(sessionId);
        newSessions.set(newSession.id, newSession);
        
        const newTabs = state.tabs.map(tab => 
          tab.sessionId === sessionId 
            ? { ...tab, sessionId: newSession.id }
            : tab
        );
        
        return { sessions: newSessions, tabs: newTabs };
      });

      console.log(`Reconnected session with password: ${sessionId} -> ${newSession.id}`);
    } catch (error) {
      console.error('Reconnect with password failed:', error);
      get().updateSessionState(sessionId, 'error', String(error));
    }
  },

  cancelReconnectDialog: () => {
    set({ reconnectPendingSessionId: null });
  },

  cancelReconnect: async (sessionId: string) => {
    try {
      await api.cancelReconnect(sessionId);
      // State will be updated via event handler
    } catch (error) {
      console.error('Failed to cancel reconnect:', error);
    }
  },

  updateSessionState: (sessionId, state, error) => {
    set((s) => {
      const session = s.sessions.get(sessionId);
      if (!session) return {};
      
      const newSessions = new Map(s.sessions);
      newSessions.set(sessionId, { ...session, state, error });
      return { sessions: newSessions };
    });
  },

  // 旧的 session_* 事件处理函数已废弃
  // 现在由 useConnectionEvents 统一处理 connection_* 事件

  setNetworkOnline: (online: boolean) => {
    set({ networkOnline: online });
    // Notify backend of network status change
    api.networkStatusChanged(online).catch((e) => {
      console.error('Failed to notify network status:', e);
    });
  },

  createTab: (type, sessionId) => {
    // Handle global/singleton tabs
    if (type === 'settings' || type === 'connection_monitor' || type === 'connection_pool' || type === 'topology' || type === 'file_manager') {
      const existingTab = get().tabs.find(t => t.type === type);
      if (existingTab) {
        set({ activeTabId: existingTab.id });
        return;
      }

      let title = i18n.t('tabs.settings');
      let icon = '⚙️';
      
      if (type === 'connection_monitor') {
        title = i18n.t('tabs.connection_monitor');
        icon = '📊';
      } else if (type === 'connection_pool') {
        title = i18n.t('tabs.connection_pool');
        icon = '🔌';
      } else if (type === 'topology') {
        title = i18n.t('tabs.connection_matrix');
        icon = '🕸️';
      } else if (type === 'file_manager') {
        title = i18n.t('fileManager.title');
        icon = '💾';
      }

      const newTab: Tab = {
        id: crypto.randomUUID(),
        type,
        title,
        icon
      };

      set((state) => ({
        tabs: [...state.tabs, newTab],
        activeTabId: newTab.id
      }));
      return;
    }

    // Handle local terminal tabs (require sessionId but don't require SSH session)
    if (type === 'local_terminal') {
      if (!sessionId) return;

      // Check if a tab with the same sessionId already exists
      const existingTab = get().tabs.find(t => t.type === 'local_terminal' && t.sessionId === sessionId);
      if (existingTab) {
        set({ activeTabId: existingTab.id });
        return;
      }

      // Try to get shell name from localTerminalStore
      // Import dynamically to avoid circular dependency
      let shellLabel = 'Local';
      try {
        // eslint-disable-next-line @typescript-eslint/no-require-imports
        const { useLocalTerminalStore } = require('./localTerminalStore');
        const terminalInfo = useLocalTerminalStore.getState().getTerminal(sessionId);
        if (terminalInfo?.shell?.label) {
          shellLabel = terminalInfo.shell.label;
        }
      } catch {
        // Fallback to default
      }

      const newTab: Tab = {
        id: crypto.randomUUID(),
        type: 'local_terminal',
        sessionId,
        title: shellLabel,
        icon: '▣'
      };

      set((state) => ({
        tabs: [...state.tabs, newTab],
        activeTabId: newTab.id
      }));
      return;
    }

    // Handle IDE tabs (require a connected SFTP session)
    if (type === 'ide') {
      if (!sessionId) return;

      // Check if an IDE tab with the same sessionId already exists
      const existingTab = get().tabs.find(t => t.type === 'ide' && t.sessionId === sessionId);
      if (existingTab) {
        set({ activeTabId: existingTab.id });
        return;
      }

      // Get session name for tab title
      const session = get().sessions.get(sessionId);
      const sessionName = session?.name || 'Remote';

      const newTab: Tab = {
        id: crypto.randomUUID(),
        type: 'ide',
        sessionId,
        title: `${i18n.t('tabs.ide')}: ${sessionName}`,
        icon: '💻'
      };

      set((state) => ({
        tabs: [...state.tabs, newTab],
        activeTabId: newTab.id
      }));
      return;
    }

    // Require sessionId for session-based tabs
    if (!sessionId) return;

    const session = get().sessions.get(sessionId);
    if (!session) return;

    // Check if a tab with the same type and sessionId already exists
    const existingTab = get().tabs.find(t => t.type === type && t.sessionId === sessionId);
    if (existingTab) {
      // Switch to existing tab instead of creating a new one
      set({ activeTabId: existingTab.id });
      return;
    }

    const newTab: Tab = {
      id: crypto.randomUUID(),
      type,
      sessionId,
      title: type === 'terminal' ? session.name : `${type === 'sftp' ? i18n.t('tabs.sftp_prefix') : i18n.t('tabs.forwards_prefix')}: ${session.name}`,
      icon: type === 'terminal' ? '>_' : type === 'sftp' ? '📁' : '🔀'
    };

    set((state) => ({
      tabs: [...state.tabs, newTab],
      activeTabId: newTab.id
    }));
  },

  closeTab: async (tabId) => {
    const tab = get().tabs.find(t => t.id === tabId);
    if (!tab) {
      console.warn(`[closeTab] Tab ${tabId} not found`);
      return;
    }
    
    const sessionId = tab.sessionId;
    const tabType = tab.type;
    
    // ========== Phase 1: UI 乐观更新（立即响应） ==========
    set((state) => {
      const newTabs = state.tabs.filter(t => t.id !== tabId);
      let newActiveId = state.activeTabId;

      if (state.activeTabId === tabId) {
        newActiveId = newTabs.length > 0 ? newTabs[newTabs.length - 1].id : null;
      }

      return {
        tabs: newTabs,
        activeTabId: newActiveId
      };
    });
    
    // 非终端类型的 Tab 无需额外清理
    if (!sessionId || (tabType !== 'terminal' && tabType !== 'local_terminal')) {
      return;
    }
    
    // ========== Phase 2: 获取 session 信息（在删除前） ==========
    const session = get().sessions.get(sessionId);
    const connectionId = session?.connectionId;
    
    // ========== Phase 3: 从 sessions Map 移除 ==========
    set((state) => {
      const newSessions = new Map(state.sessions);
      newSessions.delete(sessionId);
      return { sessions: newSessions };
    });
    
    // ========== Phase 4: 通知 sessionTreeStore 清理映射 ==========
    // 使用动态导入避免循环依赖
    try {
      const { useSessionTreeStore } = await import('./sessionTreeStore');
      useSessionTreeStore.getState().purgeTerminalMapping(sessionId);
    } catch (e) {
      console.warn('[closeTab] Failed to purge terminal mapping:', e);
    }
    
    // ========== Phase 5: 调用后端关闭终端 ==========
    // 本地终端使用不同的关闭接口
    if (tabType === 'local_terminal') {
      try {
        await api.localCloseTerminal(sessionId);
        console.log(`[closeTab] Local terminal ${sessionId} closed`);
      } catch (e) {
        // 终端可能已经不存在，忽略错误
        console.warn(`[closeTab] Failed to close local terminal ${sessionId}:`, e);
      }
      return;
    }
    
    // SSH 终端
    try {
      await api.closeTerminal(sessionId);
      console.log(`[closeTab] Terminal ${sessionId} closed`);
    } catch (e) {
      // 终端可能已经不存在，忽略错误
      console.warn(`[closeTab] Failed to close terminal ${sessionId}:`, e);
    }
    
    // ========== Phase 6: 检查是否需要断开 SSH 连接 ==========
    // 只有当该连接下没有其他终端时才断开
    if (connectionId) {
      const remainingTerminals = Array.from(get().sessions.values())
        .filter(s => s.connectionId === connectionId);
      
      if (remainingTerminals.length === 0) {
        console.log(`[closeTab] No remaining terminals for connection ${connectionId}, disconnecting SSH`);
        try {
          await api.sshDisconnect(connectionId);
          
          // 从 connections Map 移除
          set((state) => {
            const newConnections = new Map(state.connections);
            newConnections.delete(connectionId);
            return { connections: newConnections };
          });
          
          console.log(`[closeTab] SSH connection ${connectionId} disconnected`);
        } catch (e) {
          // 连接可能已经断开，忽略错误
          console.warn(`[closeTab] Failed to disconnect SSH ${connectionId}:`, e);
        }
      } else {
        console.debug(`[closeTab] Connection ${connectionId} still has ${remainingTerminals.length} terminals`);
      }
    }
  },

  setActiveTab: (tabId) => {
    set({ activeTabId: tabId });
  },

  // ═══════════════════════════════════════════════════════════════════════════
  // Split Pane Actions
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Count total panes in a layout tree
   */
  getPaneCount: (tabId) => {
    const tab = get().tabs.find(t => t.id === tabId);
    if (!tab) return 0;
    
    // Single pane mode (no rootPane)
    if (!tab.rootPane) return tab.sessionId ? 1 : 0;
    
    // Count recursively
    const countPanes = (node: PaneNode): number => {
      if (node.type === 'leaf') return 1;
      return node.children.reduce((sum, child) => sum + countPanes(child), 0);
    };
    
    return countPanes(tab.rootPane);
  },

  /**
   * Split the current active pane in the specified direction
   */
  splitPane: (tabId, direction, newSessionId, newTerminalType) => {
    set((state) => {
      const tabIndex = state.tabs.findIndex(t => t.id === tabId);
      if (tabIndex === -1) return state;
      
      const tab = state.tabs[tabIndex];
      
      // Only terminal tabs can be split
      if (tab.type !== 'terminal' && tab.type !== 'local_terminal') {
        console.warn('[AppStore] Cannot split non-terminal tab');
        return state;
      }
      
      // Check pane limit
      const currentCount = get().getPaneCount(tabId);
      if (currentCount >= MAX_PANES_PER_TAB) {
        console.warn(`[AppStore] Maximum panes (${MAX_PANES_PER_TAB}) reached`);
        return state;
      }
      
      const newPaneId = crypto.randomUUID();
      const newPane: PaneLeaf = {
        type: 'leaf',
        id: newPaneId,
        sessionId: newSessionId,
        terminalType: newTerminalType,
      };
      
      let newRootPane: PaneNode;
      
      // Case 1: No rootPane yet (single pane mode)
      if (!tab.rootPane) {
        // Convert existing session to leaf, then wrap in group
        const existingPane: PaneLeaf = {
          type: 'leaf',
          id: tab.activePaneId || crypto.randomUUID(),
          sessionId: tab.sessionId!,
          terminalType: tab.type as PaneTerminalType,
        };
        
        newRootPane = {
          type: 'group',
          id: crypto.randomUUID(),
          direction,
          children: [existingPane, newPane],
          sizes: [50, 50],
        };
      }
      // Case 2: Has rootPane - need to split the active pane
      else {
        const activePaneId = tab.activePaneId;
        if (!activePaneId) {
          console.warn('[AppStore] No active pane to split');
          return state;
        }
        
        // Deep clone and modify the tree
        newRootPane = splitPaneInTree(tab.rootPane, activePaneId, direction, newPane);
      }
      
      // Update tab
      const newTabs = [...state.tabs];
      newTabs[tabIndex] = {
        ...tab,
        rootPane: newRootPane,
        activePaneId: newPaneId, // Focus the new pane
        // Clear legacy sessionId since we now use rootPane
        sessionId: undefined,
      };
      
      return { tabs: newTabs };
    });
  },

  /**
   * Close a specific pane within a tab
   */
  closePane: (tabId, paneId) => {
    set((state) => {
      const tabIndex = state.tabs.findIndex(t => t.id === tabId);
      if (tabIndex === -1) return state;
      
      const tab = state.tabs[tabIndex];
      
      // Single pane mode - close the entire tab
      if (!tab.rootPane) {
        const newTabs = state.tabs.filter(t => t.id !== tabId);
        let newActiveId = state.activeTabId;
        if (state.activeTabId === tabId) {
          newActiveId = newTabs.length > 0 ? newTabs[newTabs.length - 1].id : null;
        }
        return { tabs: newTabs, activeTabId: newActiveId };
      }
      
      // Remove pane from tree
      const result = removePaneFromTree(tab.rootPane, paneId);
      
      // If no panes left, close the tab
      if (!result.node) {
        const newTabs = state.tabs.filter(t => t.id !== tabId);
        let newActiveId = state.activeTabId;
        if (state.activeTabId === tabId) {
          newActiveId = newTabs.length > 0 ? newTabs[newTabs.length - 1].id : null;
        }
        return { tabs: newTabs, activeTabId: newActiveId };
      }
      
      // If only one pane left, simplify to single pane mode
      if (result.node.type === 'leaf') {
        const newTabs = [...state.tabs];
        newTabs[tabIndex] = {
          ...tab,
          rootPane: undefined,
          activePaneId: result.node.id,
          sessionId: result.node.sessionId,
          type: result.node.terminalType,
        };
        return { tabs: newTabs };
      }
      
      // Update with new tree
      const newTabs = [...state.tabs];
      newTabs[tabIndex] = {
        ...tab,
        rootPane: result.node,
        activePaneId: result.newActivePaneId || tab.activePaneId,
      };
      
      return { tabs: newTabs };
    });
  },

  /**
   * Set the active pane within a tab
   */
  setActivePaneId: (tabId, paneId) => {
    set((state) => {
      const tabIndex = state.tabs.findIndex(t => t.id === tabId);
      if (tabIndex === -1) return state;
      
      const newTabs = [...state.tabs];
      newTabs[tabIndex] = {
        ...newTabs[tabIndex],
        activePaneId: paneId,
      };
      
      return { tabs: newTabs };
    });
  },

  // Sidebar actions delegated to settingsStore
  toggleSidebar: () => {
    useSettingsStore.getState().toggleSidebar();
  },

  setSidebarSection: (section) => {
    useSettingsStore.getState().setSidebarSection(section);
  },
  
  toggleModal: (modal, isOpen) => {
    set((state) => ({
      modals: { ...state.modals, [modal]: isOpen }
    }));
  },

  loadSavedConnections: async () => {
    try {
      const connections = await api.getConnections();
      set({ savedConnections: connections });
    } catch (error) {
      console.error('Failed to load saved connections:', error);
    }
  },

  loadGroups: async () => {
    try {
      const groups = await api.getGroups();
      set({ groups });
    } catch (error) {
      console.error('Failed to load groups:', error);
    }
  },

  setSelectedGroup: (group) => {
    set({ selectedGroup: group });
  },

  connectToSaved: async (connectionId) => {
    try {
      // Get full connection info with credentials from backend
      const savedConn = await api.getSavedConnectionForConnect(connectionId);

      // Map auth_type for SshConnectRequest
      const mapAuthType = (authType: string): 'password' | 'key' | 'default_key' | 'agent' => {
        if (authType === 'agent') return 'agent';
        if (authType === 'key') return 'key';
        if (authType === 'password') return 'password';
        return 'default_key';
      };

      // Map auth_type for manual preset (no default_key in HopInfo)
      const mapPresetAuthType = (authType: string): 'password' | 'key' | 'agent' => {
        if (authType === 'agent') return 'agent';
        if (authType === 'key') return 'key';
        if (authType === 'password') return 'password';
        return 'key';
      };

      // TODO: 暂不支持 proxy_chain，需要后续扩展 sshConnect
      if (savedConn.proxy_chain && savedConn.proxy_chain.length > 0) {
        // 使用 session_tree 的手工预设链连接（避免改动 connect_v2 核心握手）
        const hops: ConnectPresetChainRequest['hops'] = savedConn.proxy_chain.map((hop) => ({
          host: hop.host,
          port: hop.port,
          username: hop.username,
          authType: mapPresetAuthType(hop.auth_type),
          password: hop.password,
          keyPath: hop.key_path,
          passphrase: hop.passphrase,
        }));

        const target: ConnectPresetChainRequest['target'] = {
          host: savedConn.host,
          port: savedConn.port,
          username: savedConn.username,
          authType: mapPresetAuthType(savedConn.auth_type),
          password: savedConn.password,
          keyPath: savedConn.key_path,
          passphrase: savedConn.passphrase,
        };

        const request: ConnectPresetChainRequest = {
          savedConnectionId: connectionId,
          hops,
          target,
        };

        const response = await api.connectManualPreset(request);

        // 同步会话树并注册拓扑映射
        const { useSessionTreeStore } = await import('./sessionTreeStore');
        const treeStore = useSessionTreeStore.getState();
        await treeStore.fetchTree();
        treeStore.selectNode(response.targetNodeId);

        for (const nodeId of response.connectedNodeIds) {
          const rawNode = treeStore.getRawNode(nodeId);
          if (rawNode?.sshConnectionId) {
            topologyResolver.register(rawNode.sshConnectionId, nodeId);
          }
        }

        // 为目标节点创建终端并打开标签页
        const terminalId = await treeStore.createTerminalForNode(response.targetNodeId);
        get().createTab('terminal', terminalId);

        useToastStore.getState().addToast({
          title: i18n.t('connections.toast.proxy_chain_established'),
          description: i18n.t('connections.toast.proxy_chain_desc', { depth: response.chainDepth }),
          variant: 'success',
        });

        await api.markConnectionUsed(connectionId);
        return;
      }

      // 使用新的 sshConnect API - 只建立连接，不创建终端
      const sshRequest: SshConnectRequest = {
        host: savedConn.host,
        port: savedConn.port,
        username: savedConn.username,
        authType: mapAuthType(savedConn.auth_type),
        password: savedConn.password,
        keyPath: savedConn.key_path,
        passphrase: savedConn.passphrase,
        name: savedConn.name,
        reuseConnection: true, // 尝试复用已有连接
      };

      await get().connectSsh(sshRequest);
      await api.markConnectionUsed(connectionId);
    } catch (error) {
      console.error('Failed to connect to saved connection:', error);
      // Open editor on any error
      get().openConnectionEditor(connectionId);
    }
  },

  openConnectionEditor: (connectionId) => {
    const connection = get().savedConnections.find(c => c.id === connectionId);
    if (connection) {
      set({ editingConnection: connection });
      get().toggleModal('editConnection', true);
    }
  },

  getSession: (sessionId) => {
    return get().sessions.get(sessionId);
  },

  getConnection: (connectionId) => {
    return get().connections.get(connectionId);
  },

  getConnectionForSession: (sessionId) => {
    const session = get().sessions.get(sessionId);
    if (session?.connectionId) {
      return get().connections.get(session.connectionId);
    }
    return undefined;
  },

  updateConnectionState: (connectionId, state) => {
    set((prev) => {
      const connection = prev.connections.get(connectionId);
      if (!connection) {
        console.warn(`[Store] Connection not found: ${connectionId}`);
        return prev;
      }

      const newConnections = new Map(prev.connections);
      newConnections.set(connectionId, {
        ...connection,
        state,
      });

      console.log(`[Store] Connection ${connectionId} state updated to:`, state);
      return { connections: newConnections };
    });
  }
}));

// ═══════════════════════════════════════════════════════════════════════════
// Split Pane Tree Helper Functions
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Find a pane in the tree and split it
 * Returns a new tree with the split applied
 */
function splitPaneInTree(
  node: PaneNode,
  targetPaneId: string,
  direction: SplitDirection,
  newPane: PaneLeaf
): PaneNode {
  // Leaf node: check if this is the target
  if (node.type === 'leaf') {
    if (node.id === targetPaneId) {
      // Create a new group containing both the original and new pane
      return {
        type: 'group',
        id: crypto.randomUUID(),
        direction,
        children: [node, newPane],
        sizes: [50, 50],
      };
    }
    return node;
  }
  
  // Group node: recurse into children
  const newChildren = node.children.map(child => 
    splitPaneInTree(child, targetPaneId, direction, newPane)
  );
  
  // Check if any child was split (by comparing references)
  const wasModified = newChildren.some((child, i) => child !== node.children[i]);
  
  if (wasModified) {
    // A child was split - need to update sizes
    const newSizes = node.sizes ? [...node.sizes] : node.children.map(() => 100 / node.children.length);
    
    // Find which child was split and adjust
    for (let i = 0; i < newChildren.length; i++) {
      if (newChildren[i] !== node.children[i] && newChildren[i].type === 'group') {
        // This child was converted to a group - keep its size the same
        // The new group's internal sizes handle the 50/50 split
      }
    }
    
    return {
      ...node,
      children: newChildren,
      sizes: newSizes,
    };
  }
  
  return node;
}

/**
 * Remove a pane from the tree
 * Returns the modified tree and a suggested new active pane ID
 */
function removePaneFromTree(
  node: PaneNode,
  paneId: string
): { node: PaneNode | null; newActivePaneId?: string } {
  // Leaf node: check if this is the target
  if (node.type === 'leaf') {
    if (node.id === paneId) {
      return { node: null };
    }
    return { node };
  }
  
  // Group node: recurse into children
  const newChildren: PaneNode[] = [];
  let removedIndex = -1;
  let newActivePaneId: string | undefined;
  
  for (let i = 0; i < node.children.length; i++) {
    const result = removePaneFromTree(node.children[i], paneId);
    if (result.node === null) {
      removedIndex = i;
      newActivePaneId = result.newActivePaneId;
    } else {
      newChildren.push(result.node);
      if (result.newActivePaneId) {
        newActivePaneId = result.newActivePaneId;
      }
    }
  }
  
  // If nothing was removed, return unchanged
  if (removedIndex === -1) {
    return { node };
  }
  
  // If no children left, return null
  if (newChildren.length === 0) {
    return { node: null };
  }
  
  // If only one child left, unwrap it (remove the group)
  if (newChildren.length === 1) {
    const remaining = newChildren[0];
    // Suggest the first leaf as new active
    if (!newActivePaneId) {
      newActivePaneId = findFirstLeaf(remaining)?.id;
    }
    return { node: remaining, newActivePaneId };
  }
  
  // Multiple children remain - update sizes proportionally
  const oldSizes = node.sizes || node.children.map(() => 100 / node.children.length);
  const removedSize = oldSizes[removedIndex] || 0;
  const remainingTotal = 100 - removedSize;
  
  const newSizes = oldSizes
    .filter((_, i) => i !== removedIndex)
    .map(size => (size / remainingTotal) * 100);
  
  // Suggest the next sibling as new active
  if (!newActivePaneId) {
    const nextIndex = Math.min(removedIndex, newChildren.length - 1);
    newActivePaneId = findFirstLeaf(newChildren[nextIndex])?.id;
  }
  
  return {
    node: {
      ...node,
      children: newChildren,
      sizes: newSizes,
    },
    newActivePaneId,
  };
}

/**
 * Remove all panes that match a sessionId
 * Returns modified tree, removal flag, and suggested new active pane ID
 */
function removePanesBySessionId(
  node: PaneNode,
  sessionId: string
): { node: PaneNode | null; removed: boolean; newActivePaneId?: string } {
  if (node.type === 'leaf') {
    if (node.sessionId === sessionId) {
      return { node: null, removed: true };
    }
    return { node, removed: false };
  }

  const newChildren: PaneNode[] = [];
  const removedIndices: number[] = [];
  let newActivePaneId: string | undefined;
  let removed = false;

  for (let i = 0; i < node.children.length; i++) {
    const result = removePanesBySessionId(node.children[i], sessionId);
    if (result.node === null) {
      removedIndices.push(i);
      removed = true;
      if (result.newActivePaneId) {
        newActivePaneId = result.newActivePaneId;
      }
    } else {
      newChildren.push(result.node);
      if (result.newActivePaneId) {
        newActivePaneId = result.newActivePaneId;
      }
      if (result.removed) {
        removed = true;
      }
    }
  }

  if (!removed) {
    return { node, removed: false };
  }

  if (newChildren.length === 0) {
    return { node: null, removed: true };
  }

  if (newChildren.length === 1) {
    const remaining = newChildren[0];
    if (!newActivePaneId) {
      newActivePaneId = findFirstLeaf(remaining)?.id;
    }
    return { node: remaining, removed: true, newActivePaneId };
  }

  const oldSizes = node.sizes || node.children.map(() => 100 / node.children.length);
  const remainingSizes = oldSizes.filter((_, idx) => !removedIndices.includes(idx));
  const remainingTotal = remainingSizes.reduce((sum, size) => sum + size, 0);
  const newSizes = remainingTotal > 0
    ? remainingSizes.map(size => (size / remainingTotal) * 100)
    : remainingSizes.map(() => 100 / remainingSizes.length);

  if (!newActivePaneId) {
    newActivePaneId = findFirstLeaf(newChildren[0])?.id;
  }

  return {
    node: {
      ...node,
      children: newChildren,
      sizes: newSizes,
    },
    removed: true,
    newActivePaneId,
  };
}

/**
 * Find the first leaf node in a tree (for focus fallback)
 */
function findFirstLeaf(node: PaneNode): PaneLeaf | null {
  if (node.type === 'leaf') return node;
  if (node.children.length === 0) return null;
  return findFirstLeaf(node.children[0]);
}

/**
 * Find all leaf pane IDs in a tree
 */
export function getAllPaneIds(node: PaneNode): string[] {
  if (node.type === 'leaf') return [node.id];
  return node.children.flatMap(child => getAllPaneIds(child));
}

/**
 * Find a specific pane by ID in the tree
 */
export function findPaneById(node: PaneNode, paneId: string): PaneLeaf | null {
  if (node.type === 'leaf') {
    return node.id === paneId ? node : null;
  }
  for (const child of node.children) {
    const found = findPaneById(child, paneId);
    if (found) return found;
  }
  return null;
}

/**
 * Get session info by ID (convenience function for use outside React components)
 * Used for dynamic key generation when ws_url changes
 */
export function getSession(sessionId: string): SessionInfo | undefined {
  return useAppStore.getState().sessions.get(sessionId);
}
