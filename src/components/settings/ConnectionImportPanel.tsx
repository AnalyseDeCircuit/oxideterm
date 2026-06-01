// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import { FileInput, FolderOpen, RefreshCw, Upload } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { useToast } from '@/hooks/useToast';
import { api } from '@/lib/api';
import { cn } from '@/lib/utils';
import type {
    ConnectionImportDuplicateStrategy,
    ConnectionImportPreview,
    ConnectionImportSource,
} from '@/types';

type ConnectionImportPanelProps = {
    onImported: () => Promise<void>;
};

const IMPORT_SOURCES: ConnectionImportSource[] = ['securecrt', 'xshell', 'termius'];

// Dialog filters mirror the backend parsers so users cannot accidentally pick unrelated files.
const sourceFileFilters: Record<ConnectionImportSource, { name: string; extensions: string[] }[]> = {
    securecrt: [{ name: 'SecureCRT', extensions: ['ini'] }],
    xshell: [{ name: 'Xshell', extensions: ['xsh'] }],
    termius: [{ name: 'Termius export', extensions: ['json'] }],
};

export function ConnectionImportPanel({ onImported }: ConnectionImportPanelProps) {
    const { t } = useTranslation();
    const { success: toastSuccess, error: toastError } = useToast();
    const [source, setSource] = useState<ConnectionImportSource>('securecrt');
    const [paths, setPaths] = useState<string[]>([]);
    const [preview, setPreview] = useState<ConnectionImportPreview | null>(null);
    const [selectedDraftIds, setSelectedDraftIds] = useState<Set<string>>(new Set());
    const [duplicateStrategy, setDuplicateStrategy] = useState<ConnectionImportDuplicateStrategy>('skip');
    const [targetGroup, setTargetGroup] = useState('');
    const [previewing, setPreviewing] = useState(false);
    const [applying, setApplying] = useState(false);

    const resetPreview = () => {
        setPreview(null);
        setSelectedDraftIds(new Set());
    };

    const updateSource = (nextSource: ConnectionImportSource) => {
        setSource(nextSource);
        setPaths([]);
        resetPreview();
    };

    const chooseFiles = async () => {
        const selected = await open({
            multiple: source !== 'termius',
            directory: false,
            filters: sourceFileFilters[source],
        });
        if (!selected) return;
        const nextPaths = Array.isArray(selected) ? selected : [selected];
        setPaths(nextPaths);
        resetPreview();
    };

    const chooseDirectory = async () => {
        const selected = await open({
            multiple: false,
            directory: true,
        });
        if (!selected || Array.isArray(selected)) return;
        setPaths([selected]);
        resetPreview();
    };

    const runPreview = async () => {
        if (paths.length === 0) return;
        setPreviewing(true);
        try {
            const result = await api.previewConnectionImport(source, paths);
            setPreview(result);
            // Duplicate drafts stay visible but start unchecked so users make an explicit choice.
            setSelectedDraftIds(new Set(result.drafts.filter((draft) => draft.importable && !draft.duplicate).map((draft) => draft.id)));
        } catch (error) {
            console.error('Connection import preview failed:', error);
            toastError(t('settings_view.connections.importers.preview_failed', { error }));
        } finally {
            setPreviewing(false);
        }
    };

    const toggleDraft = (id: string) => {
        setSelectedDraftIds((previous) => {
            const next = new Set(previous);
            if (next.has(id)) next.delete(id);
            else next.add(id);
            return next;
        });
    };

    const toggleAll = () => {
        if (!preview) return;
        const importable = preview.drafts.filter((draft) => draft.importable);
        const allSelected = importable.length > 0 && importable.every((draft) => selectedDraftIds.has(draft.id));
        setSelectedDraftIds(allSelected ? new Set() : new Set(importable.map((draft) => draft.id)));
    };

    const applyImport = async () => {
        if (paths.length === 0 || selectedDraftIds.size === 0) return;
        setApplying(true);
        try {
            const result = await api.applyConnectionImport({
                source,
                paths,
                selectedDraftIds: Array.from(selectedDraftIds),
                duplicateStrategy,
                targetGroup: targetGroup.trim() || null,
            });
            const parts = [
                result.imported > 0 ? t('settings_view.connections.importers.imported_count', { count: result.imported }) : '',
                result.skipped > 0 ? t('settings_view.connections.importers.skipped_count', { count: result.skipped }) : '',
                result.renamed > 0 ? t('settings_view.connections.importers.renamed_count', { count: result.renamed }) : '',
                result.errors.length > 0 ? t('settings_view.connections.importers.error_count', { count: result.errors.length }) : '',
            ].filter(Boolean);
            toastSuccess(parts.join(' · ') || t('settings_view.connections.importers.no_changes'));
            await onImported();
            await runPreview();
        } catch (error) {
            console.error('Connection import apply failed:', error);
            toastError(t('settings_view.connections.importers.apply_failed', { error }));
        } finally {
            setApplying(false);
        }
    };

    return (
        <div className="pt-8">
            <h3 className="text-xl font-medium text-theme-text-heading mb-2">{t('settings_view.connections.importers.title')}</h3>
            <p className="text-sm text-theme-text-muted mb-4">{t('settings_view.connections.importers.description')}</p>

            <div className="grid gap-4 max-w-4xl">
                <div className="grid grid-cols-1 md:grid-cols-[220px_minmax(0,1fr)] gap-3">
                    <div className="grid gap-2">
                        <Label>{t('settings_view.connections.importers.source')}</Label>
                        <Select value={source} onValueChange={(value) => updateSource(value as ConnectionImportSource)}>
                            <SelectTrigger>
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                {IMPORT_SOURCES.map((item) => (
                                    <SelectItem key={item} value={item}>
                                        {t(`settings_view.connections.importers.sources.${item}`)}
                                    </SelectItem>
                                ))}
                            </SelectContent>
                        </Select>
                    </div>

                    <div className="grid gap-2">
                        <Label>{t('settings_view.connections.importers.paths')}</Label>
                        <div className="flex flex-wrap gap-2">
                            <Button type="button" variant="secondary" onClick={chooseFiles}>
                                <FileInput className="h-4 w-4 mr-1" /> {t('settings_view.connections.importers.choose_files')}
                            </Button>
                            {source !== 'termius' && (
                                <Button type="button" variant="secondary" onClick={chooseDirectory}>
                                    <FolderOpen className="h-4 w-4 mr-1" /> {t('settings_view.connections.importers.choose_directory')}
                                </Button>
                            )}
                            <Button type="button" onClick={runPreview} disabled={paths.length === 0 || previewing}>
                                <RefreshCw className={cn('h-4 w-4 mr-1', previewing && 'animate-spin')} />
                                {previewing ? t('settings_view.connections.importers.previewing') : t('settings_view.connections.importers.preview')}
                            </Button>
                        </div>
                        <p className="text-xs text-theme-text-muted truncate">
                            {paths.length > 0 ? paths.join(' · ') : t('settings_view.connections.importers.no_paths')}
                        </p>
                    </div>
                </div>

                {preview && (
                    <div className="grid gap-3">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <button type="button" onClick={toggleAll} className="text-xs text-theme-accent hover:text-theme-accent-hover transition-colors">
                                {preview.drafts.filter((draft) => draft.importable).every((draft) => selectedDraftIds.has(draft.id))
                                    ? t('settings_view.connections.importers.deselect_all')
                                    : t('settings_view.connections.importers.select_all')}
                            </button>
                            <div className="flex flex-wrap items-center gap-2">
                                <Select value={duplicateStrategy} onValueChange={(value) => setDuplicateStrategy(value as ConnectionImportDuplicateStrategy)}>
                                    <SelectTrigger className="w-36 h-8">
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        <SelectItem value="skip">{t('settings_view.connections.importers.duplicate_skip')}</SelectItem>
                                        <SelectItem value="rename">{t('settings_view.connections.importers.duplicate_rename')}</SelectItem>
                                    </SelectContent>
                                </Select>
                                <Input
                                    className="h-8 w-48"
                                    value={targetGroup}
                                    onChange={(event) => setTargetGroup(event.target.value)}
                                    placeholder={t('settings_view.connections.importers.target_group')}
                                />
                                <Button size="sm" onClick={applyImport} disabled={selectedDraftIds.size === 0 || applying}>
                                    <Upload className="h-4 w-4 mr-1" />
                                    {applying
                                        ? t('settings_view.connections.importers.importing')
                                        : t('settings_view.connections.importers.import_selected', { count: selectedDraftIds.size })}
                                </Button>
                            </div>
                        </div>

                        <div className="h-72 overflow-y-auto border border-theme-border rounded-md bg-theme-bg-panel">
                            {preview.drafts.map((draft) => (
                                <div key={draft.id} className={cn('grid grid-cols-[28px_minmax(0,1fr)_120px] gap-2 p-3 border-b border-theme-border/60', !draft.importable && 'opacity-50')}>
                                    <Checkbox
                                        checked={selectedDraftIds.has(draft.id)}
                                        disabled={!draft.importable}
                                        onCheckedChange={() => draft.importable && toggleDraft(draft.id)}
                                        className="mt-1 border-theme-text-muted data-[state=checked]:bg-theme-accent data-[state=checked]:border-theme-accent"
                                    />
                                    <div className="min-w-0">
                                        <div className="flex flex-wrap items-center gap-2">
                                            <span className="text-sm font-medium text-theme-text truncate">{draft.name}</span>
                                            {draft.duplicate && <span className="text-[10px] px-1.5 py-0.5 rounded bg-theme-accent/20 text-theme-accent">{t('settings_view.connections.importers.duplicate')}</span>}
                                        </div>
                                        <div className="text-xs text-theme-text-muted truncate">{draft.username}@{draft.host}:{draft.port}</div>
                                        <div className="text-xs text-theme-text-muted truncate">
                                            {[draft.group, t(`settings_view.connections.importers.sources.${draft.source}`), draft.sourcePath].filter(Boolean).join(' · ')}
                                        </div>
                                        {(draft.warnings.length > 0 || draft.unsupportedFields.length > 0) && (
                                            <div className="text-xs text-amber-500 truncate">
                                                {[...draft.warnings, ...draft.unsupportedFields].join(' · ')}
                                            </div>
                                        )}
                                    </div>
                                    <div className="text-xs text-theme-text-muted text-right truncate">
                                        {draft.authType}
                                    </div>
                                </div>
                            ))}
                            {preview.drafts.length === 0 && (
                                <div className="text-center py-12 text-theme-text-muted text-sm">
                                    {t('settings_view.connections.importers.no_drafts')}
                                </div>
                            )}
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
}
