// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

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

import { convertFileSrc } from '@tauri-apps/api/core';
import { api } from '../api';
import { usePluginStore } from '../../store/pluginStore';
import { buildPluginContext, cleanupPluginAssets } from './pluginContextFactory';
import { loadPluginI18n, removePluginI18n } from './pluginI18nManager';
import { normalizePluginRelativePath } from './pluginPaths';
import type { PluginManifest, PluginModule, PluginGlobalConfig } from '../../types/plugin';
import packageJson from '../../../package.json';

/** Current OxideTerm version for engine compatibility checks */
const OXIDETERM_VERSION = packageJson.version ?? '0.0.0';

function parseSemverParts(version: string): [number, number, number] | null {
  const match = version.trim().match(/^(\d+)\.(\d+)\.(\d+)/);
  if (!match) {
    return null;
  }

  const parts = match.slice(1).map(Number);
  if (parts.some((part) => Number.isNaN(part))) {
    return null;
  }

  return [parts[0], parts[1], parts[2]];
}

function compareSemver(a: readonly number[], b: readonly number[]): number {
  for (let index = 0; index < 3; index++) {
    if (a[index] > b[index]) return 1;
    if (a[index] < b[index]) return -1;
  }
  return 0;
}

/** Timeout for activate/deactivate calls (ms) */
const LIFECYCLE_TIMEOUT = 5000;

/** Error circuit breaker: max errors within window before auto-disable */
const MAX_ERRORS = 10;
const ERROR_WINDOW_MS = 60_000;

/** Idempotency guard for initializePluginSystem — prevents double-init under React StrictMode */
let initPromise: Promise<void> | null = null;

/** Per-plugin error tracking */
const errorTrackers = new Map<string, { count: number; windowStart: number }>();

