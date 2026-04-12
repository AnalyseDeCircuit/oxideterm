import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());
const channelInstances = vi.hoisted(() => [] as Array<{ onmessage?: (message: unknown) => void }>);
const applyImportedSettingsSnapshotMock = vi.hoisted(() => vi.fn());
const exportOxideAppSettingsSnapshotMock = vi.hoisted(() => vi.fn());
const collectPluginSettingsSnapshotMock = vi.hoisted(() => vi.fn());
const applyImportedPluginSettingsSnapshotMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
  Channel: class MockChannel<T> {
    onmessage?: (message: T) => void;

    constructor() {
      channelInstances.push(this as unknown as { onmessage?: (message: unknown) => void });
    }
  },
}));

vi.mock('@/store/settingsStore', () => ({
  applyImportedSettingsSnapshot: applyImportedSettingsSnapshotMock,
  exportOxideAppSettingsSnapshot: exportOxideAppSettingsSnapshotMock,
}));

vi.mock('@/lib/plugin/pluginSettingsManager', () => ({
  collectPluginSettingsSnapshot: collectPluginSettingsSnapshotMock,
  applyImportedPluginSettingsSnapshot: applyImportedPluginSettingsSnapshotMock,
  parseSettingStorageKey: (storageKey: string) => {
    const match = /^oxide-plugin-(.+)-setting-(.+)$/.exec(storageKey);
    if (!match) {
      return null;
    }

    return {
      pluginId: match[1],
      settingId: match[2],
    };
  },
}));

import { exportOxideWithClientState, importOxideWithClientState } from '@/lib/oxideClientState';

describe('oxideClientState', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    channelInstances.length = 0;
    exportOxideAppSettingsSnapshotMock.mockReturnValue('{"format":"oxide-settings-sections-v1"}');
    collectPluginSettingsSnapshotMock.mockReturnValue([
      { storageKey: 'oxide-plugin-plugin-a-setting-theme', serializedValue: '"night"' },
      { storageKey: 'oxide-plugin-plugin-b-setting-layout', serializedValue: '"compact"' },
      { storageKey: 'invalid-key', serializedValue: 'true' },
    ]);
    applyImportedSettingsSnapshotMock.mockResolvedValue(true);
    applyImportedPluginSettingsSnapshotMock.mockImplementation((entries: Array<{ storageKey: string }>) => entries.length);
  });

  it('filters exported plugin settings and forwards by caller selection', async () => {
    invokeMock.mockResolvedValueOnce([1, 2, 3]);

    const result = await exportOxideWithClientState({
      connectionIds: ['saved-1'],
      password: '123456',
      includeAppSettings: true,
      includePluginSettings: true,
      selectedPluginIds: ['plugin-b'],
      selectedForwardIds: ['forward-1'],
    });

    expect(Array.from(result)).toEqual([1, 2, 3]);
    expect(invokeMock).toHaveBeenCalledWith('export_to_oxide', {
      connectionIds: ['saved-1'],
      password: '123456',
      description: null,
      embedKeys: null,
      selectedForwardIds: ['forward-1'],
      appSettingsJson: '{"format":"oxide-settings-sections-v1"}',
      pluginSettings: [{
        storageKey: 'oxide-plugin-plugin-b-setting-layout',
        serializedValue: '"compact"',
      }],
    });
  });

  it('forwards Rust-side export progress through the progress-aware export command', async () => {
    const onProgress = vi.fn();
    invokeMock.mockImplementationOnce(async (_command, args: { onProgress: { onmessage?: (message: unknown) => void } }) => {
      args.onProgress.onmessage?.({ stage: 'deriving_key', current: 4, total: 9 });
      return [7, 8, 9];
    });

    const result = await exportOxideWithClientState({
      connectionIds: [],
      password: '123456',
      includeAppSettings: true,
      selectedAppSettingsSections: ['general'],
      includePluginSettings: false,
      onProgress,
    });

    expect(Array.from(result)).toEqual([7, 8, 9]);
    expect(invokeMock).toHaveBeenCalledWith('export_to_oxide_with_progress', expect.objectContaining({
      connectionIds: [],
      password: '123456',
      onProgress: channelInstances[0],
    }));
    expect(onProgress).toHaveBeenCalledWith({ stage: 'deriving_key', current: 4, total: 9 });
  });

  it('respects import toggles and reports skipped client-side sections', async () => {
    invokeMock.mockResolvedValueOnce({
      imported: 0,
      skipped: 0,
      merged: 0,
      replaced: 0,
      renamed: 0,
      errors: [],
      renames: [],
      importedForwards: 0,
      skippedForwards: 2,
      appSettingsJson: '{"theme":"imported"}',
      pluginSettings: [{
        storageKey: 'oxide-plugin-plugin-a-setting-theme',
        serializedValue: '"light"',
      }],
    });

    const result = await importOxideWithClientState(new Uint8Array([9, 8, 7]), '123456', {
      importAppSettings: false,
      importPluginSettings: false,
      importForwards: false,
    });

    expect(invokeMock).toHaveBeenCalledWith('import_from_oxide', {
      fileData: [9, 8, 7],
      password: '123456',
      selectedNames: null,
      conflictStrategy: null,
      importForwards: false,
    });
    expect(applyImportedSettingsSnapshotMock).not.toHaveBeenCalled();
    expect(applyImportedPluginSettingsSnapshotMock).not.toHaveBeenCalled();
    expect(result.importedAppSettings).toBe(false);
    expect(result.skippedAppSettings).toBe(true);
    expect(result.importedPluginSettings).toBe(0);
    expect(result.skippedPluginSettings).toBe(true);
    expect(result.skippedForwards).toBe(2);
  });

  it('filters imported plugin settings to selected plugin ids only', async () => {
    invokeMock.mockResolvedValueOnce({
      imported: 1,
      skipped: 0,
      merged: 0,
      replaced: 0,
      renamed: 0,
      errors: [],
      renames: [],
      importedForwards: 1,
      skippedForwards: 0,
      appSettingsJson: null,
      pluginSettings: [
        {
          storageKey: 'oxide-plugin-plugin-a-setting-theme',
          serializedValue: '"light"',
        },
        {
          storageKey: 'oxide-plugin-plugin-b-setting-layout',
          serializedValue: '"compact"',
        },
      ],
    });

    const result = await importOxideWithClientState(new Uint8Array([1, 2, 3]), '123456', {
      selectedPluginIds: ['plugin-b'],
    });

    expect(applyImportedPluginSettingsSnapshotMock).toHaveBeenCalledWith([
      {
        storageKey: 'oxide-plugin-plugin-b-setting-layout',
        serializedValue: '"compact"',
      },
    ]);
    expect(result.importedPluginSettings).toBe(1);
    expect(result.skippedPluginSettings).toBe(false);
  });
});