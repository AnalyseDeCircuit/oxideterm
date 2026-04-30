// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import {
  Toast,
  ToastAction,
  ToastClose,
  ToastDescription,
  ToastProvider,
  ToastTitle,
  ToastViewport,
} from './toast';
import { useToastStore } from '../../hooks/useToast';

export const Toaster = () => {
  const { toasts, removeToast } = useToastStore();

  return (
    <ToastProvider>
      {toasts.map((toast) => (
        <Toast
          key={toast.id}
          variant={toast.variant}
          onOpenChange={(open) => {
            if (!open) removeToast(toast.id);
          }}
        >
          <div className="grid min-w-0 flex-1 gap-1">
            <ToastTitle>{toast.title}</ToastTitle>
            {toast.description && (
              <ToastDescription>{toast.description}</ToastDescription>
            )}
            {toast.statusText && (
              <ToastDescription>{toast.statusText}</ToastDescription>
            )}
            {typeof toast.progress === 'number' && (
              <div className="mt-2 h-1 w-full overflow-hidden rounded-full bg-theme-border/50">
                <div
                  className="h-full rounded-full bg-theme-accent transition-[width] duration-200"
                  style={{ width: `${Math.max(0, Math.min(100, toast.progress))}%` }}
                />
              </div>
            )}
          </div>
          {toast.actions && toast.actions.length > 0 && (
            <div className="flex shrink-0 gap-2">
              {toast.actions.map((action) => (
                <ToastAction
                  key={action.label}
                  altText={action.label}
                  onClick={() => {
                    action.onClick();
                    removeToast(toast.id);
                  }}
                >
                  {action.label}
                </ToastAction>
              ))}
            </div>
          )}
          <ToastClose />
        </Toast>
      ))}
      <ToastViewport />
    </ToastProvider>
  );
};
