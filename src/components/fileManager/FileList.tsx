/**
 * FileList Component
 * Generic file list UI supporting both local and remote file systems
 */

import React, { useState, useEffect, useLayoutEffect, useRef, useCallback } from 'react';
import { 
  Folder, 
  File, 
  ArrowUp, 
  RefreshCw, 
  Home, 
  Download,
  Upload,
  Trash2,
  Edit3,
  Copy,
  Eye,
  FolderPlus,
  Search,
  ArrowUpDown,
  ArrowDownAZ,
  ArrowUpAZ,
  HardDrive,
  FolderOpen,
  CornerDownLeft,
  Terminal,
  Scissors,
  ClipboardPaste,
  Archive,
  FolderArchive,
  ExternalLink,
  FolderSearch,
  FilePlus,
  CopyPlus,
  CheckSquare,
  Info
} from 'lucide-react';
import { Button } from '../ui/button';
import { cn } from '../../lib/utils';
import { PathBreadcrumb } from '../sftp/PathBreadcrumb';
import type { FileInfo, SortField, SortDirection, ContextMenuState } from './types';

// Format file size to human readable format
export const formatFileSize = (bytes: number): string => {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const size = bytes / Math.pow(1024, i);
  return `${size.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
};

export interface FileListProps {
  // Display
  title: string;
  files: FileInfo[];
  path: string;
  isRemote?: boolean;
  active?: boolean;
  loading?: boolean;
  
  // Selection
  selected: Set<string>;
  lastSelected: string | null;
  onSelect: (name: string, multi: boolean, range: boolean) => void;
  onSelectAll: () => void;
  onClearSelection: () => void;
  
  // Navigation
  onNavigate: (path: string) => void;
  onRefresh: () => void;
  onActivate?: () => void;
  
  // Path editing
  isPathEditable?: boolean;
  pathInputValue?: string;
  onPathInputChange?: (value: string) => void;
  onPathInputSubmit?: () => void;
  
  // Filter & Sort
  filter?: string;
  onFilterChange?: (value: string) => void;
  sortField?: SortField;
  sortDirection?: SortDirection;
  onSortChange?: (field: SortField) => void;
  
  // Actions
  onPreview?: (file: FileInfo) => void;
  onTransfer?: (files: string[], direction: 'upload' | 'download') => void;
  onDelete?: (files: string[]) => void;
  onRename?: (oldName: string) => void;
  onNewFolder?: () => void;
  onBrowse?: () => void;
  onShowDrives?: () => void;
  onOpenExternal?: (path: string) => void;
  onRevealInFileManager?: (path: string) => void;
  onNewFile?: () => void;
  onDuplicate?: (files: string[]) => void;
  onProperties?: (file: FileInfo) => void;

  // Clipboard & Archive
  onCopy?: () => void;
  onCut?: () => void;
  onPaste?: () => void;
  onCompress?: () => void;
  onExtract?: () => void;
  hasClipboard?: boolean;
  canExtract?: boolean;
  
  // Drag & Drop
  isDragOver?: boolean;
  onDragOver?: (e: React.DragEvent) => void;
  onDragLeave?: (e: React.DragEvent) => void;
  onDrop?: (e: React.DragEvent) => void;
  
  // i18n
  t: (key: string, options?: Record<string, unknown>) => string;
}

export const FileList: React.FC<FileListProps> = ({
  title,
  files,
  path,
  isRemote = false,
  active = false,
  loading = false,
  selected,
  onSelect,
  onSelectAll,
  onClearSelection,
  onNavigate,
  onRefresh,
  onActivate,
  isPathEditable = false,
  pathInputValue,
  onPathInputChange,
  onPathInputSubmit,
  filter,
  onFilterChange,
  sortField = 'name',
  sortDirection = 'asc',
  onSortChange,
  onPreview,
  onTransfer,
  onDelete,
  onRename,
  onNewFolder,
  onBrowse,
  onShowDrives,
  onOpenExternal,
  onRevealInFileManager,
  onNewFile,
  onDuplicate,
  onProperties,
  onCopy,
  onCut,
  onPaste,
  onCompress,
  onExtract,
  hasClipboard,
  canExtract,
  isDragOver = false,
  onDragOver,
  onDragLeave,
  onDrop,
  t
}) => {
  const listRef = useRef<HTMLDivElement>(null);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const contextMenuRef = useRef<HTMLDivElement>(null);
  
  const isLocalPane = !isRemote;

  // Handle selection
  const handleSelect = useCallback((name: string, multi: boolean, range: boolean) => {
    onActivate?.();
    onSelect(name, multi, range);
  }, [onActivate, onSelect]);

  // Handle keyboard shortcuts
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (!active) return;
    
    const selectedFiles = Array.from(selected);
    
    // Ctrl/Cmd + A: Select all
    if ((e.metaKey || e.ctrlKey) && e.key === 'a') {
      e.preventDefault();
      onSelectAll();
      return;
    }
    
    // Enter: Open directory or preview file
    if (e.key === 'Enter' && selectedFiles.length === 1) {
      e.preventDefault();
      const file = files.find(f => f.name === selectedFiles[0]);
      if (file) {
        if (file.file_type === 'Directory') {
          const newPath = path === '/' ? `/${file.name}` : `${path}/${file.name}`;
          onNavigate(newPath);
        } else if (onPreview) {
          onPreview(file);
        }
      }
      return;
    }
    
    // Arrow keys for transfer
    if (e.key === 'ArrowRight' && isLocalPane && selectedFiles.length > 0 && onTransfer) {
      e.preventDefault();
      onTransfer(selectedFiles, 'upload');
      return;
    }
    if (e.key === 'ArrowLeft' && !isLocalPane && selectedFiles.length > 0 && onTransfer) {
      e.preventDefault();
      onTransfer(selectedFiles, 'download');
      return;
    }
    
    // Delete key
    if ((e.key === 'Delete' || e.key === 'Backspace') && selectedFiles.length > 0 && onDelete) {
      e.preventDefault();
      onDelete(selectedFiles);
      return;
    }
    
    // F2: Rename
    if (e.key === 'F2' && selectedFiles.length === 1 && onRename) {
      e.preventDefault();
      onRename(selectedFiles[0]);
      return;
    }
  }, [active, selected, files, isLocalPane, path, onNavigate, onPreview, onTransfer, onDelete, onRename, onSelectAll]);

  // Context menu handler
  const handleContextMenu = useCallback((e: React.MouseEvent, file?: FileInfo) => {
    e.preventDefault();
    e.stopPropagation();
    if (file && !selected.has(file.name)) {
      onSelect(file.name, false, false);
    }
    setContextMenu({ x: e.clientX, y: e.clientY, file });
  }, [selected, onSelect]);

  // Close context menu on click outside
  useEffect(() => {
    const handleClick = () => setContextMenu(null);
    if (contextMenu) {
      document.addEventListener('click', handleClick);
      return () => document.removeEventListener('click', handleClick);
    }
  }, [contextMenu]);

  // Adjust context menu position to stay within viewport
  useLayoutEffect(() => {
    if (!contextMenu || !contextMenuRef.current) return;
    const el = contextMenuRef.current;
    const rect = el.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const pad = 8;
    let x = contextMenu.x;
    let y = contextMenu.y;
    if (x + rect.width > vw - pad) x = vw - rect.width - pad;
    if (y + rect.height > vh - pad) y = Math.max(pad, vh - rect.height - pad);
    if (x < pad) x = pad;
    if (y < pad) y = pad;
    if (x !== contextMenu.x || y !== contextMenu.y) {
      el.style.left = `${x}px`;
      el.style.top = `${y}px`;
    }
  }, [contextMenu]);

  return (
    <div 
      className={cn(
        "flex flex-col h-full bg-theme-bg border transition-all duration-200",
        active ? "border-theme-accent/50" : "border-theme-border",
        isDragOver && "border-theme-accent border-2 bg-theme-accent/10 ring-2 ring-theme-accent/30"
      )}
      onClick={onActivate}
      onContextMenu={(e) => handleContextMenu(e)}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
    >
      {/* Header */}
      <div className={cn(
        "flex items-center gap-2 p-2 border-b transition-colors h-10",
        active ? "bg-zinc-800/50 border-theme-accent/30" : "bg-theme-bg-panel border-theme-border"
      )}>
        <span className="font-semibold text-xs text-zinc-400 uppercase tracking-wider min-w-12">{title}</span>
        
        {/* Path bar */}
        <div className="flex-1 flex items-center gap-1 bg-zinc-950 border border-theme-border px-2 py-0.5 rounded-sm overflow-hidden">
          {isPathEditable && pathInputValue !== undefined ? (
            <input
              type="text"
              value={pathInputValue}
              onChange={(e) => onPathInputChange?.(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  onPathInputSubmit?.();
                }
                if (e.key === 'Escape') {
                  onPathInputChange?.(path);
                }
              }}
              onBlur={() => onPathInputChange?.(path)}
              className="flex-1 bg-transparent text-zinc-300 text-xs outline-none"
              placeholder={t('fileManager.pathPlaceholder')}
              autoFocus
            />
          ) : (
            <PathBreadcrumb 
              path={path}
              isRemote={isRemote}
              onNavigate={onNavigate}
              className="flex-1"
            />
          )}
          {isPathEditable && (
            <Button size="icon" variant="ghost" className="h-4 w-4 shrink-0" onClick={onPathInputSubmit} title={t('fileManager.go')}>
              <CornerDownLeft className="h-3 w-3" />
            </Button>
          )}
        </div>
        
        {/* Show drives button (local only) */}
        {onShowDrives && (
          <Button size="icon" variant="ghost" className="h-6 w-6" onClick={onShowDrives} title={t('fileManager.showDrives')}>
            <HardDrive className="h-3 w-3" />
          </Button>
        )}
        
        {/* Browse button (local only) */}
        {onBrowse && (
          <Button size="icon" variant="ghost" className="h-6 w-6" onClick={onBrowse} title={t('fileManager.browse')}>
            <FolderOpen className="h-3 w-3" />
          </Button>
        )}
        
        <Button size="icon" variant="ghost" className="h-6 w-6" onClick={() => onNavigate('..')} title={t('fileManager.goUp')}>
           <ArrowUp className="h-3 w-3" />
        </Button>
        <Button size="icon" variant="ghost" className="h-6 w-6" onClick={() => onNavigate('~')} title={t('fileManager.home')}>
           <Home className="h-3 w-3" />
        </Button>
        <Button size="icon" variant="ghost" className="h-6 w-6" onClick={onRefresh} title={t('fileManager.refresh')}>
           <RefreshCw className={cn("h-3 w-3", loading && "animate-spin")} />
        </Button>
        
        {/* Transfer selected files */}
        {onTransfer && selected.size > 0 && (
          <Button 
            size="sm" 
            variant="ghost" 
            className="h-6 px-2 text-xs gap-1"
            onClick={() => onTransfer(Array.from(selected), isLocalPane ? 'upload' : 'download')}
          >
            {isLocalPane ? <Upload className="h-3 w-3" /> : <Download className="h-3 w-3" />}
            {isLocalPane 
              ? t('fileManager.uploadCount', { count: selected.size }) 
              : t('fileManager.downloadCount', { count: selected.size })}
          </Button>
        )}
      </div>

      {/* Column Headers with Sort */}
      <div className="flex items-center px-2 py-1 bg-zinc-900 border-b border-theme-border text-xs text-zinc-500">
        <button 
          className={cn(
            "flex-1 flex items-center gap-1 hover:text-zinc-300 transition-colors text-left",
            sortField === 'name' && "text-theme-accent"
          )}
          onClick={() => onSortChange?.('name')}
        >
          {t('fileManager.colName')}
          {sortField === 'name' && (
            sortDirection === 'asc' ? <ArrowUpAZ className="h-3 w-3" /> : <ArrowDownAZ className="h-3 w-3" />
          )}
        </button>
        <button 
          className={cn(
            "w-20 flex items-center justify-end gap-1 hover:text-zinc-300 transition-colors",
            sortField === 'size' && "text-theme-accent"
          )}
          onClick={() => onSortChange?.('size')}
        >
          {t('fileManager.colSize')}
          {sortField === 'size' && <ArrowUpDown className="h-3 w-3" />}
        </button>
        <button 
          className={cn(
            "w-24 flex items-center justify-end gap-1 hover:text-zinc-300 transition-colors",
            sortField === 'modified' && "text-theme-accent"
          )}
          onClick={() => onSortChange?.('modified')}
        >
          {t('fileManager.colModified')}
          {sortField === 'modified' && <ArrowUpDown className="h-3 w-3" />}
        </button>
      </div>

      {/* Filter Input */}
      {onFilterChange && (
        <div className="flex items-center gap-2 px-2 py-1 bg-zinc-900/50 border-b border-theme-border">
          <Search className="h-3 w-3 text-zinc-500" />
          <input
            type="text"
            value={filter || ''}
            onChange={(e) => onFilterChange(e.target.value)}
            placeholder={t('fileManager.filterPlaceholder')}
            className="flex-1 bg-transparent text-xs text-zinc-300 placeholder:text-zinc-600 outline-none"
          />
          {filter && (
            <button 
              onClick={() => onFilterChange('')}
              className="text-zinc-500 hover:text-zinc-300 text-xs"
            >
              ✕
            </button>
          )}
        </div>
      )}

      {/* File List */}
      <div 
        ref={listRef}
        className="flex-1 overflow-y-auto outline-none" 
        tabIndex={0} 
        onClick={onClearSelection}
        onKeyDown={handleKeyDown}
      >
        {files.map((file) => {
          const isSelected = selected.has(file.name);
          return (
            <div 
              key={file.name}
              draggable
              onDragStart={(e) => {
                e.dataTransfer.setData('application/json', JSON.stringify({
                  files: Array.from(selected.size > 0 ? selected : [file.name]),
                  source: isRemote ? 'remote' : 'local',
                  basePath: path
                }));
              }}
              onClick={(e) => {
                e.stopPropagation();
                handleSelect(file.name, e.metaKey || e.ctrlKey, e.shiftKey);
              }}
              onDoubleClick={(e) => {
                e.stopPropagation();
                if (file.file_type === 'Directory') {
                  const newPath = path === '/' ? `/${file.name}` : `${path}/${file.name}`;
                  onNavigate(newPath);
                } else if (onPreview) {
                  onPreview(file);
                }
              }}
              onContextMenu={(e) => handleContextMenu(e, file)}
              className={cn(
                "flex items-center px-2 py-1 text-xs cursor-default select-none border-b border-transparent hover:bg-zinc-800",
                isSelected && "bg-theme-accent/20 text-theme-accent"
              )}
            >
              <div className="flex-1 flex items-center gap-2 min-w-0">
                {file.file_type === 'Directory' 
                  ? <Folder className="h-3.5 w-3.5 flex-shrink-0 text-blue-400" /> 
                  : <File className="h-3.5 w-3.5 flex-shrink-0 text-zinc-400" />}
                <span className="truncate">{file.name}</span>
              </div>
              <div className="w-20 text-right text-zinc-500">
                {file.file_type === 'Directory' ? '-' : formatFileSize(file.size)}
              </div>
              <div className="w-24 text-right text-zinc-600">
                {file.modified ? new Date(file.modified * 1000).toLocaleDateString() : '-'}
              </div>
            </div>
          );
        })}
        
        {/* Empty state */}
        {files.length === 0 && !loading && (
          <div className="flex items-center justify-center h-32 text-zinc-500 text-sm">
            {t('fileManager.empty')}
          </div>
        )}
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <div
          ref={contextMenuRef}
          className="fixed z-50 bg-theme-bg-panel border border-theme-border rounded-sm shadow-lg py-1 min-w-[180px] max-h-[80vh] overflow-y-auto"
          style={{ left: contextMenu.x, top: contextMenu.y }}
        >
          {/* Open (directories only — navigate into folder) */}
          {contextMenu.file && contextMenu.file.file_type === 'Directory' && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2 font-medium"
              onClick={() => {
                onNavigate(`${path}/${contextMenu.file!.name}`);
                setContextMenu(null);
              }}
            >
              <FolderOpen className="h-3 w-3" />
              {t('fileManager.open')}
            </button>
          )}

          {/* Open in External App (files only, local only) */}
          {contextMenu.file && contextMenu.file.file_type !== 'Directory' && !isRemote && onOpenExternal && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => {
                onOpenExternal(`${path}/${contextMenu.file!.name}`);
                setContextMenu(null);
              }}
            >
              <ExternalLink className="h-3 w-3" /> {t('fileManager.openExternal')}
            </button>
          )}

          {/* Reveal in File Manager (local only) */}
          {contextMenu.file && !isRemote && onRevealInFileManager && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => {
                onRevealInFileManager(`${path}/${contextMenu.file!.name}`);
                setContextMenu(null);
              }}
            >
              <FolderSearch className="h-3 w-3" /> {t('fileManager.revealInFileManager')}
            </button>
          )}

          {/* Preview (only for files) */}
          {contextMenu.file && contextMenu.file.file_type !== 'Directory' && onPreview && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => {
                onPreview(contextMenu.file!);
                setContextMenu(null);
              }}
            >
              <Eye className="h-3 w-3" /> {t('fileManager.preview')}
            </button>
          )}

          {/* Transfer */}
          {onTransfer && selected.size > 0 && (
            <>
              <div className="border-t border-theme-border my-1" />
              <button
                className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
                onClick={() => {
                  onTransfer(Array.from(selected), isLocalPane ? 'upload' : 'download');
                  setContextMenu(null);
                }}
              >
                {isLocalPane ? <Upload className="h-3 w-3" /> : <Download className="h-3 w-3" />}
                {isLocalPane ? t('fileManager.upload') : t('fileManager.download')}
              </button>
            </>
          )}

          {/* Clipboard operations */}
          {(onCopy || onCut || onPaste) && (
            <div className="border-t border-theme-border my-1" />
          )}

          {onCut && selected.size > 0 && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => { onCut(); setContextMenu(null); }}
            >
              <Scissors className="h-3 w-3" /> {t('fileManager.cut')}
            </button>
          )}

          {onCopy && selected.size > 0 && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => { onCopy(); setContextMenu(null); }}
            >
              <Copy className="h-3 w-3" /> {t('fileManager.copy')}
            </button>
          )}

          {onPaste && hasClipboard && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => { onPaste(); setContextMenu(null); }}
            >
              <ClipboardPaste className="h-3 w-3" /> {t('fileManager.paste')}
            </button>
          )}

          {/* Duplicate */}
          {onDuplicate && selected.size > 0 && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => { onDuplicate(Array.from(selected)); setContextMenu(null); }}
            >
              <CopyPlus className="h-3 w-3" /> {t('fileManager.duplicate')}
            </button>
          )}

          {/* Rename & Path operations */}
          {contextMenu.file && (
            <div className="border-t border-theme-border my-1" />
          )}

          {contextMenu.file && selected.size === 1 && onRename && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => { onRename(contextMenu.file!.name); setContextMenu(null); }}
            >
              <Edit3 className="h-3 w-3" /> {t('fileManager.rename')}
            </button>
          )}

          {contextMenu.file && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => {
                const fullPath = `${path}/${contextMenu.file!.name}`;
                navigator.clipboard.writeText(fullPath);
                setContextMenu(null);
              }}
            >
              <Copy className="h-3 w-3" /> {t('fileManager.copyPath')}
            </button>
          )}

          {contextMenu.file && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => {
                navigator.clipboard.writeText(contextMenu.file!.name);
                setContextMenu(null);
              }}
            >
              <Copy className="h-3 w-3" /> {t('fileManager.copyName')}
            </button>
          )}

          {/* Archive operations */}
          {(onCompress || onExtract) && selected.size > 0 && (
            <div className="border-t border-theme-border my-1" />
          )}

          {onCompress && selected.size > 0 && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => { onCompress(); setContextMenu(null); }}
            >
              <Archive className="h-3 w-3" /> {t('fileManager.compress')}
            </button>
          )}

          {onExtract && selected.size === 1 && canExtract && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => { onExtract(); setContextMenu(null); }}
            >
              <FolderArchive className="h-3 w-3" /> {t('fileManager.extract')}
            </button>
          )}

          {/* Creation & Utility */}
          <div className="border-t border-theme-border my-1" />

          {onNewFolder && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => { onNewFolder(); setContextMenu(null); }}
            >
              <FolderPlus className="h-3 w-3" /> {t('fileManager.newFolder')}
            </button>
          )}

          {onNewFile && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
              onClick={() => { onNewFile(); setContextMenu(null); }}
            >
              <FilePlus className="h-3 w-3" /> {t('fileManager.newFile')}
            </button>
          )}

          <button
            className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
            onClick={() => { onSelectAll(); setContextMenu(null); }}
          >
            <CheckSquare className="h-3 w-3" /> {t('fileManager.selectAll')}
          </button>

          <button
            className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
            onClick={() => { onRefresh(); setContextMenu(null); }}
          >
            <RefreshCw className="h-3 w-3" /> {t('fileManager.refresh')}
          </button>

          {/* Properties */}
          {contextMenu.file && onProperties && (
            <>
              <div className="border-t border-theme-border my-1" />
              <button
                className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2"
                onClick={() => { onProperties(contextMenu.file!); setContextMenu(null); }}
              >
                <Info className="h-3 w-3" /> {t('fileManager.properties')}
              </button>
            </>
          )}

          {/* Delete (destructive, at bottom) */}
          {selected.size > 0 && onDelete && (
            <>
              <div className="border-t border-theme-border my-1" />
              <button
                className="w-full px-3 py-1.5 text-left text-xs hover:bg-zinc-800 flex items-center gap-2 text-red-400"
                onClick={() => { onDelete(Array.from(selected)); setContextMenu(null); }}
              >
                <Trash2 className="h-3 w-3" /> {t('fileManager.delete')}
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
};
