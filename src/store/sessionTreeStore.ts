/**
 * Session Tree Store (Unified)
 * 
 * Single Source of Truth for all session state
 * 
 * 设计原则:
 * 1. sessionTreeStore 是唯一事实来源，驱动所有 UI 渲染
 * 2. appStore.connections 只作为底层句柄池缓存
 * 3. 状态映射: NodeState = f(ConnectionStatus, TerminalSessionCount)
 */

import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import { api } from '../lib/api';
import { guardSessionConnection, isConnectionGuardError } from '../lib/connectionGuard';
import { topologyResolver } from '../lib/topologyResolver';
import { useSettingsStore } from './settingsStore';
import type { 
  FlatNode, 
  SessionTreeSummary,
  ConnectServerRequest,
  DrillDownRequest,
  ConnectPresetChainRequest,
  UnifiedFlatNode,
  UnifiedNodeStatus,
  NodeRuntimeState,
  TreeNodeState,
} from '../types';

// ============================================================================
// Types
// ============================================================================

/** 重连进度信息 */
export interface ReconnectProgress {
  attempt: number;
  maxAttempts: number | null;
  nextRetryMs?: number;
}

/** 状态漂移报告 */
export interface StateDriftReport {
  /** 检测到漂移的节点数 */
  driftCount: number;
  /** 修复的节点详情 */
  fixed: Array<{
    nodeId: string;
    field: string;
    localValue: unknown;
    backendValue: unknown;
  }>;
  /** 同步耗时 (ms) */
  syncDuration: number;
  /** 同步时间戳 */
  timestamp: number;
}

// 周期性同步定时器
let syncIntervalId: ReturnType<typeof setInterval> | null = null;

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * 计算统一节点状态
 * NodeState = f(ConnectionStatus, TerminalSessionCount)
 */
function computeUnifiedStatus(
  backendState: TreeNodeState,
  terminalCount: number,
  isLinkDown: boolean
): UnifiedNodeStatus {
  // 优先级: link-down > error > connected/active > connecting > idle
  if (isLinkDown) {
    return 'link-down';
  }
  
  switch (backendState.status) {
    case 'connecting':
      return 'connecting';
    case 'connected':
      return terminalCount > 0 ? 'active' : 'connected';
    case 'failed':
      return 'error';
    case 'disconnected':
    case 'pending':
    default:
      return 'idle';
  }
}

// ============================================================================
// Types
// ============================================================================

interface SessionTreeStore {
  // ========== State ==========
  /** 后端原始节点数据 */
  rawNodes: FlatNode[];
  /** 统一节点数据 (Single Source of Truth) */
  nodes: UnifiedFlatNode[];
  /** 当前选中的节点 ID */
  selectedNodeId: string | null;
  /** 加载状态 */
  isLoading: boolean;
  /** 错误信息 */
  error: string | null;
  /** 树摘要 */
  summary: SessionTreeSummary | null;
  
  // NOTE: expandedIds 和 focusedNodeId 现在从 settingsStore.treeUI 获取
  // 使用 getExpandedIds() 和 getFocusedNodeId() getter 访问
  
  /** 节点终端映射 (nodeId -> terminalIds) - 支持多终端 */
  nodeTerminalMap: Map<string, string[]>;
  /** 终端到节点的反向映射 (terminalId -> nodeId) */
  terminalNodeMap: Map<string, string>;
  /** 链路断开的节点 ID 集合 */
  linkDownNodeIds: Set<string>;
  /** 重连进度 (nodeId -> ReconnectProgress) */
  reconnectProgress: Map<string, ReconnectProgress>;
  /** 断开前的终端数量 (nodeId -> count) - 用于重连时恢复终端 */
  disconnectedTerminalCounts: Map<string, number>;
  
  // ========== Concurrency Lock (并发锁) ==========
  /** 正在连接的节点 ID 集合（节点级锁） */
  connectingNodeIds: Set<string>;
  /** 全局连接锁（防止多条链同时执行） */
  isConnectingChain: boolean;
  
  // ========== Data Actions ==========
  fetchTree: () => Promise<void>;
  fetchSummary: () => Promise<void>;
  
  // ========== Node Operations ==========
  addRootNode: (request: ConnectServerRequest) => Promise<string>;
  drillDown: (request: DrillDownRequest) => Promise<string>;
  /** 展开手工预设链，返回目标节点ID和路径（Phase 2.2: 只展开不连接） */
  expandManualPreset: (request: ConnectPresetChainRequest) => Promise<{ targetNodeId: string; pathNodeIds: string[]; chainDepth: number }>;
  expandAutoRoute: (request: import('../types').ExpandAutoRouteRequest) => Promise<import('../types').ExpandAutoRouteResponse>;
  removeNode: (nodeId: string) => Promise<string[]>;
  clearTree: () => Promise<void>;
  
  // ========== Connection Management ==========
  /** 连接节点 (建立 SSH 连接) */
  connectNode: (nodeId: string) => Promise<void>;
  /** 断开节点 (级联断开所有子节点) */
  disconnectNode: (nodeId: string) => Promise<void>;
  /** 级联重连节点及其之前已连接的子节点 */
  reconnectCascade: (nodeId: string, options?: { skipChildren?: boolean }) => Promise<string[]>;
  /** 
   * 重置节点状态（焦土式清理）
   * 
   * 用于连接前确保节点状态干净，包括：
   * - 关闭现有终端
   * - 清理本地映射
   * - 重置状态为 pending
   */
  resetNodeState: (nodeId: string) => Promise<void>;
  
  // ========== Terminal Management (新增) ==========
  /** 为节点创建新终端 */
  createTerminalForNode: (nodeId: string, cols?: number, rows?: number) => Promise<string>;
  /** 关闭节点的指定终端 */
  closeTerminalForNode: (nodeId: string, terminalId: string) => Promise<void>;
  /** 本地清理终端映射（不调用后端） */
  purgeTerminalMapping: (terminalId: string) => void;
  /** 获取节点的所有终端 */
  getTerminalsForNode: (nodeId: string) => string[];
  /** 通过终端 ID 查找所属节点 */
  getNodeByTerminalId: (terminalId: string) => UnifiedFlatNode | undefined;
  /** 添加 KBI (2FA) 认证后的会话 (隔离流程) */
  addKbiSession: (params: {
    sessionId: string;
    wsPort: number;
    wsToken: string;
    host: string;
    port: number;
    username: string;
    displayName: string;
  }) => Promise<void>;
  
  // ========== SFTP Management ==========
  /** 打开节点的 SFTP 会话 */
  openSftpForNode: (nodeId: string) => Promise<string | null>;
  /** 关闭节点的 SFTP 会话 */
  closeSftpForNode: (nodeId: string) => Promise<void>;
  
  // ========== State Sync ==========
  /** 更新节点状态 (来自后端事件) */
  updateNodeState: (nodeId: string, state: string, error?: string) => Promise<void>;
  /** 设置节点连接 ID */
  setNodeConnection: (nodeId: string, connectionId: string) => Promise<void>;
  /** 设置节点终端 (向后端同步) */
  setNodeTerminal: (nodeId: string, sessionId: string) => Promise<void>;
  /** 设置节点 SFTP (向后端同步) */
  setNodeSftp: (nodeId: string, sessionId: string) => Promise<void>;
  /** 标记节点为 link-down (级联) */
  /** 标记节点为 link-down (级联) */
  markLinkDown: (nodeId: string) => void;
  /** 批量标记节点为 link-down */
  markLinkDownBatch: (nodeIds: string[]) => void;
  /** 清除 link-down 标记 */
  clearLinkDown: (nodeId: string) => void;
  /** 设置重连进度 */
  setReconnectProgress: (nodeId: string, progress: ReconnectProgress | null) => void;
  
