// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * FileList Component
 * Generic file list UI supporting both local and remote file systems
 */

import React, { useState, useEffect, useLayoutEffect, useRef, useCallback } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
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
import { joinLocalPath } from './pathUtils';

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
  error?: string | null;
  
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
  onPathEditStart?: () => void;
  onPathEditCancel?: () => void;
  
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
  /** Set of file names currently in the "cut" clipboard (for visual dimming) */
  cutFileNames?: Set<string>;
  
  // Drag & Drop
  isDragOver?: boolean;
  onDragOver?: (e: React.DragEvent) => void;
  onDragLeave?: (e: React.DragEvent) => void;
  onDrop?: (e: React.DragEvent) => void;
  
  // i18n
  t: (key: string, options?: Record<string, unknown>) => string;
}

const FILE_ROW_HEIGHT = 28; // py-1 + text-xs + border ≈ 28px

function joinPanePath(basePath: string, name: string, isRemote: boolean): string {
  if (isRemote) {
    return basePath === '/' ? `/${name}` : `${basePath.replace(/\/+$/g, '')}/${name}`;
  }
  return joinLocalPath(basePath, name);
}

type FileRowProps = {
  file: FileInfo;
  isSelected: boolean;
  isCut: boolean;
  isRemote: boolean;
  path: string;
  selected: Set<string>;
  onSelect: (name: string, multi: boolean, range: boolean) => void;
  onNavigate: (path: string) => void;
  onPreview?: (file: FileInfo) => void;
  onContextMenu: (e: React.MouseEvent, file: FileInfo) => void;
};

const FileRow = React.memo<FileRowProps>(({
  file, isSelected, isCut, isRemote, path, selected,
  onSelect, onNavigate, onPreview, onContextMenu,
}) => (
  <div
    draggable
    onDragStart={(e) => {
      const draggedFiles = selected.size > 0 ? Array.from(selected) : [file.name];
      e.dataTransfer.setData('application/json', JSON.stringify({
        files: draggedFiles,
        source: isRemote ? 'remote' : 'local',
        basePath: path
      }));
      // Custom drag preview showing file count badge
      const preview = document.createElement('div');
      preview.style.cssText = 'position:absolute;top:-9999px;left:-9999px;display:flex;align-items:center;gap:6px;padding:4px 10px;background:var(--color-theme-bg-elevated,#1e1e2e);border:1px solid var(--color-theme-border,#444);border-radius:4px;color:var(--color-theme-text,#cdd6f4);font-size:12px;white-space:nowrap;';
      preview.textContent = draggedFiles.length > 1
        ? `${draggedFiles[0]} +${draggedFiles.length - 1}`
        : draggedFiles[0];
      document.body.appendChild(preview);
      e.dataTransfer.setDragImage(preview, 0, 0);
      requestAnimationFrame(() => document.body.removeChild(preview));
    }}
    onClick={(e) => {
      e.stopPropagation();
      onSelect(file.name, e.metaKey || e.ctrlKey, e.shiftKey);
    }}
    onDoubleClick={(e) => {
      e.stopPropagation();
      if (file.file_type === 'Directory') {
        const newPath = joinPanePath(path, file.name, isRemote);
        onNavigate(newPath);
      } else if (onPreview) {
        onPreview(file);
      }
    }}
    onContextMenu={(e) => onContextMenu(e, file)}
    className={cn(
      "flex items-center px-2 text-xs cursor-default select-none border-b border-transparent hover:bg-theme-bg-hover",
      isSelected && "bg-theme-accent/20 text-theme-accent",
      isCut && "opacity-50"
    )}
    style={{ height: FILE_ROW_HEIGHT }}
  >
    <div className="flex-1 flex items-center gap-2 min-w-0">
      {file.file_type === 'Directory'
        ? <Folder className="h-3.5 w-3.5 flex-shrink-0 text-blue-400" />
        : <File className="h-3.5 w-3.5 flex-shrink-0 text-theme-text-muted" />}
      <span className="truncate">{file.name}</span>
    </div>
    <div className="w-20 text-right text-theme-text-muted">
      {file.file_type === 'Directory' ? '-' : formatFileSize(file.size)}
    </div>
    <div className="w-24 text-right text-theme-text-muted">
      {file.modified ? new Date(file.modified * 1000).toLocaleDateString() : '-'}
    </div>
  </div>
), (prev, next) =>
  prev.file === next.file &&
  prev.isSelected === next.isSelected &&
  prev.isCut === next.isCut &&
  prev.path === next.path
);
FileRow.displayName = 'FileRow';

