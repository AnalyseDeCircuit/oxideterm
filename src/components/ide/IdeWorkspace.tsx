// src/components/ide/IdeWorkspace.tsx
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2 } from 'lucide-react';
import { useIdeStore, useIdeProject } from '../../store/ideStore';
import { IdeTree } from './IdeTree';
import { IdeEditorArea } from './IdeEditorArea';
import { IdeTerminal } from './IdeTerminal';
import { IdeStatusBar } from './IdeStatusBar';

interface IdeWorkspaceProps {
  connectionId: string;
  sftpSessionId: string;
  rootPath: string;
}

export function IdeWorkspace({ connectionId, sftpSessionId, rootPath }: IdeWorkspaceProps) {
  const { t } = useTranslation();
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
  
  // 初始化项目
  useEffect(() => {
    if (!project || project.rootPath !== rootPath) {
      openProject(connectionId, sftpSessionId, rootPath).catch(console.error);
    }
  }, [connectionId, sftpSessionId, rootPath, project, openProject]);
  
  // 全局快捷键
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ctrl+` 切换终端
      if (e.ctrlKey && e.key === '`') {
        e.preventDefault();
        toggleTerminal();
      }
    };
    
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [toggleTerminal]);
  
  // 加载中状态
  if (!project) {
    return (
      <div className="flex items-center justify-center h-full bg-zinc-900">
        <Loader2 className="w-8 h-8 animate-spin text-orange-500" />
        <span className="ml-3 text-zinc-400">{t('ide.loading_project')}</span>
      </div>
    );
  }
  
  return (
    <div className="flex flex-col h-full bg-zinc-900">
      {/* 主工作区 */}
      <div className="flex flex-1 overflow-hidden">
        {/* 文件树（左侧） */}
        <div 
          className="flex-shrink-0 border-r border-zinc-800 overflow-hidden"
          style={{ width: treeWidth }}
        >
          <IdeTree />
        </div>
        
        {/* 可拖拽分隔线 */}
        <div
          className="w-1 bg-zinc-800 hover:bg-orange-500/50 cursor-col-resize transition-colors"
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
                className="h-1 bg-zinc-800 hover:bg-orange-500/50 cursor-row-resize transition-colors"
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
