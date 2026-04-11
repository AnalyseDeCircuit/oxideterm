// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

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
import { listen } from '@tauri-apps/api/event';
import { api, nodeSftpInit } from '../lib/api';
import { useReconnectOrchestratorStore } from './reconnectOrchestratorStore';
import { topologyResolver } from '../lib/topologyResolver';
import { useEventLogStore } from './eventLogStore';
import { useSettingsStore } from './settingsStore';
import {
  removeConnectionsById,
  removeTerminalSessionsFromConnections,
  upsertSyntheticConnection,
  useAppStore,
} from './appStore';
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

function isAlreadyConnectedError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return message.includes('already connected');
}

function getNodeTerminalIds(node: FlatNode, nodeTerminalMap: Map<string, string[]>, nodeId: string): string[] {
  const terminalIdSet = new Set(nodeTerminalMap.get(nodeId) || []);
  if (node.terminalSessionId) {
    terminalIdSet.add(node.terminalSessionId);
  }
  return Array.from(terminalIdSet);
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
  /**
   * 线性连接节点及其所有祖先（前端驱动）
   * 
   * Phase 3.1: 核心连接器
   * - 获取祖先路径（从根到目标节点）
   * - 批量获取并发锁
   * - 连接前焦土式清理（resetNodeState）
   * - 线性 await 依次连接每个节点
   * - finally 块确保释放所有锁
   * 
   * @param nodeId 目标节点 ID
   * @returns 成功连接的节点 ID 列表
   * @throws 如果任何节点连接失败，抛出错误并指出失败点
   */
  connectNodeWithAncestors: (nodeId: string) => Promise<string[]>;
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
  /** 内部连接方法（无锁检查，供 connectNodeWithAncestors 使用） */
  connectNodeInternal: (nodeId: string) => Promise<void>;
  
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
        
        // 清理 disconnectedTerminalCounts 中与被删除节点关联的条目（防止 Map 泄漏）
        const { disconnectedTerminalCounts } = get();
        if (disconnectedTerminalCounts.size > 0) {
          const newDisconnectedCounts = new Map(disconnectedTerminalCounts);
          for (const node of nodesToRemove) {
            newDisconnectedCounts.delete(node.id);
          }
          if (newDisconnectedCounts.size !== disconnectedTerminalCounts.size) {
            set({ disconnectedTerminalCounts: newDisconnectedCounts });
          }
        }
        
        // 关闭关联的 Tab（异步导入 appStore 避免循环依赖）
        const { tabReferencesAnySession, useAppStore } = await import('./appStore');
        const appState = useAppStore.getState();
        
        // 关闭终端 Tab（通过 sessionId 匹配）
        if (terminalIdsToClose.length > 0) {
          const terminalIdSet = new Set(terminalIdsToClose);
          const tabsToClose = appState.tabs.filter((tab) =>
            tabReferencesAnySession(tab, terminalIdSet)
          );
          for (const tab of tabsToClose) {
            appState.closeTab(tab.id);
          }
        }
        
        // 关闭 SFTP/Forwards/IDE/FileManager 等 nodeId-based Tab
        // removeNode 之前只关闭 terminal Tab，导致非终端 Tab 成为孤儿
        const removedNodeIds = new Set(localRemovedIds);
        const nodeIdTabs = useAppStore.getState().tabs.filter(
          t => t.nodeId && removedNodeIds.has(t.nodeId)
        );
        for (const tab of nodeIdTabs) {
          useAppStore.getState().closeTab(tab.id);
        }
        
        // 中断被删除节点关联的 SFTP 传输（避免 transferStore 残留孤儿条目）
        const { useTransferStore } = await import('./transferStore');
        for (const removedId of localRemovedIds) {
          useTransferStore.getState().interruptTransfersByNode(removedId, 'Node removed');
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
        
        // 清理 linkDownNodeIds 和 reconnectProgress 中的残留条目
        const { linkDownNodeIds, reconnectProgress } = get();
        if (linkDownNodeIds.size > 0 || reconnectProgress.size > 0) {
          const newLinkDown = new Set(linkDownNodeIds);
          const newReconnect = new Map(reconnectProgress);
          let changed = false;
          for (const id of localRemovedIds) {
            if (newLinkDown.delete(id)) changed = true;
            if (newReconnect.delete(id)) changed = true;
          }
          if (changed) {
            set({ linkDownNodeIds: newLinkDown, reconnectProgress: newReconnect });
          }
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
        // 清空全局拓扑映射
        topologyResolver.clear();
        
        set({ 
          rawNodes: [],
          nodes: [], 
          selectedNodeId: null, 
          nodeTerminalMap: new Map(),
          terminalNodeMap: new Map(),
          disconnectedTerminalCounts: new Map(),
          linkDownNodeIds: new Set(),
          reconnectProgress: new Map(),
          connectingNodeIds: new Set(),
          isConnectingChain: false,
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
        console.debug(`[connectNode] Node ${nodeId} is already connecting (locked), skipping`);
        useEventLogStore.getState().addEntry({
          severity: 'info',
          category: 'connection',
          nodeId,
          title: 'event_log.events.already_connecting',
          source: 'connectNode',
        });
        return;
      }
      
      // 检查前端状态，避免重复连接（双重检查）
      if (node.state.status === 'connecting' || node.state.status === 'connected') {
        console.debug(`[connectNode] Node ${nodeId} is already ${node.state.status}, skipping`);
        useEventLogStore.getState().addEntry({
          severity: 'info',
          category: 'connection',
          nodeId,
          title: node.state.status === 'connected' ? 'event_log.events.already_connected' : 'event_log.events.already_connecting',
          source: 'connectNode',
        });
        return;
      }
      
      // 尝试获取锁
      if (!get().acquireConnectLock(nodeId)) {
        console.warn(`[connectNode] Failed to acquire lock for node ${nodeId}`);
        return;
      }
      
      console.debug(`[connectNode] Starting connection for node ${nodeId}`);
      
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
        
        console.debug(`[connectNode] Node ${nodeId} connected successfully`);
        
        // 写入事件日志
        useEventLogStore.getState().addEntry({
          severity: 'info',
          category: 'connection',
          nodeId,
          title: 'event_log.events.connected',
          source: 'connect_node',
        });
      } catch (e) {
        // 失败时回滚到 failed 状态
        console.error(`[connectNode] Node ${nodeId} connection failed:`, e);
        
        // 写入事件日志
        useEventLogStore.getState().addEntry({
          severity: 'error',
          category: 'connection',
          nodeId,
          title: 'event_log.events.node_state_error',
          detail: String(e),
          source: 'connect_node',
        });
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
      
      // 1.5 取消待重连队列中的节点（防止诈尸重连）
      const orchestrator = useReconnectOrchestratorStore.getState();
      for (const n of allAffectedNodes) {
        orchestrator.cancel(n.id);
      }
      
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
        const { tabReferencesAnySession, useAppStore } = await import('./appStore');
        const appStore = useAppStore.getState();
        const sessionIdSet = new Set(sessionIdsToClose);
        const affectedNodeIdSet = new Set(allAffectedNodes.map((n) => n.id));
        const tabsToClose = appStore.tabs.filter((tab) =>
          tabReferencesAnySession(tab, sessionIdSet) ||
          (!!tab.nodeId && affectedNodeIdSet.has(tab.nodeId))
        );
        for (const tab of tabsToClose) {
          appStore.closeTab(tab.id);
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
      
      // 7. 写入事件日志
      useEventLogStore.getState().addEntry({
        severity: 'info',
        category: 'connection',
        nodeId,
        title: 'event_log.events.disconnected',
        detail: allAffectedNodes.length > 1
          ? `event_log.events.affected_children:${allAffectedNodes.length - 1}`
          : undefined,
        source: 'disconnect_node',
      });
      
      // 8. 调用后端断开节点（会递归断开子节点并更新状态）
      try {
        await api.disconnectTreeNode(nodeId);
      } catch (e) {
        console.error('Failed to disconnect tree node:', e);
      }
      
      const disconnectedConnectionIds = allAffectedNodes
        .map((affectedNode) => affectedNode.sshConnectionId)
        .filter((connectionId): connectionId is string => !!connectionId);

      useAppStore.setState((state) => ({
        connections: removeConnectionsById(state.connections, disconnectedConnectionIds),
      }));

      // 8. 刷新树状态
      await get().fetchTree();
    },
    
    /**
     * 级联重连节点及其之前已连接的子节点
     * 
     * Phase 3.2 重写：使用线性连接逻辑
     * 
     * 执行流程：
     * 1. 获取链式锁
     * 2. 先重连目标节点自身（使用 connectNodeWithAncestors 确保祖先链畅通）
     * 3. 如果不跳过子节点，收集所有 link-down 的子节点
     * 4. 按深度排序，线性重连子节点
     * 
     * @param nodeId 要重连的节点 ID
     * @param options 配置选项
     * @returns 成功重连的节点 ID 列表
     */
    reconnectCascade: async (nodeId: string, options?: { skipChildren?: boolean }) => {
      console.debug(`[reconnectCascade] 📥 ENTRY: reconnectCascade called for node ${nodeId}`);
      const node = get().getNode(nodeId);
      if (!node) {
        console.error(`[reconnectCascade] ❌ Node ${nodeId} not found!`);
        throw new Error(`Node ${nodeId} not found`);
      }
      
      console.debug(`[reconnectCascade] 🚀 Starting cascade reconnect for ${nodeId}, node status: ${node.runtime.status}`);
      
      const reconnected: string[] = [];
      
      // 1. 首先确保目标节点及其祖先都已连接
      try {
        // 使用 connectNodeWithAncestors 确保整条链路畅通
        const chainResult = await get().connectNodeWithAncestors(nodeId);
        reconnected.push(...chainResult.filter(id => !reconnected.includes(id)));
        console.debug(`[reconnectCascade] Target node ${nodeId} and ancestors connected`);
      } catch (e) {
        const errorMsg = e instanceof Error ? e.message : String(e);
        console.error(`[reconnectCascade] Failed to reconnect node ${nodeId}:`, errorMsg);
        throw e; // 目标节点重连失败，不继续重连子节点
      }
      
      // 2. 如果不跳过子节点，尝试重连 link-down 的子节点
      if (!options?.skipChildren) {
        const descendants = get().getDescendants(nodeId);
        const { linkDownNodeIds } = get();
        
        // 只处理标记为 link-down 的子节点
        const linkDownChildren = descendants.filter(child => linkDownNodeIds.has(child.id));
        
        if (linkDownChildren.length > 0) {
          console.debug(`[reconnectCascade] Found ${linkDownChildren.length} link-down children to reconnect`);
          
          // 按深度排序，确保从上到下依次重连
          const sortedChildren = [...linkDownChildren].sort((a, b) => a.depth - b.depth);
          
          for (const child of sortedChildren) {
            // 检查父节点是否已连接（确保链路畅通）
            const parent = get().getNode(child.parentId!);
            if (parent?.runtime.status !== 'connected' && parent?.runtime.status !== 'active') {
              console.debug(`[reconnectCascade] Skipping ${child.id}: parent not connected`);
              continue;
            }
            
            try {
              // 使用单节点连接（父节点已确认连接）
              await get().connectNode(child.id);
              reconnected.push(child.id);
              console.debug(`[reconnectCascade] Child ${child.id} reconnected`);
              
              // 短暂延迟，避免同时发起太多连接
              await new Promise(resolve => setTimeout(resolve, 100));
            } catch (e) {
              console.warn(`[reconnectCascade] Failed to reconnect child ${child.id}:`, e);
              // 子节点重连失败不中断流程，继续尝试其他节点
            }
          }
        }
      }
      
      // 3. 刷新树状态
      await get().fetchTree();
      
      console.debug(`[reconnectCascade] Completed: ${reconnected.length} nodes reconnected`);
      
      return reconnected;
    },
    
    /**
     * 线性连接节点及其所有祖先（前端驱动）
     * 
     * Phase 3.1: 核心连接器 - 解决 OxideTerm 假死、端口占用和重影问题
     * 
     * 执行流程：
     * 1. 获取链式锁（全局唯一，防止多条链同时执行）
     * 2. 获取祖先路径（从根到目标节点）
     * 3. 跳过已连接的前缀节点（复用现有连接）
     * 4. 批量获取节点级锁
     * 5. 焦土式清理所有待连接节点（resetNodeState）
     * 6. 线性 await 依次连接每个节点
     * 7. finally 块确保释放所有锁
     * 
     * 锁机制：
     * - acquireChainLock: 全局锁，同一时刻只能有一条链在连接
     * - acquireConnectLock: 节点级锁，防止同一节点被重复连接
     * 
     * 熔断机制：
     * - 任何节点连接失败，立即中断后续节点连接
     * - 已连接的节点保持连接状态
     * - 返回部分成功结果
     * 
     * @param nodeId 目标节点 ID
     * @returns 成功连接的节点 ID 列表
     * @throws ConnectionChainError 如果任何节点连接失败
     */
    connectNodeWithAncestors: async (nodeId: string): Promise<string[]> => {
      console.debug(`[connectNodeWithAncestors] 📥 ENTRY: connectNodeWithAncestors called for node ${nodeId}`);
      
      // ========== Step 1: 获取链式锁 ==========
      const lockAcquired = get().acquireChainLock();
      console.debug(`[connectNodeWithAncestors] Chain lock acquire attempt: ${lockAcquired ? '✅ SUCCESS' : '❌ BUSY'}`);
      if (!lockAcquired) {
        console.warn(`[connectNodeWithAncestors] Chain lock busy, rejecting request for ${nodeId}`);
        throw new Error('CHAIN_LOCK_BUSY: Another connection chain is in progress');
      }
      
      const lockedNodeIds: string[] = [];
      const connectedNodeIds: string[] = [];
      
      try {
        // ========== Step 2: 获取祖先路径 ==========
        console.debug(`[connectNodeWithAncestors] Fetching path for node ${nodeId}`);
        const pathNodes = await get().getNodePath(nodeId);
        
        if (pathNodes.length === 0) {
          throw new Error(`Node path not found for ${nodeId}`);
        }
        
        console.debug(`[connectNodeWithAncestors] Path: ${pathNodes.map(n => n.id).join(' → ')}`);
        
        // ========== Step 3: 跳过已连接的前缀节点 ==========
        // 找到第一个需要连接的节点（状态非 connected/active）
        // 注意: link-down 节点虽然仍有 sshConnectionId，但连接已不可用，不能跳过
        let startIndex = 0;
        const { linkDownNodeIds } = get();
        for (let i = 0; i < pathNodes.length; i++) {
          const node = get().getRawNode(pathNodes[i].id);
          if (!node) continue;
          
          const isConnected = node.state.status === 'connected';
          const hasConnectionId = !!node.sshConnectionId;
          const isLinkDown = linkDownNodeIds.has(pathNodes[i].id);
          
          if (isConnected && hasConnectionId && !isLinkDown) {
            // 该节点已连接且未标记为 link-down，跳过
            startIndex = i + 1;
            console.debug(`[connectNodeWithAncestors] Skipping already connected node ${pathNodes[i].id}`);
          } else {
            // 遇到第一个未连接或 link-down 节点，停止跳过
            break;
          }
        }
        
        const nodesToConnect = pathNodes.slice(startIndex);
        
        if (nodesToConnect.length === 0) {
          console.debug(`[connectNodeWithAncestors] All nodes already connected`);
          return pathNodes.map(n => n.id);
        }
        
        console.debug(`[connectNodeWithAncestors] Nodes to connect: ${nodesToConnect.map(n => n.id).join(' → ')}`);
        
        // ========== Step 4: 批量获取节点级锁 ==========
        for (const node of nodesToConnect) {
          if (!get().acquireConnectLock(node.id)) {
            throw new Error(`NODE_LOCK_BUSY: Node ${node.id} is already connecting`);
          }
          lockedNodeIds.push(node.id);
        }
        
        console.debug(`[connectNodeWithAncestors] Acquired locks for ${lockedNodeIds.length} nodes`);
        
        // ========== Step 5: 焦土式清理所有待连接节点 ==========
        console.debug(`[connectNodeWithAncestors] Phase: Scorched earth cleanup`);
        for (const node of nodesToConnect) {
          await get().resetNodeState(node.id);
        }
        
        // ========== Step 6: 线性 await 依次连接 ==========
        console.debug(`[connectNodeWithAncestors] Phase: Linear connection`);
        
        for (let i = 0; i < nodesToConnect.length; i++) {
          const node = nodesToConnect[i];
          const isTarget = i === nodesToConnect.length - 1;
          
          console.debug(`[connectNodeWithAncestors] Connecting node ${i + 1}/${nodesToConnect.length}: ${node.id}${isTarget ? ' (TARGET)' : ''}`);
          
          try {
            // 调用单节点连接（不带锁检查，因为我们已经持有锁）
            await get().connectNodeInternal(node.id);
            connectedNodeIds.push(node.id);
            
            // 短暂延迟，让后端有时间稳定
            if (!isTarget) {
              await new Promise(resolve => setTimeout(resolve, 100));
            }
          } catch (e) {
            // ========== 熔断：连接失败，中断链式连接 ==========
            const errorMsg = e instanceof Error ? e.message : String(e);
            console.error(`[connectNodeWithAncestors] Node ${node.id} connection failed: ${errorMsg}`);
            
            // 标记失败节点
            try {
              await api.updateTreeNodeState(node.id, 'failed', errorMsg);
            } catch (updateErr) {
              console.warn(`[connectNodeWithAncestors] Failed to update node state:`, updateErr);
            }
            
            // 刷新树状态
            await get().fetchTree();
            
            // 抛出详细错误，包含失败位置
            throw new Error(
              `CONNECTION_CHAIN_FAILED: Node ${node.id} (position ${i + 1}/${nodesToConnect.length}) failed: ${errorMsg}`
            );
          }
        }
        
        // ========== 全部成功 ==========
        console.debug(`[connectNodeWithAncestors] Chain completed successfully: ${connectedNodeIds.length} nodes connected`);
        
        // 刷新树状态
        await get().fetchTree();
        
        // 返回所有成功连接的节点（包括之前已连接的）
        return [...pathNodes.slice(0, startIndex).map(n => n.id), ...connectedNodeIds];
        
      } finally {
        // ========== Step 7: 始终释放所有锁 ==========
        for (const nodeId of lockedNodeIds) {
          get().releaseConnectLock(nodeId);
        }
        get().releaseChainLock();
        console.debug(`[connectNodeWithAncestors] Released all locks`);
      }
    },
    
    /**
     * 内部连接方法（无锁检查，供 connectNodeWithAncestors 使用）
     * 
     * 与 connectNode 的区别：
     * - 不检查/获取锁（调用者已持有锁）
     * - 不在 finally 中释放锁
     * - 专为批量连接设计
     */
    connectNodeInternal: async (nodeId: string) => {
      const node = get().getRawNode(nodeId);
      if (!node) throw new Error(`Node ${nodeId} not found`);
      
      // 乐观更新：立即在本地设置为 connecting
      set((state) => ({
        rawNodes: state.rawNodes.map(n => 
          n.id === nodeId 
            ? { ...n, state: { ...n.state, status: 'connecting' as const } }
            : n
        )
      }));
      get().rebuildUnifiedNodes();
      
      let response: Awaited<ReturnType<typeof api.connectTreeNode>>;
      try {
        response = await api.connectTreeNode({ nodeId });
      } catch (error) {
        if (!isAlreadyConnectedError(error)) {
          throw error;
        }

        // Backend may still think the node is connected while its channel is unusable.
        // Force a backend-side disconnect once, then retry connect.
        console.warn(`[connectNodeInternal] Node ${nodeId} reported already connected, forcing backend disconnect and retrying`);
        try {
          await api.disconnectTreeNode(nodeId);
        } catch (disconnectError) {
          console.warn(`[connectNodeInternal] Forced disconnect failed for ${nodeId}:`, disconnectError);
        }

        // Refresh tree snapshot before retry to reduce stale state drift.
        await get().fetchTree();
        response = await api.connectTreeNode({ nodeId });
      }
      
      // 更新连接 ID
      await api.setTreeNodeConnection(nodeId, response.sshConnectionId);
      
      // 注册连接映射 (connectionId -> nodeId)
      topologyResolver.register(response.sshConnectionId, nodeId);
      
      // 清除该节点的 link-down 标记
      const { linkDownNodeIds } = get();
      if (linkDownNodeIds.has(nodeId)) {
        const newLinkDownIds = new Set(linkDownNodeIds);
        newLinkDownIds.delete(nodeId);
        set({ linkDownNodeIds: newLinkDownIds });
      }
      
      // ⚡ 关键修复：立即从后端同步状态，减少状态漂移
      // 不使用 fetchTree() 因为它会获取全部节点，这里只更新单个节点
      set((state) => ({
        rawNodes: state.rawNodes.map(n => 
          n.id === nodeId 
            ? { ...n, state: { status: 'connected' as const }, sshConnectionId: response.sshConnectionId }
            : n
        )
      }));
      get().rebuildUnifiedNodes();

      const parentConnectionId = node.parentId
        ? get().getRawNode(node.parentId)?.sshConnectionId ?? undefined
        : undefined;
      const terminalIds = getNodeTerminalIds(node, get().nodeTerminalMap, nodeId);
      useAppStore.setState((state) => ({
        connections: upsertSyntheticConnection(state.connections, {
          id: response.sshConnectionId,
          host: node.host,
          port: node.port,
          username: node.username,
          parentConnectionId,
          terminalIds,
          sftpSessionId: node.sftpSessionId,
          state: terminalIds.length > 0 ? 'active' : 'idle',
        }),
      }));

      // 同步 appStore.sessions 中关联终端的 connectionId
      // 重连后 connectionId 变化，必须更新 sessions 以保持一致性
      if (terminalIds.length > 0) {
        useAppStore.setState((state) => {
          const newSessions = new Map(state.sessions);
          for (const terminalId of terminalIds) {
            const session = newSessions.get(terminalId);
            if (session) {
              newSessions.set(terminalId, {
                ...session,
                connectionId: response.sshConnectionId,
              });
            }
          }
          return { sessions: newSessions };
        });
        console.debug(`[connectNodeInternal] Updated connectionId for ${terminalIds.length} sessions: ${response.sshConnectionId}`);
      }
      
      console.debug(`[connectNodeInternal] Node ${nodeId} connected with SSH ID: ${response.sshConnectionId}`);
    },
    
    /**
     * 重置节点状态（焦土式清理）
     * 
     * 执行顺序：
      * 1. 清理 appStore sessions
      * 2. 清理本地映射
      * 3. 重置节点状态为 pending
     * 
     * 异常处理：
      * - 仅做本地清理，避免与后端断开流程重复销毁终端
      * - 确保本地状态一定被清理
     */
    resetNodeState: async (nodeId: string): Promise<void> => {
      const node = get().getRawNode(nodeId);
      if (!node) {
        console.warn(`[resetNodeState] Node ${nodeId} not found`);
        return;
      }
      
      console.debug(`[resetNodeState] Resetting node ${nodeId}`);

      // ========== Phase 1: 收集现有终端 ==========

      const terminalIds = [...(get().nodeTerminalMap.get(nodeId) || [])];

      // 也检查后端记录的 terminalSessionId
      if (node.terminalSessionId && !terminalIds.includes(node.terminalSessionId)) {
        terminalIds.push(node.terminalSessionId);
      }

      // ========== Phase 2: 清理 appStore sessions ==========
      
      try {
        const { useAppStore } = await import('./appStore');
        useAppStore.setState((state) => {
          const newSessions = new Map(state.sessions);
          for (const terminalId of terminalIds) {
            newSessions.delete(terminalId);
          }
          return {
            sessions: newSessions,
            connections: node.sshConnectionId
              ? removeConnectionsById(
                  removeTerminalSessionsFromConnections(
                    state.connections,
                    state.sessions,
                    terminalIds,
                  ),
                  [node.sshConnectionId],
                )
              : removeTerminalSessionsFromConnections(
                  state.connections,
                  state.sessions,
                  terminalIds,
                ),
          };
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
      
      console.debug(`[resetNodeState] Node ${nodeId} reset complete`);
    },
    
    // ========== Terminal Management ==========
    
    createTerminalForNode: async (nodeId: string, cols?: number, rows?: number) => {
      const node = get().getNode(nodeId);
      if (!node) throw new Error(`Node ${nodeId} not found`);
      const connectionId = node.runtime.connectionId;
      if (!connectionId) {
        throw new Error(`Node ${nodeId} is not connected`);
      }
      
      // 从 settingsStore 获取后端缓冲区配置
      const { useSettingsStore } = await import('./settingsStore');
      const bufferSettings = useSettingsStore.getState().settings.buffer;
      
      // 调用 API 创建终端
      const response = await api.createTerminal({
        connectionId,
        cols,
        rows,
        maxBufferLines: bufferSettings.maxLines,
        saveOnDisconnect: bufferSettings.saveOnDisconnect,
      });
      const terminalId = response.sessionId;
      
      // 同步到 appStore.sessions（用于 createTab 兼容）
      const { useAppStore } = await import('./appStore');
      useAppStore.setState((state) => {
        const newSessions = new Map(state.sessions);
        newSessions.set(terminalId, {
          ...response.session,
          connectionId,
        });
        return {
          sessions: newSessions,
          connections: upsertSyntheticConnection(state.connections, {
            id: connectionId,
            host: node.host,
            port: node.port,
            username: node.username,
            parentConnectionId: node.parentId
              ? get().getRawNode(node.parentId)?.sshConnectionId ?? undefined
              : undefined,
            terminalIds: [terminalId],
            sftpSessionId: node.sftpSessionId,
            state: 'active',
          }),
        };
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
        console.error(`[createTerminalForNode] Failed for node ${nodeId}, rolling back:`, e);
        try {
          await api.closeTerminal(terminalId);
          useAppStore.setState((state) => {
            const newSessions = new Map(state.sessions);
            newSessions.delete(terminalId);
            return {
              sessions: newSessions,
              connections: removeTerminalSessionsFromConnections(
                state.connections,
                state.sessions,
                [terminalId],
              ),
            };
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
      
      // 使用 set(nodeId, []) 而非 delete — 空数组是 truthy，
      // 防止 rebuildUnifiedNodes 回退读取后端已过期的 terminalSessionId
      newTerminalMap.set(nodeId, filtered);
      newNodeMap.delete(terminalId);
      
      set({ nodeTerminalMap: newTerminalMap, terminalNodeMap: newNodeMap });
      
      // 当移除最后一个终端时，清除后端持久化的 terminalSessionId
      if (filtered.length === 0) {
        api.clearTreeNodeTerminal(nodeId).catch(e => {
          console.error('Failed to clear terminal session id:', e);
        });
      }
      
      // 注意：不在这里调用 api.closeTerminal — 后端终端关闭
      // 由 closeTerminalSession (appStore) 或 closeTab Phase 6 负责
      
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
      // 同 closeTerminalForNode：使用空数组防止 rebuildUnifiedNodes 回退
      newTerminalMap.set(nodeId, filtered);
      newNodeMap.delete(terminalId);

      // 清除后端持久化的 terminalSessionId
      if (filtered.length === 0) {
        api.clearTreeNodeTerminal(nodeId).catch(e => {
          console.error('Failed to clear terminal session id:', e);
        });
      }

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
      
      console.debug(`[SessionTree] Adding KBI session: ${sessionId} for ${displayName}`);
      
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
        
        console.debug(`[SessionTree] KBI root node created: ${nodeId}`);
        
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
        
        console.debug(`[SessionTree] KBI session ${sessionId} added to tree under node ${nodeId}`);
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
      
      // Node-first: 通过 nodeId 直接初始化 SFTP，后端自动路由到正确的 ConnectionEntry
      const terminalIds = get().getTerminalsForNode(nodeId);
      if (terminalIds.length === 0) {
        throw new Error('No terminal session found for SFTP initialization');
      }
      
      try {
        const sftpCwd = await nodeSftpInit(nodeId);
        
        // 更新后端节点状态
        await api.setTreeNodeSftp(nodeId, sftpCwd);
        
        // 刷新树
        await get().fetchTree();
        
        // Return the first terminal sessionId for tab creation compatibility
        return terminalIds[0];
      } catch (err) {
        console.error(`[openSftpForNode] Failed for node ${nodeId}:`, err);
        return null;
      }
    },
    
    closeSftpForNode: async (nodeId: string) => {
      const node = get().getNode(nodeId);
      if (!node || !node.runtime.sftpSessionId) return;
      
      // SFTP 生命周期由 ConnectionEntry 管理，节点断开时自动清理
      // 这里只刷新树状态以更新 UI
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
          
          // 清理孤儿节点的终端映射（同时过滤空数组防止积累）
          const newTerminalMap = new Map(
            [...nodeTerminalMap].filter(([nodeId, terminals]) => validNodeIds.has(nodeId) && terminals.length > 0)
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

          const removedConnectionIds = rawNodes
            .map((node) => node.sshConnectionId)
            .filter(
              (connectionId): connectionId is string =>
                !!connectionId && !backendNodes.some((node) => node.sshConnectionId === connectionId),
            );

          useAppStore.setState((state) => {
            let newConnections = removeConnectionsById(state.connections, removedConnectionIds);

            for (const backendNode of backendNodes) {
              if (!backendNode.sshConnectionId) {
                continue;
              }

              const terminalIds = getNodeTerminalIds(backendNode, newTerminalMap, backendNode.id);
              const parentConnectionId = backendNode.parentId
                ? backendMap.get(backendNode.parentId)?.sshConnectionId ?? undefined
                : undefined;

              newConnections = upsertSyntheticConnection(newConnections, {
                id: backendNode.sshConnectionId,
                host: backendNode.host,
                port: backendNode.port,
                username: backendNode.username,
                parentConnectionId,
                terminalIds,
                sftpSessionId: backendNode.sftpSessionId,
                state: terminalIds.length > 0 ? 'active' : 'idle',
              });
            }

            return { connections: newConnections };
          });
          
          // 🔴 Phase 5.1: 同步 appStore.sessions 中终端的 connectionId
          // StateDrift 可能包含 sshConnectionId 变化，必须同步到 sessions 否则 SFTP 会失败
          try {
            useAppStore.setState((state) => {
              const newSessions = new Map(state.sessions);
              let updated = 0;
              
              for (const backendNode of backendNodes) {
                if (!backendNode.sshConnectionId) continue;
                
                // 获取该节点关联的终端 ID
                const terminalIdSet = new Set(newTerminalMap.get(backendNode.id) || []);
                if (backendNode.terminalSessionId) {
                  terminalIdSet.add(backendNode.terminalSessionId);
                }
                const terminalIds = Array.from(terminalIdSet);
                for (const terminalId of terminalIds) {
                  const session = newSessions.get(terminalId);
                  if (session && session.connectionId !== backendNode.sshConnectionId) {
                    newSessions.set(terminalId, {
                      ...session,
                      connectionId: backendNode.sshConnectionId,
                    });
                    updated++;
                  }
                }
              }
              
              if (updated > 0) {
                console.info(`[StateDrift] Updated connectionId for ${updated} sessions`);
              }
              return { sessions: newSessions };
            });
          } catch (e) {
            console.warn('[StateDrift] Failed to sync sessions connectionId:', e);
          }
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
      
      // Pre-build lookup maps for O(1) access (avoids O(n) find per node)
      const nodeById = new Map<string, FlatNode>(rawNodes.map(n => [n.id, n]));
      const childrenByParent = new Map<string | null, FlatNode[]>();
      for (const n of rawNodes) {
        const pid = n.parentId ?? null;
        let arr = childrenByParent.get(pid);
        if (!arr) { arr = []; childrenByParent.set(pid, arr); }
        arr.push(n);
      }

      // 构建 lineGuides (连接线指示)
      const buildLineGuides = (node: FlatNode): boolean[] => {
        const guides: boolean[] = [];
        let current = node;
        
        // 从当前节点向上遍历，确定每一层是否需要显示连接线
        while (current.parentId) {
          const parent = nodeById.get(current.parentId);
          if (!parent) break;
          
          // 检查父节点是否还有更多子节点
          const siblings = childrenByParent.get(parent.id) || [];
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
        const lineGuides = buildLineGuides(node);
        
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
  
  // 监听 connection_status_changed 事件，实时同步后端状态变更
  // appStore.connections 由 useConnectionEvents 和本地增量写入维护，这里只负责树状态自愈
  let unlistenStatus: (() => void) | null = null;
  listen<unknown>('connection_status_changed', () => {
    // 后端连接状态发生变化时，立即同步 sessionTree
    store.syncFromBackend().then(report => {
      if (report.driftCount > 0) {
        console.info(`[SessionTree] Event-driven sync fixed ${report.driftCount} drift(s)`);
      }
    });
  }).then(fn => {
    unlistenStatus = fn;
  });

  // 存储 unlisten 函数以便 cleanup 时调用
  (setupTreeStoreSubscriptions as any)._unlisten = () => {
    unlistenStatus?.();
  };
}

/**
 * 清理 SessionTreeStore 订阅
 * 
 * 应在 App 卸载时调用
 */
export function cleanupTreeStoreSubscriptions() {
  const store = useSessionTreeStore.getState();
  store.stopPeriodicSync();
  // 清理 Tauri 事件监听器
  (setupTreeStoreSubscriptions as any)._unlisten?.();
}

export default useSessionTreeStore;
