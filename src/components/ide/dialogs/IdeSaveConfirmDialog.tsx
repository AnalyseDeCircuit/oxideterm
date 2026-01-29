// src/components/ide/dialogs/IdeSaveConfirmDialog.tsx
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

interface IdeSaveConfirmDialogProps {
  open: boolean;
  fileName: string;
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
}

export function IdeSaveConfirmDialog({
  open,
  fileName,
  onSave,
  onDiscard,
  onCancel,
}: IdeSaveConfirmDialogProps) {
  const { t } = useTranslation();

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onCancel()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('ide.unsaved_changes')}</DialogTitle>
          <DialogDescription>
            {t('ide.unsaved_changes_desc', { fileName })}
          </DialogDescription>
        </DialogHeader>
        
        <DialogFooter className="flex gap-2 sm:gap-0">
          <Button
            variant="outline"
            onClick={onCancel}
            className="flex-1 sm:flex-none"
          >
            {t('ide.cancel')}
          </Button>
          <Button
            variant="ghost"
            onClick={onDiscard}
            className="flex-1 sm:flex-none text-red-400 hover:text-red-300 hover:bg-red-500/10"
          >
            {t('ide.discard')}
          </Button>
          <Button
            onClick={onSave}
            className="flex-1 sm:flex-none bg-orange-600 hover:bg-orange-700"
          >
            {t('ide.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
