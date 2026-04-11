import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());
const writeToTerminalMock = vi.hoisted(() => vi.fn(() => true));
const findPaneBySessionIdMock = vi.hoisted(() => vi.fn(() => 'pane-1'));
const getActivePaneIdMock = vi.hoisted(() => vi.fn(() => 'pane-1'));
const getActivePaneMetadataMock = vi.hoisted(() => vi.fn(() => ({ sessionId: 'sess-1', terminalType: 'terminal', tabId: 'tab-1' })));
const getTerminalBufferMock = vi.hoisted(() => vi.fn(() => 'buffer text'));
const getTerminalSelectionMock = vi.hoisted(() => vi.fn(() => 'selected text'));

const appStoreState = vi.hoisted(() => ({
  tabs: [] as Array<Record<string, unknown>>,
  activeTabId: null as string | null,
  sidebarCollapsed: false,
  connections: new Map([
    ['conn-1', { id: 'conn-1', host: 'host', port: 22, username: 'user', state: 'active', refCount: 1, keepAlive: false, createdAt: '1', lastActive: '2', terminalIds: ['sess-1'] }],
  ]),
  savedConnections: [
    {
      id: 'saved-1',
      name: 'Prod',
      group: 'Ops',
      host: 'prod.example.com',
      port: 22,
      username: 'root',
      auth_type: 'password',
      key_path: null,
      cert_path: null,
      created_at: '2026-01-01T00:00:00Z',
      last_used_at: null,
      color: null,
      tags: ['prod'],
      agent_forwarding: true,
      proxy_chain: [],
    },
  ],
  loadSavedConnections: vi.fn(async () => undefined),
  refreshConnections: vi.fn(async () => undefined),
}));

const sessionTreeState = vi.hoisted(() => ({
  nodes: [
    {
      id: 'node-1',
      displayName: 'Node 1',
      host: 'host',
      port: 22,
      username: 'user',
      parentId: null,
      runtime: {
        status: 'active',
        connectionId: 'conn-1',
        terminalIds: ['sess-1'],
        sftpSessionId: null,
        errorMessage: null,
      },
    },
  ],
  getNode(nodeId: string) {
    return sessionTreeState.nodes.find((node) => node.id === nodeId) ?? null;
  },
  getNodeByTerminalId(sessionId: string) {
    return sessionTreeState.nodes.find((node) => node.runtime.terminalIds.includes(sessionId)) ?? null;
  },
}));

