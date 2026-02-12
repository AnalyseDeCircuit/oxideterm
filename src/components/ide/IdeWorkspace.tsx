// src/components/ide/IdeWorkspace.tsx
import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, AlertTriangle, RefreshCw } from 'lucide-react';
import { useIdeStore, useIdeProject } from '../../store/ideStore';
import { useTabBgActive } from '../../hooks/useTabBackground';
import { IdeTree } from './IdeTree';
import { IdeEditorArea } from './IdeEditorArea';
import { IdeTerminal } from './IdeTerminal';
import { IdeStatusBar } from './IdeStatusBar';
import { IdeSearchPanel } from './IdeSearchPanel';

interface IdeWorkspaceProps {
  nodeId: string;
  rootPath: string;
}

export function IdeWorkspace({ nodeId, rootPath }: IdeWorkspaceProps) {
  const { t } = useTranslation();
  const bgActive = useTabBgActive('ide');
  const project = useIdeProject();

  const { 
    openProject, 
    treeWidth, 
    terminalVisible, 
    terminalHeight,
    setTreeWidth,
    setTerminalHeight,
    toggleTerminal,
  } = useIdeStore();
  
  // 搜索面板状态
  const [searchOpen, setSearchOpen] = useState(false);
  // 初始化错误状态
  const [initError, setInitError] = useState<string | null>(null);
  const [retryTick, setRetryTick] = useState(0);
  
  // 切换搜索面板
  const toggleSearch = useCallback(() => {
    setSearchOpen(prev => !prev);
  }, []);
  
  // 初始化项目 — nodeId 变化触发重连后重建
  useEffect(() => {
    const needsOpen = !project || 
      (useIdeStore.getState().nodeId !== nodeId);
    
    if (needsOpen) {
      setInitError(null);
      openProject(nodeId, rootPath)
        .then(() => setInitError(null))
        .catch((err) => {
          console.error('[IdeWorkspace] openProject failed:', err);
          setInitError(err instanceof Error ? err.message : String(err));
        });
    }
  }, [nodeId, rootPath, retryTick]);
  
  // 全局快捷键
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ctrl+` 切换终端
      if (e.ctrlKey && e.key === '`') {
        e.preventDefault();
        toggleTerminal();
      }
      
      // Cmd/Ctrl+Shift+F 切换搜索面板
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        toggleSearch();
      }
    };
    
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [toggleTerminal, toggleSearch]);
  
  // 加载中状态
  if (!project && !initError) {
    return (
      <div className={`flex items-center justify-center h-full ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
        <Loader2 className="w-8 h-8 animate-spin text-theme-accent" />
        <span className="ml-3 text-theme-text-muted">
          {t('ide.loading_project')}
        </span>
      </div>
    );
  }

  // 初始化失败状态
  if (initError && !project) {
    return (
      <div className={`flex flex-col items-center justify-center h-full gap-4 ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
        <AlertTriangle className="w-10 h-10 text-amber-400" />
        <span className="text-theme-text-muted text-sm max-w-md text-center">
          {t('ide.open_failed', 'Failed to open project')}: {initError}
        </span>
        <button
          onClick={() => setRetryTick((v) => v + 1)}
          className="flex items-center gap-2 px-4 py-2 rounded bg-theme-accent text-white hover:bg-theme-accent/80 transition-colors text-sm"
        >
          <RefreshCw className="w-4 h-4" />
          {t('ide.retry', 'Retry')}
        </button>
      </div>
    );
  }
  
  return (
    <div className={`flex flex-col h-full ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
      {/* 主工作区 */}
      <div className="flex flex-1 overflow-hidden">
        {/* 搜索面板（最左侧，可选） */}
        <IdeSearchPanel open={searchOpen} onClose={() => setSearchOpen(false)} />
        
        {/* 文件树（左侧） */}
        <div 
          className="flex-shrink-0 border-r border-theme-border overflow-hidden"
          style={{ width: treeWidth }}
        >
          <IdeTree />
        </div>
        
        {/* 可拖拽分隔线 */}
        <div
          className="w-1 bg-theme-border hover:bg-theme-accent/50 cursor-col-resize transition-colors"
          onMouseDown={(e) => {
            e.preventDefault();
            const startX = e.clientX;
            const startWidth = treeWidth;
            
            const onMouseMove = (e: MouseEvent) => {
              const delta = e.clientX - startX;
              const newWidth = Math.max(200, Math.min(500, startWidth + delta));
              setTreeWidth(newWidth);
            };
            
            const onMouseUp = () => {
              document.removeEventListener('mousemove', onMouseMove);
              document.removeEventListener('mouseup', onMouseUp);
            };
            
            document.addEventListener('mousemove', onMouseMove);
            document.addEventListener('mouseup', onMouseUp);
          }}
        />
        
        {/* 编辑器区域（右侧） */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <IdeEditorArea />
          
          {/* 终端面板（底部） */}
          {terminalVisible && (
            <>
              {/* 可拖拽分隔线 */}
              <div
                className="h-1 bg-theme-border hover:bg-theme-accent/50 cursor-row-resize transition-colors"
                onMouseDown={(e) => {
                  e.preventDefault();
                  const startY = e.clientY;
                  const startHeight = terminalHeight;
                  
                  const onMouseMove = (e: MouseEvent) => {
                    const delta = startY - e.clientY;
                    const newHeight = Math.max(100, Math.min(400, startHeight + delta));
                    setTerminalHeight(newHeight);
                  };
                  
                  const onMouseUp = () => {
                    document.removeEventListener('mousemove', onMouseMove);
                    document.removeEventListener('mouseup', onMouseUp);
                  };
                  
                  document.addEventListener('mousemove', onMouseMove);
                  document.addEventListener('mouseup', onMouseUp);
                }}
              />
              <div style={{ height: terminalHeight }}>
                <IdeTerminal />
              </div>
            </>
          )}
        </div>
      </div>
      
      {/* 状态栏 */}
      <IdeStatusBar />
    </div>
  );
}
