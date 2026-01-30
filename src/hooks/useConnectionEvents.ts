/**
 * Hook to listen for SSH connection status change events from backend
 * 
 * 统一事件系统 (Phase 2 重构)
 * 
 * Events:
 * - connection_status_changed: { connection_id, status, affected_children, timestamp }
 * - connection_reconnect_progress: { connection_id, attempt, max_attempts, next_retry_ms, timestamp }
 * - connection_reconnected: { connection_id, terminal_ids, forward_ids }
 */

import { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useAppStore } from '../store/appStore';
import { useTransferStore } from '../store/transferStore';
import { useSessionTreeStore, type ReconnectProgress } from '../store/sessionTreeStore';
import { topologyResolver } from '../lib/topologyResolver';
import { api } from '../lib/api';
import i18n from '../i18n';
import type { SshConnectionState } from '../types';

interface ConnectionStatusEvent {
  connection_id: string;
  status: 'connected' | 'link_down' | 'reconnecting' | 'disconnected';
  affected_children: string[];  // 新增：受影响的子连接
  timestamp: number;            // 新增：时间戳
}

interface ConnectionReconnectProgressEvent {
  connection_id: string;
  attempt: number;
  max_attempts: number | null;
  next_retry_ms: number;
  timestamp: number;
}

interface ConnectionReconnectedEvent {
  connection_id: string;
  terminal_ids: string[];
  forward_ids: string[];
}

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

          // 使用拓扑解析器处理 link-down 级联
          if (status === 'link_down') {
            const affectedNodeIds = topologyResolver.handleLinkDown(connection_id, affected_children);
            if (affectedNodeIds.length > 0) {
              console.log(`[ConnectionEvents] Marking nodes as link-down:`, affectedNodeIds);
              getTreeStore().markLinkDownBatch(affectedNodeIds);
            }
          }

          // 重连成功时清除 link-down 标记
          if (status === 'connected') {
            const nodeId = topologyResolver.getNodeId(connection_id);
            if (nodeId) {
              console.log(`[ConnectionEvents] Clearing link-down for node ${nodeId}`);
              getTreeStore().clearLinkDown(nodeId);
              // 清除重连进度
              getTreeStore().setReconnectProgress(nodeId, null);
            }
          }

          // When connection goes down, interrupt all SFTP transfers for related sessions
          if (status === 'link_down' || status === 'disconnected') {
            const sessions = sessionsRef.current;
            sessions.forEach((session, sessionId) => {
              if (session.connectionId === connection_id) {
                interruptTransfersBySession(sessionId, 
                  status === 'link_down' ? i18n.t('connections.events.connection_lost_reconnecting') : i18n.t('connections.events.connection_closed')
                );
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

      // Listen for reconnect progress events
      try {
        const unlistenProgress = await listen<ConnectionReconnectProgressEvent>('connection_reconnect_progress', (event) => {
          if (!mounted) return;
          const { connection_id, attempt, max_attempts, next_retry_ms } = event.payload;
          console.log(`[ConnectionEvents] Reconnect progress: ${connection_id} attempt ${attempt}/${max_attempts ?? '∞'}`);

          // 通过拓扑解析器找到对应节点
          const nodeId = topologyResolver.getNodeId(connection_id);
          if (nodeId) {
            const progress: ReconnectProgress = {
              attempt,
              maxAttempts: max_attempts,
              nextRetryMs: next_retry_ms,
            };
            getTreeStore().setReconnectProgress(nodeId, progress);
          }
        });

        if (mounted) {
          unlisteners.push(unlistenProgress);
        } else {
          unlistenProgress();
        }
      } catch (error) {
        console.error('[ConnectionEvents] Failed to listen to connection_reconnect_progress:', error);
      }

      // Listen for connection reconnected events from backend
      try {
        const unlistenReconnected = await listen<ConnectionReconnectedEvent>('connection_reconnected', (event) => {
          if (!mounted) return;
          const { connection_id, terminal_ids, forward_ids } = event.payload;
          console.log(`[ConnectionEvents] Connection ${connection_id} reconnected`, {
            terminal_ids,
            forward_ids,
          });

          // Update connection state to active
          updateConnectionState(connection_id, 'active');

          // 通过拓扑解析器找到对应节点并清除 link-down
          const nodeId = topologyResolver.getNodeId(connection_id);
          if (nodeId) {
            console.log(`[ConnectionEvents] Clearing link-down for node ${nodeId}`);
            getTreeStore().clearLinkDown(nodeId);
            getTreeStore().setReconnectProgress(nodeId, null);
          }

          // 恢复终端 WebSocket 连接
          if (terminal_ids.length > 0) {
            console.log(`[ConnectionEvents] Restoring terminals:`, terminal_ids);
            restoreTerminalConnections(terminal_ids);
          }

          // 恢复端口转发
          if (forward_ids.length > 0) {
            console.log(`[ConnectionEvents] Restoring port forwards:`, forward_ids);
            restorePortForwards(connection_id, forward_ids);
          }
        });

        if (mounted) {
          unlisteners.push(unlistenReconnected);
        } else {
          unlistenReconnected();
        }
      } catch (error) {
        console.error('[ConnectionEvents] Failed to listen to connection_reconnected:', error);
      }
    };

    setupListeners();

    // Cleanup function with proper async handling
    return () => {
      mounted = false;
      unlisteners.forEach((unlisten) => unlisten());
    };
  // Dependencies are stable: updateConnectionState and interruptTransfersBySession are selectors
  // sessionsRef is updated via subscription, not as a dependency
  }, [updateConnectionState, interruptTransfersBySession]);
}

