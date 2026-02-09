/**
 * useConfirm hook — async confirmation dialog replacement for window.confirm()
 *
 * window.confirm() does not reliably block JS execution in Tauri WebView,
 * causing actions to fire before the user responds. This hook provides a
 * React-native async alternative that renders a themed Dialog and returns
 * a Promise<boolean>.
 *
 * Usage:
 *   const { confirm, ConfirmDialog } = useConfirm();
 *   
 *   const handleDelete = async () => {
 *     if (await confirm({ title: 'Delete?', description: 'This cannot be undone.' })) {
 *       // user confirmed
 *     }
 *   };
 *   
 *   return <>{ConfirmDialog}</>
 */

import { useState, useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, HelpCircle } from 'lucide-react';
import {
  Dialog,
  DialogContent,
} from '../components/ui/dialog';

interface ConfirmOptions {
  title: string;
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: 'default' | 'danger';
}

export function useConfirm() {
  const { t } = useTranslation();
  const [options, setOptions] = useState<ConfirmOptions | null>(null);
  const resolveRef = useRef<((value: boolean) => void) | null>(null);

  const confirm = useCallback((opts: ConfirmOptions): Promise<boolean> => {
    return new Promise<boolean>((resolve) => {
      resolveRef.current = resolve;
      setOptions(opts);
    });
  }, []);

  const handleConfirm = useCallback(() => {
    resolveRef.current?.(true);
    resolveRef.current = null;
    setOptions(null);
  }, []);

  const handleCancel = useCallback(() => {
    resolveRef.current?.(false);
    resolveRef.current = null;
    setOptions(null);
  }, []);

  const isDanger = options?.variant === 'danger';

  const ConfirmDialog = options ? (
    <Dialog open={true} onOpenChange={(open) => { if (!open) handleCancel(); }}>
      <DialogContent
        className="max-w-sm p-0 overflow-hidden rounded-lg border border-theme-border/60 shadow-2xl shadow-black/40"
        // Hide the default close button — we have Cancel
        onPointerDownOutside={(e) => e.preventDefault()}
      >
        {/* Body */}
        <div className="flex flex-col items-center gap-3 px-6 pt-6 pb-4">
          {/* Icon */}
          <div className={
            isDanger
              ? "flex items-center justify-center w-12 h-12 rounded-full bg-red-500/10 ring-1 ring-red-500/20"
              : "flex items-center justify-center w-12 h-12 rounded-full bg-theme-accent/10 ring-1 ring-theme-accent/20"
          }>
            {isDanger
              ? <AlertTriangle className="w-6 h-6 text-red-400" />
              : <HelpCircle className="w-6 h-6 text-theme-accent" />
            }
          </div>

          {/* Title */}
          <h3 className="text-sm font-semibold text-theme-text text-center leading-snug">
            {options.title}
          </h3>

          {/* Description */}
          {options.description && (
            <p className="text-xs text-theme-text-muted text-center leading-relaxed">
              {options.description}
            </p>
          )}
        </div>

        {/* Actions */}
        <div className="flex border-t border-theme-border/40">
          <button
            onClick={handleCancel}
            className="flex-1 py-2.5 text-sm font-medium text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-panel/50 transition-colors border-r border-theme-border/40"
          >
            {options.cancelLabel || t('common.actions.cancel', 'Cancel')}
          </button>
          <button
            onClick={handleConfirm}
            className={
              isDanger
                ? "flex-1 py-2.5 text-sm font-semibold text-red-400 hover:text-red-300 hover:bg-red-500/10 transition-colors"
                : "flex-1 py-2.5 text-sm font-semibold text-theme-accent hover:text-theme-accent/80 hover:bg-theme-accent/10 transition-colors"
            }
          >
            {options.confirmLabel || t('common.actions.confirm', 'Confirm')}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  ) : null;

  return { confirm, ConfirmDialog };
}