function createMockStore<T extends object>(state: T) {
  const listeners = new Set<(state: T) => void>();
  const selectorListeners = new Set<{
    selector: (value: T) => unknown;
    listener: (slice: unknown, previousSlice: unknown) => void;
    previousSlice: unknown;
  }>();

  return {
    getState: () => state,
    setState: (patch: Record<string, unknown> | ((prev: T) => Partial<T>)) => {
      const nextPatch = typeof patch === 'function' ? patch(state) : patch;
      Object.assign(state, nextPatch);
      for (const listener of listeners) listener(state);
      for (const entry of selectorListeners) {
        const nextSlice = entry.selector(state);
        if (nextSlice !== entry.previousSlice) {
          const previousSlice = entry.previousSlice;
          entry.previousSlice = nextSlice;
          entry.listener(nextSlice, previousSlice);
        }
      }
    },
    subscribe: (selectorOrListener: ((value: T) => unknown) | ((state: T) => void), maybeListener?: (slice: unknown, previousSlice: unknown) => void) => {
      if (maybeListener) {
        const entry = {
          selector: selectorOrListener as (value: T) => unknown,
          listener: maybeListener,
          previousSlice: (selectorOrListener as (value: T) => unknown)(state),
        };
        selectorListeners.add(entry);
        return () => selectorListeners.delete(entry);
      }

      const listener = selectorOrListener as (state: T) => void;
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

const appStoreMock = vi.hoisted(() => createMockStore(appStoreState));
const sessionTreeStoreMock = vi.hoisted(() => createMockStore(sessionTreeState));
const settingsStoreState = vi.hoisted(() => ({
  settings: {
    terminal: { theme: 'default', fontSize: 14 },
    appearance: { uiDensity: 'comfortable' },
    general: { language: 'en' },
    reconnect: { enabled: true },
    ai: { activeProviderId: null, providers: [] },
  },
  setLanguage: vi.fn(async (language: string) => {
    settingsStoreMock.setState((state) => ({
      settings: {
        ...state.settings,
        general: { ...state.settings.general, language },
      },
    }));
  }),
  updateTerminal: vi.fn((key: 'theme' | 'fontSize', value: string | number) => {
    settingsStoreMock.setState((state) => ({
      settings: {
        ...state.settings,
        terminal: { ...state.settings.terminal, [key]: value },
      },
    }));
  }),
  updateAppearance: vi.fn((key: 'uiDensity', value: string) => {
    settingsStoreMock.setState((state) => ({
      settings: {
        ...state.settings,
        appearance: { ...state.settings.appearance, [key]: value },
      },
    }));
  }),
  updateReconnect: vi.fn((key: 'enabled', value: boolean) => {
    settingsStoreMock.setState((state) => ({
      settings: {
        ...state.settings,
        reconnect: { ...state.settings.reconnect, [key]: value },
      },
    }));
  }),
}));
const settingsStoreMock = vi.hoisted(() => createMockStore(settingsStoreState));

const eventHandlers = vi.hoisted(() => new Map<string, Set<(payload: unknown) => void>>());
const pluginEventBridgeMock = vi.hoisted(() => ({
  on: vi.fn((name: string, handler: (payload: unknown) => void) => {
    if (!eventHandlers.has(name)) eventHandlers.set(name, new Set());
    eventHandlers.get(name)!.add(handler);
    return () => eventHandlers.get(name)?.delete(handler);
  }),
  emit: vi.fn((name: string, payload: unknown) => {
    for (const handler of eventHandlers.get(name) ?? []) handler(payload);
  }),
}));

const storageManagerMock = vi.hoisted(() => ({ get: vi.fn(() => null), set: vi.fn(), remove: vi.fn() }));
const settingsManagerMock = vi.hoisted(() => ({ get: vi.fn(() => null), set: vi.fn(), onChange: vi.fn(() => vi.fn()) }));
const i18nManagerMock = vi.hoisted(() => ({ t: vi.fn((key: string) => key), getLanguage: vi.fn(() => 'en'), onLanguageChange: vi.fn(() => vi.fn()) }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));
vi.mock('@/store/appStore', () => ({ useAppStore: appStoreMock }));
vi.mock('@/store/sessionTreeStore', () => ({ useSessionTreeStore: sessionTreeStoreMock }));
vi.mock('@/lib/plugin/pluginStorage', () => ({ createPluginStorage: vi.fn(() => storageManagerMock) }));
vi.mock('@/lib/plugin/pluginEventBridge', () => ({ pluginEventBridge: pluginEventBridgeMock }));
vi.mock('@/lib/plugin/pluginSettingsManager', () => ({ createPluginSettingsManager: vi.fn(() => settingsManagerMock) }));
vi.mock('@/lib/plugin/pluginI18nManager', () => ({ createPluginI18nManager: vi.fn(() => i18nManagerMock) }));
vi.mock('@/lib/plugin/pluginUtils', () => ({ toSnapshot: (value: unknown) => value }));
vi.mock('@/lib/plugin/pluginThrottledEvents', () => ({ createThrottledEmitter: vi.fn(() => ({ push: vi.fn(), dispose: vi.fn() })) }));
vi.mock('@/lib/terminalRegistry', () => ({
  getActivePaneId: getActivePaneIdMock,
  getActivePaneMetadata: getActivePaneMetadataMock,
  findPaneBySessionId: findPaneBySessionIdMock,
  getTerminalBuffer: getTerminalBufferMock,
  getTerminalSelection: getTerminalSelectionMock,
  writeToTerminal: writeToTerminalMock,
}));
vi.mock('@/store/transferStore', () => ({ useTransferStore: createMockStore({ transfers: new Map() }) }));
vi.mock('@/store/profilerStore', () => ({ useProfilerStore: createMockStore({ connections: new Map() }) }));
vi.mock('@/store/eventLogStore', () => ({ useEventLogStore: createMockStore({ entries: [] }) }));
vi.mock('@/store/ideStore', () => ({ useIdeStore: createMockStore({ nodeId: null, project: null, tabs: [], activeTabId: null }) }));
vi.mock('@/store/aiChatStore', () => ({ useAiChatStore: createMockStore({ conversations: [] }) }));
vi.mock('@/store/settingsStore', () => ({ useSettingsStore: settingsStoreMock }));

import { usePluginStore } from '@/store/pluginStore';
import { buildPluginContext, cleanupPluginAssets } from '@/lib/plugin/pluginContextFactory';
import type { PluginManifest } from '@/types/plugin';

function resetPluginStore() {
  usePluginStore.setState({
    plugins: new Map(),
    tabViews: new Map(),
    sidebarPanels: new Map(),
    inputInterceptors: [],
    outputProcessors: [],
    shortcuts: new Map(),
    commands: new Map(),
    contextMenuItems: new Map(),
    statusBarItems: new Map(),
    keybindings: new Map(),
    disposables: new Map(),
    registryEntries: [],
    installProgress: new Map(),
    availableUpdates: [],
    pluginLogs: new Map(),
  });
}

function manifest(): PluginManifest {
  return {
    id: 'plugin.example',
    name: 'Example Plugin',
    version: '1.0.0',
    main: 'dist/index.js',
    contributes: {
      tabs: [{ id: 'inspector', title: 'Inspector', icon: 'Puzzle' }],
      sidebarPanels: [{ id: 'panel', title: 'Panel', icon: 'Puzzle', position: 'bottom' }],
      terminalHooks: { inputInterceptor: true, outputProcessor: true, shortcuts: [{ key: 'Ctrl+K', command: 'focus' }] },
      apiCommands: ['ssh_get_pool_stats'],
    },
  };
}

describe('pluginContextFactory', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventHandlers.clear();
    resetPluginStore();
    appStoreState.tabs = [];
    appStoreState.activeTabId = null;
    appStoreState.savedConnections = [
      {
        id: 'saved-1',
        name: 'Prod',
        group: 'Ops',
        host: 'prod.example.com',
        port: 22,
        username: 'root',
        auth_type: 'password',
        key_path: null,
        cert_path: null,
        created_at: '2026-01-01T00:00:00Z',
        last_used_at: null,
        color: null,
        tags: ['prod'],
        agent_forwarding: true,
        proxy_chain: [],
      },
    ];
    appStoreState.loadSavedConnections.mockResolvedValue(undefined);
    appStoreState.refreshConnections.mockResolvedValue(undefined);
    settingsStoreState.settings = {
      terminal: { theme: 'default', fontSize: 14 },
      appearance: { uiDensity: 'comfortable' },
      general: { language: 'en' },
      reconnect: { enabled: true },
      ai: { activeProviderId: null, providers: [] },
    };
    document.head.innerHTML = '';
    (globalThis as typeof globalThis & { __blobCounter?: number }).__blobCounter = 0;
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => 'blob:plugin-asset'),
      revokeObjectURL: vi.fn(),
    });
    vi.stubGlobal('window', Object.assign(window, { __OXIDE__: { version: '1.2.3' } }));
  });

  it('builds a frozen context, registers tab views, and opens tabs idempotently', () => {
    const context = buildPluginContext(manifest());
    const component = (() => null) as never;

    expect(Object.isFrozen(context)).toBe(true);
    expect(Object.isFrozen(context.ui)).toBe(true);

    const disposable = context.ui.registerTabView('inspector', component);
    context.ui.openTab('inspector');
    context.ui.openTab('inspector');

    expect(usePluginStore.getState().tabViews.has('plugin.example:inspector')).toBe(true);
    expect(appStoreState.tabs).toHaveLength(1);
    expect(appStoreState.activeTabId).toBe(appStoreState.tabs[0].id as string);

    disposable.dispose();
    expect(usePluginStore.getState().tabViews.has('plugin.example:inspector')).toBe(false);
  });

  it('namespaces custom events and stops receiving them after dispose', () => {
    const context = buildPluginContext(manifest());
    const handler = vi.fn();

    const disposable = context.events.on('ready', handler);
    context.events.emit('ready', { ok: true });
    expect(handler).toHaveBeenCalledWith({ ok: true });

    disposable.dispose();
    context.events.emit('ready', { ok: false });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('routes terminal helpers through node session resolution and pane registry', () => {
    const context = buildPluginContext(manifest());

    context.terminal.writeToNode('node-1', 'pwd\n');

    expect(findPaneBySessionIdMock).toHaveBeenCalledWith('sess-1');
    expect(writeToTerminalMock).toHaveBeenCalledWith('pane-1', 'pwd\n');
    expect(context.terminal.getNodeBuffer('node-1')).toBe('buffer text');
    expect(context.terminal.getNodeSelection('node-1')).toBe('selected text');
  });

  it('exposes the last focused terminal target and writes to local terminals', () => {
    const context = buildPluginContext(manifest());

    getActivePaneMetadataMock.mockReturnValueOnce({
      sessionId: 'local-1',
      terminalType: 'local_terminal',
      tabId: 'tab-local',
    });
    getActivePaneIdMock.mockReturnValueOnce('pane-local');

    expect(context.terminal.getActiveTarget()).toEqual({
      sessionId: 'local-1',
      terminalType: 'local_terminal',
      nodeId: null,
      connectionId: null,
      connectionState: 'active',
      label: 'local-1',
    });
    expect(context.terminal.writeToActive('echo test\n')).toBe(true);
    expect(writeToTerminalMock).toHaveBeenCalledWith('pane-local', 'echo test\n');
  });

  it('loads plugin assets and cleanupPluginAssets removes styles and blob URLs', async () => {
    const context = buildPluginContext(manifest());
    invokeMock
      .mockResolvedValueOnce(Array.from(new TextEncoder().encode('body { color: red; }')))
      .mockResolvedValueOnce([1, 2, 3]);

    await context.assets.loadCSS('./style.css');
    const blobUrl = await context.assets.getAssetUrl('./icon.png');

    expect(document.head.querySelector('style[data-plugin="plugin.example"]')).not.toBeNull();
    expect(blobUrl).toBe('blob:plugin-asset');

    cleanupPluginAssets('plugin.example');

    expect(document.head.querySelector('style[data-plugin="plugin.example"]')).toBeNull();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:plugin-asset');
  });

  it('allows whitelisted backend commands and rejects undeclared ones', async () => {
    const context = buildPluginContext(manifest());
    invokeMock.mockResolvedValue({ active_connections: 1, total_sessions: 2 });

    await expect(context.api.invoke('ssh_get_pool_stats')).resolves.toEqual({ active_connections: 1, total_sessions: 2 });
    await expect(context.api.invoke('read_plugin_file')).rejects.toThrow(/not whitelisted/i);
  });

  it('exposes saved-connection sync helpers and applies skip conflict strategy', async () => {
    const context = buildPluginContext(manifest());
    const importPreview = {
      totalConnections: 2,
      unchanged: ['Prod'],
      willRename: [['Staging', 'Staging (Copy)']] as [string, string][],
      willSkip: [],
      willReplace: [],
      willMerge: [],
      hasEmbeddedKeys: false,
      totalForwards: 0,
    };

    invokeMock
      .mockResolvedValueOnce({
        revision: 'snap-rev',
        exportedAt: '2026-01-01T00:00:00Z',
        records: [{
          id: 'saved-1',
          revision: 'rec-rev',
          updatedAt: '2026-01-01T00:00:00Z',
          deleted: false,
          payload: {
            ...appStoreState.savedConnections[0],
            agent_forwarding: true,
          },
        }],
      })
      .mockResolvedValueOnce({
        applied: 1,
        skipped: 0,
        conflicts: 0,
      })
      .mockResolvedValueOnce({
        savedConnectionsRevision: 'local-rev',
        savedConnectionsUpdatedAt: '2026-01-02T00:00:00Z',
      })
      .mockResolvedValueOnce({
        revision: 'forward-rev',
        exportedAt: '2026-01-03T00:00:00Z',
        records: [{
          id: 'forward-1',
          revision: 'forward-rec-rev',
          updatedAt: '2026-01-03T00:00:00Z',
          deleted: false,
          payload: {
            id: 'forward-1',
            session_id: '',
            owner_connection_id: 'saved-1',
            forward_type: 'local',
            bind_address: '127.0.0.1',
            bind_port: 8080,
            target_host: 'localhost',
            target_port: 80,
            auto_start: true,
            created_at: '2026-01-03T00:00:00Z',
            description: 'web',
          },
        }],
      })
      .mockResolvedValueOnce({
        totalConnections: 1,
        missingKeys: [],
        connectionsWithKeys: 0,
        connectionsWithPasswords: 1,
        connectionsWithAgent: 0,
        totalKeyBytes: 0,
        canExport: true,
      })
      .mockResolvedValueOnce([1, 2, 3])
      .mockResolvedValueOnce({
        exported_at: '2026-01-01T00:00:00Z',
        exported_by: 'OxideTerm 1.1.13',
        description: 'sync payload',
        num_connections: 1,
        connection_names: ['Prod'],
      })
      .mockResolvedValueOnce(importPreview)
      .mockResolvedValueOnce({
        imported: 1,
        skipped: 1,
        merged: 0,
        replaced: 0,
        renamed: 0,
        errors: [],
        renames: [],
      });

    const savedConnections = context.sync.listSavedConnections();
    expect(savedConnections).toHaveLength(1);
    expect(savedConnections[0].name).toBe('Prod');
    expect(Object.isFrozen(savedConnections[0])).toBe(true);

    const refreshed = await context.sync.refreshSavedConnections();
    expect(appStoreState.loadSavedConnections).toHaveBeenCalledTimes(1);
    expect(refreshed[0].id).toBe('saved-1');

    const snapshot = await context.sync.exportSavedConnectionsSnapshot();
    expect(snapshot.records[0].payload?.agent_forwarding).toBe(true);
    expect(invokeMock).toHaveBeenNthCalledWith(1, 'export_saved_connections_snapshot');

    const applyResult = await context.sync.applySavedConnectionsSnapshot(snapshot, {
      conflictStrategy: 'merge',
    });
    expect(applyResult.applied).toBe(1);
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'apply_saved_connections_snapshot', {
      snapshot,
      conflictStrategy: 'merge',
    });
    expect(appStoreState.refreshConnections).toHaveBeenCalledTimes(1);
    expect(appStoreState.loadSavedConnections).toHaveBeenCalledTimes(2);

    const metadata = await context.sync.getLocalSyncMetadata();
    expect(metadata.savedConnectionsRevision).toBe('local-rev');
    expect(metadata.savedForwardsRevision).toBe('forward-rev');
    expect(invokeMock).toHaveBeenNthCalledWith(3, 'get_local_sync_metadata');
    expect(invokeMock).toHaveBeenNthCalledWith(4, 'export_saved_forwards_snapshot');

    const preflight = await context.sync.preflightExport();
    expect(preflight.totalConnections).toBe(1);
    expect(invokeMock).toHaveBeenNthCalledWith(5, 'preflight_export', {
      connectionIds: ['saved-1'],
      embedKeys: null,
    });

    const exported = await context.sync.exportOxide({ password: 'StrongPass!123' });
    expect(exported).toBeInstanceOf(Uint8Array);
    expect(Array.from(exported)).toEqual([1, 2, 3]);

    const oxideMetadata = await context.sync.validateOxide(new Uint8Array([9, 8, 7]));
    expect(oxideMetadata.connection_names).toEqual(['Prod']);

    const preview = await context.sync.previewImport(new Uint8Array([4, 5, 6]), 'ImportPass!123');
    expect(preview.willRename).toEqual([['Staging', 'Staging (Copy)']]);

    const result = await context.sync.importOxide(new Uint8Array([4, 5, 6]), 'ImportPass!123', {
      conflictStrategy: 'skip',
    });
    expect(result.imported).toBe(1);
    expect(invokeMock).toHaveBeenNthCalledWith(8, 'preview_oxide_import', {
      fileData: [4, 5, 6],
      password: 'ImportPass!123',
      conflictStrategy: null,
    });
    expect(invokeMock).toHaveBeenNthCalledWith(9, 'import_from_oxide', {
      fileData: [4, 5, 6],
      password: 'ImportPass!123',
      selectedNames: null,
      conflictStrategy: 'skip',
    });
    expect(appStoreState.loadSavedConnections).toHaveBeenCalledTimes(3);
  });

  it('exposes saved-forward sync helpers and syncable settings helpers', async () => {
    const context = buildPluginContext(manifest());

    invokeMock
      .mockResolvedValueOnce({
        revision: 'forward-rev',
        exportedAt: '2026-01-03T00:00:00Z',
        records: [{
          id: 'forward-1',
          revision: 'forward-rec-rev',
          updatedAt: '2026-01-03T00:00:00Z',
          deleted: false,
          payload: {
            id: 'forward-1',
            session_id: '',
            owner_connection_id: 'saved-1',
            forward_type: 'local',
            bind_address: '127.0.0.1',
            bind_port: 8080,
            target_host: 'localhost',
            target_port: 80,
            auto_start: true,
            created_at: '2026-01-03T00:00:00Z',
            description: 'web',
          },
        }],
      })
      .mockResolvedValueOnce({ applied: 1, skipped: 0 })
      .mockResolvedValueOnce({
        revision: 'forward-rev-2',
        exportedAt: '2026-01-04T00:00:00Z',
        records: [{
          id: 'forward-2',
          revision: 'forward-rec-rev-2',
          updatedAt: '2026-01-04T00:00:00Z',
          deleted: false,
          payload: {
            id: 'forward-2',
            session_id: '',
            owner_connection_id: 'saved-1',
            forward_type: 'remote',
            bind_address: '0.0.0.0',
            bind_port: 3000,
            target_host: 'localhost',
            target_port: 3000,
            auto_start: false,
            created_at: '2026-01-04T00:00:00Z',
            description: 'api',
          },
        }],
      });

    const snapshot = await context.forward.exportSavedForwardsSnapshot();
    expect(snapshot.records).toHaveLength(1);
    expect(context.forward.listSavedForwards()).toHaveLength(1);

    const applyResult = await context.forward.applySavedForwardsSnapshot(snapshot);
    expect(applyResult.applied).toBe(1);
    expect(invokeMock).toHaveBeenNthCalledWith(1, 'export_saved_forwards_snapshot');
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'apply_saved_forwards_snapshot', { snapshot });
    expect(invokeMock).toHaveBeenNthCalledWith(3, 'export_saved_forwards_snapshot');
    expect(context.forward.listSavedForwards()[0].id).toBe('forward-2');

    const exportedSettings = await context.settings.exportSyncableSettings();
    expect(exportedSettings.payload.appearance?.language).toBe('en');
    expect(exportedSettings.payload.terminal?.fontSize).toBe(14);

    await context.settings.applySyncableSettings({
      appearance: { language: 'ja', uiDensity: 'compact' },
      terminal: { fontSize: 16, theme: 'solarized-dark' },
      reconnect: { autoReconnect: false },
    });

    expect(settingsStoreState.setLanguage).toHaveBeenCalledWith('ja');
    expect(settingsStoreState.updateAppearance).toHaveBeenCalledWith('uiDensity', 'compact');
    expect(settingsStoreState.updateTerminal).toHaveBeenCalledWith('fontSize', 16);
    expect(settingsStoreState.updateTerminal).toHaveBeenCalledWith('theme', 'solarized-dark');
    expect(settingsStoreState.updateReconnect).toHaveBeenCalledWith('enabled', false);
  });

  it('passes merge conflict strategy through to preview and import', async () => {
    const context = buildPluginContext(manifest());

    invokeMock
      .mockResolvedValueOnce({
        totalConnections: 1,
        unchanged: [],
        willRename: [],
        willSkip: [],
        willReplace: [],
        willMerge: ['Prod'],
        hasEmbeddedKeys: false,
        totalForwards: 0,
      })
      .mockResolvedValueOnce({
        imported: 1,
        skipped: 0,
        merged: 1,
        replaced: 0,
        renamed: 0,
        errors: [],
        renames: [],
      });

    const fileData = new Uint8Array([7, 8, 9]);
    const preview = await context.sync.previewImport(fileData, 'ImportPass!123', {
      conflictStrategy: 'merge',
    });
    const result = await context.sync.importOxide(fileData, 'ImportPass!123', {
      conflictStrategy: 'merge',
    });

    expect(preview.willMerge).toEqual(['Prod']);
    expect(result.merged).toBe(1);
    expect(invokeMock).toHaveBeenNthCalledWith(1, 'preview_oxide_import', {
      fileData: [7, 8, 9],
      password: 'ImportPass!123',
      conflictStrategy: 'merge',
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'import_from_oxide', {
      fileData: [7, 8, 9],
      password: 'ImportPass!123',
      selectedNames: null,
      conflictStrategy: 'merge',
    });
  });

  it('stores plugin secrets with a plugin-scoped keychain namespace', async () => {
    const context = buildPluginContext({ ...manifest(), id: 'plugin:example' });
    invokeMock
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce('secret-token')
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(undefined);

    await context.secrets.set('webdav:token', 'secret-token');
    await expect(context.secrets.get('webdav:token')).resolves.toBe('secret-token');
    await expect(context.secrets.has('webdav:token')).resolves.toBe(true);
    await context.secrets.delete('webdav:token');

    expect(invokeMock).toHaveBeenNthCalledWith(1, 'set_plugin_secret', {
      pluginId: 'plugin:example',
      key: 'webdav:token',
      value: 'secret-token',
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'get_plugin_secret', {
      pluginId: 'plugin:example',
      key: 'webdav:token',
    });
    expect(invokeMock).toHaveBeenNthCalledWith(3, 'has_plugin_secret', {
      pluginId: 'plugin:example',
      key: 'webdav:token',
    });
    expect(invokeMock).toHaveBeenNthCalledWith(4, 'delete_plugin_secret', {
      pluginId: 'plugin:example',
      key: 'webdav:token',
    });
  });

  it('refreshes app stores after external sync', async () => {
    const context = buildPluginContext(manifest());
    await context.app.refreshAfterExternalSync({ connections: true });

    expect(appStoreState.refreshConnections).toHaveBeenCalledTimes(1);
    expect(appStoreState.loadSavedConnections).toHaveBeenCalledTimes(1);
  });

  it('notifies plugins when saved connections change', () => {
    const context = buildPluginContext(manifest());
    const handler = vi.fn();

    const disposable = context.sync.onSavedConnectionsChange(handler);
    appStoreMock.setState({
      savedConnections: [
        ...appStoreState.savedConnections,
        {
          id: 'saved-2',
          name: 'Stage',
          group: null,
          host: 'stage.example.com',
          port: 22,
          username: 'deploy',
          auth_type: 'key',
          key_path: '~/.ssh/id_ed25519',
          cert_path: null,
          created_at: '2026-01-02T00:00:00Z',
          last_used_at: null,
          color: null,
          tags: ['stage'],
          agent_forwarding: false,
          proxy_chain: [],
        },
      ],
    });

    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler.mock.calls[0][0]).toHaveLength(2);

    disposable.dispose();
  });
});