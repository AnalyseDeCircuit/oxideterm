import { create } from 'zustand';
import { api } from '../lib/api';
import i18n from '../i18n';

export type TransferState = 'pending' | 'active' | 'paused' | 'completed' | 'cancelled' | 'error';
export type TransferDirection = 'upload' | 'download';

export interface TransferItem {
  id: string;
  nodeId: string;
  name: string;
  localPath: string;
  remotePath: string;
  direction: TransferDirection;
  size: number;           // Total bytes
  transferred: number;    // Bytes transferred
  state: TransferState;
  error?: string;
  startTime: number;      // Unix timestamp ms
  endTime?: number;       // Unix timestamp ms
}

interface TransferStore {
  // State
  transfers: Map<string, TransferItem>;
  
  // Actions
  addTransfer: (transfer: Omit<TransferItem, 'transferred' | 'state' | 'startTime'>) => string;
  updateProgress: (id: string, transferred: number, total: number) => void;
  setTransferState: (id: string, state: TransferState, error?: string) => void;
  removeTransfer: (id: string) => void;
  clearCompleted: () => void;
  pauseAll: () => void;
  resumeAll: () => void;
  pauseTransfer: (id: string) => Promise<void>;
  resumeTransfer: (id: string) => Promise<void>;
  cancelTransfer: (id: string) => Promise<void>;
  interruptTransfersByNode: (nodeId: string, errorMessage?: string) => void;
  
  // Computed helpers
  getTransfersByNode: (nodeId: string) => TransferItem[];
  getActiveTransfers: () => TransferItem[];
  getAllTransfers: () => TransferItem[];
}

