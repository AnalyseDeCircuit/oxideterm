// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
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
  Loader2,
  CornerDownLeft,
  GitCompare,
  Usb,
  Globe
} from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { useTransferStore } from '../../store/transferStore';
import { useToast } from '../../hooks/useToast';
import { useTabBgActive } from '../../hooks/useTabBackground';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { cn } from '../../lib/utils';
import { TransferQueue } from './TransferQueue';
import { TransferConflictDialog, ConflictInfo, ConflictResolution } from './TransferConflictDialog';
import { PathBreadcrumb } from './PathBreadcrumb';
import { FileDiffDialog } from './FileDiffDialog';
import { RemoteFileEditor } from '../editor/RemoteFileEditor';
import { CodeHighlight } from '../fileManager/CodeHighlight';
import { OfficePreview } from '../fileManager/OfficePreview';
import { PdfViewer } from '../fileManager/PdfViewer';
import { ImageViewer } from '../fileManager/ImageViewer';
import { api, nodeSftpInit, nodeSftpListDir, nodeSftpPreview, nodeSftpPreviewHex, nodeSftpDownload, nodeSftpUpload, nodeSftpDownloadDir, nodeSftpUploadDir, nodeSftpTarProbe, nodeSftpTarCompressionProbe, nodeSftpTarUpload, nodeSftpTarDownload, nodeSftpDelete, nodeSftpDeleteRecursive, nodeSftpMkdir, nodeSftpRename, cleanupSftpPreviewTemp } from '../../lib/api';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useSettingsStore } from '../../store/settingsStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { FileInfo } from '../../types';
import type { DriveInfo } from '../fileManager/types';
import { listen } from '@tauri-apps/api/event';
import { readDir, stat, remove, rename, mkdir } from '@tauri-apps/plugin-fs';
import { homeDir } from '@tauri-apps/api/path';
import { open } from '@tauri-apps/plugin-dialog';
import { registerSftpContext, unregisterSftpContext } from '../../lib/sftpContextRegistry';

// 🔴 Key-Driven: 全局路径记忆 Map — keyed by nodeId (stable across reconnects)
const sftpPathMemory = new Map<string, string>();

function isRecoverableSftpChannelError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return (
    message.includes('ConnectFailed') ||
    message.includes('Channel error') ||
    message.includes('InvalidSession') ||
    message.includes('session not found')
  );
}

import { 
  Dialog, 
  DialogContent, 
  DialogHeader, 
  DialogTitle, 
  DialogDescription,
  DialogFooter
} from '../ui/dialog';

// Types for Transfer Events (should match Backend TransferProgress)
interface TransferProgressEvent {
    id: string;
    remote_path: string;
    local_path: string;
    direction: string;
    state: string;
    total_bytes: number;
    transferred_bytes: number;
    speed: number;
    eta_seconds: number | null;
    error: string | null;
}

interface TransferCompleteEvent {
    transfer_id: string;
    session_id: string;
    success: boolean;
    error?: string;
}

