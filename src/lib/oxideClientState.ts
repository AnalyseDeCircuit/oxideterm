// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { invoke } from '@tauri-apps/api/core';
import type { ExportPreflightResult, ImportPreview, ImportResult, OxideMetadata } from '../types';
import {
  applyImportedPluginSettingsSnapshot,
  collectPluginSettingsSnapshot,
  parseSettingStorageKey,
  type PluginSettingSnapshotEntry,
} from './plugin/pluginSettingsManager';
import {
  applyImportedSettingsSnapshot,
  exportOxideAppSettingsSnapshot,
  type OxideAppSettingsSectionId,
} from '../store/settingsStore';
import {
  applyImportedQuickCommandsSnapshot,
  exportQuickCommandsSnapshot,
  type QuickCommandImportStrategy,
} from '../store/quickCommandsStore';

type ExportOxideRequest = {
  connectionIds: string[];
  password: string;
  description?: string | null;
  embedKeys?: boolean | null;
  includePortableSecrets?: boolean;
  includeAppSettings?: boolean;
  includeQuickCommands?: boolean;
  selectedAppSettingsSections?: OxideAppSettingsSectionId[];
  includeLocalTerminalEnvVars?: boolean;
  includePluginSettings?: boolean;
  selectedPluginIds?: string[];
  selectedForwardIds?: string[];
  onProgress?: (progress: OxideExportProgress) => void;
};

export type OxideExportProgress = {
  /** Discrete completed step count within a single export invocation. */
  stage: string;
  /** Completed steps, 1-based and bounded by total. */
  current: number;
  /** Total number of discrete steps for the current export invocation. */
  total: number;
};

export type OxideImportProgress = {
  stage: string;
  current: number;
  total: number;
};

type PreviewImportOptions = {
  conflictStrategy?: 'rename' | 'skip' | 'replace' | 'merge';
  onProgress?: (progress: OxideImportProgress) => void;
};

type ImportOxideOptions = PreviewImportOptions & {
  selectedNames?: string[];
  importAppSettings?: boolean;
  importQuickCommands?: boolean;
  selectedAppSettingsSections?: string[];
  importPluginSettings?: boolean;
  selectedPluginIds?: string[];
  importForwards?: boolean;
  importPortableSecrets?: boolean;
};

type ImportFromOxideEnvelope = Omit<ImportResult, 'importedAppSettings' | 'importedPluginSettings'> & {
  appSettingsJson?: string | null;
  quickCommandsJson?: string | null;
  pluginSettings?: PluginSettingSnapshotEntry[] | null;
};

function buildClientStatePayload(options?: {
  selectedAppSettingsSections?: OxideAppSettingsSectionId[];
  includeLocalTerminalEnvVars?: boolean;
}): {
  appSettingsJson: string | null;
  quickCommandsJson: string | null;
  pluginSettings: PluginSettingSnapshotEntry[];
} {
  return {
    appSettingsJson: exportOxideAppSettingsSnapshot({
      selectedSections: options?.selectedAppSettingsSections,
      includeLocalTerminalEnvVars: options?.includeLocalTerminalEnvVars,
    }),
    quickCommandsJson: exportQuickCommandsSnapshot(),
    pluginSettings: collectPluginSettingsSnapshot(),
  };
}

export async function preflightOxideExport(
  connectionIds: string[],
  options?: { embedKeys?: boolean; includePortableSecrets?: boolean },
): Promise<ExportPreflightResult> {
  return invoke<ExportPreflightResult>('preflight_export', {
    connectionIds,
    embedKeys: options?.embedKeys ?? null,
    includePortableSecrets: options?.includePortableSecrets ?? null,
  });
}

export async function exportOxideWithClientState(
  request: ExportOxideRequest,
): Promise<Uint8Array> {
  const includeAppSettings = (request.includeAppSettings ?? true)
    && (request.selectedAppSettingsSections ? request.selectedAppSettingsSections.length > 0 : true);
  const includeQuickCommands = request.includeQuickCommands ?? true;
  const includePluginSettings = request.includePluginSettings ?? true;
  const clientState = (includeAppSettings || includeQuickCommands || includePluginSettings)
    ? buildClientStatePayload({
        selectedAppSettingsSections: request.selectedAppSettingsSections,
        includeLocalTerminalEnvVars: request.includeLocalTerminalEnvVars,
      })
    : { appSettingsJson: null, quickCommandsJson: null, pluginSettings: [] };
  const filteredPluginSettings = includePluginSettings && clientState.pluginSettings.length > 0
    ? (request.selectedPluginIds?.length
      ? clientState.pluginSettings.filter((entry) => {
          const parsed = parseSettingStorageKey(entry.storageKey);
          return parsed ? request.selectedPluginIds!.includes(parsed.pluginId) : false;
        })
      : clientState.pluginSettings)
    : [];
  const invokeArgs = {
    connectionIds: request.connectionIds,
    password: request.password,
    description: request.description ?? null,
    embedKeys: request.embedKeys ?? null,
    includePortableSecrets: request.includePortableSecrets ?? null,
    selectedForwardIds: request.selectedForwardIds ?? null,
    appSettingsJson: includeAppSettings ? clientState.appSettingsJson : null,
    quickCommandsJson: includeQuickCommands ? clientState.quickCommandsJson : null,
    pluginSettings: filteredPluginSettings.length > 0
      ? filteredPluginSettings
      : null,
  };

  const fileData = request.onProgress
    ? await (async () => {
        const { Channel } = await import('@tauri-apps/api/core');
        const channel = new Channel<OxideExportProgress>();
        channel.onmessage = (progress) => {
          request.onProgress?.(progress);
        };

        return invoke<number[]>('export_to_oxide_with_progress', {
          ...invokeArgs,
          onProgress: channel,
        });
      })()
    : await invoke<number[]>('export_to_oxide', invokeArgs);
  return new Uint8Array(fileData);
}

