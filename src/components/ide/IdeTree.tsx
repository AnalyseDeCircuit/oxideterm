// src/components/ide/IdeTree.tsx
import { useTranslation } from 'react-i18next';
import { Folder } from 'lucide-react';
import { useIdeProject } from '../../store/ideStore';

export function IdeTree() {
  const { t } = useTranslation();
  const project = useIdeProject();
  
  if (!project) {
    return <div className="p-4 text-zinc-500">{t('ide.no_project')}</div>;
  }
  
  return (
    <div className="h-full flex flex-col bg-zinc-900">
      {/* 项目标题 */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-zinc-800">
        <Folder className="w-4 h-4 text-orange-500" />
        <span className="text-sm font-medium truncate">{project.name}</span>
        {project.isGitRepo && project.gitBranch && (
          <span className="text-xs text-zinc-500 ml-auto">{project.gitBranch}</span>
        )}
      </div>
      
      {/* 文件列表（Phase 1 占位） */}
      <div className="flex-1 p-4 text-zinc-500 text-sm">
        {t('ide.file_tree_placeholder')}
      </div>
    </div>
  );
}
