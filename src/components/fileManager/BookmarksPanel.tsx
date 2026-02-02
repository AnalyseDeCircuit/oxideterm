/**
 * BookmarksPanel Component
 * Sidebar panel for managing file manager bookmarks/favorites
 */

import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  Star, 
  Folder, 
  Trash2, 
  Edit3, 
  ChevronRight,
  Plus
} from 'lucide-react';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { cn } from '../../lib/utils';
import type { Bookmark } from './types';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../ui/dialog';

export interface BookmarksPanelProps {
  bookmarks: Bookmark[];
  currentPath: string;
  isBookmarked: boolean;
  onNavigate: (path: string) => void;
  onAddBookmark: (path: string, name?: string) => void;
  onRemoveBookmark: (id: string) => void;
  onUpdateBookmark: (id: string, updates: Partial<Omit<Bookmark, 'id'>>) => void;
  collapsed?: boolean;
}

export const BookmarksPanel: React.FC<BookmarksPanelProps> = ({
  bookmarks,
  currentPath,
  isBookmarked,
  onNavigate,
  onAddBookmark,
  onRemoveBookmark,
  onUpdateBookmark,
  collapsed = false,
}) => {
  const { t } = useTranslation();
  const [editDialog, setEditDialog] = useState<Bookmark | null>(null);
  const [editName, setEditName] = useState('');
  const [hoveredId, setHoveredId] = useState<string | null>(null);

  // Handle add current path
  const handleAddCurrent = () => {
    if (!isBookmarked) {
      onAddBookmark(currentPath);
    }
  };

  // Handle edit
  const handleOpenEdit = (bookmark: Bookmark) => {
    setEditName(bookmark.name);
    setEditDialog(bookmark);
  };

  const handleSaveEdit = () => {
    if (editDialog && editName.trim()) {
      onUpdateBookmark(editDialog.id, { name: editName.trim() });
      setEditDialog(null);
    }
  };

  if (collapsed) {
    return (
      <div className="py-2">
        <Button
          size="icon"
          variant="ghost"
          className={cn(
            "w-full h-8",
            isBookmarked && "text-yellow-500"
          )}
          onClick={handleAddCurrent}
          title={isBookmarked ? t('fileManager.bookmarked') : t('fileManager.addBookmark')}
        >
          <Star className={cn("h-4 w-4", isBookmarked && "fill-current")} />
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-theme-border">
        <div className="flex items-center gap-2 text-xs font-medium text-zinc-400 uppercase tracking-wider">
          <Star className="h-3 w-3" />
          {t('fileManager.favorites')}
        </div>
        <Button
          size="icon"
          variant="ghost"
          className={cn("h-6 w-6", isBookmarked && "text-yellow-500")}
          onClick={handleAddCurrent}
          title={isBookmarked ? t('fileManager.bookmarked') : t('fileManager.addBookmark')}
        >
          {isBookmarked ? (
            <Star className="h-3 w-3 fill-current" />
          ) : (
            <Plus className="h-3 w-3" />
          )}
        </Button>
      </div>

      {/* Bookmarks list */}
      <div className="flex-1 overflow-y-auto py-1">
        {bookmarks.length === 0 ? (
          <div className="px-3 py-4 text-xs text-zinc-600 text-center">
            {t('fileManager.noBookmarks')}
          </div>
        ) : (
          bookmarks.map(bookmark => (
            <div
              key={bookmark.id}
              className={cn(
                "group flex items-center gap-2 px-3 py-1.5 cursor-pointer hover:bg-zinc-800/50 transition-colors",
                currentPath === bookmark.path && "bg-theme-accent/10 text-theme-accent"
              )}
              onClick={() => onNavigate(bookmark.path)}
              onMouseEnter={() => setHoveredId(bookmark.id)}
              onMouseLeave={() => setHoveredId(null)}
            >
              <Folder className="h-3.5 w-3.5 text-blue-400 shrink-0" />
              <span className="flex-1 text-xs truncate">{bookmark.name}</span>
              
              {/* Actions - visible on hover */}
              {hoveredId === bookmark.id && (
                <div className="flex items-center gap-0.5">
                  <Button
                    size="icon"
                    variant="ghost"
                    className="h-5 w-5 opacity-60 hover:opacity-100"
                    onClick={(e: React.MouseEvent) => {
                      e.stopPropagation();
                      handleOpenEdit(bookmark);
                    }}
                    title={t('fileManager.editBookmark')}
                  >
                    <Edit3 className="h-2.5 w-2.5" />
                  </Button>
                  <Button
                    size="icon"
                    variant="ghost"
                    className="h-5 w-5 opacity-60 hover:opacity-100 hover:text-red-400"
                    onClick={(e: React.MouseEvent) => {
                      e.stopPropagation();
                      onRemoveBookmark(bookmark.id);
                    }}
                    title={t('fileManager.removeBookmark')}
                  >
                    <Trash2 className="h-2.5 w-2.5" />
                  </Button>
                </div>
              )}
              
              {hoveredId !== bookmark.id && (
                <ChevronRight className="h-3 w-3 text-zinc-600 opacity-0 group-hover:opacity-100" />
              )}
            </div>
          ))
        )}
      </div>

      {/* Edit Dialog */}
      <Dialog open={!!editDialog} onOpenChange={() => setEditDialog(null)}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('fileManager.editBookmark')}</DialogTitle>
            <DialogDescription>{t('fileManager.editBookmarkDesc')}</DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            <div>
              <label className="text-xs text-zinc-500">{t('fileManager.bookmarkName')}</label>
              <Input
                value={editName}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) => setEditName(e.target.value)}
                placeholder={t('fileManager.bookmarkName')}
                className="mt-1"
                onKeyDown={(e: React.KeyboardEvent) => e.key === 'Enter' && handleSaveEdit()}
                autoFocus
              />
            </div>
            {editDialog && (
              <div>
                <label className="text-xs text-zinc-500">{t('fileManager.bookmarkPath')}</label>
                <p className="text-xs text-zinc-400 mt-1 font-mono bg-zinc-900 px-2 py-1 rounded truncate">
                  {editDialog.path}
                </p>
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setEditDialog(null)}>
              {t('common.cancel')}
            </Button>
            <Button onClick={handleSaveEdit} disabled={!editName.trim()}>
              {t('common.save')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};
