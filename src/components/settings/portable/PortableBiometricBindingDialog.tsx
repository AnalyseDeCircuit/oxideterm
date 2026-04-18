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

type PortableBiometricBindingDialogProps = {
  open: boolean;
  pending: boolean;
  errorMessage: string | null;
  onOpenChange: (open: boolean) => void;
  onSubmit: (password: string) => Promise<void>;
};

export function PortableBiometricBindingDialog({
  open,
  pending,
  errorMessage,
  onOpenChange,
  onSubmit,
}: PortableBiometricBindingDialogProps) {
  const { t } = useTranslation();
  const [password, setPassword] = useState('');

  useEffect(() => {
    if (!open) {
      setPassword('');
    }
  }, [open]);

  return (
    <Dialog open={open} onOpenChange={pending ? undefined : onOpenChange}>
      <DialogContent className="sm:max-w-[440px] bg-theme-bg-elevated border-theme-border text-theme-text">
        <DialogHeader>
          <DialogTitle>{t('settings_view.general.portable_enable_biometric_title')}</DialogTitle>
          <DialogDescription>
            {t('settings_view.general.portable_enable_biometric_description')}
          </DialogDescription>
        </DialogHeader>

        <form
          onSubmit={(event) => {
            event.preventDefault();
            void onSubmit(password);
          }}
        >
          <div className="space-y-4 py-2">
            <div className="space-y-2">
              <Label htmlFor="portable-biometric-password">
                {t('settings_view.general.portable_current_password')}
              </Label>
              <Input
                id="portable-biometric-password"
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                placeholder={t('portable_bootstrap.password_placeholder')}
                disabled={pending}
              />
            </div>

            {errorMessage && (
              <div className="rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-300" role="alert">
                {errorMessage}
              </div>
            )}
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)} disabled={pending}>
              {t('common.cancel')}
            </Button>
            <Button type="submit" disabled={pending || password.length === 0}>
              {pending
                ? t('settings_view.general.portable_enable_biometric_pending')
                : t('settings_view.general.portable_submit_enable_biometric')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}