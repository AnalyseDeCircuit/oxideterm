/**
 * Hook to listen for SSH connection status change events from backend
 * 
 * Phase 3.5: 前端驱动的自动重连
 * 
 * Events:
 * - connection_status_changed: { connection_id, status, affected_children, timestamp }
 * 
 * 🛑 已移除的事件监听（后端不再发送）：
 * - connection_reconnect_progress: 后端重连引擎已物理删除
 * - connection_reconnected: 后端不再自主重连
 * 
 * 重连策略：
 * - 监听 link_down 事件
 * - 防抖聚合：短时间内大量节点掉线时，只触发一次 reconnectCascade
 * - 由 reconnectCascade 内部的 BFS 深度排序逻辑进行有序恢复
 */

import { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useAppStore } from '../store/appStore';
import { useTransferStore } from '../store/transferStore';
import { useSessionTreeStore } from '../store/sessionTreeStore';
import { topologyResolver } from '../lib/topologyResolver';
import i18n from '../i18n';
import type { SshConnectionState } from '../types';

interface ConnectionStatusEvent {
  connection_id: string;
  status: 'connected' | 'link_down' | 'reconnecting' | 'disconnected';
  affected_children: string[];  // 受影响的子连接
  timestamp: number;            // 时间戳
}

// ═══════════════════════════════════════════════════════════════════════════════
// 防抖重连管理器
// ═══════════════════════════════════════════════════════════════════════════════

/** 防抖延迟（毫秒）- 聚合短时间内的多个 link_down 事件 */
const RECONNECT_DEBOUNCE_MS = 500;

/** 待重连的节点集合 */
const pendingReconnectNodes = new Set<string>();

/** 防抖定时器 */
let reconnectDebounceTimer: ReturnType<typeof setTimeout> | null = null;

/** 最大重试次数 */
const MAX_RECONNECT_RETRIES = 3;

/** 重试间隔（毫秒） */
const RECONNECT_RETRY_DELAY_MS = 2000;

/** 当前重试次数 */
let reconnectRetryCount = 0;

/** 是否正在执行重连 */
let isReconnecting = false;

/**
 * 从待重连队列中移除节点
 * 
 * 用于防止"诈尸重连"：当用户手动断开连接或关闭标签页时，
 * 调用此函数移除该节点，防止防抖期间仍然尝试重连已关闭的节点。
 * 
 * @param nodeId 要移除的节点 ID
 */
export function cancelPendingReconnect(nodeId: string): void {
  if (pendingReconnectNodes.has(nodeId)) {
    console.log(`[ReconnectScheduler] Canceling pending reconnect for node ${nodeId}`);
    pendingReconnectNodes.delete(nodeId);
  }
}

/**
 * 清除所有待重连节点
 * 
 * 用于全局重置，如用户退出应用或刷新页面。
 */
export function clearAllPendingReconnects(): void {
  if (pendingReconnectNodes.size > 0) {
    console.log(`[ReconnectScheduler] Clearing ${pendingReconnectNodes.size} pending reconnects`);
    pendingReconnectNodes.clear();
  }
  if (reconnectDebounceTimer) {
    clearTimeout(reconnectDebounceTimer);
    reconnectDebounceTimer = null;
  }
}

/**
 * 调度防抖重连
 * 
 * 设计原则：
 * - 短时间内多个节点掉线时（如跳板机断开），聚合为一次重连
 * - 选择深度最浅的节点作为起点，让 reconnectCascade 处理级联恢复
 * - 避免重复触发正在进行的重连操作
 */