/**
 * 恢复终端 WebSocket 连接
 * 
 * 原理：
 * 1. 调用后端 recreate_terminal_pty 为每个终端重建 PTY 和 WebSocket bridge
 * 2. 更新 appStore.sessions 中的 ws_url
 * 3. TerminalView 监听 session.ws_url 变化，自动重连
 */
async function restoreTerminalConnections(terminalIds: string[]): Promise<void> {
  const appStore = useAppStore.getState();
  const MAX_RETRIES = 3;
  const RETRY_DELAY = 500;

  for (const terminalId of terminalIds) {
    const session = appStore.sessions.get(terminalId);
    if (!session) {
      console.warn(`[ConnectionEvents] Session ${terminalId} not found, skipping restore`);
      continue;
    }

    let lastError: unknown = null;
    for (let attempt = 1; attempt <= MAX_RETRIES; attempt++) {
      try {
        console.log(`[ConnectionEvents] Recreating PTY for terminal ${terminalId} (attempt ${attempt}/${MAX_RETRIES})`);
        
        // 调用后端重建 PTY 并获取新的 WebSocket 信息
        const result = await api.recreateTerminalPty(terminalId);

        // 直接更新 sessions Map 中的 ws_url 和 ws_token
        // 这会触发 TerminalView 的 useEffect 重连
        useAppStore.setState((state) => {
          const newSessions = new Map(state.sessions);
          const existingSession = newSessions.get(terminalId);
          if (existingSession) {
            newSessions.set(terminalId, {
              ...existingSession,
              ws_url: result.wsUrl,
              ws_token: result.wsToken,
            });
          }
          return { sessions: newSessions };
        });

        console.log(`[ConnectionEvents] Terminal ${terminalId} PTY recreated, new ws_url: ${result.wsUrl}`);
        lastError = null;
        break; // Success, exit retry loop
      } catch (e) {
        lastError = e;
        console.warn(`[ConnectionEvents] Failed to restore terminal ${terminalId} (attempt ${attempt}/${MAX_RETRIES}):`, e);
        
        if (attempt < MAX_RETRIES) {
          // Wait before retrying
          await new Promise(resolve => setTimeout(resolve, RETRY_DELAY * attempt));
        }
      }
    }
    
    if (lastError) {
      console.error(`[ConnectionEvents] Failed to restore terminal ${terminalId} after ${MAX_RETRIES} attempts:`, lastError);
    }
  }
}
/**
 * 恢复端口转发规则
 * 
 * 后端在重连时会自动恢复端口转发配置，但前端需要：
 * 1. 刷新端口转发列表以获取最新状态
 * 2. 更新 UI 显示
 * 
 * @param connectionId - 重连成功的连接 ID
 * @param forwardIds - 需要恢复的转发规则 ID 列表
 */
async function restorePortForwards(connectionId: string, forwardIds: string[]): Promise<void> {
  console.log(`[ConnectionEvents] Restoring ${forwardIds.length} port forwards for connection ${connectionId}`);
  
  // 找到使用此 connectionId 的所有会话
  const appStore = useAppStore.getState();
  const sessionIds: string[] = [];
  
  appStore.sessions.forEach((session, sessionId) => {
    if (session.connectionId === connectionId) {
      sessionIds.push(sessionId);
    }
  });

  if (sessionIds.length === 0) {
    console.warn(`[ConnectionEvents] No sessions found for connection ${connectionId}`);
    return;
  }

  // 对每个会话刷新端口转发列表
  for (const sessionId of sessionIds) {
    try {
      // 后端已经自动恢复了端口转发，这里只需刷新前端状态
      const forwards = await api.listPortForwards(sessionId);
      console.log(`[ConnectionEvents] Session ${sessionId} has ${forwards.length} port forwards after restore`);
      
      // 如果需要，可以在这里发送通知告知用户端口转发已恢复
      const activeForwards = forwards.filter(f => f.status === 'active');
      if (activeForwards.length > 0) {
        console.log(`[ConnectionEvents] ${activeForwards.length} port forwards are now active for session ${sessionId}`);
      }
    } catch (e) {
      console.error(`[ConnectionEvents] Failed to refresh port forwards for session ${sessionId}:`, e);
    }
  }
}