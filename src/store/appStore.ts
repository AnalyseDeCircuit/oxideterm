// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { create } from 'zustand';
import { api } from '../lib/api';
import { useToastStore } from '../hooks/useToast';
import { topologyResolver } from '../lib/topologyResolver';
import { useSettingsStore, type SidebarSection } from './settingsStore';
import i18n from '../i18n';
import { useSessionTreeStore } from './sessionTreeStore';
import { useLocalTerminalStore } from './localTerminalStore';
import { 
  SessionInfo, 
  Tab, 
  TabType,
  SessionState,
  ConnectionInfo,
  SshConnectionInfo,
  SshConnectionState,
  ConnectPresetChainRequest,
  PaneNode,
  PaneLeaf,
  SplitDirection,
  PaneTerminalType,
  MAX_PANES_PER_TAB,
  RemoteEnvInfo,
} from '../types';

interface ModalsState {
  newConnection: boolean;
  settings: boolean;
  editConnection: boolean;
  connectionManager: boolean; // 新增：连接管理面板
  autoRoute: boolean; // 自动路由选择器
}

/** Pre-fill data for NewConnectionModal when opened via Quick Connect */
export type QuickConnectData = {
  host: string;
  port: number;
  username: string;
} | null;

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
  /** Last active tab type that is not 'ai_agent' — used by Agent to inherit tool context */
  lastNonAgentTabType: TabType | null;
  /** Pre-fill data consumed by NewConnectionModal on open */
  quickConnectData: QuickConnectData;
  savedConnections: ConnectionInfo[];
  groups: string[];
  selectedGroup: string | null;
  editingConnection: ConnectionInfo | null;
  networkOnline: boolean;

  // Actions - Sessions
  updateSessionState: (sessionId: string, state: SessionState, error?: string) => void;
  
  // Actions - Connection Pool
  disconnectSsh: (connectionId: string) => Promise<void>;
  createTerminalSession: (connectionId: string, cols?: number, rows?: number) => Promise<SessionInfo>;
  closeTerminalSession: (sessionId: string) => Promise<void>;
  /**
   * Force-remove a terminal session locally (no backend call).
   * Used when backend no longer recognizes the session.
   */
  purgeTerminalSession: (sessionId: string) => void;
  /**
   * @deprecated Phase 5: 使用 node:state 事件驱动替代轮询。仅保留用于初始加载和兼容。
   */
  refreshConnections: () => Promise<void>;
  setConnectionKeepAlive: (connectionId: string, keepAlive: boolean) => Promise<void>;
  
  // Actions - Network
  setNetworkOnline: (online: boolean) => void;
  
  // Actions - Tabs
  createTab: (type: TabType, sessionId?: string, options?: { nodeId?: string; skipFocus?: boolean }) => void;
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
  nextTab: () => void;
  prevTab: () => void;
  goToTab: (index: number) => void;
  moveTab: (fromIndex: number, toIndex: number) => void;

  // Actions - Tab Navigation History
  /** Tab navigation history stack */
  tabHistory: string[];
  /** Current position in navigation history (-1 = no history) */
  tabHistoryCursor: number;
  /** Whether a tab switch is currently triggered by back/forward navigation */
  _isNavigating: boolean;
  navigateBack: () => void;
  navigateForward: () => void;
  
  // Actions - Split Panes
  splitPane: (tabId: string, direction: SplitDirection, newSessionId: string, newTerminalType: PaneTerminalType) => void;
  closePane: (tabId: string, paneId: string) => void;
  setActivePaneId: (tabId: string, paneId: string) => void;
  getPaneCount: (tabId: string) => number;
  resetToSinglePane: (tabId: string) => void;
  
  /**
   * Replace old sessionId with new sessionId across ALL pane trees.
   * Used by reconnect orchestrator after creating a new terminal session.
   */
  updatePaneSessionId: (oldSessionId: string, newSessionId: string) => void;
  
  // Actions - UI
  toggleSidebar: () => void;
  setSidebarSection: (section: SidebarSection) => void;
  toggleModal: (modal: keyof ModalsState, isOpen: boolean, quickConnect?: QuickConnectData) => void;
  
  // Actions - Connections & Groups
  loadSavedConnections: () => Promise<void>;
  loadGroups: () => Promise<void>;
  setSelectedGroup: (group: string | null) => void;
  connectToSaved: (connectionId: string) => Promise<void>;
  openConnectionEditor: (connectionId: string) => void;
  
  // Actions - Connection status updates (from backend events)
  updateConnectionState: (connectionId: string, state: SshConnectionState) => void;
  updateConnectionRemoteEnv: (connectionId: string, remoteEnv: RemoteEnvInfo) => void;
  
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

const persistedState = loadPersistedUIState();

type SyntheticConnectionSeed = {
  id: string;
  host: string;
  port: number;
  username: string;
  parentConnectionId?: string | null;
  terminalIds?: string[];
  sftpSessionId?: string | null;
  forwardIds?: string[];
  keepAlive?: boolean;
  state?: SshConnectionState;
  remoteEnv?: RemoteEnvInfo;
  refCount?: number;
  createdAt?: string;
  lastActive?: string;
};

