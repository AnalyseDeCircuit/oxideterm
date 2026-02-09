// src/components/ide/dialogs/IdeRemoteFolderDialog.tsx
import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  Folder, 
  FolderOpen, 
  ChevronRight, 
  ChevronUp, 
  Loader2,
  AlertCircle,
  Home,
} from 'lucide-react';
import { nodeSftpListDir } from '../../../lib/api';
import { cn } from '../../../lib/utils';
import { FileInfo } from '../../../types';
import { Button } from '../../ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';
import { Input } from '../../ui/input';
import { useIdeStore } from '../../../store/ideStore';

interface IdeRemoteFolderDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initialPath: string;
  onSelect: (path: string) => void;
}

export function IdeRemoteFolderDialog({
  open,
  onOpenChange,
  initialPath,
  onSelect,
}: IdeRemoteFolderDialogProps) {
  const { t } = useTranslation();
  const { nodeId } = useIdeStore();
  const [currentPath, setCurrentPath] = useState(initialPath);
  const [pathInput, setPathInput] = useState(initialPath);
  const [folders, setFolders] = useState<FileInfo[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedFolder, setSelectedFolder] = useState<string | null>(null);

  // 加载目录内容
  const loadFolder = useCallback(async (path: string) => {
    if (!nodeId) return;
    
    setIsLoading(true);
    setError(null);
    setSelectedFolder(null);
    
    try {
      const result = await nodeSftpListDir(nodeId, path);
      // 只保留目录
      const dirs = result
        .filter(f => f.file_type === 'Directory')
        .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
      setFolders(dirs);
      setCurrentPath(path);
      setPathInput(path);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsLoading(false);
    }
  }, [nodeId]);

  // 初始加载
  useEffect(() => {
    if (open && nodeId) {
      loadFolder(initialPath);
    }
  }, [open, initialPath, loadFolder, nodeId]);

  // 进入子目录
  const handleEnterFolder = useCallback((folderName: string) => {
    const newPath = currentPath === '/' 
      ? `/${folderName}` 
      : `${currentPath}/${folderName}`;
    loadFolder(newPath);
  }, [currentPath, loadFolder]);

  // 返回上级目录
  const handleGoUp = useCallback(() => {
    if (currentPath === '/') return;
    const parentPath = currentPath.split('/').slice(0, -1).join('/') || '/';
    loadFolder(parentPath);
  }, [currentPath, loadFolder]);

  // 返回根目录
  const handleGoHome = useCallback(() => {
    loadFolder('/');
  }, [loadFolder]);

  // 手动输入路径
  const handlePathSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    if (pathInput.trim()) {
      loadFolder(pathInput.trim());
    }
  }, [pathInput, loadFolder]);

  // 确认选择
  const handleConfirm = useCallback(() => {
    const finalPath = selectedFolder 
      ? (currentPath === '/' ? `/${selectedFolder}` : `${currentPath}/${selectedFolder}`)
      : currentPath;
    onSelect(finalPath);
    onOpenChange(false);
  }, [selectedFolder, currentPath, onSelect, onOpenChange]);

  // 双击进入或选择
  const handleDoubleClick = useCallback((folderName: string) => {
    handleEnterFolder(folderName);
  }, [handleEnterFolder]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('ide.select_folder')}</DialogTitle>
          <DialogDescription>
            {t('ide.select_folder_desc')}
          </DialogDescription>
        </DialogHeader>

        {/* 路径输入 */}
        <form onSubmit={handlePathSubmit} className="flex gap-2">
          <Input
            value={pathInput}
            onChange={(e) => setPathInput(e.target.value)}
            placeholder="/"
            className="flex-1 font-mono text-sm"
          />
          <Button type="submit" variant="outline" size="sm">
            {t('ide.go')}
          </Button>
        </form>

        {/* 导航按钮 */}
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleGoHome}
            disabled={currentPath === '/' || isLoading}
            className="gap-1"
          >
            <Home className="w-4 h-4" />
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={handleGoUp}
            disabled={currentPath === '/' || isLoading}
            className="gap-1"
          >
            <ChevronUp className="w-4 h-4" />
            {t('ide.go_to_parent')}
          </Button>
        </div>

        {/* 文件夹列表 */}
        <div className="border border-theme-border rounded-md h-64 overflow-auto">
          {isLoading ? (
            <div className="flex items-center justify-center h-full">
              <Loader2 className="w-6 h-6 animate-spin text-theme-text-muted" />
            </div>
          ) : error ? (
            <div className="flex flex-col items-center justify-center h-full gap-2 p-4">
              <AlertCircle className="w-6 h-6 text-red-400" />
              <p className="text-sm text-red-400 text-center">{error}</p>
              <Button variant="outline" size="sm" onClick={() => loadFolder(currentPath)}>
                {t('ide.retry')}
              </Button>
            </div>
          ) : folders.length === 0 ? (
            <div className="flex items-center justify-center h-full text-theme-text-muted text-sm">
              {t('ide.no_subfolders')}
            </div>
          ) : (
            <div className="p-1">
              {folders.map(folder => (
                <div
                  key={folder.name}
                  className={cn(
                    "flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer transition-colors",
                    selectedFolder === folder.name 
                      ? "bg-theme-accent/20 text-theme-accent" 
                      : "hover:bg-theme-bg-hover"
                  )}
                  onClick={() => setSelectedFolder(folder.name === selectedFolder ? null : folder.name)}
                  onDoubleClick={() => handleDoubleClick(folder.name)}
                >
                  {selectedFolder === folder.name ? (
                    <FolderOpen className="w-4 h-4 flex-shrink-0" />
                  ) : (
                    <Folder className="w-4 h-4 flex-shrink-0" />
                  )}
                  <span className="text-sm truncate">{folder.name}</span>
                  <ChevronRight className="w-4 h-4 ml-auto opacity-50" />
                </div>
              ))}
            </div>
          )}
        </div>

        {/* 当前选择显示 */}
        <div className="text-xs text-theme-text-muted">
          {t('ide.selected_path')}: <code className="font-mono bg-theme-bg-panel px-1 rounded">
            {selectedFolder 
              ? (currentPath === '/' ? `/${selectedFolder}` : `${currentPath}/${selectedFolder}`)
              : currentPath}
          </code>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t('ide.cancel')}
          </Button>
          <Button onClick={handleConfirm} disabled={isLoading}>
            {t('ide.open_folder')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
