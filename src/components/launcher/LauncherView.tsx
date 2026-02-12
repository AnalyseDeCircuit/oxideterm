/**
 * LauncherView Component
 *
 * Platform-aware application launcher:
 * - macOS: Native Launchpad-style flat icon grid (like macOS 14/15)
 * - Windows: WSL distro list with launch button
 * - Linux: hidden (should never render)
 */

import React, { useEffect, useMemo, useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Search, Terminal, RefreshCw, AlertCircle, Loader2, ExternalLink, AppWindow, Rocket, HardDrive, ShieldCheck, Power } from 'lucide-react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { appDataDir } from '@tauri-apps/api/path';
import { Input } from '../ui/input';
import { Button } from '../ui/button';
import { cn } from '../../lib/utils';
import { platform } from '../../lib/platform';
import { useLauncherStore } from '../../store/launcherStore';
import type { AppEntry, WslDistro } from '../../store/launcherStore';
import { useTabBgActive } from '../../hooks/useTabBackground';

// ── macOS App Icon ──────────────────────────────────────────────────────────

const AppIcon: React.FC<{
  app: AppEntry;
  onLaunch: (path: string) => void;
}> = React.memo(({ app, onLaunch }) => {
  const [iconError, setIconError] = useState(false);

  // Memoize asset URL to avoid re-computing convertFileSrc on every render
  const iconUrl = useMemo(
    () => (app.iconPath ? convertFileSrc(app.iconPath) : null),
    [app.iconPath],
  );

  const handleClick = useCallback(() => onLaunch(app.path), [onLaunch, app.path]);
  const handleImgError = useCallback(() => setIconError(true), []);

  return (
    <button
      className={cn(
        "flex flex-col items-center gap-2 p-2 rounded-xl",
        "hover:bg-white/[0.06] active:scale-[0.92]",
        "transition-[background-color,transform] duration-150 cursor-pointer group",
        "outline-none focus-visible:ring-2 focus-visible:ring-theme-accent/50",
      )}
      style={{ contentVisibility: 'auto', containIntrinsicSize: '92px 100px', contain: 'layout style paint' }}
      onClick={handleClick}
      title={app.name}
    >
      {/* Icon — larger, matching native Launchpad size */}
      <div className="w-16 h-16 rounded-[14px] overflow-hidden flex items-center justify-center shadow-md shadow-black/20">
        {iconUrl && !iconError ? (
          <img
            src={iconUrl}
            alt={app.name}
            width={64}
            height={64}
            className="w-full h-full object-contain"
            onError={handleImgError}
            loading="lazy"
            decoding="async"
            draggable={false}
          />
        ) : (
          <div className="w-full h-full bg-gradient-to-b from-zinc-600 to-zinc-700 flex items-center justify-center">
            <AppWindow className="h-7 w-7 text-zinc-400" />
          </div>
        )}
      </div>
      {/* Name — small, centered, truncated to 2 lines like macOS */}
      <span className="text-[11px] text-theme-text-secondary/90 leading-tight text-center line-clamp-2 max-w-[76px] select-none">
        {app.name}
      </span>
    </button>
  );
});
AppIcon.displayName = 'AppIcon';

// ── WSL Distro Row (Windows) ────────────────────────────────────────────────

const WslDistroRow: React.FC<{
  distro: WslDistro;
  onLaunch: (name: string) => void;
}> = ({ distro, onLaunch }) => (
  <div
    className={cn(
      "flex items-center gap-3 px-4 py-3 rounded-lg",
      "hover:bg-theme-bg-hover/60 transition-colors cursor-pointer",
      "border border-theme-border/30",
    )}
    onClick={() => onLaunch(distro.name)}
  >
    <Terminal className="h-5 w-5 text-theme-accent shrink-0" />
    <div className="flex-1 min-w-0">
      <div className="text-sm font-medium text-theme-text truncate">
        {distro.name}
        {distro.is_default && (
          <span className="ml-2 text-[10px] px-1.5 py-0.5 rounded bg-theme-accent/20 text-theme-accent font-mono">
            DEFAULT
          </span>
        )}
      </div>
    </div>
    <div className={cn(
      "w-2 h-2 rounded-full shrink-0",
      distro.is_running ? "bg-green-500" : "bg-zinc-600",
    )} />
    <ExternalLink className="h-3.5 w-3.5 text-theme-text-muted opacity-0 group-hover:opacity-100 transition-opacity" />
  </div>
);

// ── Main Component ──────────────────────────────────────────────────────────

