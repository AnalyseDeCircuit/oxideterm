/**
 * useBookmarks Hook
 * Manages file manager bookmarks/favorites with localStorage persistence
 */

import { useState, useEffect, useCallback } from 'react';
import type { Bookmark } from '../types';

const STORAGE_KEY = 'oxideterm-file-bookmarks';

export interface UseBookmarksReturn {
  bookmarks: Bookmark[];
  addBookmark: (path: string, name?: string) => void;
  removeBookmark: (id: string) => void;
  updateBookmark: (id: string, updates: Partial<Omit<Bookmark, 'id'>>) => void;
  reorderBookmarks: (fromIndex: number, toIndex: number) => void;
  isBookmarked: (path: string) => boolean;
  getBookmark: (path: string) => Bookmark | undefined;
}

export function useBookmarks(): UseBookmarksReturn {
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);

  // Load bookmarks from localStorage on mount
  useEffect(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        if (Array.isArray(parsed)) {
          setBookmarks(parsed);
        }
      }
    } catch (e) {
      console.error('Failed to load bookmarks:', e);
    }
  }, []);

  // Save bookmarks to localStorage whenever they change
  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(bookmarks));
    } catch (e) {
      console.error('Failed to save bookmarks:', e);
    }
  }, [bookmarks]);

  // Add a new bookmark
  const addBookmark = useCallback((path: string, name?: string) => {
    // Extract folder/file name from path if name not provided
    const pathParts = path.replace(/[\\\/]+$/, '').split(/[\\\/]/);
    const defaultName = pathParts[pathParts.length - 1] || path;

    const newBookmark: Bookmark = {
      id: crypto.randomUUID(),
      name: name || defaultName,
      path: path,
      createdAt: Date.now(),
    };

    setBookmarks(prev => {
      // Don't add duplicate paths
      if (prev.some(b => b.path === path)) {
        return prev;
      }
      return [...prev, newBookmark];
    });
  }, []);

  // Remove a bookmark by ID
  const removeBookmark = useCallback((id: string) => {
    setBookmarks(prev => prev.filter(b => b.id !== id));
  }, []);

  // Update a bookmark
  const updateBookmark = useCallback((id: string, updates: Partial<Omit<Bookmark, 'id'>>) => {
    setBookmarks(prev => prev.map(b => 
      b.id === id ? { ...b, ...updates } : b
    ));
  }, []);

  // Reorder bookmarks (for drag-and-drop)
  const reorderBookmarks = useCallback((fromIndex: number, toIndex: number) => {
    setBookmarks(prev => {
      const newBookmarks = [...prev];
      const [removed] = newBookmarks.splice(fromIndex, 1);
      newBookmarks.splice(toIndex, 0, removed);
      return newBookmarks;
    });
  }, []);

  // Check if a path is bookmarked
  const isBookmarked = useCallback((path: string) => {
    return bookmarks.some(b => b.path === path);
  }, [bookmarks]);

  // Get bookmark by path
  const getBookmark = useCallback((path: string) => {
    return bookmarks.find(b => b.path === path);
  }, [bookmarks]);

  return {
    bookmarks,
    addBookmark,
    removeBookmark,
    updateBookmark,
    reorderBookmarks,
    isBookmarked,
    getBookmark,
  };
}
