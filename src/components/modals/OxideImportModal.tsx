// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { X, AlertTriangle, CheckCircle, CheckSquare, Square } from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogClose } from '../ui/dialog';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import {
  importOxideWithClientState,
  previewOxideImport,
  validateOxideFile,
  type OxideImportProgress,
} from '../../lib/oxideClientState';
import { useAppStore } from '../../store/appStore';
import { usePluginStore } from '../../store/pluginStore';
import { buildOxideAppSettingsSectionValueMap, useSettingsStore } from '../../store/settingsStore';
import type { OxideMetadata, ImportResult, ImportPreview } from '../../types';

type ImportConflictStrategy = 'rename' | 'skip' | 'replace' | 'merge';
type ImportOperation = 'preview' | 'import' | null;
type ImportStage = 'idle' | 'parsing' | 'decrypting' | 'analyzing' | 'applying' | 'saving' | 'done';

function mapImportProgressStage(stage: string): ImportStage {
  switch (stage) {
    case 'parsing_file':
      return 'parsing';
    case 'deriving_key':
    case 'decrypting_payload':
    case 'deserializing_payload':
    case 'verifying_checksum':
      return 'decrypting';
    case 'collecting_existing':
    case 'building_preview':
    case 'analyzing_preview':
    case 'filtering_selection':
    case 'preparing_connections':
      return 'analyzing';
    case 'applying_connections':
      return 'applying';
    case 'saving_config':
      return 'saving';
    default:
      return 'decrypting';
  }
}

