/**
 * useFileClipboard Hook
 * Manages file clipboard operations (copy, cut, paste)
 */

import { useState, useCallback } from 'react';
import { copyFile, rename, mkdir, readDir, stat } from '@tauri-apps/plugin-fs';
import type { FileInfo, ClipboardData, ClipboardMode } from '../types';

export interface UseFileClipboardOptions {
  onSuccess?: (message: string) => void;
  onError?: (title: string, message: string) => void;
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
  const { onSuccess, onError } = options;
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

  // Recursively copy a directory
  const copyDirectory = async (srcPath: string, destPath: string): Promise<void> => {
    // Create destination directory
    await mkdir(destPath, { recursive: true });
    
    // Read source directory contents
    const entries = await readDir(srcPath);
    
    for (const entry of entries) {
      const srcChildPath = `${srcPath}/${entry.name}`;
      const destChildPath = `${destPath}/${entry.name}`;
      
      if (entry.isDirectory) {
        await copyDirectory(srcChildPath, destChildPath);
      } else {
        await copyFile(srcChildPath, destChildPath);
      }
    }
  };

  // Generate unique name if file exists
  const getUniqueName = async (destPath: string, name: string, isDirectory: boolean): Promise<string> => {
    const fullPath = `${destPath}/${name}`;
    
    try {
      await stat(fullPath);
      // File exists, generate unique name
      const ext = isDirectory ? '' : (name.includes('.') ? `.${name.split('.').pop()}` : '');
      const baseName = isDirectory ? name : (ext ? name.slice(0, -ext.length) : name);
      
      let counter = 1;
      let newName = `${baseName} (${counter})${ext}`;
      
      while (true) {
        try {
          await stat(`${destPath}/${newName}`);
          counter++;
          newName = `${baseName} (${counter})${ext}`;
        } catch {
          // Name is available
          return newName;
        }
      }
    } catch {
      // File doesn't exist, use original name
      return name;
    }
  };

  // Paste files from clipboard
  const paste = useCallback(async (destPath: string) => {
    if (!clipboard) return;

    const { files, mode, sourcePath } = clipboard;
    let successCount = 0;
    let errorCount = 0;
    let firstError: string | null = null;

    for (const file of files) {
      try {
        // Check if pasting to same directory
        const isSameDir = sourcePath === destPath;
        
        // Get destination name (handle duplicates)
        const destName = isSameDir && mode === 'copy'
          ? await getUniqueName(destPath, file.name, file.file_type === 'Directory')
          : file.name;
        
        const destFilePath = `${destPath}/${destName}`;
        
        if (file.file_type === 'Directory') {
          if (mode === 'copy') {
            await copyDirectory(file.path, destFilePath);
          } else {
            // Cut = move
            if (!isSameDir) {
              await rename(file.path, destFilePath);
            }
          }
        } else {
          if (mode === 'copy') {
            await copyFile(file.path, destFilePath);
          } else {
            // Cut = move
            if (!isSameDir) {
              await rename(file.path, destFilePath);
            }
          }
        }
        
        successCount++;
      } catch (err) {
        console.error(`Failed to ${mode} file:`, file.name, err);
        if (!firstError) firstError = `${file.name}: ${String(err)}`;
        errorCount++;
      }
    }

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
