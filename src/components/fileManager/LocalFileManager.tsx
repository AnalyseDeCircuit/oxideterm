/**
 * LocalFileManager Component
 * Standalone local file browser panel with Quick Look, Bookmarks, and Terminal integration
 */

import React, { useState, useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { FolderPlus, Trash2, Terminal, Star, PanelLeftClose, PanelLeft, Copy, Scissors, ClipboardPaste, Archive, FolderArchive } from 'lucide-react';
import { openPath } from '@tauri-apps/plugin-opener';
import { FileList } from './FileList';
import { QuickLook } from './QuickLook';
import { BookmarksPanel } from './BookmarksPanel';
import { useLocalFiles, useFileSelection, useBookmarks, useFileClipboard, useFileArchive } from './hooks';
import { useToast } from '../../hooks/useToast';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { useAppStore } from '../../store/appStore';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { 
  Dialog, 
  DialogContent, 
  DialogHeader, 
  DialogTitle, 
  DialogDescription,
  DialogFooter
} from '../ui/dialog';
import { cn } from '../../lib/utils';
import type { FileInfo, FilePreview, PreviewType, FileMetadata, ArchiveInfo } from './types';

// Preview imports
import { readFile, stat } from '@tauri-apps/plugin-fs';
import { invoke } from '@tauri-apps/api/core';

// File extension categorization
const TEXT_EXTENSIONS = new Set(['txt', 'log', 'ini', 'conf', 'cfg', 'env']);
const CODE_EXTENSIONS = new Set([
  'js', 'jsx', 'ts', 'tsx', 'py', 'rs', 'go', 'java', 'c', 'cpp', 'h', 'hpp',
  'cs', 'rb', 'php', 'swift', 'kt', 'scala', 'sh', 'bash', 'zsh', 'fish', 'ps1',
  'sql', 'html', 'htm', 'css', 'scss', 'sass', 'less', 'json', 'yaml', 'yml',
  'toml', 'xml', 'vue', 'svelte'
]);
const MARKDOWN_EXTENSIONS = new Set(['md', 'markdown', 'mdx']);
const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico', 'bmp']);
const FONT_EXTENSIONS = new Set(['ttf', 'otf', 'woff', 'woff2', 'eot']);
const PDF_EXTENSIONS = new Set(['pdf']);
const OFFICE_EXTENSIONS = new Set(['docx', 'xlsx', 'pptx', 'doc', 'xls', 'ppt', 'odt', 'ods', 'odp']);
const ARCHIVE_EXTENSIONS = new Set(['zip', 'jar', 'war', 'ear', 'apk', 'xpi', 'crx', 'epub']);

// Office MIME type mapping
const OFFICE_MIME_TYPES: Record<string, string> = {
  'docx': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  'xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  'pptx': 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  'doc': 'application/msword',
  'xls': 'application/vnd.ms-excel',
  'ppt': 'application/vnd.ms-powerpoint',
  'odt': 'application/vnd.oasis.opendocument.text',
  'ods': 'application/vnd.oasis.opendocument.spreadsheet',
  'odp': 'application/vnd.oasis.opendocument.presentation',
};

// Shell config files (dotfiles without extension) that should be treated as text/code
const SHELL_CONFIG_FILES = new Set([
  '.bashrc', '.bash_profile', '.bash_login', '.bash_logout', '.bash_aliases',
  '.zshrc', '.zshenv', '.zprofile', '.zlogin', '.zlogout',
  '.profile', '.tcshrc', '.cshrc', '.kshrc', '.fishrc',
  '.vimrc', '.gvimrc', '.exrc', '.nanorc',
  '.gitconfig', '.gitignore', '.gitattributes',
  '.editorconfig', '.prettierrc', '.eslintrc', '.stylelintrc',
  '.npmrc', '.yarnrc', '.nvmrc', '.node-version', '.python-version',
  '.env', '.env.local', '.env.development', '.env.production',
  '.htaccess', '.dockerignore', '.hgignore',
  'Makefile', 'Dockerfile', 'Vagrantfile', 'Procfile', 'Gemfile', 'Rakefile',
  'CMakeLists.txt', 'Cargo.toml', 'package.json', 'tsconfig.json'
]);

// Max file size for text preview (10MB)
const MAX_PREVIEW_SIZE = 10 * 1024 * 1024;
// Stream preview threshold for large text/code files (256KB)
const STREAM_PREVIEW_THRESHOLD = 256 * 1024;

// Helper: Convert Uint8Array to base64 safely (handles large files)
function uint8ArrayToBase64(bytes: Uint8Array): string {
  const CHUNK_SIZE = 0x8000; // 32KB chunks to avoid call stack overflow
  let binary = '';
  for (let i = 0; i < bytes.length; i += CHUNK_SIZE) {
    const chunk = bytes.subarray(i, Math.min(i + CHUNK_SIZE, bytes.length));
    binary += String.fromCharCode.apply(null, Array.from(chunk));
  }
  return btoa(binary);
}

// Helper: Get file extension properly (handles dotfiles)
function getFileExtension(filename: string): string {
  // For dotfiles like .bashrc, .tcshrc - no extension
  if (filename.startsWith('.') && !filename.includes('.', 1)) {
    return '';
  }
  const lastDot = filename.lastIndexOf('.');
  if (lastDot === -1 || lastDot === 0) {
    return '';
  }
  return filename.substring(lastDot + 1).toLowerCase();
}

// Helper: Normalize file path (remove double slashes, handle trailing slashes)
function normalizePath(filePath: string): string {
  // Replace multiple consecutive slashes with single slash (except for protocol like file://)
  let normalized = filePath.replace(/([^:])\/+/g, '$1/');
  // Remove trailing slash unless it's the root
  if (normalized.length > 1 && normalized.endsWith('/')) {
    normalized = normalized.slice(0, -1);
  }
  return normalized;
}

// Language detection by extension
const LANGUAGE_MAP: Record<string, string> = {
  'js': 'javascript', 'jsx': 'jsx', 'ts': 'typescript', 'tsx': 'tsx',
  'py': 'python', 'rs': 'rust', 'go': 'go', 'java': 'java',
  'c': 'c', 'cpp': 'cpp', 'h': 'c', 'hpp': 'cpp',
  'cs': 'csharp', 'rb': 'ruby', 'php': 'php', 'swift': 'swift',
  'kt': 'kotlin', 'scala': 'scala', 'sh': 'bash', 'bash': 'bash',
  'zsh': 'bash', 'fish': 'fish', 'ps1': 'powershell', 'sql': 'sql',
  'html': 'html', 'htm': 'html', 'css': 'css', 'scss': 'scss',
  'sass': 'sass', 'less': 'less', 'json': 'json', 'yaml': 'yaml',
  'yml': 'yaml', 'toml': 'toml', 'xml': 'xml', 'vue': 'vue',
  'svelte': 'svelte'
};

export interface LocalFileManagerProps {
  className?: string;
}

export const LocalFileManager: React.FC<LocalFileManagerProps> = ({ className }) => {
  const { t } = useTranslation();
  const { success: toastSuccess, error: toastError } = useToast();
  
  // Stores
  const createTerminal = useLocalTerminalStore(s => s.createTerminal);
  const createTab = useAppStore(s => s.createTab);
  
  // Hooks
  const localFiles = useLocalFiles();
  const selection = useFileSelection({ files: localFiles.displayFiles });
  const bookmarksHook = useBookmarks();
  const fileClipboard = useFileClipboard({
    onSuccess: (msg) => toastSuccess(t('fileManager.operationSuccess'), msg),
    onError: (title, msg) => toastError(title, msg),
  });
  const fileArchive = useFileArchive({
    onSuccess: (msg) => toastSuccess(t('fileManager.operationSuccess'), msg),
    onError: (title, msg) => toastError(title, msg),
  });
  
  // UI state
  const [sidebarOpen, setSidebarOpen] = useState(true);
  
  // Dialog states
  const [newFolderDialog, setNewFolderDialog] = useState(false);
  const [renameDialog, setRenameDialog] = useState<string | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<string[] | null>(null);
  const [drivesDialog, setDrivesDialog] = useState(false);
  const [availableDrives, setAvailableDrives] = useState<string[]>([]);
  const [inputValue, setInputValue] = useState('');
  
  // Preview state (for Quick Look)
  const [previewFile, setPreviewFile] = useState<FilePreview | null>(null);
  const [previewIndex, setPreviewIndex] = useState<number>(-1);
  
  // Compute previewable files (non-directories) from displayFiles
  const previewableFiles = React.useMemo(() => 
    localFiles.displayFiles.filter(f => f.file_type !== 'Directory'),
    [localFiles.displayFiles]
  );
  
  // Handle preview (Quick Look) - Enhanced version
  const handlePreview = useCallback(async (file: FileInfo) => {
    try {
      // Normalize path to avoid double slashes and other issues
      const filePath = normalizePath(file.path);
      
      // Find the index in previewable files
      const idx = previewableFiles.findIndex(f => f.path === file.path);
      setPreviewIndex(idx);
      
      // Get extension properly (handles dotfiles)
      const ext = getFileExtension(file.name);
      
      // Check file size first
      const fileStats = await stat(filePath);
      const fileSize = fileStats.size;
      
      // Determine preview type
      let previewType: PreviewType;
      let data = '';
      let mimeType: string | undefined;
      let language: string | undefined;
      let archiveInfo: ArchiveInfo | undefined;
      // TODO: stream 变量用于流式预览大文件，目前未完全实现
      // let stream: FilePreview['stream'];

      const isShellConfig = SHELL_CONFIG_FILES.has(file.name) || (file.name.startsWith('.') && ext === '');
      const isMarkdown = MARKDOWN_EXTENSIONS.has(ext);
      const isCode = isShellConfig || CODE_EXTENSIONS.has(ext);
      const isText = TEXT_EXTENSIONS.has(ext);
      const shouldStream = (isCode || isText) && fileSize >= STREAM_PREVIEW_THRESHOLD;
      
      if (IMAGE_EXTENSIONS.has(ext)) {
        previewType = 'image';
        // Read image and convert to data URL safely
        const content = await readFile(filePath);
        mimeType = ext === 'svg' ? 'image/svg+xml' :
                        ext === 'png' ? 'image/png' :
                        ext === 'gif' ? 'image/gif' :
                        ext === 'webp' ? 'image/webp' :
                        ext === 'ico' ? 'image/x-icon' :
                        ext === 'bmp' ? 'image/bmp' : 'image/jpeg';
        const base64 = uint8ArrayToBase64(content);
        data = `data:${mimeType};base64,${base64}`;
      } else if (FONT_EXTENSIONS.has(ext)) {
        previewType = 'font';
        // Read font and convert to base64 data URL
        const content = await readFile(filePath);
        mimeType = ext === 'ttf' ? 'font/ttf' :
                   ext === 'otf' ? 'font/otf' :
                   ext === 'woff' ? 'font/woff' :
                   ext === 'woff2' ? 'font/woff2' :
                   ext === 'eot' ? 'application/vnd.ms-fontobject' : 'font/ttf';
        const base64 = uint8ArrayToBase64(content);
        data = `data:${mimeType};base64,${base64}`;
      } else if (PDF_EXTENSIONS.has(ext)) {
        previewType = 'pdf';
        // Read PDF and convert to base64 data URL
        const content = await readFile(filePath);
        mimeType = 'application/pdf';
        const base64 = uint8ArrayToBase64(content);
        data = `data:${mimeType};base64,${base64}`;
      } else if (OFFICE_EXTENSIONS.has(ext)) {
        previewType = 'office';
        // Read Office file and convert to base64
        const content = await readFile(filePath);
        mimeType = OFFICE_MIME_TYPES[ext] || 'application/octet-stream';
        data = uint8ArrayToBase64(content);
      } else if (ARCHIVE_EXTENSIONS.has(ext)) {
        // Archive preview - list contents
        previewType = 'archive';
        try {
          archiveInfo = await invoke<ArchiveInfo>('list_archive_contents', { archivePath: filePath });
        } catch (err) {
          console.error('Failed to read archive:', err);
          previewType = 'unsupported';
          data = '';
        }
      } else if (shouldStream) {
        // TODO: 流式预览大文件功能未完全实现，暂时跳过
        if (isCode) {
          previewType = 'code';
          language = isShellConfig ? 'bash' : LANGUAGE_MAP[ext];
          // stream = { path: filePath, size: fileSize, type: 'code', language, mimeType };
        } else {
          previewType = 'text';
          // stream = { path: filePath, size: fileSize, type: 'text', mimeType };
        }
      } else if (fileSize > MAX_PREVIEW_SIZE) {
        previewType = 'too-large';
        data = '';
      } else if (isShellConfig) {
        // Handle shell config files (dotfiles) as code
        
        // Also handle any dotfile without extension (e.g., .tcshrc, .hidden, etc.)
        previewType = 'code';
        language = 'bash'; // Default to bash for shell configs
        const content = await readFile(filePath);
        data = new TextDecoder().decode(content);
      } else if (isMarkdown) {
        previewType = 'markdown';
        const content = await readFile(filePath);
        data = new TextDecoder().decode(content);
      } else if (isCode) {
        previewType = 'code';
        language = LANGUAGE_MAP[ext];
        const content = await readFile(filePath);
        data = new TextDecoder().decode(content);
      } else if (isText) {
        previewType = 'text';
        const content = await readFile(filePath);
        data = new TextDecoder().decode(content);
      } else {
        // Try to read as text, fall back to unsupported
        try {
          const content = await readFile(filePath);
          const text = new TextDecoder().decode(content);
          // Check if it's likely binary (more than 10 null bytes or high ratio of non-printable chars)
          const nullCount = text.split('\0').length - 1;
          const nonPrintableCount = (text.match(/[\x00-\x08\x0E-\x1F]/g) || []).length;
          if (nullCount > 10 || nonPrintableCount > text.length * 0.1) {
            previewType = 'unsupported';
            data = '';
          } else {
            previewType = 'text';
            data = text;
          }
        } catch {
          previewType = 'unsupported';
          data = '';
        }
      }
      
      // Fetch detailed file metadata (stat call only during preview for performance)
      let metadata: FileMetadata | undefined;
      try {
        metadata = await invoke<FileMetadata>('local_get_file_metadata', { path: filePath });
      } catch (metadataErr) {
        console.warn('Failed to fetch file metadata:', metadataErr);
        // Non-fatal, continue without metadata
      }
      
      setPreviewFile({
        name: file.name,
        path: filePath,
        type: previewType,
        data,
        mimeType,
        language,
        size: fileSize,
        fileSize,
        reason: previewType === 'unsupported' ? t('fileManager.binaryFile') : undefined,
        archiveInfo,
        metadata,
      });
    } catch (err) {
      // Provide more detailed error info
      console.error('Preview error:', err, 'for file:', file.path);
      toastError(t('fileManager.previewError'), `${file.name}: ${String(err)}`);
    }
  }, [toastError, t, previewableFiles]);
  
  // Handle navigation in Quick Look (navigate to another file in the list)
  const handlePreviewNavigate = useCallback((file: FileInfo, newIndex: number) => {
    // The file parameter provides the target file directly
    handlePreview(file);
    setPreviewIndex(newIndex);
  }, [handlePreview]);
  
  // Handle keyboard shortcuts (global)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Space key for Quick Look (when file selected and no dialog open)
      if (e.key === ' ' && !previewFile && !newFolderDialog && !renameDialog && !deleteConfirm) {
        const selectedFiles = Array.from(selection.selected);
        if (selectedFiles.length === 1) {
          const file = localFiles.displayFiles.find(f => f.name === selectedFiles[0]);
          if (file && file.file_type !== 'Directory') {
            e.preventDefault();
            handlePreview(file);
          }
        }
      }
    };
    
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [selection.selected, localFiles.displayFiles, previewFile, newFolderDialog, renameDialog, deleteConfirm, handlePreview]);
  
  // Handle show drives
  const handleShowDrives = useCallback(async () => {
    const drives = await localFiles.showDrives();
    setAvailableDrives(drives);
    setDrivesDialog(true);
  }, [localFiles]);
  
  // Handle navigate with drives support
  const handleNavigate = useCallback((target: string) => {
    if (target === '..') {
      const parent = localFiles.path;
      if (/^[A-Za-z]:\\?$/.test(parent) || /^[A-Za-z]:$/.test(parent)) {
        handleShowDrives();
        return;
      }
    }
    localFiles.navigate(target);
    selection.clearSelection();
  }, [localFiles, selection, handleShowDrives]);
  
  // Handle select drive
  const handleSelectDrive = useCallback((drive: string) => {
    localFiles.navigate(drive);
    selection.clearSelection();
    setDrivesDialog(false);
  }, [localFiles, selection]);
  
  // Handle new folder
  const handleNewFolder = useCallback(async () => {
    if (!inputValue.trim()) return;
    try {
      await localFiles.createFolder(inputValue.trim());
      toastSuccess(t('fileManager.folderCreated'), inputValue.trim());
      setNewFolderDialog(false);
      setInputValue('');
    } catch (err) {
      toastError(t('fileManager.error'), String(err));
    }
  }, [localFiles, inputValue, toastSuccess, toastError, t]);
  
  // Handle rename
  const handleRename = useCallback(async () => {
    if (!renameDialog || !inputValue.trim()) return;
    try {
      await localFiles.renameFile(renameDialog, inputValue.trim());
      toastSuccess(t('fileManager.renamed'), `${renameDialog} → ${inputValue.trim()}`);
      setRenameDialog(null);
      setInputValue('');
      selection.clearSelection();
    } catch (err) {
      toastError(t('fileManager.error'), String(err));
    }
  }, [localFiles, renameDialog, inputValue, toastSuccess, toastError, t, selection]);
  
  // Handle delete
  const handleDelete = useCallback(async () => {
    if (!deleteConfirm || deleteConfirm.length === 0) return;
    try {
      await localFiles.deleteFiles(deleteConfirm);
      toastSuccess(
        t('fileManager.deleted'), 
        t('fileManager.deletedCount', { count: deleteConfirm.length })
      );
      setDeleteConfirm(null);
      selection.clearSelection();
    } catch (err) {
      toastError(t('fileManager.error'), String(err));
    }
  }, [localFiles, deleteConfirm, toastSuccess, toastError, t, selection]);
  
  // Handle open in external application
  const handleOpenExternal = useCallback(async (path: string) => {
    try {
      await openPath(path);
    } catch (err) {
      toastError(t('fileManager.error'), String(err));
    }
  }, [toastError, t]);
  
  // Handle open terminal at directory
  const handleOpenTerminal = useCallback(async (dirPath: string) => {
    try {
      const info = await createTerminal({ cwd: dirPath });
      createTab('local_terminal', info.id);
      toastSuccess(t('fileManager.terminalOpened'), dirPath);
    } catch (err) {
      toastError(t('fileManager.error'), String(err));
    }
  }, [createTerminal, createTab, toastSuccess, toastError, t]);
  
  // Handle context menu action: open terminal here
  const handleOpenTerminalHere = useCallback(() => {
    const selectedFiles = Array.from(selection.selected);
    if (selectedFiles.length === 1) {
      const file = localFiles.displayFiles.find(f => f.name === selectedFiles[0]);
      if (file?.file_type === 'Directory') {
        handleOpenTerminal(file.path);
        return;
      }
    }
    // Open terminal at current path
    handleOpenTerminal(localFiles.path);
  }, [selection.selected, localFiles.displayFiles, localFiles.path, handleOpenTerminal]);
  
  // Get selected file objects
  const getSelectedFiles = useCallback((): FileInfo[] => {
    const selectedNames = Array.from(selection.selected);
    return localFiles.displayFiles.filter(f => selectedNames.includes(f.name));
  }, [selection.selected, localFiles.displayFiles]);
  
  // Handle copy
  const handleCopy = useCallback(() => {
    const files = getSelectedFiles();
    if (files.length > 0) {
      fileClipboard.copy(files, localFiles.path);
      toastSuccess(t('fileManager.copied'), t('fileManager.copiedCount', { count: files.length }));
    }
  }, [getSelectedFiles, fileClipboard, localFiles.path, toastSuccess, t]);
  
  // Handle cut
  const handleCut = useCallback(() => {
    const files = getSelectedFiles();
    if (files.length > 0) {
      fileClipboard.cut(files, localFiles.path);
      toastSuccess(t('fileManager.cut'), t('fileManager.cutCount', { count: files.length }));
    }
  }, [getSelectedFiles, fileClipboard, localFiles.path, toastSuccess, t]);
  
  // Handle paste
  const handlePaste = useCallback(async () => {
    if (fileClipboard.hasClipboard) {
      await fileClipboard.paste(localFiles.path);
      localFiles.refresh();
    }
  }, [fileClipboard, localFiles]);
  
  // Handle compress
  const handleCompress = useCallback(async () => {
    const files = getSelectedFiles();
    if (files.length > 0) {
      const archiveName = files.length === 1 
        ? `${files[0].name}.zip`
        : `Archive_${new Date().toISOString().slice(0, 10)}.zip`;
      await fileArchive.compress(files, localFiles.path, archiveName);
      localFiles.refresh();
    }
  }, [getSelectedFiles, fileArchive, localFiles]);
  
  // Handle extract
  const handleExtract = useCallback(async () => {
    const files = getSelectedFiles();
    if (files.length === 1 && fileArchive.canExtract(files[0].name)) {
      // Extract to a folder with same name as archive
      const archiveName = files[0].name;
      const folderName = archiveName.replace(/\.(zip|tar|gz|tgz|tar\.gz|bz2|xz|7z)$/i, '');
      const destPath = `${localFiles.path}/${folderName}`;
      await fileArchive.extract(files[0].path, destPath);
      localFiles.refresh();
    }
  }, [getSelectedFiles, fileArchive, localFiles]);
  
  // Keyboard shortcuts for clipboard operations
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Only handle if not in an input field
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        return;
      }
      
      const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
      const modifier = isMac ? e.metaKey : e.ctrlKey;
      
      if (modifier && e.key === 'c') {
        e.preventDefault();
        handleCopy();
      } else if (modifier && e.key === 'x') {
        e.preventDefault();
        handleCut();
      } else if (modifier && e.key === 'v') {
        e.preventDefault();
        handlePaste();
      }
    };
    
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleCopy, handleCut, handlePaste]);
  
  return (
    <div className={cn("flex h-full", className)}>
      {/* Sidebar - Bookmarks */}
      <div className={cn(
        "border-r border-theme-border bg-zinc-900/30 transition-all duration-200 flex flex-col",
        sidebarOpen ? "w-52" : "w-10"
      )}>
        {/* Sidebar toggle */}
        <div className="flex items-center justify-end p-1 border-b border-theme-border">
          <Button
            size="icon"
            variant="ghost"
            className="h-6 w-6"
            onClick={() => setSidebarOpen(!sidebarOpen)}
            title={sidebarOpen ? t('fileManager.collapseSidebar') : t('fileManager.expandSidebar')}
          >
            {sidebarOpen ? <PanelLeftClose className="h-3.5 w-3.5" /> : <PanelLeft className="h-3.5 w-3.5" />}
          </Button>
        </div>
        
        {/* Bookmarks Panel */}
        <div className="flex-1 min-h-0 overflow-hidden">
          <BookmarksPanel
            bookmarks={bookmarksHook.bookmarks}
            currentPath={localFiles.path}
            isBookmarked={bookmarksHook.isBookmarked(localFiles.path)}
            onNavigate={(path) => {
              localFiles.navigate(path);
              selection.clearSelection();
            }}
            onAddBookmark={bookmarksHook.addBookmark}
            onRemoveBookmark={bookmarksHook.removeBookmark}
            onUpdateBookmark={bookmarksHook.updateBookmark}
            collapsed={!sidebarOpen}
          />
        </div>
        
        {/* Quick actions at bottom */}
        {sidebarOpen && (
          <div className="border-t border-theme-border p-2 space-y-1">
            <Button
              size="sm"
              variant="ghost"
              className="w-full justify-start h-7 text-xs"
              onClick={handleOpenTerminalHere}
            >
              <Terminal className="h-3 w-3 mr-2" />
              {t('fileManager.openTerminalHere')}
            </Button>
          </div>
        )}
      </div>
      
      {/* Main Content */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Toolbar */}
        <div className="flex items-center gap-2 p-2 bg-theme-bg-panel border-b border-theme-border">
          <span className="text-sm font-medium text-zinc-300">{t('fileManager.title')}</span>
          <div className="flex-1" />
          
          {/* Bookmark toggle for current path */}
          <Button
            size="sm"
            variant="ghost"
            className={cn("h-7 px-2", bookmarksHook.isBookmarked(localFiles.path) && "text-yellow-500")}
            onClick={() => {
              if (bookmarksHook.isBookmarked(localFiles.path)) {
                const bookmark = bookmarksHook.getBookmark(localFiles.path);
                if (bookmark) bookmarksHook.removeBookmark(bookmark.id);
              } else {
                bookmarksHook.addBookmark(localFiles.path);
              }
            }}
            title={bookmarksHook.isBookmarked(localFiles.path) ? t('fileManager.removeBookmark') : t('fileManager.addBookmark')}
          >
            <Star className={cn("h-3.5 w-3.5", bookmarksHook.isBookmarked(localFiles.path) && "fill-current")} />
          </Button>
          
          <Button 
            size="sm" 
            variant="ghost" 
            className="h-7"
            onClick={() => {
              setInputValue('');
              setNewFolderDialog(true);
            }}
          >
            <FolderPlus className="h-3.5 w-3.5 mr-1" />
            {t('fileManager.newFolder')}
          </Button>
          
          {/* Clipboard operations */}
          <div className="h-4 w-px bg-theme-border mx-1" />
          
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7"
            onClick={handleCopy}
            disabled={selection.selected.size === 0}
            title={t('fileManager.copy')}
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
          
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7"
            onClick={handleCut}
            disabled={selection.selected.size === 0}
            title={t('fileManager.cut')}
          >
            <Scissors className="h-3.5 w-3.5" />
          </Button>
          
          <Button
            size="icon"
            variant="ghost"
            className={cn("h-7 w-7", fileClipboard.hasClipboard && "text-theme-accent")}
            onClick={handlePaste}
            disabled={!fileClipboard.hasClipboard}
            title={t('fileManager.paste')}
          >
            <ClipboardPaste className="h-3.5 w-3.5" />
          </Button>
          
          {/* Archive operations */}
          <div className="h-4 w-px bg-theme-border mx-1" />
          
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7"
            onClick={handleCompress}
            disabled={selection.selected.size === 0 || fileArchive.compressing}
            title={t('fileManager.compress')}
          >
            <Archive className="h-3.5 w-3.5" />
          </Button>
          
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7"
            onClick={handleExtract}
            disabled={
              selection.selected.size !== 1 || 
              fileArchive.extracting ||
              !getSelectedFiles().some(f => fileArchive.canExtract(f.name))
            }
            title={t('fileManager.extract')}
          >
            <FolderArchive className="h-3.5 w-3.5" />
          </Button>
        </div>
        
        {/* File List */}
        <div className="flex-1 min-h-0">
          <FileList
            title={t('fileManager.local')}
            files={localFiles.displayFiles}
            path={localFiles.path}
            isRemote={false}
            active={true}
            loading={localFiles.loading}
            selected={selection.selected}
            lastSelected={selection.lastSelected}
            onSelect={selection.select}
            onSelectAll={selection.selectAll}
            onClearSelection={selection.clearSelection}
            onNavigate={handleNavigate}
            onRefresh={localFiles.refresh}
            isPathEditable={localFiles.isPathEditing}
            pathInputValue={localFiles.pathInput}
            onPathInputChange={localFiles.setPathInput}
            onPathInputSubmit={localFiles.submitPathInput}
            filter={localFiles.filter}
            onFilterChange={localFiles.setFilter}
            sortField={localFiles.sortField}
            sortDirection={localFiles.sortDirection}
            onSortChange={localFiles.toggleSort}
            onPreview={handlePreview}
            onDelete={(files) => setDeleteConfirm(files)}
            onRename={(name) => {
              setInputValue(name);
              setRenameDialog(name);
            }}
            onNewFolder={() => {
              setInputValue('');
              setNewFolderDialog(true);
            }}
            onBrowse={localFiles.browseFolder}
            onShowDrives={handleShowDrives}
            onOpenTerminal={handleOpenTerminalHere}
            onCopy={handleCopy}
            onCut={handleCut}
            onPaste={handlePaste}
            onCompress={handleCompress}
            onExtract={handleExtract}
            hasClipboard={fileClipboard.hasClipboard}
            canExtract={getSelectedFiles().some(f => fileArchive.canExtract(f.name))}
            t={t}
          />
        </div>
      </div>
      
      {/* Quick Look Preview */}
      <QuickLook
        preview={previewFile}
        onClose={() => {
          setPreviewFile(null);
          setPreviewIndex(-1);
        }}
        onOpenExternal={handleOpenExternal}
        fileList={previewableFiles}
        currentIndex={previewIndex}
        onNavigate={handlePreviewNavigate}
      />
      
      {/* Drives Dialog */}
      <Dialog open={drivesDialog} onOpenChange={setDrivesDialog}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('fileManager.selectDrive')}</DialogTitle>
            <DialogDescription>{t('fileManager.selectDriveDesc')}</DialogDescription>
          </DialogHeader>
          <div className="grid grid-cols-4 gap-2 py-4">
            {availableDrives.map(drive => (
              <Button
                key={drive}
                variant="outline"
                className="h-16 flex flex-col items-center justify-center gap-1"
                onClick={() => handleSelectDrive(drive)}
              >
                <span className="text-lg font-bold">{drive.replace(/[:\\\/]/g, '')}</span>
                <span className="text-xs text-zinc-500">{drive}</span>
              </Button>
            ))}
          </div>
        </DialogContent>
      </Dialog>
      
      {/* New Folder Dialog */}
      <Dialog open={newFolderDialog} onOpenChange={setNewFolderDialog}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('fileManager.newFolder')}</DialogTitle>
            <DialogDescription>{t('fileManager.newFolderDesc')}</DialogDescription>
          </DialogHeader>
          <Input
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            placeholder={t('fileManager.folderName')}
            onKeyDown={(e) => e.key === 'Enter' && handleNewFolder()}
            autoFocus
          />
          <DialogFooter>
            <Button variant="ghost" onClick={() => setNewFolderDialog(false)}>
              {t('common.cancel')}
            </Button>
            <Button onClick={handleNewFolder} disabled={!inputValue.trim()}>
              {t('common.create')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      
      {/* Rename Dialog */}
      <Dialog open={!!renameDialog} onOpenChange={() => setRenameDialog(null)}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('fileManager.rename')}</DialogTitle>
            <DialogDescription>{t('fileManager.renameDesc')}</DialogDescription>
          </DialogHeader>
          <Input
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            placeholder={t('fileManager.newName')}
            onKeyDown={(e) => e.key === 'Enter' && handleRename()}
            autoFocus
          />
          <DialogFooter>
            <Button variant="ghost" onClick={() => setRenameDialog(null)}>
              {t('common.cancel')}
            </Button>
            <Button onClick={handleRename} disabled={!inputValue.trim()}>
              {t('common.rename')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      
      {/* Delete Confirm Dialog */}
      <Dialog open={!!deleteConfirm} onOpenChange={() => setDeleteConfirm(null)}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('fileManager.confirmDelete')}</DialogTitle>
            <DialogDescription>
              {t('fileManager.confirmDeleteDesc', { count: deleteConfirm?.length || 0 })}
            </DialogDescription>
          </DialogHeader>
          <div className="max-h-40 overflow-y-auto text-sm text-zinc-400">
            {deleteConfirm?.map(name => (
              <div key={name} className="py-1">{name}</div>
            ))}
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setDeleteConfirm(null)}>
              {t('common.cancel')}
            </Button>
            <Button variant="destructive" onClick={handleDelete}>
              <Trash2 className="h-4 w-4 mr-1" />
              {t('common.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};
