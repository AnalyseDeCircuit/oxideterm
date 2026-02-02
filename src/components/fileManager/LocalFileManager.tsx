/**
 * LocalFileManager Component
 * Standalone local file browser panel
 */

import React, { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { FolderPlus, Trash2 } from 'lucide-react';
import { FileList } from './FileList';
import { useLocalFiles, useFileSelection } from './hooks';
import { useToast } from '../../hooks/useToast';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { 
  Dialog, 
  DialogContent, 
  DialogHeader, 
  DialogTitle, 
  DialogDescription,
  DialogFooter
} from '../ui/dialog';
import type { FileInfo } from './types';

// Preview imports from editor (for file preview)
import { readFile } from '@tauri-apps/plugin-fs';

export interface LocalFileManagerProps {
  className?: string;
}

export const LocalFileManager: React.FC<LocalFileManagerProps> = ({ className }) => {
  const { t } = useTranslation();
  const { success: toastSuccess, error: toastError } = useToast();
  
  // Use hooks
  const localFiles = useLocalFiles();
  const selection = useFileSelection({ files: localFiles.displayFiles });
  
  // Dialog states
  const [newFolderDialog, setNewFolderDialog] = useState(false);
  const [renameDialog, setRenameDialog] = useState<string | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<string[] | null>(null);
  const [drivesDialog, setDrivesDialog] = useState(false);
  const [availableDrives, setAvailableDrives] = useState<string[]>([]);
  const [inputValue, setInputValue] = useState('');
  
  // Preview state
  const [previewFile, setPreviewFile] = useState<{
    name: string;
    content: string;
    type: 'text' | 'image' | 'unsupported';
  } | null>(null);
  
  // Handle show drives
  const handleShowDrives = useCallback(async () => {
    const drives = await localFiles.showDrives();
    setAvailableDrives(drives);
    setDrivesDialog(true);
  }, [localFiles]);
  
  // Handle navigate with drives support
  const handleNavigate = useCallback((target: string) => {
    if (target === '..') {
      // Check if going to drives
      const parent = localFiles.path;
      if (/^[A-Za-z]:\\?$/.test(parent) || /^[A-Za-z]:$/.test(parent)) {
        handleShowDrives();
        return;
      }
    }
    localFiles.navigate(target);
    selection.clearSelection();
  }, [localFiles, selection, handleShowDrives]);
  
  // Handle select drive
  const handleSelectDrive = useCallback((drive: string) => {
    localFiles.navigate(drive);
    selection.clearSelection();
    setDrivesDialog(false);
  }, [localFiles, selection]);
  
  // Handle new folder
  const handleNewFolder = useCallback(async () => {
    if (!inputValue.trim()) return;
    try {
      await localFiles.createFolder(inputValue.trim());
      toastSuccess(t('fileManager.folderCreated'), inputValue.trim());
      setNewFolderDialog(false);
      setInputValue('');
    } catch (err) {
      toastError(t('fileManager.error'), String(err));
    }
  }, [localFiles, inputValue, toastSuccess, toastError, t]);
  
  // Handle rename
  const handleRename = useCallback(async () => {
    if (!renameDialog || !inputValue.trim()) return;
    try {
      await localFiles.renameFile(renameDialog, inputValue.trim());
      toastSuccess(t('fileManager.renamed'), `${renameDialog} → ${inputValue.trim()}`);
      setRenameDialog(null);
      setInputValue('');
      selection.clearSelection();
    } catch (err) {
      toastError(t('fileManager.error'), String(err));
    }
  }, [localFiles, renameDialog, inputValue, toastSuccess, toastError, t, selection]);
  
  // Handle delete
  const handleDelete = useCallback(async () => {
    if (!deleteConfirm || deleteConfirm.length === 0) return;
    try {
      await localFiles.deleteFiles(deleteConfirm);
      toastSuccess(
        t('fileManager.deleted'), 
        t('fileManager.deletedCount', { count: deleteConfirm.length })
      );
      setDeleteConfirm(null);
      selection.clearSelection();
    } catch (err) {
      toastError(t('fileManager.error'), String(err));
    }
  }, [localFiles, deleteConfirm, toastSuccess, toastError, t, selection]);
  
  // Handle preview
  const handlePreview = useCallback(async (file: FileInfo) => {
    try {
      // Check file extension for type
      const ext = file.name.split('.').pop()?.toLowerCase() || '';
      const textExts = ['txt', 'md', 'json', 'js', 'ts', 'jsx', 'tsx', 'css', 'html', 'xml', 'yaml', 'yml', 'toml', 'sh', 'py', 'rs', 'go', 'java', 'c', 'cpp', 'h'];
      const imageExts = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico', 'bmp'];
      
      if (textExts.includes(ext)) {
        const content = await readFile(file.path);
        const decoder = new TextDecoder();
        setPreviewFile({
          name: file.name,
          content: decoder.decode(content),
          type: 'text'
        });
      } else if (imageExts.includes(ext)) {
        // For images, we'd need to create a data URL
        // For now, just show unsupported
        setPreviewFile({
          name: file.name,
          content: t('fileManager.imagePreviewNotSupported'),
          type: 'unsupported'
        });
      } else {
        setPreviewFile({
          name: file.name,
          content: t('fileManager.unsupportedFormat'),
          type: 'unsupported'
        });
      }
    } catch (err) {
      toastError(t('fileManager.previewError'), String(err));
    }
  }, [toastError, t]);
  
  return (
    <div className={`flex flex-col h-full ${className || ''}`}>
      {/* Toolbar */}
      <div className="flex items-center gap-2 p-2 bg-theme-bg-panel border-b border-theme-border">
        <span className="text-sm font-medium text-zinc-300">{t('fileManager.title')}</span>
        <div className="flex-1" />
        <Button 
          size="sm" 
          variant="ghost" 
          className="h-7"
          onClick={() => {
            setInputValue('');
            setNewFolderDialog(true);
          }}
        >
          <FolderPlus className="h-3.5 w-3.5 mr-1" />
          {t('fileManager.newFolder')}
        </Button>
      </div>
      
      {/* File List */}
      <div className="flex-1 min-h-0">
        <FileList
          title={t('fileManager.local')}
          files={localFiles.displayFiles}
          path={localFiles.path}
          isRemote={false}
          active={true}
          loading={localFiles.loading}
          selected={selection.selected}
          lastSelected={selection.lastSelected}
          onSelect={selection.select}
          onSelectAll={selection.selectAll}
          onClearSelection={selection.clearSelection}
          onNavigate={handleNavigate}
          onRefresh={localFiles.refresh}
          isPathEditable={localFiles.isPathEditing}
          pathInputValue={localFiles.pathInput}
          onPathInputChange={localFiles.setPathInput}
          onPathInputSubmit={localFiles.submitPathInput}
          filter={localFiles.filter}
          onFilterChange={localFiles.setFilter}
          sortField={localFiles.sortField}
          sortDirection={localFiles.sortDirection}
          onSortChange={localFiles.toggleSort}
          onPreview={handlePreview}
          onDelete={(files) => setDeleteConfirm(files)}
          onRename={(name) => {
            setInputValue(name);
            setRenameDialog(name);
          }}
          onNewFolder={() => {
            setInputValue('');
            setNewFolderDialog(true);
          }}
          onBrowse={localFiles.browseFolder}
          onShowDrives={handleShowDrives}
          t={t}
        />
      </div>
      
      {/* Drives Dialog */}
      <Dialog open={drivesDialog} onOpenChange={setDrivesDialog}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('fileManager.selectDrive')}</DialogTitle>
            <DialogDescription>{t('fileManager.selectDriveDesc')}</DialogDescription>
          </DialogHeader>
          <div className="grid grid-cols-4 gap-2 py-4">
            {availableDrives.map(drive => (
              <Button
                key={drive}
                variant="outline"
                className="h-16 flex flex-col items-center justify-center gap-1"
                onClick={() => handleSelectDrive(drive)}
              >
                <span className="text-lg font-bold">{drive.replace(/[:\\\/]/g, '')}</span>
                <span className="text-xs text-zinc-500">{drive}</span>
              </Button>
            ))}
          </div>
        </DialogContent>
      </Dialog>
      
      {/* New Folder Dialog */}
      <Dialog open={newFolderDialog} onOpenChange={setNewFolderDialog}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('fileManager.newFolder')}</DialogTitle>
            <DialogDescription>{t('fileManager.newFolderDesc')}</DialogDescription>
          </DialogHeader>
          <Input
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            placeholder={t('fileManager.folderName')}
            onKeyDown={(e) => e.key === 'Enter' && handleNewFolder()}
            autoFocus
          />
          <DialogFooter>
            <Button variant="ghost" onClick={() => setNewFolderDialog(false)}>
              {t('common.cancel')}
            </Button>
            <Button onClick={handleNewFolder} disabled={!inputValue.trim()}>
              {t('common.create')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      
      {/* Rename Dialog */}
      <Dialog open={!!renameDialog} onOpenChange={() => setRenameDialog(null)}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('fileManager.rename')}</DialogTitle>
            <DialogDescription>{t('fileManager.renameDesc')}</DialogDescription>
          </DialogHeader>
          <Input
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            placeholder={t('fileManager.newName')}
            onKeyDown={(e) => e.key === 'Enter' && handleRename()}
            autoFocus
          />
          <DialogFooter>
            <Button variant="ghost" onClick={() => setRenameDialog(null)}>
              {t('common.cancel')}
            </Button>
            <Button onClick={handleRename} disabled={!inputValue.trim()}>
              {t('common.rename')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      
      {/* Delete Confirm Dialog */}
      <Dialog open={!!deleteConfirm} onOpenChange={() => setDeleteConfirm(null)}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('fileManager.confirmDelete')}</DialogTitle>
            <DialogDescription>
              {t('fileManager.confirmDeleteDesc', { count: deleteConfirm?.length || 0 })}
            </DialogDescription>
          </DialogHeader>
          <div className="max-h-40 overflow-y-auto text-sm text-zinc-400">
            {deleteConfirm?.map(name => (
              <div key={name} className="py-1">{name}</div>
            ))}
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setDeleteConfirm(null)}>
              {t('common.cancel')}
            </Button>
            <Button variant="destructive" onClick={handleDelete}>
              <Trash2 className="h-4 w-4 mr-1" />
              {t('common.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      
      {/* Preview Dialog */}
      <Dialog open={!!previewFile} onOpenChange={() => setPreviewFile(null)}>
        <DialogContent className="max-w-3xl max-h-[80vh]">
          <DialogHeader>
            <DialogTitle>{previewFile?.name}</DialogTitle>
          </DialogHeader>
          <div className="overflow-auto max-h-[60vh]">
            {previewFile?.type === 'text' ? (
              <pre className="text-xs font-mono bg-zinc-900 p-4 rounded overflow-x-auto whitespace-pre-wrap">
                {previewFile.content}
              </pre>
            ) : (
              <div className="text-center py-8 text-zinc-500">
                {previewFile?.content}
              </div>
            )}
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
};
