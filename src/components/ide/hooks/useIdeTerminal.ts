// src/components/ide/hooks/useIdeTerminal.ts
import { useState, useCallback, useRef } from 'react';
import { api } from '../../../lib/api';
import { useIdeStore } from '../../../store/ideStore';
import { useSessionTreeStore } from '../../../store/sessionTreeStore';
import { CreateTerminalResponse } from '../../../types';

export type TerminalStatus = 'idle' | 'creating' | 'connected' | 'error' | 'closed';

interface UseIdeTerminalResult {
  /** 终端会话 ID（用于 TerminalView） */
  terminalSessionId: string | null;
  /** WebSocket URL */
  wsUrl: string | null;
  /** WebSocket Token */
  wsToken: string | null;
  /** 终端状态 */
  status: TerminalStatus;
  /** 错误信息 */
  error: string | null;
  /** 创建终端 */
  createTerminal: () => Promise<void>;
  /** 关闭终端 */
  closeTerminal: () => Promise<void>;
  /** 重置状态（用于重试） */
  reset: () => void;
}

export function useIdeTerminal(): UseIdeTerminalResult {
  const { nodeId, terminalSessionId, setTerminalSession } = useIdeStore();
  
  const [wsUrl, setWsUrl] = useState<string | null>(null);
  const [wsToken, setWsToken] = useState<string | null>(null);
  const [status, setStatus] = useState<TerminalStatus>('idle');
  const [error, setError] = useState<string | null>(null);
  
  // 跟踪是否已创建，避免重复创建
  const creatingRef = useRef(false);
  
  // 创建终端
  const createTerminal = useCallback(async () => {
    if (!nodeId) {
      setError('No node ID');
      setStatus('error');
      return;
    }
    
    // Phase 4 bridge: resolve nodeId → connectionId via sessionTreeStore
    const treeNode = useSessionTreeStore.getState().getNode(nodeId);
    const connectionId = treeNode?.runtime.connectionId;
    if (!connectionId) {
      setError('Node not connected');
      setStatus('error');
      return;
    }
    
    if (creatingRef.current) {
      return;
    }
    
    creatingRef.current = true;
    setStatus('creating');
    setError(null);
    
    try {
      const response: CreateTerminalResponse = await api.createTerminal({
        connectionId,
        cols: 80,
        rows: 24,
      });
      
      setTerminalSession(response.sessionId);
      setWsUrl(response.wsUrl);
      setWsToken(response.wsToken);
      setStatus('connected');
      
      console.log('[useIdeTerminal] Terminal created:', response.sessionId);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      console.error('[useIdeTerminal] Failed to create terminal:', message);
      setError(message);
      setStatus('error');
    } finally {
      creatingRef.current = false;
    }
  }, [nodeId, setTerminalSession]);
  
  // 关闭终端
  const closeTerminal = useCallback(async () => {
    if (!terminalSessionId) return;
    
    try {
      await api.closeTerminal(terminalSessionId);
      console.log('[useIdeTerminal] Terminal closed:', terminalSessionId);
    } catch (e) {
      console.error('[useIdeTerminal] Failed to close terminal:', e);
    } finally {
      setTerminalSession(null);
      setWsUrl(null);
      setWsToken(null);
      setStatus('closed');
    }
  }, [terminalSessionId, setTerminalSession]);
  
  // 重置状态
  const reset = useCallback(() => {
    setStatus('idle');
    setError(null);
    setWsUrl(null);
    setWsToken(null);
    creatingRef.current = false;
  }, []);
  
  // 组件卸载时不自动关闭终端，因为可能只是隐藏面板
  // 终端的清理由 IdeWorkspace 或项目关闭时处理
  
  return {
    terminalSessionId,
    wsUrl,
    wsToken,
    status,
    error,
    createTerminal,
    closeTerminal,
    reset,
  };
}
