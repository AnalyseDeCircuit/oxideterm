// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { getVersion } from '@tauri-apps/api/app';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import { ArrowDownToLine, ArrowRight, BookOpen, CheckCircle2, ExternalLink, FolderOpen, Github, HelpCircle, Keyboard, Loader2, RefreshCw, RotateCw, Shield, SkipForward, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import { getFontFamilyCSS } from '@/components/fileManager/fontUtils';
import { useUpdateStore } from '@/store/updateStore';
import { useSettingsStore, type UpdateChannel } from '@/store/settingsStore';
import { api } from '@/lib/api';
import { platform } from '@/lib/platform';
import { getShortcutCategories } from '@/lib/shortcuts';
import { APP_AUTHOR, APP_GITHUB } from '@/lib/identity';

type HelpAboutSectionProps = {
    isPortableMode?: boolean | null;
};

const formatBytes = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
};

const formatSpeed = (bytesPerSec: number): string => {
    if (bytesPerSec <= 0) return '0 B/s';
    return `${formatBytes(bytesPerSec)}/s`;
};

const formatEta = (seconds: number): string => {
    if (seconds > 86400) return '...';
    if (seconds < 60) return `~${Math.round(seconds)}s`;
    const minutes = Math.floor(seconds / 60);
    const remainderSeconds = Math.round(seconds % 60);
    return remainderSeconds > 0 ? `~${minutes}m ${remainderSeconds}s` : `~${minutes}m`;
};

