// src/components/ide/IdeStatusBar.tsx
import { useIdeProject, useIdeActiveTab, useIdeDirtyCount } from '../../store/ideStore';
import { GitBranch } from 'lucide-react';

export function IdeStatusBar() {
  const project = useIdeProject();
  const activeTab = useIdeActiveTab();
  const dirtyCount = useIdeDirtyCount();
  
  return (
    <div className="h-6 bg-zinc-800 border-t border-zinc-700 flex items-center px-3 text-xs text-zinc-400">
      {/* Git 分支 */}
      {project?.isGitRepo && project.gitBranch && (
        <div className="flex items-center gap-1 mr-4">
          <GitBranch className="w-3 h-3" />
          <span>{project.gitBranch}</span>
        </div>
      )}
      
      {/* 光标位置 */}
      {activeTab?.cursor && (
        <span className="mr-4">
          Ln {activeTab.cursor.line}, Col {activeTab.cursor.col}
        </span>
      )}
      
      {/* 语言 */}
      {activeTab && (
        <span className="mr-4">{activeTab.language}</span>
      )}
      
      {/* 未保存文件数 */}
      {dirtyCount > 0 && (
        <span className="ml-auto text-orange-500">
          {dirtyCount} unsaved
        </span>
      )}
    </div>
  );
}