function scheduleReconnect(nodeId: string): void {
  console.log(`[ReconnectScheduler] 📥 scheduleReconnect called for node ${nodeId}`);
  console.log(`[ReconnectScheduler] Current state: pending=${pendingReconnectNodes.size}, isReconnecting=${isReconnecting}, timerActive=${reconnectDebounceTimer !== null}`);
  
  pendingReconnectNodes.add(nodeId);
  
  // 清除之前的定时器
  if (reconnectDebounceTimer) {
    clearTimeout(reconnectDebounceTimer);
    console.log(`[ReconnectScheduler] Cleared previous debounce timer`);
  }
  
  // 设置新的防抖定时器
  console.log(`[ReconnectScheduler] Setting debounce timer for ${RECONNECT_DEBOUNCE_MS}ms`);
  reconnectDebounceTimer = setTimeout(async () => {
    console.log(`[ReconnectScheduler] ⏰ Debounce timer fired`);
    reconnectDebounceTimer = null;
    
    // 如果正在重连，跳过此次调度
    if (isReconnecting) {
      console.log('[ReconnectScheduler] ❌ Reconnect already in progress, skipping');
      return;
    }
    
    // 获取所有待重连节点
    const nodeIds = Array.from(pendingReconnectNodes);
    pendingReconnectNodes.clear();
    
    if (nodeIds.length === 0) return;
    
    console.log(`[ReconnectScheduler] Processing ${nodeIds.length} pending reconnect nodes:`, nodeIds);
    
    // 找到深度最浅的节点（根节点优先）
    // 这样 reconnectCascade 会自动处理所有后代的恢复
    const treeStore = useSessionTreeStore.getState();
    const nodes = nodeIds
      .map(id => treeStore.getNode(id))
      .filter((n): n is NonNullable<typeof n> => n !== undefined);
    
    if (nodes.length === 0) {
      console.warn('[ReconnectScheduler] No valid nodes found for reconnect');
      return;
    }
    
    // 按深度排序，找到最浅的节点
    nodes.sort((a, b) => a.depth - b.depth);
    const rootNode = nodes[0];
    
    console.log(`[ReconnectScheduler] 🚀 Starting reconnect from shallowest node: ${rootNode.id} (depth=${rootNode.depth})`);
    console.log(`[ReconnectScheduler] All pending nodes:`, nodeIds);
    
    isReconnecting = true;
    reconnectRetryCount = 0;
    
    const attemptReconnect = async (): Promise<void> => {
      try {
        // 使用 reconnectCascade 进行有序恢复
        const reconnected = await treeStore.reconnectCascade(rootNode.id);
        console.log(`[ReconnectScheduler] ✅ Reconnect completed: ${reconnected.length} nodes reconnected`);
        reconnectRetryCount = 0; // 重置重试计数
      } catch (e) {
        const errorMsg = e instanceof Error ? e.message : String(e);
        console.error(`[ReconnectScheduler] ❌ Reconnect failed (attempt ${reconnectRetryCount + 1}/${MAX_RECONNECT_RETRIES}):`, errorMsg);
        
        // 检查是否是锁忙错误，如果是则重试
        const isRetryable = errorMsg.includes('CHAIN_LOCK_BUSY') || errorMsg.includes('NODE_LOCK_BUSY');
        
        if (isRetryable && reconnectRetryCount < MAX_RECONNECT_RETRIES - 1) {
          reconnectRetryCount++;
          console.log(`[ReconnectScheduler] 🔄 Scheduling retry ${reconnectRetryCount}/${MAX_RECONNECT_RETRIES} in ${RECONNECT_RETRY_DELAY_MS}ms`);
          
          // 延迟后重试
          await new Promise(resolve => setTimeout(resolve, RECONNECT_RETRY_DELAY_MS));
          
          // 检查节点是否还需要重连（可能用户已手动处理）
          const currentNode = treeStore.getNode(rootNode.id);
          if (currentNode && (currentNode.runtime.status === 'link-down' || currentNode.runtime.status === 'idle' || currentNode.runtime.status === 'error')) {
            console.log(`[ReconnectScheduler] 🔄 Retrying reconnect for node ${rootNode.id}`);
            await attemptReconnect();
          } else {
            console.log(`[ReconnectScheduler] Node ${rootNode.id} status changed to ${currentNode?.runtime.status}, skipping retry`);
          }
        } else {
          console.warn(`[ReconnectScheduler] ⚠️ Reconnect failed after ${reconnectRetryCount + 1} attempts, giving up. User can trigger manual reconnect.`);
        }
      }
    };
    
    try {
      await attemptReconnect();
    } finally {
      isReconnecting = false;
      reconnectRetryCount = 0;
    }
  }, RECONNECT_DEBOUNCE_MS);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 主 Hook
// ═══════════════════════════════════════════════════════════════════════════════

export function useConnectionEvents(): void {
  // Use selectors to get stable function references
  const updateConnectionState = useAppStore((state) => state.updateConnectionState);
  const interruptTransfersBySession = useTransferStore((state) => state.interruptTransfersBySession);
  
  // Use ref for sessions to avoid re-subscribing on every session change
  const sessionsRef = useRef(useAppStore.getState().sessions);
  
  // Keep sessionsRef in sync without triggering re-renders
  useEffect(() => {
    const unsubscribe = useAppStore.subscribe(
      (state) => { sessionsRef.current = state.sessions; }
    );
    return unsubscribe;
  }, []);

  useEffect(() => {
    let mounted = true;
    const unlisteners: Array<() => void> = [];
    
    // 获取 sessionTreeStore 方法（避免闭包问题）
    const getTreeStore = () => useSessionTreeStore.getState();

    // Setup all listeners asynchronously
    const setupListeners = async () => {
      // Listen for connection status changes from backend
      try {
        const unlistenStatus = await listen<ConnectionStatusEvent>('connection_status_changed', (event) => {
          if (!mounted) return;
          const { connection_id, status, affected_children } = event.payload;
          console.log(`[ConnectionEvents] ${connection_id} -> ${status}`, { affected_children });

          // Map backend status to frontend state
          let state: SshConnectionState;
          switch (status) {
            case 'connected':
              state = 'active';
              break;
            case 'link_down':
              state = 'link_down';
              break;
            case 'reconnecting':
              // 🛑 后端不再发送 reconnecting 状态（重连引擎已删除）
              // 保留此分支以兼容可能的遗留事件
              state = 'reconnecting';
              break;
            case 'disconnected':
              state = 'disconnected';
              break;
            default:
              console.warn(`[ConnectionEvents] Unknown status: ${status}`);
              return;
          }

          updateConnectionState(connection_id, state);

          // ========== link_down 处理：前端驱动重连 ==========
          if (status === 'link_down') {
            console.log(`[ConnectionEvents] 🔴 LINK_DOWN received for connection ${connection_id}`);
            console.log(`[ConnectionEvents] topologyResolver size: ${topologyResolver.size()}`);
            
            // 1. 标记受影响的节点
            const affectedNodeIds = topologyResolver.handleLinkDown(connection_id, affected_children);
            if (affectedNodeIds.length > 0) {
              console.log(`[ConnectionEvents] Marking nodes as link-down:`, affectedNodeIds);
              getTreeStore().markLinkDownBatch(affectedNodeIds);
            } else {
              console.warn(`[ConnectionEvents] ⚠️ No affected nodes found for connection ${connection_id}`);
            }
            
            // 2. 调度防抖重连
            // 找到断开连接对应的节点
            const nodeId = topologyResolver.getNodeId(connection_id);
            console.log(`[ConnectionEvents] topologyResolver.getNodeId(${connection_id}) = ${nodeId}`);
            if (nodeId) {
              console.log(`[ConnectionEvents] ✅ Scheduling reconnect for node ${nodeId}`);
              scheduleReconnect(nodeId);
            } else {
              console.error(`[ConnectionEvents] ❌ Cannot schedule reconnect: no nodeId found for connection ${connection_id}`);
            }
            
            // 3. 中断 SFTP 传输
            const sessions = sessionsRef.current;
            sessions.forEach((session, sessionId) => {
              if (session.connectionId === connection_id) {
                interruptTransfersBySession(sessionId, i18n.t('connections.events.connection_lost_reconnecting'));
              }
            });
          }

          // ========== connected 处理：清除 link-down 标记 ==========
          if (status === 'connected') {
            const nodeId = topologyResolver.getNodeId(connection_id);
            if (nodeId) {
              console.log(`[ConnectionEvents] Clearing link-down for node ${nodeId}`);
              getTreeStore().clearLinkDown(nodeId);
              // 清除重连进度（如果有）
              getTreeStore().setReconnectProgress(nodeId, null);
            }
          }
          
          // ========== disconnected 处理：关闭相关 tabs ==========
          // 只有在彻底断开时才关闭 tabs
          // link_down 时保留 tabs，让终端进入待命模式等待自动重连
          if (status === 'disconnected') {
            const sessions = sessionsRef.current;
            const appStore = useAppStore.getState();
            const sessionIdsToClose: string[] = [];
            
            sessions.forEach((session, sessionId) => {
              if (session.connectionId === connection_id) {
                sessionIdsToClose.push(sessionId);
              }
            });
            
            if (sessionIdsToClose.length > 0) {
              console.log(`[ConnectionEvents] Connection disconnected, closing tabs for sessions:`, sessionIdsToClose);
              const sessionIdSet = new Set(sessionIdsToClose);
              const tabsToClose = appStore.tabs.filter(tab => tab.sessionId && sessionIdSet.has(tab.sessionId));
              for (const tab of tabsToClose) {
                appStore.closeTab(tab.id);
              }
            }
            
            // 中断 SFTP 传输
            const sessions2 = sessionsRef.current;
            sessions2.forEach((session, sessionId) => {
              if (session.connectionId === connection_id) {
                interruptTransfersBySession(sessionId, i18n.t('connections.events.connection_closed'));
              }
            });
          }
        });
        
        if (mounted) {
          unlisteners.push(unlistenStatus);
        } else {
          unlistenStatus();
        }
      } catch (error) {
        console.error('[ConnectionEvents] Failed to listen to connection_status_changed:', error);
      }

      // ═══════════════════════════════════════════════════════════════════════════════
      // 🛑 已移除的事件监听
      // ═══════════════════════════════════════════════════════════════════════════════
      // 
      // connection_reconnect_progress: 后端重连引擎已物理删除，不再发送此事件
      // connection_reconnected: 后端不再自主重连，所有重连由前端 reconnectCascade 驱动
      //
      // 前端通过 connectingNodeIds 状态跟踪连接进度，无需监听后端事件
      // ═══════════════════════════════════════════════════════════════════════════════
    };

    setupListeners();

    // Cleanup function with proper async handling
    return () => {
      mounted = false;
      unlisteners.forEach((unlisten) => unlisten());
      
      // 清理防抖定时器
      if (reconnectDebounceTimer) {
        clearTimeout(reconnectDebounceTimer);
        reconnectDebounceTimer = null;
      }
      pendingReconnectNodes.clear();
    };
  // Dependencies are stable: updateConnectionState and interruptTransfersBySession are selectors
  // sessionsRef is updated via subscription, not as a dependency
  }, [updateConnectionState, interruptTransfersBySession]);
}