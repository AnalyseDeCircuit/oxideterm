// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState, useEffect, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { X, AlertTriangle, Key, Lock, Shield, FileKey, Loader2, Sparkles } from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogClose } from '../ui/dialog';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Checkbox } from '../ui/checkbox';
import { Label } from '../ui/label';
import { useAppStore } from '../../store/appStore';
import { exportOxideWithClientState, type OxideExportProgress } from '../../lib/oxideClientState';
import { api } from '../../lib/api';
import { collectPluginSettingsSnapshot, parseSettingStorageKey } from '../../lib/plugin/pluginSettingsManager';
import {
  getDefaultOxideAppSettingsExportSections,
  type OxideAppSettingsSectionId,
} from '../../store/settingsStore';
import type { ExportPreflightResult, PersistedForwardInfo } from '../../types';

type OxideExportModalProps = {
  isOpen: boolean;
  onClose: () => void;
};

type ExportStage = 'idle' | 'reading_keys' | 'encrypting' | 'writing' | 'done';
type PasswordStrength = 'weak' | 'fair' | 'strong';

function mapExportProgressStage(stage: string, embedKeys: boolean): ExportStage {
  switch (stage) {
    case 'collecting_connections':
      return embedKeys ? 'reading_keys' : 'encrypting';
    case 'serializing_file':
      return 'writing';
    case 'computing_checksum':
    case 'building_metadata':
    case 'generating_salt_nonce':
    case 'deriving_key':
    case 'serializing_payload':
    case 'encrypting_payload':
    case 'finalizing_file':
      return 'encrypting';
    default:
      return 'encrypting';
  }
}

function getFallbackProgressPercent(stage: ExportStage): number {
  switch (stage) {
    case 'reading_keys':
      return 12;
    case 'encrypting':
      return 40;
    case 'writing':
      return 96;
    case 'done':
      return 100;
    default:
      return 0;
  }
}

