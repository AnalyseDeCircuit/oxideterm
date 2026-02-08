/**
 * Hook to listen for SSH connection status change events from backend
 *
 * 重连逻辑已委托给 reconnectOrchestratorStore。
 * 本 hook 仅负责：
 *   1. 监听 connection_status_changed 事件并更新 store
 *   2. link_down → 委托给 orchestrator.scheduleReconnect
 *   3. connected → 清除 link-down 标记
 *   4. disconnected → 关闭相关 tabs
 *   5. env:detected → 更新远程环境信息
 */

import { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useAppStore } from '../store/appStore';
import { useTransferStore } from '../store/transferStore';
import { useSessionTreeStore } from '../store/sessionTreeStore';
import { useReconnectOrchestratorStore } from '../store/reconnectOrchestratorStore';
import { topologyResolver } from '../lib/topologyResolver';
import { slog } from '../lib/structuredLog';
import i18n from '../i18n';
import type { SshConnectionState } from '../types';

interface ConnectionStatusEvent {
  connection_id: string;
  status: 'connected' | 'link_down' | 'reconnecting' | 'disconnected';
  affected_children: string[];  // 受影响的子连接
  timestamp: number;            // 时间戳
}

/** Event payload for env:detected */
interface EnvDetectedEvent {
  connectionId: string;
  osType: string;
  osVersion?: string;
  kernel?: string;
  arch?: string;
  shell?: string;
  detectedAt: number;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 主 Hook
// ═══════════════════════════════════════════════════════════════════════════════

export function useConnectionEvents(): void {
  // Use selectors to get stable function references
  const updateConnectionState = useAppStore((state) => state.updateConnectionState);
  const updateConnectionRemoteEnv = useAppStore((state) => state.updateConnectionRemoteEnv);
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
    
    // 获取 store 方法（避免闭包问题）
    const getTreeStore = () => useSessionTreeStore.getState();
    const getOrchestrator = () => useReconnectOrchestratorStore.getState();

    // Setup all listeners asynchronously
    const setupListeners = async () => {
      // Listen for connection status changes from backend
      try {
        const unlistenStatus = await listen<ConnectionStatusEvent>('connection_status_changed', (event) => {
          if (!mounted) return;
          const { connection_id, status, affected_children } = event.payload;
          console.log(`[ConnectionEvents] ${connection_id} -> ${status}`, { affected_children });

          // Structured log for diagnostics
          slog({
            component: 'ConnectionEvents',
            event: 'status_changed',
            connectionId: connection_id,
            detail: status,
            nodeId: topologyResolver.getNodeId(connection_id) ?? undefined,
          });

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

          // ========== link_down 处理：委托给 Orchestrator ==========
          if (status === 'link_down') {
            console.log(`[ConnectionEvents] 🔴 LINK_DOWN received for connection ${connection_id}`);
            
            // 1. 标记受影响的节点
            const affectedNodeIds = topologyResolver.handleLinkDown(connection_id, affected_children);

            slog({
              component: 'ConnectionEvents',
              event: 'link_down',
              connectionId: connection_id,
              nodeId: topologyResolver.getNodeId(connection_id) ?? undefined,
              outcome: 'ok',
              detail: `affected=${affectedNodeIds.length} children=${affected_children.length}`,
            });

            if (affectedNodeIds.length > 0) {
              getTreeStore().markLinkDownBatch(affectedNodeIds);
            }
            
            // 2. 委托给 orchestrator 调度重连
            const nodeId = topologyResolver.getNodeId(connection_id);
            if (nodeId) {
              getOrchestrator().scheduleReconnect(nodeId);
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
              getTreeStore().clearLinkDown(nodeId);
              getTreeStore().setReconnectProgress(nodeId, null);
            }
          }
          
          // ========== disconnected 处理：关闭相关 tabs ==========
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
              const sessionIdSet = new Set(sessionIdsToClose);
              const tabsToClose = appStore.tabs.filter(tab => tab.sessionId && sessionIdSet.has(tab.sessionId));
              for (const tab of tabsToClose) {
                appStore.closeTab(tab.id);
              }
            }
            
            // 中断 SFTP 传输
            sessions.forEach((session, sessionId) => {
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
      // Remote Environment Detection Event
      // ═══════════════════════════════════════════════════════════════════════════════
      try {
        const unlistenEnvDetected = await listen<EnvDetectedEvent>('env:detected', (event) => {
          if (!mounted) return;
          const { connectionId, osType, osVersion, kernel, arch, shell, detectedAt } = event.payload;
          console.log(`[ConnectionEvents] env:detected for ${connectionId}: ${osType}`);
          
          updateConnectionRemoteEnv(connectionId, {
            osType,
            osVersion,
            kernel,
            arch,
            shell,
            detectedAt,
          });
        });
        
        if (mounted) {
          unlisteners.push(unlistenEnvDetected);
        } else {
          unlistenEnvDetected();
        }
      } catch (error) {
        console.error('[ConnectionEvents] Failed to listen to env:detected:', error);
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
    };
  // Dependencies are stable: updateConnectionState, updateConnectionRemoteEnv, and interruptTransfersBySession are selectors
  // sessionsRef is updated via subscription, not as a dependency
  }, [updateConnectionState, updateConnectionRemoteEnv, interruptTransfersBySession]);
}