export async function validateOxideFile(fileData: Uint8Array): Promise<OxideMetadata> {
  return invoke<OxideMetadata>('validate_oxide_file', {
    fileData: Array.from(fileData),
  });
}

export async function previewOxideImport(
  fileData: Uint8Array,
  password: string,
  options?: PreviewImportOptions,
): Promise<ImportPreview> {
  const invokeArgs = {
    fileData: Array.from(fileData),
    password,
    conflictStrategy: options?.conflictStrategy ?? null,
  };

  return options?.onProgress
    ? (async () => {
        const { Channel } = await import('@tauri-apps/api/core');
        const channel = new Channel<OxideImportProgress>();
        channel.onmessage = (progress) => {
          options.onProgress?.(progress);
        };

        return invoke<ImportPreview>('preview_oxide_import_with_progress', {
          ...invokeArgs,
          onProgress: channel,
        });
      })()
    : invoke<ImportPreview>('preview_oxide_import', invokeArgs);
}

export async function importOxideWithClientState(
  fileData: Uint8Array,
  password: string,
  options?: ImportOxideOptions,
): Promise<ImportResult> {
  const selectedAppSettingsSections = options?.importAppSettings === false
    ? []
    : options?.selectedAppSettingsSections;
  const shouldImportApp = options?.importAppSettings !== false;
  const shouldImportQuickCommands = options?.importQuickCommands !== false;
  const shouldImportPlugin = options?.importPluginSettings !== false;
  const invokeArgs = {
    fileData: Array.from(fileData),
    password,
    selectedNames: options?.selectedNames ?? null,
    conflictStrategy: options?.conflictStrategy ?? null,
    importForwards: options?.importForwards ?? null,
    importPortableSecrets: options?.importPortableSecrets ?? null,
  };
  const envelope = options?.onProgress
    ? await (async () => {
        const { Channel } = await import('@tauri-apps/api/core');
        const channel = new Channel<OxideImportProgress>();
        channel.onmessage = (progress) => {
          options.onProgress?.(progress);
        };

        return invoke<ImportFromOxideEnvelope>('import_from_oxide_with_progress', {
          ...invokeArgs,
          onProgress: channel,
        });
      })()
    : await invoke<ImportFromOxideEnvelope>('import_from_oxide', invokeArgs);

  let importedAppSettings = false;
  if (shouldImportApp && envelope.appSettingsJson) {
    importedAppSettings = await applyImportedSettingsSnapshot(envelope.appSettingsJson, {
      selectedSections: selectedAppSettingsSections,
    });
  }

  const quickCommandsResult = shouldImportQuickCommands && envelope.quickCommandsJson
    ? applyImportedQuickCommandsSnapshot(
      envelope.quickCommandsJson,
      (options?.conflictStrategy ?? 'rename') as QuickCommandImportStrategy,
    )
    : { imported: 0, skipped: 0, errors: [] };

  const filteredPluginSettings = shouldImportPlugin && envelope.pluginSettings?.length
    ? (options?.selectedPluginIds
      ? (options.selectedPluginIds.length > 0
        ? envelope.pluginSettings.filter((entry) => {
            const parsed = parseSettingStorageKey(entry.storageKey);
            return parsed ? options.selectedPluginIds!.includes(parsed.pluginId) : false;
          })
        : [])
      : envelope.pluginSettings)
    : [];

  const importedPluginSettings = filteredPluginSettings.length
    ? applyImportedPluginSettingsSnapshot(filteredPluginSettings)
    : 0;

  const {
    appSettingsJson: _appSettingsJson,
    quickCommandsJson: _quickCommandsJson,
    pluginSettings: _pluginSettings,
    ...result
  } = envelope;
  return {
    ...result,
    importedAppSettings,
    skippedAppSettings: !shouldImportApp && Boolean(envelope.appSettingsJson),
    importedQuickCommands: quickCommandsResult.imported,
    skippedQuickCommands: (!shouldImportQuickCommands && Boolean(envelope.quickCommandsJson))
      || quickCommandsResult.errors.length > 0,
    quickCommandsErrors: quickCommandsResult.errors,
    importedPluginSettings,
    skippedPluginSettings: !shouldImportPlugin && Boolean(envelope.pluginSettings?.length),
  };
}
