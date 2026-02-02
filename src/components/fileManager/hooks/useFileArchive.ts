/**
 * useFileArchive Hook
 * Manages file compression and extraction operations
 */

import { useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { FileInfo } from '../types';

export interface UseFileArchiveOptions {
  onSuccess?: (message: string) => void;
  onError?: (title: string, message: string) => void;
}

export interface UseFileArchiveReturn {
  compressing: boolean;
  extracting: boolean;
  compress: (files: FileInfo[], destPath: string, archiveName?: string) => Promise<string | null>;
  extract: (archivePath: string, destPath: string) => Promise<boolean>;
  canExtract: (fileName: string) => boolean;
}

// Supported archive extensions
const ARCHIVE_EXTENSIONS = new Set(['zip', 'tar', 'gz', 'tgz', 'tar.gz', 'bz2', 'xz', '7z']);

export function useFileArchive(options: UseFileArchiveOptions = {}): UseFileArchiveReturn {
  const { onSuccess, onError } = options;
  const [compressing, setCompressing] = useState(false);
  const [extracting, setExtracting] = useState(false);

  // Check if file can be extracted
  const canExtract = useCallback((fileName: string): boolean => {
    const lower = fileName.toLowerCase();
    for (const ext of ARCHIVE_EXTENSIONS) {
      if (lower.endsWith(`.${ext}`)) {
        return true;
      }
    }
    return false;
  }, []);

  // Compress files into a zip archive
  const compress = useCallback(async (
    files: FileInfo[],
    destPath: string,
    archiveName?: string
  ): Promise<string | null> => {
    if (files.length === 0) return null;

    setCompressing(true);
    try {
      // Generate archive name if not provided
      const name = archiveName || (
        files.length === 1 
          ? `${files[0].name}.zip`
          : `Archive_${new Date().toISOString().slice(0, 10)}.zip`
      );
      
      const archivePath = `${destPath}/${name}`;
      const filePaths = files.map(f => f.path);
      
      // Call Rust backend to create archive
      await invoke('compress_files', {
        files: filePaths,
        archivePath,
      });
      
      onSuccess?.(`Created ${name}`);
      return archivePath;
    } catch (err) {
      console.error('Compression error:', err);
      onError?.('Compression Error', String(err));
      return null;
    } finally {
      setCompressing(false);
    }
  }, [onSuccess, onError]);

  // Extract archive to destination
  const extract = useCallback(async (
    archivePath: string,
    destPath: string
  ): Promise<boolean> => {
    setExtracting(true);
    try {
      // Call Rust backend to extract archive
      await invoke('extract_archive', {
        archivePath,
        destPath,
      });
      
      const archiveName = archivePath.split('/').pop() || 'archive';
      onSuccess?.(`Extracted ${archiveName}`);
      return true;
    } catch (err) {
      console.error('Extraction error:', err);
      onError?.('Extraction Error', String(err));
      return false;
    } finally {
      setExtracting(false);
    }
  }, [onSuccess, onError]);

  return {
    compressing,
    extracting,
    compress,
    extract,
    canExtract,
  };
}
