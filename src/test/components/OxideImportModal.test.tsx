import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMutableSelectorStore } from '@/test/helpers/mockStore';

const translationMap: Record<string, string> = {
  'modals.import.title': 'Import Configuration',
  'modals.import.close': 'Close',
  'modals.import.select_file': 'Select .oxide File',
  'modals.import.instructions_title': 'Import Instructions',
  'modals.import.instructions_1': 'Instruction 1',
  'modals.import.instructions_2': 'Instruction 2',
  'modals.import.instructions_3': 'Instruction 3',
  'modals.import.instructions_4': 'Instruction 4',
  'modals.import.file_info': 'File Information',
  'modals.import.exported_at': 'Exported at:',
  'modals.import.exported_by': 'Exported by:',
  'modals.import.contains': 'Contains:',
  'modals.import.connections_count': '{{count}} connections',
  'modals.import.partial_import_hint': 'Partial import hint',
  'modals.import.connection_list': 'Connection List:',
  'modals.import.password': 'Decryption Password',
  'modals.import.password_placeholder': 'Enter password set during export',
  'modals.import.conflict_strategy': 'Conflict Strategy',
  'modals.import.strategy_rename': 'Rename on Conflict',
  'modals.import.strategy_skip': 'Skip Conflicts',
  'modals.import.strategy_replace': 'Replace Existing',
  'modals.import.strategy_merge': 'Merge Existing',
  'modals.import.reselect_file': 'Reselect File',
  'modals.import.cancel': 'Cancel',
  'modals.import.preview': 'Preview',
  'modals.import.previewing': 'Loading...',
  'modals.import.stage_parsing': 'Parsing file...',
  'modals.import.stage_decrypting': 'Decrypting...',
  'modals.import.stage_analyzing': 'Analyzing import preview...',
  'modals.import.stage_preparing': 'Preparing import...',
  'modals.import.stage_applying': 'Applying changes...',
  'modals.import.stage_saving': 'Saving imported data...',
  'modals.import.stage_done': 'Done!',
  'modals.import.preview_title': 'Import Preview',
  'modals.import.preview_total': '{{count}} connections will be imported',
  'modals.import.selected_count': '{{count}} selected',
  'modals.import.preview_unchanged': 'Unchanged',
  'modals.import.back': 'Back',
  'modals.import.confirm_import': 'Confirm Import',
  'modals.import.importing': 'Importing...',
  'modals.import.warning_title': 'Warning',
  'modals.import.warning_text': 'Warning text',
  'modals.import.passwords_not_included': 'Passwords are not included',
};

const openMock = vi.hoisted(() => vi.fn());
const readFileMock = vi.hoisted(() => vi.fn());
const validateOxideFileMock = vi.hoisted(() => vi.fn());
const previewOxideImportMock = vi.hoisted(() => vi.fn());
const importOxideWithClientStateMock = vi.hoisted(() => vi.fn());
const loadSavedConnectionsMock = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));

const appStoreState = vi.hoisted(() => ({
  loadSavedConnections: loadSavedConnectionsMock,
}));

const pluginStoreState = vi.hoisted(() => ({
  plugins: new Map(),
}));

const settingsStoreState = vi.hoisted(() => ({
  settings: {},
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, string | number>) => {
      const template = translationMap[key] ?? key;
      if (!params) {
        return template;
      }

      return Object.entries(params).reduce(
        (text, [name, value]) => text.replace(`{{${name}}}`, String(value)),
        template,
      );
    },
  }),
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: createMutableSelectorStore(appStoreState),
}));

vi.mock('@/store/pluginStore', () => ({
  usePluginStore: createMutableSelectorStore(pluginStoreState),
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: createMutableSelectorStore(settingsStoreState),
  buildOxideAppSettingsSectionValueMap: () => ({}),
}));

vi.mock('@/lib/oxideClientState', () => ({
  validateOxideFile: validateOxideFileMock,
  previewOxideImport: previewOxideImportMock,
  importOxideWithClientState: importOxideWithClientStateMock,
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: openMock,
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readFile: readFileMock,
}));

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ open, children }: { open: boolean; children: React.ReactNode }) => (open ? <div>{children}</div> : null),
  DialogContent: ({ children, className }: { children: React.ReactNode; className?: string }) => <div className={className}>{children}</div>,
  DialogHeader: ({ children, className }: { children: React.ReactNode; className?: string }) => <div className={className}>{children}</div>,
  DialogTitle: ({ children, className }: { children: React.ReactNode; className?: string }) => <h2 className={className}>{children}</h2>,
  DialogClose: ({ children, className }: { children: React.ReactNode; className?: string }) => <button className={className}>{children}</button>,
}));

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, onClick, disabled, type = 'button', ...props }: React.ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button type={type} onClick={onClick} disabled={disabled} {...props}>{children}</button>
  ),
}));

vi.mock('@/components/ui/input', () => ({
  Input: (props: React.InputHTMLAttributes<HTMLInputElement>) => <input {...props} />,
}));

vi.mock('@/components/ui/label', () => ({
  Label: ({ children, className }: React.LabelHTMLAttributes<HTMLLabelElement>) => <label className={className}>{children}</label>,
}));

import { OxideImportModal } from '@/components/modals/OxideImportModal';

describe('OxideImportModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    openMock.mockResolvedValue('/tmp/test.oxide');
    readFileMock.mockResolvedValue(new Uint8Array([1, 2, 3]));
    validateOxideFileMock.mockResolvedValue({
      exported_at: '2026-04-12T00:00:00Z',
      exported_by: 'OxideTerm',
      description: null,
      num_connections: 1,
      connection_names: ['Prod'],
      has_app_settings: false,
      plugin_settings_count: 0,
    });
    importOxideWithClientStateMock.mockResolvedValue({
      imported: 1,
      skipped: 0,
      merged: 0,
      replaced: 0,
      renamed: 0,
      errors: [],
      renames: [],
      imported_forwards: 0,
      skipped_forwards: 0,
      importedAppSettings: false,
      skippedAppSettings: false,
      importedPluginSettings: 0,
      skippedPluginSettings: false,
    });
  });

  it('shows fine-grained preview progress while decrypting an import', async () => {
    let resolvePreview: ((value: unknown) => void) | undefined;
    previewOxideImportMock.mockImplementation((_fileData, _password, options) => {
      options?.onProgress?.({
        stage: 'deriving_key',
        current: 2,
        total: 8,
      });

      return new Promise((resolve) => {
        resolvePreview = resolve;
      });
    });

    render(<OxideImportModal isOpen onClose={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Select .oxide File' }));

    await waitFor(() => {
      expect(validateOxideFileMock).toHaveBeenCalled();
    });

    fireEvent.change(screen.getByPlaceholderText('Enter password set during export'), {
      target: { value: 'secret123' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'Preview' }));

    expect(await screen.findByText('Decrypting...')).toBeInTheDocument();
    expect(screen.getByText('2/8')).toBeInTheDocument();
    expect(screen.getByText('25%')).toBeInTheDocument();

    resolvePreview?.({
      totalConnections: 1,
      unchanged: ['Prod'],
      willRename: [],
      willSkip: [],
      willReplace: [],
      willMerge: [],
      hasEmbeddedKeys: false,
      totalForwards: 0,
      hasAppSettings: false,
      appSettingsFormat: null,
      appSettingsKeys: [],
      appSettingsPreview: {},
      appSettingsSections: [],
      pluginSettingsCount: 0,
      pluginSettingsByPlugin: {},
      forwardDetails: [],
      records: [],
    });

    expect(await screen.findByText('Import Preview')).toBeInTheDocument();
  });
});