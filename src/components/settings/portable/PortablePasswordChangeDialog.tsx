import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

type PortablePasswordChangeDialogProps = {
  open: boolean;
  pending: boolean;
  errorMessage: string | null;
  onOpenChange: (open: boolean) => void;
  onSubmit: (currentPassword: string, newPassword: string) => Promise<void>;
};

export function PortablePasswordChangeDialog({
  open,
  pending,
  errorMessage,
  onOpenChange,
  onSubmit,
}: PortablePasswordChangeDialogProps) {
  const { t } = useTranslation();
  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [validationError, setValidationError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setCurrentPassword('');
      setNewPassword('');
      setConfirmPassword('');
      setValidationError(null);
    }
  }, [open]);

  const handleSubmit = async () => {
    if (newPassword.length < 6) {
      setValidationError(t('settings_view.general.portable_password_too_short'));
      return;
    }

    if (newPassword !== confirmPassword) {
      setValidationError(t('settings_view.general.portable_password_mismatch'));
      return;
    }

    setValidationError(null);
    await onSubmit(currentPassword, newPassword);
  };

  return (
    <Dialog open={open} onOpenChange={pending ? undefined : onOpenChange}>
      <DialogContent className="sm:max-w-[460px] bg-theme-bg-elevated border-theme-border text-theme-text">
        <DialogHeader>
          <DialogTitle>{t('settings_view.general.portable_change_password_title')}</DialogTitle>
          <DialogDescription>
            {t('settings_view.general.portable_change_password_description')}
          </DialogDescription>
        </DialogHeader>

        <form
          onSubmit={(event) => {
            event.preventDefault();
            void handleSubmit();
          }}
        >
          <div className="space-y-4 py-2">
            <div className="space-y-2">
              <Label htmlFor="portable-current-password">
                {t('settings_view.general.portable_current_password')}
              </Label>
              <Input
                id="portable-current-password"
                type="password"
                value={currentPassword}
                onChange={(event) => setCurrentPassword(event.target.value)}
                placeholder={t('portable_bootstrap.password_placeholder')}
                disabled={pending}
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="portable-new-password">
                {t('settings_view.general.portable_new_password')}
              </Label>
              <Input
                id="portable-new-password"
                type="password"
                value={newPassword}
                onChange={(event) => setNewPassword(event.target.value)}
                placeholder={t('portable_bootstrap.password_placeholder')}
                disabled={pending}
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="portable-confirm-password">
                {t('settings_view.general.portable_confirm_password')}
              </Label>
              <Input
                id="portable-confirm-password"
                type="password"
                value={confirmPassword}
                onChange={(event) => setConfirmPassword(event.target.value)}
                placeholder={t('portable_bootstrap.confirm_password_placeholder')}
                disabled={pending}
              />
            </div>

            {(validationError || errorMessage) && (
              <div className="rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-300" role="alert">
                {validationError || errorMessage}
              </div>
            )}
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)} disabled={pending}>
              {t('common.cancel')}
            </Button>
            <Button type="submit" disabled={pending || currentPassword.length === 0}>
              {pending
                ? t('settings_view.general.portable_change_password_pending')
                : t('settings_view.general.portable_submit_change_password')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}