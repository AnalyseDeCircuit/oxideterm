/**
 * Plugin Manager View
 *
 * UI for managing installed plugins — view status, enable/disable, and inspect info.
 * Also supports browsing and installing plugins from a remote registry.
 * Styled to match SettingsView panels (rounded-lg border border-theme-border bg-theme-bg-panel/50).
 */

import { useState, useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Puzzle,
  Power,
  PowerOff,
  RefreshCw,
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  FolderOpen,
  Download,
  Trash2,
  Search,
  Globe,
  Loader2,
  ArrowUpCircle,
} from 'lucide-react';
import { homeDir, join } from '@tauri-apps/api/path';
import { openPath } from '@tauri-apps/plugin-opener';
import { Separator } from '../ui/separator';
import { usePluginStore } from '../../store/pluginStore';
import { api } from '../../lib/api';
import {
  loadPlugin,
  unloadPlugin,
  discoverPlugins,
  loadPluginGlobalConfig,
  savePluginGlobalConfig,
} from '../../lib/plugin/pluginLoader';
import { clearPluginStorage } from '../../lib/plugin/pluginStorage';
import { useTabBgActive } from '../../hooks/useTabBackground';
import type { PluginState, PluginInfo, RegistryEntry } from '../../types/plugin';

/** Status indicator dot + label */
function StatusBadge({ state }: { state: PluginState }) {
  const { t } = useTranslation();

  const config: Record<PluginState, { color: string; label: string }> = {
    active: { color: 'bg-green-400', label: t('plugin.status.active') },
    inactive: { color: 'bg-zinc-500', label: t('plugin.status.inactive') },
    loading: { color: 'bg-blue-400 animate-pulse', label: t('plugin.status.loading') },
    error: { color: 'bg-red-400', label: t('plugin.status.error') },
    disabled: { color: 'bg-yellow-500', label: t('plugin.status.disabled') },
  };

  const cfg = config[state];

  return (
    <span className="inline-flex items-center gap-1.5 text-xs text-theme-text-muted">
      <span className={`h-2 w-2 rounded-full ${cfg.color}`} />
      {cfg.label}
    </span>
  );
}

