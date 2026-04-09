// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { X, AlertTriangle, CheckCircle, CheckSquare, Square } from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogClose } from '../ui/dialog';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { useAppStore } from '../../store/appStore';
import type { OxideMetadata, ImportResult, ImportPreview } from '../../types';

type ImportConflictStrategy = 'rename' | 'skip' | 'replace' | 'merge';

interface OxideImportModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function OxideImportModal({ isOpen, onClose }: OxideImportModalProps) {
  const { t } = useTranslation();
  const { loadSavedConnections } = useAppStore();
  const [fileData, setFileData] = useState<Uint8Array | null>(null);
  const [metadata, setMetadata] = useState<OxideMetadata | null>(null);
  const [password, setPassword] = useState('');
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [importing, setImporting] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [selectedNames, setSelectedNames] = useState<Set<string>>(new Set());
  const [conflictStrategy, setConflictStrategy] = useState<ImportConflictStrategy>('rename');

  const getSelectableNames = (nextPreview: ImportPreview) => new Set([
    ...nextPreview.unchanged,
    ...nextPreview.willRename.map(([original]) => original),
    ...nextPreview.willSkip,
    ...nextPreview.willReplace,
    ...nextPreview.willMerge,
  ]);

  const totalSelectable = preview
    ? preview.unchanged.length
      + preview.willRename.length
      + preview.willSkip.length
      + preview.willReplace.length
      + preview.willMerge.length
    : 0;

  // ... (handlers unchanged)

  const handleSelectFile = async () => {
    setError(null);
    setResult(null);
    setPreview(null);

    try {
      const selected = await open({
        filters: [{ name: 'Oxide Config', extensions: ['oxide'] }],
        multiple: false,
      });

      if (selected && typeof selected === 'string') {
        const filePath = selected;
        const data = await readFile(filePath);
        setFileData(data);

        // Validate file and extract metadata (no password needed)
        try {
          const meta: OxideMetadata = await invoke('validate_oxide_file', {
            fileData: Array.from(data),
          });
          setMetadata(meta);
        } catch (err) {
          console.error('File validation failed:', err);
          setError(`Invalid .oxide file: ${err}`);
          setFileData(null);
        }
      }
    } catch (err) {
      console.error('File selection failed:', err);
      setError(`File selection failed: ${err}`);
    }
  };

  const handlePreview = async () => {
    if (!fileData || !password) {
      setError(t('modals.import.error_enter_password'));
      return;
    }

    setError(null);
    setPreviewing(true);

    try {
      const previewResult: ImportPreview = await invoke('preview_oxide_import', {
        fileData: Array.from(fileData),
        password,
        conflictStrategy,
      });
      setPreview(previewResult);
      setSelectedNames(getSelectableNames(previewResult));
    } catch (err) {
      console.error('Preview failed:', err);
      const errorMsg = String(err).toLowerCase();
      if ((errorMsg.includes('password') && (errorMsg.includes('incorrect') || errorMsg.includes('wrong') || errorMsg.includes('failed'))) || errorMsg.includes('decryption failed') || errorMsg.includes('密码错误')) {
        setError(t('modals.import.error_password'));
      } else if (errorMsg.includes('checksum') || errorMsg.includes('tamper') || errorMsg.includes('verification failed')) {
        setError(t('modals.import.error_tampered'));
      } else {
        setError(`${t('modals.import.title')}: ${err}`);
      }
    } finally {
      setPreviewing(false);
    }
  };

