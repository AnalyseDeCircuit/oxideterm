// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { create } from 'zustand';
import {
  ragCreateCollection,
  ragListCollections,
  ragDeleteCollection,
  ragGetCollectionStats,
  ragAddDocument,
  ragRemoveDocument,
  ragListDocuments,
  ragGetPendingEmbeddings,
  ragStoreEmbeddings,
  ragSearch,
  ragReindexCollection,
  ragCancelReindex,
  ragGetDocumentContent,
  ragUpdateDocument,
  ragCreateBlankDocument,
  ragOpenDocumentExternal,
} from '@/lib/api';
import type {
  RagCollection,
  RagDocument,
  RagCollectionStats,
  RagPendingEmbedding,
  RagSearchResult,
} from '@/types';

// ═══════════════════════════════════════════════════════════════════════════
// Store Interface
// ═══════════════════════════════════════════════════════════════════════════

type RagStoreState = {
  collections: RagCollection[];
  selectedCollectionId: string | null;
  documents: RagDocument[];
  documentTotal: number;
  stats: RagCollectionStats | null;
  statsStale: boolean;
  searchResults: RagSearchResult[];
  isLoading: boolean;
  error: string | null;
  editingDocId: string | null;
  editFilePath: string | null;
  editingDocVersion: number | null;

  // Actions
  loadCollections: (scopeFilter?: string) => Promise<void>;
  createCollection: (name: string, scope: 'global' | { connectionId: string }) => Promise<RagCollection>;
  deleteCollection: (collectionId: string) => Promise<void>;
  selectCollection: (collectionId: string | null) => Promise<void>;
  loadMoreDocuments: (offset: number, limit: number) => Promise<void>;
  addDocument: (collectionId: string, title: string, content: string, format: string, sourcePath?: string) => Promise<RagDocument>;
  removeDocument: (docId: string) => Promise<void>;
  search: (query: string, collectionIds: string[], queryVector?: number[], topK?: number) => Promise<RagSearchResult[]>;
  getPendingEmbeddings: (collectionId: string, limit?: number) => Promise<RagPendingEmbedding[]>;
  storeEmbeddings: (embeddings: Array<{ chunkId: string; vector: number[] }>, modelName: string) => Promise<number>;
  reindexCollection: (collectionId: string) => Promise<number>;
  cancelReindex: () => Promise<void>;
  createBlankDocument: (collectionId: string, title: string, format: string) => Promise<RagDocument>;
  openDocumentExternal: (docId: string) => Promise<string>;
  syncExternalEdits: () => Promise<{ updated: boolean; docId: string } | null>;
  clearEditing: () => void;
  clearError: () => void;
};

export const RAG_SYNC_VERSION_CONFLICT_ERROR = 'RAG_SYNC_VERSION_CONFLICT';

// ═══════════════════════════════════════════════════════════════════════════
// Store Implementation
// ═══════════════════════════════════════════════════════════════════════════