  // ========== Concurrency Lock Methods (并发锁方法) ==========
  /** 尝试获取节点连接锁 */
  acquireConnectLock: (nodeId: string) => boolean;
  /** 释放节点连接锁 */
  releaseConnectLock: (nodeId: string) => void;
  /** 尝试获取链式连接锁（全局唯一） */
  acquireChainLock: () => boolean;
  /** 释放链式连接锁 */
  releaseChainLock: () => void;
  /** 检查节点是否正在连接中 */
  isNodeConnecting: (nodeId: string) => boolean;
  
  // ========== State Drift Detection ==========
  /** 从后端同步状态并修复漂移 */
  syncFromBackend: () => Promise<StateDriftReport>;
  /** 启动周期性同步（默认 30s） */
  startPeriodicSync: (intervalMs?: number) => void;
  /** 停止周期性同步 */
  stopPeriodicSync: () => void;
  
  // ========== UI Actions ==========
  selectNode: (nodeId: string | null) => void;
  toggleExpand: (nodeId: string) => void;
  expandAll: () => void;
  collapseAll: () => void;
  
  // ========== Focus Mode Actions (聚焦模式) ==========
  /** 设置聚焦节点（进入/返回某层） */
  setFocusedNode: (nodeId: string | null) => void;
  /** 获取面包屑路径 */
  getBreadcrumbPath: () => UnifiedFlatNode[];
  /** 获取当前视图可见的节点 */
  getVisibleNodes: () => UnifiedFlatNode[];
  /** 进入子节点（双击进入） */
  enterNode: (nodeId: string) => void;
  /** 返回上一层 */
  goBack: () => void;
  
  // ========== Helpers ==========
  getNode: (nodeId: string) => UnifiedFlatNode | undefined;
  getRawNode: (nodeId: string) => FlatNode | undefined;
  getNodePath: (nodeId: string) => Promise<FlatNode[]>;
  getDescendants: (nodeId: string) => UnifiedFlatNode[];
  /** 重建统一节点列表 */
  rebuildUnifiedNodes: () => void;
  
  // ========== Settings Store Proxies ==========
  /** 获取展开的节点 ID 集合（从 settingsStore 读取） */
  getExpandedIds: () => Set<string>;
  /** 获取聚焦节点 ID（从 settingsStore 读取） */
  getFocusedNodeId: () => string | null;
}

// ============================================================================
// Orphan ID Pruning
// ============================================================================

/**
 * 清理 settingsStore.treeUI 中不再有效的节点 ID
 * 
 * 调用时机：rawNodes 更新后
 * 清理逻辑：
 *   - expandedIds: 移除所有不在 rawNodes 中的 ID
 *   - focusedNodeId: 如果不在 rawNodes 中，置为 null
 */
function pruneOrphanedTreeUIState(currentNodes: FlatNode[]): void {
  // 空节点列表时不清理，避免启动时误清
  if (currentNodes.length === 0) {
    return;
  }
  
  const settingsStore = useSettingsStore.getState();
  const { expandedIds, focusedNodeId } = settingsStore.settings.treeUI;
  
  // 构建当前有效 ID 集合
  const validIds = new Set(currentNodes.map(node => node.id));
  
  // 过滤 expandedIds
  const prunedExpandedIds = expandedIds.filter(id => validIds.has(id));
  const expandedChanged = prunedExpandedIds.length !== expandedIds.length;
  
  // 检查 focusedNodeId
  const focusedValid = focusedNodeId === null || validIds.has(focusedNodeId);
  
  // 仅在有变化时更新（避免无意义的 localStorage 写入）
  if (expandedChanged || !focusedValid) {
    console.debug(
      '[SessionTree] Pruning orphaned IDs:',
      expandedChanged ? `expandedIds: ${expandedIds.length} -> ${prunedExpandedIds.length}` : '',
      !focusedValid ? `focusedNodeId: ${focusedNodeId} -> null` : ''
    );
    
    // 批量更新 settingsStore
    if (expandedChanged) {
      settingsStore.setTreeExpanded(prunedExpandedIds);
    }
    if (!focusedValid) {
      settingsStore.setFocusedNode(null);
    }
  }
}

// ============================================================================
// Store Implementation
// ============================================================================