/** Single plugin row inside a settings-style card */
function PluginRow({ info, onToggle, onReload }: {
  info: PluginInfo;
  onToggle: (id: string, enable: boolean) => void;
  onReload: (id: string) => void;
}) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const { manifest } = info;

  const isActive = info.state === 'active';
  const isDisabled = info.state === 'disabled';
  const isError = info.state === 'error';

  return (
    <div className="space-y-3">
      {/* Main row */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3 min-w-0">
          <button
            onClick={() => setExpanded(!expanded)}
            className="flex-shrink-0 text-theme-text-muted hover:text-theme-text transition-colors"
          >
            {expanded
              ? <ChevronDown className="h-4 w-4" />
              : <ChevronRight className="h-4 w-4" />
            }
          </button>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium text-theme-text truncate">{manifest.name}</span>
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-theme-accent/20 text-theme-accent font-medium">
                v{manifest.version}
              </span>
              <StatusBadge state={info.state} />
            </div>
            <p className="text-xs text-theme-text-muted mt-0.5 line-clamp-2">
              {manifest.description || manifest.id}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2 flex-shrink-0">
          {(isError || isActive) && (
            <button
              onClick={() => onReload(manifest.id)}
              className="p-1.5 rounded hover:bg-theme-bg-panel text-theme-text-muted hover:text-theme-text transition-colors"
              title={t('plugin.reload')}
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </button>
          )}

          <button
            onClick={() => onToggle(manifest.id, !isActive && !isDisabled ? false : isDisabled)}
            className={`p-1.5 rounded transition-colors ${isActive
              ? 'text-green-400 hover:text-red-400 hover:bg-red-400/10'
              : 'text-theme-text-muted hover:text-green-400 hover:bg-green-400/10'
              }`}
            title={isActive ? t('plugin.disable') : t('plugin.enable')}
          >
            {isActive ? <Power className="h-4 w-4" /> : <PowerOff className="h-4 w-4" />}
          </button>
        </div>
      </div>

      {/* Error message */}
      {isError && info.error && (
        <div className="ml-7 p-2.5 rounded bg-red-500/10 border border-red-500/20">
          <p className="text-xs text-red-400 flex items-start gap-1.5">
            <AlertTriangle className="h-3.5 w-3.5 flex-shrink-0 mt-0.5" />
            <span className="break-all">{info.error}</span>
          </p>
        </div>
      )}

      {/* Expanded details */}
      {expanded && (
        <div className="ml-7 p-3 rounded bg-theme-bg-panel/30 border border-theme-border/50 space-y-2 text-xs text-theme-text-muted">
          {manifest.description && (
            <p className="text-theme-text-muted whitespace-pre-wrap">{manifest.description}</p>
          )}
          <div className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5">
            <span className="font-medium text-theme-text">ID</span>
            <span className="font-mono">{manifest.id}</span>

            <span className="font-medium text-theme-text">{t('plugin.detail_version')}</span>
            <span>{manifest.version}</span>

            <span className="font-medium text-theme-text">{t('plugin.detail_entry')}</span>
            <span className="font-mono">{manifest.main}</span>

            {manifest.author && (
              <>
                <span className="font-medium text-theme-text">{t('plugin.by_author', { author: '' }).replace(/ $/, '')}</span>
                <span>{manifest.author}</span>
              </>
            )}

            {manifest.engines?.oxideterm && (
              <>
                <span className="font-medium text-theme-text">{t('plugin.detail_requires')}</span>
                <span>OxideTerm {manifest.engines.oxideterm}</span>
              </>
            )}
          </div>

          {manifest.contributes && (
            <div className="pt-2 border-t border-theme-border/30">
              <span className="font-medium text-theme-text">{t('plugin.detail_contributes')}</span>
              <div className="flex flex-wrap gap-1.5 mt-1.5">
                {manifest.contributes.tabs && manifest.contributes.tabs.length > 0 && (
                  <span className="px-2 py-0.5 rounded-full bg-theme-accent/10 text-theme-accent text-[10px]">
                    {t('plugin.contrib_tabs', { count: manifest.contributes.tabs.length })}
                  </span>
                )}
                {manifest.contributes.sidebarPanels && manifest.contributes.sidebarPanels.length > 0 && (
                  <span className="px-2 py-0.5 rounded-full bg-theme-accent/10 text-theme-accent text-[10px]">
                    {t('plugin.contrib_sidebar_panels', { count: manifest.contributes.sidebarPanels.length })}
                  </span>
                )}
                {manifest.contributes.settings && manifest.contributes.settings.length > 0 && (
                  <span className="px-2 py-0.5 rounded-full bg-theme-accent/10 text-theme-accent text-[10px]">
                    {t('plugin.contrib_settings', { count: manifest.contributes.settings.length })}
                  </span>
                )}
                {manifest.contributes.terminalHooks?.inputInterceptor && (
                  <span className="px-2 py-0.5 rounded-full bg-theme-accent/10 text-theme-accent text-[10px]">
                    {t('plugin.contrib_input_interceptor')}
                  </span>
                )}
                {manifest.contributes.terminalHooks?.outputProcessor && (
                  <span className="px-2 py-0.5 rounded-full bg-theme-accent/10 text-theme-accent text-[10px]">
                    {t('plugin.contrib_output_processor')}
                  </span>
                )}
                {manifest.contributes.terminalHooks?.shortcuts && manifest.contributes.terminalHooks.shortcuts.length > 0 && (
                  <span className="px-2 py-0.5 rounded-full bg-theme-accent/10 text-theme-accent text-[10px]">
                    {t('plugin.contrib_shortcuts', { count: manifest.contributes.terminalHooks.shortcuts.length })}
                  </span>
                )}
                {manifest.contributes.connectionHooks && manifest.contributes.connectionHooks.length > 0 && (
                  <span className="px-2 py-0.5 rounded-full bg-theme-accent/10 text-theme-accent text-[10px]">
                    {t('plugin.contrib_connection_hooks', { count: manifest.contributes.connectionHooks.length })}
                  </span>
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** Registry plugin card for the Browse tab */
function RegistryPluginCard({ entry, isInstalled, hasUpdate, onInstall, onUpdate }: {
  entry: RegistryEntry;
  isInstalled: boolean;
  hasUpdate: boolean;
  onInstall: (entry: RegistryEntry) => void;
  onUpdate: (entry: RegistryEntry) => void;
}) {
  const { t } = useTranslation();
  const installProgress = usePluginStore((s) => s.installProgress.get(entry.id));
  const isInstalling = installProgress?.state === 'downloading' || installProgress?.state === 'extracting' || installProgress?.state === 'installing';
  const hasError = installProgress?.state === 'error';

  return (
    <div className="p-4 rounded-lg border border-theme-border bg-theme-bg-panel/30 hover:bg-theme-bg-panel/50 transition-colors">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 mb-1">
            <span className="text-sm font-medium text-theme-text truncate">{entry.name}</span>
            <span className="text-[10px] px-1.5 py-0.5 rounded bg-theme-accent/20 text-theme-accent font-medium">
              v{entry.version}
            </span>
            {isInstalled && !hasUpdate && (
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-green-500/20 text-green-400 font-medium">
                {t('plugin.installed')}
              </span>
            )}
            {hasUpdate && (
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-yellow-500/20 text-yellow-400 font-medium flex items-center gap-1">
                <ArrowUpCircle className="h-3 w-3" />
                {t('plugin.update_available')}
              </span>
            )}
          </div>
          <p className="text-xs text-theme-text-muted line-clamp-2 mb-2">
            {entry.description || entry.id}
          </p>
          <div className="flex items-center gap-3 text-[10px] text-theme-text-muted">
            {entry.author && <span>{t('plugin.by_author', { author: entry.author })}</span>}
            {entry.tags && entry.tags.length > 0 && (
              <span className="flex items-center gap-1">
                {entry.tags.slice(0, 3).map((tag) => (
                  <span key={tag} className="px-1.5 py-0.5 rounded bg-theme-bg-panel text-theme-text-muted">
                    {tag}
                  </span>
                ))}
              </span>
            )}
          </div>
        </div>

        <div className="flex-shrink-0">
          {hasError ? (
            <div className="text-xs text-red-400 flex items-center gap-1">
              <AlertTriangle className="h-3.5 w-3.5" />
              {t('plugin.install_error')}
            </div>
          ) : isInstalling ? (
            <button
              disabled
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded bg-theme-accent/20 text-theme-accent"
            >
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t('plugin.installing')}
            </button>
          ) : hasUpdate ? (
            <button
              onClick={() => onUpdate(entry)}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded bg-yellow-500/20 text-yellow-400 hover:bg-yellow-500/30 transition-colors"
            >
              <ArrowUpCircle className="h-3.5 w-3.5" />
              {t('plugin.update')}
            </button>
          ) : isInstalled ? (
            <span className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded bg-green-500/10 text-green-400">
              <CheckCircle2 className="h-3.5 w-3.5" />
              {t('plugin.installed')}
            </span>
          ) : (
            <button
              onClick={() => onInstall(entry)}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded bg-theme-accent text-white hover:bg-theme-accent/80 transition-colors"
            >
              <Download className="h-3.5 w-3.5" />
              {t('plugin.install')}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/** Placeholder shown when no plugin registry URL is configured */
function RegistryNotConfigured() {
  const { t } = useTranslation();
  return (
    <div className="text-center py-16 text-theme-text-muted">
      <Globe className="h-12 w-12 mx-auto mb-4 opacity-15" />
      <p className="text-base font-medium text-theme-text/70 mb-2">
        {t('plugin.registry_coming_soon')}
      </p>
      <p className="text-sm max-w-md mx-auto leading-relaxed">
        {t('plugin.registry_coming_soon_desc')}
      </p>
    </div>
  );
}

/** Plugin Manager main view — uses SettingsView panel style */
export function PluginManagerView() {
  const { t } = useTranslation();
  const bgActive = useTabBgActive('plugin_manager');
  const plugins = usePluginStore((s) => s.plugins);
  const registryEntries = usePluginStore((s) => s.registryEntries);
  const availableUpdates = usePluginStore((s) => s.availableUpdates);
  const [refreshing, setRefreshing] = useState(false);
  const [activeTab, setActiveTab] = useState<'installed' | 'browse'>('installed');
  const [searchQuery, setSearchQuery] = useState('');
  const [fetchingRegistry, setFetchingRegistry] = useState(false);
  const [registryError, setRegistryError] = useState<string | null>(null);
  const [registryConfigured, setRegistryConfigured] = useState<boolean | null>(null);

  const pluginList = Array.from(plugins.values());
  const activeCount = pluginList.filter(p => p.state === 'active').length;
  const installedIds = new Set(pluginList.map(p => p.manifest.id));
  const updateIds = new Set(availableUpdates.map(u => u.id));

  // Filter registry entries by search query
  const filteredRegistry = registryEntries.filter((entry) => {
    if (!searchQuery) return true;
    const q = searchQuery.toLowerCase();
    return (
      entry.name.toLowerCase().includes(q) ||
      entry.id.toLowerCase().includes(q) ||
      entry.description?.toLowerCase().includes(q) ||
      entry.tags?.some(tag => tag.toLowerCase().includes(q))
    );
  });

  // Fetch registry on mount or when switching to browse tab
  useEffect(() => {
    if (activeTab === 'browse' && registryEntries.length === 0 && !fetchingRegistry) {
      loadPluginGlobalConfig().then((config) => {
        const hasUrl = !!config.registryUrl;
        setRegistryConfigured(hasUrl);
        if (hasUrl) {
          handleFetchRegistry();
        }
      });
    }
  }, [activeTab]);

  const handleFetchRegistry = useCallback(async () => {
    setFetchingRegistry(true);
    setRegistryError(null);
    try {
      const config = await loadPluginGlobalConfig();
      const registryUrl = config.registryUrl;
      if (!registryUrl) {
        // No registry URL configured — nothing to fetch
        return;
      }
      const registry = await api.pluginFetchRegistry(registryUrl);
      usePluginStore.getState().setRegistryEntries(registry.plugins);

      // Check for updates
      const installed = pluginList.map(p => ({ id: p.manifest.id, version: p.manifest.version }));
      if (installed.length > 0) {
        const updates = await api.pluginCheckUpdates(registryUrl, installed);
        usePluginStore.getState().setAvailableUpdates(updates);
      }
    } catch (err) {
      setRegistryError(err instanceof Error ? err.message : String(err));
    } finally {
      setFetchingRegistry(false);
    }
  }, [pluginList]);

  const handleInstall = useCallback(async (entry: RegistryEntry) => {
    const store = usePluginStore.getState();
    store.setInstallProgress(entry.id, 'downloading');
    try {
      const manifest = await api.pluginInstall(entry.downloadUrl, entry.id, entry.checksum);
      store.setInstallProgress(entry.id, 'installing');

      // Register and load the plugin
      store.registerPlugin(manifest);
      await loadPlugin(manifest);

      store.setInstallProgress(entry.id, 'done');
      // Clear progress after a short delay
      setTimeout(() => store.clearInstallProgress(entry.id), 2000);
    } catch (err) {
      store.setInstallProgress(entry.id, 'error', err instanceof Error ? err.message : String(err));
    }
  }, []);

  const handleUpdate = useCallback(async (entry: RegistryEntry) => {
    const store = usePluginStore.getState();
    const existingPlugin = store.getPlugin(entry.id);

    // Unload existing plugin first
    if (existingPlugin && (existingPlugin.state === 'active' || existingPlugin.state === 'loading')) {
      await unloadPlugin(entry.id);
    }

    // Install the new version
    await handleInstall(entry);

    // Remove from available updates
    store.setAvailableUpdates(store.availableUpdates.filter(u => u.id !== entry.id));
  }, [handleInstall]);

  const handleUninstall = useCallback(async (pluginId: string) => {
    const store = usePluginStore.getState();
    const info = store.getPlugin(pluginId);

    // Unload if active
    if (info && (info.state === 'active' || info.state === 'loading')) {
      await unloadPlugin(pluginId);
    }

    // Remove from disk
    await api.pluginUninstall(pluginId);

    // Clear plugin localStorage data
    clearPluginStorage(pluginId);

    // Remove from store
    store.removePlugin(pluginId);
  }, []);

  const handleToggle = useCallback(async (pluginId: string, enable: boolean) => {
    const config = await loadPluginGlobalConfig();

    if (enable) {
      config.plugins[pluginId] = { enabled: true };
      await savePluginGlobalConfig(config);
      const info = usePluginStore.getState().getPlugin(pluginId);
      if (info?.manifest) {
        await loadPlugin(info.manifest);
      }
    } else {
      config.plugins[pluginId] = { enabled: false };
      await savePluginGlobalConfig(config);
      await unloadPlugin(pluginId);
      usePluginStore.getState().setPluginState(pluginId, 'disabled');
    }
  }, []);

  const handleReload = useCallback(async (pluginId: string) => {
    const info = usePluginStore.getState().getPlugin(pluginId);
    if (!info?.manifest) return;

    await unloadPlugin(pluginId);
    await loadPlugin(info.manifest);
  }, []);

  const handleRefresh = useCallback(async () => {
    setRefreshing(true);
    try {
      const manifests = await discoverPlugins();
      const store = usePluginStore.getState();
      const discoveredIds = new Set(manifests.map((m) => m.id));

      // Register any newly discovered plugins
      for (const manifest of manifests) {
        if (!store.getPlugin(manifest.id)) {
          store.registerPlugin(manifest);
        }
      }

      // Remove plugins whose folders no longer exist
      for (const [id, info] of store.plugins) {
        if (!discoveredIds.has(id) && id !== '__builtin__') {
          // Unload if active or still loading, then remove from store
          if (info.state === 'active' || info.state === 'loading') {
            await unloadPlugin(id);
          }
          store.removePlugin(id);
        }
      }
    } finally {
      setRefreshing(false);
    }
  }, []);

  const handleOpenPluginsDir = useCallback(async () => {
    try {
      const home = await homeDir();
      const pluginsPath = await join(home, '.oxideterm', 'plugins');
      await openPath(pluginsPath);
    } catch (err) {
      console.error('[PluginManager] Failed to open plugins directory:', err);
    }
  }, []);

  return (
    <div className={`h-full overflow-auto ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
      <div className="max-w-4xl mx-auto p-10">
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
          {/* Page Header — matches SettingsView */}
          <div>
            <h3 className="text-2xl font-medium text-theme-text mb-2">
              {t('plugin.manager_title')}
            </h3>
            <p className="text-theme-text-muted">
              {t('plugin.manager_description')}
            </p>
          </div>
          <Separator />

          {/* Actions card */}
          <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
            <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
              {t('plugin.manager_title')}
            </h4>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3 text-xs text-theme-text-muted">
                <span className="inline-flex items-center gap-1.5">
                  <Puzzle className="h-4 w-4 text-theme-accent" />
                  {t('plugin.footer', { count: pluginList.length })}
                </span>
                <span>·</span>
                <span className="inline-flex items-center gap-1.5">
                  <CheckCircle2 className="h-3.5 w-3.5 text-green-400" />
                  {t('plugin.active_count', { count: activeCount })}
                </span>
              </div>

              <div className="flex items-center gap-2">
                <button
                  onClick={handleOpenPluginsDir}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded border border-theme-border text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-panel transition-colors"
                  title={t('plugin.open_plugins_dir')}
                >
                  <FolderOpen className="h-3.5 w-3.5" />
                  {t('plugin.open_plugins_dir')}
                </button>

                <button
                  onClick={handleRefresh}
                  disabled={refreshing}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded border border-theme-border text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-panel transition-colors disabled:opacity-50"
                >
                  <RefreshCw className={`h-3.5 w-3.5 ${refreshing ? 'animate-spin' : ''}`} />
                  {t('plugin.refresh')}
                </button>
              </div>
            </div>
          </div>

          {/* Tab buttons - Pill Style */}
          <div className="flex items-center gap-2 mb-6">
            <button
              onClick={() => setActiveTab('installed')}
              className={`inline-flex items-center gap-2 px-4 py-2 text-sm font-medium rounded-md transition-all border ${activeTab === 'installed'
                ? 'bg-theme-bg-panel text-theme-text border-theme-border shadow-sm'
                : 'bg-transparent border-transparent text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-panel/50'
                }`}
            >
              <Puzzle className={`h-4 w-4 ${activeTab === 'installed' ? 'text-theme-accent' : ''}`} />
              {t('plugin.tab_installed')}
              {pluginList.length > 0 && (
                <span className={`text-xs px-1.5 py-0.5 rounded ml-1 ${activeTab === 'installed' ? 'bg-theme-accent/10 text-theme-accent' : 'bg-theme-bg-panel border border-theme-border/50'
                  }`}>
                  {pluginList.length}
                </span>
              )}
            </button>
            <button
              onClick={() => setActiveTab('browse')}
              className={`inline-flex items-center gap-2 px-4 py-2 text-sm font-medium rounded-md transition-all border ${activeTab === 'browse'
                ? 'bg-theme-bg-panel text-theme-text border-theme-border shadow-sm'
                : 'bg-transparent border-transparent text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-panel/50'
                }`}
            >
              <Globe className={`h-4 w-4 ${activeTab === 'browse' ? 'text-theme-accent' : ''}`} />
              {t('plugin.tab_browse')}
              {availableUpdates.length > 0 && (
                <span className={`text-xs px-1.5 py-0.5 rounded ml-1 ${activeTab === 'browse' ? 'bg-yellow-500/10 text-yellow-500' : 'bg-theme-bg-panel border border-theme-border/50'
                  }`}>
                  {availableUpdates.length} {t('plugin.updates')}
                </span>
              )}
            </button>
          </div>

          {/* Installed tab content */}
          {activeTab === 'installed' && (
            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
              <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                {t('plugin.empty_title')}
              </h4>

              {pluginList.length === 0 ? (
                <div className="text-center py-10 text-theme-text-muted">
                  <Puzzle className="h-10 w-10 mx-auto mb-3 opacity-20" />
                  <p className="text-sm">{t('plugin.empty_description')}</p>
                </div>
              ) : (
                <div className="space-y-4">
                  {pluginList.map((info, idx) => (
                    <div key={info.manifest.id}>
                      <div className="flex items-start justify-between">
                        <div className="flex-1">
                          <PluginRow
                            info={info}
                            onToggle={handleToggle}
                            onReload={handleReload}
                          />
                        </div>
                        <button
                          onClick={() => handleUninstall(info.manifest.id)}
                          className="ml-2 p-1.5 rounded text-theme-text-muted hover:text-red-400 hover:bg-red-400/10 transition-colors"
                          title={t('plugin.uninstall')}
                        >
                          <Trash2 className="h-4 w-4" />
                        </button>
                      </div>
                      {idx < pluginList.length - 1 && (
                        <div className="border-b border-theme-border/40 mt-4" />
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Browse tab content */}
          {activeTab === 'browse' && (
            <div className="space-y-4">
              {/* Registry URL not configured — show coming soon */}
              {registryConfigured === false && (
                <RegistryNotConfigured />
              )}

              {/* Registry is configured — show normal browse UI */}
              {registryConfigured === true && (
                <>
                  {/* Search and refresh */}
                  <div className="flex items-center gap-3">
                    <div className="relative flex-1">
                      <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-theme-text-muted" />
                      <input
                        type="text"
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        placeholder={t('plugin.search_placeholder')}
                        className="w-full pl-10 pr-4 py-2 text-sm rounded-lg border border-theme-border bg-theme-bg-panel/50 text-theme-text placeholder:text-theme-text-muted focus:outline-none focus:ring-2 focus:ring-theme-accent/50"
                      />
                    </div>
                    <button
                      onClick={handleFetchRegistry}
                      disabled={fetchingRegistry}
                      className="inline-flex items-center gap-1.5 px-3 py-2 text-sm rounded-lg border border-theme-border text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-panel transition-colors disabled:opacity-50"
                    >
                      <RefreshCw className={`h-4 w-4 ${fetchingRegistry ? 'animate-spin' : ''}`} />
                      {t('plugin.refresh')}
                    </button>
                  </div>

                  {/* Registry error */}
                  {registryError && (
                    <div className="p-4 rounded-lg bg-red-500/10 border border-red-500/20">
                      <p className="text-sm text-red-400 flex items-center gap-2">
                        <AlertTriangle className="h-4 w-4" />
                        {t('plugin.registry_error')}: {registryError}
                      </p>
                    </div>
                  )}

                  {/* Loading state */}
                  {fetchingRegistry && registryEntries.length === 0 && (
                    <div className="text-center py-16 text-theme-text-muted">
                      <Loader2 className="h-8 w-8 mx-auto mb-3 animate-spin opacity-50" />
                      <p className="text-sm">{t('plugin.loading_registry')}</p>
                    </div>
                  )}

                  {/* Empty state */}
                  {!fetchingRegistry && registryEntries.length === 0 && !registryError && (
                    <div className="text-center py-16 text-theme-text-muted">
                      <Globe className="h-10 w-10 mx-auto mb-3 opacity-20" />
                      <p className="text-sm">{t('plugin.registry_empty')}</p>
                    </div>
                  )}

                  {/* No search results */}
                  {!fetchingRegistry && registryEntries.length > 0 && filteredRegistry.length === 0 && (
                    <div className="text-center py-16 text-theme-text-muted">
                      <Search className="h-10 w-10 mx-auto mb-3 opacity-20" />
                      <p className="text-sm">{t('plugin.no_search_results')}</p>
                    </div>
                  )}

                  {/* Registry plugin cards */}
                  {filteredRegistry.length > 0 && (
                    <div className="grid gap-3">
                      {filteredRegistry.map((entry) => (
                        <RegistryPluginCard
                          key={entry.id}
                          entry={entry}
                          isInstalled={installedIds.has(entry.id)}
                          hasUpdate={updateIds.has(entry.id)}
                          onInstall={handleInstall}
                          onUpdate={handleUpdate}
                        />
                      ))}
                    </div>
                  )}
                </>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