function uniqueStringValues(values: Array<string | null | undefined>): string[] {
  return Array.from(
    new Set(values.filter((value): value is string => !!value)),
  );
}

function computeSyntheticRefCount(
  terminalIds: string[],
  sftpSessionId?: string,
  forwardIds: string[] = [],
): number {
  return terminalIds.length + (sftpSessionId ? 1 : 0) + forwardIds.length;
}

function nextConnectionStateAfterTerminalRemoval(
  connection: SshConnectionInfo,
  nextRefCount: number,
): SshConnectionState {
  if (
    nextRefCount === 0 &&
    !connection.sftpSessionId &&
    connection.forwardIds.length === 0 &&
    (connection.state === 'active' || connection.state === 'idle')
  ) {
    return 'idle';
  }

  return connection.state;
}

export function upsertSyntheticConnection(
  connections: ReadonlyMap<string, SshConnectionInfo>,
  seed: SyntheticConnectionSeed,
): Map<string, SshConnectionInfo> {
  const next = new Map(connections);
  const existing = next.get(seed.id);
  const terminalIds = uniqueStringValues([
    ...(existing?.terminalIds ?? []),
    ...(seed.terminalIds ?? []),
  ]);
  const forwardIds = uniqueStringValues([
    ...(existing?.forwardIds ?? []),
    ...(seed.forwardIds ?? []),
  ]);
  const sftpSessionId = seed.sftpSessionId === undefined
    ? existing?.sftpSessionId
    : seed.sftpSessionId || undefined;
  const computedRefCount = computeSyntheticRefCount(
    terminalIds,
    sftpSessionId,
    forwardIds,
  );
  const now = seed.lastActive ?? new Date().toISOString();

  next.set(seed.id, {
    id: seed.id,
    host: seed.host,
    port: seed.port,
    username: seed.username,
    state: seed.state ?? existing?.state ?? 'active',
    refCount: seed.refCount ?? computedRefCount,
    keepAlive: seed.keepAlive ?? existing?.keepAlive ?? false,
    createdAt: seed.createdAt ?? existing?.createdAt ?? now,
    lastActive: now,
    terminalIds,
    sftpSessionId,
    forwardIds,
    parentConnectionId: seed.parentConnectionId ?? existing?.parentConnectionId,
    remoteEnv: seed.remoteEnv ?? existing?.remoteEnv,
  });

  return next;
}

export function removeConnectionsById(
  connections: ReadonlyMap<string, SshConnectionInfo>,
  connectionIds: Iterable<string>,
): Map<string, SshConnectionInfo> {
  const next = new Map(connections);
  for (const connectionId of connectionIds) {
    next.delete(connectionId);
  }
  return next;
}

export function removeTerminalSessionsFromConnections(
  connections: ReadonlyMap<string, SshConnectionInfo>,
  sessions: ReadonlyMap<string, SessionInfo>,
  sessionIds: Iterable<string>,
): Map<string, SshConnectionInfo> {
  const next = new Map(connections);
  const sessionIdsByConnection = new Map<string, Set<string>>();

  for (const sessionId of sessionIds) {
    const session = sessions.get(sessionId);
    if (!session?.connectionId) {
      continue;
    }

    const ids = sessionIdsByConnection.get(session.connectionId) ?? new Set<string>();
    ids.add(sessionId);
    sessionIdsByConnection.set(session.connectionId, ids);
  }

  for (const [connectionId, removedSessionIds] of sessionIdsByConnection) {
    const connection = next.get(connectionId);
    if (!connection) {
      continue;
    }

    const nextTerminalIds = connection.terminalIds.filter(
      (terminalId) => !removedSessionIds.has(terminalId),
    );
    if (nextTerminalIds.length === connection.terminalIds.length) {
      continue;
    }

    const nextRefCount = computeSyntheticRefCount(
      nextTerminalIds,
      connection.sftpSessionId,
      connection.forwardIds,
    );
    next.set(connectionId, {
      ...connection,
      terminalIds: nextTerminalIds,
      refCount: nextRefCount,
      state: nextConnectionStateAfterTerminalRemoval(connection, nextRefCount),
      lastActive: new Date().toISOString(),
    });
  }

  return next;
}

