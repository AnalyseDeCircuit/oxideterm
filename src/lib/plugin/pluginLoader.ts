/**
 * Plugin Loader
 *
 * Handles the full plugin lifecycle:
 * - Discovery: scan plugins directory via Rust backend
 * - Validation: check manifest required fields and version compatibility
 * - Loading: fetch ESM bundle → Blob URL → dynamic import → activate
 * - Unloading: deactivate → cleanup all registrations
 * - Error circuit breaker: auto-disable plugins that error too frequently
 */

import { api } from '../api';
import { usePluginStore } from '../../store/pluginStore';
import { buildPluginContext } from './pluginContextFactory';
import { loadPluginI18n, removePluginI18n } from './pluginI18nManager';
import type { PluginManifest, PluginModule, PluginGlobalConfig } from '../../types/plugin';

/** Current OxideTerm version for engine compatibility checks */
const OXIDETERM_VERSION = '1.6.2';

/** Timeout for activate/deactivate calls (ms) */
const LIFECYCLE_TIMEOUT = 5000;

/** Error circuit breaker: max errors within window before auto-disable */
const MAX_ERRORS = 10;
const ERROR_WINDOW_MS = 60_000;

/** Per-plugin error tracking */
const errorTrackers = new Map<string, { count: number; windowStart: number }>();

/**
 * Check if a plugin should be circuit-broken due to excessive errors.
 * Returns true if the plugin should be disabled.
 */
export function trackPluginError(pluginId: string): boolean {
  const now = Date.now();
  let tracker = errorTrackers.get(pluginId);

  if (!tracker || now - tracker.windowStart > ERROR_WINDOW_MS) {
    tracker = { count: 0, windowStart: now };
    errorTrackers.set(pluginId, tracker);
  }

  tracker.count++;

  if (tracker.count >= MAX_ERRORS) {
    console.error(`[PluginLoader] Plugin "${pluginId}" exceeded error limit (${MAX_ERRORS} in ${ERROR_WINDOW_MS / 1000}s), auto-disabling`);
    errorTrackers.delete(pluginId);

    // Persist the disabled state so the plugin stays disabled across restarts
    persistAutoDisable(pluginId);

    return true;
  }

  return false;
}

/**
 * Persist auto-disable: update config and set store state to 'disabled'.
 * Called by the circuit breaker when a plugin exceeds the error limit.
 */
async function persistAutoDisable(pluginId: string): Promise<void> {
  try {
    const config = await loadPluginGlobalConfig();
    config.plugins[pluginId] = { enabled: false };
    await savePluginGlobalConfig(config);
    usePluginStore.getState().setPluginState(pluginId, 'disabled', 'Auto-disabled: exceeded error limit');
  } catch (err) {
    console.error(`[PluginLoader] Failed to persist auto-disable for "${pluginId}":`, err);
  }
}

/**
 * Validate a plugin manifest for required fields.
 */
function validateManifest(manifest: PluginManifest): string | null {
  if (!manifest.id || typeof manifest.id !== 'string') return 'Missing or invalid "id"';
  if (!manifest.name || typeof manifest.name !== 'string') return 'Missing or invalid "name"';
  if (!manifest.version || typeof manifest.version !== 'string') return 'Missing or invalid "version"';
  if (!manifest.main || typeof manifest.main !== 'string') return 'Missing or invalid "main"';

  // Check engine compatibility (simple semver >=)
  const required = manifest.engines?.oxideterm;
  if (required) {
    const match = required.match(/^>=?\s*(\d+\.\d+\.\d+)/);
    if (match) {
      const requiredParts = match[1].split('.').map(Number);
      const currentParts = OXIDETERM_VERSION.split('.').map(Number);
      for (let i = 0; i < 3; i++) {
        if (currentParts[i] > requiredParts[i]) break;
        if (currentParts[i] < requiredParts[i]) {
          return `Requires OxideTerm >=${match[1]}, current is ${OXIDETERM_VERSION}`;
        }
      }
    }
  }

  return null;
}

/**
 * Run a function with a timeout. Rejects if the function doesn't resolve in time.
 */