export const useRagStore = create<RagStoreState>()((set, get) => ({
  collections: [],
  selectedCollectionId: null,
  documents: [],
  documentTotal: 0,
  stats: null,
  statsStale: false,
  searchResults: [],
  isLoading: false,
  error: null,
  editingDocId: null,
  editFilePath: null,
  editingDocVersion: null,

  loadCollections: async (scopeFilter?: string) => {
    set({ isLoading: true, error: null });
    try {
      const collections = await ragListCollections(scopeFilter);
      set({ collections, isLoading: false });
    } catch (e) {
      set({ error: String(e), isLoading: false });
    }
  },

  createCollection: async (name, scope) => {
    const scopeDto = scope === 'global'
      ? 'Global' as const
      : { Connection: { connection_id: scope.connectionId } };
    const collection = await ragCreateCollection(name, scopeDto);
    set((s) => ({ collections: [...s.collections, collection] }));
    return collection;
  },

  deleteCollection: async (collectionId) => {
    await ragDeleteCollection(collectionId);
    set((s) => ({
      collections: s.collections.filter((c) => c.id !== collectionId),
      selectedCollectionId: s.selectedCollectionId === collectionId ? null : s.selectedCollectionId,
      documents: s.selectedCollectionId === collectionId ? [] : s.documents,
      documentTotal: s.selectedCollectionId === collectionId ? 0 : s.documentTotal,
      stats: s.selectedCollectionId === collectionId ? null : s.stats,
    }));
  },

  selectCollection: async (collectionId) => {
    set({ selectedCollectionId: collectionId, documents: [], documentTotal: 0, stats: null });
    if (!collectionId) return;
    try {
      set({ isLoading: true });
      const [result, stats] = await Promise.all([
        ragListDocuments(collectionId),
        ragGetCollectionStats(collectionId),
      ]);
      set({ documents: result.documents, documentTotal: result.total, stats, isLoading: false, statsStale: false });
    } catch (e) {
      set({ error: String(e), isLoading: false, statsStale: true });
    }
  },

  loadMoreDocuments: async (offset, limit) => {
    const { selectedCollectionId, documentTotal } = get();
    if (!selectedCollectionId || offset >= documentTotal) return;
    const result = await ragListDocuments(selectedCollectionId, offset, limit);
    set((s) => ({
      documents: [...s.documents, ...result.documents],
      documentTotal: result.total,
    }));
  },

  addDocument: async (collectionId, title, content, format, sourcePath) => {
    const doc = await ragAddDocument({ collectionId, title, content, format, sourcePath });
    set((s) => ({
      documents: s.selectedCollectionId === collectionId ? [...s.documents, doc] : s.documents,
      documentTotal: s.selectedCollectionId === collectionId ? s.documentTotal + 1 : s.documentTotal,
    }));
    // Refresh stats
    try {
      const stats = await ragGetCollectionStats(collectionId);
      set({ stats, statsStale: false });
    } catch {
      set({ statsStale: true });
    }
    return doc;
  },

  removeDocument: async (docId) => {
    // Clean up temp file if this doc is currently being edited
    const { editingDocId, editFilePath } = get();
    if (editingDocId === docId && editFilePath) {
      try {
        const { remove } = await import('@tauri-apps/plugin-fs');
        await remove(editFilePath);
      } catch { /* best-effort */ }
      set({ editingDocId: null, editFilePath: null, editingDocVersion: null });
    }
    await ragRemoveDocument(docId);
    set((s) => ({
      documents: s.documents.filter((d) => d.id !== docId),
      documentTotal: s.documents.some((d) => d.id === docId) ? Math.max(0, s.documentTotal - 1) : s.documentTotal,
    }));
    // Refresh stats
    const { selectedCollectionId } = get();
    if (selectedCollectionId) {
      try {
        const stats = await ragGetCollectionStats(selectedCollectionId);
        set({ stats, statsStale: false });
      } catch {
        set({ statsStale: true });
      }
    }
  },

  search: async (query, collectionIds, queryVector, topK) => {
    const results = await ragSearch({ query, collectionIds, queryVector, topK });
    set({ searchResults: results });
    return results;
  },

  getPendingEmbeddings: (collectionId, limit) =>
    ragGetPendingEmbeddings(collectionId, limit),

  storeEmbeddings: (embeddings, modelName) =>
    ragStoreEmbeddings(embeddings, modelName),

  reindexCollection: (collectionId) =>
    ragReindexCollection(collectionId),

  cancelReindex: () => ragCancelReindex(),

  createBlankDocument: async (collectionId, title, format) => {
    const doc = await ragCreateBlankDocument({ collectionId, title, format });
    set((s) => ({
      documents: s.selectedCollectionId === collectionId ? [...s.documents, doc] : s.documents,
      documentTotal: s.selectedCollectionId === collectionId ? s.documentTotal + 1 : s.documentTotal,
    }));
    try {
      const stats = await ragGetCollectionStats(collectionId);
      set({ stats, statsStale: false });
    } catch {
      set({ statsStale: true });
    }
    return doc;
  },

  openDocumentExternal: async (docId) => {
    const filePath = await ragOpenDocumentExternal(docId);
    const currentDoc = get().documents.find((doc) => doc.id === docId);
    set({ editingDocId: docId, editFilePath: filePath, editingDocVersion: currentDoc?.version ?? null });
    return filePath;
  },

  syncExternalEdits: async () => {
    const { editingDocId, editFilePath, editingDocVersion, selectedCollectionId } = get();
    if (!editingDocId || !editFilePath) return null;

    const { readTextFile, remove } = await import('@tauri-apps/plugin-fs');
    let fileContent: string;
    try {
      fileContent = await readTextFile(editFilePath);
    } catch (error) {
      try { await remove(editFilePath); } catch { /* best-effort */ }
      set({ editingDocId: null, editFilePath: null, editingDocVersion: null });
      throw error;
    }
    const storedContent = await ragGetDocumentContent(editingDocId);

    if (fileContent === storedContent) {
      // Clean up temp file
      try { await remove(editFilePath); } catch { /* best-effort */ }
      set({ editingDocId: null, editFilePath: null, editingDocVersion: null });
      return { updated: false, docId: editingDocId };
    }

    // Look up current version for optimistic locking
    const currentDoc = get().documents.find((d) => d.id === editingDocId);
    let updatedDoc;
    try {
      updatedDoc = await ragUpdateDocument(
        editingDocId,
        fileContent,
        currentDoc?.version ?? editingDocVersion ?? undefined,
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (message.includes('Version conflict')) {
        set({ editingDocId: null, editFilePath: null, editingDocVersion: null });
        throw new Error(RAG_SYNC_VERSION_CONFLICT_ERROR);
      }
      throw error;
    }
    // Clean up temp file after successful sync
    try { await remove(editFilePath); } catch { /* best-effort */ }
    set((s) => ({
      documents: s.documents.map((d) => d.id === updatedDoc.id ? updatedDoc : d),
      editingDocId: null,
      editFilePath: null,
      editingDocVersion: null,
    }));

    // Refresh stats
    if (selectedCollectionId) {
      try {
        const stats = await ragGetCollectionStats(selectedCollectionId);
        set({ stats, statsStale: false });
      } catch {
        set({ statsStale: true });
      }
    }

    return { updated: true, docId: editingDocId };
  },

  clearEditing: () => set({ editingDocId: null, editFilePath: null, editingDocVersion: null }),

  clearError: () => set({ error: null }),
}));
