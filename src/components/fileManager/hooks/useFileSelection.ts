/**
 * useFileSelection Hook
 * Handles multi-select, range select, and selection state
 */

import { useState, useCallback } from 'react';
import type { FileInfo } from '../types';

export interface UseFileSelectionOptions {
  files: FileInfo[];
}

export interface UseFileSelectionReturn {
  selected: Set<string>;
  lastSelected: string | null;
  
  // Selection actions
  select: (name: string, multi: boolean, range: boolean) => void;
  selectAll: () => void;
  clearSelection: () => void;
  setSelected: (selected: Set<string>) => void;
  setLastSelected: (name: string | null) => void;
  
  // Helpers
  isSelected: (name: string) => boolean;
  getSelectedFiles: () => FileInfo[];
  getSelectedNames: () => string[];
}

export function useFileSelection({ files }: UseFileSelectionOptions): UseFileSelectionReturn {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [lastSelected, setLastSelected] = useState<string | null>(null);
  
  // Select with multi and range support
  const select = useCallback((name: string, multi: boolean, range: boolean) => {
    const newSelected = new Set(multi ? selected : []);
    
    if (range && lastSelected && files.length > 0) {
      // Range select (Shift+click)
      const start = files.findIndex(f => f.name === lastSelected);
      const end = files.findIndex(f => f.name === name);
      
      if (start > -1 && end > -1) {
        const [min, max] = [Math.min(start, end), Math.max(start, end)];
        for (let i = min; i <= max; i++) {
          newSelected.add(files[i].name);
        }
      }
    } else {
      // Single or multi select
      if (newSelected.has(name) && multi) {
        newSelected.delete(name);
      } else {
        newSelected.add(name);
      }
    }
    
    setSelected(newSelected);
    setLastSelected(name);
  }, [selected, lastSelected, files]);
  
  // Select all files
  const selectAll = useCallback(() => {
    setSelected(new Set(files.map(f => f.name)));
  }, [files]);
  
  // Clear selection
  const clearSelection = useCallback(() => {
    setSelected(new Set());
    setLastSelected(null);
  }, []);
  
  // Check if file is selected
  const isSelected = useCallback((name: string) => {
    return selected.has(name);
  }, [selected]);
  
  // Get selected FileInfo objects
  const getSelectedFiles = useCallback(() => {
    return files.filter(f => selected.has(f.name));
  }, [files, selected]);
  
  // Get selected names as array
  const getSelectedNames = useCallback(() => {
    return Array.from(selected);
  }, [selected]);
  
  return {
    selected,
    lastSelected,
    select,
    selectAll,
    clearSelection,
    setSelected,
    setLastSelected,
    isSelected,
    getSelectedFiles,
    getSelectedNames,
  };
}