function getFallbackImportPercent(stage: ImportStage, operation: ImportOperation): number {
  switch (stage) {
    case 'parsing':
      return 8;
    case 'decrypting':
      return 32;
    case 'analyzing':
      return operation === 'preview' ? 72 : 58;
    case 'applying':
      return 82;
    case 'saving':
      return 96;
    case 'done':
      return 100;
    default:
      return 0;
  }
}

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
  const [activeOperation, setActiveOperation] = useState<ImportOperation>(null);
  const [progressStage, setProgressStage] = useState<ImportStage>('idle');
  const [progressDetail, setProgressDetail] = useState<OxideImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [selectedNames, setSelectedNames] = useState<Set<string>>(new Set());
  const [selectedAppSettingsSections, setSelectedAppSettingsSections] = useState<Set<string>>(new Set());
  const [importPluginSettings, setImportPluginSettings] = useState(true);
  const [selectedPluginIds, setSelectedPluginIds] = useState<Set<string>>(new Set());
  const [importForwards, setImportForwards] = useState(true);
  const [conflictStrategy, setConflictStrategy] = useState<ImportConflictStrategy>('rename');
  const [expandedAppSettingsSections, setExpandedAppSettingsSections] = useState<Set<string>>(new Set());
  const plugins = usePluginStore((state) => state.plugins);
  const currentSettings = useSettingsStore((state) => state.settings);

  const getSelectableNames = (nextPreview: ImportPreview) => new Set([
    ...nextPreview.unchanged,
    ...nextPreview.willRename.map(([original]) => original),
    ...nextPreview.willSkip,
    ...nextPreview.willReplace,
    ...nextPreview.willMerge,
  ]);

  const getSelectablePluginIds = (nextPreview: ImportPreview) => new Set(
    Object.keys(nextPreview.pluginSettingsByPlugin ?? {}),
  );

  const getSelectableAppSettingsSectionIds = (nextPreview: ImportPreview) => new Set(
    (nextPreview.appSettingsSections ?? []).map((section) => section.id),
  );

  const totalSelectable = preview
    ? preview.unchanged.length
      + preview.willRename.length
      + preview.willSkip.length
      + preview.willReplace.length
      + preview.willMerge.length
    : 0;

  const pluginEntries = preview ? Object.entries(preview.pluginSettingsByPlugin ?? {}) : [];
  const hasStructuredPluginPreview = pluginEntries.length > 0;
  const hasStructuredForwardPreview = Boolean(preview?.forwardDetails?.length);
  const importAppSettings = selectedAppSettingsSections.size > 0;
  const currentAppSettingsBySection = useMemo(() => buildOxideAppSettingsSectionValueMap(
    currentSettings,
    (preview?.appSettingsSections ?? []).map((section) => section.id),
  ), [currentSettings, preview]);
  const hasAnySelectedContent = Boolean(
    selectedNames.size > 0
    || (preview?.hasAppSettings && importAppSettings)
    || ((preview?.pluginSettingsCount ?? 0) > 0 && importPluginSettings)
    || ((preview?.totalForwards ?? 0) > 0 && importForwards),
  );
  const progressPercent = progressDetail && progressDetail.total > 0
    ? Math.round((progressDetail.current / progressDetail.total) * 100)
    : getFallbackImportPercent(progressStage, activeOperation);

  const progressLabel = (() => {
    switch (progressStage) {
      case 'parsing':
        return t('modals.import.stage_parsing');
      case 'decrypting':
        return t('modals.import.stage_decrypting');
      case 'analyzing':
        return activeOperation === 'import'
          ? t('modals.import.stage_preparing')
          : t('modals.import.stage_analyzing');
      case 'applying':
        return t('modals.import.stage_applying');
      case 'saving':
        return t('modals.import.stage_saving');
      case 'done':
        return t('modals.import.stage_done');
      default:
        return activeOperation === 'import'
          ? t('modals.import.importing')
          : t('modals.import.previewing');
    }
  })();

  const progressSummary = progressDetail
    ? `${progressDetail.current}/${progressDetail.total}`
    : null;

  const handleImportProgress = (operation: Exclude<ImportOperation, null>) => (progress: OxideImportProgress) => {
    setActiveOperation(operation);
    setProgressStage(mapImportProgressStage(progress.stage));
    setProgressDetail(progress);
  };

  const resetImportState = () => {
    setPassword('');
    setPreview(null);
    setError(null);
    setResult(null);
    setSelectedNames(new Set());
    setSelectedAppSettingsSections(new Set());
    setImportPluginSettings(true);
    setSelectedPluginIds(new Set());
    setImportForwards(true);
    setConflictStrategy('rename');
    setExpandedAppSettingsSections(new Set());
    setActiveOperation(null);
    setProgressStage('idle');
    setProgressDetail(null);
  };

  const formatPluginName = (pluginId: string) => {
    const pluginName = plugins.get(pluginId)?.manifest.name;
    return pluginName && pluginName !== pluginId
      ? `${pluginName} (${pluginId})`
      : pluginId;
  };

  const formatAppSettingsSectionLabel = (sectionId: string) => {
    switch (sectionId) {
      case 'general':
        return t('settings_view.general.title');
      case 'terminalAppearance':
        return t('modals.export.app_settings_section_terminal_appearance');
      case 'terminalBehavior':
        return t('modals.export.app_settings_section_terminal_behavior');
      case 'appearance':
        return t('settings_view.appearance.title');
      case 'connections':
        return t('settings_view.connections.title');
      case 'fileAndEditor':
        return t('modals.export.app_settings_section_file_editor');
      case 'localTerminal':
        return t('settings_view.local_terminal.title');
      case 'legacy':
        return t('modals.import.app_settings_legacy_title');
      default:
        return sectionId;
    }
  };

  const formatAppSettingsFieldLabel = (fieldKey: string) => {
    switch (fieldKey) {
      case 'language':
        return t('settings_view.general.language');
      case 'updateChannel':
        return t('settings_view.general.update_channel');
      case 'theme':
        return t('settings_view.appearance.theme');
      case 'fontFamily':
        return t('settings_view.terminal.font_family');
      case 'customFontFamily':
        return t('settings_view.terminal.custom_font_stack');
      case 'fontSize':
        return t('settings_view.terminal.font_size');
      case 'lineHeight':
        return t('settings_view.terminal.line_height');
      case 'cursorStyle':
        return t('settings_view.terminal.cursor_style');
      case 'cursorBlink':
        return t('settings_view.terminal.cursor_blink');
      case 'backgroundEnabled':
        return t('settings_view.terminal.bg_enabled');
      case 'backgroundImage':
        return t('settings_view.terminal.bg_label');
      case 'backgroundOpacity':
        return t('settings_view.terminal.bg_opacity');
      case 'backgroundBlur':
        return t('settings_view.terminal.bg_blur');
      case 'backgroundFit':
        return t('settings_view.terminal.bg_fit');
      case 'backgroundEnabledTabs':
        return t('settings_view.terminal.bg_tabs');
      case 'scrollback':
        return t('settings_view.terminal.scrollback');
      case 'renderer':
        return t('settings_view.terminal.renderer');
      case 'adaptiveRenderer':
        return t('settings_view.terminal.adaptive_renderer');
      case 'showFpsOverlay':
        return t('settings_view.terminal.show_fps_overlay');
      case 'pasteProtection':
        return t('settings_view.terminal.paste_protection');
      case 'smartCopy':
        return t('settings_view.terminal.smart_copy');
      case 'osc52Clipboard':
        return t('settings_view.terminal.osc52_clipboard');
      case 'sidebarCollapsedDefault':
        return t('modals.settings.appearance.sidebar_collapse');
      case 'uiDensity':
        return t('settings_view.appearance.density');
      case 'borderRadius':
        return t('settings_view.appearance.border_radius');
      case 'uiFontFamily':
        return t('settings_view.appearance.ui_font');
      case 'animationSpeed':
        return t('settings_view.appearance.animation');
      case 'frostedGlass':
        return t('settings_view.appearance.frosted_glass');
      case 'connectionDefaults.username':
        return t('settings_view.connections.default_username');
      case 'connectionDefaults.port':
        return t('settings_view.connections.default_port');
      case 'reconnect.enabled':
        return t('settings_view.reconnect.enabled');
      case 'reconnect.maxAttempts':
        return t('settings_view.reconnect.max_attempts');
      case 'reconnect.baseDelayMs':
        return t('settings_view.reconnect.base_delay');
      case 'reconnect.maxDelayMs':
        return t('settings_view.reconnect.max_delay');
      case 'connectionPool.idleTimeoutSecs':
        return t('settings_view.connections.idle_timeout.label');
      case 'sftp.maxConcurrentTransfers':
        return t('settings_view.sftp.concurrent');
      case 'sftp.speedLimitEnabled':
        return t('settings_view.sftp.bandwidth');
      case 'sftp.speedLimitKBps':
        return t('settings_view.sftp.speed_limit');
      case 'sftp.conflictAction':
        return t('settings_view.sftp.conflict');
      case 'ide.autoSave':
        return t('settings_view.ide.auto_save');
      case 'ide.fontSize':
        return t('settings_view.ide.font_size');
      case 'ide.lineHeight':
        return t('settings_view.ide.line_height');
      case 'ide.agentMode':
        return t('settings_view.ide.agent_mode_label');
      case 'ide.wordWrap':
        return t('settings_view.ide.word_wrap');
      case 'defaultShellId':
        return t('settings_view.local_terminal.default_shell');
      case 'recentShellIds':
        return t('modals.import.app_settings_field_recent_shells');
      case 'defaultCwd':
        return t('settings_view.local_terminal.default_cwd');
      case 'loadShellProfile':
        return t('settings_view.local_terminal.load_shell_profile');
      case 'ohMyPoshEnabled':
        return t('settings_view.local_terminal.oh_my_posh_enable');
      case 'ohMyPoshTheme':
        return t('settings_view.local_terminal.oh_my_posh_theme');
      case 'customEnvVars':
        return t('settings_view.local_terminal.custom_env');
      default:
        return fieldKey;
    }
  };

  const buildAppSettingsDiffEntries = (sectionId: string, importedValues?: Record<string, string>) => {
    if (!importedValues) {
      return [] as Array<{ key: string; label: string; current: string; imported: string }>;
    }

    const currentValues = currentAppSettingsBySection[sectionId] ?? {};
    const keys = Array.from(new Set([
      ...Object.keys(currentValues),
      ...Object.keys(importedValues),
    ])).sort();

    return keys
      .map((key) => ({
        key,
        label: formatAppSettingsFieldLabel(key),
        current: currentValues[key] ?? '—',
        imported: importedValues[key] ?? '—',
      }))
      .filter((entry) => entry.current !== entry.imported);
  };

  const handleSelectFile = async () => {
    setError(null);
    setResult(null);
    setPreview(null);
    setSelectedNames(new Set());
    setSelectedAppSettingsSections(new Set());
    setImportPluginSettings(true);
    setSelectedPluginIds(new Set());
    setImportForwards(true);

    try {
      const selected = await open({
        filters: [{ name: 'Oxide Config', extensions: ['oxide'] }],
        multiple: false,
      });

      if (selected && typeof selected === 'string') {
        const data = await readFile(selected);
        setFileData(data);

        try {
          const meta: OxideMetadata = await validateOxideFile(data);
          setMetadata(meta);
        } catch (err) {
          console.error('File validation failed:', err);
          setError(`Invalid .oxide file: ${err}`);
          setFileData(null);
          setMetadata(null);
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
    setActiveOperation('preview');
    setProgressStage('parsing');
    setProgressDetail(null);

    try {
      const previewResult: ImportPreview = await previewOxideImport(fileData, password, {
        conflictStrategy,
        onProgress: handleImportProgress('preview'),
      });
      setPreview(previewResult);
      setSelectedNames(getSelectableNames(previewResult));
      setSelectedAppSettingsSections(getSelectableAppSettingsSectionIds(previewResult));
      setSelectedPluginIds(getSelectablePluginIds(previewResult));
      setExpandedAppSettingsSections(new Set());
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
      setProgressStage('done');
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
    setActiveOperation('import');
    setProgressStage('parsing');
    setProgressDetail(null);

    try {
      const importResult: ImportResult = await importOxideWithClientState(fileData, password, {
        selectedNames: Array.from(selectedNames),
        conflictStrategy,
        importAppSettings,
        selectedAppSettingsSections: Array.from(selectedAppSettingsSections),
        importPluginSettings,
        selectedPluginIds: importPluginSettings
          ? (hasStructuredPluginPreview ? Array.from(selectedPluginIds) : undefined)
          : [],
        importForwards,
        onProgress: handleImportProgress('import'),
      });

      setResult(importResult);
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
      setProgressStage('done');
      setImporting(false);
    }
  };

  const toggleName = (name: string) => {
    setSelectedNames((prev) => {
      const next = new Set(prev);
      if (next.has(name)) {
        next.delete(name);
      } else {
        next.add(name);
      }
      return next;
    });
  };

  const toggleAll = () => {
    if (!preview) {
      return;
    }

    const allNames = getSelectableNames(preview);
    if (selectedNames.size === allNames.size) {
      setSelectedNames(new Set());
    } else {
      setSelectedNames(allNames);
    }
  };

  const toggleImportPluginSettings = () => {
    if (!preview) {
      return;
    }

    if (importPluginSettings) {
      setImportPluginSettings(false);
      return;
    }

    if (selectedPluginIds.size === 0) {
      setSelectedPluginIds(getSelectablePluginIds(preview));
    }
    setImportPluginSettings(true);
  };

  const toggleAllAppSettingsSections = () => {
    if (!preview) {
      return;
    }

    const allSections = getSelectableAppSettingsSectionIds(preview);
    if (selectedAppSettingsSections.size === allSections.size) {
      setSelectedAppSettingsSections(new Set());
      setExpandedAppSettingsSections(new Set());
    } else {
      setSelectedAppSettingsSections(allSections);
    }
  };

  const toggleAppSettingsSection = (sectionId: string) => {
    setSelectedAppSettingsSections((prev) => {
      const next = new Set(prev);
      if (next.has(sectionId)) {
        next.delete(sectionId);
      } else {
        next.add(sectionId);
      }
      return next;
    });
  };

  const toggleAppSettingsDiff = (sectionId: string) => {
    setExpandedAppSettingsSections((prev) => {
      const next = new Set(prev);
      if (next.has(sectionId)) {
        next.delete(sectionId);
      } else {
        next.add(sectionId);
      }
      return next;
    });
  };

  const togglePluginId = (pluginId: string) => {
    setSelectedPluginIds((prev) => {
      const next = new Set(prev);
      if (next.has(pluginId)) {
        next.delete(pluginId);
      } else {
        next.add(pluginId);
      }

      setImportPluginSettings(next.size > 0);
      return next;
    });
  };

  const handleClose = () => {
    setFileData(null);
    setMetadata(null);
    resetImportState();
    onClose();
  };

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="max-w-2xl max-h-[85vh] gap-0 bg-theme-bg-elevated border-theme-border text-theme-text p-0 overflow-hidden flex flex-col">
        <DialogHeader className="flex flex-row items-center justify-between border-b border-theme-border px-6 py-4">
          <DialogTitle className="text-xl font-semibold text-theme-text-heading">{t('modals.import.title')}</DialogTitle>
          <DialogClose className="rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none data-[state=open]:bg-accent data-[state=open]:text-muted-foreground">
            <X className="h-4 w-4" />
            <span className="sr-only">{t('modals.import.close')}</span>
          </DialogClose>
        </DialogHeader>

        <div className="p-6 space-y-4 overflow-y-auto flex-1 min-h-0">
          {!fileData ? (
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
                {result.importedAppSettings && (
                  <p className="text-sm mt-1">{t('modals.import.imported_app_settings')}</p>
                )}
                {preview?.hasAppSettings && !importAppSettings && (
                  <p className="text-sm mt-1">{t('modals.import.skipped_app_settings')}</p>
                )}
                {result.importedPluginSettings > 0 && (
                  <p className="text-sm mt-1">{t('modals.import.imported_plugin_settings', { count: result.importedPluginSettings })}</p>
                )}
                {(preview?.pluginSettingsCount ?? 0) > 0 && !importPluginSettings && (
                  <p className="text-sm mt-1">{t('modals.import.skipped_plugin_settings')}</p>
                )}
                {(preview?.totalForwards ?? 0) > 0 && !importForwards && (
                  <p className="text-sm mt-1">{t('modals.import.skipped_forwards')}</p>
                )}
                {result.replaced > 0 && (
                  <p className="text-sm mt-1">{t('modals.import.replaced', { count: result.replaced })}</p>
                )}
                {result.renamed > 0 && (
                  <div className="mt-2">
                    <p className="text-sm font-semibold text-yellow-400">{t('modals.import.renamed', { count: result.renamed })}</p>
                    <ul className="text-xs mt-1 space-y-1 opacity-90 max-h-24 overflow-y-auto">
                      {result.renames.map(([original, renamed], index) => (
                        <li key={index}>• "{original}" → "{renamed}"</li>
                      ))}
                    </ul>
                  </div>
                )}
                {result.errors.length > 0 && (
                  <div className="mt-2">
                    <p className="text-sm font-semibold">{t('modals.import.errors')}</p>
                    <ul className="text-xs mt-1 space-y-1 opacity-90">
                      {result.errors.map((item, index) => (
                        <li key={index}>• {item}</li>
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
            <>
              {importing && (
                <div className="border border-theme-border rounded-md p-4 space-y-3 bg-theme-bg mb-4">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <p className="text-sm font-semibold text-theme-text">{progressLabel}</p>
                      {progressSummary && (
                        <p className="text-xs text-theme-text-muted mt-1">{progressSummary}</p>
                      )}
                    </div>
                    <span className="text-sm font-semibold text-theme-accent">{progressPercent}%</span>
                  </div>
                  <div className="h-2 w-full overflow-hidden rounded-full bg-theme-bg-hover">
                    <div
                      className="h-full rounded-full bg-theme-accent transition-all duration-200"
                      style={{ width: `${progressPercent}%` }}
                    />
                  </div>
                </div>
              )}

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

                {preview.unchanged.length > 0 && (
                  <div>
                    <p className="text-sm font-semibold text-green-500">
                      {t('modals.import.preview_unchanged', { count: preview.unchanged.length })}
                    </p>
                    <ul className="text-xs text-theme-text-muted mt-1 space-y-1 max-h-20 overflow-y-auto">
                      {preview.unchanged.map((name, index) => (
                        <li
                          key={index}
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

                {preview.willRename.length > 0 && (
                  <div>
                    <div className="flex items-center gap-2">
                      <AlertTriangle className="h-4 w-4 text-yellow-500" />
                      <p className="text-sm font-semibold text-yellow-500">
                        {t('modals.import.preview_will_rename', { count: preview.willRename.length })}
                      </p>
                    </div>
                    <ul className="text-xs text-yellow-400 mt-1 space-y-1 max-h-24 overflow-y-auto">
                      {preview.willRename.map(([original, renamed], index) => (
                        <li
                          key={index}
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
                      {preview.willMerge.map((name, index) => (
                        <li
                          key={index}
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
                      {preview.willReplace.map((name, index) => (
                        <li
                          key={index}
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
                      {preview.willSkip.map((name, index) => (
                        <li
                          key={index}
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

                {preview.hasAppSettings && (
                  <div className="rounded-md border border-theme-border bg-theme-bg-elevated/60 p-3">
                    <button
                      type="button"
                      onClick={toggleAllAppSettingsSections}
                      className="flex w-full items-start gap-2 text-left"
                    >
                      {importAppSettings
                        ? <CheckSquare className="mt-0.5 h-4 w-4 flex-shrink-0 text-theme-accent" />
                        : <Square className="mt-0.5 h-4 w-4 flex-shrink-0 text-theme-text-muted" />}
                      <div>
                        <p className="text-sm font-semibold text-theme-text">{t('modals.import.section_app_settings')}</p>
                        <p className="text-xs text-theme-text-muted">{t('modals.import.toggle_app_settings')}</p>
                        {!importAppSettings && (
                          <p className="mt-1 text-xs text-yellow-400">{t('modals.import.skipped_app_settings')}</p>
                        )}
                      </div>
                    </button>

                    {(preview.appSettingsSections?.length ?? 0) > 0 && (
                      <div className="mt-3 border-t border-theme-border pt-3 space-y-2">
                        <p className="text-xs font-semibold text-theme-text">
                          {t('modals.import.app_settings_sections_title', { count: preview.appSettingsSections?.length ?? 0 })}
                        </p>

                        <div className="space-y-2">
                          {(preview.appSettingsSections ?? []).map((section) => {
                            const diffEntries = buildAppSettingsDiffEntries(section.id, section.fieldValues);
                            const isExpanded = expandedAppSettingsSections.has(section.id);
                            const isSelected = selectedAppSettingsSections.has(section.id);

                            return (
                              <div key={section.id} className="rounded-md border border-theme-border bg-theme-bg/60 p-3 space-y-2">
                                <button
                                  type="button"
                                  onClick={() => toggleAppSettingsSection(section.id)}
                                  className="flex w-full items-start justify-between gap-3 text-left"
                                >
                                  <span className="flex items-start gap-2 text-theme-text">
                                    {isSelected
                                      ? <CheckSquare className="mt-0.5 h-4 w-4 flex-shrink-0 text-theme-accent" />
                                      : <Square className="mt-0.5 h-4 w-4 flex-shrink-0 text-theme-text-muted" />}
                                    <span>
                                      <span className="block text-sm font-semibold text-theme-text">{formatAppSettingsSectionLabel(section.id)}</span>
                                      <span className="mt-1 block text-xs text-theme-text-muted">
                                        {section.id === 'legacy'
                                          ? t('modals.import.app_settings_legacy_description')
                                          : t('modals.import.app_settings_keys', {
                                              keys: section.fieldKeys.map(formatAppSettingsFieldLabel).join(', '),
                                            })}
                                      </span>
                                      {section.containsEnvVars && (
                                        <span className="mt-1 block text-xs text-yellow-400">{t('modals.import.app_settings_contains_env_vars')}</span>
                                      )}
                                    </span>
                                  </span>
                                  <span className="text-xs text-theme-text-muted">
                                    {t('modals.import.plugin_settings_items', { count: section.fieldKeys.length })}
                                  </span>
                                </button>

                                {section.id !== 'legacy' && Object.keys(section.fieldValues ?? {}).length > 0 && (
                                  <div className="space-y-2 border-t border-theme-border pt-2">
                                    <button
                                      type="button"
                                      onClick={() => toggleAppSettingsDiff(section.id)}
                                      className="text-xs text-theme-accent hover:text-theme-accent-hover transition-colors"
                                    >
                                      {isExpanded
                                        ? t('modals.import.app_settings_hide_changes')
                                        : t('modals.import.app_settings_view_changes')}
                                    </button>

                                    {isExpanded && (
                                      diffEntries.length > 0 ? (
                                        <div className="space-y-2">
                                          <p className="text-xs font-semibold text-theme-text">{t('modals.import.app_settings_diff_title')}</p>
                                          <ul className="space-y-2 max-h-40 overflow-y-auto text-xs">
                                            {diffEntries.map((entry) => (
                                              <li key={`${section.id}-${entry.key}`} className="rounded-md bg-theme-bg-elevated/60 px-2 py-2 text-theme-text-muted">
                                                <p className="font-medium text-theme-text">{entry.label}</p>
                                                <p>{t('modals.import.app_settings_current')}: {entry.current}</p>
                                                <p>{t('modals.import.app_settings_imported')}: {entry.imported}</p>
                                              </li>
                                            ))}
                                          </ul>
                                        </div>
                                      ) : (
                                        <p className="text-xs text-theme-text-muted">{t('modals.import.app_settings_diff_empty')}</p>
                                      )
                                    )}
                                  </div>
                                )}
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    )}
                  </div>
                )}

                {preview.pluginSettingsCount > 0 && (
                  hasStructuredPluginPreview ? (
                    <div className="rounded-md border border-theme-border bg-theme-bg-elevated/60 p-3 space-y-3">
                      <button
                        type="button"
                        onClick={toggleImportPluginSettings}
                        className="flex w-full items-start gap-2 text-left"
                      >
                        {importPluginSettings
                          ? <CheckSquare className="mt-0.5 h-4 w-4 flex-shrink-0 text-theme-accent" />
                          : <Square className="mt-0.5 h-4 w-4 flex-shrink-0 text-theme-text-muted" />}
                        <div>
                          <p className="text-sm font-semibold text-theme-text">
                            {t('modals.import.section_plugin_settings', { count: pluginEntries.length })}
                          </p>
                          <p className="text-xs text-theme-text-muted">{t('modals.import.toggle_plugin_settings')}</p>
                          {!importPluginSettings && (
                            <p className="mt-1 text-xs text-yellow-400">{t('modals.import.skipped_plugin_settings')}</p>
                          )}
                        </div>
                      </button>

                      <div className="space-y-1">
                        {pluginEntries.map(([pluginId, count]) => (
                          <button
                            key={pluginId}
                            type="button"
                            onClick={() => togglePluginId(pluginId)}
                            disabled={!importPluginSettings && !selectedPluginIds.has(pluginId)}
                            className="flex w-full items-center justify-between rounded-md px-2 py-1.5 text-left text-xs transition-colors hover:bg-theme-bg disabled:cursor-not-allowed disabled:opacity-60"
                          >
                            <span className="flex items-center gap-2 text-theme-text">
                              {selectedPluginIds.has(pluginId)
                                ? <CheckSquare className="h-3.5 w-3.5 flex-shrink-0 text-theme-accent" />
                                : <Square className="h-3.5 w-3.5 flex-shrink-0 text-theme-text-muted" />}
                              <span>{formatPluginName(pluginId)}</span>
                            </span>
                            <span className="text-theme-text-muted">
                              {t('modals.import.plugin_settings_items', { count })}
                            </span>
                          </button>
                        ))}
                      </div>
                    </div>
                  ) : (
                    <div className="bg-blue-500/10 border border-blue-500/20 text-blue-500 px-3 py-2 rounded text-xs">
                      {t('modals.import.preview_plugin_settings', { count: preview.pluginSettingsCount })}
                    </div>
                  )
                )}

                {preview.totalForwards > 0 && (
                  hasStructuredForwardPreview ? (
                    <div className="rounded-md border border-theme-border bg-theme-bg-elevated/60 p-3 space-y-3">
                      <button
                        type="button"
                        onClick={() => setImportForwards((prev) => !prev)}
                        className="flex w-full items-start gap-2 text-left"
                      >
                        {importForwards
                          ? <CheckSquare className="mt-0.5 h-4 w-4 flex-shrink-0 text-theme-accent" />
                          : <Square className="mt-0.5 h-4 w-4 flex-shrink-0 text-theme-text-muted" />}
                        <div>
                          <p className="text-sm font-semibold text-theme-text">
                            {t('modals.import.section_forwards', { count: preview.totalForwards })}
                          </p>
                          <p className="text-xs text-theme-text-muted">{t('modals.import.toggle_forwards')}</p>
                          {!importForwards && (
                            <p className="mt-1 text-xs text-yellow-400">{t('modals.import.skipped_forwards')}</p>
                          )}
                        </div>
                      </button>

                      <ul className="space-y-1 max-h-28 overflow-y-auto text-xs text-theme-text-muted">
                        {preview.forwardDetails.map((detail, index) => (
                          <li key={`${detail.ownerConnectionName}-${detail.direction}-${index}`} className="rounded-md px-2 py-1.5 bg-theme-bg/60">
                            <span className="font-medium text-theme-text">{detail.ownerConnectionName}</span>
                            {' · '}
                            <span>{detail.description}</span>
                          </li>
                        ))}
                      </ul>
                    </div>
                  ) : (
                    <div className="bg-blue-500/10 border border-blue-500/20 text-blue-500 px-3 py-2 rounded text-xs">
                      {t('modals.import.preview_forwards', { count: preview.totalForwards })}
                    </div>
                  )
                )}

                {preview.hasEmbeddedKeys && (
                  <div className="bg-blue-500/10 border border-blue-500/20 text-blue-500 px-3 py-2 rounded text-xs">
                    {t('modals.import.preview_embedded_keys')}
                  </div>
                )}
              </div>

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
                  disabled={importing || !hasAnySelectedContent}
                  className="bg-theme-accent text-white hover:bg-theme-accent-hover disabled:opacity-50"
                >
                  {importing ? t('modals.import.importing') : t('modals.import.confirm_import')}
                </Button>
              </div>
            </>
          ) : (
            <>
              {previewing && (
                <div className="border border-theme-border rounded-md p-4 space-y-3 bg-theme-bg">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <p className="text-sm font-semibold text-theme-text">{progressLabel}</p>
                      {progressSummary && (
                        <p className="text-xs text-theme-text-muted mt-1">{progressSummary}</p>
                      )}
                    </div>
                    <span className="text-sm font-semibold text-theme-accent">{progressPercent}%</span>
                  </div>
                  <div className="h-2 w-full overflow-hidden rounded-full bg-theme-bg-hover">
                    <div
                      className="h-full rounded-full bg-theme-accent transition-all duration-200"
                      style={{ width: `${progressPercent}%` }}
                    />
                  </div>
                </div>
              )}

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
                    {metadata.has_app_settings && (
                      <p><span className="text-theme-text-muted">{t('modals.import.contains_app_settings')}</span> {t('modals.import.contains_app_settings_partial')}</p>
                    )}
                    {typeof metadata.plugin_settings_count === 'number' && metadata.plugin_settings_count > 0 && (
                      <p><span className="text-theme-text-muted">{t('modals.import.contains_plugin_settings')}</span> {t('modals.import.plugin_settings_count', { count: metadata.plugin_settings_count })}</p>
                    )}
                  </div>

                  <p className="mt-3 text-xs text-theme-text-muted">
                    {t('modals.import.partial_import_hint')}
                  </p>

                  <div className="mt-3">
                    <p className="text-sm font-semibold text-theme-text">{t('modals.import.connection_list')}</p>
                    <ul className="text-xs text-theme-text-muted mt-1 space-y-1 max-h-32 overflow-y-auto">
                      {metadata.connection_names.map((name, index) => (
                        <li key={index}>• {name}</li>
                      ))}
                    </ul>
                  </div>
                </div>
              )}

              <div>
                <Label className="text-theme-text">{t('modals.import.password')}</Label>
                <Input
                  type="password"
                  placeholder={t('modals.import.password_placeholder')}
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && password) {
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

              {error && (
                <div className="bg-red-500/10 border border-red-500/20 text-red-500 px-3 py-2 rounded text-sm">
                  {error}
                </div>
              )}

              <div className="bg-yellow-500/10 border border-yellow-500/20 text-yellow-500 px-3 py-2 rounded text-sm">
                <p className="font-semibold">{t('modals.import.warning_title')}</p>
                <p className="text-xs mt-1 opacity-90">
                  {t('modals.import.warning_text')}
                </p>
                <p className="text-xs mt-1 opacity-90">
                  {t('modals.import.passwords_not_included')}
                </p>
              </div>

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