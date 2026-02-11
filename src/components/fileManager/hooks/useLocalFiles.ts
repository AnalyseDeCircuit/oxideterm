/**
 * useLocalFiles Hook
 * Manages local file system navigation and listing
 */

import { useState, useEffect, useCallback, useMemo } from 'react';
import { readDir, stat, remove, rename as fsRename, mkdir } from '@tauri-apps/plugin-fs';
import { homeDir } from '@tauri-apps/api/path';
import { open } from '@tauri-apps/plugin-dialog';
import { api } from '../../../lib/api';
import type { FileInfo, SortField, SortDirection, DriveInfo } from '../types';

export interface UseLocalFilesOptions {
  initialPath?: string;
}

export interface UseLocalFilesReturn {
  // State
  files: FileInfo[];
  displayFiles: FileInfo[];
  path: string;
  homePath: string;
  loading: boolean;
  error: string | null;
  
  // Path editing
  pathInput: string;
  isPathEditing: boolean;
  setPathInput: (value: string) => void;
  setIsPathEditing: (editing: boolean) => void;
  submitPathInput: () => void;
  
  // Filter & Sort
  filter: string;
  setFilter: (value: string) => void;
  sortField: SortField;
  sortDirection: SortDirection;
  toggleSort: (field: SortField) => void;
  
  // Navigation
  navigate: (target: string) => void;
  goUp: () => void;
  goHome: () => void;
  refresh: () => Promise<void>;
  
  // Actions
  browseFolder: () => Promise<void>;
  showDrives: () => Promise<DriveInfo[]>;
  createFolder: (name: string) => Promise<void>;
  deleteFiles: (names: string[]) => Promise<void>;
  renameFile: (oldName: string, newName: string) => Promise<void>;
}

