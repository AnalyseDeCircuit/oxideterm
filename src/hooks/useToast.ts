// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { create } from 'zustand';

export type ToastVariant = 'default' | 'success' | 'error' | 'warning';

export interface ToastData {
  id: string;
  title: string;
  description?: string;
  variant?: ToastVariant;
  duration?: number;
  progress?: number;
  statusText?: string;
  persistent?: boolean;
  actions?: ToastActionData[];
}

export interface ToastActionData {
  label: string;
  onClick: () => void;
}

interface ToastStore {
  toasts: ToastData[];
  addToast: (toast: Omit<ToastData, 'id'>) => string;
  updateToast: (id: string, patch: Partial<Omit<ToastData, 'id'>>) => void;
  upsertToast: (id: string, toast: Omit<ToastData, 'id'>) => string;
  removeToast: (id: string) => void;
  clearToasts: () => void;
}

const toastTimers = new Map<string, ReturnType<typeof setTimeout>>();

function getDefaultDuration(toast: Pick<ToastData, 'variant'>): number {
  return toast.variant === 'error' ? 8000 : toast.variant === 'warning' ? 7000 : 5000;
}

function clearToastTimer(id: string) {
  const timer = toastTimers.get(id);
  if (timer) {
    clearTimeout(timer);
    toastTimers.delete(id);
  }
}

function scheduleToastTimer(
  id: string,
  toast: ToastData,
  removeToast: (id: string) => void,
) {
  clearToastTimer(id);
  if (toast.persistent) return;
  if (toast.duration && toast.duration > 0) {
    toastTimers.set(
      id,
      setTimeout(() => {
        toastTimers.delete(id);
        removeToast(id);
      }, toast.duration),
    );
  }
}

export const useToastStore = create<ToastStore>((set, get) => ({
  toasts: [],
  
  addToast: (toast) => {
    const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    get().upsertToast(id, toast);
    return id;
  },

  updateToast: (id, patch) => {
    let updatedToast: ToastData | null = null;
    set((state) => ({
      toasts: state.toasts.map((toast) => {
        if (toast.id !== id) return toast;
        updatedToast = {
          ...toast,
          ...patch,
          duration: patch.duration ?? toast.duration,
        };
        return updatedToast;
      }),
    }));
    if (updatedToast) {
      scheduleToastTimer(id, updatedToast, get().removeToast);
    }
  },

  upsertToast: (id, toast) => {
    const newToast: ToastData = {
      ...toast,
      id,
      duration: toast.duration ?? getDefaultDuration(toast),
    };

    set((state) => {
      const existingIndex = state.toasts.findIndex((item) => item.id === id);
      if (existingIndex === -1) {
        return { toasts: [...state.toasts, newToast] };
      }
      return {
        toasts: state.toasts.map((item) => (item.id === id ? { ...item, ...newToast } : item)),
      };
    });

    scheduleToastTimer(id, newToast, get().removeToast);
    return id;
  },
  
  removeToast: (id) => {
    clearToastTimer(id);
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    }));
  },
  
  clearToasts: () => {
    for (const id of toastTimers.keys()) {
      clearToastTimer(id);
    }
    set({ toasts: [] });
  },
}));

// Convenience hook for components
export const useToast = () => {
  const { addToast, updateToast, upsertToast, removeToast, clearToasts } = useToastStore();
  
  return {
    toast: (toast: Omit<ToastData, 'id'>) => addToast(toast),
    success: (title: string, description?: string) => 
      addToast({ title, description, variant: 'success' }),
    error: (title: string, description?: string) => 
      addToast({ title, description, variant: 'error' }),
    warning: (title: string, description?: string) => 
      addToast({ title, description, variant: 'warning' }),
    update: updateToast,
    upsert: upsertToast,
    dismiss: removeToast,
    clear: clearToasts,
  };
};