function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms);
    promise.then(
      (val) => { clearTimeout(timer); resolve(val); },
      (err) => { clearTimeout(timer); reject(err); },
    );
  });
}

/**
 * Discover all installed plugins by scanning the plugins directory.
 */
export async function discoverPlugins(): Promise<PluginManifest[]> {
  try {
    const manifests = await api.pluginList();
    return manifests;
  } catch (err) {
    console.error('[PluginLoader] Failed to discover plugins:', err);
    return [];
  }
}

/**
 * Load and activate a single plugin.
 */
export async function loadPlugin(manifest: PluginManifest): Promise<void> {
  const { id } = manifest;
  const store = usePluginStore.getState();

  // Validate manifest
  const validationError = validateManifest(manifest);
  if (validationError) {
    store.setPluginState(id, 'error', validationError);
    console.error(`[PluginLoader] Plugin "${id}" validation failed: ${validationError}`);
    return;
  }

  store.setPluginState(id, 'loading');

  try {
    // 1. Read the plugin's entry file
    const mainPath = manifest.main.replace(/^\.\//, '');
    const fileBytes = await api.pluginReadFile(id, mainPath);
    const blob = new Blob([new Uint8Array(fileBytes)], { type: 'application/javascript' });
    const blobUrl = URL.createObjectURL(blob);

    let module: PluginModule;
    try {
      // 2. Dynamic import
      module = await import(/* @vite-ignore */ blobUrl);
    } finally {
      // Always revoke blob URL
      URL.revokeObjectURL(blobUrl);
    }

    // Validate module exports
    if (typeof module.activate !== 'function') {
      throw new Error('Plugin module must export an "activate" function');
    }

    store.setPluginModule(id, module);

    // 3. Load i18n resources if declared
    if (manifest.locales) {
      try {
        await loadPluginLocales(id, manifest.locales);
      } catch (err) {
        console.warn(`[PluginLoader] Failed to load i18n for plugin "${id}":`, err);
      }
    }

    // 4. Build the membrane context
    const ctx = buildPluginContext(manifest);

    // 5. Activate with timeout
    const activateResult = module.activate(ctx);
    if (activateResult instanceof Promise) {
      await withTimeout(activateResult, LIFECYCLE_TIMEOUT, `Plugin "${id}" activate()`);
    }

    // Guard: if the plugin was removed/unloaded while activate() was running
    // (e.g. refresh removed it mid-load), bail out instead of marking active.
    const postActivateInfo = store.getPlugin(id);
    if (!postActivateInfo || postActivateInfo.state === 'inactive' || postActivateInfo.state === 'disabled') {
      console.warn(`[PluginLoader] Plugin "${id}" was removed/disabled during activation, skipping`);
      store.cleanupPlugin(id);
      removePluginI18n(id);
      return;
    }

    store.setPluginState(id, 'active');
    console.log(`[PluginLoader] Plugin "${id}" v${manifest.version} activated successfully`);
  } catch (err) {
    const errorMsg = err instanceof Error ? err.message : String(err);

    // Clean up any partial state left by a failed activation:
    // - module reference (set before activate() was called)
    // - i18n resources (loaded before activate() was called)
    // - any disposables registered during a partially-completed activate()
    store.cleanupPlugin(id);
    removePluginI18n(id);

    store.setPluginState(id, 'error', errorMsg);
    console.error(`[PluginLoader] Failed to load plugin "${id}":`, errorMsg);
  }
}

/**
 * Unload and deactivate a plugin.
 */
export async function unloadPlugin(pluginId: string): Promise<void> {
  const store = usePluginStore.getState();
  const pluginInfo = store.getPlugin(pluginId);

  if (!pluginInfo) {
    console.warn(`[PluginLoader] Plugin "${pluginId}" not found`);
    return;
  }

  // Call deactivate if available
  if (pluginInfo.module?.deactivate) {
    try {
      const result = pluginInfo.module.deactivate();
      if (result instanceof Promise) {
        await withTimeout(result, LIFECYCLE_TIMEOUT, `Plugin "${pluginId}" deactivate()`);
      }
    } catch (err) {
      console.warn(`[PluginLoader] Error during deactivate() for "${pluginId}":`, err);
    }
  }

  // Clean up all registrations, disposables, and close plugin tabs
  store.cleanupPlugin(pluginId);

  // Remove i18n resources
  removePluginI18n(pluginId);

  // Close any open tabs for this plugin
  const { useAppStore } = await import('../../store/appStore');
  const appState = useAppStore.getState();
  const pluginTabs = appState.tabs.filter(
    (t) => t.type === 'plugin' && t.pluginTabId?.startsWith(`${pluginId}:`),
  );
  for (const tab of pluginTabs) {
    await appState.closeTab(tab.id);
  }

  // Clear error tracker
  errorTrackers.delete(pluginId);

  // Respect 'disabled' state — if the plugin was auto-disabled by the circuit
  // breaker (persistAutoDisable), don't overwrite with 'inactive'.
  const currentState = store.getPlugin(pluginId)?.state;
  if (currentState !== 'disabled') {
    store.setPluginState(pluginId, 'inactive');
  }
  console.log(`[PluginLoader] Plugin "${pluginId}" unloaded`);
}

/**
 * Load locale files for a plugin from its locales directory.
 */
async function loadPluginLocales(pluginId: string, localesPath: string): Promise<void> {
  const basePath = localesPath.replace(/^\.\//, '');

  // Try loading common language files
  const languages = ['en', 'zh-CN', 'ja', 'ko', 'fr-FR', 'de', 'es-ES', 'pt-BR', 'it', 'vi', 'zh-TW'];
  const locales: Record<string, Record<string, string>> = {};

  for (const lang of languages) {
    try {
      const fileBytes = await api.pluginReadFile(pluginId, `${basePath}/${lang}.json`);
      const text = new TextDecoder().decode(new Uint8Array(fileBytes));
      locales[lang] = JSON.parse(text);
    } catch {
      // Locale file doesn't exist for this language — skip
    }
  }

  if (Object.keys(locales).length > 0) {
    await loadPluginI18n(pluginId, locales);
  }
}

/**
 * Load plugin global configuration (enabled/disabled state).
 */
export async function loadPluginGlobalConfig(): Promise<PluginGlobalConfig> {
  try {
    const raw = await api.pluginLoadConfig();
    const parsed = JSON.parse(raw) as Partial<PluginGlobalConfig>;
    return { plugins: {}, ...parsed };
  } catch {
    return { plugins: {} };
  }
}

/**
 * Save plugin global configuration.
 */
export async function savePluginGlobalConfig(config: PluginGlobalConfig): Promise<void> {
  try {
    await api.pluginSaveConfig(JSON.stringify(config, null, 2));
  } catch (err) {
    console.error('[PluginLoader] Failed to save plugin config:', err);
  }
}

/**
 * Initialize the plugin system — called once at app startup.
 * Discovers all plugins, loads configuration, and activates enabled plugins.
 */
export async function initializePluginSystem(): Promise<void> {
  console.log('[PluginLoader] Initializing plugin system...');

  const store = usePluginStore.getState();

  // 1. Discover all installed plugins
  const manifests = await discoverPlugins();
  if (manifests.length === 0) {
    console.log('[PluginLoader] No plugins found');
    return;
  }

  // 2. Register all discovered plugins
  for (const manifest of manifests) {
    store.registerPlugin(manifest);
  }

  // 3. Load configuration to determine which are enabled
  const config = await loadPluginGlobalConfig();

  // 4. Load enabled plugins
  for (const manifest of manifests) {
    const pluginConfig = config.plugins[manifest.id];
    // Default: enabled unless explicitly disabled
    const isEnabled = pluginConfig?.enabled !== false;

    if (isEnabled) {
      await loadPlugin(manifest);
    } else {
      store.setPluginState(manifest.id, 'disabled');
    }
  }

  console.log(`[PluginLoader] Plugin system initialized: ${manifests.length} plugins found, ${store.getActivePlugins().length} active`);
}
