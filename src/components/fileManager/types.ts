/**
 * File Manager Types
 * Shared types for local file management functionality
 */

export interface FileInfo {
  name: string;
  path: string;
  file_type: 'File' | 'Directory' | 'Symlink';
  size: number;
  modified: number;  // Unix timestamp
  permissions: string;
}

export type SortField = 'name' | 'size' | 'modified';
export type SortDirection = 'asc' | 'desc';

export interface SortOptions {
  field: SortField;
  direction: SortDirection;
}

export interface FileSelection {
  selected: Set<string>;
  lastSelected: string | null;
}

export interface FileNavigationState {
  path: string;
  pathInput: string;
  isEditing: boolean;
  homePath: string;
}

export interface FileListState {
  files: FileInfo[];
  loading: boolean;
  error: string | null;
}

export interface DragDropData {
  files: string[];
  source: 'local' | 'remote';
  basePath: string;
}

// Context menu actions
export type FileAction = 
  | 'open'
  | 'preview'
  | 'rename'
  | 'delete'
  | 'copy-path'
  | 'new-folder'
  | 'upload'
  | 'download'
  | 'compare'
  | 'copy'
  | 'cut'
  | 'paste'
  | 'compress'
  | 'extract';

export interface ContextMenuState {
  x: number;
  y: number;
  file?: FileInfo;
}

// File preview types
export type PreviewType =
  | 'text'
  | 'code'
  | 'markdown'
  | 'image'
  | 'video'
  | 'audio'
  | 'pdf'
  | 'office'
  | 'archive'
  | 'hex'
  | 'too-large'
  | 'unsupported';

// Archive entry info
export interface ArchiveEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  compressedSize: number;
  modified: string | null;
}

// Archive info for preview
export interface ArchiveInfo {
  entries: ArchiveEntry[];
  totalFiles: number;
  totalDirs: number;
  totalSize: number;
  compressedSize: number;
}

// File metadata from Rust backend
export interface FileMetadata {
  size: number;
  modified?: number;  // Unix timestamp in seconds
  created?: number;   // Unix timestamp in seconds (may not be available)
  accessed?: number;  // Unix timestamp in seconds
  mode?: number;      // Unix permissions mode (e.g., 0o755)
  readonly: boolean;
  isDir: boolean;
  isSymlink: boolean;
  mimeType?: string;
}

export interface StreamPreviewInfo {
  path: string;
  size: number;
  type: 'text' | 'code';
  language?: string;
  mimeType?: string;
}

export interface FilePreview {
  name: string;
  path: string;
  type: PreviewType;
  data: string;
  mimeType?: string;
  language?: string | null;
  encoding?: string;
  size?: number;
  // Metadata (loaded on preview)
  metadata?: FileMetadata;
  // Stream preview info (for large text/code files)
  stream?: StreamPreviewInfo;
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
  // Archive specific
  archiveInfo?: ArchiveInfo;
}

// Bookmark/Favorite types
export interface Bookmark {
  id: string;
  name: string;
  path: string;
  icon?: string;
  createdAt: number;
}

// Clipboard types for copy/cut/paste operations
export type ClipboardMode = 'copy' | 'cut';

export interface ClipboardData {
  files: FileInfo[];
  mode: ClipboardMode;
  sourcePath: string;
}