/** Emit a log entry to the plugin store for UI display */
function pluginLog(pluginId: string, level: 'info' | 'warn' | 'error', message: string): void {
  usePluginStore.getState().addPluginLog(pluginId, level, message);
}

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
    const msg = `Auto-disabled: ${MAX_ERRORS} errors in ${ERROR_WINDOW_MS / 1000}s. Reload the plugin after fixing the issue, or disable it in Plugin Manager.`;
    console.error(`[PluginLoader] Plugin "${pluginId}" exceeded error limit (${MAX_ERRORS} in ${ERROR_WINDOW_MS / 1000}s), auto-disabling`);
    pluginLog(pluginId, 'error', msg);
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

  // Check engine compatibility.
  // Supported syntax today: >x.y.z and >=x.y.z.
  // Pre-release suffixes are ignored for ordering (e.g. 1.2.3-beta.1 -> 1.2.3).
  const required = manifest.engines?.oxideterm?.trim();
  if (required) {
    const match = required.match(/^(>=|>)\s*(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/);
    if (!match) {
      console.warn(
        `[PluginLoader] Plugin "${manifest.id}" uses unsupported engines.oxideterm format "${required}". ` +
        'Only >x.y.z and >=x.y.z are currently supported.',
      );
      return null;
    }

    const operator = match[1];
    const requiredVersion = match[2];
    const currentParts = parseSemverParts(OXIDETERM_VERSION);
    const requiredParts = parseSemverParts(requiredVersion);

    if (!currentParts || !requiredParts) {
      console.warn(
        `[PluginLoader] Skipping engines.oxideterm check for plugin "${manifest.id}" because the host or required version is invalid ` +
        `(host="${OXIDETERM_VERSION}", required="${requiredVersion}").`,
      );
      return null;
    }

    const comparison = compareSemver(currentParts, requiredParts);
    if (comparison < 0 || (comparison === 0 && operator === '>')) {
      return `Requires OxideTerm ${operator}${requiredVersion}, current is ${OXIDETERM_VERSION}`;
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
 * Load a plugin module via Blob URL (single-file bundles).
 */
async function loadPluginViaBlobUrl(pluginId: string, main: string): Promise<PluginModule> {
  const mainPath = normalizePluginRelativePath(main);
  const fileBytes = await api.pluginReadFile(pluginId, mainPath);
  const blob = new Blob([new Uint8Array(fileBytes)], { type: 'application/javascript' });
  const blobUrl = URL.createObjectURL(blob);

  try {
    return await import(/* @vite-ignore */ blobUrl);
  } finally {
    URL.revokeObjectURL(blobUrl);
  }
}

function isRelativeModuleSpecifier(specifier: string): boolean {
  return specifier.startsWith('./') || specifier.startsWith('../');
}

function resolvePluginModulePath(fromPath: string, specifier: string): string {
  const baseUrl = new URL(`https://plugin.local/${normalizePluginRelativePath(fromPath)}`);
  return normalizePluginRelativePath(new URL(specifier, baseUrl).pathname);
}

async function loadPluginPackageViaBlobUrls(pluginId: string, main: string): Promise<PluginModule> {
  const blobUrls: string[] = [];
  const moduleUrlCache = new Map<string, Promise<string>>();

  const buildModuleUrl = async (modulePath: string): Promise<string> => {
    const normalizedPath = normalizePluginRelativePath(modulePath);
    const cached = moduleUrlCache.get(normalizedPath);
    if (cached) {
      return cached;
    }

    const pending = (async () => {
      const fileBytes = await api.pluginReadFile(pluginId, normalizedPath);
      const source = new TextDecoder().decode(new Uint8Array(fileBytes));

      // Each file needs its own regex instance — the `g` flag makes the regex
      // stateful (lastIndex), and recursive `await buildModuleUrl()` calls
      // would corrupt the shared state causing duplicate / missing rewrites.
      const importPattern = /((?:import|export)\s+(?:[^'"()]|\([^)]*\))*?\sfrom\s*)(['"])([^'"]+)\2|(\bimport\s*\(\s*)(['"])([^'"]+)\5(\s*\))/g;

      let transformed = '';
      let lastIndex = 0;
      let match: RegExpExecArray | null;

      while ((match = importPattern.exec(source)) !== null) {
        const [fullMatch] = match;
        transformed += source.slice(lastIndex, match.index);

        const staticPrefix = match[1];
        const staticQuote = match[2];
        const staticSpecifier = match[3];
        const dynamicPrefix = match[4];
        const dynamicQuote = match[5];
        const dynamicSpecifier = match[6];
        const dynamicSuffix = match[7];

        if (staticSpecifier !== undefined) {
          if (isRelativeModuleSpecifier(staticSpecifier)) {
            const resolvedPath = resolvePluginModulePath(normalizedPath, staticSpecifier);
            const resolvedUrl = await buildModuleUrl(resolvedPath);
            transformed += `${staticPrefix}${staticQuote}${resolvedUrl}${staticQuote}`;
          } else {
            transformed += fullMatch;
          }
        } else if (dynamicSpecifier !== undefined) {
          if (isRelativeModuleSpecifier(dynamicSpecifier)) {
            const resolvedPath = resolvePluginModulePath(normalizedPath, dynamicSpecifier);
            const resolvedUrl = await buildModuleUrl(resolvedPath);
            transformed += `${dynamicPrefix}${dynamicQuote}${resolvedUrl}${dynamicQuote}${dynamicSuffix}`;
          } else {
            transformed += fullMatch;
          }
        } else {
          transformed += fullMatch;
        }

        lastIndex = match.index + fullMatch.length;
      }

      transformed += source.slice(lastIndex);

      const blobUrl = URL.createObjectURL(new Blob([transformed], { type: 'application/javascript' }));
      blobUrls.push(blobUrl);
      return blobUrl;
    })();

    moduleUrlCache.set(normalizedPath, pending);
    return pending;
  };

  const entryUrl = await buildModuleUrl(main);

  try {
    return await import(/* @vite-ignore */ entryUrl);
  } finally {
    for (const blobUrl of blobUrls) {
      URL.revokeObjectURL(blobUrl);
    }
  }
}

/** Cached plugin server port */
let pluginServerPort: number | null = null;

async function loadPluginViaAssetUrl(pluginId: string, main: string): Promise<PluginModule> {
  const mainPath = normalizePluginRelativePath(main);
  const assetPath = await api.pluginAllowAssetEntry(pluginId, mainPath);
  const assetUrl = convertFileSrc(assetPath);
  return await import(/* @vite-ignore */ assetUrl);
}

function isRecoverableAssetLoadError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return /failed to fetch dynamically imported module|importing a module script failed/i.test(message);
}

/**
 * Load a plugin module via the local HTTP file server (multi-file packages).
 * This allows relative imports between plugin files to resolve correctly.
 */
async function loadPluginViaServer(pluginId: string, main: string): Promise<PluginModule> {
  if (pluginServerPort === null) {
    pluginServerPort = await api.pluginStartServer();
  }
  const mainPath = normalizePluginRelativePath(main);
  const url = `http://127.0.0.1:${pluginServerPort}/plugins/${pluginId}/${mainPath}`;
  return await import(/* @vite-ignore */ url);
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
    pluginLog(id, 'error', `Manifest validation failed: ${validationError}`);
    console.error(`[PluginLoader] Plugin "${id}" validation failed: ${validationError}`);
    return;
  }

  // Defensive cleanup: if the plugin was previously active (e.g. app restarted
  // without a clean deactivate cycle), clean up stale registrations to prevent
  // duplicate status-bar items, commands, etc.
  const existingPlugin = store.getPlugin(id);
  if (existingPlugin?.state === 'active' || existingPlugin?.state === 'loading' || existingPlugin?.module) {
    try {
      if (existingPlugin.module?.deactivate) {
        const result = existingPlugin.module.deactivate();
        if (result instanceof Promise) {
          await withTimeout(result, LIFECYCLE_TIMEOUT, `Plugin "${id}" deactivate() during reload`);
        }
      }
    } catch (err) {
      console.warn(`[PluginLoader] Error during pre-load deactivate() for "${id}":`, err);
    }
    store.cleanupPlugin(id);
    removePluginI18n(id);
    cleanupPluginAssets(id);
  }

  store.setPluginState(id, 'loading');

  // Check shared dependencies (advisory — warn but don't block)
  if (manifest.sharedDependencies) {
    const available = window.__OXIDE__;
    const knownShared = new Set(['react', 'react-dom', 'zustand', 'lucide-react', 'clsx', 'react-i18next']);
    for (const dep of Object.keys(manifest.sharedDependencies)) {
      if (!knownShared.has(dep) || !available) {
        console.warn(
          `[PluginLoader] Plugin "${id}" declares shared dependency "${dep}" which is not provided by the host.`,
        );
      }
    }
  }

  try {
    let module: PluginModule;

    if (manifest.format === 'package') {
      // Package plugins need relative ESM imports to preserve directory semantics.
      // convertFileSrc() percent-encodes the full path, which collapses it into a
      // single URL segment and breaks sibling imports such as ./panel.js.
      try {
        module = await loadPluginPackageViaBlobUrls(id, manifest.main);
      } catch (blobErr) {
        console.warn(`[PluginLoader] Package blob load failed for plugin "${id}", trying asset URL fallback:`, blobErr);

        try {
          module = await loadPluginViaAssetUrl(id, manifest.main);
        } catch (assetErr) {
          if (!isRecoverableAssetLoadError(assetErr)) {
            throw assetErr;
          }
          console.warn(`[PluginLoader] Asset URL load failed for plugin "${id}", falling back to localhost server:`, assetErr);
          module = await loadPluginViaServer(id, manifest.main);
        }
      }
    } else {
      // Single-file bundle (default): load via Blob URL
      module = await loadPluginViaBlobUrl(id, manifest.main);
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

    // 4. Auto-load CSS files declared in manifest.styles
    if (manifest.styles && manifest.styles.length > 0) {
      for (const cssPath of manifest.styles) {
        try {
          const normalizedPath = normalizePluginRelativePath(cssPath);
          const fileBytes = await api.pluginReadFile(id, normalizedPath);
          const cssText = new TextDecoder().decode(new Uint8Array(fileBytes));

          const styleEl = document.createElement('style');
          styleEl.setAttribute('data-plugin', id);
          styleEl.setAttribute('data-path', normalizedPath);
          styleEl.textContent = cssText;
          document.head.appendChild(styleEl);
        } catch (err) {
          console.warn(`[PluginLoader] Failed to load CSS "${cssPath}" for plugin "${id}":`, err);
        }
      }
    }

    // 5. Build the membrane context
    const ctx = buildPluginContext(manifest);

    // 6. Activate with timeout
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
      cleanupPluginAssets(id);
      return;
    }

    store.setPluginState(id, 'active');
    pluginLog(id, 'info', `Activated v${manifest.version}`);
    console.log(`[PluginLoader] Plugin "${id}" v${manifest.version} activated successfully`);
  } catch (err) {
    const errorMsg = err instanceof Error ? err.message : String(err);

    // Produce actionable error messages
    let hint = '';
    if (errorMsg.includes('timed out')) {
      hint = ' (Hint: activate() must resolve within 5s. Use async patterns for long init work.)';
    } else if (errorMsg.includes('activate')) {
      hint = ' (Hint: ensure your main.js exports an activate() function.)';
    } else if (errorMsg.includes('import') || errorMsg.includes('SyntaxError')) {
      hint = ' (Hint: check that main.js is a valid ES module bundle.)';
    }

    const fullError = errorMsg + hint;

    // Clean up any partial state left by a failed activation:
    // - module reference (set before activate() was called)
    // - i18n resources (loaded before activate() was called)
    // - CSS/asset URLs (loaded before activate() was called)
    // - any disposables registered during a partially-completed activate()
    store.cleanupPlugin(id);
    removePluginI18n(id);
    cleanupPluginAssets(id);

    store.setPluginState(id, 'error', fullError);
    pluginLog(id, 'error', `Load failed: ${fullError}`);
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

  // Clean up injected CSS and asset blob URLs
  cleanupPluginAssets(pluginId);

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
  pluginLog(pluginId, 'info', 'Unloaded');
  console.log(`[PluginLoader] Plugin "${pluginId}" unloaded`);
}

/**
 * Load locale files for a plugin from its locales directory.
 */
async function loadPluginLocales(pluginId: string, localesPath: string): Promise<void> {
  const basePath = normalizePluginRelativePath(localesPath);

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
  // Idempotency: if already initializing or initialized, return the same promise.
  // Prevents duplicate plugin activations under React StrictMode double-mount.
  if (initPromise) {
    return initPromise;
  }
  initPromise = doInitializePluginSystem();
  return initPromise;
}

async function doInitializePluginSystem(): Promise<void> {
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
