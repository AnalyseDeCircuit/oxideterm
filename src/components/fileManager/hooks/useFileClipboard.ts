// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * useFileClipboard Hook
 * Manages file clipboard operations (copy, cut, paste)
 */

import { useState, useCallback } from 'react';
import { copyFile, rename, mkdir, readDir } from '@tauri-apps/plugin-fs';
import type { FileInfo, ClipboardData, ClipboardMode } from '../types';
import { isLocalPathEqual, isLocalPathWithin, joinLocalPath } from '../pathUtils';

export interface PasteProgress {
  /** Currently processing file index (1-based) */
  current: number;
  /** Total file count (including files inside directories) */
  total: number;
  /** Name of the file currently being processed */
  fileName: string;
  /** Whether the operation is in progress */
  active: boolean;
}

export interface UseFileClipboardOptions {
  onSuccess?: (message: string) => void;
  onError?: (title: string, message: string) => void;
  onProgress?: (progress: PasteProgress) => void;
}

export interface UseFileClipboardReturn {
  clipboard: ClipboardData | null;
  hasClipboard: boolean;
  clipboardMode: ClipboardMode | null;
  copy: (files: FileInfo[], sourcePath: string) => void;
  cut: (files: FileInfo[], sourcePath: string) => void;
  paste: (destPath: string) => Promise<void>;
  clear: () => void;
}

