// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * RAG Document Manager — Settings Panel Tab
 *
 * Manages knowledge base collections and documents for AI-assisted retrieval.
 * Users can create collections, import Markdown/TXT files, generate embeddings,
 * and rebuild BM25 indexes.
 */

import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import { readTextFile, stat } from '@tauri-apps/plugin-fs';
import { listen } from '@tauri-apps/api/event';
import { useRagStore } from '../../store/ragStore';
import { RAG_SYNC_VERSION_CONFLICT_ERROR } from '../../store/ragStore';
import { useSettingsStore } from '../../store/settingsStore';
import { getProvider } from '../../lib/ai/providerRegistry';
import { useToast } from '../../hooks/useToast';
import { api } from '../../lib/api';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Separator } from '../ui/separator';
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogHeader,
  DialogFooter,
} from '../ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../ui/select';
import {
  Plus,
  Trash2,
  FileText,
  FolderOpen,
  RefreshCw,
  Loader2,
  BookOpen,
  Sparkles,
  Pencil,
  FilePlus,
  X,
} from 'lucide-react';
import type { RagCollection, RagDocument, RagCollectionStats } from '../../types';

/** Max file size for import (5 MB) */
const MAX_IMPORT_FILE_SIZE = 5 * 1024 * 1024;

// ═══════════════════════════════════════════════════════════════════════════
// Helper: Format timestamps
// ═══════════════════════════════════════════════════════════════════════════