export const useTransferStore = create<TransferStore>((set, get) => ({
  transfers: new Map(),
  
  addTransfer: (transfer) => {
    const id = transfer.id || `transfer-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    const newTransfer: TransferItem = {
      ...transfer,
      id,
      transferred: 0,
      state: 'pending',
      startTime: Date.now(),
    };
    
    set((state) => {
      const newTransfers = new Map(state.transfers);
      newTransfers.set(id, newTransfer);
      return { transfers: newTransfers };
    });
    
    return id;
  },
  
  updateProgress: (id, transferred, total) => {
    set((state) => {
      const transfer = state.transfers.get(id);
      if (!transfer) return state;
      
      const newTransfers = new Map(state.transfers);
      
      // Determine new state: preserve 'paused' state, otherwise set to completed/active
      let newState: TransferState;
      if (transfer.state === 'paused') {
        newState = 'paused'; // Keep paused state
      } else if (transferred >= total) {
        newState = 'completed';
      } else {
        newState = 'active';
      }
      
      newTransfers.set(id, {
        ...transfer,
        transferred,
        size: total,
        state: newState,
        endTime: newState === 'completed' ? Date.now() : undefined,
      });
      return { transfers: newTransfers };
    });
  },
  
  setTransferState: (id, newState, error) => {
    set((state) => {
      const transfer = state.transfers.get(id);
      if (!transfer) return state;
      
      const newTransfers = new Map(state.transfers);
      newTransfers.set(id, {
        ...transfer,
        state: newState,
        error,
        endTime: (newState === 'completed' || newState === 'error') ? Date.now() : transfer.endTime,
      });

      // Auto-cleanup: remove completed/cancelled transfers older than 5 minutes
      // or when there are more than 100 finished items
      const AUTO_CLEANUP_MS = 5 * 60 * 1000;
      const MAX_FINISHED = 100;
      const now = Date.now();
      let finishedCount = 0;
      for (const [, t] of newTransfers) {
        if ((t.state === 'completed' || t.state === 'cancelled') && t.endTime) finishedCount++;
      }
      if (finishedCount > MAX_FINISHED) {
        for (const [tid, t] of newTransfers) {
          if ((t.state === 'completed' || t.state === 'cancelled') && t.endTime && (now - t.endTime > AUTO_CLEANUP_MS)) {
            newTransfers.delete(tid);
          }
        }
      }

      return { transfers: newTransfers };
    });
  },
  
  removeTransfer: (id) => {
    set((state) => {
      const newTransfers = new Map(state.transfers);
      newTransfers.delete(id);
      return { transfers: newTransfers };
    });
  },
  
  clearCompleted: () => {
    set((state) => {
      const newTransfers = new Map(state.transfers);
      for (const [id, transfer] of newTransfers) {
        if (transfer.state === 'completed' || transfer.state === 'cancelled') {
          newTransfers.delete(id);
        }
      }
      return { transfers: newTransfers };
    });
  },
  
  pauseAll: () => {
    set((state) => {
      const newTransfers = new Map(state.transfers);
      for (const [id, transfer] of newTransfers) {
        if (transfer.state === 'active' || transfer.state === 'pending') {
          newTransfers.set(id, { ...transfer, state: 'paused' });
        }
      }
      return { transfers: newTransfers };
    });
  },
  
  resumeAll: () => {
    set((state) => {
      const newTransfers = new Map(state.transfers);
      for (const [id, transfer] of newTransfers) {
        if (transfer.state === 'paused') {
          newTransfers.set(id, { ...transfer, state: 'pending' });
        }
      }
      return { transfers: newTransfers };
    });
  },
  
  pauseTransfer: async (id) => {
    const state = get();
    const transfer = state.transfers.get(id);
    if (!transfer || (transfer.state !== 'active' && transfer.state !== 'pending')) return;
    
    try {
      // 调用后端 API 实际暂停传输
      await api.sftpPauseTransfer(id);
      
      set((state) => {
        const newTransfers = new Map(state.transfers);
        const t = newTransfers.get(id);
        if (t) {
          newTransfers.set(id, { ...t, state: 'paused' });
        }
        return { transfers: newTransfers };
      });
    } catch (e) {
      console.error('Failed to pause transfer:', e);
    }
  },
  
  resumeTransfer: async (id) => {
    const state = get();
    const transfer = state.transfers.get(id);
    if (!transfer || transfer.state !== 'paused') return;
    
    try {
      // 调用后端 API 实际恢复传输
      await api.sftpResumeTransfer(id);
      
      set((state) => {
        const newTransfers = new Map(state.transfers);
        const t = newTransfers.get(id);
        if (t) {
          newTransfers.set(id, { ...t, state: 'pending' });
        }
        return { transfers: newTransfers };
      });
    } catch (e) {
      console.error('Failed to resume transfer:', e);
    }
  },
  
  cancelTransfer: async (id) => {
    // Call backend API first to actually cancel the transfer
    try {
      await api.sftpCancelTransfer(id);
    } catch (e) {
      console.error('Failed to cancel transfer on backend:', e);
      // Continue to update UI state even if backend call fails
    }
    
    set((state) => {
      const transfer = state.transfers.get(id);
      if (!transfer) return state;
      
      const newTransfers = new Map(state.transfers);
      newTransfers.set(id, { 
        ...transfer, 
        state: 'cancelled',
        endTime: Date.now(),
      });
      return { transfers: newTransfers };
    });
  },
  
  // Mark all active/pending transfers for a node as interrupted (error state)
  // Used when connection is lost - preserves transferred bytes for resume
  interruptTransfersByNode: (nodeId, errorMessage) => {
    // Use i18n.t() for default message (lazy translation)
    const message = errorMessage ?? i18n.t('sftp.errors.connection_lost');
    set((state) => {
      const newTransfers = new Map(state.transfers);
      let interrupted = 0;
      
      for (const [id, transfer] of newTransfers) {
        if (transfer.nodeId === nodeId && 
            (transfer.state === 'active' || transfer.state === 'pending')) {
          newTransfers.set(id, {
            ...transfer,
            state: 'error',
            error: message,
            // Keep transferred bytes for resume capability
          });
          interrupted++;
        }
      }
      
      if (interrupted > 0) {
        console.log(`[TransferStore] Interrupted ${interrupted} transfers for node ${nodeId}`);
      }
      
      return { transfers: newTransfers };
    });
  },
  
  getTransfersByNode: (nodeId) => {
    return Array.from(get().transfers.values()).filter(t => t.nodeId === nodeId);
  },
  
  getActiveTransfers: () => {
    return Array.from(get().transfers.values()).filter(t => 
      t.state === 'active' || t.state === 'pending'
    );
  },
  
  getAllTransfers: () => {
    return Array.from(get().transfers.values());
  },
}));

// Helper function to format bytes
export const formatBytes = (bytes: number): string => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
};

// Helper function to format transfer speed
export const formatSpeed = (bytesPerSecond: number): string => {
  return `${formatBytes(bytesPerSecond)}/s`;
};

// Helper function to calculate speed from transfer
export const calculateSpeed = (transfer: TransferItem): number => {
  if (transfer.state !== 'active' || transfer.transferred === 0) return 0;
  const elapsed = (Date.now() - transfer.startTime) / 1000; // seconds
  if (elapsed <= 0) return 0;
  return transfer.transferred / elapsed;
};