export const useSessionTreeStore = create<SessionTreeStore>()(
  subscribeWithSelector((set, get) => ({
    // ========== Initial State ==========
    rawNodes: [],
    nodes: [],
    selectedNodeId: null,
    isLoading: false,
    error: null,
    summary: null,
    // NOTE: expandedIds 和 focusedNodeId 现在从 settingsStore 获取
    nodeTerminalMap: new Map<string, string[]>(),
    terminalNodeMap: new Map<string, string>(),
    linkDownNodeIds: new Set<string>(),
    reconnectProgress: new Map<string, ReconnectProgress>(),
    disconnectedTerminalCounts: new Map<string, number>(),
    
    // ========== Concurrency Lock Initial State ==========
    connectingNodeIds: new Set<string>(),
    isConnectingChain: false,
    
    // ========== Data Actions ==========
    
    fetchTree: async () => {
      set({ isLoading: true, error: null });
      try {
        const rawNodes = await api.getSessionTree();
        
        // 获取当前 expandedIds（从 settingsStore）
        const settingsStore = useSettingsStore.getState();
        const currentExpandedIds = settingsStore.settings.treeUI.expandedIds;
        
        // 如果当前没有展开的节点，默认展开所有有子节点的节点
        if (currentExpandedIds.length === 0 && rawNodes.length > 0) {
          const defaultExpanded = rawNodes.filter(n => n.hasChildren).map(n => n.id);
          settingsStore.setTreeExpanded(defaultExpanded);
        }
        
        set({ rawNodes, isLoading: false });
        
        // 清理孤儿 ID（移除不存在的 expandedIds/focusedNodeId）
        pruneOrphanedTreeUIState(rawNodes);
        
        get().rebuildUnifiedNodes();
      } catch (e) {
        set({ error: String(e), isLoading: false });
      }
    },
    
    fetchSummary: async () => {
      try {
        const summary = await api.getSessionTreeSummary();
        set({ summary });
      } catch (e) {
        console.error('Failed to fetch session tree summary:', e);
      }
    },
    
    // ========== Node Operations ==========
    
    addRootNode: async (request: ConnectServerRequest) => {
      set({ isLoading: true, error: null });
      try {
        const nodeId = await api.addRootNode(request);
        await get().fetchTree();
        set({ selectedNodeId: nodeId, isLoading: false });
        return nodeId;
      } catch (e) {
        set({ error: String(e), isLoading: false });
        throw e;
      }
    },
    
    drillDown: async (request: DrillDownRequest) => {
      // 前置校验：检查父节点状态
      const parentNode = get().getNode(request.parentNodeId);
      if (!parentNode) {
        throw new Error(`Parent node ${request.parentNodeId} not found`);
      }
      if (parentNode.runtime.status === 'link-down') {
        throw new Error('Cannot drill down from a link-down node');
      }
      if (parentNode.runtime.status !== 'connected') {
        throw new Error(`Parent node is not connected (status: ${parentNode.runtime.status})`);
      }
      
      set({ isLoading: true, error: null });
      try {
        const nodeId = await api.treeDrillDown(request);
        await get().fetchTree();
        // 展开父节点（通过 settingsStore）
        const settingsStore = useSettingsStore.getState();
        const currentExpanded = settingsStore.settings.treeUI.expandedIds;
        if (!currentExpanded.includes(request.parentNodeId)) {
          settingsStore.setTreeExpanded([...currentExpanded, request.parentNodeId]);
        }
        set({ selectedNodeId: nodeId, isLoading: false });
        return nodeId;
      } catch (e) {
        set({ error: String(e), isLoading: false });
        throw e;
      }
    },
    
    expandManualPreset: async (request: ConnectPresetChainRequest) => {
      set({ isLoading: true, error: null });
      try {
        const response = await api.expandManualPreset(request);
        await get().fetchTree();
        set({ selectedNodeId: response.targetNodeId, isLoading: false });
        return response;
      } catch (e) {
        set({ error: String(e), isLoading: false });
        throw e;
      }
    },
    
    expandAutoRoute: async (request) => {
      set({ isLoading: true, error: null });
      try {
        const result = await api.expandAutoRoute(request);
        await get().fetchTree();
        set({ selectedNodeId: result.targetNodeId, isLoading: false });
        return result;
      } catch (e) {
        set({ error: String(e), isLoading: false });
        throw e;
      }
    },
    
    removeNode: async (nodeId: string) => {
      set({ isLoading: true, error: null });
      try {
        // 清理该节点和所有子节点的终端映射
        const descendants = get().getDescendants(nodeId);
        const currentNode = get().getNode(nodeId);
        const nodesToRemove = currentNode ? [currentNode, ...descendants] : descendants;
        
        // 在调用 API 前记录本地计算的待删除 ID（用于后续清理 selectedNodeId）
        const localRemovedIds = nodesToRemove.map(n => n.id);
        
        const { nodeTerminalMap, terminalNodeMap } = get();
        const newTerminalMap = new Map(nodeTerminalMap);
        const newNodeMap = new Map(terminalNodeMap);
        
        // 收集所有需要关闭的终端 ID
        const terminalIdsToClose: string[] = [];
        
        for (const node of nodesToRemove) {
          const terminals = newTerminalMap.get(node.id) || [];
          for (const termId of terminals) {
            terminalIdsToClose.push(termId);
            newNodeMap.delete(termId);
          }
          newTerminalMap.delete(node.id);
        }
        
        set({ nodeTerminalMap: newTerminalMap, terminalNodeMap: newNodeMap });
        
        // 关闭关联的 Tab（异步导入 appStore 避免循环依赖）
        if (terminalIdsToClose.length > 0) {
          const { useAppStore } = await import('./appStore');
          const appState = useAppStore.getState();
          for (const termId of terminalIdsToClose) {
            const tab = appState.tabs.find(t => t.sessionId === termId);
            if (tab) {
              appState.closeTab(tab.id);
            }
          }
        }
        
        // 使用本地计算的 ID 清理 selectedNodeId（在 API 调用前，避免后端返回不完整）
        const { selectedNodeId } = get();
        if (selectedNodeId && localRemovedIds.includes(selectedNodeId)) {
          set({ selectedNodeId: null });
        }
        
        // 🔴 清理拓扑映射（在 API 调用前）
        for (const node of nodesToRemove) {
          topologyResolver.unregister(node.id);
        }
        
        const removedIds = await api.removeTreeNode(nodeId);
        await get().fetchTree();
        
        set({ isLoading: false });
        return removedIds;
      } catch (e) {
        set({ error: String(e), isLoading: false });
        throw e;
      }
    },
    
    clearTree: async () => {
      set({ isLoading: true, error: null });
      try {
        await api.clearSessionTree();
        // 清空 settingsStore 中的树状态
        useSettingsStore.getState().setTreeExpanded([]);
        useSettingsStore.getState().setFocusedNode(null);
        set({ 
          rawNodes: [],
          nodes: [], 
          selectedNodeId: null, 
          nodeTerminalMap: new Map(),
          terminalNodeMap: new Map(),
          linkDownNodeIds: new Set(),
          isLoading: false 
        });
      } catch (e) {
        set({ error: String(e), isLoading: false });
        throw e;
      }
    },
    
    // ========== Connection Management ==========
    
    /**
     * 连接节点（建立 SSH 连接）
     * 
     * 包含并发锁机制：
     * 1. 获取节点锁，防止重复连接
     * 2. 执行连接
     * 3. finally 释放锁
     * 
     * 异常处理：
     * - 锁获取失败：静默返回，不抛异常
     * - 连接失败：回滚状态，释放锁，抛出异常
     */
    connectNode: async (nodeId: string) => {
      const node = get().getRawNode(nodeId);
      if (!node) throw new Error(`Node ${nodeId} not found`);
      
      // ========== 并发锁检查 ==========
      
      // 检查节点是否已在连接中（通过锁）
      if (get().isNodeConnecting(nodeId)) {
        console.log(`[connectNode] Node ${nodeId} is already connecting (locked), skipping`);
        return;
      }
      
      // 检查前端状态，避免重复连接（双重检查）
      if (node.state.status === 'connecting' || node.state.status === 'connected') {
        console.log(`[connectNode] Node ${nodeId} is already ${node.state.status}, skipping`);
        return;
      }
      
      // 尝试获取锁
      if (!get().acquireConnectLock(nodeId)) {
        console.warn(`[connectNode] Failed to acquire lock for node ${nodeId}`);
        return;
      }
      
      console.log(`[connectNode] Starting connection for node ${nodeId}`);
      
      try {
        // 乐观更新：立即在本地设置为 connecting
        set((state) => ({
          rawNodes: state.rawNodes.map(n => 
            n.id === nodeId 
              ? { ...n, state: { ...n.state, status: 'connecting' as const } }
              : n
          )
        }));
        get().rebuildUnifiedNodes();
        
        const response = await api.connectTreeNode({ nodeId });
        
        // 更新连接 ID
        await api.setTreeNodeConnection(nodeId, response.sshConnectionId);
        
        // 🔴 注册连接映射 (connectionId -> nodeId)
        topologyResolver.register(response.sshConnectionId, nodeId);
        
        // 连接成功后，清除该节点及其所有子节点的 link-down 标记
        // 因为父节点已恢复连接，子节点现在可以尝试连接了
        const descendants = get().getDescendants(nodeId);
        const allAffectedNodes = [node, ...descendants];
        const { linkDownNodeIds } = get();
        const newLinkDownIds = new Set(linkDownNodeIds);
        for (const n of allAffectedNodes) {
          newLinkDownIds.delete(n.id);
        }
        set({ linkDownNodeIds: newLinkDownIds });
        
        await get().fetchTree();
        
        console.log(`[connectNode] Node ${nodeId} connected successfully`);
      } catch (e) {
        // 失败时回滚到 failed 状态
        console.error(`[connectNode] Node ${nodeId} connection failed:`, e);
        try {
          await api.updateTreeNodeState(nodeId, 'failed', String(e));
        } catch (updateErr) {
          console.warn(`[connectNode] Failed to update node state to failed:`, updateErr);
        }
        await get().fetchTree();
        throw e;
      } finally {
        // ========== 始终释放锁 ==========
        get().releaseConnectLock(nodeId);
      }
    },
    
    disconnectNode: async (nodeId: string) => {
      const node = get().getNode(nodeId);
      if (!node) return;
      
      // 1. 获取所有子节点 (包括当前节点)
      const descendants = get().getDescendants(nodeId);
      const allAffectedNodes = [node, ...descendants];
      
      // 2. 保存断开前的终端数量（用于重连时恢复）
      const { disconnectedTerminalCounts } = get();
      const newDisconnectedCounts = new Map(disconnectedTerminalCounts);
      for (const n of allAffectedNodes) {
        const terminalCount = n.runtime.terminalIds?.length || 0;
        if (terminalCount > 0) {
          newDisconnectedCounts.set(n.id, terminalCount);
        }
      }
      set({ disconnectedTerminalCounts: newDisconnectedCounts });
      
      // 3. 收集所有需要关闭的 Tab sessionId
      const sessionIdsToClose: string[] = [];
      for (const n of allAffectedNodes) {
        // 收集终端 ID
        if (n.runtime.terminalIds) {
          sessionIdsToClose.push(...n.runtime.terminalIds);
        }
        // 收集 SFTP 会话 ID
        if (n.runtime.sftpSessionId) {
          sessionIdsToClose.push(n.runtime.sftpSessionId);
        }
      }
      
      // 4. 关闭 appStore 中的相关 Tab
      if (sessionIdsToClose.length > 0) {
        const { useAppStore } = await import('./appStore');
        const appStore = useAppStore.getState();
        const sessionIdSet = new Set(sessionIdsToClose);
        for (const tab of appStore.tabs) {
          if (tab.sessionId && sessionIdSet.has(tab.sessionId)) {
            appStore.closeTab(tab.id);
          }
        }
      }
      
      // 5. 标记所有子节点为 link-down（表示链路断开，需要父节点先恢复才能连接）
      // 注意：不标记父节点本身，只标记子节点
      const { linkDownNodeIds } = get();
      const newLinkDownIds = new Set(linkDownNodeIds);
      for (const child of descendants) {
        newLinkDownIds.add(child.id);
      }
      set({ linkDownNodeIds: newLinkDownIds });
      
      // 6. 清理拓扑映射
      for (const n of allAffectedNodes) {
        topologyResolver.unregister(n.id);
      }
      
      // 7. 调用后端断开节点（会递归断开子节点并更新状态）
      try {
        await api.disconnectTreeNode(nodeId);
      } catch (e) {
        console.error('Failed to disconnect tree node:', e);
      }
      
      // 8. 刷新树状态
      await get().fetchTree();
    },
    
    /**
     * 级联重连节点及其之前已连接的子节点
     * 
     * @param nodeId 要重连的节点 ID
     * @param options 配置选项
     * @returns 成功重连的节点 ID 列表
     */
    reconnectCascade: async (nodeId: string, options?: { skipChildren?: boolean }) => {
      const node = get().getNode(nodeId);
      if (!node) throw new Error(`Node ${nodeId} not found`);
      
      const reconnected: string[] = [];
      
      // 1. 首先重连目标节点本身
      try {
        await get().connectNode(nodeId);
        reconnected.push(nodeId);
      } catch (e) {
        console.error(`Failed to reconnect node ${nodeId}:`, e);
        throw e; // 父节点重连失败，不继续重连子节点
      }
      
      // 2. 如果不跳过子节点，且有 link-down 的子节点，尝试重连它们
      if (!options?.skipChildren) {
        const descendants = get().getDescendants(nodeId);
        const { linkDownNodeIds } = get();
        
        // 按深度排序，确保从上到下依次重连
        const sortedDescendants = [...descendants].sort((a, b) => a.depth - b.depth);
        
        for (const child of sortedDescendants) {
          // 只重连之前标记为 link-down 的子节点
          if (linkDownNodeIds.has(child.id)) {
            // 检查父节点是否已连接（确保链路畅通）
            const parent = get().getNode(child.parentId!);
            if (parent?.runtime.status !== 'connected' && parent?.runtime.status !== 'active') {
              // 父节点未连接，跳过此子节点
              continue;
            }
            
            try {
              await get().connectNode(child.id);
              reconnected.push(child.id);
              // 短暂延迟，避免同时发起太多连接
              await new Promise(resolve => setTimeout(resolve, 100));
            } catch (e) {
              console.warn(`Failed to reconnect child node ${child.id}:`, e);
              // 子节点重连失败不中断流程，继续尝试其他节点
            }
          }
        }
      }
      
      // 3. 刷新树状态
      await get().fetchTree();
      
      return reconnected;
    },
    
    /**
     * 重置节点状态（焦土式清理）
     * 
     * 执行顺序：
     * 1. 关闭该节点的所有终端（调用后端）
     * 2. 清理本地映射
     * 3. 重置节点状态为 pending
     * 
     * 异常处理：
     * - 后端调用失败时记录警告但不中断流程
     * - 确保本地状态一定被清理（即使后端失败）
     */
    resetNodeState: async (nodeId: string): Promise<void> => {
      const node = get().getRawNode(nodeId);
      if (!node) {
        console.warn(`[resetNodeState] Node ${nodeId} not found`);
        return;
      }
      
      console.log(`[resetNodeState] Resetting node ${nodeId}`);
      
      // ========== Phase 1: 后端物理销毁 ==========
      
      // 1a. 关闭该节点的所有终端
      const terminalIds = get().nodeTerminalMap.get(nodeId) || [];
      
      // 也检查后端记录的 terminalSessionId
      if (node.terminalSessionId && !terminalIds.includes(node.terminalSessionId)) {
        terminalIds.push(node.terminalSessionId);
      }
      
      for (const terminalId of terminalIds) {
        try {
          await api.closeTerminal(terminalId);
          console.debug(`[resetNodeState] Closed terminal ${terminalId}`);
        } catch (e) {
          // 终端可能已不存在，忽略错误
          console.warn(`[resetNodeState] Failed to close terminal ${terminalId}:`, e);
        }
      }
      
      // 1b. 关闭 SFTP 会话（如果有）
      if (node.sftpSessionId) {
        try {
          // 使用第一个终端 ID 来关闭 SFTP（SFTP 依赖终端会话）
          const anyTerminalId = terminalIds[0];
          if (anyTerminalId) {
            await api.sftpClose(anyTerminalId);
            console.debug(`[resetNodeState] Closed SFTP for node ${nodeId}`);
          }
        } catch (e) {
          console.warn(`[resetNodeState] Failed to close SFTP:`, e);
        }
      }
      
      // 1c. 短暂等待确保后端资源释放
      await new Promise(resolve => setTimeout(resolve, 50));
      
      // ========== Phase 2: 清理 appStore sessions ==========
      
      try {
        const { useAppStore } = await import('./appStore');
        useAppStore.setState((state) => {
          const newSessions = new Map(state.sessions);
          for (const terminalId of terminalIds) {
            newSessions.delete(terminalId);
          }
          return { sessions: newSessions };
        });
      } catch (e) {
        console.warn(`[resetNodeState] Failed to clear appStore sessions:`, e);
      }
      
      // ========== Phase 3: 清理本地映射 ==========
      
      const { nodeTerminalMap, terminalNodeMap } = get();
      const newTerminalMap = new Map(nodeTerminalMap);
      const newNodeMap = new Map(terminalNodeMap);
      
      // 清理该节点的所有终端映射
      const existingTerminals = newTerminalMap.get(nodeId) || [];
      newTerminalMap.delete(nodeId);
      for (const tid of existingTerminals) {
        newNodeMap.delete(tid);
      }
      // 也清理后端记录的 terminalSessionId
      if (node.terminalSessionId) {
        newNodeMap.delete(node.terminalSessionId);
      }
      
      set({ 
        nodeTerminalMap: newTerminalMap, 
        terminalNodeMap: newNodeMap 
      });
      
      // ========== Phase 4: 重置节点状态为 pending ==========
      
      set((state) => ({
        rawNodes: state.rawNodes.map(n => 
          n.id === nodeId 
            ? { 
                ...n, 
                state: { status: 'pending' as const },
                sshConnectionId: null,
                terminalSessionId: null,
                sftpSessionId: null,
              }
            : n
        )
      }));
      
      // ========== Phase 5: 清除 link-down 标记 ==========
      
      const { linkDownNodeIds } = get();
      if (linkDownNodeIds.has(nodeId)) {
        const newLinkDownIds = new Set(linkDownNodeIds);
        newLinkDownIds.delete(nodeId);
        set({ linkDownNodeIds: newLinkDownIds });
      }
      
      // ========== Phase 6: 清除重连进度 ==========
      
      const { reconnectProgress } = get();
      if (reconnectProgress.has(nodeId)) {
        const newProgress = new Map(reconnectProgress);
        newProgress.delete(nodeId);
        set({ reconnectProgress: newProgress });
      }
      
      // 重建统一节点
      get().rebuildUnifiedNodes();
      
      console.log(`[resetNodeState] Node ${nodeId} reset complete`);
    },
    
    // ========== Terminal Management ==========
    
    createTerminalForNode: async (nodeId: string, cols?: number, rows?: number) => {
      const node = get().getNode(nodeId);
      if (!node) throw new Error(`Node ${nodeId} not found`);
      if (!node.runtime.connectionId) {
        throw new Error(`Node ${nodeId} is not connected`);
      }
      
      // 调用 API 创建终端
      const response = await api.createTerminal({
        connectionId: node.runtime.connectionId,
        cols,
        rows,
      });
      const terminalId = response.sessionId;
      
      // 同步到 appStore.sessions（用于 createTab 兼容）
      const { useAppStore } = await import('./appStore');
      useAppStore.setState((state) => {
        const newSessions = new Map(state.sessions);
        newSessions.set(terminalId, response.session);
        return { sessions: newSessions };
      });
      
      // 获取当前映射状态（用于可能的回滚）
      const { nodeTerminalMap, terminalNodeMap } = get();
      const existing = nodeTerminalMap.get(nodeId) || [];
      
      // 通知后端更新节点终端 (使用第一个终端作为主终端)
      // 先调用后端 API，成功后再更新本地映射
      try {
        if (existing.length === 0) {
          await api.setTreeNodeTerminal(nodeId, terminalId);
        }
      } catch (e) {
        // 后端 API 失败，回滚：关闭刚创建的终端和 session
        console.error('Failed to set tree node terminal, rolling back:', e);
        try {
          await api.closeTerminal(terminalId);
          useAppStore.setState((state) => {
            const newSessions = new Map(state.sessions);
            newSessions.delete(terminalId);
            return { sessions: newSessions };
          });
        } catch (rollbackError) {
          console.error('Rollback failed:', rollbackError);
        }
        throw e;
      }
      
      // 后端成功后，更新本地终端映射
      const newTerminalMap = new Map(nodeTerminalMap);
      const newNodeMap = new Map(terminalNodeMap);
      
      newTerminalMap.set(nodeId, [...existing, terminalId]);
      newNodeMap.set(terminalId, nodeId);
      
      set({ nodeTerminalMap: newTerminalMap, terminalNodeMap: newNodeMap });
      
      // 重建统一节点
      get().rebuildUnifiedNodes();
      
      return terminalId;
    },
    
    closeTerminalForNode: async (nodeId: string, terminalId: string) => {
      const { nodeTerminalMap, terminalNodeMap } = get();
      
      // 从映射中移除
      const newTerminalMap = new Map(nodeTerminalMap);
      const newNodeMap = new Map(terminalNodeMap);
      
      const existing = newTerminalMap.get(nodeId) || [];
      const filtered = existing.filter(id => id !== terminalId);
      
      if (filtered.length > 0) {
        newTerminalMap.set(nodeId, filtered);
      } else {
        newTerminalMap.delete(nodeId);
      }
      newNodeMap.delete(terminalId);
      
      set({ nodeTerminalMap: newTerminalMap, terminalNodeMap: newNodeMap });
      
      // 调用 API 关闭终端
      try {
        await api.closeTerminal(terminalId);
      } catch (e) {
        console.error('Failed to close terminal:', e);
      }
      
      // 重建统一节点
      get().rebuildUnifiedNodes();
    },

    purgeTerminalMapping: (terminalId: string) => {
      const { nodeTerminalMap, terminalNodeMap } = get();
      const nodeId = terminalNodeMap.get(terminalId);
      if (!nodeId) return;

      const newTerminalMap = new Map(nodeTerminalMap);
      const newNodeMap = new Map(terminalNodeMap);

      const existing = newTerminalMap.get(nodeId) || [];
      const filtered = existing.filter(id => id !== terminalId);
      if (filtered.length > 0) {
        newTerminalMap.set(nodeId, filtered);
      } else {
        newTerminalMap.delete(nodeId);
      }
      newNodeMap.delete(terminalId);

      set({ nodeTerminalMap: newTerminalMap, terminalNodeMap: newNodeMap });
      get().rebuildUnifiedNodes();
    },
    
    getTerminalsForNode: (nodeId: string) => {
      return get().nodeTerminalMap.get(nodeId) || [];
    },
    
    getNodeByTerminalId: (terminalId: string) => {
      const nodeId = get().terminalNodeMap.get(terminalId);
      if (!nodeId) return undefined;
      return get().getNode(nodeId);
    },

    /**
     * Add a KBI (2FA) session to the tree.
     * 
     * This is a special path for sessions created via the isolated ssh_connect_kbi flow.
     * Unlike regular connections, KBI sessions bypass addRootNode+connectNode because
     * the authentication is interactive and the session is already established by the time
     * we need to add it to the tree.
     */
    addKbiSession: async (params) => {
      const { sessionId, wsPort, wsToken, host, port, username, displayName } = params;
      
      console.log(`[SessionTree] Adding KBI session: ${sessionId} for ${displayName}`);
      
      try {
        // 1. Create a root node for this KBI session
        // We use a special request with keyboard_interactive auth type
        const nodeId = await api.addRootNode({
          displayName,
          host,
          port,
          username,
          authType: 'keyboard_interactive',
        });
        
        console.log(`[SessionTree] KBI root node created: ${nodeId}`);
        
        // 2. The session is already connected via KBI, so we need to update the node state
        // Set the terminal session (which was created during KBI flow)
        await api.setTreeNodeTerminal(nodeId, sessionId);
        
        // 3. Update appStore with the session info so TerminalView can connect
        // We directly update the sessions Map since there's no dedicated addSession method
        const { useAppStore } = await import('./appStore');
        const sessionInfo = {
          id: sessionId,
          host,
          port,
          username,
          name: displayName,
          state: 'connected' as const,
          ws_url: `ws://127.0.0.1:${wsPort}`,
          ws_token: wsToken,
          auth_type: 'keyboard_interactive' as const,
          color: '#4ade80', // Green for KBI sessions
          uptime_secs: 0,
          order: Date.now(), // Use timestamp for ordering
        };
        
        useAppStore.setState((state) => {
          const newSessions = new Map(state.sessions);
          newSessions.set(sessionId, sessionInfo);
          return { sessions: newSessions };
        });
        
        // Also create a tab for the terminal
        useAppStore.getState().createTab('terminal', sessionId);
        
        // 4. Update local state maps
        set((state) => ({
          nodeTerminalMap: new Map(state.nodeTerminalMap).set(nodeId, [
            ...(state.nodeTerminalMap.get(nodeId) || []),
            sessionId,
          ]),
          terminalNodeMap: new Map(state.terminalNodeMap).set(sessionId, nodeId),
        }));
        
        // 5. Refresh the tree from backend to get consistent state
        await get().fetchTree();
        
        console.log(`[SessionTree] KBI session ${sessionId} added to tree under node ${nodeId}`);
      } catch (error) {
        console.error(`[SessionTree] Failed to add KBI session:`, error);
        throw error;
      }
    },
    
    // ========== SFTP Management ==========
    
    openSftpForNode: async (nodeId: string) => {
      const node = get().getNode(nodeId);
      if (!node) throw new Error(`Node ${nodeId} not found`);
      if (!node.runtime.connectionId) {
        throw new Error(`Node ${nodeId} is not connected`);
      }
      
      // 检查节点状态
      if (node.runtime.status === 'link-down') {
        throw new Error('Cannot open SFTP on a link-down node');
      }
      
      // 调用 API 初始化 SFTP (使用终端会话 ID)
      // 注意: SFTP 需要一个关联的终端会话
      const terminalIds = get().getTerminalsForNode(nodeId);
      if (terminalIds.length === 0) {
        throw new Error('No terminal session found for SFTP initialization');
      }
      
      // 验证终端会话是否在 appStore 中存在（避免使用已关闭的会话）
      const { useAppStore } = await import('./appStore');
      const validTerminalId = terminalIds.find(id => 
        useAppStore.getState().sessions.has(id)
      );
      
      if (!validTerminalId) {
        throw new Error('No valid terminal session found. Please create a new terminal first.');
      }
      
      try {
        await guardSessionConnection(validTerminalId);
      } catch (err) {
        if (!isConnectionGuardError(err)) {
          throw err;
        }
        return null;
      }

      const sftpId = await api.sftpInit(validTerminalId);
      
      // 更新后端节点状态
      await api.setTreeNodeSftp(nodeId, sftpId);
      
      // 刷新树
      await get().fetchTree();
      
      return sftpId;
    },
    
    closeSftpForNode: async (nodeId: string) => {
      const node = get().getNode(nodeId);
      if (!node || !node.runtime.sftpSessionId) return;
      
      // 显式关闭 SFTP 会话
      const terminalIds = node.runtime.terminalIds || [];
      if (terminalIds.length > 0) {
        try {
          await api.sftpClose(terminalIds[0]);
        } catch (e) {
          console.error('Failed to close SFTP session:', e);
        }
      }
      
      // 刷新树
      await get().fetchTree();
    },
    
    // ========== State Sync ==========
    
    updateNodeState: async (nodeId: string, state: string, error?: string) => {
      try {
        await api.updateTreeNodeState(nodeId, state, error);
        await get().fetchTree();
      } catch (e) {
        console.error('Failed to update node state:', e);
      }
    },
    
    setNodeConnection: async (nodeId: string, connectionId: string) => {
      try {
        await api.setTreeNodeConnection(nodeId, connectionId);
        await get().fetchTree();
      } catch (e) {
        console.error('Failed to set node connection:', e);
      }
    },
    
    setNodeTerminal: async (nodeId: string, sessionId: string) => {
      try {
        await api.setTreeNodeTerminal(nodeId, sessionId);
        await get().fetchTree();
      } catch (e) {
        console.error('Failed to set node terminal:', e);
      }
    },
    
    setNodeSftp: async (nodeId: string, sessionId: string) => {
      try {
        await api.setTreeNodeSftp(nodeId, sessionId);
        await get().fetchTree();
      } catch (e) {
        console.error('Failed to set node SFTP:', e);
      }
    },
    
    markLinkDown: (nodeId: string) => {
      const descendants = get().getDescendants(nodeId);
      const { linkDownNodeIds } = get();
      const newLinkDownIds = new Set(linkDownNodeIds);
      
      for (const child of descendants) {
        newLinkDownIds.add(child.id);
      }
      
      set({ linkDownNodeIds: newLinkDownIds });
      get().rebuildUnifiedNodes();
    },
    
    markLinkDownBatch: (nodeIds: string[]) => {
      if (nodeIds.length === 0) return;
      
      const { linkDownNodeIds } = get();
      const newLinkDownIds = new Set(linkDownNodeIds);
      
      for (const nodeId of nodeIds) {
        newLinkDownIds.add(nodeId);
      }
      
      set({ linkDownNodeIds: newLinkDownIds });
      get().rebuildUnifiedNodes();
    },
    
    clearLinkDown: (nodeId: string) => {
      const { linkDownNodeIds, rawNodes } = get();
      const newLinkDownIds = new Set(linkDownNodeIds);
      newLinkDownIds.delete(nodeId);
      
      // 只清除子节点中那些自身连接已恢复的节点
      // 如果子节点有自己的连接且仍处于 link-down，保留其标记
      const descendants = get().getDescendants(nodeId);
      for (const child of descendants) {
        // 查找原始节点数据
        const rawChild = rawNodes.find(n => n.id === child.id);
        // 如果子节点有自己的连接 ID，检查其状态
        // 如果没有自己的连接或连接状态正常，清除 link-down
        if (!rawChild?.sshConnectionId) {
          // 子节点没有自己的连接，继承父节点状态
          newLinkDownIds.delete(child.id);
        }
        // 如果子节点有自己的连接，保留其 link-down 标记（需要等待自己的连接恢复）
      }
      
      set({ linkDownNodeIds: newLinkDownIds });
      get().rebuildUnifiedNodes();
    },
    
    setReconnectProgress: (nodeId: string, progress: ReconnectProgress | null) => {
      const { reconnectProgress } = get();
      const newProgress = new Map(reconnectProgress);
      
      if (progress) {
        newProgress.set(nodeId, progress);
      } else {
        newProgress.delete(nodeId);
      }
      
      set({ reconnectProgress: newProgress });
    },
    
    // ========== Concurrency Lock Methods ==========
    
    /**
     * 尝试获取节点连接锁
     * 
     * @param nodeId 节点 ID
     * @returns true 如果成功获取锁，false 如果节点已在连接中
     * 
     * 异常处理：
     * - 如果节点已被锁定，返回 false 而不是抛出异常
     * - 调用者负责处理返回 false 的情况（显示 Toast 等）
     */
    acquireConnectLock: (nodeId: string): boolean => {
      const { connectingNodeIds } = get();
      if (connectingNodeIds.has(nodeId)) {
        console.warn(`[Lock] Node ${nodeId} is already connecting, rejecting duplicate request`);
        return false;
      }
      
      const newSet = new Set(connectingNodeIds);
      newSet.add(nodeId);
      set({ connectingNodeIds: newSet });
      console.debug(`[Lock] Acquired lock for node ${nodeId}`);
      return true;
    },
    
    /**
     * 释放节点连接锁
     * 
     * 安全性：即使节点未被锁定也不会报错（幂等操作）
     */
    releaseConnectLock: (nodeId: string): void => {
      const { connectingNodeIds } = get();
      if (!connectingNodeIds.has(nodeId)) {
        console.debug(`[Lock] Node ${nodeId} was not locked, skipping release`);
        return;
      }
      
      const newSet = new Set(connectingNodeIds);
      newSet.delete(nodeId);
      set({ connectingNodeIds: newSet });
      console.debug(`[Lock] Released lock for node ${nodeId}`);
    },
    
    /**
     * 尝试获取链式连接锁（全局唯一）
     * 
     * @returns true 如果成功获取锁，false 如果已有链在连接中
     * 
     * 用途：防止多条跳板链同时执行，避免竞态条件
     */
    acquireChainLock: (): boolean => {
      if (get().isConnectingChain) {
        console.warn('[Lock] A chain connection is already in progress');
        return false;
      }
      set({ isConnectingChain: true });
      console.debug('[Lock] Acquired chain lock');
      return true;
    },
    
    /**
     * 释放链式连接锁
     * 
     * 安全性：即使未被锁定也不会报错（幂等操作）
     */
    releaseChainLock: (): void => {
      if (!get().isConnectingChain) {
        console.debug('[Lock] Chain was not locked, skipping release');
        return;
      }
      set({ isConnectingChain: false });
      console.debug('[Lock] Released chain lock');
    },
    
    /**
     * 检查节点是否正在连接中
     * 
     * @param nodeId 节点 ID
     * @returns true 如果节点正在连接中
     */
    isNodeConnecting: (nodeId: string): boolean => {
      return get().connectingNodeIds.has(nodeId);
    },
    
    // ========== State Drift Detection ==========
    
    syncFromBackend: async () => {
      const startTime = performance.now();
      const fixed: StateDriftReport['fixed'] = [];
      
      try {
        // 从后端获取最新的节点数据
        const backendNodes = await api.getSessionTree();
        const { rawNodes, nodeTerminalMap, linkDownNodeIds } = get();
        
        // 创建后端节点的映射表，便于快速查找
        const backendMap = new Map(backendNodes.map(n => [n.id, n]));
        const localMap = new Map(rawNodes.map(n => [n.id, n]));
        
        let hasDrift = false;
        
        // 检测漂移并收集修复信息
        for (const [nodeId, backendNode] of backendMap) {
          const localNode = localMap.get(nodeId);
          
          if (!localNode) {
            // 本地缺少该节点（后端新增）
            fixed.push({
              nodeId,
              field: 'node',
              localValue: null,
              backendValue: 'exists',
            });
            hasDrift = true;
            continue;
          }
          
          // 检查状态字段
          if (localNode.state.status !== backendNode.state.status) {
            fixed.push({
              nodeId,
              field: 'state.status',
              localValue: localNode.state.status,
              backendValue: backendNode.state.status,
            });
            hasDrift = true;
          }
          
          // 检查连接 ID
          if (localNode.sshConnectionId !== backendNode.sshConnectionId) {
            fixed.push({
              nodeId,
              field: 'sshConnectionId',
              localValue: localNode.sshConnectionId,
              backendValue: backendNode.sshConnectionId,
            });
            hasDrift = true;
          }
          
          // 检查终端会话 ID
          if (localNode.terminalSessionId !== backendNode.terminalSessionId) {
            fixed.push({
              nodeId,
              field: 'terminalSessionId',
              localValue: localNode.terminalSessionId,
              backendValue: backendNode.terminalSessionId,
            });
            hasDrift = true;
          }
          
          // 检查 SFTP 会话 ID
          if (localNode.sftpSessionId !== backendNode.sftpSessionId) {
            fixed.push({
              nodeId,
              field: 'sftpSessionId',
              localValue: localNode.sftpSessionId,
              backendValue: backendNode.sftpSessionId,
            });
            hasDrift = true;
          }
        }
        
        // 检查本地有但后端没有的节点（孤儿节点）
        for (const [nodeId] of localMap) {
          if (!backendMap.has(nodeId)) {
            fixed.push({
              nodeId,
              field: 'node',
              localValue: 'exists',
              backendValue: null,
            });
            hasDrift = true;
          }
        }
        
        // 如果检测到漂移，使用后端数据覆盖本地
        if (hasDrift) {
          console.warn(`[StateDrift] Detected ${fixed.length} drift(s), auto-fixing...`);
          
          // 清理孤儿节点的 link-down 标记
          const validNodeIds = new Set(backendNodes.map(n => n.id));
          const newLinkDownIds = new Set(
            [...linkDownNodeIds].filter(id => validNodeIds.has(id))
          );
          
          // 清理孤儿节点的终端映射
          const newTerminalMap = new Map(
            [...nodeTerminalMap].filter(([nodeId]) => validNodeIds.has(nodeId))
          );
          const newNodeMap = new Map<string, string>();
          for (const [nodeId, terminals] of newTerminalMap) {
            for (const termId of terminals) {
              newNodeMap.set(termId, nodeId);
            }
          }
          
          set({
            rawNodes: backendNodes,
            linkDownNodeIds: newLinkDownIds,
            nodeTerminalMap: newTerminalMap,
            terminalNodeMap: newNodeMap,
          });
          
          get().rebuildUnifiedNodes();
        }
        
        const syncDuration = performance.now() - startTime;
        
        const report: StateDriftReport = {
          driftCount: fixed.length,
          fixed,
          syncDuration: Math.round(syncDuration),
          timestamp: Date.now(),
        };
        
        if (fixed.length > 0) {
          console.info('[StateDrift] Sync complete:', report);
        }
        
        return report;
        
      } catch (e) {
        console.error('[StateDrift] Sync failed:', e);
        return {
          driftCount: 0,
          fixed: [],
          syncDuration: Math.round(performance.now() - startTime),
          timestamp: Date.now(),
        };
      }
    },
    
    startPeriodicSync: (intervalMs = 30000) => {
      // 先停止已有的定时器
      if (syncIntervalId !== null) {
        clearInterval(syncIntervalId);
      }
      
      console.info(`[StateDrift] Starting periodic sync every ${intervalMs}ms`);
      
      syncIntervalId = setInterval(async () => {
        const report = await get().syncFromBackend();
        if (report.driftCount > 0) {
          console.warn(`[StateDrift] Auto-fixed ${report.driftCount} drift(s)`);
        }
      }, intervalMs);
    },
    
    stopPeriodicSync: () => {
      if (syncIntervalId !== null) {
        clearInterval(syncIntervalId);
        syncIntervalId = null;
        console.info('[StateDrift] Periodic sync stopped');
      }
    },
    
    // ========== UI Actions ==========
    
    selectNode: (nodeId: string | null) => {
      set({ selectedNodeId: nodeId });
    },
    
    toggleExpand: (nodeId: string) => {
      // 使用 settingsStore 管理 expandedIds
      useSettingsStore.getState().toggleTreeNode(nodeId);
      get().rebuildUnifiedNodes();
    },
    
    expandAll: () => {
      const { rawNodes } = get();
      const allExpandable = rawNodes.filter(n => n.hasChildren).map(n => n.id);
      useSettingsStore.getState().setTreeExpanded(allExpandable);
      get().rebuildUnifiedNodes();
    },
    
    collapseAll: () => {
      useSettingsStore.getState().setTreeExpanded([]);
      get().rebuildUnifiedNodes();
    },
    
    // ========== Focus Mode Actions (聚焦模式) ==========
    
    setFocusedNode: (nodeId: string | null) => {
      // 使用 settingsStore 管理 focusedNodeId
      useSettingsStore.getState().setFocusedNode(nodeId);
    },
    
    getBreadcrumbPath: () => {
      const focusedNodeId = get().getFocusedNodeId();
      const { nodes } = get();
      if (!focusedNodeId) return [];
      
      const path: UnifiedFlatNode[] = [];
      const nodeMap = new Map(nodes.map(n => [n.id, n]));
      let currentId: string | null = focusedNodeId;
      
      while (currentId) {
        const node = nodeMap.get(currentId);
        if (!node) break;
        path.unshift(node);
        currentId = node.parentId;
      }
      
      return path;
    },
    
    getVisibleNodes: () => {
      const focusedNodeId = get().getFocusedNodeId();
      const { nodes } = get();
      
      if (!focusedNodeId) {
        // 根视图：显示所有 depth=0 的节点（直连服务器）
        return nodes.filter(n => n.depth === 0);
      }
      
      // 聚焦视图：显示聚焦节点的直接子节点
      return nodes.filter(n => n.parentId === focusedNodeId);
    },
    
    enterNode: (nodeId: string) => {
      const node = get().getNode(nodeId);
      if (!node) return;
      
      // 只有有子节点的节点才能"进入"
      if (node.hasChildren) {
        useSettingsStore.getState().setFocusedNode(nodeId);
      }
    },
    
    goBack: () => {
      const focusedNodeId = get().getFocusedNodeId();
      if (!focusedNodeId) return; // 已经在根视图
      
      const { nodes } = get();
      const nodeMap = new Map(nodes.map(n => [n.id, n]));
      const currentNode = nodeMap.get(focusedNodeId);
      
      // 返回父节点，如果没有父节点则返回根视图
      const parentId = currentNode?.parentId || null;
      useSettingsStore.getState().setFocusedNode(parentId);
    },
    
    // ========== Helpers ==========
    
    // 从 settingsStore 获取 expandedIds (作为 Set)
    getExpandedIds: () => {
      const expandedArray = useSettingsStore.getState().settings.treeUI.expandedIds;
      return new Set(expandedArray);
    },
    
    // 从 settingsStore 获取 focusedNodeId
    getFocusedNodeId: () => {
      return useSettingsStore.getState().settings.treeUI.focusedNodeId;
    },
    
    getNode: (nodeId: string) => {
      return get().nodes.find(n => n.id === nodeId);
    },
    
    getRawNode: (nodeId: string) => {
      return get().rawNodes.find(n => n.id === nodeId);
    },
    
    getNodePath: async (nodeId: string) => {
      return api.getTreeNodePath(nodeId);
    },
    
    getDescendants: (nodeId: string) => {
      const { nodes } = get();
      const result: UnifiedFlatNode[] = [];
      
      // 递归收集所有子节点
      const collectChildren = (parentId: string) => {
        for (const node of nodes) {
          if (node.parentId === parentId) {
            result.push(node);
            collectChildren(node.id);
          }
        }
      };
      
      collectChildren(nodeId);
      return result;
    },
    
    rebuildUnifiedNodes: () => {
      const { rawNodes, nodeTerminalMap, linkDownNodeIds } = get();
      // 从 settingsStore 获取 expandedIds
      const expandedIds = get().getExpandedIds();
      
      // 构建 lineGuides (连接线指示)
      const buildLineGuides = (node: FlatNode, allNodes: FlatNode[]): boolean[] => {
        const guides: boolean[] = [];
        let current = node;
        
        // 从当前节点向上遍历，确定每一层是否需要显示连接线
        while (current.parentId) {
          const parent = allNodes.find(n => n.id === current.parentId);
          if (!parent) break;
          
          // 检查父节点是否还有更多子节点
          const siblings = allNodes.filter(n => n.parentId === parent.id);
          const currentIndex = siblings.findIndex(s => s.id === current.id);
          const hasMoreSiblings = currentIndex < siblings.length - 1;
          
          guides.unshift(hasMoreSiblings);
          current = parent;
        }
        
        return guides;
      };
      
      // 创建统一节点
      const unifiedNodes: UnifiedFlatNode[] = rawNodes.map(node => {
        const isExpanded = expandedIds.has(node.id);
        const lineGuides = buildLineGuides(node, rawNodes);
        
        // 获取该节点的所有终端
        const terminalIds = nodeTerminalMap.get(node.id) || 
          (node.terminalSessionId ? [node.terminalSessionId] : []);
        
        // 计算状态
        const isLinkDown = linkDownNodeIds.has(node.id);
        const runtime: NodeRuntimeState = {
          connectionId: node.sshConnectionId,
          status: computeUnifiedStatus(node.state, terminalIds.length, isLinkDown),
          terminalIds,
          sftpSessionId: node.sftpSessionId,
          errorMessage: node.state.status === 'failed' ? node.state.error : undefined,
          lastConnectedAt: node.state.status === 'connected' ? Date.now() : undefined,
        };
        
        return {
          ...node,
          runtime,
          isExpanded,
          lineGuides,
        };
      });
      
      set({ nodes: unifiedNodes });
    },
  }))
);

// ============================================================================
// Subscriptions & Side Effects
// ============================================================================

/**
 * 初始化 SessionTreeStore 订阅和副作用
 * 
 * 应在 App 初始化时调用此函数，启用：
 * 1. 周期性状态同步（检测和修复前后端漂移）
 * 2. 后端事件监听
 */
export function setupTreeStoreSubscriptions() {
  const store = useSessionTreeStore.getState();
  
  // 启动周期性状态同步（每 30 秒）
  // 可以通过 stopPeriodicSync() 停止
  store.startPeriodicSync(30000);
  
  // 首次启动时立即进行一次同步
  store.syncFromBackend().then(report => {
    if (report.driftCount > 0) {
      console.info(`[SessionTree] Initial sync fixed ${report.driftCount} drift(s)`);
    }
  });
  
  // TODO: 添加 Tauri 事件监听器
  // listen('ssh-connection-state-changed', (event) => { ... })
}

/**
 * 清理 SessionTreeStore 订阅
 * 
 * 应在 App 卸载时调用
 */
export function cleanupTreeStoreSubscriptions() {
  const store = useSessionTreeStore.getState();
  store.stopPeriodicSync();
}

export default useSessionTreeStore;
