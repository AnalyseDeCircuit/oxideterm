// src/components/ide/dialogs/IdeConflictDialog.tsx
import { useTranslation } from 'react-i18next';
import { AlertTriangle } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';
import { Button } from '../../ui/button';

export type ConflictResolution = 'overwrite' | 'reload' | 'cancel';

interface IdeConflictDialogProps {
  open: boolean;
  fileName: string;
  localTime: Date;
  remoteTime: Date;
  onResolve: (resolution: ConflictResolution) => void;
}

export function IdeConflictDialog({
  open,
  fileName,
  localTime,
  remoteTime,
  onResolve,
}: IdeConflictDialogProps) {
  const { t } = useTranslation();

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onResolve('cancel')}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <div className="flex items-center gap-2">
            <AlertTriangle className="w-5 h-5 text-yellow-500" />
            <DialogTitle>{t('ide.conflict_title')}</DialogTitle>
          </div>
          <DialogDescription className="pt-2">
            {t('ide.conflict_desc', { fileName })}
          </DialogDescription>
        </DialogHeader>
        
        {/* 时间对比 */}
        <div className="space-y-2 text-sm">
          <div className="flex justify-between items-center px-3 py-2 bg-theme-bg-hover/50 rounded">
            <span className="text-theme-text-muted">{t('ide.your_version')}</span>
            <span className="text-theme-text font-mono text-xs">
              {localTime.toLocaleString()}
            </span>
          </div>
          <div className="flex justify-between items-center px-3 py-2 bg-theme-bg-hover/50 rounded">
            <span className="text-theme-text-muted">{t('ide.remote_version')}</span>
            <span className="text-theme-accent font-mono text-xs">
              {remoteTime.toLocaleString()}
            </span>
          </div>
        </div>
        
        <DialogFooter className="flex gap-2 sm:gap-0 pt-2">
          <Button
            variant="outline"
            onClick={() => onResolve('cancel')}
            className="flex-1 sm:flex-none"
          >
            {t('ide.cancel')}
          </Button>
          <Button
            variant="ghost"
            onClick={() => onResolve('reload')}
            className="flex-1 sm:flex-none text-blue-400 hover:text-blue-300 hover:bg-blue-500/10"
          >
            {t('ide.reload_remote')}
          </Button>
          <Button
            onClick={() => onResolve('overwrite')}
            className="flex-1 sm:flex-none bg-orange-600 hover:bg-orange-700"
          >
            {t('ide.overwrite')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
