import { beforeEach, describe, expect, it, vi } from 'vitest';
import { waitFor } from '@testing-library/react';
import packageJson from '../../../package.json';

const toastMock = vi.hoisted(() => ({
  addToast: vi.fn(),
}));

const themeMocks = vi.hoisted(() => ({
  themes: { default: { background: '#000' } },
  getTerminalTheme: vi.fn(() => ({ background: '#000' })),
  isCustomTheme: vi.fn((name: string) => name.startsWith('custom-')),
  applyCustomThemeCSS: vi.fn(),
  clearCustomThemeCSS: vi.fn(),
}));

const fontUtilsMock = vi.hoisted(() => ({
  getFontFamilyCSS: vi.fn((fontFamily: string) => `${fontFamily}, monospace`),
}));

const i18nMocks = vi.hoisted(() => ({
  changeLanguage: vi.fn().mockResolvedValue(undefined),
  t: vi.fn((key: string) => key),
}));

const providerRegistryMock = vi.hoisted(() => ({
  fetchModels: vi.fn().mockResolvedValue(['model-a', 'model-b']),
  fetchModelDetails: vi.fn().mockResolvedValue({ 'model-a': 32000 }),
}));

const apiMocks = vi.hoisted(() => ({
  sftpUpdateSettings: vi.fn().mockResolvedValue(undefined),
  getAiProviderApiKey: vi.fn().mockResolvedValue('secret-key'),
  sshGetPoolConfig: vi.fn().mockResolvedValue({ idleTimeoutSecs: 1800, maxConnections: 0, protectOnExit: true }),
  sshSetPoolConfig: vi.fn().mockResolvedValue(undefined),
  loadAppSettings: vi.fn(),
  saveAppSettings: vi.fn(),
  resetAppSettings: vi.fn(),
  exportAppSettingsSnapshot: vi.fn(),
  applyAppSettingsSnapshot: vi.fn(),
}));

vi.mock('@/lib/themes', () => themeMocks);

vi.mock('@/hooks/useToast', () => ({
  useToastStore: {
    getState: () => toastMock,
  },
}));

vi.mock('@/components/fileManager/fontUtils', () => fontUtilsMock);

vi.mock('@/i18n', () => ({
  default: { t: i18nMocks.t },
  changeLanguage: i18nMocks.changeLanguage,
}));

vi.mock('@/lib/ai/providers', () => ({
  DEFAULT_PROVIDERS: [
    {
      type: 'openai',
      name: 'OpenAI',
      baseUrl: 'https://api.openai.com/v1',
      defaultModel: 'gpt-4o-mini',
      models: ['gpt-4o-mini'],
    },
    {
      type: 'ollama',
      name: 'Ollama',
      baseUrl: 'http://localhost:11434/v1',
      defaultModel: 'qwen2.5',
      models: ['qwen2.5'],
    },
  ],
}));

vi.mock('@/lib/platform', () => ({
  platform: {
    isWindows: false,
  },
}));

vi.mock('@/lib/ai/providerRegistry', () => ({
  getProvider: vi.fn(() => providerRegistryMock),
}));

