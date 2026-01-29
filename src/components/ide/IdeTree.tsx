// src/components/ide/IdeTree.tsx
import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  Folder, 
  FolderOpen,
  ChevronRight,
  ChevronDown,
  RefreshCw,
  AlertCircle,
  Loader2,
} from 'lucide-react';
import { api } from '../../lib/api';
import { useIdeStore, useIdeProject } from '../../store/ideStore';
import { cn } from '../../lib/utils';
import { FileInfo } from '../../types';
import { Button } from '../ui/button';

// 判断文件是否为目录
function isDirectory(file: FileInfo): boolean {
  return file.file_type === 'Directory';
}

// 文件图标映射（基于扩展名）
const FILE_ICONS: Record<string, string> = {
  ts: '📘', tsx: '📘', js: '📙', jsx: '📙',
  rs: '🦀', py: '🐍', go: '🐹', rb: '💎',
  json: '📋', yaml: '📋', yml: '📋', toml: '📋',
  md: '📝', txt: '📄', html: '🌐', css: '🎨',
  scss: '🎨', less: '🎨', svg: '🖼️', png: '🖼️',
  jpg: '🖼️', jpeg: '🖼️', gif: '🖼️', webp: '🖼️',
  sh: '📜', bash: '📜', zsh: '📜', fish: '📜',
  dockerfile: '🐳', gitignore: '🙈', lock: '🔒',
  vue: '💚', svelte: '🧡', astro: '🚀',
};

function getFileIcon(name: string): string {
  const lowerName = name.toLowerCase();
  
  // 特殊文件名匹配
  if (lowerName === 'dockerfile') return '🐳';
  if (lowerName === '.gitignore') return '🙈';
  if (lowerName === 'cargo.toml') return '📦';
  if (lowerName === 'package.json') return '📦';
  if (lowerName.endsWith('.lock')) return '🔒';
  
  // 扩展名匹配
  const ext = name.includes('.') ? name.split('.').pop()?.toLowerCase() || '' : '';
  return FILE_ICONS[ext] || '📄';
}

// 排序：目录优先，然后按名称字母顺序
function sortFiles(files: FileInfo[]): FileInfo[] {
  return [...files].sort((a, b) => {
    const aIsDir = isDirectory(a);
    const bIsDir = isDirectory(b);
    if (aIsDir !== bIsDir) {
      return aIsDir ? -1 : 1;
    }
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
  });
}

// 单个树节点
interface TreeNodeProps {
  file: FileInfo;
  depth: number;
  sftpSessionId: string;
  parentPath: string;
}

