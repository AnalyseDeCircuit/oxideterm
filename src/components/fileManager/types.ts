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
  | 'compare';

export interface ContextMenuState {
  x: number;
  y: number;
  file?: FileInfo;
}

// File preview types
export type PreviewType = 
  | 'text' 
  | 'image' 
  | 'video' 
  | 'audio' 
  | 'pdf' 
  | 'hex' 
  | 'too-large' 
  | 'unsupported';

export interface FilePreview {
  name: string;
  path: string;
  type: PreviewType;
  data: string;
  mimeType?: string;
  language?: string | null;
  encoding?: string;
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
}