export const HelpAboutSection = ({ isPortableMode = null }: HelpAboutSectionProps) => {
    const { t } = useTranslation();
    const [appVersion, setAppVersion] = useState<string>('...');
    const updater = useUpdateStore();
    const updateChannel = useSettingsStore((state) => state.settings.general.updateChannel);
    const updateGeneral = useSettingsStore((state) => state.updateGeneral);

    useEffect(() => {
        getVersion().then(setAppVersion).catch(() => setAppVersion('1.4.0'));
    }, []);

    const isMac = platform.isMac;
    const shortcutCategories = getShortcutCategories(t);
    const { fontFamily, customFontFamily } = useSettingsStore((state) => state.settings.terminal);
    const terminalFontCSS = fontFamily === 'custom' && customFontFamily
        ? customFontFamily
        : getFontFamilyCSS(fontFamily);

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
            <div>
                <h3 className="text-2xl font-medium text-theme-text-heading mb-2">{t('settings_view.help.title')}</h3>
                <p className="text-theme-text-muted">{t('settings_view.help.description')}</p>
            </div>
            <Separator />

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                    {t('settings_view.help.version_info')}
                </h4>
                <div className="space-y-3">
                    <div className="flex items-center justify-between">
                        <span className="text-theme-text-muted">{t('settings_view.help.app_name')}</span>
                        <span className="text-theme-text font-medium">OxideTerm</span>
                    </div>
                    <div className="flex items-center justify-between">
                        <span className="text-theme-text-muted">{t('settings_view.help.version')}</span>
                        <span className="text-theme-text font-mono">{appVersion}</span>
                    </div>
                    <div className="flex items-center justify-between">
                        {isPortableMode === true ? (
                            <>
                                <div>
                                    <span className="text-theme-text-muted">{t('settings_view.help.portable_mode')}</span>
                                    <p className="text-xs text-theme-text-muted/60 mt-0.5">{t('settings_view.help.portable_mode_hint')}</p>
                                </div>
                                <span className="rounded-full border border-theme-border bg-theme-bg-elevated px-3 py-1 text-xs font-medium text-theme-text">
                                    {t('settings_view.help.updates_manual_only')}
                                </span>
                            </>
                        ) : isPortableMode === false ? (
                            <>
                                <div>
                                    <span className="text-theme-text-muted">{t('settings_view.help.update_channel')}</span>
                                    <p className="text-xs text-theme-text-muted/60 mt-0.5">{t('settings_view.help.update_channel_hint')}</p>
                                </div>
                                <Select
                                    value={updateChannel}
                                    onValueChange={(value) => updateGeneral('updateChannel', value as UpdateChannel)}
                                >
                                    <SelectTrigger className="w-[140px]">
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        <SelectItem value="stable">{t('settings_view.help.channel_stable')}</SelectItem>
                                        <SelectItem value="beta">{t('settings_view.help.channel_beta')}</SelectItem>
                                    </SelectContent>
                                </Select>
                            </>
                        ) : (
                            <div className="flex w-full justify-end">
                                <Loader2 className="h-4 w-4 animate-spin text-theme-text-muted" aria-hidden="true" />
                            </div>
                        )}
                    </div>
                </div>

                <div className="mt-4 pt-4 border-t border-theme-border/50 space-y-3">
                    {isPortableMode === true ? (
                        <div className="rounded-md border border-theme-border/60 bg-theme-bg-elevated/70 p-4">
                            <div className="flex items-center gap-2 text-sm font-medium text-theme-text">
                                <Shield className="h-4 w-4 text-amber-400" />
                                {t('settings_view.help.updates_manual_only')}
                            </div>
                            <p className="mt-2 text-sm leading-6 text-theme-text-muted">
                                {t('settings_view.help.updates_manual_only_hint')}
                            </p>
                        </div>
                    ) : isPortableMode === false ? (
                        <>
                            <div className="flex items-center gap-3">
                                <Button
                                    variant="outline"
                                    size="sm"
                                    onClick={() => updater.checkForUpdate()}
                                    disabled={updater.stage === 'checking' || updater.stage === 'downloading' || updater.stage === 'verifying' || updater.stage === 'installing' || updater.stage === 'ready'}
                                    className="gap-2 shrink-0"
                                >
                                    {updater.stage === 'checking'
                                        ? <Loader2 className="h-3.5 w-3.5 animate-spin" />
                                        : <RefreshCw className="h-3.5 w-3.5" />}
                                    {t('settings_view.help.check_update')}
                                </Button>

                                {updater.stage === 'checking' && (
                                    <span className="text-sm text-theme-text-muted">{t('settings_view.help.checking')}</span>
                                )}
                                {updater.stage === 'up-to-date' && (
                                    <span className="flex items-center gap-1.5 text-sm text-emerald-400">
                                        <CheckCircle2 className="h-3.5 w-3.5 shrink-0" />
                                        {t('settings_view.help.up_to_date')}
                                    </span>
                                )}
                                {(updater.stage === 'verifying' || updater.stage === 'installing') && (
                                    <span className="text-sm text-theme-text-muted">
                                        {updater.stage === 'verifying' ? t('settings_view.help.verifying') : t('settings_view.help.installing')}
                                        {updater.attempt > 1 && ` (${t('settings_view.help.retry')} #${updater.attempt})`}
                                    </span>
                                )}
                                {updater.stage === 'ready' && (
                                    <span className="text-sm text-emerald-400">{t('settings_view.help.ready_to_restart')}</span>
                                )}
                                {updater.stage === 'error' && (
                                    <span className="text-sm text-red-400 truncate">{updater.errorMessage || t('settings_view.help.update_error')}</span>
                                )}

                                {updater.stage === 'ready' && (
                                    <Button variant="default" size="sm" onClick={updater.restartApp} className="gap-2 shrink-0 ml-auto">
                                        <RotateCw className="h-3.5 w-3.5" />
                                        {t('settings_view.help.restart_now')}
                                    </Button>
                                )}
                            </div>

                            {updater.stage === 'available' && (
                        <div className="space-y-3">
                            <div className="flex items-center gap-2 text-sm">
                                <span className="text-theme-text">{t('settings_view.help.update_available')}</span>
                                <span className="font-mono text-theme-text-muted">v{updater.currentVersion ?? appVersion}</span>
                                <ArrowRight className="h-3.5 w-3.5 text-theme-accent shrink-0" />
                                <span className="font-mono text-theme-accent font-medium">v{updater.newVersion}</span>
                            </div>

                            {updater.releaseBody ? (
                                <div className="rounded-md border border-theme-border/50 bg-theme-bg/50 p-3 max-h-48 overflow-y-auto">
                                    <h5 className="text-xs font-medium text-theme-text-muted uppercase tracking-wider mb-2">
                                        {t('settings_view.help.release_notes')}
                                    </h5>
                                    <div
                                        className="prose prose-sm prose-invert max-w-none text-sm text-theme-text leading-relaxed [&_h1]:text-base [&_h1]:font-semibold [&_h1]:mt-3 [&_h1]:mb-1 [&_h2]:text-sm [&_h2]:font-semibold [&_h2]:mt-3 [&_h2]:mb-1 [&_h3]:text-sm [&_h3]:font-medium [&_h3]:mt-2 [&_h3]:mb-1 [&_ul]:my-1 [&_ul]:pl-5 [&_ol]:my-1 [&_ol]:pl-5 [&_li]:my-0.5 [&_p]:my-1 [&_code]:text-xs [&_code]:bg-theme-bg-hover [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_pre]:bg-theme-bg-hover [&_pre]:p-2 [&_pre]:rounded [&_pre]:my-2 [&_pre]:overflow-x-auto [&_a]:text-theme-accent [&_a]:underline"
                                        dangerouslySetInnerHTML={{ __html: DOMPurify.sanitize(String(marked.parse(updater.releaseBody, { async: false }))) }}
                                    />
                                </div>
                            ) : (
                                <p className="text-xs text-theme-text-muted italic">{t('settings_view.help.no_changelog')}</p>
                            )}

                            <div className="flex items-center gap-2">
                                <Button
                                    variant="ghost"
                                    size="sm"
                                    onClick={() => updater.newVersion && updater.skipVersion(updater.newVersion)}
                                    className="gap-2 text-theme-text-muted hover:text-theme-text"
                                >
                                    <SkipForward className="h-3.5 w-3.5" />
                                    {t('settings_view.help.skip_version')}
                                </Button>
                                <Button variant="default" size="sm" onClick={updater.startDownload} className="gap-2 shrink-0 ml-auto">
                                    <ArrowDownToLine className="h-3.5 w-3.5" />
                                    {t('settings_view.help.download_install')}
                                </Button>
                            </div>
                        </div>
                            )}

                            {updater.stage === 'downloading' && (
                        <div className="space-y-2">
                            <div className="flex items-center justify-between">
                                <span className="text-sm text-theme-text-muted">
                                    {t('settings_view.help.downloading')}
                                    {updater.attempt > 1 && ` (${t('settings_view.help.retry')} #${updater.attempt})`}
                                </span>
                                <Button
                                    variant="ghost"
                                    size="sm"
                                    onClick={updater.cancelDownload}
                                    className="gap-1.5 h-7 text-xs text-theme-text-muted hover:text-theme-text"
                                >
                                    <X className="h-3 w-3" />
                                    {t('settings_view.help.cancel')}
                                </Button>
                            </div>

                            <div role="progressbar" aria-valuenow={updater.totalBytes ? Math.round((updater.downloadedBytes / updater.totalBytes) * 100) : 0} aria-valuemin={0} aria-valuemax={100} className="h-1.5 bg-theme-bg rounded-full overflow-hidden">
                                <div className="h-full bg-theme-accent rounded-full transition-[width] duration-300" style={{ width: `${updater.totalBytes ? Math.min(100, (updater.downloadedBytes / updater.totalBytes) * 100) : 0}%` }} />
                            </div>

                            <div className="flex items-center justify-between text-xs text-theme-text-muted">
                                <span>
                                    {updater.totalBytes
                                        ? `${formatBytes(updater.downloadedBytes)} / ${formatBytes(updater.totalBytes)}`
                                        : formatBytes(updater.downloadedBytes)}
                                </span>
                                <span className="tabular-nums">
                                    {updater.downloadSpeed > 0 && formatSpeed(updater.downloadSpeed)}
                                    {updater.downloadSpeed > 0 && updater.etaSeconds != null && updater.etaSeconds > 0 && (
                                        <> · {formatEta(updater.etaSeconds)}</>
                                    )}
                                    {updater.totalBytes && updater.totalBytes > 0 && (
                                        <> · {Math.round((updater.downloadedBytes / updater.totalBytes) * 100)}%</>
                                    )}
                                </span>
                            </div>
                        </div>
                            )}

                            {updater.skippedVersion && updater.stage === 'idle' && (
                        <div className="flex items-center gap-2 text-xs text-theme-text-muted">
                            <SkipForward className="h-3 w-3 shrink-0" />
                            <span>{t('settings_view.help.skipped_version', { version: updater.skippedVersion })}</span>
                            <button type="button" onClick={() => updater.clearSkippedVersion()} className="text-theme-accent hover:underline cursor-pointer">
                                {t('settings_view.help.clear_skip')}
                            </button>
                        </div>
                            )}
                        </>
                    ) : null}
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                    {t('settings_view.help.diagnostics')}
                </h4>
                <div className="space-y-3">
                    <div className="flex items-center justify-between">
                        <div>
                            <span className="text-theme-text">{t('settings_view.help.open_logs')}</span>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.help.open_logs_hint')}</p>
                        </div>
                        <Button variant="outline" size="sm" className="gap-2 shrink-0" onClick={() => api.openLogDirectory().catch(() => {})}>
                            <FolderOpen className="h-3.5 w-3.5" />
                            {t('settings_view.help.open')}
                        </Button>
                    </div>
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                    {t('settings_view.help.tech_stack')}
                </h4>
                <div className="flex flex-wrap gap-2">
                    <span className="px-3 py-1 rounded-full bg-orange-500/20 text-orange-400 text-xs font-medium">Rust</span>
                    <span className="px-3 py-1 rounded-full bg-cyan-500/20 text-cyan-400 text-xs font-medium">Tauri 2.0</span>
                    <span className="px-3 py-1 rounded-full bg-blue-500/20 text-blue-400 text-xs font-medium">React</span>
                    <span className="px-3 py-1 rounded-full bg-yellow-500/20 text-yellow-400 text-xs font-medium">TypeScript</span>
                    <span className="px-3 py-1 rounded-full bg-purple-500/20 text-purple-400 text-xs font-medium">xterm.js</span>
                    <span className="px-3 py-1 rounded-full bg-emerald-500/20 text-emerald-400 text-xs font-medium">redb</span>
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider flex items-center gap-2">
                    <Keyboard className="h-4 w-4" />
                    {t('settings_view.help.shortcuts')}
                </h4>
                <div className="space-y-5 text-sm">
                    {shortcutCategories.map((category, categoryIndex) => (
                        <div key={categoryIndex}>
                            <h5 className="text-xs font-medium text-theme-text-muted uppercase tracking-wider mb-2">
                                {category.title}
                            </h5>
                            <div className="space-y-1">
                                {category.shortcuts.map((shortcut, index) => (
                                    <div key={index} className={`flex items-center justify-between py-1.5 ${index < category.shortcuts.length - 1 ? 'border-b border-theme-border/30' : ''}`}>
                                        <span className="text-theme-text-muted">{shortcut.label}</span>
                                        <kbd className="px-2 py-0.5 rounded bg-theme-bg text-theme-text text-xs" style={{ fontFamily: terminalFontCSS }}>
                                            {isMac ? shortcut.mac : shortcut.other}
                                        </kbd>
                                    </div>
                                ))}
                            </div>
                        </div>
                    ))}
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                    {t('settings_view.help.resources')}
                </h4>
                <div className="space-y-2">
                    <a href="https://oxideterm.app" target="_blank" rel="noopener noreferrer" className="flex items-center justify-between p-3 rounded-lg hover:bg-theme-bg-hover transition-colors group">
                        <div className="flex items-center gap-3">
                            <ExternalLink className="h-5 w-5 text-theme-text-muted" />
                            <span className="text-theme-text">{t('settings_view.help.website')}</span>
                        </div>
                        <ExternalLink className="h-4 w-4 text-theme-text-muted opacity-0 group-hover:opacity-100 transition-opacity" />
                    </a>
                    <a href="https://oxideterm.app/docs" target="_blank" rel="noopener noreferrer" className="flex items-center justify-between p-3 rounded-lg hover:bg-theme-bg-hover transition-colors group">
                        <div className="flex items-center gap-3">
                            <BookOpen className="h-5 w-5 text-theme-text-muted" />
                            <span className="text-theme-text">{t('settings_view.help.documentation')}</span>
                        </div>
                        <ExternalLink className="h-4 w-4 text-theme-text-muted opacity-0 group-hover:opacity-100 transition-opacity" />
                    </a>
                    <a href={APP_GITHUB} target="_blank" rel="noopener noreferrer" className="flex items-center justify-between p-3 rounded-lg hover:bg-theme-bg-hover transition-colors group">
                        <div className="flex items-center gap-3">
                            <Github className="h-5 w-5 text-theme-text-muted" />
                            <span className="text-theme-text">{t('settings_view.help.github')}</span>
                        </div>
                        <ExternalLink className="h-4 w-4 text-theme-text-muted opacity-0 group-hover:opacity-100 transition-opacity" />
                    </a>
                    <a href={`${APP_GITHUB}/issues`} target="_blank" rel="noopener noreferrer" className="flex items-center justify-between p-3 rounded-lg hover:bg-theme-bg-hover transition-colors group">
                        <div className="flex items-center gap-3">
                            <HelpCircle className="h-5 w-5 text-theme-text-muted" />
                            <span className="text-theme-text">{t('settings_view.help.issues')}</span>
                        </div>
                        <ExternalLink className="h-4 w-4 text-theme-text-muted opacity-0 group-hover:opacity-100 transition-opacity" />
                    </a>
                    <a href={`${APP_GITHUB}/blob/main/DISCLAIMER.md`} target="_blank" rel="noopener noreferrer" className="flex items-center justify-between p-3 rounded-lg hover:bg-theme-bg-hover transition-colors group">
                        <div className="flex items-center gap-3">
                            <Shield className="h-5 w-5 text-theme-text-muted" />
                            <span className="text-theme-text">{t('settings_view.help.disclaimer')}</span>
                        </div>
                        <ExternalLink className="h-4 w-4 text-theme-text-muted opacity-0 group-hover:opacity-100 transition-opacity" />
                    </a>
                </div>
            </div>

            <div className="text-center text-xs text-theme-text-muted space-y-1">
                <p>{t('settings_view.help.copyright', { year: new Date().getFullYear(), author: APP_AUTHOR })}</p>
                <p>{t('settings_view.help.license')}</p>
            </div>
        </div>
    );
};