function TreeNode({ file, depth, sftpSessionId, parentPath }: TreeNodeProps) {
  const { expandedPaths, togglePath, openFile, tabs } = useIdeStore();
  const [children, setChildren] = useState<FileInfo[] | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const isDir = isDirectory(file);
  const fullPath = parentPath === '/' 
    ? `/${file.name}` 
    : `${parentPath}/${file.name}`;
  const isExpanded = expandedPaths.has(fullPath);
  const isOpen = tabs.some(t => t.path === fullPath);
  
  // 加载子目录内容
  const loadChildren = useCallback(async () => {
    if (!isDir || children !== null) return;
    
    setIsLoading(true);
    setError(null);
    
    try {
      const result = await api.sftpListDir(sftpSessionId, fullPath);
      setChildren(sortFiles(result));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsLoading(false);
    }
  }, [isDir, fullPath, sftpSessionId, children]);
  
  // 展开时加载子目录
  useEffect(() => {
    if (isExpanded && isDir && children === null) {
      loadChildren();
    }
  }, [isExpanded, isDir, children, loadChildren]);
  
  // 点击处理
  const handleClick = useCallback(() => {
    if (isDir) {
      togglePath(fullPath);
    } else {
      openFile(fullPath).catch(console.error);
    }
  }, [isDir, fullPath, togglePath, openFile]);
  
  // 双击处理（文件打开）
  const handleDoubleClick = useCallback(() => {
    if (!isDir) {
      openFile(fullPath).catch(console.error);
    }
  }, [isDir, fullPath, openFile]);
  
  return (
    <div>
      {/* 节点本身 */}
      <div
        className={cn(
          'flex items-center gap-1 py-0.5 px-1 cursor-pointer rounded-sm',
          'hover:bg-zinc-700/50 transition-colors',
          isOpen && 'bg-orange-500/10 text-orange-400'
        )}
        style={{ paddingLeft: `${depth * 12 + 4}px` }}
        onClick={handleClick}
        onDoubleClick={handleDoubleClick}
      >
        {/* 展开/折叠箭头 */}
        <span className="w-4 h-4 flex items-center justify-center flex-shrink-0">
          {isDir ? (
            isLoading ? (
              <Loader2 className="w-3 h-3 animate-spin text-zinc-500" />
            ) : isExpanded ? (
              <ChevronDown className="w-3.5 h-3.5 text-zinc-500" />
            ) : (
              <ChevronRight className="w-3.5 h-3.5 text-zinc-500" />
            )
          ) : null}
        </span>
        
        {/* 图标 */}
        <span className="w-4 h-4 flex items-center justify-center flex-shrink-0 text-sm">
          {isDir ? (
            isExpanded ? (
              <FolderOpen className="w-4 h-4 text-orange-400" />
            ) : (
              <Folder className="w-4 h-4 text-zinc-400" />
            )
          ) : (
            <span>{getFileIcon(file.name)}</span>
          )}
        </span>
        
        {/* 文件名 */}
        <span className={cn(
          'truncate text-xs',
          isDir ? 'text-zinc-300' : 'text-zinc-400',
          isOpen && 'text-orange-400 font-medium'
        )}>
          {file.name}
        </span>
      </div>
      
      {/* 子节点 */}
      {isDir && isExpanded && (
        <div>
          {error ? (
            <div 
              className="flex items-center gap-1 py-1 text-xs text-red-400"
              style={{ paddingLeft: `${(depth + 1) * 12 + 4}px` }}
            >
              <AlertCircle className="w-3 h-3" />
              <span className="truncate">{error}</span>
            </div>
          ) : children?.map(child => (
            <TreeNode
              key={child.name}
              file={child}
              depth={depth + 1}
              sftpSessionId={sftpSessionId}
              parentPath={fullPath}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function IdeTree() {
  const { t } = useTranslation();
  const project = useIdeProject();
  const { sftpSessionId, expandedPaths } = useIdeStore();
  const [rootFiles, setRootFiles] = useState<FileInfo[] | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  // 加载根目录
  const loadRoot = useCallback(async () => {
    if (!project || !sftpSessionId) return;
    
    setIsLoading(true);
    setError(null);
    
    try {
      const result = await api.sftpListDir(sftpSessionId, project.rootPath);
      setRootFiles(sortFiles(result));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsLoading(false);
    }
  }, [project, sftpSessionId]);
  
  // 初始加载
  useEffect(() => {
    if (project && sftpSessionId && expandedPaths.has(project.rootPath)) {
      loadRoot();
    }
  }, [project, sftpSessionId, expandedPaths, loadRoot]);
  
  // 刷新
  const handleRefresh = useCallback(() => {
    setRootFiles(null);
    loadRoot();
  }, [loadRoot]);
  
  if (!project) {
    return (
      <div className="h-full flex items-center justify-center p-4">
        <p className="text-xs text-zinc-500">{t('ide.no_project')}</p>
      </div>
    );
  }
  
  return (
    <div className="h-full flex flex-col bg-zinc-900/50">
      {/* 标题栏 */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-700/50">
        <div className="flex items-center gap-2 min-w-0">
          <Folder className="w-4 h-4 text-orange-400 flex-shrink-0" />
          <span className="text-xs font-medium text-zinc-300 truncate">
            {project.name}
          </span>
          {project.isGitRepo && project.gitBranch && (
            <span className="text-[10px] text-zinc-500 truncate ml-1">
              ({project.gitBranch})
            </span>
          )}
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={handleRefresh}
          disabled={isLoading}
          className="h-6 w-6 p-0 hover:bg-zinc-700/50"
        >
          <RefreshCw className={cn('w-3.5 h-3.5 text-zinc-400', isLoading && 'animate-spin')} />
        </Button>
      </div>
      
      {/* 文件树 */}
      <div className="flex-1 overflow-auto py-1">
        {isLoading && rootFiles === null ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="w-5 h-5 animate-spin text-zinc-500" />
          </div>
        ) : error ? (
          <div className="flex flex-col items-center justify-center gap-2 py-8 px-4">
            <AlertCircle className="w-5 h-5 text-red-400" />
            <p className="text-xs text-red-400 text-center">{error}</p>
            <Button
              variant="ghost"
              size="sm"
              onClick={handleRefresh}
              className="text-xs"
            >
              {t('ide.retry')}
            </Button>
          </div>
        ) : rootFiles?.map(file => (
          <TreeNode
            key={file.name}
            file={file}
            depth={0}
            sftpSessionId={sftpSessionId!}
            parentPath={project.rootPath}
          />
        ))}
      </div>
    </div>
  );
}
