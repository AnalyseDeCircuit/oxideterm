// src/components/ide/dialogs/IdeDeleteConfirmDialog.tsx
import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';
import { Button } from '../../ui/button';
import { AlertTriangle, Folder, File, AlertCircle } from 'lucide-react';

interface IdeDeleteConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  path: string;
  name: string;
  isDirectory: boolean;
  affectedTabCount: number; // 将被关闭的标签数
  unsavedTabCount: number; // 有未保存更改的标签数
  onConfirm: () => void;
  isDeleting?: boolean; // 删除中状态
}

export function IdeDeleteConfirmDialog({
  open,
  onOpenChange,
  path,
  name,
  isDirectory,
  affectedTabCount,
  unsavedTabCount,
  onConfirm,
  isDeleting = false,
}: IdeDeleteConfirmDialogProps) {
  const { t } = useTranslation();

  const canDelete = unsavedTabCount === 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <AlertTriangle className="w-5 h-5 text-red-500" />
            {t('ide.delete.confirmTitle', 'Confirm Delete')}
          </DialogTitle>
          <DialogDescription asChild>
            <div className="space-y-3 pt-2">
              {/* 文件/文件夹信息 */}
              <div className="flex items-center gap-2 p-3 bg-theme-bg-hover rounded-lg">
                {isDirectory ? (
                  <Folder className="w-5 h-5 text-yellow-500 flex-shrink-0" />
                ) : (
                  <File className="w-5 h-5 text-theme-text-muted flex-shrink-0" />
                )}
                <span className="font-mono text-sm truncate" title={path}>
                  {name}
                </span>
              </div>

              {/* 目录警告 */}
              {isDirectory && (
                <div className="flex items-start gap-2 text-yellow-500 text-sm">
                  <AlertCircle className="w-4 h-4 mt-0.5 flex-shrink-0" />
                  <span>
                    {t(
                      'ide.delete.folderWarning',
                      'This will permanently delete all contents inside the folder.'
                    )}
                  </span>
                </div>
              )}

              {/* 受影响标签提示 */}
              {affectedTabCount > 0 && unsavedTabCount === 0 && (
                <div className="text-sm text-theme-text-muted">
                  {t('ide.delete.willCloseTabs', {
                    count: affectedTabCount,
                    defaultValue: '{{count}} open tab(s) will be closed.',
                  })}
                </div>
              )}

              {/* 未保存更改警告 */}
              {unsavedTabCount > 0 && (
                <div className="flex items-start gap-2 text-red-400 text-sm p-2 bg-red-500/10 rounded border border-red-500/20">
                  <AlertTriangle className="w-4 h-4 mt-0.5 flex-shrink-0" />
                  <span>
                    {t('ide.delete.hasUnsaved', {
                      count: unsavedTabCount,
                      defaultValue:
                        'Cannot delete: {{count}} file(s) have unsaved changes. Save or discard changes first.',
                    })}
                  </span>
                </div>
              )}
            </div>
          </DialogDescription>
        </DialogHeader>

        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={isDeleting}
          >
            {t('common.cancel', 'Cancel')}
          </Button>
          <Button
            variant="destructive"
            onClick={onConfirm}
            disabled={!canDelete || isDeleting}
            className="bg-red-600 hover:bg-red-700"
          >
            {isDeleting
              ? t('ide.delete.deleting', 'Deleting...')
              : t('ide.delete.confirm', 'Delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
