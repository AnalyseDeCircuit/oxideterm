// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

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

type PortableSetupDialogProps = {
  open: boolean;
  pending: boolean;
  errorMessage: string | null;
  onOpenChange: (open: boolean) => void;
  onSubmit: (password: string) => Promise<void>;
};

export function PortableSetupDialog({
  open,
  pending,
  errorMessage,
  onOpenChange,
  onSubmit,
}: PortableSetupDialogProps) {
  const { t } = useTranslation();
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [validationError, setValidationError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setPassword('');
      setConfirmPassword('');
      setValidationError(null);
    }
  }, [open]);

  const handleSubmit = async () => {
    if (password.length < 6) {
      setValidationError(t('common.portable_bootstrap.password_too_short'));
      return;
    }

    if (password !== confirmPassword) {
      setValidationError(t('common.portable_bootstrap.password_mismatch'));
      return;
    }

    setValidationError(null);
    await onSubmit(password);
  };

  return (
    <Dialog open={open} onOpenChange={pending ? undefined : onOpenChange}>
      <DialogContent className="sm:max-w-[440px] bg-theme-bg-elevated border-theme-border text-theme-text">
        <DialogHeader>
          <DialogTitle>{t('common.portable_bootstrap.setup_title')}</DialogTitle>
          <DialogDescription>
            {t('common.portable_bootstrap.setup_description')}
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
              <Label htmlFor="portable-setup-password">
                {t('common.portable_bootstrap.password_label')}
              </Label>
              <Input
                id="portable-setup-password"
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                placeholder={t('common.portable_bootstrap.password_placeholder')}
                disabled={pending}
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="portable-setup-confirm-password">
                {t('common.portable_bootstrap.confirm_password_label')}
              </Label>
              <Input
                id="portable-setup-confirm-password"
                type="password"
                value={confirmPassword}
                onChange={(event) => setConfirmPassword(event.target.value)}
                placeholder={t('common.portable_bootstrap.confirm_password_placeholder')}
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
            <Button type="submit" disabled={pending}>
              {pending
                ? t('common.portable_bootstrap.setup_pending')
                : t('common.portable_bootstrap.setup_submit')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}