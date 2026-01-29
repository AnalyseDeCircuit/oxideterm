// src/components/ide/IdeTree.tsx
import { useState, useEffect, useCallback, createContext, useContext } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  ChevronRight,
  ChevronDown,
  RefreshCw,
  AlertCircle,
  Loader2,
  GitBranch,
  Folder,
} from 'lucide-react';
import { api } from '../../lib/api';
import { useIdeStore, useIdeProject } from '../../store/ideStore';
import { cn } from '../../lib/utils';
import { FileIcon, FolderIcon } from '../../lib/fileIcons';
import { FileInfo } from '../../types';
import { Button } from '../ui/button';
import { 
  useGitStatus, 
  GitFileStatus, 
  GIT_STATUS_COLORS, 
  GIT_STATUS_LABELS 
} from './hooks/useGitStatus';

// ═══════════════════════════════════════════════════════════════════════════
// Git 状态 Context（避免在每个节点中调用 hook）
// ═══════════════════════════════════════════════════════════════════════════
interface GitStatusContextValue {
  getFileStatus: (relativePath: string) => GitFileStatus | undefined;
  projectRootPath: string;
}

const GitStatusContext = createContext<GitStatusContextValue | null>(null);

function useGitStatusContext() {
  return useContext(GitStatusContext);
}

// 判断文件是否为目录
function isDirectory(file: FileInfo): boolean {
  return file.file_type === 'Directory';
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
  const gitStatusCtx = useGitStatusContext();
  const [children, setChildren] = useState<FileInfo[] | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const isDir = isDirectory(file);
  const fullPath = parentPath === '/' 
    ? `/${file.name}` 
    : `${parentPath}/${file.name}`;
  const isExpanded = expandedPaths.has(fullPath);
  const isOpen = tabs.some(t => t.path === fullPath);
  
  // 计算相对于项目根目录的路径（用于 Git 状态查询）
  const relativePath = gitStatusCtx 
    ? fullPath.startsWith(gitStatusCtx.projectRootPath)
      ? fullPath.substring(gitStatusCtx.projectRootPath.length + 1) // 移除根路径和前导斜杠
      : file.name
    : '';
  const gitStatus = gitStatusCtx?.getFileStatus(relativePath);
  
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
          'hover:bg-theme-bg-hover/50 transition-colors',
          isOpen && 'bg-theme-accent/10 text-theme-accent'
        )}
        style={{ paddingLeft: `${depth * 12 + 4}px` }}
        onClick={handleClick}
        onDoubleClick={handleDoubleClick}
      >
        {/* 展开/折叠箭头 */}
        <span className="w-4 h-4 flex items-center justify-center flex-shrink-0">
          {isDir ? (
            isLoading ? (
              <Loader2 className="w-3 h-3 animate-spin text-theme-text-muted" />
            ) : isExpanded ? (
              <ChevronDown className="w-3.5 h-3.5 text-theme-text-muted" />
            ) : (
              <ChevronRight className="w-3.5 h-3.5 text-theme-text-muted" />
            )
          ) : null}
        </span>
        
        {/* 图标 */}
        <span className="w-4 h-4 flex items-center justify-center flex-shrink-0">
          {isDir ? (
            <FolderIcon isOpen={isExpanded} size={16} />
          ) : (
            <FileIcon 
              filename={file.name} 
              size={14}
              // Git 状态颜色覆盖默认颜色
              overrideColor={gitStatus ? GIT_STATUS_COLORS[gitStatus] : undefined}
            />
          )}
        </span>
        
        {/* 文件名 */}
        <span className={cn(
          'truncate text-xs flex-1',
          isDir ? 'text-theme-text' : 'text-theme-text-muted',
          isOpen && 'text-theme-accent font-medium',
          // Git 状态颜色（仅对未打开的文件名生效）
          !isOpen && gitStatus && GIT_STATUS_COLORS[gitStatus]
        )}>
          {file.name}
        </span>
        
        {/* Git 状态指示器 */}
        {gitStatus && gitStatus !== 'ignored' && (
          <span className={cn(
            'text-[10px] mr-1 font-mono',
            GIT_STATUS_COLORS[gitStatus]
          )}>
            {GIT_STATUS_LABELS[gitStatus]}
          </span>
        )}
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
  const { status: gitStatus, getFileStatus, refresh: refreshGit, isLoading: gitLoading } = useGitStatus();
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
  
  // 刷新（同时刷新文件列表和 Git 状态）
  const handleRefresh = useCallback(() => {
    setRootFiles(null);
    loadRoot();
    refreshGit();
  }, [loadRoot, refreshGit]);
  
  // Git 状态上下文值
  const gitStatusContextValue: GitStatusContextValue | null = project ? {
    getFileStatus,
    projectRootPath: project.rootPath,
  } : null;
  
  if (!project) {
    return (
      <div className="h-full flex items-center justify-center p-4">
        <p className="text-xs text-theme-text-muted">{t('ide.no_project')}</p>
      </div>
    );
  }
  
  return (
    <GitStatusContext.Provider value={gitStatusContextValue}>
      <div className="h-full flex flex-col bg-theme-bg/50">
        {/* 标题栏 */}
        <div className="flex items-center justify-between px-3 py-2 border-b border-theme-border/50">
          <div className="flex items-center gap-2 min-w-0">
            <Folder className="w-4 h-4 text-theme-accent flex-shrink-0" />
            <span className="text-xs font-medium text-theme-text truncate">
              {project.name}
            </span>
            {/* Git 分支信息 */}
            {project.isGitRepo && gitStatus && (
              <span className="flex items-center gap-1 text-[10px] text-theme-text-muted truncate ml-1">
                <GitBranch className="w-3 h-3" />
                {gitStatus.branch}
                {(gitStatus.ahead > 0 || gitStatus.behind > 0) && (
                  <span className="opacity-60">
                    {gitStatus.ahead > 0 && `↑${gitStatus.ahead}`}
                    {gitStatus.behind > 0 && `↓${gitStatus.behind}`}
                  </span>
                )}
              </span>
            )}
          </div>
          <Button
            variant="ghost"
            size="sm"
            onClick={handleRefresh}
            disabled={isLoading || gitLoading}
            className="h-6 w-6 p-0 hover:bg-theme-bg-hover/50"
          >
            <RefreshCw className={cn('w-3.5 h-3.5 text-theme-text-muted', (isLoading || gitLoading) && 'animate-spin')} />
          </Button>
        </div>
        
        {/* 文件树 */}
        <div className="flex-1 overflow-auto py-1">
          {isLoading && rootFiles === null ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="w-5 h-5 animate-spin text-theme-text-muted" />
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
    </GitStatusContext.Provider>
  );
}