export const FileList: React.FC<FileListProps> = ({
  title,
  files,
  path,
  isRemote = false,
  active = false,
  loading = false,
  error = null,
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
  onPathEditStart,
  onPathEditCancel,
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
  cutFileNames,
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

  const virtualizer = useVirtualizer({
    count: files.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => FILE_ROW_HEIGHT,
    overscan: 15,
  });

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
          const newPath = joinPanePath(path, file.name, isRemote);
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
        active ? "bg-theme-bg-panel border-theme-accent/30" : "bg-theme-bg-panel border-theme-border"
      )}>
        <span className="font-semibold text-xs text-theme-text-muted uppercase tracking-wider min-w-12">{title}</span>
        
        {/* Path bar */}
        <div
          className="flex-1 flex items-center gap-1 bg-theme-bg-sunken border border-theme-border px-2 py-0.5 rounded-sm overflow-hidden cursor-text"
          onDoubleClick={() => { if (!isPathEditable) onPathEditStart?.(); }}
        >
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
                  e.preventDefault();
                  onPathEditCancel?.();
                }
              }}
              onBlur={(e) => {
                const related = e.relatedTarget as HTMLElement | null;
                if (related?.closest('[data-path-go-btn]')) return;
                onPathEditCancel?.();
              }}
              className="flex-1 bg-transparent text-theme-text text-xs outline-none"
              placeholder={t('fileManager.pathPlaceholder')}
              autoFocus
              onFocus={(e) => e.target.select()}
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
            <Button data-path-go-btn size="icon" variant="ghost" className="h-4 w-4 shrink-0" onClick={onPathInputSubmit} title={t('fileManager.go')}>
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
      <div className="flex items-center px-2 py-1 bg-theme-bg-panel border-b border-theme-border text-xs text-theme-text-muted">
        <button 
          className={cn(
            "flex-1 flex items-center gap-1 hover:text-theme-text transition-colors text-left",
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
            "w-20 flex items-center justify-end gap-1 hover:text-theme-text transition-colors",
            sortField === 'size' && "text-theme-accent"
          )}
          onClick={() => onSortChange?.('size')}
        >
          {t('fileManager.colSize')}
          {sortField === 'size' && <ArrowUpDown className="h-3 w-3" />}
        </button>
        <button 
          className={cn(
            "w-24 flex items-center justify-end gap-1 hover:text-theme-text transition-colors",
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
        <div className="flex items-center gap-2 px-2 py-1 bg-theme-bg-panel border-b border-theme-border">
          <Search className="h-3 w-3 text-theme-text-muted" />
          <input
            type="text"
            value={filter || ''}
            onChange={(e) => onFilterChange(e.target.value)}
            placeholder={t('fileManager.filterPlaceholder')}
            className="flex-1 bg-transparent text-xs text-theme-text placeholder:text-theme-text-muted outline-none"
          />
          {filter && (
            <button 
              onClick={() => onFilterChange('')}
              className="text-theme-text-muted hover:text-theme-text text-xs"
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
        {error && !loading ? (
          <div className="m-3 rounded-md border border-red-500/30 bg-red-500/10 p-3 text-xs text-red-200">
            <div className="font-medium text-red-100">{t('fileManager.error')}</div>
            <div className="mt-1 break-words text-red-200/90">{error}</div>
            <Button
              size="sm"
              variant="ghost"
              className="mt-2 h-7 px-2 text-xs"
              onClick={(e) => {
                e.stopPropagation();
                onRefresh();
              }}
            >
              <RefreshCw className="mr-1 h-3 w-3" />
              {t('fileManager.refresh')}
            </Button>
          </div>
        ) : files.length > 0 ? (
          <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const file = files[virtualRow.index];
              return (
                <div
                  key={file.name}
                  style={{
                    position: 'absolute',
                    top: 0,
                    left: 0,
                    width: '100%',
                    transform: `translateY(${virtualRow.start}px)`,
                  }}
                >
                  <FileRow
                    file={file}
                    isSelected={selected.has(file.name)}
                    isCut={cutFileNames?.has(file.name) ?? false}
                    isRemote={isRemote}
                    path={path}
                    selected={selected}
                    onSelect={handleSelect}
                    onNavigate={onNavigate}
                    onPreview={onPreview}
                    onContextMenu={handleContextMenu}
                  />
                </div>
              );
            })}
          </div>
        ) : (
          /* Empty state */
          !loading && (
            <div className="flex flex-col items-center justify-center h-32 text-theme-text-muted gap-2">
              <FolderOpen className="h-8 w-8 opacity-30" />
              <span className="text-sm">{t('fileManager.empty')}</span>
            </div>
          )
        )}
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <div
          ref={contextMenuRef}
          className="fixed z-50 bg-theme-bg-elevated border border-theme-border rounded-md shadow-lg py-1 min-w-[180px] max-h-[80vh] overflow-y-auto"
          style={{ left: contextMenu.x, top: contextMenu.y }}
        >
          {/* Open (directories only — navigate into folder) */}
          {contextMenu.file && contextMenu.file.file_type === 'Directory' && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2 font-medium"
              onClick={() => {
                onNavigate(joinPanePath(path, contextMenu.file!.name, isRemote));
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
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => {
                onOpenExternal(joinPanePath(path, contextMenu.file!.name, false));
                setContextMenu(null);
              }}
            >
              <ExternalLink className="h-3 w-3" /> {t('fileManager.openExternal')}
            </button>
          )}

          {/* Reveal in File Manager (local only) */}
          {contextMenu.file && !isRemote && onRevealInFileManager && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => {
                onRevealInFileManager(joinPanePath(path, contextMenu.file!.name, false));
                setContextMenu(null);
              }}
            >
              <FolderSearch className="h-3 w-3" /> {t('fileManager.revealInFileManager')}
            </button>
          )}

          {/* Preview (only for files) */}
          {contextMenu.file && contextMenu.file.file_type !== 'Directory' && onPreview && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
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
                className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
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
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => { onCut(); setContextMenu(null); }}
            >
              <Scissors className="h-3 w-3" /> {t('fileManager.cut')}
            </button>
          )}

          {onCopy && selected.size > 0 && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => { onCopy(); setContextMenu(null); }}
            >
              <Copy className="h-3 w-3" /> {t('fileManager.copy')}
            </button>
          )}

          {onPaste && hasClipboard && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => { onPaste(); setContextMenu(null); }}
            >
              <ClipboardPaste className="h-3 w-3" /> {t('fileManager.paste')}
            </button>
          )}

          {/* Duplicate */}
          {onDuplicate && selected.size > 0 && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
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
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => { onRename(contextMenu.file!.name); setContextMenu(null); }}
            >
              <Edit3 className="h-3 w-3" /> {t('fileManager.rename')}
            </button>
          )}

          {contextMenu.file && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => {
                const fullPath = joinPanePath(path, contextMenu.file!.name, isRemote);
                navigator.clipboard.writeText(fullPath);
                setContextMenu(null);
              }}
            >
              <Copy className="h-3 w-3" /> {t('fileManager.copyPath')}
            </button>
          )}

          {contextMenu.file && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
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
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => { onCompress(); setContextMenu(null); }}
            >
              <Archive className="h-3 w-3" /> {t('fileManager.compress')}
            </button>
          )}

          {onExtract && selected.size === 1 && canExtract && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => { onExtract(); setContextMenu(null); }}
            >
              <FolderArchive className="h-3 w-3" /> {t('fileManager.extract')}
            </button>
          )}

          {/* Creation & Utility */}
          <div className="border-t border-theme-border my-1" />

          {onNewFolder && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => { onNewFolder(); setContextMenu(null); }}
            >
              <FolderPlus className="h-3 w-3" /> {t('fileManager.newFolder')}
            </button>
          )}

          {onNewFile && (
            <button
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => { onNewFile(); setContextMenu(null); }}
            >
              <FilePlus className="h-3 w-3" /> {t('fileManager.newFile')}
            </button>
          )}

          <button
            className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
            onClick={() => { onSelectAll(); setContextMenu(null); }}
          >
            <CheckSquare className="h-3 w-3" /> {t('fileManager.selectAll')}
          </button>

          <button
            className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
            onClick={() => { onRefresh(); setContextMenu(null); }}
          >
            <RefreshCw className="h-3 w-3" /> {t('fileManager.refresh')}
          </button>

          {/* Properties */}
          {contextMenu.file && onProperties && (
            <>
              <div className="border-t border-theme-border my-1" />
              <button
                className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
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
                className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2 text-red-400"
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
