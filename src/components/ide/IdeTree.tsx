// src/components/ide/IdeTree.tsx
import { useState, useEffect, useCallback, useRef, createContext, useContext } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  ChevronRight,
  ChevronDown,
  RefreshCw,
  AlertCircle,
  Loader2,
  GitBranch,
  Folder,
  FolderInput,
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
import { IdeRemoteFolderDialog } from './dialogs/IdeRemoteFolderDialog';
import { IdeDeleteConfirmDialog } from './dialogs/IdeDeleteConfirmDialog';
import { IdeTreeContextMenu } from './IdeTreeContextMenu';
import { IdeInlineInput } from './IdeInlineInput';
import { normalizePath, getParentPath } from '../../lib/pathUtils';
import { useToast } from '../../hooks/useToast';

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
  onContextMenu: (e: React.MouseEvent, path: string, isDir: boolean, name: string) => void;
  inlineInput: InlineInputState | null;
  onInlineInputConfirm: (value: string) => void;
  onInlineInputCancel: () => void;
}

// 内联输入状态类型
interface InlineInputState {
  type: 'newFile' | 'newFolder' | 'rename';
  parentPath: string;
  targetPath?: string;
  originalName?: string;
}

function TreeNode({ 
  file, 
  depth, 
  sftpSessionId, 
  parentPath,
  onContextMenu,
  inlineInput,
  onInlineInputConfirm,
  onInlineInputCancel,
}: TreeNodeProps) {
  const { t } = useTranslation();
  const gitStatusCtx = useGitStatusContext();
  const [children, setChildren] = useState<FileInfo[] | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const loadingRef = useRef(false);
  
  const isDir = isDirectory(file);
  // 使用后端返回的标准化路径，而不是手动构建
  // file.path 已经是 canonicalized 的绝对路径
  const fullPath = file.path;
  const normalizedFullPath = normalizePath(fullPath);
  
  // 精细化 selector：只订阅需要的状态，减少不必要的重渲染
  const isExpanded = useIdeStore(state => state.expandedPaths.has(fullPath));
  const isOpen = useIdeStore(state => state.tabs.some(t => t.path === fullPath));
  const togglePath = useIdeStore(state => state.togglePath);
  const openFile = useIdeStore(state => state.openFile);
  
  // 订阅刷新信号（使用可选链确保类型安全）
  const refreshSignal = useIdeStore(
    state => state.treeRefreshSignal?.[normalizedFullPath] ?? 0
  );
  
  // 当刷新信号变化时，重新加载子节点（仅当是已展开的目录）
  useEffect(() => {
    if (isDir && isExpanded && refreshSignal > 0 && children !== null) {
      setChildren(null);
      loadingRef.current = false;
    }
  }, [refreshSignal, isDir, isExpanded]);
  
  // 计算相对于项目根目录的路径（用于 Git 状态查询）
  const relativePath = gitStatusCtx 
    ? fullPath.startsWith(gitStatusCtx.projectRootPath)
      ? fullPath.substring(gitStatusCtx.projectRootPath.length + 1) // 移除根路径和前导斜杠
      : file.name
    : '';
  const gitStatus = gitStatusCtx?.getFileStatus(relativePath);
  
  // 加载子目录内容
  const loadChildren = useCallback(async () => {
    if (!isDir || loadingRef.current) return;
    
    loadingRef.current = true;
    setIsLoading(true);
    setError(null);
    
    try {
      const result = await api.sftpListDir(sftpSessionId, fullPath);
      setChildren(sortFiles(result));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsLoading(false);
      loadingRef.current = false;
    }
  }, [isDir, fullPath, sftpSessionId]);
  
  // 展开时加载子目录
  useEffect(() => {
    if (isExpanded && isDir && children === null && !loadingRef.current) {
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
  
  // 右键菜单处理
  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    onContextMenu(e, fullPath, isDir, file.name);
  }, [fullPath, isDir, file.name, onContextMenu]);
  
  // 检查此节点是否需要显示内联输入框（重命名场景）
  const showRenameInput = inlineInput?.type === 'rename' && inlineInput.targetPath === fullPath;
  
  // 检查此目录下是否需要显示新建输入框
  const showNewInput = (inlineInput?.type === 'newFile' || inlineInput?.type === 'newFolder') 
    && inlineInput.parentPath === fullPath;
  
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
        onContextMenu={handleContextMenu}
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
        
        {/* 文件名（或重命名输入框） */}
        {showRenameInput ? (
          <IdeInlineInput
            defaultValue={file.name}
            selectBaseName={!isDir}
            onConfirm={onInlineInputConfirm}
            onCancel={onInlineInputCancel}
            className="flex-1"
          />
        ) : (
          <span className={cn(
            'truncate text-xs flex-1',
            isDir ? 'text-theme-text' : 'text-theme-text-muted',
            isOpen && 'text-theme-accent font-medium',
            // Git 状态颜色（仅对未打开的文件名生效）
            !isOpen && gitStatus && GIT_STATUS_COLORS[gitStatus]
          )}>
            {file.name}
          </span>
        )}
        
        {/* Git 状态指示器 */}
        {!showRenameInput && gitStatus && gitStatus !== 'ignored' && (
          <span className={cn(
            'text-[10px] mr-1 font-mono',
            GIT_STATUS_COLORS[gitStatus]
          )}>
            {GIT_STATUS_LABELS[gitStatus]}
          </span>
        )}
      </div>
      
      {/* 新建文件/文件夹输入框（显示在子节点列表顶部） */}
      {isDir && isExpanded && showNewInput && (
        <div
          className="flex items-center gap-1 py-0.5 px-1"
          style={{ paddingLeft: `${(depth + 1) * 12 + 4}px` }}
        >
          <span className="w-4 h-4" /> {/* 占位 */}
          <span className="w-4 h-4 flex items-center justify-center flex-shrink-0">
            {inlineInput?.type === 'newFolder' ? (
              <FolderIcon isOpen={false} size={16} />
            ) : (
              <FileIcon filename="new" size={14} />
            )}
          </span>
          <IdeInlineInput
            placeholder={inlineInput?.type === 'newFolder' 
              ? t('ide.inline.newFolderPlaceholder', 'folder name')
              : t('ide.inline.newFilePlaceholder', 'filename.ext')
            }
            onConfirm={onInlineInputConfirm}
            onCancel={onInlineInputCancel}
            className="flex-1"
          />
        </div>
      )}
      
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
              onContextMenu={onContextMenu}
              inlineInput={inlineInput}
              onInlineInputConfirm={onInlineInputConfirm}
              onInlineInputCancel={onInlineInputCancel}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function IdeTree() {
  const { t } = useTranslation();
  const { toast } = useToast();
  const project = useIdeProject();
  
  // 精细化 selector：只订阅需要的状态
  const sftpSessionId = useIdeStore(state => state.sftpSessionId);
  const expandedPaths = useIdeStore(state => state.expandedPaths);
  const changeRootPath = useIdeStore(state => state.changeRootPath);
  const hasDirtyFiles = useIdeStore(state => state.tabs.some(t => t.isDirty));
  
  // 文件操作 actions
  const createFile = useIdeStore(state => state.createFile);
  const createFolder = useIdeStore(state => state.createFolder);
  const deleteItem = useIdeStore(state => state.deleteItem);
  const renameItem = useIdeStore(state => state.renameItem);
  const getAffectedTabs = useIdeStore(state => state.getAffectedTabs);
  const openFile = useIdeStore(state => state.openFile);
  const togglePath = useIdeStore(state => state.togglePath);
  
  const { status: gitStatus, getFileStatus, refresh: refreshGit, isLoading: gitLoading } = useGitStatus();
  const [rootFiles, setRootFiles] = useState<FileInfo[] | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isChangingRoot, setIsChangingRoot] = useState(false);
  const [folderDialogOpen, setFolderDialogOpen] = useState(false);
  
  // 右键菜单状态
  const [contextMenu, setContextMenu] = useState<{
    position: { x: number; y: number };
    path: string;
    isDirectory: boolean;
    name: string;
  } | null>(null);
  
  // 内联输入状态
  const [inlineInput, setInlineInput] = useState<InlineInputState | null>(null);
  
  // 删除确认对话框状态
  const [deleteConfirm, setDeleteConfirm] = useState<{
    path: string;
    name: string;
    isDirectory: boolean;
    affectedTabCount: number;
    unsavedTabCount: number;
  } | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  
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
  
  // 订阅根目录刷新信号
  const rootRefreshSignal = useIdeStore(
    state => project ? (state.treeRefreshSignal?.[normalizePath(project.rootPath)] ?? 0) : 0
  );
  
  // 当根目录刷新信号变化时，重新加载
  useEffect(() => {
    if (project && sftpSessionId && rootRefreshSignal > 0) {
      loadRoot();
    }
  }, [rootRefreshSignal, project, sftpSessionId, loadRoot]);
  
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
  
  // 打开文件夹选择对话框
  const handleOpenFolderDialog = useCallback(() => {
    if (hasDirtyFiles) {
      // TODO: 显示提示保存文件
      return;
    }
    setFolderDialogOpen(true);
  }, [hasDirtyFiles]);
  
  // 处理文件夹选择
  const handleFolderSelect = useCallback(async (path: string) => {
    if (isChangingRoot) return;
    
    setIsChangingRoot(true);
    try {
      await changeRootPath(path);
    } catch (e) {
      console.error('Failed to change root:', e);
    } finally {
      setIsChangingRoot(false);
    }
  }, [isChangingRoot, changeRootPath]);
  
  // ═══════════════════════════════════════════════════════════════════════════
  // 右键菜单和文件操作处理
  // ═══════════════════════════════════════════════════════════════════════════
  
  // 右键菜单处理
  const handleContextMenu = useCallback((
    e: React.MouseEvent, 
    path: string, 
    isDir: boolean, 
    name: string
  ) => {
    setContextMenu({
      position: { x: e.clientX, y: e.clientY },
      path,
      isDirectory: isDir,
      name,
    });
  }, []);
  
  // 空白区域右键菜单（在根目录新建）
  const handleEmptyAreaContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!project) return;
    // 空白区域 = 在根目录下操作
    setContextMenu({
      position: { x: e.clientX, y: e.clientY },
      path: project.rootPath,
      isDirectory: true,
      name: project.name,
    });
  }, [project]);
  
  const closeContextMenu = useCallback(() => {
    setContextMenu(null);
  }, []);
  
  // 新建文件
  const handleNewFile = useCallback(() => {
    if (!contextMenu) return;
    const parentPath = contextMenu.isDirectory ? contextMenu.path : getParentPath(contextMenu.path);
    // 确保目录已展开
    if (!expandedPaths.has(parentPath)) {
      togglePath(parentPath);
    }
    setInlineInput({
      type: 'newFile',
      parentPath,
    });
  }, [contextMenu, expandedPaths, togglePath]);
  
  // 新建文件夹
  const handleNewFolder = useCallback(() => {
    if (!contextMenu) return;
    const parentPath = contextMenu.isDirectory ? contextMenu.path : getParentPath(contextMenu.path);
    if (!expandedPaths.has(parentPath)) {
      togglePath(parentPath);
    }
    setInlineInput({
      type: 'newFolder',
      parentPath,
    });
  }, [contextMenu, expandedPaths, togglePath]);
  
  // 重命名
  const handleRename = useCallback(() => {
    if (!contextMenu) return;
    setInlineInput({
      type: 'rename',
      parentPath: getParentPath(contextMenu.path),
      targetPath: contextMenu.path,
      originalName: contextMenu.name,
    });
  }, [contextMenu]);
  
  // 准备删除（显示确认对话框）
  const handleDelete = useCallback(() => {
    if (!contextMenu) return;
    const { affected, unsaved } = getAffectedTabs(contextMenu.path);
    setDeleteConfirm({
      path: contextMenu.path,
      name: contextMenu.name,
      isDirectory: contextMenu.isDirectory,
      affectedTabCount: affected.length,
      unsavedTabCount: unsaved.length,
    });
  }, [contextMenu, getAffectedTabs]);
  
  // 确认删除
  const handleDeleteConfirm = useCallback(async () => {
    if (!deleteConfirm) return;
    
    setIsDeleting(true);
    try {
      await deleteItem(deleteConfirm.path, deleteConfirm.isDirectory);
      toast({
        title: t('ide.toast.deleted', 'Deleted'),
        description: deleteConfirm.name,
      });
      setDeleteConfirm(null);
    } catch (e) {
      toast({
        title: t('ide.toast.deleteFailed', 'Failed to delete'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'error',
      });
    } finally {
      setIsDeleting(false);
    }
  }, [deleteConfirm, deleteItem, toast, t]);
  
  // 复制路径
  const handleCopyPath = useCallback(() => {
    if (!contextMenu) return;
    navigator.clipboard.writeText(contextMenu.path);
    toast({
      title: t('ide.toast.pathCopied', 'Path copied'),
      description: contextMenu.path,
    });
  }, [contextMenu, toast, t]);
  
  // 在终端中打开
  const handleRevealInTerminal = useCallback(() => {
    if (!contextMenu) return;
    const dirPath = contextMenu.isDirectory ? contextMenu.path : getParentPath(contextMenu.path);
    // TODO: 向终端发送 cd 命令
    toast({
      title: t('ide.toast.revealInTerminal', 'Open in Terminal'),
      description: `cd ${dirPath}`,
    });
  }, [contextMenu, toast, t]);
  
  // 内联输入确认
  const handleInlineInputConfirm = useCallback(async (value: string) => {
    if (!inlineInput) return;
    
    try {
      if (inlineInput.type === 'newFile') {
        const newPath = await createFile(inlineInput.parentPath, value);
        toast({
          title: t('ide.toast.fileCreated', 'File created'),
          description: value,
        });
        // 创建成功后自动打开
        await openFile(newPath);
      } else if (inlineInput.type === 'newFolder') {
        await createFolder(inlineInput.parentPath, value);
        toast({
          title: t('ide.toast.folderCreated', 'Folder created'),
          description: value,
        });
      } else if (inlineInput.type === 'rename' && inlineInput.targetPath) {
        await renameItem(inlineInput.targetPath, value);
        toast({
          title: t('ide.toast.renamed', 'Renamed'),
          description: `${inlineInput.originalName} → ${value}`,
        });
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      // 翻译错误消息
      const errorTitle = message.startsWith('ide.') 
        ? t(message, message.split('.').pop() || message)
        : message;
      toast({
        title: t('ide.toast.operationFailed', 'Operation failed'),
        description: errorTitle,
        variant: 'error',
      });
    } finally {
      setInlineInput(null);
    }
  }, [inlineInput, createFile, createFolder, renameItem, openFile, toast, t]);
  
  // 内联输入取消
  const handleInlineInputCancel = useCallback(() => {
    setInlineInput(null);
  }, []);
  
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
          <div className="flex items-center gap-2 min-w-0 flex-1">
            <Folder className="w-4 h-4 text-theme-accent flex-shrink-0" />
            <span className="text-xs font-medium text-theme-text truncate">
              {project.name}
            </span>
            {/* Git 分支信息 */}
            {project.isGitRepo && gitStatus && (
              <span className="flex items-center gap-1 text-[10px] text-theme-text-muted truncate">
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
          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              size="sm"
              onClick={handleOpenFolderDialog}
              disabled={hasDirtyFiles || isChangingRoot}
              className="h-6 w-6 p-0 hover:bg-theme-bg-hover/50"
              title={t('ide.open_folder')}
            >
              <FolderInput className="w-3.5 h-3.5 text-theme-text-muted" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={handleRefresh}
              disabled={isLoading || gitLoading || isChangingRoot}
              className="h-6 w-6 p-0 hover:bg-theme-bg-hover/50"
            >
              <RefreshCw className={cn('w-3.5 h-3.5 text-theme-text-muted', (isLoading || gitLoading || isChangingRoot) && 'animate-spin')} />
            </Button>
          </div>
        </div>
        
        {/* 文件树 */}
        <div 
          className="flex-1 overflow-auto py-1"
          onContextMenu={handleEmptyAreaContextMenu}
        >
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
              onContextMenu={handleContextMenu}
              inlineInput={inlineInput}
              onInlineInputConfirm={handleInlineInputConfirm}
              onInlineInputCancel={handleInlineInputCancel}
            />
          ))}
          
          {/* 根目录新建输入框（当在根目录下新建时） */}
          {inlineInput && (inlineInput.type === 'newFile' || inlineInput.type === 'newFolder') 
            && inlineInput.parentPath === project.rootPath && (
            <div
              className="flex items-center gap-1 py-0.5 px-1"
              style={{ paddingLeft: '16px' }}
            >
              <span className="w-4 h-4" />
              <span className="w-4 h-4 flex items-center justify-center flex-shrink-0">
                {inlineInput.type === 'newFolder' ? (
                  <FolderIcon isOpen={false} size={16} />
                ) : (
                  <FileIcon filename="new" size={14} />
                )}
              </span>
              <IdeInlineInput
                placeholder={inlineInput.type === 'newFolder' 
                  ? t('ide.inline.newFolderPlaceholder', 'folder name')
                  : t('ide.inline.newFilePlaceholder', 'filename.ext')
                }
                onConfirm={handleInlineInputConfirm}
                onCancel={handleInlineInputCancel}
                className="flex-1"
              />
            </div>
          )}
        </div>
        
        {/* 远程文件夹选择对话框 */}
        <IdeRemoteFolderDialog
          open={folderDialogOpen}
          onOpenChange={setFolderDialogOpen}
          initialPath={project.rootPath}
          onSelect={handleFolderSelect}
        />
        
        {/* 右键菜单 */}
        {contextMenu && (
          <IdeTreeContextMenu
            position={contextMenu.position}
            path={contextMenu.path}
            isDirectory={contextMenu.isDirectory}
            name={contextMenu.name}
            onNewFile={handleNewFile}
            onNewFolder={handleNewFolder}
            onRename={handleRename}
            onDelete={handleDelete}
            onCopyPath={handleCopyPath}
            onRevealInTerminal={handleRevealInTerminal}
            onClose={closeContextMenu}
          />
        )}
        
        {/* 删除确认对话框 */}
        {deleteConfirm && (
          <IdeDeleteConfirmDialog
            open={!!deleteConfirm}
            onOpenChange={(open) => !open && setDeleteConfirm(null)}
            path={deleteConfirm.path}
            name={deleteConfirm.name}
            isDirectory={deleteConfirm.isDirectory}
            affectedTabCount={deleteConfirm.affectedTabCount}
            unsavedTabCount={deleteConfirm.unsavedTabCount}
            onConfirm={handleDeleteConfirm}
            isDeleting={isDeleting}
          />
        )}
      </div>
    </GitStatusContext.Provider>
  );
}