function formatDate(ts: number): string {
  if (!ts) return '—';
  return new Date(ts).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

function scopeLabel(scope: RagCollection['scope'], t: (key: string) => string): string {
  if (scope === 'Global') return t('settings_view.knowledge.scope_global');
  return t('settings_view.knowledge.scope_connection');
}

// ═══════════════════════════════════════════════════════════════════════════
// Main Component
// ═══════════════════════════════════════════════════════════════════════════

export function DocumentManager() {
  const { t } = useTranslation();
  const { error: toastError, success: toastSuccess } = useToast();
  const {
    collections,
    selectedCollectionId,
    documents,
    stats,
    statsStale,
    isLoading,
    error,
    editingDocId,
    loadCollections,
    createCollection,
    deleteCollection,
    selectCollection,
    addDocument,
    removeDocument,
    reindexCollection,
    cancelReindex,
    getPendingEmbeddings,
    storeEmbeddings,
    createBlankDocument,
    openDocumentExternal,
    syncExternalEdits,
    clearError,
  } = useRagStore();

  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [newCollectionName, setNewCollectionName] = useState('');
  const [newCollectionScope, setNewCollectionScope] = useState<'global'>('global');
  const [importProgress, setImportProgress] = useState<{ current: number; total: number } | null>(null);
  const [embeddingProgress, setEmbeddingProgress] = useState<{ current: number; total: number } | null>(null);
  const [reindexProgress, setReindexProgress] = useState<{ current: number; total: number } | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<{ type: 'collection' | 'document'; id: string; name: string } | null>(null);
  const [newDocDialogOpen, setNewDocDialogOpen] = useState(false);
  const [newDocTitle, setNewDocTitle] = useState('');
  const [newDocFormat, setNewDocFormat] = useState<'markdown' | 'plaintext'>('markdown');

  // Load collections on mount
  useEffect(() => {
    loadCollections();
  }, [loadCollections]);

  // Listen for reindex progress events from backend
  useEffect(() => {
    let mounted = true;
    const unlisten = listen<{ current: number; total: number }>('rag_reindex_progress', (e) => {
      if (mounted) {
        setReindexProgress(e.payload);
      }
    });
    return () => {
      mounted = false;
      unlisten.then((fn) => fn());
    };
  }, []);

  // Auto-dismiss error
  useEffect(() => {
    if (error) {
      const timer = setTimeout(clearError, 5000);
      return () => clearTimeout(timer);
    }
  }, [error, clearError]);

  // Sync external edits on window focus
  useEffect(() => {
    const handleFocus = async () => {
      const { editingDocId: eid } = useRagStore.getState();
      if (!eid) return;
      try {
        const result = await syncExternalEdits();
        if (!result) return;
        if (result.updated) {
          toastSuccess(t('settings_view.knowledge.doc_updated'));
        }
      } catch (e) {
        if (e instanceof Error && e.message === RAG_SYNC_VERSION_CONFLICT_ERROR) {
          toastError(t('settings_view.knowledge.error_sync'));
          return;
        }
        toastError(t('settings_view.knowledge.error_sync'), e instanceof Error ? e.message : String(e));
      }
    };
    window.addEventListener('focus', handleFocus);
    return () => window.removeEventListener('focus', handleFocus);
  }, [syncExternalEdits, toastError, toastSuccess, t]);

  // ─── Create Collection ───────────────────────────────────────────────
  const handleCreate = useCallback(async () => {
    if (!newCollectionName.trim()) return;
    try {
      const collection = await createCollection(newCollectionName.trim(), newCollectionScope);
      setCreateDialogOpen(false);
      setNewCollectionName('');
      await selectCollection(collection.id);
    } catch (e) {
      toastError(t('settings_view.knowledge.error_create_collection'), e instanceof Error ? e.message : String(e));
    }
  }, [newCollectionName, newCollectionScope, createCollection, selectCollection, toastError, t]);

  // ─── Import Files ────────────────────────────────────────────────────
  const handleImportFiles = useCallback(async () => {
    if (!selectedCollectionId) return;
    try {
      const selected = await open({
        multiple: true,
        filters: [
          { name: t('settings_view.knowledge.file_filter_documents'), extensions: ['md', 'txt', 'markdown'] },
        ],
      });
      if (!selected) return;

      const paths = Array.isArray(selected) ? selected : [selected];
      if (paths.length === 0) return;
      setImportProgress({ current: 0, total: paths.length });
      for (let i = 0; i < paths.length; i++) {
        const filePath = paths[i];
        // Check file size before reading
        const fileStat = await stat(filePath);
        if (fileStat.size > MAX_IMPORT_FILE_SIZE) {
          const fileName = filePath.split(/[/\\]/).pop() ?? filePath;
          throw new Error(`File "${fileName}" exceeds 5 MB limit (${Math.round(fileStat.size / 1024 / 1024)} MB)`);
        }
        const content = await readTextFile(filePath);
        const fileName = filePath.split(/[/\\]/).pop() ?? filePath;
        const ext = fileName.split('.').pop()?.toLowerCase() ?? '';
        const format = ext === 'md' || ext === 'markdown' ? 'markdown' : 'plaintext';
        await addDocument(selectedCollectionId, fileName, content, format, filePath);
        setImportProgress({ current: i + 1, total: paths.length });
      }
    } catch (e) {
      toastError(t('settings_view.knowledge.error_import'), e instanceof Error ? e.message : String(e));
    } finally {
      setImportProgress(null);
    }
  }, [selectedCollectionId, addDocument, toastError, t]);

  // ─── Generate Embeddings ─────────────────────────────────────────────
  const handleGenerateEmbeddings = useCallback(async () => {
    if (!selectedCollectionId) return;

    const aiSettings = useSettingsStore.getState().settings.ai;
    if (!aiSettings?.enabled || !aiSettings.providers?.length) return;

    // Resolve embedding provider: embeddingConfig > active chat provider
    const embCfg = aiSettings.embeddingConfig;
    const embProviderId = embCfg?.providerId || aiSettings.activeProviderId;
    const providerConfig = aiSettings.providers.find(p => p.id === embProviderId) ?? aiSettings.providers[0];
    const provider = getProvider(providerConfig.type);
    if (!provider?.embedTexts) {
      toastError(t('settings_view.knowledge.error_no_embedding_support'));
      return;
    }

    // Fetch API key from OS keychain
    let apiKey = '';
    try {
      apiKey = (await api.getAiProviderApiKey(providerConfig.id)) ?? '';
    } catch {
      // Ollama doesn't require API key
    }

    const embeddingModel = embCfg?.model || providerConfig.defaultModel;
    if (!embeddingModel) {
      toastError(t('settings_view.knowledge.error_no_embedding_model'));
      return;
    }
    const BATCH_SIZE = 32;
    let processed = 0;

    try {
      const pending = await getPendingEmbeddings(selectedCollectionId, 500);
      if (pending.length === 0) return;

      setEmbeddingProgress({ current: 0, total: pending.length });
      let failedCount = 0;

      for (let i = 0; i < pending.length; i += BATCH_SIZE) {
        const batch = pending.slice(i, i + BATCH_SIZE);
        const texts = batch.map((p) => p.content);

        try {
          const vectors = await provider.embedTexts(
            {
              baseUrl: providerConfig.baseUrl,
              apiKey,
              model: embeddingModel,
            },
            texts,
          );

          const embeddings = batch.map((p, idx) => ({
            chunkId: p.chunkId,
            vector: vectors[idx],
          }));

          await storeEmbeddings(embeddings, embeddingModel);
        } catch {
          failedCount += batch.length;
        }
        processed += batch.length;
        setEmbeddingProgress({ current: processed, total: pending.length });
      }

      if (failedCount > 0) {
        toastError(
          t('settings_view.knowledge.error_generate_embeddings'),
          t('settings_view.knowledge.embedding_partial_failure', { failed: failedCount, total: pending.length }),
        );
      }
    } catch (e) {
      toastError(t('settings_view.knowledge.error_generate_embeddings'), e instanceof Error ? e.message : String(e));
    } finally {
      setEmbeddingProgress(null);
      // Refresh stats
      if (selectedCollectionId) await selectCollection(selectedCollectionId);
    }
  }, [selectedCollectionId, getPendingEmbeddings, storeEmbeddings, selectCollection, toastError, t]);

  // ─── Reindex ─────────────────────────────────────────────────────────
  const handleReindex = useCallback(async () => {
    if (!selectedCollectionId) return;
    setReindexProgress({ current: 0, total: 0 });
    try {
      await reindexCollection(selectedCollectionId);
      await selectCollection(selectedCollectionId);
    } catch (e) {
      toastError(t('settings_view.knowledge.error_reindex'), e instanceof Error ? e.message : String(e));
    } finally {
      setReindexProgress(null);
    }
  }, [selectedCollectionId, reindexCollection, selectCollection, toastError, t]);

  const handleCancelReindex = useCallback(async () => {
    try {
      await cancelReindex();
    } catch { /* best-effort */ }
  }, [cancelReindex]);

  // ─── Delete Collection ───────────────────────────────────────────────
  const handleDeleteCollection = useCallback((id: string, name: string) => {
    setDeleteConfirm({ type: 'collection', id, name });
  }, []);

  // ─── Delete Document ─────────────────────────────────────────────────
  const handleDeleteDocument = useCallback((docId: string, title: string) => {
    setDeleteConfirm({ type: 'document', id: docId, name: title });
  }, []);

  // ─── Create Blank Document ──────────────────────────────────────────
  const handleCreateBlankDocument = useCallback(async () => {
    if (!selectedCollectionId || !newDocTitle.trim()) return;
    try {
      const doc = await createBlankDocument(selectedCollectionId, newDocTitle.trim(), newDocFormat);
      setNewDocDialogOpen(false);
      setNewDocTitle('');
      // Automatically open in external editor
      await openDocumentExternal(doc.id);
    } catch (e) {
      toastError(t('settings_view.knowledge.error_create_document'), e instanceof Error ? e.message : String(e));
    }
  }, [selectedCollectionId, newDocTitle, newDocFormat, createBlankDocument, openDocumentExternal, toastError, t]);

  // ─── Edit Document Externally ───────────────────────────────────────
  const handleEditExternal = useCallback(async (docId: string) => {
    try {
      await openDocumentExternal(docId);
    } catch (e) {
      toastError(t('settings_view.knowledge.error_open_external'), e instanceof Error ? e.message : String(e));
    }
  }, [openDocumentExternal, toastError, t]);

  // ─── Manual Sync ────────────────────────────────────────────────────
  const handleSyncEdits = useCallback(async () => {
    try {
      const result = await syncExternalEdits();
      if (!result) return;
      if (result.updated) {
        toastSuccess(t('settings_view.knowledge.doc_updated'));
      } else {
        toastSuccess(t('settings_view.knowledge.doc_no_changes'));
      }
    } catch (e) {
      if (e instanceof Error && e.message === RAG_SYNC_VERSION_CONFLICT_ERROR) {
        toastError(t('settings_view.knowledge.error_sync'));
        return;
      }
      toastError(t('settings_view.knowledge.error_sync'), e instanceof Error ? e.message : String(e));
    }
  }, [syncExternalEdits, toastError, toastSuccess, t]);

  // ─── Confirm Delete ──────────────────────────────────────────────────
  const handleConfirmDelete = useCallback(async () => {
    if (!deleteConfirm) return;
    try {
      if (deleteConfirm.type === 'collection') {
        await deleteCollection(deleteConfirm.id);
      } else {
        await removeDocument(deleteConfirm.id);
      }
    } catch (e) {
      toastError(t('settings_view.knowledge.error_delete'), e instanceof Error ? e.message : String(e));
    } finally {
      setDeleteConfirm(null);
    }
  }, [deleteConfirm, deleteCollection, removeDocument, toastError, t]);

  const selectedCollection = collections.find((c) => c.id === selectedCollectionId);

  return (
    <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
      {/* Header */}
      <div>
        <h3 className="text-2xl font-medium text-theme-text mb-2">
          {t('settings_view.knowledge.title')}
        </h3>
        <p className="text-theme-text-muted">
          {t('settings_view.knowledge.description')}
        </p>
      </div>
      <Separator />

      {/* Error Toast */}
      {error && (
        <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400">
          {error}
        </div>
      )}

      {/* ── Collections Panel ── */}
      <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
        <div className="flex items-center justify-between mb-4">
          <h4 className="text-sm font-medium text-theme-text uppercase tracking-wider">
            {t('settings_view.knowledge.collections')}
          </h4>
          <Button size="sm" variant="outline" onClick={() => setCreateDialogOpen(true)}>
            <Plus className="h-3.5 w-3.5 mr-1.5" />
            {t('settings_view.knowledge.create_collection')}
          </Button>
        </div>

        {collections.length === 0 && !isLoading && (
          <div className="flex flex-col items-center justify-center py-12 text-theme-text-muted">
            <BookOpen className="h-10 w-10 mb-3 opacity-40" />
            <p className="text-sm">{t('settings_view.knowledge.no_collections')}</p>
          </div>
        )}

        {collections.length > 0 && (
          <div className="space-y-1">
            {collections.map((col) => (
              <CollectionRow
                key={col.id}
                collection={col}
                isSelected={col.id === selectedCollectionId}
                onSelect={() => selectCollection(col.id)}
                onDelete={() => handleDeleteCollection(col.id, col.name)}
                t={t}
              />
            ))}
          </div>
        )}
      </div>

      {/* ── Selected Collection Details ── */}
      {selectedCollection && (
        <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
          <div className="flex items-center justify-between mb-4">
            <div>
              <h4 className="text-sm font-medium text-theme-text uppercase tracking-wider">
                {selectedCollection.name}
              </h4>
              <p className="text-xs text-theme-text-muted mt-0.5">
                {scopeLabel(selectedCollection.scope, t)}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <Button
                size="sm"
                variant="outline"
                onClick={handleImportFiles}
                disabled={!!importProgress}
              >
                {importProgress ? (
                  <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
                ) : (
                  <FolderOpen className="h-3.5 w-3.5 mr-1.5" />
                )}
                {importProgress
                  ? `${importProgress.current}/${importProgress.total}`
                  : t('settings_view.knowledge.import_files')}
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => setNewDocDialogOpen(true)}
              >
                <FilePlus className="h-3.5 w-3.5 mr-1.5" />
                {t('settings_view.knowledge.new_document')}
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={handleGenerateEmbeddings}
                disabled={!!embeddingProgress}
              >
                {embeddingProgress ? (
                  <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
                ) : (
                  <Sparkles className="h-3.5 w-3.5 mr-1.5" />
                )}
                {embeddingProgress
                  ? `${embeddingProgress.current}/${embeddingProgress.total}`
                  : t('settings_view.knowledge.generate_embeddings')}
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={reindexProgress ? handleCancelReindex : handleReindex}
                disabled={reindexProgress !== null && reindexProgress.total === 0}
              >
                {reindexProgress ? (
                  <>
                    <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
                    {reindexProgress.total > 0
                      ? `${reindexProgress.current}/${reindexProgress.total}`
                      : t('settings_view.knowledge.reindex')}
                    {reindexProgress.total > 0 && (
                      <X className="h-3 w-3 ml-1.5" />
                    )}
                  </>
                ) : (
                  <>
                    <RefreshCw className="h-3.5 w-3.5 mr-1.5" />
                    {t('settings_view.knowledge.reindex')}
                  </>
                )}
              </Button>
            </div>
          </div>

          {/* Stats */}
          {stats && (
            <StatsBar stats={stats} statsStale={statsStale} t={t} />
          )}

          <Separator className="my-4" />

          {/* Documents List */}
          {documents.length === 0 && !isLoading && (
            <div className="flex flex-col items-center justify-center py-8 text-theme-text-muted">
              <FileText className="h-8 w-8 mb-2 opacity-40" />
              <p className="text-sm">{t('settings_view.knowledge.no_documents')}</p>
            </div>
          )}

          {documents.length > 0 && (
            <div className="space-y-1">
              {documents.map((doc) => (
                <DocumentRow
                  key={doc.id}
                  document={doc}
                  isEditing={editingDocId === doc.id}
                  onEdit={() => handleEditExternal(doc.id)}
                  onSync={handleSyncEdits}
                  onDelete={() => handleDeleteDocument(doc.id, doc.title)}
                  t={t}
                />
              ))}
            </div>
          )}
        </div>
      )}

      {/* ── Create Collection Dialog ── */}
      <Dialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t('settings_view.knowledge.create_collection')}</DialogTitle>
            <DialogDescription>
              {t('settings_view.knowledge.create_description')}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-2">
            <div>
              <Label>{t('settings_view.knowledge.collection_name')}</Label>
              <Input
                value={newCollectionName}
                onChange={(e) => setNewCollectionName(e.target.value)}
                placeholder={t('settings_view.knowledge.collection_name_placeholder')}
                className="mt-1.5"
                onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
              />
            </div>
            <div>
              <Label>{t('settings_view.knowledge.scope')}</Label>
              <Select value={newCollectionScope} onValueChange={(v) => setNewCollectionScope(v as 'global')}>
                <SelectTrigger className="mt-1.5">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="global">{t('settings_view.knowledge.scope_global')}</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setCreateDialogOpen(false)}>
              {t('common.cancel')}
            </Button>
            <Button onClick={handleCreate} disabled={!newCollectionName.trim()}>
              {t('common.create')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* ── New Document Dialog ── */}
      <Dialog open={newDocDialogOpen} onOpenChange={setNewDocDialogOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t('settings_view.knowledge.new_document')}</DialogTitle>
            <DialogDescription>
              {t('settings_view.knowledge.new_document_description')}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-2">
            <div>
              <Label>{t('settings_view.knowledge.new_document_title')}</Label>
              <Input
                value={newDocTitle}
                onChange={(e) => setNewDocTitle(e.target.value)}
                placeholder={t('settings_view.knowledge.new_document_title_placeholder')}
                className="mt-1.5"
                onKeyDown={(e) => e.key === 'Enter' && handleCreateBlankDocument()}
              />
            </div>
            <div>
              <Label>{t('settings_view.knowledge.new_document_format')}</Label>
              <Select value={newDocFormat} onValueChange={(v) => setNewDocFormat(v as 'markdown' | 'plaintext')}>
                <SelectTrigger className="mt-1.5">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="markdown">Markdown</SelectItem>
                  <SelectItem value="plaintext">Plain Text</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setNewDocDialogOpen(false)}>
              {t('common.cancel')}
            </Button>
            <Button onClick={handleCreateBlankDocument} disabled={!newDocTitle.trim()}>
              {t('common.create')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* ── Delete Confirmation Dialog ── */}
      <Dialog open={!!deleteConfirm} onOpenChange={(open) => { if (!open) setDeleteConfirm(null); }}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t('settings_view.knowledge.delete_confirm_title')}</DialogTitle>
            <DialogDescription>
              {deleteConfirm?.type === 'collection'
                ? t('settings_view.knowledge.delete_collection_confirm', { name: deleteConfirm.name })
                : t('settings_view.knowledge.delete_document_confirm', { name: deleteConfirm?.name ?? '' })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setDeleteConfirm(null)}>
              {t('common.cancel')}
            </Button>
            <Button variant="destructive" onClick={handleConfirmDelete}>
              {t('common.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════════════
// Sub-components
// ═══════════════════════════════════════════════════════════════════════════

function CollectionRow({
  collection,
  isSelected,
  onSelect,
  onDelete,
  t,
}: {
  collection: RagCollection;
  isSelected: boolean;
  onSelect: () => void;
  onDelete: () => void;
  t: (key: string) => string;
}) {
  return (
    <div
      className={`flex items-center justify-between rounded-md px-3 py-2 cursor-pointer transition-colors ${
        isSelected
          ? 'bg-theme-accent/10 border border-theme-accent/30'
          : 'hover:bg-theme-bg-hover border border-transparent'
      }`}
      onClick={onSelect}
    >
      <div className="flex items-center gap-3 min-w-0">
        <BookOpen className="h-4 w-4 text-theme-text-muted shrink-0" />
        <div className="min-w-0">
          <p className="text-sm text-theme-text truncate">{collection.name}</p>
          <p className="text-xs text-theme-text-muted">
            {scopeLabel(collection.scope, t)} · {formatDate(collection.updatedAt)}
          </p>
        </div>
      </div>
      <Button
        size="sm"
        variant="ghost"
        className="h-7 w-7 p-0 text-theme-text-muted hover:text-red-400 shrink-0"
        onClick={(e) => {
          e.stopPropagation();
          onDelete();
        }}
      >
        <Trash2 className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}

function DocumentRow({
  document: doc,
  isEditing,
  onEdit,
  onSync,
  onDelete,
  t,
}: {
  document: RagDocument;
  isEditing: boolean;
  onEdit: () => void;
  onSync: () => void;
  onDelete: () => void;
  t: (key: string) => string;
}) {
  return (
    <div className="flex items-center justify-between rounded-md px-3 py-2 hover:bg-theme-bg-hover transition-colors">
      <div className="flex items-center gap-3 min-w-0">
        <FileText className="h-4 w-4 text-theme-text-muted shrink-0" />
        <div className="min-w-0">
          <p className="text-sm text-theme-text truncate">{doc.title}</p>
          <p className="text-xs text-theme-text-muted">
            {doc.format} · {doc.chunkCount} {t('settings_view.knowledge.chunks')} · {formatDate(doc.indexedAt)}
          </p>
        </div>
      </div>
      <div className="flex items-center gap-1 shrink-0">
        {isEditing ? (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0 text-theme-accent hover:text-theme-accent"
            onClick={onSync}
            title={t('settings_view.knowledge.sync_changes')}
          >
            <RefreshCw className="h-3.5 w-3.5" />
          </Button>
        ) : (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0 text-theme-text-muted hover:text-theme-text"
            onClick={onEdit}
            title={t('settings_view.knowledge.edit_externally')}
          >
            <Pencil className="h-3.5 w-3.5" />
          </Button>
        )}
        <Button
          size="sm"
          variant="ghost"
          className="h-7 w-7 p-0 text-theme-text-muted hover:text-red-400 shrink-0"
          onClick={onDelete}
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  );
}

function StatsBar({ stats, statsStale, t }: { stats: RagCollectionStats; statsStale: boolean; t: (key: string) => string }) {
  const embeddedPct = stats.chunkCount > 0
    ? Math.round((stats.embeddedChunkCount / stats.chunkCount) * 100)
    : 0;

  return (
    <div className="flex items-center gap-6 text-xs text-theme-text-muted">
      <span>
        <strong className="text-theme-text">{stats.docCount}</strong> {t('settings_view.knowledge.stat_docs')}
      </span>
      <span>
        <strong className="text-theme-text">{stats.chunkCount}</strong> {t('settings_view.knowledge.stat_chunks')}
      </span>
      <span>
        <strong className="text-theme-text">{embeddedPct}%</strong> {t('settings_view.knowledge.stat_embedded')}
      </span>
      {stats.lastUpdated > 0 && (
        <span>
          {t('settings_view.knowledge.stat_updated')} {formatDate(stats.lastUpdated)}
        </span>
      )}
      {statsStale && (
        <span className="text-yellow-500" title={t('settings_view.knowledge.stats_stale')}>
          ⚠
        </span>
      )}
    </div>
  );
}
