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

type PortableUnlockDialogProps = {
  open: boolean;
  pending: boolean;
  errorMessage: string | null;
  onOpenChange: (open: boolean) => void;
  onSubmit: (password: string) => Promise<void>;
};

export function PortableUnlockDialog({
  open,
  pending,
  errorMessage,
  onOpenChange,
  onSubmit,
}: PortableUnlockDialogProps) {
  const { t } = useTranslation();
  const [password, setPassword] = useState('');

  useEffect(() => {
    if (!open) {
      setPassword('');
    }
  }, [open]);

  const handleSubmit = async () => {
    await onSubmit(password);
  };

  return (
    <Dialog open={open} onOpenChange={pending ? undefined : onOpenChange}>
      <DialogContent className="sm:max-w-[420px] bg-theme-bg-elevated border-theme-border text-theme-text">
        <DialogHeader>
          <DialogTitle>{t('common.portable_bootstrap.unlock_title')}</DialogTitle>
          <DialogDescription>
            {t('common.portable_bootstrap.unlock_description')}
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
              <Label htmlFor="portable-unlock-password">
                {t('common.portable_bootstrap.password_label')}
              </Label>
              <Input
                id="portable-unlock-password"
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                placeholder={t('common.portable_bootstrap.password_placeholder')}
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
                ? t('common.portable_bootstrap.unlock_pending')
                : t('common.portable_bootstrap.unlock_submit')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}