vi.mock('@/lib/api', () => ({
  api: apiMocks,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

async function loadSettingsStore() {
  const mod = await import('@/store/settingsStore');
  installSettingsApiMocks(mod);
  await mod.initializeSettings();
  return mod.useSettingsStore;
}

type SettingsStoreModule = typeof import('@/store/settingsStore');

function settingsLoadResult(settings: unknown, migratedFromLegacyLocalStorage = false) {
  return {
    settings,
    version: 3,
    updatedAt: Date.now(),
    migrationWarnings: [],
    validationWarnings: [],
    migratedFromLegacyLocalStorage,
    recoveredFromCorruptFile: false,
  };
}

function settingsSaveResult(settings: unknown) {
  return {
    settings,
    version: 3,
    updatedAt: Date.now(),
    validationWarnings: [],
  };
}

function installSettingsApiMocks(mod: SettingsStoreModule) {
  apiMocks.loadAppSettings.mockImplementation(async (legacySettingsJson?: string | null) => {
    const raw = legacySettingsJson ? JSON.parse(legacySettingsJson) : {};
    return settingsLoadResult(mod.mergeWithDefaults(raw), Boolean(legacySettingsJson));
  });
  apiMocks.saveAppSettings.mockImplementation(async (settings: unknown) => (
    settingsSaveResult(mod.mergeWithDefaults(settings as Record<string, unknown>))
  ));
  apiMocks.resetAppSettings.mockImplementation(async () => (
    settingsLoadResult(mod.mergeWithDefaults({}))
  ));
  apiMocks.exportAppSettingsSnapshot.mockImplementation(async () => (
    mod.exportOxideAppSettingsSnapshot()
  ));
}

async function expectLastSavedSettings() {
  await waitFor(() => {
    expect(apiMocks.saveAppSettings).toHaveBeenCalled();
  });
  const calls = apiMocks.saveAppSettings.mock.calls;
  return calls[calls.length - 1]?.[0];
}

function buildSavedSettings(overrides: Record<string, unknown> = {}) {
  return {
    version: 3,
    general: { language: 'en' },
    terminal: { theme: 'default', renderer: 'auto' },
    buffer: { maxLines: 2000 },
    appearance: { sidebarCollapsedDefault: false, uiDensity: 'comfortable', borderRadius: 6, uiFontFamily: '', animationSpeed: 'normal', frostedGlass: 'off' },
    connectionDefaults: { username: 'root', port: 22 },
    treeUI: { expandedIds: [], focusedNodeId: null },
    sidebarUI: { collapsed: false, activeSection: 'sessions', width: 300, aiSidebarCollapsed: true, aiSidebarWidth: 340, zenMode: false },
    ai: {
      enabled: true,
      enabledConfirmed: true,
      baseUrl: 'https://custom.example/v1',
      model: 'custom-model',
      providers: [],
      activeProviderId: null,
      activeModel: null,
      contextMaxChars: 8000,
      contextVisibleLines: 120,
      thinkingStyle: 'detailed',
      thinkingDefaultExpanded: false,
      toolUse: {
        enabled: true,
        autoApproveReadOnly: true,
        autoApproveAll: false,
      },
      contextSources: { ide: true, sftp: true },
    },
    ...overrides,
  };
}

describe('settingsStore', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
    document.documentElement.removeAttribute('data-density');
    document.documentElement.style.cssText = '';
  });

  it('migrates legacy AI provider and tool-use settings on load', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings()));

    const useSettingsStore = await loadSettingsStore();
    const settings = useSettingsStore.getState().settings;

    expect(settings.ai.providers[0]).toMatchObject({
      type: 'openai_compatible',
      name: 'Custom (Migrated)',
      baseUrl: 'https://custom.example/v1',
      defaultModel: 'custom-model',
    });
    expect(settings.ai.activeProviderId).toBe(settings.ai.providers[0].id);
    expect(settings.ai.activeModel).toBe('custom-model');
    expect(settings.ai.toolUse?.autoApproveTools.list_targets).toBe(true);
    expect(settings.ai.toolUse?.autoApproveTools.select_target).toBe(true);
    expect(settings.ai.toolUse?.autoApproveTools.read_resource).toBe(true);
    expect(settings.ai.toolUse?.autoApproveTools.run_command).toBe(false);
    expect(settings.ai.toolUse?.maxRounds).toBe(10);

    const persisted = await expectLastSavedSettings();
    expect(persisted.ai.providers.length).toBeGreaterThan(0);
    expect(persisted.ai.toolUse.autoApproveTools.list_targets).toBe(true);
    expect(persisted.ai.toolUse.autoApproveTools.run_command).toBe(false);
    expect(persisted.ai.toolUse.maxRounds).toBe(10);
  });

  it('normalizes persisted AI tool round limits on load', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      ai: {
        ...buildSavedSettings().ai,
        toolUse: {
          enabled: true,
          autoApproveTools: {},
          disabledTools: [],
          maxRounds: 999,
        },
      },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.ai.toolUse?.maxRounds).toBe(30);
  });

  it('merges terminal autosuggest defaults for existing settings', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: { theme: 'default', renderer: 'auto' },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.terminal.autosuggest).toEqual({
      localShellHistory: true,
    });
  });

  it('merges terminal command bar defaults for existing settings', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: { theme: 'default', renderer: 'auto' },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.terminal.commandBar).toEqual({
      enabled: true,
      showLegacyToolbar: false,
      gitStatus: true,
      smartCompletion: true,
      quickCommandsEnabled: true,
      quickCommandsConfirmBeforeRun: false,
      quickCommandsShowToast: true,
      focusHandoffCommands: [
        'btop',
        'emacs',
        'fzf',
        'htop',
        'lazydocker',
        'lazygit',
        'less',
        'man',
        'micro',
        'nano',
        'nvim',
        'ranger',
        'screen',
        'ssh',
        'tig',
        'tmux',
        'top',
        'vi',
        'vim',
        'yazi',
      ],
    });
  });

  it('merges terminal command mark defaults for existing settings', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: { theme: 'default', renderer: 'auto' },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.terminal.commandMarks).toEqual({
      enabled: true,
      userInputObserved: false,
      heuristicDetection: false,
      showHoverActions: true,
    });
  });

  it('maps legacy write_resource approval to settings and file only', async () => {
    const base = buildSavedSettings();
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      ai: {
        ...base.ai,
        toolUse: {
          enabled: true,
          autoApproveTools: { write_resource: true },
          disabledTools: [],
          maxRounds: 10,
        },
      },
    })));

    const useSettingsStore = await loadSettingsStore();
    const autoApproveTools = useSettingsStore.getState().settings.ai.toolUse?.autoApproveTools;

    expect(autoApproveTools?.write_resource).toBe(true);
    expect(autoApproveTools?.['write_resource:settings']).toBe(true);
    expect(autoApproveTools?.['write_resource:file']).toBe(true);
    expect(autoApproveTools?.transfer_resource).toBe(false);
    expect(autoApproveTools?.remember_preference).toBe(false);
  });

  it('clears legacy localStorage keys when loading defaults', async () => {
    localStorage.setItem('oxide-settings', '{"legacy":true}');
    localStorage.setItem('oxide-ui-state', '{"sidebar":false}');

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.version).toBe(3);
    expect(localStorage.getItem('oxide-settings')).toBeNull();
    expect(localStorage.getItem('oxide-ui-state')).toBeNull();
  });

  it('defaults update channel from the package version', async () => {
    const useSettingsStore = await loadSettingsStore();
    const expectedChannel = /-(?:alpha|beta|rc|pre|preview)(?:[.-]|$)/i.test(packageJson.version)
      ? 'beta'
      : 'stable';

    expect(useSettingsStore.getState().settings.general.updateChannel).toBe(expectedChannel);
  });

  it('detects prerelease package versions for beta defaults', async () => {
    const { isPrereleaseVersion, isGpuiPreviewVersion } = await import('@/store/settingsStore');

    expect(isPrereleaseVersion('1.4.2-beta.0')).toBe(true);
    expect(isPrereleaseVersion('1.4.2-rc.1')).toBe(true);
    expect(isGpuiPreviewVersion('1.4.2-gpui-preview.0')).toBe(true);
    expect(isGpuiPreviewVersion('1.4.2-native-preview.0')).toBe(true);
    expect(isGpuiPreviewVersion('1.4.2-beta.0')).toBe(false);
    expect(isPrereleaseVersion('1.4.2')).toBe(false);
  });

  it('preserves an explicitly selected stable update channel', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      general: { language: 'en', updateChannel: 'stable' },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.general.updateChannel).toBe('stable');
  });

  it('preserves the gpui preview update channel', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      general: { language: 'en', updateChannel: 'gpui-preview' },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.general.updateChannel).toBe('gpui-preview');
  });

  it('uses layered scrollback defaults', async () => {
    const useSettingsStore = await loadSettingsStore();

    const settings = useSettingsStore.getState().settings;
    expect(settings.terminal.scrollback).toBe(1000);
    expect(settings.buffer.maxLines).toBe(8000);
    expect(settings.experimental?.gpuCanvas).toBe(false);
  });

  it('merges experimental GPU canvas defaults for existing settings', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      experimental: { virtualSessionProxy: false },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.experimental).toMatchObject({
      virtualSessionProxy: false,
      gpuCanvas: false,
    });
  });

  it('persists experimental GPU canvas setting updates', async () => {
    const useSettingsStore = await loadSettingsStore();

    useSettingsStore.getState().updateExperimental('gpuCanvas', true);

    expect(useSettingsStore.getState().settings.experimental?.gpuCanvas).toBe(true);
    const persisted = await expectLastSavedSettings();
    expect(persisted.experimental.gpuCanvas).toBe(true);
  });

  it('migrates pre-layered scrollback into light xterm and backend hot buffer settings', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      version: 2,
      terminal: { theme: 'default', renderer: 'auto', scrollback: 5000 },
      buffer: { maxLines: 2000 },
    })));

    const useSettingsStore = await loadSettingsStore();
    const settings = useSettingsStore.getState().settings;

    expect(settings.version).toBe(3);
    expect(settings.terminal.scrollback).toBe(1000);
    expect(settings.buffer.maxLines).toBe(10000);
  });

  it('clamps oversized persisted history settings on load', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: { theme: 'default', renderer: 'auto', scrollback: 100000 },
      buffer: { maxLines: 100000 },
    })));

    const useSettingsStore = await loadSettingsStore();
    const settings = useSettingsStore.getState().settings;

    expect(settings.terminal.scrollback).toBe(20000);
    expect(settings.buffer.maxLines).toBe(12000);
  });

  it('clamps oversized pre-layered scrollback migration', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      version: 2,
      terminal: { theme: 'default', renderer: 'auto', scrollback: 100000 },
      buffer: { maxLines: 100000 },
    })));

    const useSettingsStore = await loadSettingsStore();
    const settings = useSettingsStore.getState().settings;

    expect(settings.terminal.scrollback).toBe(1000);
    expect(settings.buffer.maxLines).toBe(12000);
  });

  it('preserves an explicit osc52Clipboard false setting on load', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: { theme: 'default', renderer: 'auto', osc52Clipboard: false },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.terminal.osc52Clipboard).toBe(false);
  });

  it('defaults osc52Clipboard to true when the persisted terminal settings omit it', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: { theme: 'default', renderer: 'auto' },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.terminal.osc52Clipboard).toBe(true);
  });

  it('defaults selectionRequiresShift to false when the persisted terminal settings omit it', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: { theme: 'default', renderer: 'auto' },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.terminal.selectionRequiresShift).toBe(false);
  });

  it('defaults copyOnSelect and middleClickPaste to false when omitted', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: { theme: 'default', renderer: 'auto' },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.terminal.copyOnSelect).toBe(false);
    expect(useSettingsStore.getState().settings.terminal.middleClickPaste).toBe(false);
  });

  it('normalizes legacy terminal encoding aliases on load', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: { theme: 'default', renderer: 'auto', terminalEncoding: 'shift-jis' },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.terminal.terminalEncoding).toBe('shift_jis');
  });

  it('falls back to utf-8 for invalid terminal encoding updates', async () => {
    const useSettingsStore = await loadSettingsStore();

    useSettingsStore.getState().updateTerminal('terminalEncoding', 'latin1' as never);

    expect(useSettingsStore.getState().settings.terminal.terminalEncoding).toBe('utf-8');
    const persisted = await expectLastSavedSettings();
    expect(persisted.terminal.terminalEncoding).toBe('utf-8');
  });

  it('defaults and normalizes in-band transfer settings on load and update', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: {
        theme: 'default',
        renderer: 'auto',
        inBandTransfer: {
          enabled: true,
          provider: 'other',
          allowDirectory: false,
          maxChunkBytes: 1,
          maxFileCount: 0,
          maxTotalBytes: 1,
        },
      },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.terminal.inBandTransfer).toEqual({
      enabled: true,
      provider: 'trzsz',
      allowDirectory: false,
      maxChunkBytes: 64 * 1024,
      maxFileCount: 1,
      maxTotalBytes: 100 * 1024 * 1024,
    });

    useSettingsStore.getState().updateTerminal('inBandTransfer', {
      enabled: false,
      provider: 'trzsz',
      allowDirectory: true,
      maxChunkBytes: 99 * 1024 * 1024,
      maxFileCount: 99_999,
      maxTotalBytes: 999 * 1024 * 1024 * 1024,
    });

    expect(useSettingsStore.getState().settings.terminal.inBandTransfer).toEqual({
      enabled: false,
      provider: 'trzsz',
      allowDirectory: true,
      maxChunkBytes: 8 * 1024 * 1024,
      maxFileCount: 10_000,
      maxTotalBytes: 100 * 1024 * 1024 * 1024,
    });

    const persisted = await expectLastSavedSettings();
    expect(persisted.terminal.inBandTransfer).toEqual({
      enabled: false,
      provider: 'trzsz',
      allowDirectory: true,
      maxChunkBytes: 8 * 1024 * 1024,
      maxFileCount: 10_000,
      maxTotalBytes: 100 * 1024 * 1024 * 1024,
    });

    useSettingsStore.getState().updateTerminal('inBandTransfer', {
      enabled: true,
      provider: 'trzsz',
      allowDirectory: true,
      maxChunkBytes: Number.NaN,
      maxFileCount: Number.NaN,
      maxTotalBytes: Number.NaN,
    });

    expect(useSettingsStore.getState().settings.terminal.inBandTransfer).toEqual({
      enabled: true,
      provider: 'trzsz',
      allowDirectory: true,
      maxChunkBytes: 1024 * 1024,
      maxFileCount: 1024,
      maxTotalBytes: 10 * 1024 * 1024 * 1024,
    });
  });

  it('keeps in-band transfer identity stable for unrelated terminal updates', async () => {
    const useSettingsStore = await loadSettingsStore();
    const initialTransferSettings = useSettingsStore.getState().settings.terminal.inBandTransfer;

    useSettingsStore.getState().updateTerminal('theme', 'azurite');

    expect(useSettingsStore.getState().settings.terminal.theme).toBe('azurite');
    expect(useSettingsStore.getState().settings.terminal.inBandTransfer).toBe(initialTransferSettings);
  });

  it('preserves explicit copyOnSelect and middleClickPaste settings on load and update', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: {
        theme: 'default',
        renderer: 'auto',
        copyOnSelect: true,
        middleClickPaste: true,
      },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.terminal.copyOnSelect).toBe(true);
    expect(useSettingsStore.getState().settings.terminal.middleClickPaste).toBe(true);

    useSettingsStore.getState().updateTerminal('copyOnSelect', false);
    useSettingsStore.getState().updateTerminal('middleClickPaste', false);

    expect(useSettingsStore.getState().settings.terminal.copyOnSelect).toBe(false);
    expect(useSettingsStore.getState().settings.terminal.middleClickPaste).toBe(false);
    const persisted = await expectLastSavedSettings();
    expect(persisted.terminal.copyOnSelect).toBe(false);
    expect(persisted.terminal.middleClickPaste).toBe(false);
  });

  it('preserves an explicit selectionRequiresShift setting on load and update', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: { theme: 'default', renderer: 'auto', selectionRequiresShift: true },
    })));

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.terminal.selectionRequiresShift).toBe(true);

    useSettingsStore.getState().updateTerminal('selectionRequiresShift', false);

    expect(useSettingsStore.getState().settings.terminal.selectionRequiresShift).toBe(false);
    const persisted = await expectLastSavedSettings();
    expect(persisted.terminal.selectionRequiresShift).toBe(false);
  });

  it('sanitizes persisted terminal highlight rules on load', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      terminal: {
        theme: 'default',
        renderer: 'auto',
        highlightRules: [
          {
            id: 'duplicate',
            label: 'Error',
            pattern: 'error',
            enabled: true,
            isRegex: false,
            caseSensitive: false,
            foreground: '#ff0000',
            background: 'bad-color',
            renderMode: 'background',
            priority: 2,
          },
          {
            id: 'duplicate',
            label: 'Warn',
            pattern: 'warn',
            enabled: true,
            isRegex: false,
            caseSensitive: false,
            priority: 2,
          },
        ],
      },
    })));

    const useSettingsStore = await loadSettingsStore();
    const rules = useSettingsStore.getState().settings.terminal.highlightRules;

    expect(rules).toHaveLength(2);
    expect(new Set(rules.map((rule) => rule.id)).size).toBe(2);
    expect(rules[0].background).toBeUndefined();
    expect(new Set(rules.map((rule) => rule.priority)).size).toBe(2);
  });

  it('setLanguage persists app_lang and delegates to i18n', async () => {
    const useSettingsStore = await loadSettingsStore();

    await useSettingsStore.getState().setLanguage('fr-FR');

    expect(useSettingsStore.getState().settings.general.language).toBe('fr-FR');
    expect(localStorage.getItem('app_lang')).toBe('fr-FR');
    expect(i18nMocks.changeLanguage).toHaveBeenCalledWith('fr-FR');
  });

  it('decouples main terminal scrollback from backend hot-buffer updates', async () => {
    const useSettingsStore = await loadSettingsStore();
    const initialBufferLines = useSettingsStore.getState().settings.buffer.maxLines;

    useSettingsStore.getState().updateTerminal('scrollback', 4000);
    expect(useSettingsStore.getState().settings.terminal.scrollback).toBe(4000);
    expect(useSettingsStore.getState().settings.buffer.maxLines).toBe(initialBufferLines);

    useSettingsStore.getState().updateTerminal('scrollback', 100000);
    expect(useSettingsStore.getState().settings.terminal.scrollback).toBe(20000);
    expect(useSettingsStore.getState().settings.buffer.maxLines).toBe(initialBufferLines);

    useSettingsStore.getState().updateBuffer('maxLines', 100000);
    expect(useSettingsStore.getState().settings.buffer.maxLines).toBe(12000);
  });

  it('sanitizes highlight rules on terminal updates before persisting', async () => {
    const useSettingsStore = await loadSettingsStore();

    useSettingsStore.getState().updateTerminal('highlightRules', [
      {
        id: 'rule-a',
        label: 'A',
        pattern: 'error',
        enabled: true,
        isRegex: false,
        caseSensitive: false,
        priority: 3,
      },
      {
        id: 'rule-a',
        label: 'B',
        pattern: 'warn',
        enabled: true,
        isRegex: false,
        caseSensitive: false,
        priority: 3,
      },
    ]);

    const persisted = await expectLastSavedSettings();
    const rules = persisted.terminal.highlightRules;

    expect(rules).toHaveLength(2);
    expect(new Set(rules.map((rule: { id: string }) => rule.id)).size).toBe(2);
    expect(new Set(rules.map((rule: { priority: number }) => rule.priority)).size).toBe(2);
  });

  it('clamps sidebar widths and records MRU commands without duplicates', async () => {
    const useSettingsStore = await loadSettingsStore();
    const store = useSettingsStore.getState();

    store.setSidebarWidth(999);
    store.setAiSidebarWidth(100);
    store.recordCommandMru('command-a');
    store.recordCommandMru('command-b');
    store.recordCommandMru('command-a');

    const settings = useSettingsStore.getState().settings;
    expect(settings.sidebarUI.width).toBe(600);
    expect(settings.sidebarUI.aiSidebarWidth).toBe(280);
    expect(settings.commandPaletteMru).toEqual(['command-a', 'command-b']);
  });

  it('syncs SFTP settings to backend when transfer-related settings change', async () => {
    const useSettingsStore = await loadSettingsStore();
    apiMocks.sftpUpdateSettings.mockClear();

    useSettingsStore.getState().updateSftp('maxConcurrentTransfers', 5);
    await waitFor(() => {
      expect(apiMocks.sftpUpdateSettings).toHaveBeenCalledWith(5, 0, 4);
    });

    useSettingsStore.getState().updateSftp('speedLimitEnabled', true);
    useSettingsStore.getState().updateSftp('speedLimitKBps', 256);
    await waitFor(() => {
      expect(apiMocks.sftpUpdateSettings.mock.calls).toContainEqual([5, 256, 4]);
    });

    useSettingsStore.getState().updateSftp('directoryParallelism', 8);
    await waitFor(() => {
      expect(apiMocks.sftpUpdateSettings.mock.calls).toContainEqual([5, 256, 8]);
    });
  });

  it('syncs SFTP defaults to backend when resetting settings', async () => {
    const useSettingsStore = await loadSettingsStore();
    apiMocks.sftpUpdateSettings.mockClear();

    useSettingsStore.getState().updateSftp('maxConcurrentTransfers', 5);
    useSettingsStore.getState().updateSftp('speedLimitEnabled', true);
    useSettingsStore.getState().updateSftp('speedLimitKBps', 256);

    await waitFor(() => {
      expect(apiMocks.sftpUpdateSettings.mock.calls).toContainEqual([5, 256, 4]);
    });

    apiMocks.sftpUpdateSettings.mockClear();
    useSettingsStore.getState().resetToDefaults();

    await waitFor(() => {
      expect(apiMocks.sftpUpdateSettings).toHaveBeenCalledWith(3, 0, 4);
    });
  });

  it('persists connection pool idle timeout and syncs it on startup and updates', async () => {
    localStorage.setItem('oxide-settings-v2', JSON.stringify(buildSavedSettings({
      connectionPool: { idleTimeoutSecs: 3600 },
    })));

    const useSettingsStore = await loadSettingsStore();

    await waitFor(() => {
      expect(apiMocks.sshSetPoolConfig).toHaveBeenCalledWith({
        idleTimeoutSecs: 3600,
        maxConnections: 0,
        protectOnExit: true,
      });
    });

    useSettingsStore.getState().updateConnectionPool('idleTimeoutSecs', 900);

    await waitFor(() => {
      expect(apiMocks.sshSetPoolConfig).toHaveBeenCalledWith({
        idleTimeoutSecs: 900,
        maxConnections: 0,
        protectOnExit: true,
      });
    });

    const persisted = await expectLastSavedSettings();
    expect(persisted.connectionPool.idleTimeoutSecs).toBe(900);
  });

  it('refreshes provider models and merges context windows under the provider id', async () => {
    const useSettingsStore = await loadSettingsStore();
    const providerId = useSettingsStore.getState().settings.ai.providers[0].id;

    const models = await useSettingsStore.getState().refreshProviderModels(providerId);

    expect(models).toEqual(['model-a', 'model-b']);
    expect(apiMocks.getAiProviderApiKey).toHaveBeenCalledWith(providerId);
    expect(useSettingsStore.getState().settings.ai.modelContextWindows?.[providerId]).toEqual({
      'model-a': 32000,
    });
  });

  it('persists per-model user context window overrides', async () => {
    const useSettingsStore = await loadSettingsStore();
    const providerId = useSettingsStore.getState().settings.ai.providers[0].id;

    useSettingsStore.getState().setUserContextWindow(providerId, 'model-a', 65536);
    expect(useSettingsStore.getState().settings.ai.userContextWindows?.[providerId]).toEqual({
      'model-a': 65536,
    });

    useSettingsStore.getState().setUserContextWindow(providerId, 'model-a', null);
    expect(useSettingsStore.getState().settings.ai.userContextWindows?.[providerId]).toBeUndefined();

    const persisted = await expectLastSavedSettings();
    expect(persisted.ai.userContextWindows?.[providerId]).toBeUndefined();
  });

  it('persists provider and model reasoning overrides', async () => {
    const useSettingsStore = await loadSettingsStore();
    const providerId = useSettingsStore.getState().settings.ai.providers[0].id;

    useSettingsStore.getState().setProviderReasoningEffort(providerId, 'high');
    useSettingsStore.getState().setModelReasoningEffort(providerId, 'model-a', 'max');

    expect(useSettingsStore.getState().settings.ai.reasoningProviderOverrides?.[providerId]).toBe('high');
    expect(useSettingsStore.getState().settings.ai.reasoningModelOverrides?.[providerId]).toEqual({
      'model-a': 'max',
    });

    useSettingsStore.getState().setProviderReasoningEffort(providerId, null);
    useSettingsStore.getState().setModelReasoningEffort(providerId, 'model-a', null);

    expect(useSettingsStore.getState().settings.ai.reasoningProviderOverrides?.[providerId]).toBeUndefined();
    expect(useSettingsStore.getState().settings.ai.reasoningModelOverrides?.[providerId]).toBeUndefined();
  });

  it('exports only selected .oxide app settings sections and excludes local env vars by default', async () => {
    const mod = await import('@/store/settingsStore');
    installSettingsApiMocks(mod);
    const useSettingsStore = mod.useSettingsStore;

    useSettingsStore.setState((state) => ({
      settings: {
        ...state.settings,
        general: { ...state.settings.general, language: 'fr-FR', updateChannel: 'beta' },
        terminal: { ...state.settings.terminal, theme: 'oxide', scrollback: 4096 },
        localTerminal: {
          ...state.settings.localTerminal!,
          defaultShellId: 'zsh',
          customEnvVars: { NODE_AUTH_TOKEN: 'secret', PATH: '/tmp/bin' },
        },
      },
    }));

    const snapshotJson = mod.exportOxideAppSettingsSnapshot({
      selectedSections: ['general', 'localTerminal'],
    });

    expect(snapshotJson).toBeTruthy();

    const snapshot = JSON.parse(snapshotJson || '{}');
    expect(snapshot).toMatchObject({
      format: 'oxide-settings-sections-v1',
      version: 1,
      sectionIds: ['general', 'localTerminal'],
    });
    expect(snapshot.settings.general).toEqual({ language: 'fr-FR', updateChannel: 'beta' });
    expect(snapshot.settings.localTerminal.defaultShellId).toBe('zsh');
    expect(snapshot.settings.localTerminal.customEnvVars).toBeUndefined();
    expect(snapshot.settings.terminal).toBeUndefined();
    expect(snapshot.settings.ai).toBeUndefined();
  });

  it('merges only selected sectioned .oxide app settings on import', async () => {
    const mod = await import('@/store/settingsStore');
    installSettingsApiMocks(mod);
    const useSettingsStore = mod.useSettingsStore;

    useSettingsStore.setState((state) => ({
      settings: {
        ...state.settings,
        general: { ...state.settings.general, language: 'en', updateChannel: 'stable' },
        terminal: { ...state.settings.terminal, theme: 'oxide', scrollback: 3000 },
        connectionPool: { idleTimeoutSecs: 1800 },
      },
    }));

    apiMocks.sshSetPoolConfig.mockClear();
    i18nMocks.changeLanguage.mockClear();

    const imported = JSON.stringify({
      format: 'oxide-settings-sections-v1',
      version: 1,
      sectionIds: ['general', 'terminalAppearance', 'connections'],
      settings: {
        general: { language: 'ja', updateChannel: 'beta' },
        terminal: { theme: 'paper-oxide' },
        connectionPool: { idleTimeoutSecs: 900 },
      },
    });
    apiMocks.applyAppSettingsSnapshot.mockImplementationOnce(async () => {
      const current = useSettingsStore.getState().settings;
      return {
        imported: true,
        settings: mod.mergeWithDefaults({
          ...current,
          general: { ...current.general, language: 'ja', updateChannel: 'beta' },
          connectionPool: { idleTimeoutSecs: 900 },
        }),
        version: 3,
        updatedAt: Date.now(),
        migrationWarnings: [],
        validationWarnings: [],
        errors: [],
      };
    });

    const applied = await mod.applyImportedSettingsSnapshot(imported, {
      selectedSections: ['general', 'connections'],
    });

    expect(applied).toBe(true);
    expect(useSettingsStore.getState().settings.general.language).toBe('ja');
    expect(useSettingsStore.getState().settings.connectionPool?.idleTimeoutSecs).toBe(900);
    expect(useSettingsStore.getState().settings.terminal.theme).toBe('oxide');
    expect(i18nMocks.changeLanguage).toHaveBeenCalledWith('ja');

    await waitFor(() => {
      expect(apiMocks.sshSetPoolConfig).toHaveBeenCalledWith({
        idleTimeoutSecs: 900,
        maxConnections: 0,
        protectOnExit: true,
      });
    });
  });
});
