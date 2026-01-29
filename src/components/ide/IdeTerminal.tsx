// src/components/ide/IdeTerminal.tsx
import { useEffect, useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Terminal, X, Loader2, RefreshCw, AlertCircle } from 'lucide-react';
import { useIdeStore } from '../../store/ideStore';
import { useIdeTerminal } from './hooks/useIdeTerminal';
import { TerminalView } from '../terminal/TerminalView';
import { Button } from '../ui/button';

interface IdeTerminalProps {
  /** 是否处于活动状态（用于 TerminalView） */
  isActive?: boolean;
}

export function IdeTerminal({ isActive = true }: IdeTerminalProps) {
  const { t } = useTranslation();
  const { terminalVisible, toggleTerminal, project } = useIdeStore();
  const { 
    terminalSessionId, 
    status, 
    error, 
    createTerminal, 
    closeTerminal,
    reset 
  } = useIdeTerminal();
  
  // 自动创建终端（当首次显示终端面板且没有会话时）
  const hasAutoCreatedRef = useRef(false);
  
  useEffect(() => {
    if (terminalVisible && !terminalSessionId && status === 'idle' && !hasAutoCreatedRef.current) {
      hasAutoCreatedRef.current = true;
      createTerminal();
    }
  }, [terminalVisible, terminalSessionId, status, createTerminal]);
  
  // 重置自动创建标记（当项目变化时）
  useEffect(() => {
    hasAutoCreatedRef.current = false;
  }, [project?.rootPath]);
  
  // 处理关闭终端面板
  const handleClose = useCallback(() => {
    toggleTerminal();
  }, [toggleTerminal]);
  
  // 处理重试
  const handleRetry = useCallback(() => {
    reset();
    createTerminal();
  }, [reset, createTerminal]);
  
  // 处理重新连接
  const handleReconnect = useCallback(async () => {
    await closeTerminal();
    reset();
    createTerminal();
  }, [closeTerminal, reset, createTerminal]);
  
  if (!terminalVisible) {
    return null;
  }
  
  return (
    <div className="h-full flex flex-col bg-zinc-950 border-t border-zinc-700/50">
      {/* 标题栏 */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-zinc-900/80 border-b border-zinc-700/50">
        <div className="flex items-center gap-2">
          <Terminal className="w-3.5 h-3.5 text-orange-400" />
          <span className="text-xs font-medium text-zinc-300">{t('ide.terminal')}</span>
          {status === 'creating' && (
            <Loader2 className="w-3 h-3 animate-spin text-zinc-500" />
          )}
          {status === 'connected' && (
            <span className="w-1.5 h-1.5 rounded-full bg-green-500" />
          )}
          {status === 'error' && (
            <span className="w-1.5 h-1.5 rounded-full bg-red-500" />
          )}
        </div>
        
        <div className="flex items-center gap-1">
          {/* 重新连接按钮 */}
          {(status === 'connected' || status === 'error') && (
            <Button
              variant="ghost"
              size="sm"
              onClick={handleReconnect}
              className="h-5 w-5 p-0 hover:bg-zinc-700/50"
              title={t('ide.terminal_reconnect')}
            >
              <RefreshCw className="w-3 h-3 text-zinc-400" />
            </Button>
          )}
          
          {/* 关闭按钮 */}
          <Button
            variant="ghost"
            size="sm"
            onClick={handleClose}
            className="h-5 w-5 p-0 hover:bg-zinc-700/50"
          >
            <X className="w-3.5 h-3.5 text-zinc-400" />
          </Button>
        </div>
      </div>
      
      {/* 终端内容 */}
      <div className="flex-1 min-h-0 relative">
        {/* 创建中 */}
        {status === 'creating' && (
          <div className="absolute inset-0 flex flex-col items-center justify-center bg-zinc-950">
            <Loader2 className="w-6 h-6 animate-spin text-zinc-500 mb-2" />
            <span className="text-xs text-zinc-500">{t('ide.creating_terminal')}</span>
          </div>
        )}
        
        {/* 错误状态 */}
        {status === 'error' && (
          <div className="absolute inset-0 flex flex-col items-center justify-center bg-zinc-950">
            <AlertCircle className="w-6 h-6 text-red-400 mb-2" />
            <span className="text-xs text-red-400 mb-2">{t('ide.terminal_error')}</span>
            {error && (
              <span className="text-xs text-zinc-500 max-w-xs text-center truncate mb-3">
                {error}
              </span>
            )}
            <Button
              variant="outline"
              size="sm"
              onClick={handleRetry}
              className="text-xs"
            >
              {t('ide.retry')}
            </Button>
          </div>
        )}
        
        {/* 空闲状态（等待创建） */}
        {status === 'idle' && !terminalSessionId && (
          <div className="absolute inset-0 flex flex-col items-center justify-center bg-zinc-950">
            <Terminal className="w-8 h-8 text-zinc-600 mb-2" />
            <Button
              variant="outline"
              size="sm"
              onClick={createTerminal}
              className="text-xs"
            >
              {t('ide.open_terminal')}
            </Button>
          </div>
        )}
        
        {/* 终端视图 */}
        {terminalSessionId && status === 'connected' && (
          <TerminalView
            sessionId={terminalSessionId}
            isActive={isActive && terminalVisible}
            paneId={`ide-terminal-${terminalSessionId}`}
          />
        )}
      </div>
    </div>
  );
}