export const LauncherView: React.FC = () => {
  const { t } = useTranslation();
  const bgActive = useTabBgActive('launcher');
  const {
    enabled,
    apps,
    wslDistros,
    searchQuery,
    loading,
    error,
    enableLauncher,
    disableLauncher,
    loadApps,
    launchApp,
    launchWsl,
    setSearch,
  } = useLauncherStore();

  // Resolve the cache path to show in the consent screen
  const [cachePath, setCachePath] = useState<string | null>(null);
  useEffect(() => {
    if (platform.isMac) {
      appDataDir().then(dir => setCachePath(`${dir}launcher_icons/`)).catch(() => {});
    }
  }, []);

  // Disable confirmation state
  const [showDisableConfirm, setShowDisableConfirm] = useState(false);
  const handleDisable = useCallback(async () => {
    await disableLauncher();
    setShowDisableConfirm(false);
  }, [disableLauncher]);

  // Load on mount (macOS requires opt-in; Windows always auto-loads)
  useEffect(() => {
    if (platform.isMac && !enabled) return;
    if (apps.length === 0 && wslDistros.length === 0) {
      loadApps();
    }
  }, [enabled]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleRefresh = useCallback(() => {
    useLauncherStore.setState({ apps: [], wslDistros: [], iconDir: null, error: null });
    loadApps();
  }, [loadApps]);

  // Filter apps by search query (name or bundleId)
  const filteredApps = useMemo(() => {
    if (!searchQuery.trim()) return apps;
    const q = searchQuery.toLowerCase();
    return apps.filter(
      app =>
        app.name.toLowerCase().includes(q) ||
        (app.bundleId && app.bundleId.toLowerCase().includes(q))
    );
  }, [apps, searchQuery]);

  // Filter WSL distros by search
  const filteredDistros = useMemo(() => {
    if (!searchQuery.trim()) return wslDistros;
    const q = searchQuery.toLowerCase();
    return wslDistros.filter(d => d.name.toLowerCase().includes(q));
  }, [wslDistros, searchQuery]);

  // ── macOS Launchpad View ────────────────────────────────────────────────

  if (platform.isMac) {
    // ── Consent screen: shown before first scan ───────────────────────────
    if (!enabled) {
      return (
        <div className={`flex flex-col h-full ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
          <div className="flex-1 flex items-center justify-center px-8">
            <div className="max-w-sm w-full space-y-6 text-center">
              <div className="mx-auto w-14 h-14 rounded-2xl bg-theme-accent/10 flex items-center justify-center">
                <Rocket className="h-7 w-7 text-theme-accent" />
              </div>
              <div className="space-y-2">
                <h2 className="text-base font-semibold text-theme-text">
                  {t('launcher.consentTitle')}
                </h2>
                <p className="text-sm text-theme-text-secondary leading-relaxed">
                  {t('launcher.consentDescription')}
                </p>
              </div>
              <div className="space-y-2 text-left bg-white/[0.03] rounded-lg p-3 border border-white/[0.06]">
                <div className="flex items-start gap-2.5">
                  <Search className="h-4 w-4 text-theme-text-muted mt-0.5 shrink-0" />
                  <span className="text-xs text-theme-text-muted leading-relaxed">
                    {t('launcher.consentScan')}
                  </span>
                </div>
                <div className="flex items-start gap-2.5">
                  <HardDrive className="h-4 w-4 text-theme-text-muted mt-0.5 shrink-0" />
                  <span className="text-xs text-theme-text-muted leading-relaxed">
                    {t('launcher.consentCache')}
                    {cachePath && (
                      <span className="block mt-1 font-mono text-[10px] text-theme-text-muted/60 break-all">
                        {cachePath}
                      </span>
                    )}
                  </span>
                </div>
                <div className="flex items-start gap-2.5">
                  <ShieldCheck className="h-4 w-4 text-theme-text-muted mt-0.5 shrink-0" />
                  <span className="text-xs text-theme-text-muted leading-relaxed">
                    {t('launcher.consentPrivacy')}
                  </span>
                </div>
              </div>
              <Button
                onClick={enableLauncher}
                className="w-full"
                size="sm"
              >
                {t('launcher.consentEnable')}
              </Button>
            </div>
          </div>
        </div>
      );
    }

    return (
      <div className={`flex flex-col h-full ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
        {/* Search bar — centered at top, like native Launchpad */}
        <div className="flex items-center justify-center px-6 pt-5 pb-3 shrink-0">
          <div className="relative w-full max-w-xs">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-theme-text-muted/60" />
            <Input
              value={searchQuery}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={t('launcher.search')}
              className="pl-9 h-8 text-sm bg-white/[0.06] border-white/[0.08] rounded-lg placeholder:text-theme-text-muted/40 focus:bg-white/[0.08]"
              autoFocus
            />
            {/* App count + refresh + disable inline */}
            <div className="absolute right-1.5 top-1/2 -translate-y-1/2 flex items-center gap-1">
              <span className="text-[10px] font-mono text-theme-text-muted/50 tabular-nums">
                {filteredApps.length !== apps.length ? `${filteredApps.length}/` : ''}{apps.length}
              </span>
              <Button
                size="icon"
                variant="ghost"
                className="h-5 w-5 opacity-50 hover:opacity-100"
                onClick={handleRefresh}
                title={t('launcher.refresh')}
                disabled={loading}
              >
                <RefreshCw className={cn("h-3 w-3", loading && "animate-spin")} />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                className="h-5 w-5 opacity-50 hover:opacity-100 hover:text-red-400"
                onClick={() => setShowDisableConfirm(true)}
                title={t('launcher.disable')}
              >
                <Power className="h-3 w-3" />
              </Button>
            </div>
          </div>
        </div>

        {/* Disable confirmation banner */}
        {showDisableConfirm && (
          <div className="mx-6 mb-3 px-3 py-2.5 rounded-lg bg-red-500/10 border border-red-500/20 flex items-center gap-3">
            <span className="text-xs text-red-400 flex-1">
              {t('launcher.disableConfirm')}
            </span>
            <Button
              size="sm"
              variant="ghost"
              className="h-6 text-xs text-theme-text-muted hover:text-theme-text"
              onClick={() => setShowDisableConfirm(false)}
            >
              {t('launcher.disableCancel')}
            </Button>
            <Button
              size="sm"
              variant="destructive"
              className="h-6 text-xs"
              onClick={handleDisable}
            >
              {t('launcher.disableAction')}
            </Button>
          </div>
        )}

        {/* App grid */}
        <div
          className="flex-1 overflow-y-auto min-h-0 scrollbar-thin scrollbar-thumb-zinc-700/50 scrollbar-track-transparent"
          style={{ willChange: 'scroll-position', WebkitOverflowScrolling: 'touch' } as React.CSSProperties}
        >
          {loading && apps.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full gap-3">
              <Loader2 className="h-8 w-8 text-theme-accent/60 animate-spin" />
              <span className="text-sm text-theme-text-muted/60">
                {t('launcher.scanning')}
              </span>
            </div>
          ) : error ? (
            <div className="flex flex-col items-center justify-center h-full gap-3 px-8">
              <AlertCircle className="h-8 w-8 text-red-400/80" />
              <span className="text-sm text-red-400/80 text-center">{error}</span>
              <Button variant="outline" size="sm" onClick={handleRefresh}>
                {t('launcher.retry')}
              </Button>
            </div>
          ) : filteredApps.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full gap-2">
              <Search className="h-6 w-6 text-theme-text-muted/30" />
              <span className="text-sm text-theme-text-muted/60">
                {searchQuery
                  ? t('launcher.noResults')
                  : t('launcher.empty')}
              </span>
            </div>
          ) : (
            <div className="px-6 pb-6 pt-1">
              <div
                className="grid grid-cols-[repeat(auto-fill,minmax(88px,1fr))] gap-x-2 gap-y-1 justify-items-center"
                style={{ contain: 'layout style' }}
              >
                {filteredApps.map((app) => (
                  <AppIcon key={app.path} app={app} onLaunch={launchApp} />
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  // ── Windows WSL View ──────────────────────────────────────────────────────

  if (platform.isWindows) {
    return (
      <div className="flex flex-col h-full bg-theme-bg">
        {/* Header */}
        <div className="flex items-center gap-3 px-4 py-3 border-b border-theme-border shrink-0">
          <Terminal className="h-4 w-4 text-theme-accent" />
          <h2 className="text-sm font-medium text-theme-text">
            {t('launcher.wslTitle')}
          </h2>
          <div className="flex-1" />
          <span className="text-[10px] font-mono text-theme-text-muted">
            {filteredDistros.length} distros
          </span>
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7"
            onClick={handleRefresh}
            title={t('launcher.refresh')}
            disabled={loading}
          >
            <RefreshCw className={cn("h-3.5 w-3.5", loading && "animate-spin")} />
          </Button>
        </div>

        {/* Search */}
        <div className="px-4 py-2 border-b border-theme-border/50 shrink-0">
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-theme-text-muted" />
            <Input
              value={searchQuery}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={t('launcher.searchWsl')}
              className="pl-8 h-8 text-sm bg-theme-bg-hover/30 border-theme-border/50"
            />
          </div>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto min-h-0 p-4 space-y-2 scrollbar-thin scrollbar-thumb-zinc-700">
          {loading ? (
            <div className="flex flex-col items-center justify-center h-full gap-3">
              <Loader2 className="h-8 w-8 text-theme-accent animate-spin" />
              <span className="text-sm text-theme-text-muted">
                {t('launcher.loadingWsl')}
              </span>
            </div>
          ) : error ? (
            <div className="flex flex-col items-center justify-center h-full gap-3 px-8">
              <AlertCircle className="h-8 w-8 text-red-400" />
              <span className="text-sm text-red-400 text-center">{error}</span>
              <Button variant="outline" size="sm" onClick={handleRefresh}>
                {t('launcher.retry')}
              </Button>
            </div>
          ) : filteredDistros.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full gap-2">
              <Terminal className="h-6 w-6 text-theme-text-muted/40" />
              <span className="text-sm text-theme-text-muted">
                {searchQuery
                  ? t('launcher.noWslResults')
                  : t('launcher.noWsl')}
              </span>
            </div>
          ) : (
            filteredDistros.map((distro) => (
              <WslDistroRow
                key={distro.name}
                distro={distro}
                onLaunch={launchWsl}
              />
            ))
          )}
        </div>
      </div>
    );
  }

  // ── Linux / Unsupported ───────────────────────────────────────────────────

  return null;
};
