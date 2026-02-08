/**
 * Plugin Confirm Dialog
 *
 * A themed confirmation dialog for plugins, replacing window.confirm().
 * Uses the event bridge pattern: plugins emit a confirm request, this
 * component renders it, and resolves the Promise.
 */

import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { pluginEventBridge } from '../../lib/plugin/pluginEventBridge';

interface ConfirmRequest {
  title: string;
  description: string;
  resolve: (result: boolean) => void;
}

export function PluginConfirmDialog() {
  const { t } = useTranslation();
  const [request, setRequest] = useState<ConfirmRequest | null>(null);

  useEffect(() => {
    const cleanup = pluginEventBridge.on('plugin:confirm', (data) => {
      const req = data as ConfirmRequest;
      setRequest(req);
    });
    return cleanup;
  }, []);

  const handleConfirm = useCallback(() => {
    request?.resolve(true);
    setRequest(null);
  }, [request]);

  const handleCancel = useCallback(() => {
    request?.resolve(false);
    setRequest(null);
  }, [request]);

  if (!request) return null;

  return (
    <Dialog open={true} onOpenChange={(open) => { if (!open) handleCancel(); }}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{request.title}</DialogTitle>
          <DialogDescription>{request.description}</DialogDescription>
        </DialogHeader>
        <DialogFooter className="flex gap-2 sm:gap-0">
          <Button
            variant="outline"
            onClick={handleCancel}
            className="flex-1 sm:flex-none"
          >
            {t('common.cancel', 'Cancel')}
          </Button>
          <Button
            onClick={handleConfirm}
            className="flex-1 sm:flex-none bg-theme-accent hover:bg-theme-accent/80"
          >
            {t('common.confirm', 'Confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
