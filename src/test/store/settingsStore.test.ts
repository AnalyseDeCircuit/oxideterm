import { beforeEach, describe, expect, it, vi } from 'vitest';
import { waitFor } from '@testing-library/react';

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
  return mod.useSettingsStore;
}

function buildSavedSettings(overrides: Record<string, unknown> = {}) {
  return {
    version: 2,
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
    expect(settings.ai.toolUse?.autoApproveTools.read_file).toBe(true);
    expect(settings.ai.toolUse?.autoApproveTools.terminal_exec).toBe(false);

    const persisted = JSON.parse(localStorage.getItem('oxide-settings-v2') || '{}');
    expect(persisted.ai.providers.length).toBeGreaterThan(0);
    expect(persisted.ai.toolUse.autoApproveTools.read_file).toBe(true);
  });

  it('clears legacy localStorage keys when loading defaults', async () => {
    localStorage.setItem('oxide-settings', '{"legacy":true}');
    localStorage.setItem('oxide-ui-state', '{"sidebar":false}');

    const useSettingsStore = await loadSettingsStore();

    expect(useSettingsStore.getState().settings.version).toBe(2);
    expect(localStorage.getItem('oxide-settings')).toBeNull();
    expect(localStorage.getItem('oxide-ui-state')).toBeNull();
  });

  it('uses derived backend hot-buffer defaults', async () => {
    const useSettingsStore = await loadSettingsStore();

    const buffer = useSettingsStore.getState().settings.buffer;
    expect(buffer.maxLines).toBe(6000);
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

    const persisted = JSON.parse(localStorage.getItem('oxide-settings-v2') || '{}');
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
    const persisted = JSON.parse(localStorage.getItem('oxide-settings-v2') || '{}');
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
    const persisted = JSON.parse(localStorage.getItem('oxide-settings-v2') || '{}');
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

  it('derives backend hot-buffer lines from scrollback changes', async () => {
    const useSettingsStore = await loadSettingsStore();

    useSettingsStore.getState().updateTerminal('scrollback', 4000);
    expect(useSettingsStore.getState().settings.terminal.scrollback).toBe(4000);
    expect(useSettingsStore.getState().settings.buffer.maxLines).toBe(8000);

    useSettingsStore.getState().updateTerminal('scrollback', 100000);
    expect(useSettingsStore.getState().settings.terminal.scrollback).toBe(20000);
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

    const persisted = JSON.parse(localStorage.getItem('oxide-settings-v2') || '{}');
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

    const mod = await import('@/store/settingsStore');
    const useSettingsStore = mod.useSettingsStore;

    mod.initializeSettings();

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

    const persisted = JSON.parse(localStorage.getItem('oxide-settings-v2') || '{}');
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

    const persisted = JSON.parse(localStorage.getItem('oxide-settings-v2') || '{}');
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