export function OxideExportModal({ isOpen, onClose }: OxideExportModalProps) {
  const { t } = useTranslation();
  const { savedConnections, loadSavedConnections } = useAppStore();
  const defaultAppSettingsSections = useMemo(
    () => getDefaultOxideAppSettingsExportSections(),
    [],
  );
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [includeAppSettings, setIncludeAppSettings] = useState(true);
  const [selectedAppSettingsSections, setSelectedAppSettingsSections] = useState<Set<OxideAppSettingsSectionId>>(
    () => new Set(defaultAppSettingsSections),
  );
  const [includeLocalTerminalEnvVars, setIncludeLocalTerminalEnvVars] = useState(false);
  const [includePluginSettings, setIncludePluginSettings] = useState(true);
  const [pluginGroups, setPluginGroups] = useState<Record<string, number>>({});
  const [selectedPluginIds, setSelectedPluginIds] = useState<Set<string>>(new Set());
  const [allSavedForwards, setAllSavedForwards] = useState<PersistedForwardInfo[]>([]);
  const [selectedForwardIds, setSelectedForwardIds] = useState<Set<string>>(new Set());
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [description, setDescription] = useState('');
  const [embedKeys, setEmbedKeys] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exportStage, setExportStage] = useState<ExportStage>('idle');
  const [exportProgress, setExportProgress] = useState<OxideExportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preflight, setPreflight] = useState<ExportPreflightResult | null>(null);
  const [preflightLoading, setPreflightLoading] = useState(false);

  const lastExportTimestamp = typeof localStorage !== 'undefined'
    ? Number(localStorage.getItem('oxideterm:lastExportTimestamp') || '0')
    : 0;

  const pluginGroupEntries = Object.entries(pluginGroups).sort(([left], [right]) => left.localeCompare(right));
  const selectedForwardOwnerIds = useMemo(
    () => allSavedForwards
      .filter((forward) => selectedForwardIds.has(forward.id) && forward.owner_connection_id)
      .map((forward) => forward.owner_connection_id as string),
    [allSavedForwards, selectedForwardIds],
  );
  const effectiveConnectionIds = useMemo(
    () => Array.from(new Set([...selectedIds, ...selectedForwardOwnerIds])),
    [selectedIds, selectedForwardOwnerIds],
  );
  const hasSelectedAppSettings = includeAppSettings && selectedAppSettingsSections.size > 0;
  const hasSelectedPluginSettings = includePluginSettings && selectedPluginIds.size > 0;
  const hasAnyContent = Boolean(
    selectedIds.length > 0
    || hasSelectedAppSettings
    || hasSelectedPluginSettings
    || selectedForwardIds.size > 0,
  );
  const selectedPluginSettingCount = pluginGroupEntries.reduce((total, [pluginId, count]) => (
    selectedPluginIds.has(pluginId) ? total + count : total
  ), 0);
  const appSettingsSectionIds = defaultAppSettingsSections;

  const formatAppSettingsSectionLabel = (sectionId: OxideAppSettingsSectionId) => {
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
    }
  };

  const selectedAppSettingsLabels = appSettingsSectionIds
    .filter((sectionId) => selectedAppSettingsSections.has(sectionId))
    .map(formatAppSettingsSectionLabel);

  const forwardGroups = allSavedForwards.reduce<Record<string, PersistedForwardInfo[]>>((groups, forward) => {
    const ownerLabel = forward.owner_connection_name || forward.owner_connection_id || '-';
    if (!groups[ownerLabel]) {
      groups[ownerLabel] = [];
    }
    groups[ownerLabel].push(forward);
    return groups;
  }, {});

  const isNewSinceLastExport = (createdAt: string): boolean => {
    if (!lastExportTimestamp) {
      return false;
    }
    return new Date(createdAt).getTime() > lastExportTimestamp;
  };

  const newConnectionCount = savedConnections.filter((connection) => isNewSinceLastExport(connection.created_at)).length;

  const loadExportSources = useCallback(async () => {
    await loadSavedConnections();

    const groupedPluginSettings = collectPluginSettingsSnapshot().reduce<Record<string, number>>((groups, entry) => {
      const parsed = parseSettingStorageKey(entry.storageKey);
      if (!parsed) {
        return groups;
      }
      groups[parsed.pluginId] = (groups[parsed.pluginId] || 0) + 1;
      return groups;
    }, {});

    setPluginGroups(groupedPluginSettings);
    setSelectedPluginIds(new Set(Object.keys(groupedPluginSettings)));

    const forwards = await api.listAllSavedForwards();
    setAllSavedForwards(forwards);
    setSelectedForwardIds(new Set(forwards.map((forward) => forward.id)));
  }, [loadSavedConnections]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    setSelectedIds([]);
    setIncludeAppSettings(true);
    setSelectedAppSettingsSections(new Set(defaultAppSettingsSections));
    setIncludeLocalTerminalEnvVars(false);
    setIncludePluginSettings(true);
    setPassword('');
    setConfirmPassword('');
    setDescription('');
    setEmbedKeys(false);
    setError(null);
    setPreflight(null);
    setExportStage('idle');
    setExportProgress(null);

    void loadExportSources();
  }, [isOpen, loadExportSources]);

  const runPreflight = useCallback(async (ids: string[], embed: boolean) => {
    if (ids.length === 0) {
      setPreflight(null);
      return;
    }

    setPreflightLoading(true);
    try {
      const result: ExportPreflightResult = await invoke('preflight_export', {
        connectionIds: ids,
        embedKeys: embed || null,
      });
      setPreflight(result);
    } catch (err) {
      console.error('Preflight check failed:', err);
    } finally {
      setPreflightLoading(false);
    }
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => {
      void runPreflight(effectiveConnectionIds, embedKeys);
    }, 300);
    return () => clearTimeout(timer);
  }, [effectiveConnectionIds, embedKeys, runPreflight]);

  const handleSelectAll = () => {
    if (selectedIds.length === savedConnections.length) {
      setSelectedIds([]);
    } else {
      setSelectedIds(savedConnections.map((connection) => connection.id));
    }
  };

  const handleToggleConnection = (id: string) => {
    setSelectedIds((prev) => (
      prev.includes(id)
        ? prev.filter((connectionId) => connectionId !== id)
        : [...prev, id]
    ));
  };

  const handleToggleForward = (id: string) => {
    setSelectedForwardIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const handleTogglePlugin = (pluginId: string) => {
    setSelectedPluginIds((prev) => {
      const next = new Set(prev);
      if (next.has(pluginId)) {
        next.delete(pluginId);
      } else {
        next.add(pluginId);
      }
      return next;
    });
  };

  const handleToggleAppSettingsSection = (sectionId: OxideAppSettingsSectionId) => {
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

  const validatePassword = (): boolean => {
    if (password.length < 6) {
      setError(t('modals.export.error_password_too_short'));
      return false;
    }

    if (password !== confirmPassword) {
      setError(t('modals.export.error_password_mismatch'));
      return false;
    }

    return true;
  };

  const getPasswordStrength = (value: string): PasswordStrength => {
    if (value.length < 8) {
      return 'weak';
    }

    const hasUpper = /[A-Z]/.test(value);
    const hasLower = /[a-z]/.test(value);
    const hasDigit = /[0-9]/.test(value);
    const hasSpecial = /[^A-Za-z0-9]/.test(value);
    const classes = [hasUpper, hasLower, hasDigit, hasSpecial].filter(Boolean).length;

    if (value.length >= 12 && classes >= 3) {
      return 'strong';
    }

    return 'fair';
  };

  const getStrengthLabel = (strength: PasswordStrength): string => {
    switch (strength) {
      case 'weak':
        return t('modals.export.password_strength_weak');
      case 'strong':
        return t('modals.export.password_strength_strong');
      default:
        return t('modals.export.password_strength_fair');
    }
  };

  const getStrengthTone = (strength: PasswordStrength, active: boolean): string => {
    if (!active) {
      return 'bg-theme-border';
    }

    switch (strength) {
      case 'weak':
        return 'bg-yellow-500';
      case 'strong':
        return 'bg-green-500';
      default:
        return 'bg-theme-accent';
    }
  };

  const passwordStrength = password ? getPasswordStrength(password) : null;
  const exportPercent = exportProgress && exportProgress.total > 0
    ? Math.round((exportProgress.current / exportProgress.total) * 100)
    : getFallbackProgressPercent(exportStage);

  const formatBytes = (bytes: number): string => {
    if (bytes < 1024) {
      return `${bytes} B`;
    }
    if (bytes < 1024 * 1024) {
      return `${(bytes / 1024).toFixed(1)} KB`;
    }
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  };

  const formatForwardSummary = (forward: PersistedForwardInfo): string => {
    const direction = forward.forward_type === 'local'
      ? 'L'
      : forward.forward_type === 'remote'
        ? 'R'
        : 'D';
    return `${direction} ${forward.bind_address}:${forward.bind_port} -> ${forward.target_host}:${forward.target_port}`;
  };

  const handleExport = async () => {
    setError(null);

    if (!hasAnyContent) {
      setError(t('modals.export.error_select_something'));
      return;
    }

    if (!validatePassword()) {
      return;
    }

    setExporting(true);
    setExportProgress(null);

    try {
      if (embedKeys && preflight && preflight.connectionsWithKeys > 0) {
        setExportStage('reading_keys');
        await new Promise((resolve) => setTimeout(resolve, 300));
      }

      setExportStage('encrypting');

      const fileData = await exportOxideWithClientState({
        connectionIds: effectiveConnectionIds,
        password,
        description: description || null,
        embedKeys: embedKeys || null,
        includeAppSettings,
        selectedAppSettingsSections: Array.from(selectedAppSettingsSections),
        includeLocalTerminalEnvVars,
        includePluginSettings: hasSelectedPluginSettings,
        selectedPluginIds: hasSelectedPluginSettings ? Array.from(selectedPluginIds) : [],
        selectedForwardIds: Array.from(selectedForwardIds),
        onProgress: (progress) => {
          setExportStage(mapExportProgressStage(progress.stage, embedKeys));
          setExportProgress(progress);
        },
      });

      setExportStage('writing');
      setExportProgress((prev) => prev ? { ...prev, current: prev.total } : { stage: 'serializing_file', current: 1, total: 1 });
      const savePath = await save({
        defaultPath: `oxide-export-${Date.now()}.oxide`,
        filters: [{ name: 'Oxide Config', extensions: ['oxide'] }],
      });

      if (savePath) {
        await writeFile(savePath, fileData);
        setExportStage('done');
        setExportProgress((prev) => prev ? { ...prev, current: prev.total } : { stage: 'serializing_file', current: 1, total: 1 });
        localStorage.setItem('oxideterm:lastExportTimestamp', String(Date.now()));
        await new Promise((resolve) => setTimeout(resolve, 500));
        onClose();
      } else {
        setExportStage('idle');
        setExportProgress(null);
      }
    } catch (err) {
      console.error('Export failed:', err);
      setError(`${t('modals.export.error_export_failed')}: ${err}`);
      setExportStage('idle');
      setExportProgress(null);
    } finally {
      setExporting(false);
    }
  };

  const getStageText = (): string => {
    switch (exportStage) {
      case 'reading_keys':
        return t('modals.export.stage_reading_keys');
      case 'encrypting':
        return t('modals.export.stage_encrypting');
      case 'writing':
        return t('modals.export.stage_writing');
      case 'done':
        return t('modals.export.stage_done');
      default:
        return t('modals.export.exporting');
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-2xl max-h-[85vh] bg-theme-bg-elevated border-theme-border text-theme-text p-0 gap-0 overflow-hidden flex flex-col">
        <DialogHeader className="flex flex-row items-center justify-between border-b border-theme-border px-6 py-4 flex-shrink-0">
          <DialogTitle className="text-xl font-semibold text-theme-text-heading">{t('modals.export.title')}</DialogTitle>
          <DialogClose className="rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none data-[state=open]:bg-accent data-[state=open]:text-muted-foreground">
            <X className="h-4 w-4" />
            <span className="sr-only">{t('modals.export.close')}</span>
          </DialogClose>
        </DialogHeader>

        <div className="p-6 space-y-4 overflow-y-auto flex-1">
          <div>
            <div className="flex items-center justify-between mb-2">
              <Label className="text-theme-text">{t('modals.export.select_connections', { selected: selectedIds.length, total: savedConnections.length })}</Label>
              <Button size="sm" variant="outline" onClick={handleSelectAll} className="h-7 text-xs border-theme-border text-theme-text hover:bg-theme-bg-hover">
                {selectedIds.length === savedConnections.length ? t('modals.export.deselect_all') : t('modals.export.select_all')}
              </Button>
            </div>

            {newConnectionCount > 0 && (
              <div className="flex items-center gap-1.5 text-xs text-green-500 mb-1">
                <Sparkles className="h-3 w-3" />
                {t('modals.export.new_since_last_export', { count: newConnectionCount })}
              </div>
            )}

            <div className="max-h-64 overflow-y-auto border border-theme-border rounded-md p-2 space-y-1 bg-theme-bg">
              {savedConnections.length === 0 ? (
                <p className="text-sm text-theme-text-muted py-4 text-center">
                  {t('modals.export.no_connections')}
                </p>
              ) : (
                savedConnections.map((connection) => (
                  <div key={connection.id} className="flex items-center space-x-2 p-2 hover:bg-theme-bg-hover rounded cursor-pointer" onClick={() => handleToggleConnection(connection.id)}>
                    <Checkbox
                      checked={selectedIds.includes(connection.id)}
                      onCheckedChange={() => handleToggleConnection(connection.id)}
                      className="border-theme-text-muted data-[state=checked]:bg-theme-accent data-[state=checked]:border-theme-accent"
                    />
                    <Label className="flex-1 cursor-pointer text-theme-text">
                      <div className="font-medium flex items-center gap-1.5">
                        {connection.name}
                        {isNewSinceLastExport(connection.created_at) && (
                          <span className="inline-flex items-center gap-0.5 text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-green-500/15 text-green-500 leading-none">
                            <Sparkles className="h-2.5 w-2.5" />
                            {t('modals.export.badge_new')}
                          </span>
                        )}
                      </div>
                      <div className="text-xs text-theme-text-muted">
                        {connection.username}@{connection.host}:{connection.port}
                        {connection.group && ` [${connection.group}]`}
                      </div>
                    </Label>
                  </div>
                ))
              )}
            </div>
          </div>

          <div className="border border-theme-border rounded-md p-3 bg-theme-bg space-y-2">
            <div className="flex items-center gap-2 text-sm font-semibold text-theme-text">
              <Shield className="h-4 w-4" />
              {t('modals.export.section_forwards', { count: allSavedForwards.length })}
            </div>
            <p className="text-xs text-theme-text-muted">
              {t('modals.export.forwards_owner_notice')}
            </p>

            {allSavedForwards.length === 0 ? (
              <p className="text-xs text-theme-text-muted">{t('modals.export.no_forwards')}</p>
            ) : (
              <div className="max-h-52 overflow-y-auto space-y-3">
                {Object.entries(forwardGroups).sort(([left], [right]) => left.localeCompare(right)).map(([ownerLabel, forwards]) => (
                  <div key={ownerLabel} className="space-y-1">
                    <p className="text-xs font-semibold text-theme-text">{ownerLabel}</p>
                    {forwards.map((forward) => (
                      <div key={forward.id} className="flex items-start space-x-2 px-1 py-1 rounded hover:bg-theme-bg-hover">
                        <Checkbox
                          checked={selectedForwardIds.has(forward.id)}
                          onCheckedChange={() => handleToggleForward(forward.id)}
                          className="mt-0.5 border-theme-text-muted data-[state=checked]:bg-theme-accent data-[state=checked]:border-theme-accent"
                        />
                        <div className="text-xs text-theme-text">
                          <div>{forward.description || formatForwardSummary(forward)}</div>
                          <div className="text-theme-text-muted">{formatForwardSummary(forward)}</div>
                        </div>
                      </div>
                    ))}
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="space-y-2">
            <div className="flex items-start space-x-2">
              <Checkbox
                id="includeAppSettings"
                checked={includeAppSettings}
                onCheckedChange={(checked) => setIncludeAppSettings(checked === true)}
                className="mt-0.5 border-theme-text-muted data-[state=checked]:bg-theme-accent data-[state=checked]:border-theme-accent"
              />
              <div className="flex flex-col">
                <Label htmlFor="includeAppSettings" className="cursor-pointer text-theme-text">
                  {t('modals.export.include_app_settings')}
                </Label>
                <p className="text-xs text-theme-text-muted mt-0.5">
                  {t('modals.export.include_app_settings_description')}
                </p>
              </div>
            </div>

            {includeAppSettings && (
              <div className="border border-theme-border rounded-md p-3 bg-theme-bg space-y-3">
                <div>
                  <p className="text-sm font-semibold text-theme-text">{t('modals.export.app_settings_sections_title')}</p>
                  <p className="text-xs text-theme-text-muted mt-1">{t('modals.export.app_settings_sections_hint')}</p>
                </div>

                <div className="space-y-2">
                  {appSettingsSectionIds.map((sectionId) => (
                    <div key={sectionId} className="flex items-start space-x-2">
                      <Checkbox
                        checked={selectedAppSettingsSections.has(sectionId)}
                        onCheckedChange={() => handleToggleAppSettingsSection(sectionId)}
                        className="mt-0.5 border-theme-text-muted data-[state=checked]:bg-theme-accent data-[state=checked]:border-theme-accent"
                      />
                      <div className="flex flex-col">
                        <Label className="cursor-pointer text-theme-text">
                          {formatAppSettingsSectionLabel(sectionId)}
                        </Label>
                      </div>
                    </div>
                  ))}
                </div>

                {selectedAppSettingsSections.has('localTerminal') && (
                  <div className="rounded-md border border-theme-border bg-theme-bg-elevated/60 p-3">
                    <div className="flex items-start space-x-2">
                      <Checkbox
                        id="includeLocalTerminalEnvVars"
                        checked={includeLocalTerminalEnvVars}
                        onCheckedChange={(checked) => setIncludeLocalTerminalEnvVars(checked === true)}
                        className="mt-0.5 border-theme-text-muted data-[state=checked]:bg-theme-accent data-[state=checked]:border-theme-accent"
                      />
                      <div className="flex flex-col">
                        <Label htmlFor="includeLocalTerminalEnvVars" className="cursor-pointer text-theme-text">
                          {t('modals.export.app_settings_include_env_vars')}
                        </Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">
                          {t('modals.export.app_settings_include_env_vars_description')}
                        </p>
                      </div>
                    </div>
                  </div>
                )}

                {selectedAppSettingsSections.size === 0 && (
                  <p className="text-xs text-yellow-400">{t('modals.export.app_settings_no_sections')}</p>
                )}
              </div>
            )}

            <div className="flex items-start space-x-2">
              <Checkbox
                id="includePluginSettings"
                checked={includePluginSettings}
                onCheckedChange={(checked) => setIncludePluginSettings(checked === true)}
                className="mt-0.5 border-theme-text-muted data-[state=checked]:bg-theme-accent data-[state=checked]:border-theme-accent"
              />
              <div className="flex flex-col">
                <Label htmlFor="includePluginSettings" className="cursor-pointer text-theme-text">
                  {t('modals.export.include_plugin_settings')}
                </Label>
                <p className="text-xs text-theme-text-muted mt-0.5">
                  {t('modals.export.include_plugin_settings_description')}
                </p>
              </div>
            </div>

            <div className="border border-theme-border rounded-md p-3 bg-theme-bg space-y-2">
              {pluginGroupEntries.length === 0 ? (
                <p className="text-xs text-theme-text-muted">{t('modals.export.no_plugin_settings')}</p>
              ) : (
                pluginGroupEntries.map(([pluginId, count]) => (
                  <div key={pluginId} className="flex items-center space-x-2">
                    <Checkbox
                      checked={selectedPluginIds.has(pluginId)}
                      disabled={!includePluginSettings}
                      onCheckedChange={() => handleTogglePlugin(pluginId)}
                      className="border-theme-text-muted data-[state=checked]:bg-theme-accent data-[state=checked]:border-theme-accent"
                    />
                    <Label className="cursor-pointer text-sm text-theme-text">
                      {t('modals.export.section_plugin_by_id', { pluginId, count })}
                    </Label>
                  </div>
                ))
              )}
            </div>
          </div>

          <div>
            <div className="flex items-center gap-2 text-sm font-semibold text-theme-text mb-2">
              <Shield className="h-4 w-4" />
              {t('modals.export.summary_title')}
              {preflightLoading && <Loader2 className="h-3 w-3 animate-spin" />}
            </div>

            {effectiveConnectionIds.length > 0 && preflight && (
              <div className="border border-theme-border rounded-md p-3 bg-theme-bg space-y-2">
                <div className="grid grid-cols-3 gap-2 text-xs">
                  <div className="flex items-center gap-1.5 text-theme-text-muted">
                    <Lock className="h-3 w-3" />
                    <span>{t('modals.export.summary_passwords', { count: preflight.connectionsWithPasswords })}</span>
                  </div>
                  <div className="flex items-center gap-1.5 text-theme-text-muted">
                    <Key className="h-3 w-3" />
                    <span>{t('modals.export.summary_keys', { count: preflight.connectionsWithKeys })}</span>
                  </div>
                  <div className="flex items-center gap-1.5 text-theme-text-muted">
                    <FileKey className="h-3 w-3" />
                    <span>{t('modals.export.summary_agent', { count: preflight.connectionsWithAgent })}</span>
                  </div>
                </div>

                {preflight.connectionsWithPasswords > 0 && (
                  <div className="bg-yellow-500/10 border border-yellow-500/20 text-yellow-500 px-2 py-1.5 rounded text-xs">
                    {t('modals.export.warning_passwords_excluded', { count: preflight.connectionsWithPasswords })}
                  </div>
                )}

                {embedKeys && preflight.missingKeys.length > 0 && (
                  <div className="bg-yellow-500/10 border border-yellow-500/20 text-yellow-500 px-2 py-1.5 rounded text-xs">
                    <div className="flex items-center gap-1.5 font-semibold">
                      <AlertTriangle className="h-3 w-3" />
                      {t('modals.export.warning_missing_keys', { count: preflight.missingKeys.length })}
                    </div>
                    <ul className="mt-1 space-y-0.5 max-h-16 overflow-y-auto">
                      {preflight.missingKeys.map(([name, path], index) => (
                        <li key={`${name}-${path}-${index}`} className="opacity-80">• {name}: {path}</li>
                      ))}
                    </ul>
                  </div>
                )}

                {embedKeys && preflight.totalKeyBytes > 0 && (
                  <div className="text-xs text-theme-text-muted">
                    {t('modals.export.key_size', { size: formatBytes(preflight.totalKeyBytes) })}
                  </div>
                )}
              </div>
            )}
          </div>

          <div>
            <Label className="text-theme-text">{t('modals.export.description')}</Label>
            <Input
              placeholder={t('modals.export.description_placeholder')}
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              className="mt-1 bg-theme-bg border-theme-border text-theme-text placeholder:text-theme-text-muted focus-visible:ring-theme-accent"
            />
          </div>

          <div className="flex items-start space-x-2">
            <Checkbox
              id="embedKeys"
              checked={embedKeys}
              onCheckedChange={(checked) => setEmbedKeys(checked === true)}
              className="mt-0.5 border-theme-text-muted data-[state=checked]:bg-theme-accent data-[state=checked]:border-theme-accent"
            />
            <div className="flex flex-col">
              <Label htmlFor="embedKeys" className="cursor-pointer text-theme-text">
                {t('modals.export.embed_keys')}
              </Label>
              <p className="text-xs text-theme-text-muted mt-0.5">
                {t('modals.export.embed_keys_description')}
              </p>
            </div>
          </div>

          <div className="border border-theme-border rounded-md p-3 bg-theme-bg space-y-2">
            <div className="flex items-center gap-2 text-sm font-semibold text-theme-text">
              <Shield className="h-4 w-4" />
              {t('modals.export.content_summary_title')}
            </div>
            <ul className="space-y-1 text-xs text-theme-text-muted">
              {effectiveConnectionIds.length > 0 && (
                <li>{t('modals.export.content_summary_connections', { count: effectiveConnectionIds.length })}</li>
              )}
              {selectedForwardIds.size > 0 && (
                <li>{t('modals.export.content_summary_forwards', { count: selectedForwardIds.size })}</li>
              )}
              {hasSelectedAppSettings && (
                <li>{t('modals.export.content_summary_app_settings')}: {selectedAppSettingsLabels.join(', ')}</li>
              )}
              {hasSelectedPluginSettings && (
                <li>{t('modals.export.content_summary_plugin_settings', {
                  plugins: selectedPluginIds.size,
                  count: selectedPluginSettingCount,
                })}</li>
              )}
              {embedKeys && (
                <li>{t('modals.export.content_summary_embed_keys')}</li>
              )}
            </ul>
          </div>

          <div>
            <Label className="text-theme-text">{t('modals.export.password')}</Label>
            <Input
              type="password"
              placeholder={t('modals.export.password_placeholder')}
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              className="mt-1 bg-theme-bg border-theme-border text-theme-text placeholder:text-theme-text-muted focus-visible:ring-theme-accent"
            />
            {passwordStrength && (
              <div className="mt-2 space-y-1.5">
                <div className="grid grid-cols-3 gap-1.5">
                  <div className={`h-1.5 rounded-full ${getStrengthTone(passwordStrength, true)}`} />
                  <div className={`h-1.5 rounded-full ${getStrengthTone(passwordStrength, passwordStrength !== 'weak')}`} />
                  <div className={`h-1.5 rounded-full ${getStrengthTone(passwordStrength, passwordStrength === 'strong')}`} />
                </div>
                <p className={`text-xs ${passwordStrength === 'weak' ? 'text-yellow-500' : passwordStrength === 'strong' ? 'text-green-500' : 'text-theme-text-muted'}`}>
                  {getStrengthLabel(passwordStrength)}
                </p>
              </div>
            )}
          </div>

          <div>
            <Label className="text-theme-text">{t('modals.export.confirm_password')}</Label>
            <Input
              type="password"
              placeholder={t('modals.export.confirm_password_placeholder')}
              value={confirmPassword}
              onChange={(event) => setConfirmPassword(event.target.value)}
              className="mt-1 bg-theme-bg border-theme-border text-theme-text placeholder:text-theme-text-muted focus-visible:ring-theme-accent"
            />
          </div>

          {error && (
            <div className="bg-red-500/10 border border-red-500/20 text-red-500 px-3 py-2 rounded text-sm">
              {error}
            </div>
          )}

          <div className="bg-blue-500/10 border border-blue-500/20 text-blue-500 px-3 py-2 rounded text-sm">
            <p className="font-semibold">{t('modals.export.security_notice')}</p>
            <ul className="mt-1 space-y-1 text-xs opacity-90">
              <li>• {t('modals.export.security_encryption')}</li>
              <li>• {t('modals.export.security_kdf')}</li>
              <li>• {t('modals.export.security_contains')}</li>
              <li>• {t('modals.export.security_settings', {
                app: includeAppSettings ? t('common.yes') : t('common.no'),
                plugin: hasSelectedPluginSettings ? t('common.yes') : t('common.no'),
              })}</li>
              <li>• {t('modals.export.security_passwords_excluded')}</li>
              <li>• <strong>{t('modals.export.security_no_session')}</strong></li>
              <li>• {t('modals.export.security_keep_safe')}</li>
            </ul>
          </div>

          {exporting && (
            <div className="rounded-md border border-theme-border bg-theme-bg p-3 space-y-2">
              <div className="flex items-center justify-between gap-3 text-sm text-theme-text">
                <span className="font-medium">{getStageText()}</span>
                <span className="text-theme-text-muted">{exportPercent}%</span>
              </div>
              <div
                role="progressbar"
                aria-valuenow={exportPercent}
                aria-valuemin={0}
                aria-valuemax={100}
                className="h-2 rounded-full overflow-hidden bg-theme-bg-hover"
              >
                <div
                  className="h-full bg-theme-accent transition-[width] duration-200 ease-out"
                  style={{ width: `${exportPercent}%` }}
                />
              </div>
            </div>
          )}

          <div className="flex justify-end space-x-2 pt-2">
            <Button variant="outline" onClick={onClose} disabled={exporting} className="border-theme-border text-theme-text hover:bg-theme-bg-hover">
              {t('modals.export.cancel')}
            </Button>
            <Button
              onClick={handleExport}
              disabled={exporting || !hasAnyContent}
              className="bg-theme-accent text-white hover:bg-theme-accent-hover disabled:opacity-50 min-w-[140px]"
            >
              {exporting ? (
                <span className="flex items-center gap-2">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {getStageText()}
                </span>
              ) : (
                t('modals.export.export')
              )}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}