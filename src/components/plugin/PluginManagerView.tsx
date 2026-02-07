/**
 * Plugin Manager View
 *
 * UI for managing installed plugins — view status, enable/disable, and inspect info.
 * Styled to match SettingsView panels (rounded-lg border border-theme-border bg-theme-bg-panel/50).
 */

import { useState, useCallback } from 'react';
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
} from 'lucide-react';
import { homeDir, join } from '@tauri-apps/api/path';
import { openPath } from '@tauri-apps/plugin-opener';
import { Separator } from '../ui/separator';
import { usePluginStore } from '../../store/pluginStore';
import {
  loadPlugin,
  unloadPlugin,
  discoverPlugins,
  loadPluginGlobalConfig,
  savePluginGlobalConfig,
} from '../../lib/plugin/pluginLoader';
import type { PluginState, PluginInfo } from '../../types/plugin';

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
            className={`p-1.5 rounded transition-colors ${
              isActive
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

/** Plugin Manager main view — uses SettingsView panel style */
export function PluginManagerView() {
  const { t } = useTranslation();
  const plugins = usePluginStore((s) => s.plugins);
  const [refreshing, setRefreshing] = useState(false);

  const pluginList = Array.from(plugins.values());
  const activeCount = pluginList.filter(p => p.state === 'active').length;

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
    <div className="h-full overflow-auto">
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

          {/* Installed plugins card */}
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
                    <PluginRow
                      info={info}
                      onToggle={handleToggle}
                      onReload={handleReload}
                    />
                    {idx < pluginList.length - 1 && (
                      <div className="border-b border-theme-border/40 mt-4" />
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