export function useLocalFiles(options: UseLocalFilesOptions = {}): UseLocalFilesReturn {
  const { initialPath } = options;
  
  // Core state
  const [files, setFiles] = useState<FileInfo[]>([]);
  const [path, setPath] = useState<string>(initialPath || '');
  const [homePath, setHomePath] = useState<string>('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  // Path editing
  const [pathInput, setPathInput] = useState('');
  const [isPathEditing, setIsPathEditing] = useState(false);
  
  // Filter & Sort
  const [filter, setFilter] = useState('');
  const [sortField, setSortField] = useState<SortField>('name');
  const [sortDirection, setSortDirection] = useState<SortDirection>('asc');
  
  // Initialize home directory
  useEffect(() => {
    homeDir().then(home => {
      setHomePath(home);
      if (!initialPath) {
        setPath(home);
        setPathInput(home);
      }
    }).catch(() => {
      if (!initialPath) {
        setPath('/');
        setPathInput('/');
      }
    });
  }, [initialPath]);
  
  // Sync path input when path changes (and not editing)
  useEffect(() => {
    if (!isPathEditing) {
      setPathInput(path);
    }
  }, [path, isPathEditing]);
  
  // Refresh file list
  const refresh = useCallback(async () => {
    if (!path) return;
    
    setLoading(true);
    setError(null);
    
    try {
      const entries = await readDir(path);
      const fileList: FileInfo[] = await Promise.all(
        entries.map(async (entry) => {
          // Normalize path to avoid double slashes
          const basePath = path.endsWith('/') ? path.slice(0, -1) : path;
          const fullPath = `${basePath}/${entry.name}`;
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
      
      // Initial sort: directories first, then alphabetically
      fileList.sort((a, b) => {
        if (a.file_type === 'Directory' && b.file_type !== 'Directory') return -1;
        if (a.file_type !== 'Directory' && b.file_type === 'Directory') return 1;
        return a.name.localeCompare(b.name);
      });
      
      setFiles(fileList);
    } catch (err) {
      console.error("Local list error:", err);
      setError(String(err));
      setFiles([]);
    } finally {
      setLoading(false);
    }
  }, [path]);
  
  // Auto-refresh when path changes
  useEffect(() => {
    refresh();
  }, [refresh]);
  
  // Filter and sort files
  const displayFiles = useMemo(() => {
    let result = files;
    
    // Apply filter
    if (filter.trim()) {
      const lowerFilter = filter.toLowerCase();
      result = result.filter(f => f.name.toLowerCase().includes(lowerFilter));
    }
    
    // Apply sort (directories always first)
    result = [...result].sort((a, b) => {
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
    
    return result;
  }, [files, filter, sortField, sortDirection]);
  
  // Toggle sort
  const toggleSort = useCallback((field: SortField) => {
    if (sortField === field) {
      setSortDirection(d => d === 'asc' ? 'desc' : 'asc');
    } else {
      setSortField(field);
      setSortDirection('asc');
    }
  }, [sortField]);
  
  // Path utilities
  const getParentPath = useCallback((currentPath: string): string | '__DRIVES__' => {
    // Windows drive root
    if (/^[A-Za-z]:\\?$/.test(currentPath) || /^[A-Za-z]:$/.test(currentPath)) {
      return '__DRIVES__';
    }
    // Unix root
    if (currentPath === '/') {
      return '/';
    }
    
    const normalized = currentPath.replace(/\\/g, '/');
    const parts = normalized.split('/').filter(Boolean);
    parts.pop();
    
    // Windows drive letter
    if (parts.length === 1 && /^[A-Za-z]:$/.test(parts[0])) {
      return parts[0] + '\\';
    }
    // Unix or Windows path
    if (parts.length === 0) {
      if (/^[A-Za-z]:/.test(currentPath)) {
        return currentPath.substring(0, 3);
      }
      return '/';
    }
    
    const separator = currentPath.includes('\\') ? '\\' : '/';
    const result = parts.join(separator);
    if (/^[A-Za-z]:$/.test(result)) {
      return result + '\\';
    }
    return currentPath.startsWith('/') ? '/' + result : result;
  }, []);
  
  // Navigation
  const navigate = useCallback((target: string) => {
    if (target === '..') {
      const parent = getParentPath(path);
      if (parent !== '__DRIVES__') {
        setPath(parent);
      }
      // If __DRIVES__, caller should handle showing drives dialog
    } else if (target === '~') {
      setPath(homePath);
    } else {
      setPath(target);
    }
    setIsPathEditing(false);
  }, [path, homePath, getParentPath]);
  
  const goUp = useCallback(() => {
    navigate('..');
  }, [navigate]);
  
  const goHome = useCallback(() => {
    navigate('~');
  }, [navigate]);
  
  // Submit path input
  const submitPathInput = useCallback(() => {
    if (pathInput.trim()) {
      setPath(pathInput.trim());
    }
    setIsPathEditing(false);
  }, [pathInput]);
  
  // Browse folder dialog
  const browseFolder = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: path || undefined
      });
      if (selected && typeof selected === 'string') {
        setPath(selected);
        setPathInput(selected);
        setIsPathEditing(false);
      }
    } catch (err) {
      console.error('Browse folder error:', err);
    }
  }, [path]);
  
  // Show drives / mounted volumes (cross-platform)
  const showDrives = useCallback(async (): Promise<DriveInfo[]> => {
    try {
      return await api.localGetDrives();
    } catch {
      return [{ path: '/', name: 'System', driveType: 'system', totalSpace: 0, availableSpace: 0, isReadOnly: false }];
    }
  }, []);
  
  // Create folder
  const createFolder = useCallback(async (name: string) => {
    const newPath = `${path}/${name}`;
    await mkdir(newPath);
    await refresh();
  }, [path, refresh]);
  
  // Delete files
  const deleteFiles = useCallback(async (names: string[]) => {
    for (const name of names) {
      const filePath = `${path}/${name}`;
      await remove(filePath, { recursive: true });
    }
    await refresh();
  }, [path, refresh]);
  
  // Rename file
  const renameFile = useCallback(async (oldName: string, newName: string) => {
    const oldPath = `${path}/${oldName}`;
    const newPath = `${path}/${newName}`;
    await fsRename(oldPath, newPath);
    await refresh();
  }, [path, refresh]);
  
  return {
    // State
    files,
    displayFiles,
    path,
    homePath,
    loading,
    error,
    
    // Path editing
    pathInput,
    isPathEditing,
    setPathInput,
    setIsPathEditing,
    submitPathInput,
    
    // Filter & Sort
    filter,
    setFilter,
    sortField,
    sortDirection,
    toggleSort,
    
    // Navigation
    navigate,
    goUp,
    goHome,
    refresh,
    
    // Actions
    browseFolder,
    showDrives,
    createFolder,
    deleteFiles,
    renameFile,
  };
}
