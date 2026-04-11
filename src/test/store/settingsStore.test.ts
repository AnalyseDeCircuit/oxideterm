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

  it('uses metadata-only buffer persistence defaults', async () => {
    const useSettingsStore = await loadSettingsStore();

    const buffer = useSettingsStore.getState().settings.buffer;
    expect(buffer.maxLines).toBe(8000);
  });

  it('setLanguage persists app_lang and delegates to i18n', async () => {
    const useSettingsStore = await loadSettingsStore();

    await useSettingsStore.getState().setLanguage('fr-FR');

    expect(useSettingsStore.getState().settings.general.language).toBe('fr-FR');
    expect(localStorage.getItem('app_lang')).toBe('fr-FR');
    expect(i18nMocks.changeLanguage).toHaveBeenCalledWith('fr-FR');
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

    useSettingsStore.getState().updateSftp('maxConcurrentTransfers', 5);
    await waitFor(() => {
      expect(apiMocks.sftpUpdateSettings).toHaveBeenCalledWith(5, 0);
    });

    useSettingsStore.getState().updateSftp('speedLimitEnabled', true);
    useSettingsStore.getState().updateSftp('speedLimitKBps', 256);
    await waitFor(() => {
      expect(apiMocks.sftpUpdateSettings.mock.calls).toContainEqual([5, 256]);
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
});