export const useAppStore = create<AppStore>((set, get) => ({
  sessions: new Map(),
  connections: new Map(), // 新增：连接池状态
  tabs: persistedState.tabs,
  activeTabId: persistedState.activeTabId,
  tabHistory: [],
  tabHistoryCursor: -1,
  _isNavigating: false,
  lastNonAgentTabType: null,
  // Sidebar state is now delegated to settingsStore
  // These getters provide backwards compatibility for components that read from appStore
  get sidebarCollapsed() {
    return useSettingsStore.getState().settings.sidebarUI.collapsed;
  },
  get sidebarActiveSection() {
    return useSettingsStore.getState().settings.sidebarUI.activeSection;
  },
  modals: {
    newConnection: false,
    settings: false,
    editConnection: false,
    connectionManager: false,
    autoRoute: false, // 自动路由
  },
  quickConnectData: null,
  savedConnections: [],
  groups: [],
  selectedGroup: null,
  editingConnection: null,
  networkOnline: true,

  // ═══════════════════════════════════════════════════════════════════════════
  // Connection Pool Actions
  // ═══════════════════════════════════════════════════════════════════════════

  disconnectSsh: async (connectionId: string) => {
    try {
      await api.sshDisconnect(connectionId);
      const nodeId = topologyResolver.getNodeId(connectionId);
      const terminalIdsToPurge = Array.from(get().sessions.entries())
        .filter(([, session]) => session.connectionId === connectionId)
        .map(([sessionId]) => sessionId);
      
      set((state) => {
        const newConnections = new Map(state.connections);
        newConnections.delete(connectionId);

        const newSessions = new Map(state.sessions);
        const sessionIdSet = new Set<string>();
        for (const [sessionId, session] of state.sessions) {
          if (session.connectionId === connectionId) {
            sessionIdSet.add(sessionId);
            newSessions.delete(sessionId);
          }
        }

        const newTabs = state.tabs.filter((tab) => {
          if (tabReferencesAnySession(tab, sessionIdSet)) {
            return false;
          }
          if (nodeId && tab.nodeId === nodeId) {
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

      for (const terminalId of terminalIdsToPurge) {
        useSessionTreeStore.getState().purgeTerminalMapping(terminalId);
      }

      if (nodeId) {
        topologyResolver.unregister(nodeId);
      }
    } catch (error) {
      console.error('SSH disconnect failed:', error);
      throw error;
    }
  },

  createTerminalSession: async (connectionId: string, cols?: number, rows?: number) => {
    try {
      // 从 settingsStore 获取后端缓冲区配置
      const { useSettingsStore } = await import('./settingsStore');
      const bufferSettings = useSettingsStore.getState().settings.buffer;
      
      const response = await api.createTerminal({
        connectionId,
        cols,
        rows,
        maxBufferLines: bufferSettings.maxLines,
        saveOnDisconnect: bufferSettings.saveOnDisconnect,
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

  /** @deprecated Phase 5: 使用 node:state 事件驱动替代轮询 */
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

  createTab: (type, sessionId, options) => {
    // Plugin tabs are created via ctx.ui.openTab() — not through this path
    if (type === 'plugin') {
      return;
    }

    // nodeId is required for SFTP/IDE/Forwards tabs — auto-migrate from sessionId if missing
    if ((type === 'sftp' || type === 'ide' || type === 'forwards') && !options?.nodeId && sessionId) {
      const node = useSessionTreeStore.getState().getNodeByTerminalId(sessionId);
      if (node) {
        console.info(`[AppStore] Auto-migrated ${type} tab: sessionId=${sessionId} → nodeId=${node.id}`);
        options = { ...options, nodeId: node.id };
      } else {
        console.warn(`[AppStore] Creating ${type} tab without nodeId (no node found for sessionId=${sessionId}). Tab may not survive reconnects.`);
      }
    }

    // Handle global/singleton tabs
    if (type === 'settings' || type === 'connection_monitor' || type === 'connection_pool' || type === 'topology' || type === 'file_manager' || type === 'session_manager' || type === 'plugin_manager' || type === 'graphics' || type === 'launcher' || type === 'ai_agent') {
      const existingTab = get().tabs.find(t => t.type === type);
      if (existingTab) {
        if (!options?.skipFocus) set({ activeTabId: existingTab.id });
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
      } else if (type === 'session_manager') {
        title = i18n.t('tabs.session_manager');
        icon = '📋';
      } else if (type === 'plugin_manager') {
        title = i18n.t('tabs.plugin_manager', 'Plugins');
        icon = '🧩';
      } else if (type === 'graphics') {
        title = i18n.t('graphics.tab_title', 'WSL Graphics');
        icon = '🖥️';
      } else if (type === 'launcher') {
        title = i18n.t('launcher.tabTitle', 'Launcher');
        icon = '🚀';
      } else if (type === 'ai_agent') {
        title = i18n.t('agent.tabTitle', 'AI Agent');
        icon = '🤖';
      }

      const newTab: Tab = {
        id: crypto.randomUUID(),
        type,
        title,
        icon
      };

      set((state) => ({
        tabs: [...state.tabs, newTab],
        ...(options?.skipFocus ? {} : { activeTabId: newTab.id })
      }));
      return;
    }

    // Handle local terminal tabs (require sessionId but don't require SSH session)
    if (type === 'local_terminal') {
      if (!sessionId) return;

      // Check if a tab with the same sessionId already exists
      const existingTab = get().tabs.find(t => t.type === 'local_terminal' && t.sessionId === sessionId);
      if (existingTab) {
        if (!options?.skipFocus) set({ activeTabId: existingTab.id });
        return;
      }

      // Try to get shell name from localTerminalStore
      let shellLabel = 'Local';
      const terminalInfo = useLocalTerminalStore.getState().getTerminal(sessionId);
      if (terminalInfo?.shell?.label) {
        shellLabel = terminalInfo.shell.label;
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
        ...(options?.skipFocus ? {} : { activeTabId: newTab.id })
      }));
      return;
    }

    // Handle IDE tabs (require a connected SFTP session)
    if (type === 'ide') {
      if (!sessionId) return;

      const nodeId = options?.nodeId;

      // Dedup IDE tab by nodeId (canonical) — sessionId fallback only for legacy tabs
      const existingTab = get().tabs.find(t => {
        if (t.type !== 'ide') return false;
        if (nodeId) return t.nodeId === nodeId;
        return !t.nodeId && t.sessionId === sessionId;
      });
      if (existingTab) {
        if (!options?.skipFocus) set({ activeTabId: existingTab.id });
        return;
      }

      // Get session name for tab title
      const session = get().sessions.get(sessionId);
      const sessionName = session?.name || 'Remote';

      const newTab: Tab = {
        id: crypto.randomUUID(),
        type: 'ide',
        sessionId,
        ...(nodeId ? { nodeId } : {}),
        title: `${i18n.t('tabs.ide')}: ${sessionName}`,
        icon: '💻'
      };

      set((state) => ({
        tabs: [...state.tabs, newTab],
        ...(options?.skipFocus ? {} : { activeTabId: newTab.id })
      }));
      return;
    }

    // Require sessionId for session-based tabs
    if (!sessionId) return;

    const session = get().sessions.get(sessionId);
    if (!session) return;

    const nodeId = options?.nodeId;

    // Dedup by nodeId (canonical for sftp/forwards) — sessionId fallback only for legacy tabs
    const existingTab = get().tabs.find(t => {
      if (t.type !== type) return false;
      if ((type === 'sftp' || type === 'forwards') && nodeId) return t.nodeId === nodeId;
      if ((type === 'sftp' || type === 'forwards')) return !t.nodeId && t.sessionId === sessionId;
      return t.sessionId === sessionId;
    });
    if (existingTab) {
      // Switch to existing tab instead of creating a new one
      if (!options?.skipFocus) set({ activeTabId: existingTab.id });
      return;
    }

    const newTab: Tab = {
      id: crypto.randomUUID(),
      type,
      sessionId,
      ...(((type === 'sftp' || type === 'forwards') && nodeId) ? { nodeId } : {}),
      title: type === 'terminal' ? session.name : `${type === 'sftp' ? i18n.t('tabs.sftp_prefix') : i18n.t('tabs.forwards_prefix')}: ${session.name}`,
      icon: type === 'terminal' ? '>_' : type === 'sftp' ? '📁' : '🔀'
    };

    set((state) => ({
      tabs: [...state.tabs, newTab],
      ...(options?.skipFocus ? {} : { activeTabId: newTab.id })
    }));
  },

  closeTab: async (tabId) => {
    const tab = get().tabs.find(t => t.id === tabId);
    if (!tab) {
      console.warn(`[closeTab] Tab ${tabId} not found`);
      return;
    }
    
    const tabType = tab.type;
    
    // ========== Phase 1: 收集分屏中所有终端 session ==========
    // v1.4.0: 支持递归清理分屏中的所有 PTY 进程
    let localTerminalIds: string[] = [];
    let sshTerminalIds: string[] = [];
    
    if (tab.rootPane) {
      // Tab 有分屏布局，递归收集所有 pane 的 sessionId
      const sessions = collectAllPaneSessions(tab.rootPane);
      localTerminalIds = sessions.localTerminalIds;
      sshTerminalIds = sessions.sshTerminalIds;
      console.log(`[closeTab] Split pane tab: ${localTerminalIds.length} local, ${sshTerminalIds.length} ssh terminals`);
    } else if (tab.sessionId) {
      // 单窗格模式
      if (tabType === 'local_terminal') {
        localTerminalIds = [tab.sessionId];
      } else if (tabType === 'terminal') {
        sshTerminalIds = [tab.sessionId];
      }
    }
    
    // ========== Phase 2: UI 乐观更新（立即响应） ==========
    set((state) => {
      const oldIndex = state.tabs.findIndex(t => t.id === tabId);
      const newTabs = state.tabs.filter(t => t.id !== tabId);
      let newActiveId = state.activeTabId;

      if (state.activeTabId === tabId) {
        // Activate the adjacent tab: prefer right neighbour, fall back to left
        if (newTabs.length > 0) {
          const nextIndex = Math.min(oldIndex, newTabs.length - 1);
          newActiveId = newTabs[nextIndex].id;
        } else {
          newActiveId = null;
        }
      }

      return {
        tabs: newTabs,
        activeTabId: newActiveId
      };
    });
    
    // 非终端类型的 Tab 无需额外清理
    if (tabType !== 'terminal' && tabType !== 'local_terminal') {
      return;
    }
    
    // ========== Phase 3: 从 sessions Map 移除所有关联 session ==========
    const allSessionIds = [...localTerminalIds, ...sshTerminalIds];
    
    set((state) => {
      const newSessions = new Map(state.sessions);
      for (const sid of allSessionIds) {
        newSessions.delete(sid);
      }
      return {
        sessions: newSessions,
        connections: removeTerminalSessionsFromConnections(
          state.connections,
          state.sessions,
          sshTerminalIds,
        ),
      };
    });
    
    // ========== Phase 4: 通知 sessionTreeStore 清理映射 ==========
    // 使用动态导入避免循环依赖
    try {
      const { useSessionTreeStore } = await import('./sessionTreeStore');
      for (const sid of allSessionIds) {
        useSessionTreeStore.getState().purgeTerminalMapping(sid);
      }
    } catch (e) {
      console.warn('[closeTab] Failed to purge terminal mapping:', e);
    }
    
    // ========== Phase 5: 关闭所有本地终端 PTY ==========
    // v1.4.0: 递归关闭分屏中的所有本地终端
    if (localTerminalIds.length > 0) {
      const { useLocalTerminalStore } = await import('./localTerminalStore');
      
      // 并行关闭所有本地终端（跳过已后台挂起的会话）
      const bgSessions = useLocalTerminalStore.getState().backgroundSessions;
      await Promise.all(
        localTerminalIds.map(async (sid) => {
          // Skip detached sessions — they stay alive in the background
          if (bgSessions.has(sid)) {
            console.log(`[closeTab] Local terminal ${sid} is detached, skipping backend close`);
            return;
          }
          try {
            await api.localCloseTerminal(sid);
            console.log(`[closeTab] Local terminal ${sid} closed`);
          } catch (e) {
            // 终端可能已经不存在，忽略错误
            console.warn(`[closeTab] Failed to close local terminal ${sid}:`, e);
          }
        })
      );
      
      // Strong Sync: 刷新 localTerminalStore 确保状态一致
      // 这会从后端重新获取终端列表，确保侧边栏计数正确
      await useLocalTerminalStore.getState().refreshTerminals();
      console.log('[closeTab] Local terminal store refreshed (Strong Sync)');
    }
    
    // ========== Phase 6: 关闭所有 SSH 终端 ==========
    for (const sid of sshTerminalIds) {
      try {
        await api.closeTerminal(sid);
        console.log(`[closeTab] Terminal ${sid} closed`);
      } catch (e) {
        // 终端可能已经不存在，忽略错误
        console.warn(`[closeTab] Failed to close terminal ${sid}:`, e);
      }
    }

    // SSH 连接生命周期由后端 idle timer 管理，不在 closeTab 中断连
    // 用户可通过右键"断开"或后端空闲超时来断开 SSH
  },

  setActiveTab: (tabId) => {
    const state = get();

    // Track the last non-agent tab type so Agent can inherit tool context
    const targetTab = state.tabs.find(t => t.id === tabId);
    const patch: Partial<AppStore> = { activeTabId: tabId };
    if (targetTab && targetTab.type !== 'ai_agent') {
      patch.lastNonAgentTabType = targetTab.type;
    }

    // Push to navigation history unless we're navigating via back/forward
    if (!state._isNavigating && tabId !== state.activeTabId) {
      // Truncate any forward history beyond cursor
      const newHistory = state.tabHistory.slice(0, state.tabHistoryCursor + 1);
      // Don't push duplicate consecutive entries
      if (newHistory[newHistory.length - 1] !== state.activeTabId && state.activeTabId) {
        newHistory.push(state.activeTabId);
      }
      newHistory.push(tabId);
      // Cap history size at 50
      if (newHistory.length > 50) newHistory.splice(0, newHistory.length - 50);
      set({ ...patch, tabHistory: newHistory, tabHistoryCursor: newHistory.length - 1 });
    } else {
      set(patch);
    }
  },

  nextTab: () => {
    const { tabs, activeTabId } = get();
    if (tabs.length === 0) return;
    const currentIndex = tabs.findIndex(t => t.id === activeTabId);
    const nextIndex = (currentIndex + 1) % tabs.length;
    get().setActiveTab(tabs[nextIndex].id);
  },

  prevTab: () => {
    const { tabs, activeTabId } = get();
    if (tabs.length === 0) return;
    const currentIndex = tabs.findIndex(t => t.id === activeTabId);
    const prevIndex = (currentIndex - 1 + tabs.length) % tabs.length;
    get().setActiveTab(tabs[prevIndex].id);
  },

  goToTab: (index: number) => {
    const { tabs } = get();
    if (index >= 0 && index < tabs.length) {
      get().setActiveTab(tabs[index].id);
    }
  },

  navigateBack: () => {
    const { tabHistory, tabHistoryCursor, tabs } = get();
    if (tabHistoryCursor <= 0) return;
    // Find the previous valid entry (tab might have been closed)
    let newCursor = tabHistoryCursor - 1;
    while (newCursor >= 0) {
      const targetTabId = tabHistory[newCursor];
      if (tabs.find(t => t.id === targetTabId)) {
        set({ _isNavigating: true, tabHistoryCursor: newCursor, activeTabId: targetTabId });
        set({ _isNavigating: false });
        return;
      }
      newCursor--;
    }
  },

  navigateForward: () => {
    const { tabHistory, tabHistoryCursor, tabs } = get();
    if (tabHistoryCursor >= tabHistory.length - 1) return;
    // Find the next valid entry
    let newCursor = tabHistoryCursor + 1;
    while (newCursor < tabHistory.length) {
      const targetTabId = tabHistory[newCursor];
      if (tabs.find(t => t.id === targetTabId)) {
        set({ _isNavigating: true, tabHistoryCursor: newCursor, activeTabId: targetTabId });
        set({ _isNavigating: false });
        return;
      }
      newCursor++;
    }
  },

  moveTab: (fromIndex, toIndex) => {
    set((state) => {
      if (fromIndex === toIndex) return state;
      const newTabs = [...state.tabs];
      const [moved] = newTabs.splice(fromIndex, 1);
      newTabs.splice(toIndex, 0, moved);
      return { tabs: newTabs };
    });
  },

  // ═══════════════════════════════════════════════════════════════════════════
  // Split Pane Actions
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Reset tab to single-pane mode, keeping only the active pane.
   * Closes all other pane sessions.
   */
  resetToSinglePane: (tabId) => {
    const tab = get().tabs.find(t => t.id === tabId);
    if (!tab || !tab.rootPane) return;  // already single pane

    // Find the active pane leaf
    const activeLeaf = tab.activePaneId
      ? findPaneById(tab.rootPane, tab.activePaneId)
      : findFirstLeaf(tab.rootPane);
    if (!activeLeaf) return;

    // Collect all other pane sessions to close
    const allSessions = collectAllPaneSessions(tab.rootPane);
    const keepSessionId = activeLeaf.sessionId;

    // Close other sessions asynchronously
    (async () => {
      for (const id of allSessions.localTerminalIds) {
        if (id === keepSessionId) continue;
        try {
          const { useLocalTerminalStore } = await import('./localTerminalStore');
          await useLocalTerminalStore.getState().closeTerminal(id);
        } catch (e) {
          console.warn(`[resetToSinglePane] Failed to close local terminal ${id}:`, e);
        }
      }
      for (const id of allSessions.sshTerminalIds) {
        if (id === keepSessionId) continue;
        try {
          const { useSessionTreeStore } = await import('./sessionTreeStore');
          useSessionTreeStore.getState().purgeTerminalMapping(id);
          useAppStore.setState((s) => {
            const newSessions = new Map(s.sessions);
            newSessions.delete(id);
            return { sessions: newSessions };
          });
          await api.closeTerminal(id);
        } catch (e) {
          console.warn(`[resetToSinglePane] Failed to close SSH terminal ${id}:`, e);
        }
      }
    })();

    // Immediately update the tab to single-pane mode
    set((state) => {
      const idx = state.tabs.findIndex(t => t.id === tabId);
      if (idx === -1) return state;
      const newTabs = [...state.tabs];
      newTabs[idx] = {
        ...state.tabs[idx],
        rootPane: undefined,
        activePaneId: activeLeaf.id,
        sessionId: activeLeaf.sessionId,
        type: activeLeaf.terminalType,
      };
      return { tabs: newTabs };
    });
  },

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
   * Replace old sessionId with new sessionId across ALL tab pane trees.
   * Used by reconnect orchestrator after creating a new terminal session,
   * so TerminalPane key-driven reset can remount with the correct session.
   */
  updatePaneSessionId: (oldSessionId, newSessionId) => {
    set((state) => {
      let changed = false;
      const newTabs = state.tabs.map(tab => {
        // Legacy single-pane mode
        if (tab.sessionId === oldSessionId) {
          changed = true;
          return { ...tab, sessionId: newSessionId };
        }

        // Split pane tree
        if (tab.rootPane) {
          const updatedRoot = replacePaneSessionId(tab.rootPane, oldSessionId, newSessionId);
          if (updatedRoot !== tab.rootPane) {
            changed = true;
            return { ...tab, rootPane: updatedRoot };
          }
        }

        return tab;
      });

      if (!changed) return state;
      console.log(`[AppStore] updatePaneSessionId: ${oldSessionId} → ${newSessionId}`);
      return { tabs: newTabs };
    });
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
      const paneToClose = tab.rootPane ? findPaneById(tab.rootPane, paneId) : null;
      if (paneToClose) {
        const closedSessionId = paneToClose.sessionId;
        const closedType = paneToClose.terminalType;
        (async () => {
          try {
            if (closedType === 'local_terminal') {
              const { useLocalTerminalStore } = await import('./localTerminalStore');
              await useLocalTerminalStore.getState().closeTerminal(closedSessionId);
            } else if (closedType === 'terminal') {
              // Fix: SSH terminals in split panes must also release backend resources.
              // Without this, SSH PTY channel, WsBridge, and connection ref_count leak.
              const { useSessionTreeStore } = await import('./sessionTreeStore');
              useSessionTreeStore.getState().purgeTerminalMapping(closedSessionId);

              // Remove from sessions Map and collect connectionId
              let connectionId: string | undefined;
              useAppStore.setState((s) => {
                const newSessions = new Map(s.sessions);
                const session = newSessions.get(closedSessionId);
                connectionId = session?.connectionId;
                newSessions.delete(closedSessionId);
                return {
                  sessions: newSessions,
                  connections: removeTerminalSessionsFromConnections(
                    s.connections,
                    s.sessions,
                    [closedSessionId],
                  ),
                };
              });

              // Close backend terminal (PTY + WsBridge + refs)
              try {
                await api.closeTerminal(closedSessionId);
                console.log(`[closePane] SSH terminal ${closedSessionId} closed`);
              } catch (e) {
                console.warn(`[closePane] Failed to close SSH terminal ${closedSessionId}:`, e);
              }

              // If no remaining terminals on this connection, disconnect SSH
              if (connectionId) {
                const remaining = Array.from(useAppStore.getState().sessions.values())
                  .filter(s => s.connectionId === connectionId);
                if (remaining.length === 0) {
                  try {
                    await api.sshDisconnect(connectionId);
                    useAppStore.setState((s) => {
                      return {
                        connections: removeConnectionsById(s.connections, [connectionId!]),
                      };
                    });
                    console.log(`[closePane] SSH connection ${connectionId} disconnected (no remaining terminals)`);
                  } catch (e) {
                    console.warn(`[closePane] Failed to disconnect SSH ${connectionId}:`, e);
                  }
                }
              }
            }
          } catch (error) {
            console.error('[closePane] Failed to close pane terminal:', error);
          }
        })();
      }
      
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
  
  toggleModal: (modal, isOpen, quickConnect) => {
    set((state) => ({
      modals: { ...state.modals, [modal]: isOpen },
      // Set or clear quickConnectData when opening/closing newConnection modal
      quickConnectData: modal === 'newConnection' ? (isOpen ? (quickConnect ?? null) : null) : state.quickConnectData,
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

  /**
   * 连接到保存的连接
   * 
   * Phase 3.4: 使用新的前端驱动架构
   * - 对于 proxy_chain: 使用 expandManualPreset + connectNodeWithAncestors
   * - 对于直连: 使用 addRootNode + connectNodeWithAncestors
   * 
   * 注意: 此方法保留向后兼容，但推荐直接使用 Sidebar 的 handleConnectSaved
   */
  connectToSaved: async (connectionId) => {
    try {
      // Get full connection info with credentials from backend
      const savedConn = await api.getSavedConnectionForConnect(connectionId);

      // Map auth_type for manual preset (no default_key in HopInfo)
      const mapPresetAuthType = (authType: string): 'password' | 'key' | 'agent' => {
        if (authType === 'agent') return 'agent';
        if (authType === 'key') return 'key';
        if (authType === 'password') return 'password';
        return 'key'; // default_key fallback to key
      };

      // 动态导入 sessionTreeStore
      const { useSessionTreeStore } = await import('./sessionTreeStore');
      const treeStore = useSessionTreeStore.getState();

      // ========== Phase 3.4: Proxy Chain 支持 ==========
      // 使用 expandManualPreset + connectNodeWithAncestors 实现前端驱动的线性连接
      if (savedConn.proxy_chain && savedConn.proxy_chain.length > 0) {
        const hops: ConnectPresetChainRequest['hops'] = savedConn.proxy_chain.map((hop) => ({
          host: hop.host,
          port: hop.port,
          username: hop.username,
          authType: mapPresetAuthType(hop.auth_type),
          password: hop.password,
          keyPath: hop.key_path,
          passphrase: hop.passphrase,
          agentForwarding: hop.agent_forwarding,
        }));

        const target: ConnectPresetChainRequest['target'] = {
          host: savedConn.host,
          port: savedConn.port,
          username: savedConn.username,
          authType: mapPresetAuthType(savedConn.auth_type),
          password: savedConn.password,
          keyPath: savedConn.key_path,
          passphrase: savedConn.passphrase,
          agentForwarding: savedConn.agent_forwarding,
        };

        const request: ConnectPresetChainRequest = {
          savedConnectionId: connectionId,
          hops,
          target,
        };

        // Step 1: 展开预设链为树节点（不建立连接）
        const expandResult = await treeStore.expandManualPreset(request);

        // Step 2: 使用线性连接器连接整条链路
        const connectedNodeIds = await treeStore.connectNodeWithAncestors(expandResult.targetNodeId);

        // Step 3: 注册拓扑映射
        for (const nodeId of connectedNodeIds) {
          const rawNode = treeStore.getRawNode(nodeId);
          if (rawNode?.sshConnectionId) {
            topologyResolver.register(rawNode.sshConnectionId, nodeId);
          }
        }

        // Step 4: 为目标节点创建终端并打开标签页
        const terminalId = await treeStore.createTerminalForNode(expandResult.targetNodeId);
        get().createTab('terminal', terminalId);

        useToastStore.getState().addToast({
          title: i18n.t('connections.toast.proxy_chain_established'),
          description: i18n.t('connections.toast.proxy_chain_desc', { depth: expandResult.chainDepth }),
          variant: 'success',
        });

        await api.markConnectionUsed(connectionId);
        return;
      }

      // ========== 直连（无 proxy_chain）==========
      // 检查是否已有相同主机的根节点
      const existingNode = treeStore.nodes.find((n) =>
        n.depth === 0 &&
        n.host === savedConn.host &&
        n.port === savedConn.port &&
        n.username === savedConn.username
      );

      let nodeId: string;

      if (existingNode) {
        // 已存在相同节点 - 直接使用
        nodeId = existingNode.id;
        treeStore.selectNode(nodeId);

        // 如果节点未连接，尝试连接
        if (existingNode.runtime.status === 'idle' || existingNode.runtime.status === 'error') {
          await treeStore.connectNodeWithAncestors(nodeId);
        }
      } else {
        // 创建新根节点
        const mapAuthType = (authType: string): 'password' | 'key' | 'agent' | undefined => {
          if (authType === 'agent') return 'agent';
          if (authType === 'key') return 'key';
          if (authType === 'password') return 'password';
          return undefined; // default_key
        };

        nodeId = await treeStore.addRootNode({
          host: savedConn.host,
          port: savedConn.port,
          username: savedConn.username,
          authType: mapAuthType(savedConn.auth_type),
          password: savedConn.password,
          keyPath: savedConn.key_path,
          passphrase: savedConn.passphrase,
          displayName: savedConn.name,
        });

        // 自动连接新创建的节点
        await treeStore.connectNodeWithAncestors(nodeId);
      }

      await api.markConnectionUsed(connectionId);
    } catch (error) {
      console.error('Failed to connect to saved connection:', error);
      // 只有真正的连接错误才打开编辑器，不包括锁错误
      const errorMsg = String(error);
      if (!errorMsg.includes('already connecting') && 
          !errorMsg.includes('already connected') &&
          !errorMsg.includes('CHAIN_LOCK_BUSY') &&
          !errorMsg.includes('NODE_LOCK_BUSY')) {
        get().openConnectionEditor(connectionId);
      }
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
  },

  updateConnectionRemoteEnv: (connectionId, remoteEnv) => {
    set((prev) => {
      const connection = prev.connections.get(connectionId);
      if (!connection) {
        console.warn(`[Store] updateConnectionRemoteEnv: connection not found: ${connectionId}`);
        return prev;
      }

      const newConnections = new Map(prev.connections);
      newConnections.set(connectionId, {
        ...connection,
        remoteEnv,
      });

      console.log(`[Store] Connection ${connectionId} remoteEnv updated:`, remoteEnv.osType);
      return { connections: newConnections };
    });
  }
}));

export function paneTreeHasAnySession(
  node: PaneNode,
  sessionIds: ReadonlySet<string>
): boolean {
  if (sessionIds.size === 0) {
    return false;
  }

  if (node.type === 'leaf') {
    return sessionIds.has(node.sessionId);
  }

  return node.children.some((child) => paneTreeHasAnySession(child, sessionIds));
}

export function tabReferencesAnySession(
  tab: Tab,
  sessionIds: ReadonlySet<string>
): boolean {
  if (sessionIds.size === 0) {
    return false;
  }

  if (tab.sessionId && sessionIds.has(tab.sessionId)) {
    return true;
  }

  return !!tab.rootPane && paneTreeHasAnySession(tab.rootPane, sessionIds);
}

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
 * Recursively replace sessionId inside a pane tree.
 * Returns the same node reference if no replacement was made (structural sharing).
 */
function replacePaneSessionId(
  node: PaneNode,
  oldSessionId: string,
  newSessionId: string,
): PaneNode {
  if (node.type === 'leaf') {
    if (node.sessionId === oldSessionId) {
      return { ...node, sessionId: newSessionId };
    }
    return node;
  }
  // Group node — recurse into children
  const newChildren = node.children.map(child =>
    replacePaneSessionId(child, oldSessionId, newSessionId),
  );
  // Only create new object if something changed (structural sharing)
  if (newChildren.every((c, i) => c === node.children[i])) {
    return node;
  }
  return { ...node, children: newChildren };
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
 * Collect all session IDs from a pane tree, grouped by terminal type
 * Used for recursive cleanup when closing a tab with split panes
 */
export function collectAllPaneSessions(node: PaneNode): {
  localTerminalIds: string[];
  sshTerminalIds: string[];
} {
  if (node.type === 'leaf') {
    if (node.terminalType === 'local_terminal') {
      return { localTerminalIds: [node.sessionId], sshTerminalIds: [] };
    } else {
      return { localTerminalIds: [], sshTerminalIds: [node.sessionId] };
    }
  }
  
  const result = { localTerminalIds: [] as string[], sshTerminalIds: [] as string[] };
  for (const child of node.children) {
    const childResult = collectAllPaneSessions(child);
    result.localTerminalIds.push(...childResult.localTerminalIds);
    result.sshTerminalIds.push(...childResult.sshTerminalIds);
  }
  return result;
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