  const handleImport = async () => {
    if (!fileData || !password) {
      setError(t('modals.import.error_enter_password'));
      return;
    }

    setError(null);
    setImporting(true);

    try {
      const importResult: ImportResult = await invoke('import_from_oxide', {
        fileData: Array.from(fileData),
        password,
        selectedNames: Array.from(selectedNames),
        conflictStrategy,
      });

      setResult(importResult);

      // Refresh connections list
      await loadSavedConnections();

      if (importResult.errors.length === 0) {
        setTimeout(() => {
          onClose();
        }, 2000);
      }
    } catch (err) {
      console.error('Import failed:', err);
      const errorMsg = String(err).toLowerCase();
      if ((errorMsg.includes('password') && (errorMsg.includes('incorrect') || errorMsg.includes('wrong') || errorMsg.includes('failed'))) || errorMsg.includes('decryption failed')) {
        setError(t('modals.import.error_password'));
      } else if (errorMsg.includes('checksum') || errorMsg.includes('tamper') || errorMsg.includes('verification failed')) {
        setError(t('modals.import.error_tampered'));
      } else {
        setError(`${t('modals.import.title')}: ${err}`);
      }
    } finally {
      setImporting(false);
    }
  };

  const toggleName = (name: string) => {
    setSelectedNames(prev => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  const toggleAll = () => {
    if (!preview) return;
    const allNames = getSelectableNames(preview);
    if (selectedNames.size === allNames.size) {
      setSelectedNames(new Set());
    } else {
      setSelectedNames(allNames);
    }
  };

  const handleClose = () => {
    setFileData(null);
    setMetadata(null);
    setPassword('');
    setPreview(null);
    setError(null);
    setResult(null);
    setSelectedNames(new Set());
    setConflictStrategy('rename');
    onClose();
  };

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="max-w-2xl gap-0 bg-theme-bg-elevated border-theme-border text-theme-text p-0 overflow-hidden">
        <DialogHeader className="flex flex-row items-center justify-between border-b border-theme-border px-6 py-4">
          <DialogTitle className="text-xl font-semibold text-theme-text-heading">{t('modals.import.title')}</DialogTitle>
          <DialogClose className="rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none data-[state=open]:bg-accent data-[state=open]:text-muted-foreground">
            <X className="h-4 w-4" />
            <span className="sr-only">{t('modals.import.close')}</span>
          </DialogClose>
        </DialogHeader>

        <div className="p-6 space-y-4">
          {!fileData ? (
            /* File Selection */
            <div className="text-center py-8">
              <Button onClick={handleSelectFile} className="bg-theme-accent text-white hover:bg-theme-accent-hover">
                {t('modals.import.select_file')}
              </Button>
              
              <div className="mt-6 bg-blue-500/10 border border-blue-500/20 text-blue-500 px-4 py-3 rounded text-sm text-left">
                <p className="font-semibold">{t('modals.import.instructions_title')}</p>
                <ul className="mt-1 space-y-1 text-xs opacity-90 list-disc list-inside">
                  <li>{t('modals.import.instructions_1')}</li>
                  <li>{t('modals.import.instructions_2')}</li>
                  <li>{t('modals.import.instructions_3')}</li>
                  <li>{t('modals.import.instructions_4')}</li>
                </ul>
              </div>
            </div>
          ) : result ? (
            /* Import Result */
            <div className="py-4">
              <div className={`p-4 rounded border ${
                result.errors.length === 0 
                  ? 'bg-green-500/10 border-green-500/20 text-green-500'
                  : 'bg-yellow-500/10 border-yellow-500/20 text-yellow-500'
              }`}>
                <p className="font-semibold text-lg">
                  {t('modals.import.success', { count: result.imported })}
                </p>
                {result.skipped > 0 && (
                  <p className="text-sm mt-1">{t('modals.import.skipped', { count: result.skipped })}</p>
                )}
                {result.merged > 0 && (
                  <p className="text-sm mt-1">{t('modals.import.merged', { count: result.merged })}</p>
                )}
                {result.replaced > 0 && (
                  <p className="text-sm mt-1">{t('modals.import.replaced', { count: result.replaced })}</p>
                )}
                {result.renamed > 0 && (
                  <div className="mt-2">
                    <p className="text-sm font-semibold text-yellow-400">{t('modals.import.renamed', { count: result.renamed })}</p>
                    <ul className="text-xs mt-1 space-y-1 opacity-90 max-h-24 overflow-y-auto">
                      {result.renames.map(([original, renamed], i) => (
                        <li key={i}>• "{original}" → "{renamed}"</li>
                      ))}
                    </ul>
                  </div>
                )}
                {result.errors.length > 0 && (
                  <div className="mt-2">
                    <p className="text-sm font-semibold">{t('modals.import.errors')}</p>
                    <ul className="text-xs mt-1 space-y-1 opacity-90">
                      {result.errors.map((err, i) => (
                        <li key={i}>• {err}</li>
                      ))}
                    </ul>
                  </div>
                )}
              </div>

              {result.errors.length === 0 && (
                <p className="text-sm text-theme-text-muted text-center mt-4">
                  {t('modals.import.auto_close')}
                </p>
              )}
            </div>
          ) : preview ? (
            /* Preview - Show what will happen before confirming */
            <>
              <div className="border border-theme-border rounded-md p-4 space-y-3 bg-theme-bg">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <CheckCircle className="h-5 w-5 text-green-500" />
                    <h3 className="font-semibold text-theme-text">{t('modals.import.preview_title')}</h3>
                  </div>
                  <button
                    type="button"
                    onClick={toggleAll}
                    className="text-xs text-theme-accent hover:text-theme-accent-hover transition-colors"
                  >
                    {selectedNames.size === totalSelectable
                      ? t('modals.import.deselect_all')
                      : t('modals.import.select_all')}
                  </button>
                </div>
                
                <p className="text-sm text-theme-text">
                  {t('modals.import.preview_total', { count: preview.totalConnections })}
                  {' — '}
                  <span className="text-theme-accent font-medium">
                    {t('modals.import.selected_count', { count: selectedNames.size })}
                  </span>
                </p>

                {/* Connections without conflicts */}
                {preview.unchanged.length > 0 && (
                  <div>
                    <p className="text-sm font-semibold text-green-500">
                      {t('modals.import.preview_unchanged', { count: preview.unchanged.length })}
                    </p>
                    <ul className="text-xs text-theme-text-muted mt-1 space-y-1 max-h-20 overflow-y-auto">
                      {preview.unchanged.map((name, i) => (
                        <li
                          key={i}
                          className="flex items-center gap-1.5 cursor-pointer hover:text-theme-text transition-colors"
                          onClick={() => toggleName(name)}
                        >
                          {selectedNames.has(name)
                            ? <CheckSquare className="h-3.5 w-3.5 text-theme-accent flex-shrink-0" />
                            : <Square className="h-3.5 w-3.5 flex-shrink-0" />}
                          {name}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                {/* Connections with name conflicts that will be renamed */}
                {preview.willRename.length > 0 && (
                  <div>
                    <div className="flex items-center gap-2">
                      <AlertTriangle className="h-4 w-4 text-yellow-500" />
                      <p className="text-sm font-semibold text-yellow-500">
                        {t('modals.import.preview_will_rename', { count: preview.willRename.length })}
                      </p>
                    </div>
                    <ul className="text-xs text-yellow-400 mt-1 space-y-1 max-h-24 overflow-y-auto">
                      {preview.willRename.map(([original, renamed], i) => (
                        <li
                          key={i}
                          className="flex items-center gap-1.5 cursor-pointer hover:text-yellow-300 transition-colors"
                          onClick={() => toggleName(original)}
                        >
                          {selectedNames.has(original)
                            ? <CheckSquare className="h-3.5 w-3.5 text-theme-accent flex-shrink-0" />
                            : <Square className="h-3.5 w-3.5 flex-shrink-0" />}
                          "{original}" → "{renamed}"
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                {preview.willMerge.length > 0 && (
                  <div>
                    <div className="flex items-center gap-2">
                      <CheckCircle className="h-4 w-4 text-blue-500" />
                      <p className="text-sm font-semibold text-blue-500">
                        {t('modals.import.preview_will_merge', { count: preview.willMerge.length })}
                      </p>
                    </div>
                    <ul className="text-xs text-blue-400 mt-1 space-y-1 max-h-24 overflow-y-auto">
                      {preview.willMerge.map((name, i) => (
                        <li
                          key={i}
                          className="flex items-center gap-1.5 cursor-pointer hover:text-blue-300 transition-colors"
                          onClick={() => toggleName(name)}
                        >
                          {selectedNames.has(name)
                            ? <CheckSquare className="h-3.5 w-3.5 text-theme-accent flex-shrink-0" />
                            : <Square className="h-3.5 w-3.5 flex-shrink-0" />}
                          {name}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                {preview.willReplace.length > 0 && (
                  <div>
                    <div className="flex items-center gap-2">
                      <AlertTriangle className="h-4 w-4 text-orange-500" />
                      <p className="text-sm font-semibold text-orange-500">
                        {t('modals.import.preview_will_replace', { count: preview.willReplace.length })}
                      </p>
                    </div>
                    <ul className="text-xs text-orange-400 mt-1 space-y-1 max-h-24 overflow-y-auto">
                      {preview.willReplace.map((name, i) => (
                        <li
                          key={i}
                          className="flex items-center gap-1.5 cursor-pointer hover:text-orange-300 transition-colors"
                          onClick={() => toggleName(name)}
                        >
                          {selectedNames.has(name)
                            ? <CheckSquare className="h-3.5 w-3.5 text-theme-accent flex-shrink-0" />
                            : <Square className="h-3.5 w-3.5 flex-shrink-0" />}
                          {name}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                {preview.willSkip.length > 0 && (
                  <div>
                    <div className="flex items-center gap-2">
                      <AlertTriangle className="h-4 w-4 text-slate-400" />
                      <p className="text-sm font-semibold text-slate-300">
                        {t('modals.import.preview_will_skip', { count: preview.willSkip.length })}
                      </p>
                    </div>
                    <ul className="text-xs text-slate-400 mt-1 space-y-1 max-h-24 overflow-y-auto">
                      {preview.willSkip.map((name, i) => (
                        <li
                          key={i}
                          className="flex items-center gap-1.5 cursor-pointer hover:text-slate-200 transition-colors"
                          onClick={() => toggleName(name)}
                        >
                          {selectedNames.has(name)
                            ? <CheckSquare className="h-3.5 w-3.5 text-theme-accent flex-shrink-0" />
                            : <Square className="h-3.5 w-3.5 flex-shrink-0" />}
                          {name}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                {/* Embedded keys notice */}
                {preview.hasEmbeddedKeys && (
                  <div className="bg-blue-500/10 border border-blue-500/20 text-blue-500 px-3 py-2 rounded text-xs">
                    {t('modals.import.preview_embedded_keys')}
                  </div>
                )}

                {/* Port forwarding rules notice */}
                {preview.totalForwards > 0 && (
                  <div className="bg-blue-500/10 border border-blue-500/20 text-blue-500 px-3 py-2 rounded text-xs">
                    {t('modals.import.preview_forwards', { count: preview.totalForwards })}
                  </div>
                )}
              </div>

              {/* Actions */}
              <div className="flex justify-end space-x-2 pt-2">
                <Button 
                  variant="outline" 
                  onClick={() => setPreview(null)} 
                  disabled={importing} 
                  className="border-theme-border text-theme-text hover:bg-theme-bg-hover"
                >
                  {t('modals.import.back')}
                </Button>
                <Button 
                  onClick={handleImport} 
                  disabled={importing || selectedNames.size === 0}
                  className="bg-theme-accent text-white hover:bg-theme-accent-hover disabled:opacity-50"
                >
                  {importing ? t('modals.import.importing') : t('modals.import.confirm_import')}
                </Button>
              </div>
            </>
          ) : (
            /* File Info & Password Input */
            <>
              {metadata && (
                <div className="border border-theme-border rounded-md p-4 space-y-2 bg-theme-bg">
                  <h3 className="font-semibold text-theme-text">{t('modals.import.file_info')}</h3>
                  <div className="text-sm space-y-1 text-theme-text">
                    <p><span className="text-theme-text-muted">{t('modals.import.exported_at')}</span> {new Date(metadata.exported_at).toLocaleString()}</p>
                    <p><span className="text-theme-text-muted">{t('modals.import.exported_by')}</span> {metadata.exported_by}</p>
                    {metadata.description && (
                      <p><span className="text-theme-text-muted">{t('modals.import.description')}</span> {metadata.description}</p>
                    )}
                    <p><span className="text-theme-text-muted">{t('modals.import.contains')}</span> {t('modals.import.connections_count', { count: metadata.num_connections })}</p>
                  </div>

                  <div className="mt-3">
                    <p className="text-sm font-semibold text-theme-text">{t('modals.import.connection_list')}</p>
                    <ul className="text-xs text-theme-text-muted mt-1 space-y-1 max-h-32 overflow-y-auto">
                      {metadata.connection_names.map((name, i) => (
                        <li key={i}>• {name}</li>
                      ))}
                    </ul>
                  </div>
                </div>
              )}

              {/* Password Input */}
              <div>
                <Label className="text-theme-text">{t('modals.import.password')}</Label>
                <Input
                  type="password"
                  placeholder={t('modals.import.password_placeholder')}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && password) {
                      handlePreview();
                    }
                  }}
                  className="mt-1 bg-theme-bg border-theme-border text-theme-text placeholder:text-theme-text-muted focus-visible:ring-theme-accent"
                  autoFocus
                />
              </div>

              <div>
                <Label className="text-theme-text">{t('modals.import.conflict_strategy')}</Label>
                <div className="grid grid-cols-2 gap-2 mt-2">
                  {(['rename', 'skip', 'replace', 'merge'] as const).map((strategy) => (
                    <button
                      key={strategy}
                      type="button"
                      onClick={() => setConflictStrategy(strategy)}
                      className={`rounded-md border px-3 py-2 text-sm text-left transition-colors ${
                        conflictStrategy === strategy
                          ? 'border-theme-accent bg-theme-accent/10 text-theme-text'
                          : 'border-theme-border bg-theme-bg text-theme-text-muted hover:bg-theme-bg-hover hover:text-theme-text'
                      }`}
                    >
                      {t(`modals.import.strategy_${strategy}`)}
                    </button>
                  ))}
                </div>
              </div>

              {/* Error Message */}
              {error && (
                <div className="bg-red-500/10 border border-red-500/20 text-red-500 px-3 py-2 rounded text-sm">
                  {error}
                </div>
              )}

              {/* Warning */}
              <div className="bg-yellow-500/10 border border-yellow-500/20 text-yellow-500 px-3 py-2 rounded text-sm">
                <p className="font-semibold">{t('modals.import.warning_title')}</p>
                <p className="text-xs mt-1 opacity-90">
                  {t('modals.import.warning_text')}
                </p>
              </div>

              {/* Actions */}
              <div className="flex justify-end space-x-2 pt-2">
                <Button variant="outline" onClick={handleSelectFile} disabled={previewing} className="border-theme-border text-theme-text hover:bg-theme-bg-hover">
                  {t('modals.import.reselect_file')}
                </Button>
                <Button variant="outline" onClick={handleClose} disabled={previewing} className="border-theme-border text-theme-text hover:bg-theme-bg-hover">
                  {t('modals.import.cancel')}
                </Button>
                <Button 
                  onClick={handlePreview} 
                  disabled={previewing || !password}
                  className="bg-theme-accent text-white hover:bg-theme-accent-hover disabled:opacity-50"
                >
                  {previewing ? t('modals.import.previewing') : t('modals.import.preview')}
                </Button>
              </div>
            </>
          )}

          {/* Result Actions */}
          {result && (
            <div className="flex justify-end space-x-2 pt-2">
              <Button onClick={handleClose} className="bg-theme-accent text-white hover:bg-theme-accent-hover">
                {t('modals.import.close')}
              </Button>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