export function useFileClipboard(options: UseFileClipboardOptions = {}): UseFileClipboardReturn {
  const { onSuccess, onError, onProgress } = options;
  const [clipboard, setClipboard] = useState<ClipboardData | null>(null);

  // Copy files to clipboard
  const copy = useCallback((files: FileInfo[], sourcePath: string) => {
    setClipboard({
      files: [...files],
      mode: 'copy',
      sourcePath,
    });
  }, []);

  // Cut files to clipboard
  const cut = useCallback((files: FileInfo[], sourcePath: string) => {
    setClipboard({
      files: [...files],
      mode: 'cut',
      sourcePath,
    });
  }, []);

  // Clear clipboard
  const clear = useCallback(() => {
    setClipboard(null);
  }, []);

  // Count total files recursively (for progress tracking)
  // MAX_COUNT_DEPTH prevents unbounded recursion; visited detects symlink cycles.
  const MAX_COUNT_DEPTH = 20;

  const countFiles = async (files: FileInfo[]): Promise<number> => {
    const visited = new Set<string>();
    let count = 0;
    for (const file of files) {
      if (file.file_type === 'Directory') {
        count += await countDirFiles(file.path, 0, visited);
      } else {
        count++;
      }
    }
    return count;
  };

  const countDirFiles = async (dirPath: string, depth: number, visited: Set<string>): Promise<number> => {
    if (depth >= MAX_COUNT_DEPTH || visited.has(dirPath)) {
      // Estimate 1 for directories too deep or already visited (symlink cycle)
      return 1;
    }
    visited.add(dirPath);
    let count = 0;
    try {
      const entries = await readDir(dirPath);
      for (const entry of entries) {
        if (entry.isDirectory && entry.isSymlink !== true) {
          count += await countDirFiles(joinLocalPath(dirPath, entry.name), depth + 1, visited);
        } else {
          count++;
        }
      }
    } catch {
      // If we can't read, count as 1 to not block progress
      count = 1;
    }
    return count;
  };

  // Mutable progress tracker shared across a single paste operation.
  // emitProgress throttles onProgress calls to at most once per PROGRESS_THROTTLE_MS
  // to avoid a full-pane rerender on every copied file.
  const PROGRESS_THROTTLE_MS = 100;

  const emitProgress = (
    tracker: { done: number; total: number; fileName: string; lastEmit: number },
    force?: boolean,
  ) => {
    const now = performance.now();
    if (force || now - tracker.lastEmit >= PROGRESS_THROTTLE_MS) {
      tracker.lastEmit = now;
      onProgress?.({
        current: tracker.done,
        total: tracker.total,
        fileName: tracker.fileName,
        active: true,
      });
    }
  };

  // Recursively copy a directory (with progress tracking)
  const copyDirectory = async (
    srcPath: string,
    destPath: string,
    tracker: { done: number; total: number; fileName: string; lastEmit: number },
  ): Promise<void> => {
    // Create destination directory
    await mkdir(destPath, { recursive: true });
    
    // Read source directory contents
    const entries = await readDir(srcPath);
    
    for (const entry of entries) {
      const srcChildPath = joinLocalPath(srcPath, entry.name);
      const destChildPath = joinLocalPath(destPath, entry.name);
      
      if (entry.isDirectory && entry.isSymlink !== true) {
        await copyDirectory(srcChildPath, destChildPath, tracker);
      } else {
        await copyFile(srcChildPath, destChildPath);
        tracker.done++;
        tracker.fileName = entry.name;
        emitProgress(tracker);
      }
    }
  };

  // Try to copy/move a single file, auto-appending (N) suffix on name collision.
  // This avoids the TOCTOU race of checking existence before operating.
  const MAX_COLLISION_RETRIES = 100;

  const copyOrMoveWithRetry = async (
    srcPath: string,
    destDir: string,
    name: string,
    isDirectory: boolean,
    mode: ClipboardMode,
    tracker: { done: number; total: number; fileName: string; lastEmit: number },
  ): Promise<void> => {
    // Guard against path traversal in file names
    if (name.includes('/') || name.includes('\\') || name.includes('..')) {
      throw new Error(`Invalid file name: ${name}`);
    }

    const ext = isDirectory ? '' : (name.includes('.') ? `.${name.split('.').pop()}` : '');
    const baseName = isDirectory ? name : (ext ? name.slice(0, -ext.length) : name);

    let attempt = 0;
    let destName = name;

    while (attempt < MAX_COLLISION_RETRIES) {
      const destFilePath = joinLocalPath(destDir, destName);
      try {
        if (isDirectory) {
          if (mode === 'copy') {
            await copyDirectory(srcPath, destFilePath, tracker);
          } else {
            await rename(srcPath, destFilePath);
            tracker.done++;
            tracker.fileName = destName;
            emitProgress(tracker);
          }
        } else {
          if (mode === 'copy') {
            await copyFile(srcPath, destFilePath);
            tracker.done++;
            tracker.fileName = destName;
            emitProgress(tracker);
          } else {
            await rename(srcPath, destFilePath);
            tracker.done++;
            tracker.fileName = destName;
            emitProgress(tracker);
          }
        }
        return; // success
      } catch (err) {
        const errStr = String(err).toLowerCase();
        // Retry with incremented suffix on "already exists" errors
        if (errStr.includes('exist') || errStr.includes('eexist') || errStr.includes('already')) {
          attempt++;
          destName = `${baseName} (${attempt})${ext}`;
        } else {
          throw err; // non-collision error — propagate
        }
      }
    }
    // Exhausted retries — fall through with last attempted name
    throw new Error(`Too many name collisions for ${name}`);
  };

  // Paste files from clipboard
  const paste = useCallback(async (destPath: string) => {
    if (!clipboard) return;

    const { files, mode, sourcePath } = clipboard;
    let successCount = 0;
    let errorCount = 0;
    let firstError: string | null = null;

    // Count total files for progress (only for copy; move is atomic per top-level item)
    const totalFiles = mode === 'copy'
      ? await countFiles(files)
      : files.length;
    const tracker = { done: 0, total: totalFiles, fileName: '', lastEmit: 0 };

    // Signal progress start (force — always render the initial 0%)
    emitProgress(tracker, true);

    for (const file of files) {
      try {
        // Check if pasting to same directory
        const isSameDir = isLocalPathEqual(sourcePath, destPath);

        if (file.file_type === 'Directory' && !isSameDir && isLocalPathWithin(file.path, destPath)) {
          throw new Error(`Cannot ${mode} folder into itself or a subdirectory: ${file.name}`);
        }
        
        if (isSameDir && mode === 'cut') {
          // Cut + paste in same dir is a no-op
          tracker.done++;
          tracker.fileName = file.name;
          emitProgress(tracker);
          successCount++;
          continue;
        }

        // Use collision-safe copy/move (handles duplicates atomically)
        await copyOrMoveWithRetry(
          file.path,
          destPath,
          file.name,
          file.file_type === 'Directory',
          isSameDir ? 'copy' : mode, // same-dir cut treated above
          tracker,
        );
        
        successCount++;
      } catch (err) {
        console.error(`Failed to ${mode} file:`, file.name, err);
        if (!firstError) firstError = `${file.name}: ${String(err)}`;
        errorCount++;
        // Still advance tracker on error so bar doesn't stall
        tracker.done++;
        tracker.fileName = file.name;
        emitProgress(tracker);
      }
    }

    // Signal progress end (force — always render the final 100% / dismiss)
    onProgress?.({ current: totalFiles, total: totalFiles, fileName: '', active: false });

    // Clear clipboard after cut operation
    if (mode === 'cut') {
      setClipboard(null);
    }

    // Report results
    if (successCount > 0 && errorCount === 0) {
      const action = mode === 'copy' ? 'Copied' : 'Moved';
      onSuccess?.(`${action} ${successCount} item(s)`);
    } else if (errorCount > 0) {
      const detail = errorCount === 1 && firstError
        ? firstError
        : `Failed to paste ${errorCount} of ${files.length} items${firstError ? `\n${firstError}` : ''}`;
      onError?.('Paste Error', detail);
    }
  }, [clipboard, onSuccess, onError]);

  return {
    clipboard,
    hasClipboard: clipboard !== null && clipboard.files.length > 0,
    clipboardMode: clipboard?.mode ?? null,
    copy,
    cut,
    paste,
    clear,
  };
}
