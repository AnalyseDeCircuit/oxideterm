// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * useLocalFiles Hook
 * Manages local file system navigation and listing
 */

import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { readDir, stat, remove, rename as fsRename, mkdir } from '@tauri-apps/plugin-fs';
import { homeDir } from '@tauri-apps/api/path';
import { open } from '@tauri-apps/plugin-dialog';
import { api } from '../../../lib/api';
import type { FileInfo, SortField, SortDirection, DriveInfo } from '../types';
import { getLocalParentPath, joinLocalPath, normalizeLocalPath } from '../pathUtils';

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
  const normalizedInitialPath = initialPath ? normalizeLocalPath(initialPath) : '';
  
  // Core state
  const [files, setFiles] = useState<FileInfo[]>([]);
  const [path, setPath] = useState<string>(normalizedInitialPath || '');
  const [homePath, setHomePath] = useState<string>('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refreshRequestId = useRef(0);
  
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
      const normalizedHome = normalizeLocalPath(home);
      setHomePath(normalizedHome);
      if (!initialPath) {
        setPath(normalizedHome);
        setPathInput(normalizedHome);
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
    const requestId = ++refreshRequestId.current;
    
    setLoading(true);
    setError(null);
    
    try {
      const entries = await readDir(path);

      if (refreshRequestId.current !== requestId) {
        return;
      }

      // Map entries to stat tasks, batched to limit concurrency
      const BATCH_SIZE = entries.length >= 5000 ? 20 : entries.length >= 1000 ? 30 : 50;
      const fileList: FileInfo[] = [];

      for (let i = 0; i < entries.length; i += BATCH_SIZE) {
        const batch = entries.slice(i, i + BATCH_SIZE);
        const results = await Promise.all(
          batch.map(async (entry) => {
            const fullPath = joinLocalPath(path, entry.name);
            const isDir = entry.isDirectory === true;
            const isSymlink = entry.isSymlink === true;
            // Symlinks to directories keep 'Directory' so navigation/sorting works;
            // only non-directory symlinks get the 'Symlink' type.
            const fileType: FileInfo['file_type'] = isDir ? 'Directory' : isSymlink ? 'Symlink' : 'File';
            try {
              const info = await stat(fullPath);
              return {
                name: entry.name,
                path: fullPath,
                file_type: fileType,
                size: info.size || 0,
                modified: info.mtime ? Math.floor(info.mtime.getTime() / 1000) : 0,
                permissions: ''
              } satisfies FileInfo;
            } catch {
              return {
                name: entry.name,
                path: fullPath,
                file_type: fileType,
                size: 0,
                modified: 0,
                permissions: ''
              } satisfies FileInfo;
            }
          })
        );
        fileList.push(...results);

        if (refreshRequestId.current !== requestId) {
          return;
        }
      }
      
      // Initial sort: directories first, then alphabetically
      fileList.sort((a, b) => {
        if (a.file_type === 'Directory' && b.file_type !== 'Directory') return -1;
        if (a.file_type !== 'Directory' && b.file_type === 'Directory') return 1;
        return a.name.localeCompare(b.name);
      });
      
      if (refreshRequestId.current === requestId) {
        setFiles(fileList);
      }
    } catch (err) {
      console.error("Local list error:", err);
      if (refreshRequestId.current === requestId) {
        setError(String(err));
        setFiles([]);
      }
    } finally {
      if (refreshRequestId.current === requestId) {
        setLoading(false);
      }
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
    return getLocalParentPath(currentPath);
  }, []);
  
  // Navigation
  const navigate = useCallback((target: string) => {
    if (target === '..') {
      const parent = getParentPath(path);
      if (parent !== '__DRIVES__') {
        setPath(normalizeLocalPath(parent));
      }
      // If __DRIVES__, caller should handle showing drives dialog
    } else if (target === '~') {
      setPath(homePath);
    } else {
      setPath(normalizeLocalPath(target));
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
      setPath(normalizeLocalPath(pathInput.trim()));
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
        const normalizedSelected = normalizeLocalPath(selected);
        setPath(normalizedSelected);
        setPathInput(normalizedSelected);
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
    const newPath = joinLocalPath(path, name);
    await mkdir(newPath);
    await refresh();
  }, [path, refresh]);
  
  // Delete files
  const deleteFiles = useCallback(async (names: string[]) => {
    for (const name of names) {
      const filePath = joinLocalPath(path, name);
      await remove(filePath, { recursive: true });
    }
    await refresh();
  }, [path, refresh]);
  
  // Rename file
  const renameFile = useCallback(async (oldName: string, newName: string) => {
    const oldPath = joinLocalPath(path, oldName);
    const newPath = joinLocalPath(path, newName);
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