// Format file size to human readable format
const formatFileSize = (bytes: number): string => {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const size = bytes / Math.pow(1024, i);
  return `${size.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
};

// Sort options
type SortField = 'name' | 'size' | 'modified';
type SortDirection = 'asc' | 'desc';

const FileList = ({ 
  title, 
  path, 
  files, 
  onNavigate, 
  onRefresh,
  active,
  onActivate,
  onPreview,
  onTransfer,
  onDelete,
  onRename,
  onNewFolder,
  selected,
  setSelected,
  lastSelected,
  setLastSelected,
  isDragOver,
  onDragOver,
  onDragLeave,
  onDrop,
  filter,
  onFilterChange,
  sortField,
  sortDirection,
  onSortChange,
  onBrowse,
  onShowDrives,
  isPathEditable = false,
  pathInputValue,
  onPathInputChange,
  onPathInputSubmit,
  onPathEditStart,
  onPathEditCancel,
  isRemote = false,
  loading = false,
  t
}: { 
  title: string, 
  path: string, 
  files: FileInfo[],
  onNavigate: (path: string) => void,
  onRefresh: () => void,
  active: boolean,
  onActivate: () => void,
  onPreview?: (file: FileInfo) => void,
  onTransfer?: (files: string[], direction: 'upload' | 'download') => void,
  onDelete?: (files: string[]) => void,
  onRename?: (oldName: string) => void,
  onNewFolder?: () => void,
  selected: Set<string>,
  setSelected: (s: Set<string>) => void,
  lastSelected: string | null,
  setLastSelected: (s: string | null) => void,
  isDragOver?: boolean,
  onDragOver?: (e: React.DragEvent) => void,
  onDragLeave?: (e: React.DragEvent) => void,
  onDrop?: (e: React.DragEvent) => void,
  filter?: string,
  onFilterChange?: (v: string) => void,
  sortField?: SortField,
  sortDirection?: SortDirection,
  onSortChange?: (field: SortField) => void,
  onBrowse?: () => void,
  onShowDrives?: () => void,
  isPathEditable?: boolean,
  pathInputValue?: string,
  onPathInputChange?: (v: string) => void,
  onPathInputSubmit?: () => void,
  onPathEditStart?: () => void,
  onPathEditCancel?: () => void,
  isRemote?: boolean,
  loading?: boolean,
  t: (key: string, options?: Record<string, unknown>) => string
}) => {
  const listRef = useRef<HTMLDivElement>(null);
  const [contextMenu, setContextMenu] = useState<{x: number, y: number, file?: FileInfo} | null>(null);

  const handleSelect = (name: string, multi: boolean, range: boolean) => {
    onActivate();
    const newSelected = new Set(multi ? selected : []);
    
    if (range && lastSelected && files.length > 0) {
       let start = files.findIndex(f => f.name === lastSelected);
       let end = files.findIndex(f => f.name === name);
       if (start > -1 && end > -1) {
           const [min, max] = [Math.min(start, end), Math.max(start, end)];
           for (let i = min; i <= max; i++) {
               newSelected.add(files[i].name);
           }
       }
    } else {
        if (newSelected.has(name) && multi) {
            newSelected.delete(name);
        } else {
            newSelected.add(name);
        }
    }
    
    setSelected(newSelected);
    setLastSelected(name);
  };

  // Handle keyboard shortcuts
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (!active) return;
    
    const selectedFiles = Array.from(selected);
    const isLocalPane = !isRemote;
    
    // Ctrl/Cmd + A: Select all
    if ((e.metaKey || e.ctrlKey) && e.key === 'a') {
      e.preventDefault();
      setSelected(new Set(files.map(f => f.name)));
      return;
    }
    
    // Enter: Open directory
    if (e.key === 'Enter' && selectedFiles.length === 1) {
      e.preventDefault();
      const file = files.find(f => f.name === selectedFiles[0]);
      if (file && file.file_type === 'Directory') {
        const newPath = path === '/' ? `/${file.name}` : `${path}/${file.name}`;
        onNavigate(newPath);
      }
      return;
    }

    // Space: Quick look / preview file
    if (e.key === ' ' && selectedFiles.length === 1) {
      e.preventDefault();
      const file = files.find(f => f.name === selectedFiles[0]);
      if (file && file.file_type !== 'Directory' && onPreview) {
        onPreview(file);
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

    // Arrow Up/Down: Navigate file list
    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault();
      if (files.length === 0) return;
      const currentName = selectedFiles.length === 1 ? selectedFiles[0] : null;
      const currentIndex = currentName ? files.findIndex(f => f.name === currentName) : -1;
      let nextIndex: number;
      if (e.key === 'ArrowDown') {
        nextIndex = currentIndex < files.length - 1 ? currentIndex + 1 : 0;
      } else {
        nextIndex = currentIndex > 0 ? currentIndex - 1 : files.length - 1;
      }
      const nextFile = files[nextIndex];
      setSelected(new Set([nextFile.name]));
      setLastSelected(nextFile.name);
      // Scroll the selected item into view
      const container = listRef.current;
      if (container) {
        const row = container.querySelector(`[data-filename="${CSS.escape(nextFile.name)}"]`) as HTMLElement | null;
        row?.scrollIntoView({ block: 'nearest' });
      }
      return;
    }
  }, [active, selected, files, isRemote, path, onNavigate, onPreview, onTransfer, onDelete, onRename, setSelected, setLastSelected]);

  // Context menu handler
  const handleContextMenu = (e: React.MouseEvent, file?: FileInfo) => {
    e.preventDefault();
    e.stopPropagation();
    if (file && !selected.has(file.name)) {
      setSelected(new Set([file.name]));
      setLastSelected(file.name);
    }
    setContextMenu({ x: e.clientX, y: e.clientY, file });
  };

  // Close context menu on click outside
  useEffect(() => {
    const handleClick = () => setContextMenu(null);
    if (contextMenu) {
      document.addEventListener('click', handleClick);
      return () => document.removeEventListener('click', handleClick);
    }
  }, [contextMenu]);

  const isLocalPane = !isRemote;

  return (
    <div 
      className={cn(
        "flex flex-col h-full bg-theme-bg border transition-all duration-200",
        active ? "border-oxide-accent/50" : "border-theme-border",
        isDragOver && "border-oxide-accent border-2 bg-theme-accent/10 ring-2 ring-oxide-accent/30"
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
        active ? "bg-theme-bg-hover/50 border-oxide-accent/30" : "bg-theme-bg-panel border-theme-border"
      )}>
        <span className="font-semibold text-xs text-theme-text-muted uppercase tracking-wider min-w-12">{title}</span>
        {/* Path bar - breadcrumb navigation or editable input */}
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
                // Don't cancel if clicking the Go button (it would unmount before onClick fires)
                const related = e.relatedTarget as HTMLElement | null;
                if (related?.closest('[data-path-go-btn]')) return;
                onPathEditCancel?.();
              }}
              className="flex-1 bg-transparent text-theme-text text-xs outline-none"
              placeholder={t('sftp.file_list.path_placeholder')}
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
            <Button data-path-go-btn size="icon" variant="ghost" className="h-4 w-4 shrink-0" onClick={onPathInputSubmit} title={t('sftp.file_list.go')}>
              <CornerDownLeft className="h-3 w-3" />
            </Button>
          )}
        </div>
        {/* Show drives button (local only) */}
        {onShowDrives && (
          <Button size="icon" variant="ghost" className="h-6 w-6" onClick={onShowDrives} title={t('sftp.toolbar.show_drives')}>
            <HardDrive className="h-3 w-3" />
          </Button>
        )}
        {/* Browse button (local only) */}
        {onBrowse && (
          <Button size="icon" variant="ghost" className="h-6 w-6" onClick={onBrowse} title={t('sftp.toolbar.browse_folder')}>
            <FolderOpen className="h-3 w-3" />
          </Button>
        )}
        <Button size="icon" variant="ghost" className="h-6 w-6" onClick={() => onNavigate('..')} title={t('sftp.toolbar.go_up')}>
           <ArrowUp className="h-3 w-3" />
        </Button>
        <Button size="icon" variant="ghost" className="h-6 w-6" onClick={() => onNavigate('~')} title={t('sftp.toolbar.home')}>
           <Home className="h-3 w-3" />
        </Button>
        <Button size="icon" variant="ghost" className="h-6 w-6" onClick={onRefresh} title={t('sftp.toolbar.refresh')}>
           <RefreshCw className="h-3 w-3" />
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
            {isLocalPane ? t('sftp.toolbar.upload_count', { count: selected.size }) : t('sftp.toolbar.download_count', { count: selected.size })}
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
          {t('sftp.file_list.col_name')}
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
          {t('sftp.file_list.col_size')}
          {sortField === 'size' && <ArrowUpDown className="h-3 w-3" />}
        </button>
        <button 
          className={cn(
            "w-24 flex items-center justify-end gap-1 hover:text-theme-text transition-colors",
            sortField === 'modified' && "text-theme-accent"
          )}
          onClick={() => onSortChange?.('modified')}
        >
          {t('sftp.file_list.col_modified')}
          {sortField === 'modified' && <ArrowUpDown className="h-3 w-3" />}
        </button>
      </div>

      {/* Filter Input */}
      {onFilterChange && (
        <div className="flex items-center gap-2 px-2 py-1 bg-theme-bg-panel/80 border-b border-theme-border">
          <Search className="h-3 w-3 text-theme-text-muted" />
          <input
            type="text"
            value={filter || ''}
            onChange={(e) => onFilterChange(e.target.value)}
            placeholder={t('sftp.file_list.filter_placeholder')}
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
        onClick={() => setSelected(new Set())}
        onKeyDown={handleKeyDown}
      >
        {loading ? (
          <div className="flex items-center justify-center py-12 text-theme-text-muted">
            <Loader2 className="h-5 w-5 animate-spin mr-2" />
            <span className="text-xs">{t('sftp.file_list.loading')}</span>
          </div>
        ) : files.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-theme-text-muted">
            <FolderOpen className="h-8 w-8 mb-2 opacity-40" />
            <span className="text-xs">{t('sftp.file_list.empty')}</span>
          </div>
        ) : files.map((file) => {
          const isSelected = selected.has(file.name);
          return (
            <div 
              key={file.name}
              data-filename={file.name}
              draggable
              onDragStart={(e) => {
                  e.dataTransfer.setData('application/json', JSON.stringify({
                      files: Array.from(selected.size > 0 ? selected : [file.name]),
                      source: title.includes('Remote') ? 'remote' : 'local',
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
                "flex items-center px-2 py-1 text-xs cursor-default select-none border-b border-transparent hover:bg-theme-bg-hover",
                isSelected && "bg-theme-accent/20 text-theme-accent"
              )}
            >
              <div className="flex-1 flex items-center gap-2 min-w-0">
                {file.file_type === 'Directory' ? <Folder className="h-3.5 w-3.5 flex-shrink-0 text-blue-400" /> : <File className="h-3.5 w-3.5 flex-shrink-0 text-theme-text-muted" />}
                <span className="truncate">{file.name}</span>
              </div>
              <div className="w-20 text-right text-theme-text-muted">
                {file.file_type === 'Directory' ? '-' : formatFileSize(file.size)}
              </div>
              <div className="w-24 text-right text-theme-text-muted">
                {file.modified ? new Date(file.modified * 1000).toLocaleDateString() : '-'}
              </div>
            </div>
          );
        })}
      </div>

      {/* Context Menu — rendered via Portal to escape contain:layout containment */}
      {contextMenu && createPortal(
        <div 
          className="fixed z-50 bg-theme-bg-elevated border border-theme-border rounded-sm shadow-lg py-1 min-w-[180px]"
          style={{ left: contextMenu.x, top: contextMenu.y }}
        >
          {/* Transfer */}
          {onTransfer && selected.size > 0 && (
            <button 
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => {
                onTransfer(Array.from(selected), isLocalPane ? 'upload' : 'download');
                setContextMenu(null);
              }}
            >
              {isLocalPane ? <Upload className="h-3 w-3" /> : <Download className="h-3 w-3" />}
              {isLocalPane ? t('sftp.context.upload') : t('sftp.context.download')}
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
              <Eye className="h-3 w-3" /> {t('sftp.context.preview')}
            </button>
          )}
          
          {/* Rename */}
          {contextMenu.file && selected.size === 1 && onRename && (
            <button 
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => {
                onRename(contextMenu.file!.name);
                setContextMenu(null);
              }}
            >
              <Edit3 className="h-3 w-3" /> {t('sftp.context.rename')}
            </button>
          )}
          
          {/* Copy Path */}
          {contextMenu.file && (
            <button 
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => {
                const fullPath = `${path}/${contextMenu.file!.name}`;
                navigator.clipboard.writeText(fullPath);
                setContextMenu(null);
              }}
            >
              <Copy className="h-3 w-3" /> {t('sftp.context.copy_path')}
            </button>
          )}
          
          {/* Delete */}
          {selected.size > 0 && onDelete && (
            <button 
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2 text-red-400"
              onClick={() => {
                onDelete(Array.from(selected));
                setContextMenu(null);
              }}
            >
              <Trash2 className="h-3 w-3" /> {t('sftp.context.delete')}
            </button>
          )}
          
          <div className="border-t border-theme-border my-1" />
          
          {/* New Folder */}
          {onNewFolder && (
            <button 
              className="w-full px-3 py-1.5 text-left text-xs hover:bg-theme-bg-hover flex items-center gap-2"
              onClick={() => {
                onNewFolder();
                setContextMenu(null);
              }}
            >
              <FolderPlus className="h-3 w-3" /> {t('sftp.context.new_folder')}
            </button>
          )}
        </div>,
        document.body
      )}
    </div>
  );
}

/**
 * Tiny wrapper for SFTP media previews (audio / video) that properly releases
 * browser-buffered decoded media data on unmount to prevent memory leaks.
 */
const SFTPMediaPreview: React.FC<{
  type: 'audio' | 'video';
  src: string;
  name: string;
  fallbackText: string;
}> = ({ type, src, name, fallbackText }) => {
  const ref = useRef<HTMLAudioElement & HTMLVideoElement>(null);

  useEffect(() => {
    return () => {
      const el = ref.current;
      if (el) {
        el.pause();
        el.removeAttribute('src');
        el.load();
      }
    };
  }, []);

  if (type === 'video') {
    return (
      <div className="flex items-center justify-center h-full p-4">
        <video ref={ref} controls className="max-w-full max-h-full" src={src}>
          {fallbackText}
        </video>
      </div>
    );
  }

  return (
    <div className="flex flex-col items-center justify-center h-full p-4 gap-4">
      <div className="text-6xl">🎵</div>
      <div className="text-theme-text-muted">{name}</div>
      <audio ref={ref} controls className="w-full max-w-md" src={src}>
        {fallbackText}
      </audio>
    </div>
  );
};

// Module-level cache for tar capability probes, keyed by nodeId.
// Survives component remounts (e.g. reconnect) without re-probing.
type TarCompressionKind = 'zstd' | 'gzip' | 'none';
const tarSupportCache = new Map<string, boolean>();
const tarCompressionCache = new Map<string, TarCompressionKind>();
const tarProbePromises = new Map<string, Promise<boolean>>();
const tarCompressionProbePromises = new Map<string, Promise<TarCompressionKind>>();

export const SFTPView = ({ nodeId }: { nodeId: string }) => {
  const { t } = useTranslation();
  const bgActive = useTabBgActive('sftp');
  const { getSession } = useAppStore();

  // Memory key: always nodeId for path persistence across reconnects
  const memoryKey = nodeId;

  // Get session info for display (host, username) from sessionTreeStore
  const treeNode = useSessionTreeStore(state => state.getNode(nodeId));
  const session = treeNode ? getSession(treeNode.runtime.connectionId || '') : undefined;
  const { error: toastError } = useToast();
  const [remoteFiles, setRemoteFiles] = useState<FileInfo[]>([]);
  const [remotePath, setRemotePath] = useState('');
  const [remoteHome, setRemoteHome] = useState('');
  const [remoteLoading, setRemoteLoading] = useState(false);
  const previousRemotePathRef = useRef('');
  
  const [localFiles, setLocalFiles] = useState<FileInfo[]>([]);
  const [localPath, setLocalPath] = useState('');
  const [localHome, setLocalHome] = useState('');

  const [activePane, setActivePane] = useState<'local' | 'remote'>('remote');
  const [sftpInitialized, setSftpInitialized] = useState(false);
  const [initError, setInitError] = useState<string | null>(null);
  const [initRetryTick, setInitRetryTick] = useState(0);
  const initializingRef = useRef(false);
  const guardErrorNotifiedRef = useRef(false);

  // Path input state for editable path bars
  const [localPathInput, setLocalPathInput] = useState('');
  const [remotePathInput, setRemotePathInput] = useState('');
  const [isLocalPathEditing, setIsLocalPathEditing] = useState(false);
  const [isRemotePathEditing, setIsRemotePathEditing] = useState(false);

  // Drives dialog state (cross-platform volume detection)
  const [showDrivesDialog, setShowDrivesDialog] = useState(false);
  const [availableDrives, setAvailableDrives] = useState<DriveInfo[]>([]);

  // Selection state (lifted up for cross-pane operations)
  const [localSelected, setLocalSelected] = useState<Set<string>>(new Set());
  const [localLastSelected, setLocalLastSelected] = useState<string | null>(null);
  const [remoteSelected, setRemoteSelected] = useState<Set<string>>(new Set());
  const [remoteLastSelected, setRemoteLastSelected] = useState<string | null>(null);

  // Preview State
  const [previewFile, setPreviewFile] = useState<{
    name: string;
    path: string;
    type: 'text' | 'image' | 'video' | 'audio' | 'pdf' | 'office' | 'hex' | 'too-large' | 'unsupported';
    data: string;
    mimeType?: string;
    language?: string | null;
    encoding?: string; // Detected file encoding
    /** asset:// URL for streaming media from a temp file (set when backend returns AssetFile) */
    assetSrc?: string;
    /** Raw local path of the temp file, for cleanup */
    tempPath?: string;
    // Hex specific
    hexOffset?: number;
    hexTotalSize?: number;
    hexHasMore?: boolean;
    // Too large specific
    recommendDownload?: boolean;
    maxSize?: number;
    fileSize?: number;
    // Unsupported specific
    reason?: string;
  } | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [hexLoadingMore, setHexLoadingMore] = useState(false);
  const [sftpPdfZoom, setSftpPdfZoom] = useState(1);

  // Dialog States
  const [renameDialog, setRenameDialog] = useState<{oldName: string, isRemote: boolean} | null>(null);
  const [newFolderDialog, setNewFolderDialog] = useState<{isRemote: boolean} | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<{files: string[], isRemote: boolean} | null>(null);
  const [inputValue, setInputValue] = useState('');

  // Transfer conflict dialog state
  const [conflictDialog, setConflictDialog] = useState<{
    conflicts: ConflictInfo[];
    currentIndex: number;
    pendingTransfers: Array<{
      file: string;
      direction: 'upload' | 'download';
      basePath: string;
      fileInfo: FileInfo | undefined;
    }>;
    resolvedActions: Map<string, ConflictResolution>;
  } | null>(null);

  // Diff compare dialog state
  const [diffDialog, setDiffDialog] = useState<{
    localFile: { path: string; content: string };
    remoteFile: { path: string; content: string };
  } | null>(null);

  // IDE Mode: Remote file editor state
  const [editorFile, setEditorFile] = useState<{
    path: string;
    content: string;
    language: string | null;
    encoding: string;
  } | null>(null);

  // Drag and Drop state
  const [localDragOver, setLocalDragOver] = useState(false);
  const [remoteDragOver, setRemoteDragOver] = useState(false);

  // Filter and Sort state
  const [localFilter, setLocalFilter] = useState('');
  const [remoteFilter, setRemoteFilter] = useState('');
  const [localSortField, setLocalSortField] = useState<SortField>('name');
  const [localSortDirection, setLocalSortDirection] = useState<SortDirection>('asc');
  const [remoteSortField, setRemoteSortField] = useState<SortField>('name');
  const [remoteSortDirection, setRemoteSortDirection] = useState<SortDirection>('asc');

  const recoverSftpSession = useCallback(async (): Promise<string> => {
    // 🔧 SFTP 静默重建：后端 invalidate_and_reacquire_sftp 会在
    // 现有 SSH 连接上重建 SFTP 通道，不影响终端。
    // 前端只需重新调用 nodeSftpInit 触发后端重建流程。
    console.info(`[SFTPView] Recovering SFTP session for node ${nodeId}`);
    return await nodeSftpInit(nodeId);
  }, [nodeId]);

  // Sort handler
  const handleSortChange = (isLocal: boolean, field: SortField) => {
    if (isLocal) {
      if (localSortField === field) {
        setLocalSortDirection(d => d === 'asc' ? 'desc' : 'asc');
      } else {
        setLocalSortField(field);
        setLocalSortDirection('asc');
      }
    } else {
      if (remoteSortField === field) {
        setRemoteSortDirection(d => d === 'asc' ? 'desc' : 'asc');
      } else {
        setRemoteSortField(field);
        setRemoteSortDirection('asc');
      }
    }
  };

  // Filter and sort files
  const filterAndSortFiles = useCallback((
    files: FileInfo[],
    filter: string,
    sortField: SortField,
    sortDirection: SortDirection
  ): FileInfo[] => {
    // Filter
    let filtered = files;
    if (filter.trim()) {
      const lowerFilter = filter.toLowerCase();
      filtered = files.filter(f => f.name.toLowerCase().includes(lowerFilter));
    }

    // Sort (directories first, then by field)
    const sorted = [...filtered].sort((a, b) => {
      // Directories always first
      if (a.file_type === 'Directory' && b.file_type !== 'Directory') return -1;
      if (a.file_type !== 'Directory' && b.file_type === 'Directory') return 1;

      let cmp = 0;
      switch (sortField) {
        case 'name':
          cmp = a.name.localeCompare(b.name);
          break;
        case 'size':
          cmp = a.size - b.size;
          break;
        case 'modified':
          cmp = (a.modified || 0) - (b.modified || 0);
          break;
      }
      return sortDirection === 'asc' ? cmp : -cmp;
    });

    return sorted;
  }, []);

  // Memoized filtered/sorted file lists
  const displayLocalFiles = useMemo(
    () => filterAndSortFiles(localFiles, localFilter, localSortField, localSortDirection),
    [localFiles, localFilter, localSortField, localSortDirection, filterAndSortFiles]
  );

  const displayRemoteFiles = useMemo(
    () => filterAndSortFiles(remoteFiles, remoteFilter, remoteSortField, remoteSortDirection),
    [remoteFiles, remoteFilter, remoteSortField, remoteSortDirection, filterAndSortFiles]
  );

  // Initialize local home directory
  useEffect(() => {
    homeDir().then(home => {
      setLocalHome(home);
      setLocalPath(home);
      setLocalPathInput(home);
    }).catch(() => {
      setLocalPath('/');
      setLocalPathInput('/');
    });
  }, []);

  // Sync path input when path changes
  useEffect(() => {
    if (!isLocalPathEditing) {
      setLocalPathInput(localPath);
    }
  }, [localPath, isLocalPathEditing]);

  useEffect(() => {
    if (!isRemotePathEditing) {
      setRemotePathInput(remotePath);
    }
  }, [remotePath, isRemotePathEditing]);

  // Keyboard shortcut: Cmd/Ctrl+L to toggle path editing on active pane
  const activePaneRef = useRef(activePane);
  activePaneRef.current = activePane;
  const localEditingRef = useRef(isLocalPathEditing);
  localEditingRef.current = isLocalPathEditing;
  const remoteEditingRef = useRef(isRemotePathEditing);
  remoteEditingRef.current = isRemotePathEditing;
  const localPathRef = useRef(localPath);
  localPathRef.current = localPath;
  const remotePathRef = useRef(remotePath);
  remotePathRef.current = remotePath;

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'l') {
        e.preventDefault();
        if (activePaneRef.current === 'local') {
          if (localEditingRef.current) {
            setLocalPathInput(localPathRef.current);
            setIsLocalPathEditing(false);
          } else {
            setLocalPathInput(localPathRef.current);
            setIsLocalPathEditing(true);
          }
        } else {
          if (remoteEditingRef.current) {
            setRemotePathInput(remotePathRef.current);
            setIsRemotePathEditing(false);
          } else {
            setRemotePathInput(remotePathRef.current);
            setIsRemotePathEditing(true);
          }
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // 🔴 路径记忆：卸载前保存当前路径（使用 memoryKey 以支持 nodeId 代理）
  useEffect(() => {
    return () => {
      // 组件卸载时，保存当前路径到全局 Map（用于重连后恢复）
      if (remotePath && remotePath !== '/' && remotePath !== '/home') {
        sftpPathMemory.set(memoryKey, remotePath);
        console.debug(`[SFTPView] Path saved for key ${memoryKey}: ${remotePath}`);
      }
    };
  }, [memoryKey, remotePath]);

  // Register SFTP context for AI sidebar awareness
  useEffect(() => {
    return () => {
      unregisterSftpContext(nodeId);
      // Clean up module-level tar probe caches for this node
      tarSupportCache.delete(nodeId);
      tarCompressionCache.delete(nodeId);
    };
  }, [nodeId]);

  useEffect(() => {
    if (remotePath) {
      registerSftpContext(nodeId, remotePath, remoteHome, Array.from(remoteSelected));
    }
  }, [nodeId, remotePath, remoteHome, remoteSelected]);

  // 🔴 初始化模型（node-first 模式）
  // Max auto-retries: connection errors get 1 retry, channel errors get 2
  const MAX_INIT_AUTO_RETRIES = 3;
  useEffect(() => {
    let cancelled = false;

    // Dedup guard: prevent concurrent init calls from React double-render or
    // rapid retryTick updates (e.g. ideStore + SFTPView both calling init).
    if (initializingRef.current) {
      console.debug(`[SFTPView] Init already in progress, skipping (retryTick=${initRetryTick})`);
      return;
    }
    initializingRef.current = true;
    
    const init = async () => {
      console.info(`[SFTPView] Initializing SFTP: nodeId=${nodeId}, retryTick=${initRetryTick}`);
      
      try {
        // node-first: nodeSftpInit 是幂等的
        let cwd: string;
        // node-first: nodeSftpInit 触发后端 acquire_sftp，
        // 后续操作通过 sftp_with_retry 自动处理通道错误。
        cwd = await nodeSftpInit(nodeId);
        if (cancelled) return;

        setSftpInitialized(true);
        setInitError(null);
        guardErrorNotifiedRef.current = false;
        
        // 记住 SFTP 返回的真实 home 目录（用于 ~ 导航）
        if (cwd) setRemoteHome(cwd);
        
        // 🔴 路径继承：优先恢复记忆的路径，否则使用 SFTP 返回的 cwd
        const savedPath = sftpPathMemory.get(memoryKey);
        const targetPath = savedPath || cwd || '/';
        setRemotePath(targetPath);
        
        console.info(`[SFTPView] SFTP ready: cwd=${cwd}, restored=${savedPath}, using=${targetPath}`);
      } catch (err) {
        if (cancelled) return;
        const message = err instanceof Error ? err.message : String(err);
        console.error(`[SFTPView] Init failed:`, err);
        
        // Auto-retry for transient errors, with a hard cap
        if (initRetryTick < MAX_INIT_AUTO_RETRIES) {
          const isConnectionError = message.includes('Not connected') || 
                                    message.includes('NotConnected') ||
                                    message.includes('Connection timeout') ||
                                    message.includes('ConnectionTimeout');
          const isChannelError = isRecoverableSftpChannelError(err);
          if (isConnectionError || isChannelError) {
            // Exponential backoff: 2s, 4s, 8s...
            const delay = 2000 * Math.pow(2, initRetryTick);
            console.info(`[SFTPView] Transient error, scheduling auto-retry #${initRetryTick + 1} in ${delay}ms...`);
            const timer = setTimeout(() => {
              if (!cancelled) setInitRetryTick(t => t + 1);
            }, delay);
            return () => clearTimeout(timer);
          }
        }
        
        setSftpInitialized(false);
        setInitError(message);
      } finally {
        initializingRef.current = false;
      }
    };

    init();
    return () => {
      cancelled = true;
      initializingRef.current = false;
    };
  }, [nodeId, initRetryTick, memoryKey, recoverSftpSession]);

  // Refresh remote (only after initialization)
  useEffect(() => {
     if (!sftpInitialized || !remotePath) return;
     let cancelled = false;

     const refresh = async () => {
        setRemoteLoading(true);
        try {
          const files = await nodeSftpListDir(nodeId, remotePath);
          if (!cancelled) {
            setRemoteFiles(files);
            previousRemotePathRef.current = remotePath;
          }
        } catch (err) {
          if (!cancelled) {
            const errMsg = String(err);
            console.error("SFTP List Error:", err);
            
            const isPermissionDenied = errMsg.includes('Permission denied') || errMsg.includes('permission denied') || errMsg.includes('PermissionDenied');
            const isNotFound = errMsg.includes('not found') || errMsg.includes('No such file');

            if (isPermissionDenied) {
              toastError(t('sftp.toast.permission_denied'), t('sftp.toast.permission_denied_path', { path: remotePath }));
              // Revert to previous working path
              const fallback = previousRemotePathRef.current || remoteHome || '/';
              if (remotePath !== fallback) {
                setRemotePath(fallback);
              }
            } else if (isNotFound && remotePath !== '/') {
              console.warn(`[SFTPView] Path "${remotePath}" not found, falling back to /`);
              setRemotePath('/');
            }
          }
        } finally {
          if (!cancelled) setRemoteLoading(false);
        }
     };

     refresh();
     return () => {
       cancelled = true;
     };
  }, [nodeId, remotePath, sftpInitialized]);

  // Refresh local files using Tauri fs plugin
  const refreshLocalFiles = useCallback(async () => {
    if (!localPath) return;
    try {
      const entries = await readDir(localPath);
      const files: FileInfo[] = await Promise.all(
        entries.map(async (entry) => {
          const fullPath = `${localPath}/${entry.name}`;
          const isDir = entry.isDirectory === true;
          try {
            const info = await stat(fullPath);
            return {
              name: entry.name,
              path: fullPath,
              file_type: isDir ? 'Directory' : 'File',
              size: info.size || 0,
              modified: info.mtime ? Math.floor(info.mtime.getTime() / 1000) : 0,
              permissions: ''
            } satisfies FileInfo;
          } catch {
            return {
              name: entry.name,
              path: fullPath,
              file_type: isDir ? 'Directory' : 'File',
              size: 0,
              modified: 0,
              permissions: ''
            } satisfies FileInfo;
          }
        })
      );
      // Sort: directories first, then alphabetically
      files.sort((a, b) => {
        if (a.file_type === 'Directory' && b.file_type !== 'Directory') return -1;
        if (a.file_type !== 'Directory' && b.file_type === 'Directory') return 1;
        return a.name.localeCompare(b.name);
      });
      setLocalFiles(files);
    } catch (err) {
      console.error("Local list error:", err);
      setLocalFiles([]);
    }
  }, [localPath]);

  useEffect(() => {
    refreshLocalFiles();
  }, [refreshLocalFiles]);

  // Cross-platform path utilities
  const getParentPath = useCallback((currentPath: string, isRemote: boolean): string => {
    if (isRemote) {
      // Remote: always use / separator
      const parts = currentPath.split('/').filter(Boolean);
      parts.pop();
      return parts.length === 0 ? '/' : '/' + parts.join('/');
    } else {
      // Local: handle Windows drive letters and Unix paths
      // Check for Windows drive root (C:\, D:\, etc.)
      if (/^[A-Za-z]:\\?$/.test(currentPath) || /^[A-Za-z]:$/.test(currentPath)) {
        // Already at drive root, show drives dialog
        return '__DRIVES__';
      }
      // Check for Unix root
      if (currentPath === '/') {
        return '/';
      }
      // Handle both separators
      const normalized = currentPath.replace(/\\/g, '/');
      const parts = normalized.split('/').filter(Boolean);
      parts.pop();
      
      // If on Windows and went up to drive letter
      if (parts.length === 1 && /^[A-Za-z]:$/.test(parts[0])) {
        return parts[0] + '\\';
      }
      // Unix or Windows path
      if (parts.length === 0) {
        // Check if original was Windows path
        if (/^[A-Za-z]:/.test(currentPath)) {
          return currentPath.substring(0, 3); // Keep drive root like C:\
        }
        return '/';
      }
      // Reconstruct with proper separator
      const separator = currentPath.includes('\\') ? '\\' : '/';
      const result = parts.join(separator);
      // Ensure Windows paths have trailing slash for root
      if (/^[A-Za-z]:$/.test(result)) {
        return result + '\\';
      }
      return currentPath.startsWith('/') ? '/' + result : result;
    }
  }, []);

  // Browse folder using system dialog (local only)
  const handleBrowseFolder = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: localPath || undefined
      });
      if (selected && typeof selected === 'string') {
        setLocalPath(selected);
        setLocalPathInput(selected);
        setIsLocalPathEditing(false);
      }
    } catch (err) {
      console.error('Browse folder error:', err);
    }
  }, [localPath]);

  // Show drives dialog
  const handleShowDrives = useCallback(async () => {
    try {
      const drives = await api.localGetDrives();
      setAvailableDrives(drives);
      setShowDrivesDialog(true);
    } catch (err) {
      console.error('Get drives error:', err);
      // Fallback to root
      setAvailableDrives([{ path: '/', name: 'System', driveType: 'system', totalSpace: 0, availableSpace: 0, isReadOnly: false }]);
      setShowDrivesDialog(true);
    }
  }, []);

  // Navigate to drive
  const handleSelectDrive = useCallback((drivePath: string) => {
    setLocalPath(drivePath);
    setLocalPathInput(drivePath);
    setShowDrivesDialog(false);
  }, []);

  // Handle local path navigation with Windows support
  const handleLocalNavigate = useCallback((target: string) => {
    if (target === '..') {
      const parent = getParentPath(localPath, false);
      if (parent === '__DRIVES__') {
        handleShowDrives();
      } else {
        setLocalPath(parent);
      }
    } else if (target === '~') {
      setLocalPath(localHome);
    } else {
      setLocalPath(target);
    }
    setIsLocalPathEditing(false);
  }, [localPath, localHome, getParentPath, handleShowDrives]);

  // Handle remote path navigation
  const handleRemoteNavigate = useCallback((target: string) => {
    if (target === '..') {
      setRemotePath(getParentPath(remotePath, true));
    } else if (target === '~') {
      setRemotePath(remoteHome || '/');
    } else {
      setRemotePath(target);
    }
    setIsRemotePathEditing(false);
  }, [remotePath, remoteHome, getParentPath]);

  // Handle path input submission
  const handleLocalPathSubmit = useCallback(() => {
    const trimmed = localPathInput.trim();
    if (trimmed) {
      setLocalPath(trimmed);
    }
    setIsLocalPathEditing(false);
  }, [localPathInput]);

  const handleRemotePathSubmit = useCallback(() => {
    let trimmed = remotePathInput.trim();
    if (trimmed) {
      // Remote paths must be absolute
      if (!trimmed.startsWith('/')) {
        trimmed = '/' + trimmed;
      }
      setRemotePath(trimmed);
    }
    setIsRemotePathEditing(false);
  }, [remotePathInput]);

  // Get transfer store actions
  const { addTransfer, updateProgress, setTransferState, getAllTransfers } = useTransferStore();

  // Event Listeners for Transfer Progress
  useEffect(() => {
      // 使用 mounted 标志防止组件卸载后仍处理事件
      let mounted = true;
      let unlistenProgressFn: (() => void) | null = null;
      let unlistenCompleteFn: (() => void) | null = null;
      
      // Setup progress listener
      listen<TransferProgressEvent>(`sftp:progress:${nodeId}`, (event) => {
        if (!mounted) return; // 组件已卸载，忽略事件
        
        const { id, remote_path, local_path, transferred_bytes, total_bytes, speed } = event.payload;
        // Prefer matching by transfer_id for accuracy; fall back to path matching
        const transfers = getAllTransfers();
        const normalizePath = (p: string) => p.replace(/\/+/g, '/').replace(/\/$/, '');
        
        let match = transfers.find(t => t.id === id);
        
        if (!match) {
          // Fallback: match by exact normalized paths
          const normalizedRemote = normalizePath(remote_path);
          const normalizedLocal = normalizePath(local_path);
          match = transfers.find(t => {
            const tRemote = normalizePath(t.remotePath);
            const tLocal = normalizePath(t.localPath);
            return tRemote === normalizedRemote || tLocal === normalizedLocal;
          });
        }
        
        if (match) {
          updateProgress(match.id, transferred_bytes, total_bytes, speed);
        } else {
          console.log('[SFTP Progress] No match found for:', { id, remote_path, local_path, transfers: transfers.map(t => ({ id: t.id, remotePath: t.remotePath, localPath: t.localPath })) });
        }
      }).then((fn) => {
        if (mounted) {
          unlistenProgressFn = fn;
        } else {
          fn(); // Component unmounted, clean up immediately
        }
      });
      
      // Setup complete listener
      listen<TransferCompleteEvent>(`sftp:complete:${nodeId}`, (event) => {
        if (!mounted) return; // 组件已卸载，忽略事件
        
        const { transfer_id, success, error } = event.payload;
        if (success) {
            setTransferState(transfer_id, 'completed');
            // Refresh file lists
            refreshLocalFiles();
            nodeSftpListDir(nodeId, remotePath).then(setRemoteFiles);
        } else {
            setTransferState(transfer_id, 'error', error || 'Transfer failed');
        }
      }).then((fn) => {
        if (mounted) {
          unlistenCompleteFn = fn;
        } else {
          fn(); // Component unmounted, clean up immediately
        }
      });
      
      return () => { 
          mounted = false;
          unlistenProgressFn?.();
          unlistenCompleteFn?.();
      };
  }, [nodeId, updateProgress, setTransferState, refreshLocalFiles, remotePath, getAllTransfers]);

  // Toast notifications
  const { success: toastSuccess } = useToast();

  // Get SFTP settings
  const sftpSettings = useSettingsStore((state) => state.settings.sftp);

  // Generate unique filename for "Keep Both" option
  const generateUniqueName = (name: string, existingFiles: FileInfo[]): string => {
    const existingNames = new Set(existingFiles.map(f => f.name));
    const lastDot = name.lastIndexOf('.');
    const baseName = lastDot > 0 ? name.slice(0, lastDot) : name;
    const ext = lastDot > 0 ? name.slice(lastDot) : '';
    
    let counter = 1;
    let newName = `${baseName} (${counter})${ext}`;
    while (existingNames.has(newName)) {
      counter++;
      newName = `${baseName} (${counter})${ext}`;
    }
    return newName;
  };

  // Try tar streaming for directory transfer; fall back to SFTP if unavailable
  const transferDirWithTarFallback = async (
    nid: string,
    localFile: string,
    remoteFile: string,
    tid: string,
    dir: 'upload' | 'download'
  ) => {
    // Lazy-probe tar support (deduplicated for concurrent transfers, cached per nodeId)
    if (!tarSupportCache.has(nid)) {
      if (!tarProbePromises.has(nid)) {
        tarProbePromises.set(nid, nodeSftpTarProbe(nid)
          .then((supported) => {
            tarSupportCache.set(nid, supported);
            return supported;
          })
          .catch(() => {
            tarSupportCache.set(nid, false);
            return false;
          })
          .finally(() => {
            tarProbePromises.delete(nid);
          }));
      }

      await tarProbePromises.get(nid);
    }

    if (tarSupportCache.get(nid)) {
      // Lazy-probe best compression (deduplicated, cached per nodeId)
      if (!tarCompressionCache.has(nid)) {
        if (!tarCompressionProbePromises.has(nid)) {
          tarCompressionProbePromises.set(nid, nodeSftpTarCompressionProbe(nid)
            .then((comp) => {
              tarCompressionCache.set(nid, comp);
              return comp;
            })
            .catch(() => {
              tarCompressionCache.set(nid, 'none');
              return 'none' as TarCompressionKind;
            })
            .finally(() => {
              tarCompressionProbePromises.delete(nid);
            }));
        }
        await tarCompressionProbePromises.get(nid);
      }

      const comp = tarCompressionCache.get(nid) ?? 'none';
      // Tar fast path (with compression)
      if (dir === 'upload') {
        await nodeSftpTarUpload(nid, localFile, remoteFile, tid, comp);
      } else {
        await nodeSftpTarDownload(nid, remoteFile, localFile, tid, comp);
      }
    } else {
      // SFTP fallback
      if (dir === 'upload') {
        await nodeSftpUploadDir(nid, localFile, remoteFile, tid);
      } else {
        await nodeSftpDownloadDir(nid, remoteFile, localFile, tid);
      }
    }
  };

  // Execute single file transfer
  const executeTransfer = async (
    file: string,
    direction: 'upload' | 'download',
    basePath: string,
    fileInfo: FileInfo | undefined,
    targetFileName?: string  // For rename option
  ): Promise<boolean> => {
    const isDirectory = fileInfo?.file_type === 'Directory';
    const actualFileName = targetFileName || file;
    
    const localFilePath = direction === 'upload' 
      ? `${basePath}/${file}` 
      : `${localPath}/${actualFileName}`;
    const remoteFilePath = direction === 'upload'
      ? `${remotePath}/${actualFileName}`
      : `${basePath}/${file}`;
    
    const transferId = addTransfer({
      id: `${nodeId}-${Date.now()}-${file}`,
      nodeId: nodeId,
      name: isDirectory ? `${actualFileName}/` : actualFileName,
      localPath: localFilePath,
      remotePath: remoteFilePath,
      direction,
      size: fileInfo?.size || 0,
    });
    
    try {
      if (direction === 'upload') {
        if (isDirectory) {
          await transferDirWithTarFallback(nodeId, localFilePath, remoteFilePath, transferId, 'upload');
        } else {
          await nodeSftpUpload(nodeId, localFilePath, remoteFilePath, transferId);
        }
      } else {
        if (isDirectory) {
          await transferDirWithTarFallback(nodeId, localFilePath, remoteFilePath, transferId, 'download');
        } else {
          await nodeSftpDownload(nodeId, remoteFilePath, localFilePath, transferId);
        }
      }
      setTransferState(transferId, 'completed');
      return true;
    } catch (err) {
      console.error("Transfer failed:", err);
      setTransferState(transferId, 'error', String(err));
      return false;
    }
  };

  // Process conflict resolution and continue transfers
  const processConflictResolution = async (
    resolution: ConflictResolution,
    applyToAll: boolean
  ) => {
    if (!conflictDialog) return;
    
    const { conflicts, currentIndex, pendingTransfers, resolvedActions } = conflictDialog;
    const currentConflict = conflicts[currentIndex];
    
    // Store the resolution
    const newResolvedActions = new Map(resolvedActions);
    
    if (applyToAll) {
      // Apply to current and all remaining conflicts
      for (let i = currentIndex; i < conflicts.length; i++) {
        newResolvedActions.set(conflicts[i].fileName, resolution);
      }
      setConflictDialog(null);
      
      // Execute all remaining transfers with resolved actions
      await executeResolvedTransfers(pendingTransfers, newResolvedActions);
    } else {
      newResolvedActions.set(currentConflict.fileName, resolution);
      
      if (currentIndex + 1 < conflicts.length) {
        // Move to next conflict
        setConflictDialog({
          ...conflictDialog,
          currentIndex: currentIndex + 1,
          resolvedActions: newResolvedActions,
        });
      } else {
        // All conflicts resolved, execute transfers
        setConflictDialog(null);
        await executeResolvedTransfers(pendingTransfers, newResolvedActions);
      }
    }
  };

  // Execute transfers with resolved conflict actions
  const executeResolvedTransfers = async (
    pendingTransfers: Array<{
      file: string;
      direction: 'upload' | 'download';
      basePath: string;
      fileInfo: FileInfo | undefined;
    }>,
    resolvedActions: Map<string, ConflictResolution>
  ) => {
    let successCount = 0;
    let failCount = 0;
    let skippedCount = 0;
    
    const targetFiles = pendingTransfers[0]?.direction === 'upload' ? remoteFiles : localFiles;
    
    for (const transfer of pendingTransfers) {
      const resolution = resolvedActions.get(transfer.file);
      
      if (resolution === 'skip' || resolution === 'cancel') {
        skippedCount++;
        continue;
      }
      
      if (resolution === 'skip-older') {
        // Check if source is newer
        const targetFile = targetFiles.find(f => f.name === transfer.file);
        if (targetFile && transfer.fileInfo?.modified && targetFile.modified) {
          if (transfer.fileInfo.modified <= targetFile.modified) {
            skippedCount++;
            continue;
          }
        }
      }
      
      let targetFileName: string | undefined;
      if (resolution === 'rename') {
        targetFileName = generateUniqueName(transfer.file, targetFiles);
      }
      
      const success = await executeTransfer(
        transfer.file,
        transfer.direction,
        transfer.basePath,
        transfer.fileInfo,
        targetFileName
      );
      
      if (success) {
        successCount++;
      } else {
        failCount++;
      }
    }
    
    // Show toast notification
    const isUpload = pendingTransfers[0]?.direction === 'upload';
    if (successCount > 0 && failCount === 0) {
      const msg = skippedCount > 0 
        ? t('sftp.toast.transferred_skipped', { count: successCount, skipped: skippedCount })
        : t('sftp.toast.transferred_count', { count: successCount });
      toastSuccess(isUpload ? t('sftp.toast.upload_complete') : t('sftp.toast.download_complete'), msg);
    } else if (failCount > 0 && successCount === 0) {
      toastError(isUpload ? t('sftp.toast.upload_failed') : t('sftp.toast.download_failed'), t('sftp.toast.failed_count', { count: failCount }));
    } else if (successCount > 0 || failCount > 0) {
      toastError(isUpload ? t('sftp.toast.upload_partial') : t('sftp.toast.download_partial'), t('sftp.toast.partial_detail', { success: successCount, failed: failCount, skipped: skippedCount }));
    }
    
    // Refresh file lists
    if (pendingTransfers[0]?.direction === 'upload') {
      nodeSftpListDir(nodeId, remotePath).then(setRemoteFiles);
    } else {
      refreshLocalFiles();
    }
  };

  // Transfer handler (upload/download) - supports both files and directories
  const handleTransfer = async (files: string[], direction: 'upload' | 'download', basePath: string) => {
    const sourceFiles = direction === 'upload' ? localFiles : remoteFiles;
    const targetFiles = direction === 'upload' ? remoteFiles : localFiles;
    const conflictAction = sftpSettings?.conflictAction || 'ask';
    
    // Build pending transfers list
    const pendingTransfers = files.map(file => ({
      file,
      direction,
      basePath,
      fileInfo: sourceFiles.find(f => f.name === file),
    }));
    
    // Check for conflicts (only for files, not directories)
    const conflicts: ConflictInfo[] = [];
    for (const transfer of pendingTransfers) {
      if (transfer.fileInfo?.file_type === 'Directory') continue;
      
      const targetFile = targetFiles.find(f => f.name === transfer.file);
      if (targetFile && targetFile.file_type !== 'Directory') {
        conflicts.push({
          fileName: transfer.file,
          sourceFile: {
            size: transfer.fileInfo?.size || 0,
            modified: transfer.fileInfo?.modified || null,
          },
          targetFile: {
            size: targetFile.size,
            modified: targetFile.modified,
          },
          direction,
        });
      }
    }
    
    // Handle conflicts based on settings
    if (conflicts.length > 0 && conflictAction === 'ask') {
      // Show conflict dialog
      setConflictDialog({
        conflicts,
        currentIndex: 0,
        pendingTransfers,
        resolvedActions: new Map(),
      });
      return;
    }
    
    // Apply default action to all conflicts
    const resolvedActions = new Map<string, ConflictResolution>();
    for (const conflict of conflicts) {
      resolvedActions.set(conflict.fileName, conflictAction as ConflictResolution);
    }
    
    // Execute transfers
    await executeResolvedTransfers(pendingTransfers, resolvedActions);
  };

  // Delete handler - uses recursive delete for directories
  const handleDelete = async () => {
    if (!deleteConfirm) return;
    const { files, isRemote } = deleteConfirm;
    try {
      if (isRemote) {
        let totalDeleted = 0;
        for (const file of files) {
          const filePath = `${remotePath}/${file}`;
          // Check if it's a directory
          const fileInfo = remoteFiles.find(f => f.name === file);
          if (fileInfo?.file_type === 'Directory') {
            const count = await nodeSftpDeleteRecursive(nodeId, filePath);
            totalDeleted += count;
          } else {
            await nodeSftpDelete(nodeId, filePath);
            totalDeleted += 1;
          }
        }
        nodeSftpListDir(nodeId, remotePath).then(setRemoteFiles);
        setRemoteSelected(new Set());
        toastSuccess(t('sftp.toast.deleted'), t('sftp.toast.deleted_count', { count: totalDeleted }));
      } else {
        // Local delete
        for (const file of files) {
          const filePath = localPath.endsWith('/') ? `${localPath}${file}` : `${localPath}/${file}`;
          await remove(filePath, { recursive: true });
        }
        refreshLocalFiles();
        setLocalSelected(new Set());
        toastSuccess(t('sftp.toast.deleted'), t('sftp.toast.deleted_count', { count: files.length }));
      }
    } catch (err) {
      console.error("Delete failed:", err);
      toastError(t('sftp.toast.delete_failed'), String(err));
    }
    setDeleteConfirm(null);
  };

  // Rename handler
  const handleRename = async () => {
    if (!renameDialog || !inputValue.trim()) return;
    const { oldName, isRemote } = renameDialog;
    try {
      if (isRemote) {
        await nodeSftpRename(nodeId, `${remotePath}/${oldName}`, `${remotePath}/${inputValue}`);
        nodeSftpListDir(nodeId, remotePath).then(setRemoteFiles);
        setRemoteSelected(new Set());
        toastSuccess(t('sftp.toast.renamed'), t('sftp.toast.renamed_detail', { old: oldName, new: inputValue }));
      } else {
        // Local rename
        const oldPath = localPath.endsWith('/') ? `${localPath}${oldName}` : `${localPath}/${oldName}`;
        const newPath = localPath.endsWith('/') ? `${localPath}${inputValue}` : `${localPath}/${inputValue}`;
        await rename(oldPath, newPath);
        refreshLocalFiles();
        setLocalSelected(new Set());
        toastSuccess(t('sftp.toast.renamed'), t('sftp.toast.renamed_detail', { old: oldName, new: inputValue }));
      }
    } catch (err) {
      console.error("Rename failed:", err);
      toastError(t('sftp.toast.rename_failed'), String(err));
    }
    setRenameDialog(null);
    setInputValue('');
  };

  // New folder handler
  const handleNewFolder = async () => {
    if (!newFolderDialog || !inputValue.trim()) return;
    const { isRemote } = newFolderDialog;
    try {
      if (isRemote) {
        await nodeSftpMkdir(nodeId, `${remotePath}/${inputValue}`);
        nodeSftpListDir(nodeId, remotePath).then(setRemoteFiles);
        toastSuccess(t('sftp.toast.folder_created'), inputValue);
      } else {
        // Local mkdir
        const newPath = localPath.endsWith('/') ? `${localPath}${inputValue}` : `${localPath}/${inputValue}`;
        await mkdir(newPath, { recursive: true });
        refreshLocalFiles();
        toastSuccess(t('sftp.toast.folder_created'), inputValue);
      }
    } catch (err) {
      console.error("New folder failed:", err);
      toastError(t('sftp.toast.create_folder_failed'), String(err));
    }
    setNewFolderDialog(null);
    setInputValue('');
  };

  const handleDrop = async (e: React.DragEvent, target: 'local' | 'remote') => {
      e.preventDefault();
      try {
          const data = JSON.parse(e.dataTransfer.getData('application/json'));
          const { files, source, basePath } = data;
          
          if (source === target) return; // Ignore self-drop

          if (source === 'local' && target === 'remote') {
              await handleTransfer(files, 'upload', basePath);
          } else if (source === 'remote' && target === 'local') {
              await handleTransfer(files, 'download', basePath);
          }
      } catch (err) {
          console.error("Drop failed:", err);
      }
  };

  const handlePreview = async (file: FileInfo) => {
      setPreviewLoading(true);
      try {
          const fullPath = `${remotePath}/${file.name}`;
          const content = await nodeSftpPreview(nodeId, fullPath);
          
          // Handle all response types from backend
          if ('TooLarge' in content) {
              setPreviewFile({
                  name: file.name,
                  path: fullPath,
                  type: 'too-large',
                  data: '',
                  fileSize: content.TooLarge.size,
                  maxSize: content.TooLarge.max_size,
                  recommendDownload: content.TooLarge.recommend_download,
              });
              return;
          }
          
          if ('Unsupported' in content) {
              setPreviewFile({
                  name: file.name,
                  path: fullPath,
                  type: 'unsupported',
                  data: '',
                  mimeType: content.Unsupported.mime_type,
                  reason: content.Unsupported.reason,
              });
              return;
          }
          
          if ('Text' in content) {
              setPreviewFile({
                  name: file.name,
                  path: fullPath,
                  type: 'text',
                  data: content.Text.data,
                  mimeType: content.Text.mime_type || undefined,
                  language: content.Text.language,
                  encoding: content.Text.encoding,
              });
              return;
          }
          
          if ('Image' in content) {
              setPreviewFile({
                  name: file.name,
                  path: fullPath,
                  type: 'image',
                  data: content.Image.data,
                  mimeType: content.Image.mime_type,
              });
              return;
          }
          
          // AssetFile: backend streamed file to temp and allowed it on asset scope.
          // Build an asset:// URL and map the kind to the right preview type.
          if ('AssetFile' in content) {
              const { path: assetPath, mime_type, kind } = (content as { AssetFile: { path: string; mime_type: string; kind: string } }).AssetFile;
              const assetUrl = convertFileSrc(assetPath) + `?t=${Date.now()}`;
              const typeMap: Record<string, 'image' | 'video' | 'audio' | 'pdf' | 'office'> = {
                  image: 'image', video: 'video', audio: 'audio', pdf: 'pdf', office: 'office',
              };
              setPreviewFile({
                  name: file.name,
                  path: fullPath,
                  type: typeMap[kind] || 'unsupported',
                  data: '', // no base64 data — asset:// streams directly
                  mimeType: mime_type,
                  assetSrc: assetUrl,
                  tempPath: assetPath,
              });
              return;
          }

          if ('Hex' in content) {
              setPreviewFile({
                  name: file.name,
                  path: fullPath,
                  type: 'hex',
                  data: content.Hex.data,
                  hexOffset: content.Hex.offset,
                  hexTotalSize: content.Hex.total_size,
                  hexHasMore: content.Hex.has_more,
              });
              return;
          }
      } catch (e) {
          console.error("Preview failed:", e);
          toastError(t('sftp.toast.preview_failed'), String(e));
      } finally {
          setPreviewLoading(false);
      }
  };

  const handleLoadMoreHex = async () => {
      if (!previewFile || previewFile.type !== 'hex' || !previewFile.hexHasMore) return;
      
      setHexLoadingMore(true);
      try {
          const newOffset = (previewFile.hexOffset || 0) + 16 * 1024; // 16KB chunks
          const content = await nodeSftpPreviewHex(nodeId, previewFile.path, newOffset);
          
          if ('Hex' in content) {
              setPreviewFile({
                  ...previewFile,
                  data: previewFile.data + content.Hex.data,
                  hexOffset: newOffset,
                  hexHasMore: content.Hex.has_more,
              });
          }
      } catch (e) {
          console.error("Load more hex failed:", e);
          toastError(t('sftp.toast.load_more_failed'), String(e));
      } finally {
          setHexLoadingMore(false);
      }
  };

  // Handle file comparison (diff view)
  const handleCompare = async () => {
    if (!previewFile || previewFile.type !== 'text') return;
    
    // Check if local file exists with same name
    const localFilePath = `${localPath}/${previewFile.name}`;
    const localFileInfo = localFiles.find(f => f.name === previewFile.name);
    
    if (!localFileInfo || localFileInfo.file_type !== 'File') {
      toastError(t('sftp.toast.compare_failed'), t('sftp.toast.compare_no_local'));
      return;
    }
    
    try {
      // Read local file content
      const { readTextFile } = await import('@tauri-apps/plugin-fs');
      const localContent = await readTextFile(localFilePath);
      
      setDiffDialog({
        localFile: { path: localFilePath, content: localContent },
        remoteFile: { path: previewFile.path, content: previewFile.data },
      });
    } catch (e) {
      console.error("Compare failed:", e);
      toastError(t('sftp.toast.compare_failed'), String(e));
    }
  };

  return (
    <div className={`flex flex-col h-full w-full p-2 gap-2 ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
      {initError && (
        <div className="flex items-center justify-between rounded border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-xs text-theme-text">
          <span>SFTP waiting for connection sync: {initError}</span>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2"
            onClick={() => {
              // Reset retry tick to 0 to allow the full auto-retry chain again
              setInitRetryTick(0);
              // Use a microtask to trigger a fresh init cycle
              queueMicrotask(() => setInitRetryTick(1));
            }}
          >
            Retry
          </Button>
        </div>
      )}
      <div className="flex-1 flex gap-2 min-h-0">
        {/* Local Pane */}
        <div className="flex-1 min-w-0" style={{ contain: 'layout style' }}>
           <FileList 
             title={t('sftp.file_list.local')} 
             path={localPath} 
             files={displayLocalFiles}
             onNavigate={handleLocalNavigate}
             onRefresh={refreshLocalFiles}
             active={activePane === 'local'}
             onActivate={() => setActivePane('local')}
             onTransfer={(files, dir) => handleTransfer(files, dir, localPath)}
             onDelete={(files) => setDeleteConfirm({ files, isRemote: false })}
             onRename={(name) => { setRenameDialog({ oldName: name, isRemote: false }); setInputValue(name); }}
             onNewFolder={() => setNewFolderDialog({ isRemote: false })}
             selected={localSelected}
             setSelected={setLocalSelected}
             lastSelected={localLastSelected}
             setLastSelected={setLocalLastSelected}
             isDragOver={localDragOver}
             onDragOver={(e) => { e.preventDefault(); setLocalDragOver(true); }}
             onDragLeave={() => setLocalDragOver(false)}
             onDrop={(e) => { setLocalDragOver(false); handleDrop(e, 'local'); }}
             filter={localFilter}
             onFilterChange={setLocalFilter}
             sortField={localSortField}
             sortDirection={localSortDirection}
             onSortChange={(field) => handleSortChange(true, field)}
             onBrowse={handleBrowseFolder}
             onShowDrives={handleShowDrives}
             isPathEditable={isLocalPathEditing}
             pathInputValue={localPathInput}
             onPathInputChange={(v) => { setLocalPathInput(v); setIsLocalPathEditing(true); }}
             onPathInputSubmit={handleLocalPathSubmit}
             onPathEditStart={() => { setLocalPathInput(localPath); setIsLocalPathEditing(true); }}
             onPathEditCancel={() => { setLocalPathInput(localPath); setIsLocalPathEditing(false); }}
             t={t}
           />
        </div>

        {/* Remote Pane */}
        <div className="flex-1 min-w-0" style={{ contain: 'layout style' }}>
           <FileList 
             title={t('sftp.file_list.remote', { host: session?.host })}
             path={remotePath}
             files={displayRemoteFiles}
             onNavigate={handleRemoteNavigate}
             onRefresh={() => nodeSftpListDir(nodeId, remotePath).then(setRemoteFiles)}
             active={activePane === 'remote'}
             onActivate={() => setActivePane('remote')}
             onPreview={handlePreview}
             onTransfer={(files, dir) => handleTransfer(files, dir, remotePath)}
             onDelete={(files) => setDeleteConfirm({ files, isRemote: true })}
             onRename={(name) => { setRenameDialog({ oldName: name, isRemote: true }); setInputValue(name); }}
             onNewFolder={() => setNewFolderDialog({ isRemote: true })}
             selected={remoteSelected}
             setSelected={setRemoteSelected}
             lastSelected={remoteLastSelected}
             setLastSelected={setRemoteLastSelected}
             isDragOver={remoteDragOver}
             onDragOver={(e) => { e.preventDefault(); setRemoteDragOver(true); }}
             onDragLeave={() => setRemoteDragOver(false)}
             onDrop={(e) => { setRemoteDragOver(false); handleDrop(e, 'remote'); }}
             filter={remoteFilter}
             onFilterChange={setRemoteFilter}
             sortField={remoteSortField}
             sortDirection={remoteSortDirection}
             onSortChange={(field) => handleSortChange(false, field)}
             loading={remoteLoading}
             isPathEditable={isRemotePathEditing}
             pathInputValue={remotePathInput}
             onPathInputChange={(v) => { setRemotePathInput(v); setIsRemotePathEditing(true); }}
             onPathInputSubmit={handleRemotePathSubmit}
             onPathEditStart={() => { setRemotePathInput(remotePath); setIsRemotePathEditing(true); }}
             onPathEditCancel={() => { setRemotePathInput(remotePath); setIsRemotePathEditing(false); }}
             isRemote={true}
             t={t}
           />
        </div>
      </div>
      
      {/* Drives Selection Dialog */}
      <Dialog open={showDrivesDialog} onOpenChange={setShowDrivesDialog}>
        <DialogContent className="max-w-xs">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <HardDrive className="h-4 w-4" />
              {t('sftp.dialogs.select_drive')}
            </DialogTitle>
            <DialogDescription>
              {t('sftp.dialogs.select_drive_desc')}
            </DialogDescription>
          </DialogHeader>
          <div className="px-4 py-2">
            <div className="rounded-md border border-theme-border overflow-hidden">
              {availableDrives.map((drive, index) => {
                const DriveIcon = drive.driveType === 'removable' ? Usb
                  : drive.driveType === 'network' ? Globe
                  : HardDrive;
                const usedRatio = drive.totalSpace > 0
                  ? ((drive.totalSpace - drive.availableSpace) / drive.totalSpace) * 100
                  : 0;
                return (
                  <button
                    key={drive.path}
                    className={cn(
                      "w-full flex items-center gap-3 px-3 py-2.5 text-left transition-colors hover:bg-theme-bg-hover active:scale-[0.99]",
                      index < availableDrives.length - 1 && "border-b border-theme-border/50"
                    )}
                    onClick={() => handleSelectDrive(drive.path)}
                  >
                    <DriveIcon className="h-4 w-4 shrink-0 text-theme-text-muted" />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        <span className="text-sm font-medium text-theme-text">{drive.name}</span>
                        {drive.isReadOnly && (
                          <span className="inline-flex items-center rounded px-1 py-0.5 text-[10px] font-medium leading-none bg-amber-500/15 text-amber-600 dark:text-amber-400">
                            {t('sftp.dialogs.readOnly')}
                          </span>
                        )}
                      </div>
                      <div className="text-xs text-theme-text-muted">{drive.path}</div>
                      {drive.totalSpace > 0 && (
                        <div className="text-[10px] text-theme-text-muted mt-0.5">
                          {formatFileSize(drive.availableSpace)} {t('sftp.dialogs.available')} / {formatFileSize(drive.totalSpace)}
                        </div>
                      )}
                    </div>
                  </button>
                );
              })}
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Transfer Queue Panel */}
      <TransferQueue nodeId={nodeId} />

      {/* Preview Dialog */}
      <Dialog open={!!previewFile} onOpenChange={(open) => {
        if (!open) {
          // Clean up temp file if the preview used an asset:// stream
          if (previewFile?.tempPath) {
            cleanupSftpPreviewTemp(previewFile.tempPath).catch(() => {});
          }
          setPreviewFile(null);
          setSftpPdfZoom(1);
        }
      }}>
        <DialogContent className="max-w-4xl h-[85vh] flex flex-col p-0 gap-0" aria-describedby="preview-desc">
            <DialogHeader className="px-4 py-2 border-b border-theme-border bg-theme-bg-panel flex flex-row items-center justify-between">
                <div className="flex flex-col gap-1">
                    <DialogTitle className="text-sm font-mono flex items-center gap-2">
                        {previewFile?.name}
                        {previewFile?.type === 'hex' && previewFile.hexTotalSize && (
                            <span className="text-xs text-theme-text-muted">
                                ({formatFileSize(previewFile.hexTotalSize)})
                            </span>
                        )}
                        {previewFile?.type === 'office' && (
                            <span className="text-xs px-1.5 py-0.5 bg-blue-500/20 text-blue-400 rounded">
                                {previewFile.name.endsWith('.docx') || previewFile.name.endsWith('.doc') ? 'Word' :
                                 previewFile.name.endsWith('.xlsx') || previewFile.name.endsWith('.xls') ? 'Excel' :
                                 previewFile.name.endsWith('.pptx') || previewFile.name.endsWith('.ppt') ? 'PowerPoint' : 'Office'}
                            </span>
                        )}
                        {previewFile?.language && (
                            <span className="text-xs px-1.5 py-0.5 bg-theme-accent/20 text-theme-accent rounded">
                                {previewFile.language}
                            </span>
                        )}
                    </DialogTitle>
                    <DialogDescription id="preview-desc" className="sr-only">{t('sftp.preview.description')}</DialogDescription>
                </div>
            </DialogHeader>
            
            {/* Preview Content Area */}
            <div className={`flex-1 bg-theme-bg-sunken ${previewFile?.type === 'pdf' ? 'flex flex-col min-h-0' : 'overflow-auto'}`}>
                {/* Loading State */}
                {previewLoading && (
                    <div className="flex items-center justify-center h-full">
                        <RefreshCw className="h-6 w-6 animate-spin text-theme-text-muted" />
                    </div>
                )}
                
                {/* Text Preview with syntax highlighting */}
                {!previewLoading && previewFile?.type === 'text' && (
                    <CodeHighlight
                        code={previewFile.data}
                        language={previewFile.language || undefined}
                        filename={previewFile.name}
                        showLineNumbers={true}
                        className="p-4"
                    />
                )}
                
                {/* Image Preview */}
                {!previewLoading && previewFile?.type === 'image' && (
                    <ImageViewer
                        src={previewFile.assetSrc || `data:${previewFile.mimeType};base64,${previewFile.data}`}
                        alt={previewFile.name}
                        className="p-4"
                    />
                )}
                
                {/* Video Preview */}
                {!previewLoading && previewFile?.type === 'video' && previewFile.assetSrc && (
                    <SFTPMediaPreview type="video" src={previewFile.assetSrc} name={previewFile.name} fallbackText={t('sftp.preview.video_unsupported')} />
                )}
                
                {/* Audio Preview */}
                {!previewLoading && previewFile?.type === 'audio' && previewFile.assetSrc && (
                    <SFTPMediaPreview type="audio" src={previewFile.assetSrc} name={previewFile.name} fallbackText={t('sftp.preview.audio_unsupported')} />
                )}
                
                {/* PDF Preview */}
                {!previewLoading && previewFile?.type === 'pdf' && (
                    <PdfViewer
                        url={previewFile.assetSrc}
                        name={previewFile.name}
                        zoom={sftpPdfZoom}
                        onZoomChange={setSftpPdfZoom}
                        className="flex-1 min-h-0"
                    />
                )}

                {/* Office Document Preview */}
                {!previewLoading && previewFile?.type === 'office' && (
                    <OfficePreview
                        url={previewFile.assetSrc}
                        mimeType={previewFile.mimeType || 'application/octet-stream'}
                        filename={previewFile.name}
                        className="h-full"
                    />
                )}

                {/* Hex Preview */}
                {!previewLoading && previewFile?.type === 'hex' && (
                    <div className="p-4">
                        <div className="text-xs text-theme-text-muted mb-2 flex items-center gap-2">
                            <span>{t('sftp.preview.hex_view')}</span>
                            <span>•</span>
                            <span>{t('sftp.preview.showing_first', { size: formatFileSize((previewFile.hexOffset || 0) + 16384) })}</span>
                            {previewFile.hexTotalSize && (
                                <>
                                    <span>•</span>
                                    <span>{t('sftp.preview.total_size', { size: formatFileSize(previewFile.hexTotalSize) })}</span>
                                </>
                            )}
                        </div>
                        <pre className="font-mono text-xs text-theme-text whitespace-pre overflow-x-auto leading-5">
                            {previewFile.data}
                        </pre>
                        {previewFile.hexHasMore && (
                            <div className="mt-4 flex justify-center">
                                <Button 
                                    variant="secondary" 
                                    size="sm" 
                                    onClick={handleLoadMoreHex}
                                    disabled={hexLoadingMore}
                                >
                                    {hexLoadingMore ? (
                                        <>
                                            <RefreshCw className="h-3 w-3 mr-2 animate-spin" />
                                            {t('sftp.preview.loading')}
                                        </>
                                    ) : (
                                        t('sftp.preview.load_more')
                                    )}
                                </Button>
                            </div>
                        )}
                    </div>
                )}
                
                {/* Too Large */}
                {!previewLoading && previewFile?.type === 'too-large' && (
                    <div className="flex flex-col items-center justify-center h-full p-8 gap-4">
                        <div className="text-6xl">📦</div>
                        <div className="text-theme-text text-lg">{t('sftp.preview.too_large')}</div>
                        <div className="text-theme-text-muted text-sm text-center">
                            <p>{t('sftp.preview.file_size', { size: formatFileSize(previewFile.fileSize || 0) })}</p>
                            <p>{t('sftp.preview.preview_limit', { size: formatFileSize(previewFile.maxSize || 0) })}</p>
                        </div>
                        {previewFile.recommendDownload && (
                            <p className="text-theme-text-muted text-sm">{t('sftp.preview.recommend_download')}</p>
                        )}
                    </div>
                )}
                
                {/* Unsupported */}
                {!previewLoading && previewFile?.type === 'unsupported' && (
                    <div className="flex flex-col items-center justify-center h-full p-8 gap-4">
                        <div className="text-6xl">❓</div>
                        <div className="text-theme-text text-lg">{t('sftp.preview.unsupported')}</div>
                        <div className="text-theme-text-muted text-sm text-center">
                            <p>{t('sftp.preview.type', { type: previewFile.mimeType })}</p>
                            {previewFile.reason && <p className="mt-2">{previewFile.reason}</p>}
                        </div>
                    </div>
                )}
            </div>
            
            <DialogFooter className="p-2 border-t border-theme-border bg-theme-bg-panel justify-between sm:justify-between">
                <div className="text-xs text-theme-text-muted self-center px-2 truncate max-w-md">
                    {previewFile?.path}
                </div>
                <div className="flex gap-2">
                    {/* Edit button - only for text files */}
                    {previewFile?.type === 'text' && (
                      <Button 
                        variant="default" 
                        size="sm" 
                        onClick={() => {
                          if (previewFile) {
                            setEditorFile({
                              path: previewFile.path,
                              content: previewFile.data,
                              language: previewFile.language || null,
                              encoding: previewFile.encoding || 'UTF-8',
                            });
                            setPreviewFile(null);
                          }
                        }}
                        title={t('editor.edit_mode')}
                      >
                        <Edit3 className="h-3 w-3 mr-2" /> {t('editor.edit_mode')}
                      </Button>
                    )}
                    {/* Compare button - only for text files */}
                    {previewFile?.type === 'text' && localFiles.some(f => f.name === previewFile.name && f.file_type === 'File') && (
                      <Button 
                        variant="outline" 
                        size="sm" 
                        onClick={handleCompare}
                        title={t('sftp.preview.compare_tooltip')}
                      >
                        <GitCompare className="h-3 w-3 mr-2" /> {t('sftp.preview.compare')}
                      </Button>
                    )}
                    <Button variant="secondary" size="sm" onClick={async () => {
                        if (!previewFile) return;
                        try {
                            const localDest = `${localPath}/${previewFile.name}`;
                            await nodeSftpDownload(nodeId, previewFile.path, localDest);
                            refreshLocalFiles();
                            setPreviewFile(null);
                        } catch (e) {
                            console.error("Download failed:", e);
                        }
                    }}>
                        <Download className="h-3 w-3 mr-2" /> {t('sftp.preview.download')}
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => setPreviewFile(null)}>{t('sftp.preview.close')}</Button>
                </div>
            </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Rename Dialog */}
      <Dialog open={!!renameDialog} onOpenChange={(open) => !open && setRenameDialog(null)}>
        <DialogContent className="max-w-sm" aria-describedby="rename-desc">
          <DialogHeader>
            <DialogTitle>{t('sftp.dialogs.rename')}</DialogTitle>
            <DialogDescription id="rename-desc">{t('sftp.dialogs.rename_desc')}</DialogDescription>
          </DialogHeader>
          <Input 
            value={inputValue} 
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleRename()}
            autoFocus
          />
          <DialogFooter>
            <Button variant="ghost" onClick={() => setRenameDialog(null)}>{t('sftp.dialogs.cancel')}</Button>
            <Button onClick={handleRename}>{t('sftp.dialogs.rename')}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* New Folder Dialog */}
      <Dialog open={!!newFolderDialog} onOpenChange={(open) => !open && setNewFolderDialog(null)}>
        <DialogContent className="max-w-sm" aria-describedby="newfolder-desc">
          <DialogHeader>
            <DialogTitle>{t('sftp.dialogs.new_folder')}</DialogTitle>
            <DialogDescription id="newfolder-desc">{t('sftp.dialogs.new_folder_desc')}</DialogDescription>
          </DialogHeader>
          <Input 
            value={inputValue} 
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleNewFolder()}
            placeholder={t('sftp.dialogs.new_folder_placeholder')}
            autoFocus
          />
          <DialogFooter>
            <Button variant="ghost" onClick={() => setNewFolderDialog(null)}>{t('sftp.dialogs.cancel')}</Button>
            <Button onClick={handleNewFolder}>{t('sftp.dialogs.create')}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <Dialog open={!!deleteConfirm} onOpenChange={(open) => !open && setDeleteConfirm(null)}>
        <DialogContent className="max-w-sm" aria-describedby="delete-desc">
          <DialogHeader>
            <DialogTitle>{t('sftp.dialogs.delete')}</DialogTitle>
            <DialogDescription id="delete-desc">
              {t('sftp.dialogs.delete_confirm', { count: deleteConfirm?.files.length || 0 })}
            </DialogDescription>
          </DialogHeader>
          <div className="max-h-32 overflow-auto text-xs text-theme-text-muted bg-theme-bg-sunken p-2 rounded">
            {deleteConfirm?.files.map(f => <div key={f}>{f}</div>)}
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setDeleteConfirm(null)}>{t('sftp.dialogs.cancel')}</Button>
            <Button variant="destructive" onClick={handleDelete}>{t('sftp.dialogs.delete')}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Transfer Conflict Dialog */}
      <TransferConflictDialog
        isOpen={!!conflictDialog}
        conflicts={conflictDialog?.conflicts || []}
        currentIndex={conflictDialog?.currentIndex || 0}
        onResolve={processConflictResolution}
        onCancel={() => setConflictDialog(null)}
      />

      {/* File Diff Dialog */}
      <FileDiffDialog
        isOpen={!!diffDialog}
        onClose={() => setDiffDialog(null)}
        localFile={diffDialog?.localFile || null}
        remoteFile={diffDialog?.remoteFile || null}
      />

      {/* IDE Mode: Remote File Editor */}
      {editorFile && (
        <RemoteFileEditor
          open={!!editorFile}
          onClose={() => setEditorFile(null)}
          nodeId={nodeId}
          filePath={editorFile.path}
          initialContent={editorFile.content}
          language={editorFile.language}
          encoding={editorFile.encoding}
          onSaved={() => {
            // Refresh remote file list to update mtime
            nodeSftpListDir(nodeId, remotePath).then(setRemoteFiles);
          }}
        />
      )}
    </div>
  );
};
