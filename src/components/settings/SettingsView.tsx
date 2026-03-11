import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { getVersion } from '@tauri-apps/api/app';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { open as openFileDialog } from '@tauri-apps/plugin-dialog';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore, type RendererType, type AdaptiveRendererMode, type FontFamily, type CursorStyle, type Language, type BackgroundFit, type UiDensity, type AnimationSpeed, type FrostedGlassMode } from '../../store/settingsStore';
import { useTabBgActive } from '../../hooks/useTabBackground';
import { useUpdateStore } from '../../store/updateStore';
import { Button } from '../ui/button';
import { Label } from '../ui/label';
import { Input } from '../ui/input';
import { Checkbox } from '../ui/checkbox';
import { Separator } from '../ui/separator';
import {
    Dialog,
    DialogContent,
    DialogTitle,
    DialogDescription,
    DialogHeader,
    DialogFooter
} from '../ui/dialog';
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
    SelectGroup,
    SelectLabel,
    SelectSeparator
} from '../ui/select';
import { Monitor, Key, Terminal as TerminalIcon, Shield, Plus, Trash2, FolderInput, Sparkles, Square, HardDrive, HelpCircle, Github, ExternalLink, Keyboard, RefreshCw, ImageIcon, X, Code2, WifiOff, Download, Upload, Network, ArrowLeftRight, Settings, Folder, ListTree, Rocket, Puzzle, Activity, Loader2, CheckCircle2, ArrowDownToLine, RotateCw, Wrench, FileText, Pen, FolderOpen, Search, GitBranch, Radio, CirclePlus, CircleStop, FolderSearch, FileCode, Info } from 'lucide-react';
import { api } from '../../lib/api';
import { TOOL_GROUPS, WRITE_TOOLS } from '../../lib/ai/tools';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { SshKeyInfo, SshHostInfo } from '../../types';
import { themes, getTerminalTheme, getCustomThemes, isCustomTheme, exportTheme, importTheme } from '../../lib/themes';
import { platform } from '../../lib/platform';
import { cn } from '../../lib/utils';
import { getShortcutCategories } from '../../lib/shortcuts';
import { ThemeEditorModal } from './ThemeEditorModal';
import { useToast } from '../../hooks/useToast';
import { useConfirm } from '../../hooks/useConfirm';

const formatThemeName = (key: string) => {
    if (isCustomTheme(key)) {
        const custom = getCustomThemes()[key];
        return custom ? custom.name : key.replace('custom:', '');
    }
    return key.split('-')
        .map(word => word.charAt(0).toUpperCase() + word.slice(1))
        .join(' ');
};

const ThemePreview = ({ themeName }: { themeName: string }) => {
    const theme = getTerminalTheme(themeName);

    return (
        <div className="mt-2 p-3 rounded-md border border-theme-border" style={{ backgroundColor: theme.background }}>
            <div className="flex gap-2 mb-2">
                <div className="w-3 h-3 rounded-full" style={{ backgroundColor: theme.red }}></div>
                <div className="w-3 h-3 rounded-full" style={{ backgroundColor: theme.yellow }}></div>
                <div className="w-3 h-3 rounded-full" style={{ backgroundColor: theme.green }}></div>
            </div>
            <div className="font-mono text-xs space-y-1" style={{ color: theme.foreground }}>
                <div>$ echo "Hello World"</div>
                <div style={{ color: theme.blue }}>~ <span style={{ color: theme.magenta }}>git</span> status</div>
                <div className="flex items-center">
                    <span>&gt; </span>
                    <span className="w-2 h-4 ml-1 animate-pulse" style={{ backgroundColor: theme.cursor }}></span>
                </div>
            </div>
        </div>
    );
};

// Provider API Key Input Component
const ProviderKeyInput = ({ providerId }: { providerId: string }) => {
    const { t } = useTranslation();
    const { error: toastError } = useToast();
    const { confirm, ConfirmDialog } = useConfirm();
    const refreshProviderModels = useSettingsStore((s) => s.refreshProviderModels);
    const [hasKey, setHasKey] = useState(false);
    const [keyInput, setKeyInput] = useState('');
    const [saving, setSaving] = useState(false);

    useEffect(() => {
        api.hasAiProviderApiKey(providerId)
            .then(setHasKey)
            .catch(() => setHasKey(false));
    }, [providerId]);

    return (
        <div className="grid gap-1">
            <Label className="text-xs text-theme-text-muted">{t('settings_view.ai.api_key')}</Label>
            <div className="flex gap-2">
                {hasKey ? (
                    <div className="flex-1 flex items-center gap-2">
                        <div className="flex-1 h-8 px-2 flex items-center bg-theme-bg-panel/50 border border-theme-border/50 rounded text-theme-text-muted text-xs italic">
                            ••••••••••••••••
                        </div>
                        <Button
                            variant="ghost"
                            size="sm"
                            className="text-red-400 hover:text-red-300 hover:bg-red-400/10 h-8 text-xs"
                            onClick={async () => {
                                if (await confirm({ title: t('settings_view.ai.remove_confirm'), variant: 'danger' })) {
                                    try {
                                        await api.deleteAiProviderApiKey(providerId);
                                        setHasKey(false);
                                        window.dispatchEvent(new CustomEvent('ai-api-key-updated'));
                                    } catch (e) {
                                        toastError(t('settings_view.ai.remove_failed', { error: e }));
                                    }
                                }
                            }}
                        >
                            {t('settings_view.ai.remove')}
                        </Button>
                    </div>
                ) : (
                    <>
                        <Input
                            type="password"
                            placeholder="sk-..."
                            className="flex-1 bg-theme-bg h-8 text-xs"
                            value={keyInput}
                            onChange={(e) => setKeyInput(e.target.value)}
                        />
                        <Button
                            variant="secondary"
                            size="sm"
                            className="h-8 text-xs"
                            disabled={!keyInput.trim() || saving}
                            onClick={async () => {
                                if (!keyInput.trim()) return;
                                setSaving(true);
                                try {
                                    await api.setAiProviderApiKey(providerId, keyInput);
                                    setKeyInput('');
                                    setHasKey(true);
                                    window.dispatchEvent(new CustomEvent('ai-api-key-updated'));
                                    // Auto-fetch models with the new key
                                    refreshProviderModels(providerId).catch((e) =>
                                        console.warn('[ProviderKeyInput] Auto-fetch models failed:', e)
                                    );
                                } catch (e) {
                                    toastError(t('settings_view.ai.save_failed', { error: e }));
                                } finally {
                                    setSaving(false);
                                }
                            }}
                        >
                            {saving ? t('settings_view.ai.saving') : t('settings_view.ai.save')}
                        </Button>
                    </>
                )}
            </div>
            {ConfirmDialog}
        </div>
    );
};

// Local Terminal Settings Component
const LocalTerminalSettings = () => {
    const { t } = useTranslation();
    const { shells, loadShells, shellsLoaded } = useLocalTerminalStore();
    const { settings, updateLocalTerminal } = useSettingsStore();
    const localSettings = settings.localTerminal;

    useEffect(() => {
        if (!shellsLoaded) {
            loadShells();
        }
    }, [shellsLoaded, loadShells]);

    const defaultShellId = localSettings?.defaultShellId;
    const defaultShell = shells.find(s => s.id === defaultShellId) || shells[0];

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
            <div>
                <h3 className="text-2xl font-medium text-theme-text mb-2">{t('settings_view.local_terminal.title')}</h3>
                <p className="text-theme-text-muted">{t('settings_view.local_terminal.description')}</p>
            </div>
            <Separator />

            {/* Default Shell Section */}
            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.local_terminal.shell')}</h4>
                <div className="space-y-5">
                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.local_terminal.default_shell')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.local_terminal.default_shell_hint')}</p>
                        </div>
                        <Select
                            value={defaultShellId || ''}
                            onValueChange={(val) => updateLocalTerminal('defaultShellId', val)}
                        >
                            <SelectTrigger className="w-[200px]">
                                <SelectValue placeholder={t('settings_view.local_terminal.select_shell')} />
                            </SelectTrigger>
                            <SelectContent>
                                {shells.map((shell) => (
                                    <SelectItem key={shell.id} value={shell.id}>
                                        {shell.label}
                                    </SelectItem>
                                ))}
                            </SelectContent>
                        </Select>
                    </div>

                    {defaultShell && (
                        <div className="text-xs text-theme-text-muted bg-theme-bg-panel/30 p-3 rounded border border-theme-border/50">
                            <div className="flex items-center gap-2 mb-1">
                                <span className="text-theme-text-muted">{t('settings_view.local_terminal.path')}:</span>
                                <code className="text-theme-text">{defaultShell.path}</code>
                            </div>
                        </div>
                    )}

                    <Separator className="opacity-50" />

                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.local_terminal.default_cwd')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.local_terminal.default_cwd_hint')}</p>
                        </div>
                        <Input
                            value={localSettings?.defaultCwd || ''}
                            onChange={(e) => updateLocalTerminal('defaultCwd', e.target.value)}
                            placeholder="~"
                            className="w-[200px]"
                        />
                    </div>
                </div>
            </div>

            {/* Shell Profile Section */}
            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.local_terminal.shell_profile')}</h4>
                <div className="space-y-5">
                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.local_terminal.load_shell_profile')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.local_terminal.load_shell_profile_hint')}</p>
                        </div>
                        <Checkbox
                            checked={localSettings?.loadShellProfile ?? true}
                            onCheckedChange={(checked) => updateLocalTerminal('loadShellProfile', checked === true)}
                        />
                    </div>
                </div>
            </div>

            {/* Oh My Posh Section (Windows-specific hint) */}
            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.local_terminal.oh_my_posh')}</h4>
                <div className="space-y-5">
                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.local_terminal.oh_my_posh_enable')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.local_terminal.oh_my_posh_enable_hint')}</p>
                        </div>
                        <Checkbox
                            checked={localSettings?.ohMyPoshEnabled ?? false}
                            onCheckedChange={(checked) => updateLocalTerminal('ohMyPoshEnabled', checked === true)}
                        />
                    </div>

                    {localSettings?.ohMyPoshEnabled && (
                        <>
                            {/* Info note about auto-initialization */}
                            <div className="px-3 py-2 rounded bg-blue-500/10 border border-blue-500/20">
                                <p className="text-xs text-blue-400">
                                    💡 {t('settings_view.local_terminal.oh_my_posh_note')}
                                </p>
                            </div>
                            <Separator className="opacity-50" />
                            <div className="flex items-center justify-between">
                                <div>
                                    <Label className="text-theme-text">{t('settings_view.local_terminal.oh_my_posh_theme')}</Label>
                                    <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.local_terminal.oh_my_posh_theme_hint')}</p>
                                </div>
                                <Input
                                    value={localSettings?.ohMyPoshTheme || ''}
                                    onChange={(e) => updateLocalTerminal('ohMyPoshTheme', e.target.value)}
                                    placeholder={t('settings_view.local_terminal.oh_my_posh_theme_placeholder')}
                                    className="w-[300px]"
                                />
                            </div>
                        </>
                    )}
                </div>
            </div>

            {/* Keyboard Shortcuts Section */}
            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.local_terminal.shortcuts')}</h4>
                <div className="space-y-3 text-sm">
                    <div className="flex items-center justify-between py-2">
                        <span className="text-theme-text">{t('settings_view.local_terminal.new_default_shell')}</span>
                        <kbd className="px-2 py-1 bg-theme-bg-hover rounded text-xs text-theme-text-muted border border-theme-border">{platform.isMac ? '⌘T' : 'Ctrl+T'}</kbd>
                    </div>
                    <Separator className="opacity-30" />
                    <div className="flex items-center justify-between py-2">
                        <span className="text-theme-text">{t('settings_view.local_terminal.new_shell_launcher')}</span>
                        <kbd className="px-2 py-1 bg-theme-bg-hover rounded text-xs text-theme-text-muted border border-theme-border">{platform.isMac ? '⌘⇧T' : 'Ctrl+Shift+T'}</kbd>
                    </div>
                </div>
            </div>

            {/* Available Shells Section */}
            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.local_terminal.available_shells')}</h4>
                <div className="space-y-2">
                    {shells.length === 0 ? (
                        <div className="text-center py-8 text-theme-text-muted">
                            {t('settings_view.local_terminal.loading_shells')}
                        </div>
                    ) : (
                        shells.map((shell) => (
                            <div
                                key={shell.id}
                                className="flex items-center justify-between p-3 rounded-md bg-theme-bg-panel/30 border border-theme-border/50"
                            >
                                <div className="flex items-center gap-3">
                                    <div>
                                        <div className="text-sm text-theme-text">{shell.label}</div>
                                        <div className="text-xs text-theme-text-muted">{shell.path}</div>
                                    </div>
                                </div>
                                {shell.id === defaultShellId && (
                                    <span className="text-xs text-yellow-500">{t('settings_view.local_terminal.default')}</span>
                                )}
                            </div>
                        ))
                    )}
                </div>
            </div>
        </div>
    );
};

// Help & About Section Component
const HelpAboutSection = () => {
    const { t } = useTranslation();
    const [appVersion, setAppVersion] = useState<string>('...');
    const updater = useUpdateStore();

    useEffect(() => {
        getVersion().then(setAppVersion).catch(() => setAppVersion('1.4.0'));
    }, []);

    const isMac = platform.isMac;

    // Use shared shortcuts data
    const shortcutCategories = getShortcutCategories(t);

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
            <div>
                <h3 className="text-2xl font-medium text-theme-text mb-2">{t('settings_view.help.title')}</h3>
                <p className="text-theme-text-muted">{t('settings_view.help.description')}</p>
            </div>
            <Separator />

            {/* Version Info */}
            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
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
                </div>

                {/* Update check UI */}
                <div className="mt-4 pt-4 border-t border-theme-border/50 space-y-2">
                    <div className="flex items-center gap-3">
                        {/* 检查更新按钮 — 始终显示，下载/重启时禁用 */}
                        <Button
                            variant="outline"
                            size="sm"
                            onClick={() => updater.checkForUpdate()}
                            disabled={updater.stage === 'checking' || updater.stage === 'downloading' || updater.stage === 'verifying' || updater.stage === 'installing' || updater.stage === 'ready'}
                            className="gap-2 shrink-0"
                        >
                            {updater.stage === 'checking'
                                ? <Loader2 className="h-3.5 w-3.5 animate-spin" />
                                : <RefreshCw className="h-3.5 w-3.5" />
                            }
                            {t('settings_view.help.check_update')}
                        </Button>

                        {/* 各状态行内提示 */}
                        {updater.stage === 'checking' && (
                            <span className="text-sm text-theme-text-muted">
                                {t('settings_view.help.checking')}
                            </span>
                        )}
                        {updater.stage === 'up-to-date' && (
                            <span className="flex items-center gap-1.5 text-sm text-emerald-400">
                                <CheckCircle2 className="h-3.5 w-3.5 shrink-0" />
                                {t('settings_view.help.up_to_date')}
                            </span>
                        )}
                        {updater.stage === 'available' && (
                            <span className="flex items-center gap-1.5 text-sm">
                                <span className="text-theme-text">{t('settings_view.help.update_available')}</span>
                                <span className="font-mono text-theme-accent">v{updater.newVersion}</span>
                            </span>
                        )}
                        {(updater.stage === 'downloading' || updater.stage === 'verifying' || updater.stage === 'installing') && (
                            <span className="text-sm text-theme-text-muted">
                                {updater.stage === 'downloading'
                                    ? `${t('settings_view.help.downloading')} ${updater.totalBytes ? Math.round((updater.downloadedBytes / updater.totalBytes) * 100) : 0}%`
                                    : updater.stage === 'verifying'
                                        ? t('settings_view.help.verifying', 'Verifying...')
                                        : t('settings_view.help.installing', 'Installing...')}
                                {updater.attempt > 1 && ` (${t('settings_view.help.retry', 'Retry')} #${updater.attempt})`}
                            </span>
                        )}
                        {updater.stage === 'ready' && (
                            <span className="text-sm text-emerald-400">
                                {t('settings_view.help.ready_to_restart')}
                            </span>
                        )}
                        {updater.stage === 'error' && (
                            <span className="text-sm text-red-400 truncate">
                                {updater.errorMessage || t('settings_view.help.update_error')}
                            </span>
                        )}

                        {/* 下载 / 取消 / 重启操作按钮 */}
                        {updater.stage === 'available' && (
                            <Button
                                variant="default"
                                size="sm"
                                onClick={updater.startDownload}
                                className="gap-2 shrink-0 ml-auto"
                            >
                                <ArrowDownToLine className="h-3.5 w-3.5" />
                                {t('settings_view.help.download_install')}
                            </Button>
                        )}
                        {updater.stage === 'downloading' && (
                            <Button
                                variant="outline"
                                size="sm"
                                onClick={updater.cancelDownload}
                                className="gap-2 shrink-0 ml-auto"
                            >
                                <X className="h-3.5 w-3.5" />
                                {t('settings_view.help.cancel', 'Cancel')}
                            </Button>
                        )}
                        {updater.stage === 'ready' && (
                            <Button
                                variant="default"
                                size="sm"
                                onClick={updater.restartApp}
                                className="gap-2 shrink-0 ml-auto"
                            >
                                <RotateCw className="h-3.5 w-3.5" />
                                {t('settings_view.help.restart_now')}
                            </Button>
                        )}
                    </div>

                    {/* 下载进度条 */}
                    {updater.stage === 'downloading' && (
                        <div className="h-1.5 bg-theme-bg rounded-full overflow-hidden">
                            <div
                                className="h-full bg-theme-accent rounded-full transition-[width] duration-300"
                                style={{ width: `${updater.totalBytes ? (updater.downloadedBytes / updater.totalBytes) * 100 : 0}%` }}
                            />
                        </div>
                    )}
                </div>
            </div>

            {/* Tech Stack */}
            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
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

            {/* Keyboard Shortcuts */}
            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider flex items-center gap-2">
                    <Keyboard className="h-4 w-4" />
                    {t('settings_view.help.shortcuts')}
                </h4>
                <div className="space-y-5 text-sm">
                    {shortcutCategories.map((category, catIndex) => (
                        <div key={catIndex}>
                            <h5 className="text-xs font-medium text-theme-text-muted uppercase tracking-wider mb-2">
                                {category.title}
                            </h5>
                            <div className="space-y-1">
                                {category.shortcuts.map((shortcut, index) => (
                                    <div key={index} className={`flex items-center justify-between py-1.5 ${index < category.shortcuts.length - 1 ? 'border-b border-theme-border/30' : ''}`}>
                                        <span className="text-theme-text-muted">{shortcut.label}</span>
                                        <kbd className="px-2 py-0.5 rounded bg-theme-bg text-theme-text text-xs font-mono">
                                            {isMac ? shortcut.mac : shortcut.other}
                                        </kbd>
                                    </div>
                                ))}
                            </div>
                        </div>
                    ))}
                </div>
            </div>

            {/* Resources */}
            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                    {t('settings_view.help.resources')}
                </h4>
                <div className="space-y-2">
                    <a
                        href="https://github.com/AnalyseDeCircuit/oxideterm"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="flex items-center justify-between p-3 rounded-lg hover:bg-theme-bg-hover transition-colors group"
                    >
                        <div className="flex items-center gap-3">
                            <Github className="h-5 w-5 text-theme-text-muted" />
                            <span className="text-theme-text">{t('settings_view.help.github')}</span>
                        </div>
                        <ExternalLink className="h-4 w-4 text-theme-text-muted opacity-0 group-hover:opacity-100 transition-opacity" />
                    </a>
                    <a
                        href="https://github.com/AnalyseDeCircuit/oxideterm/issues"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="flex items-center justify-between p-3 rounded-lg hover:bg-theme-bg-hover transition-colors group"
                    >
                        <div className="flex items-center gap-3">
                            <HelpCircle className="h-5 w-5 text-theme-text-muted" />
                            <span className="text-theme-text">{t('settings_view.help.issues')}</span>
                        </div>
                        <ExternalLink className="h-4 w-4 text-theme-text-muted opacity-0 group-hover:opacity-100 transition-opacity" />
                    </a>
                </div>
            </div>

            {/* License */}
            <div className="text-center text-xs text-theme-text-muted">
                <p>{t('settings_view.help.license')}</p>
            </div>
        </div>
    );
};

// ── Background Image Settings Sub-Component ────────────────────────────────

interface BackgroundImageSectionProps {
    terminal: import('../../store/settingsStore').TerminalSettings;
    updateTerminal: <K extends keyof import('../../store/settingsStore').TerminalSettings>(
        key: K, value: import('../../store/settingsStore').TerminalSettings[K]
    ) => void;
}

const BackgroundImageSection = ({ terminal, updateTerminal }: BackgroundImageSectionProps) => {
    const { t } = useTranslation();
    const [processing, setProcessing] = useState(false);
    const [gallery, setGallery] = useState<string[]>([]);
    const galleryGenRef = useRef(0); // generation token to discard stale async responses

    // Local slider state for smooth dragging (debounced commit to store)
    const [localOpacity, setLocalOpacity] = useState(() => Math.round(terminal.backgroundOpacity * 100));
    const [localBlur, setLocalBlur] = useState(() => terminal.backgroundBlur);
    const opacityTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const blurTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    // Sync local state when store value changes externally
    useEffect(() => { setLocalOpacity(Math.round(terminal.backgroundOpacity * 100)); }, [terminal.backgroundOpacity]);
    useEffect(() => { setLocalBlur(terminal.backgroundBlur); }, [terminal.backgroundBlur]);

    const handleOpacityChange = useCallback((val: number) => {
        setLocalOpacity(val);
        if (opacityTimerRef.current) clearTimeout(opacityTimerRef.current);
        opacityTimerRef.current = setTimeout(() => updateTerminal('backgroundOpacity', val / 100), 150);
    }, [updateTerminal]);

    const handleBlurChange = useCallback((val: number) => {
        setLocalBlur(val);
        if (blurTimerRef.current) clearTimeout(blurTimerRef.current);
        blurTimerRef.current = setTimeout(() => updateTerminal('backgroundBlur', val), 150);
    }, [updateTerminal]);

    // Flush on unmount — commit the last pending slider value before clearing timers
    const localOpacityRef = useRef(localOpacity);
    const localBlurRef = useRef(localBlur);
    localOpacityRef.current = localOpacity;
    localBlurRef.current = localBlur;

    useEffect(() => () => {
        if (opacityTimerRef.current) {
            clearTimeout(opacityTimerRef.current);
            updateTerminal('backgroundOpacity', localOpacityRef.current / 100);
        }
        if (blurTimerRef.current) {
            clearTimeout(blurTimerRef.current);
            updateTerminal('backgroundBlur', localBlurRef.current);
        }
    }, []);

    // Load gallery on mount
    const refreshGallery = useCallback(async () => {
        const gen = ++galleryGenRef.current;
        try {
            const paths = await invoke<string[]>('list_terminal_backgrounds');
            // Discard if a newer refresh was issued while we were waiting
            if (gen !== galleryGenRef.current) return;
            setGallery(paths);
        } catch (err) {
            console.error('[Background] Failed to list:', err);
        }
    }, []);

    useEffect(() => { refreshGallery(); }, [refreshGallery]);

    const handleUploadImage = async () => {
        try {
            const selected = await openFileDialog({
                multiple: false,
                directory: false,
                title: t('settings_view.terminal.bg_select_title'),
                filters: [
                    { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] }
                ],
            });
            if (!selected || typeof selected !== 'string') return;

            setProcessing(true);
            const result = await invoke<{
                path: string;
                originalSize: number;
                storedSize: number;
                animated: boolean;
            }>('upload_terminal_background', { sourcePath: selected });

            // Auto-activate the newly uploaded image
            updateTerminal('backgroundImage', result.path);
            await refreshGallery();
        } catch (err) {
            console.error('[Background] Failed to upload:', err);
        } finally {
            setProcessing(false);
        }
    };

    const handleActivate = (path: string) => {
        updateTerminal('backgroundImage', path);
    };

    const handleDeleteImage = async (path: string) => {
        try {
            await invoke('delete_terminal_background', { path });
            // If the deleted image was active, clear selection
            if (terminal.backgroundImage === path) {
                updateTerminal('backgroundImage', null);
            }
            await refreshGallery();
        } catch (err) {
            console.error('[Background] Failed to delete:', err);
        }
    };

    const handleClearAll = async () => {
        try {
            await invoke('clear_terminal_background');
            // Invalidate any in-flight gallery refresh before clearing local state
            galleryGenRef.current++;
            updateTerminal('backgroundImage', null);
            setGallery([]);
        } catch {
            // Backend failed — re-fetch to show actual disk state
            await refreshGallery();
        }
    };

    return (
        <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
            <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider flex items-center gap-2">
                <ImageIcon className="h-4 w-4" />
                {t('settings_view.terminal.bg_title')}
            </h4>

            <div className="space-y-4">
                {/* Master toggle */}
                {terminal.backgroundImage && (
                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.bg_enabled')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.bg_enabled_hint')}</p>
                        </div>
                        <Checkbox
                            checked={terminal.backgroundEnabled !== false}
                            onCheckedChange={(v) => updateTerminal('backgroundEnabled', !!v)}
                        />
                    </div>
                )}

                {/* Gallery grid */}
                <div>
                    <div className="flex items-center justify-between mb-2">
                        <Label className="text-theme-text">{t('settings_view.terminal.bg_gallery')}</Label>
                        <div className="flex items-center gap-2">
                            <Button variant="outline" size="sm" onClick={handleUploadImage} disabled={processing}>
                                <Plus className="h-3.5 w-3.5 mr-1" />
                                {processing ? '...' : t('settings_view.terminal.bg_add')}
                            </Button>
                            {gallery.length > 0 && (
                                <Button variant="ghost" size="sm" onClick={handleClearAll} className="text-red-400 hover:text-red-300">
                                    <Trash2 className="h-3.5 w-3.5 mr-1" />
                                    {t('settings_view.terminal.bg_clear_all')}
                                </Button>
                            )}
                        </div>
                    </div>
                    {gallery.length === 0 ? (
                        <p className="text-xs text-theme-text-muted">{t('settings_view.terminal.bg_hint')}</p>
                    ) : (
                        <div className="grid grid-cols-4 gap-2">
                            {gallery.map((path) => {
                                const isActive = terminal.backgroundImage === path;
                                const url = convertFileSrc(path);
                                return (
                                    <div
                                        key={path}
                                        className={cn(
                                            "relative group rounded-md overflow-hidden border-2 cursor-pointer transition-all aspect-video",
                                            isActive
                                                ? "border-theme-accent ring-1 ring-theme-accent/50"
                                                : "border-theme-border hover:border-theme-accent/50"
                                        )}
                                        onClick={() => handleActivate(path)}
                                    >
                                        <img
                                            src={url}
                                            alt=""
                                            className="w-full h-full object-cover"
                                            draggable={false}
                                        />
                                        {/* Active badge */}
                                        {isActive && (
                                            <div className="absolute top-1 left-1 px-1.5 py-0.5 bg-theme-accent text-white text-[10px] rounded font-medium">
                                                {t('settings_view.terminal.bg_active')}
                                            </div>
                                        )}
                                        {/* Delete button */}
                                        <button
                                            className="absolute top-1 right-1 p-0.5 rounded bg-black/60 text-white/80 hover:text-red-400 opacity-0 group-hover:opacity-100 transition-opacity"
                                            onClick={(e) => {
                                                e.stopPropagation();
                                                handleDeleteImage(path);
                                            }}
                                        >
                                            <X className="h-3 w-3" />
                                        </button>
                                    </div>
                                );
                            })}
                        </div>
                    )}
                </div>

                {/* Opacity slider */}
                {terminal.backgroundImage && (
                    <>
                        <div className="flex items-center justify-between">
                            <div>
                                <Label className="text-theme-text">{t('settings_view.terminal.bg_opacity')}</Label>
                                <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.bg_opacity_hint')}</p>
                            </div>
                            <div className="flex items-center gap-2">
                                <input
                                    type="range"
                                    min={3}
                                    max={50}
                                    value={localOpacity}
                                    onChange={(e) => handleOpacityChange(parseInt(e.target.value))}
                                    className="w-28 accent-orange-500"
                                />
                                <span className="text-xs text-theme-text-muted w-10 text-right">
                                    {localOpacity}%
                                </span>
                            </div>
                        </div>

                        {/* Blur slider */}
                        <div className="flex items-center justify-between">
                            <div>
                                <Label className="text-theme-text">{t('settings_view.terminal.bg_blur')}</Label>
                                <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.bg_blur_hint')}</p>
                            </div>
                            <div className="flex items-center gap-2">
                                <input
                                    type="range"
                                    min={0}
                                    max={20}
                                    value={localBlur}
                                    onChange={(e) => handleBlurChange(parseInt(e.target.value))}
                                    className="w-28 accent-orange-500"
                                />
                                <span className="text-xs text-theme-text-muted w-10 text-right">
                                    {localBlur}px
                                </span>
                            </div>
                        </div>

                        {/* Fit mode */}
                        <div className="flex items-center justify-between">
                            <div>
                                <Label className="text-theme-text">{t('settings_view.terminal.bg_fit')}</Label>
                                <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.bg_fit_hint')}</p>
                            </div>
                            <Select
                                value={terminal.backgroundFit}
                                onValueChange={(val) => updateTerminal('backgroundFit', val as BackgroundFit)}
                            >
                                <SelectTrigger className="w-32">
                                    <SelectValue />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="cover">{t('settings_view.terminal.bg_fit_cover')}</SelectItem>
                                    <SelectItem value="contain">{t('settings_view.terminal.bg_fit_contain')}</SelectItem>
                                    <SelectItem value="fill">{t('settings_view.terminal.bg_fit_fill')}</SelectItem>
                                    <SelectItem value="tile">{t('settings_view.terminal.bg_fit_tile')}</SelectItem>
                                </SelectContent>
                            </Select>
                        </div>

                        {/* Tab type toggles */}
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.bg_tabs')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5 mb-3">{t('settings_view.terminal.bg_tabs_hint')}</p>
                            <div className="grid grid-cols-3 gap-2">
                                {([
                                    ['terminal', t('settings_view.terminal.bg_tab_terminal'), TerminalIcon],
                                    ['local_terminal', t('settings_view.terminal.bg_tab_local'), Monitor],
                                    ['sftp', t('settings_view.terminal.bg_tab_sftp'), FolderInput],
                                    ['forwards', t('settings_view.terminal.bg_tab_forwards'), ArrowLeftRight],
                                    ['settings', t('settings_view.terminal.bg_tab_settings'), Settings],
                                    ['ide', t('settings_view.terminal.bg_tab_ide'), Code2],
                                    ['connection_monitor', t('settings_view.terminal.bg_tab_monitor'), Activity],
                                    ['connection_pool', t('settings_view.terminal.bg_tab_connections'), Network],
                                    ['topology', t('settings_view.terminal.bg_tab_topology'), Network],
                                    ['file_manager', t('settings_view.terminal.bg_tab_files'), Folder],
                                    ['session_manager', t('settings_view.terminal.bg_tab_sessions'), ListTree],
                                    ...(platform.isMac ? [['launcher', t('settings_view.terminal.bg_tab_launcher'), Rocket] as const] : []),
                                    ['plugin_manager', t('settings_view.terminal.bg_tab_plugins'), Puzzle],
                                ] as const).map(([type, label, Icon]) => {
                                    const enabledTabs = terminal.backgroundEnabledTabs ?? ['terminal', 'local_terminal'];
                                    const checked = enabledTabs.includes(type);
                                    return (
                                        <button
                                            key={type}
                                            type="button"
                                            onClick={() => {
                                                const next = checked
                                                    ? enabledTabs.filter((t: string) => t !== type)
                                                    : [...enabledTabs, type];
                                                updateTerminal('backgroundEnabledTabs', next);
                                            }}
                                            className={cn(
                                                "flex items-center gap-2 rounded-md border px-3 py-2 text-xs transition-colors cursor-pointer select-none",
                                                checked
                                                    ? "border-theme-accent/60 bg-theme-accent/10 text-theme-accent"
                                                    : "border-theme-border bg-theme-bg-panel/30 text-theme-text-muted hover:border-theme-border hover:bg-theme-bg-hover/50"
                                            )}
                                        >
                                            <Icon className="size-3.5 shrink-0" />
                                            <span className="truncate">{label}</span>
                                        </button>
                                    );
                                })}
                            </div>
                        </div>
                    </>
                )}
            </div>
        </div>
    );
};

/** Icon map for per-tool approval UI */
const TOOL_ICON_MAP: Record<string, React.ElementType> = {
    terminal_exec: TerminalIcon, read_file: FileText, write_file: Pen,
    list_directory: FolderOpen, grep_search: Search, git_status: GitBranch,
    list_sessions: Network, get_terminal_buffer: TerminalIcon, search_terminal: Search,
    list_connections: Network, list_port_forwards: Radio, get_detected_ports: Radio,
    get_connection_health: Activity, create_port_forward: CirclePlus, stop_port_forward: CircleStop,
    sftp_list_dir: FolderSearch, sftp_read_file: HardDrive, sftp_stat: Info, sftp_get_cwd: HardDrive,
    ide_get_open_files: FileCode, ide_get_file_content: FileCode, ide_get_project_info: Code2, ide_apply_edit: Pen,
    // Local terminal
    local_list_shells: TerminalIcon, local_get_terminal_info: ListTree, local_exec: TerminalIcon, local_get_drives: HardDrive,
    // Settings
    get_settings: Settings, update_setting: Settings,
    // Connection pool
    get_pool_stats: Activity, set_pool_config: Settings,
    // Connection monitor
    get_all_health: Activity, get_resource_metrics: Activity,
    // Session manager
    list_saved_connections: Network, search_saved_connections: Search, get_session_tree: ListTree,
    // Plugin manager
    list_plugins: Puzzle,
};

/** Group icon map for tool group headers */
const TOOL_GROUP_ICONS: Record<string, React.ElementType> = {
    terminal: TerminalIcon, session: Network, infrastructure: Radio, sftp: FolderInput, ide: Code2,
    local_terminal: TerminalIcon, settings: Settings, connection_pool: Activity,
    connection_monitor: Activity, session_manager: Network, plugin_manager: Puzzle,
};

export const SettingsView = () => {
    const { t } = useTranslation();
    const { success: toastSuccess, error: toastError } = useToast();
    const { confirm: confirmDialog, ConfirmDialog } = useConfirm();
    const bgActive = useTabBgActive('settings');
    const [activeTab, setActiveTab] = useState('general');

    // Use unified settings store
    const { settings, updateTerminal, updateAppearance, updateConnectionDefaults, updateAi, updateSftp, updateIde, updateReconnect, setLanguage, addProvider, removeProvider, updateProvider, setActiveProvider, refreshProviderModels } = useSettingsStore();
    const { general, terminal, appearance, connectionDefaults, ai, sftp, ide, reconnect } = settings;

    // AI enable confirmation dialog
    const [showAiConfirm, setShowAiConfirm] = useState(false);
    const [refreshingModels, setRefreshingModels] = useState<string | null>(null);

    // Custom theme editor state
    const [themeEditorOpen, setThemeEditorOpen] = useState(false);
    const [editingThemeId, setEditingThemeId] = useState<string | null>(null);

    // Local slider state for border radius (debounced like opacity/blur)
    const [localBorderRadius, setLocalBorderRadius] = useState(() => appearance.borderRadius);
    const borderRadiusTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    useEffect(() => { setLocalBorderRadius(appearance.borderRadius); }, [appearance.borderRadius]);
    const handleBorderRadiusChange = useCallback((val: number) => {
        setLocalBorderRadius(val);
        if (borderRadiusTimerRef.current) clearTimeout(borderRadiusTimerRef.current);
        borderRadiusTimerRef.current = setTimeout(() => updateAppearance('borderRadius', val), 150);
    }, [updateAppearance]);
    const localBorderRadiusRef = useRef(localBorderRadius);
    localBorderRadiusRef.current = localBorderRadius;
    useEffect(() => () => {
        if (borderRadiusTimerRef.current) {
            clearTimeout(borderRadiusTimerRef.current);
            updateAppearance('borderRadius', localBorderRadiusRef.current);
        }
    }, []);

    // Local state for UI font (debounced commit)
    const [localUiFont, setLocalUiFont] = useState(() => appearance.uiFontFamily);
    const uiFontTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    useEffect(() => { setLocalUiFont(appearance.uiFontFamily); }, [appearance.uiFontFamily]);
    const handleUiFontChange = useCallback((val: string) => {
        setLocalUiFont(val);
        if (uiFontTimerRef.current) clearTimeout(uiFontTimerRef.current);
        uiFontTimerRef.current = setTimeout(() => updateAppearance('uiFontFamily', val), 300);
    }, [updateAppearance]);

    // Data State
    const [keys, setKeys] = useState<SshKeyInfo[]>([]);
    const [groups, setGroups] = useState<string[]>([]);
    const [newGroup, setNewGroup] = useState('');
    const [sshHosts, setSshHosts] = useState<SshHostInfo[]>([]);

    useEffect(() => {
        if (activeTab === 'ssh') {
            api.checkSshKeys()
                .then(setKeys)
                .catch((e) => {
                    console.error('Failed to load SSH keys:', e);
                    setKeys([]);
                });
        } else if (activeTab === 'connections') {
            api.getGroups()
                .then(setGroups)
                .catch((e) => {
                    console.error('Failed to load groups:', e);
                    setGroups([]);
                });
            api.listSshConfigHosts()
                .then(setSshHosts)
                .catch((e) => {
                    console.error('Failed to load SSH hosts:', e);
                    setSshHosts([]);
                });
        }
    }, [activeTab]);

    const handleCreateGroup = async () => {
        if (!newGroup.trim()) return;
        try {
            await api.createGroup(newGroup.trim());
            setNewGroup('');
            const updatedGroups = await api.getGroups();
            setGroups(updatedGroups);
        } catch (e) {
            console.error('Failed to create group:', e);
            toastError(t('settings_view.errors.create_group_failed', { error: e }));
        }
    };

    const handleDeleteGroup = async (name: string) => {
        try {
            await api.deleteGroup(name);
            const updatedGroups = await api.getGroups();
            setGroups(updatedGroups);
        } catch (e) {
            console.error('Failed to delete group:', e);
            toastError(t('settings_view.errors.delete_group_failed', { error: e }));
        }
    };

    const handleImportHost = async (alias: string) => {
        try {
            const imported = await api.importSshHost(alias);
            toastSuccess(t('settings_view.errors.import_success', { name: imported.name }));
            // Remove from list to show it's imported
            setSshHosts(prev => prev.filter(h => h.alias !== alias));
            // Refresh saved connections in sidebar
            const { loadSavedConnections } = useAppStore.getState();
            await loadSavedConnections();
        } catch (e) {
            console.error('Failed to import SSH host:', e);
            toastError(t('settings_view.errors.import_failed', { error: e }));
        }
    };

    return (
        <div className={`flex h-full w-full text-theme-text ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
            {/* Sidebar */}
            <div className="w-56 bg-theme-bg-panel border-r border-theme-border flex flex-col pt-6 pb-4 min-h-0">
                <div className="px-5 mb-6">
                    <h2 className="text-xl font-semibold text-theme-text">{t('settings_view.title')}</h2>
                </div>
                <div className="space-y-1 px-3 flex-1 overflow-y-auto min-h-0">
                    {/* ── 基础 ── */}
                    <Button
                        variant={activeTab === 'general' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('general')}
                    >
                        <Monitor className="h-4 w-4" /> {t('settings.general.title')}
                    </Button>

                    <Separator className="!my-2" />

                    {/* ── 终端（字体/光标/缓冲区 → 主题/背景 → 本地 shell） ── */}
                    <Button
                        variant={activeTab === 'terminal' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('terminal')}
                    >
                        <TerminalIcon className="h-4 w-4" /> {t('settings.terminal.title')}
                    </Button>
                    <Button
                        variant={activeTab === 'appearance' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('appearance')}
                    >
                        <Monitor className="h-4 w-4" /> {t('settings_view.tabs.appearance')}
                    </Button>
                    <Button
                        variant={activeTab === 'local' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('local')}
                    >
                        <Square className="h-4 w-4" /> {t('settings_view.tabs.local')}
                    </Button>

                    <Separator className="!my-2" />

                    {/* ── 连接（默认/分组 → 密钥 → 重连策略） ── */}
                    <Button
                        variant={activeTab === 'connections' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('connections')}
                    >
                        <Shield className="h-4 w-4" /> {t('settings_view.tabs.connections')}
                    </Button>
                    <Button
                        variant={activeTab === 'ssh' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('ssh')}
                    >
                        <Key className="h-4 w-4" /> {t('settings_view.tabs.ssh')}
                    </Button>
                    <Button
                        variant={activeTab === 'reconnect' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('reconnect')}
                    >
                        <WifiOff className="h-4 w-4" /> {t('settings_view.tabs.reconnect')}
                    </Button>

                    <Separator className="!my-2" />

                    {/* ── 功能（文件传输 → 编辑器 → AI） ── */}
                    <Button
                        variant={activeTab === 'sftp' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('sftp')}
                    >
                        <HardDrive className="h-4 w-4" /> {t('settings_view.tabs.sftp')}
                    </Button>
                    <Button
                        variant={activeTab === 'ide' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('ide')}
                    >
                        <Code2 className="h-4 w-4" /> {t('settings_view.tabs.ide', 'IDE')}
                    </Button>
                    <Button
                        variant={activeTab === 'ai' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('ai')}
                    >
                        <Sparkles className="h-4 w-4" /> {t('settings_view.tabs.ai')}
                    </Button>

                    <Separator className="!my-2" />

                    {/* ── 帮助 ── */}
                    <Button
                        variant={activeTab === 'help' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('help')}
                    >
                        <HelpCircle className="h-4 w-4" /> {t('settings_view.tabs.help')}
                    </Button>
                </div>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-y-auto">
                <div className="max-w-4xl mx-auto p-10">
                    {activeTab === 'general' && (
                        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
                            <div>
                                <h3 className="text-2xl font-medium text-theme-text mb-2">{t('settings_view.general.title')}</h3>
                                <p className="text-theme-text-muted">{t('settings_view.general.description')}</p>
                            </div>
                            <Separator />

                            {/* Language Selection */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                                    {t('settings_view.general.language')}
                                </h4>
                                <div className="space-y-5">
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.general.language')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.general.language_hint')}</p>
                                        </div>
                                        <Select
                                            value={general.language}
                                            onValueChange={(val) => setLanguage(val as Language)}
                                        >
                                            <SelectTrigger className="w-[200px]">
                                                <SelectValue />
                                            </SelectTrigger>
                                            <SelectContent>
                                                <SelectItem value="de">Deutsch</SelectItem>
                                                <SelectItem value="en">English</SelectItem>
                                                <SelectItem value="es-ES">Español (España)</SelectItem>
                                                <SelectItem value="fr-FR">Français (France)</SelectItem>
                                                <SelectItem value="it">Italiano</SelectItem>
                                                <SelectItem value="ko">한국어</SelectItem>
                                                <SelectItem value="pt-BR">Português (Brasil)</SelectItem>
                                                <SelectItem value="vi">Tiếng Việt</SelectItem>
                                                <SelectItem value="ja">日本語</SelectItem>
                                                <SelectItem value="zh-CN">简体中文</SelectItem>
                                                <SelectItem value="zh-TW">繁體中文</SelectItem>
                                            </SelectContent>
                                        </Select>
                                    </div>
                                </div>
                            </div>
                        </div>
                    )}

                    {activeTab === 'terminal' && (
                        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
                            <div>
                                <h3 className="text-2xl font-medium text-theme-text mb-2">{t('settings_view.terminal.title')}</h3>
                                <p className="text-theme-text-muted">{t('settings_view.terminal.description')}</p>
                            </div>
                            <Separator />

                            {/* Font Section */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.terminal.font')}</h4>
                                <div className="space-y-5">
                                    {/* 预设轨道: Preset Font Selector */}
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.terminal.font_family')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.font_family_hint')}</p>
                                        </div>
                                        <Select
                                            value={terminal.fontFamily}
                                            onValueChange={(val) => updateTerminal('fontFamily', val as FontFamily)}
                                        >
                                            <SelectTrigger className="w-[200px]">
                                                <SelectValue placeholder={t('settings_view.terminal.select_font')} />
                                            </SelectTrigger>
                                            <SelectContent>
                                                <SelectItem value="jetbrains">JetBrains Mono NF (Subset) ✓</SelectItem>
                                                <SelectItem value="meslo">MesloLGM NF (Subset) ✓</SelectItem>
                                                <SelectItem value="maple">Maple Mono NF CN (Subset) ✓</SelectItem>
                                                <SelectItem value="cascadia">Cascadia Code</SelectItem>
                                                <SelectItem value="consolas">Consolas</SelectItem>
                                                <SelectItem value="menlo">Menlo</SelectItem>
                                                <SelectItem value="custom">{t('settings_view.terminal.custom_font')}</SelectItem>
                                            </SelectContent>
                                        </Select>
                                    </div>

                                    {/* 自定义轨道: Custom Font Input */}
                                    {terminal.fontFamily === 'custom' && (
                                        <div className="flex items-center justify-between">
                                            <div>
                                                <Label className="text-theme-text">{t('settings_view.terminal.custom_font_stack')}</Label>
                                                <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.custom_font_stack_hint')}</p>
                                            </div>
                                            <Input
                                                type="text"
                                                value={terminal.customFontFamily}
                                                onChange={(e) => updateTerminal('customFontFamily', e.target.value)}
                                                placeholder="'Sarasa Fixed SC', 'Fira Code', monospace"
                                                className="w-[300px] font-mono text-sm"
                                            />
                                        </div>
                                    )}

                                    {/* 字体预览 */}
                                    <div className="rounded-md border border-theme-border bg-zinc-950 p-4">
                                        <p className="text-xs text-theme-text-muted mb-2">{t('settings_view.terminal.font_preview')}</p>
                                        <div
                                            className="text-theme-text leading-relaxed"
                                            style={{
                                                fontFamily: terminal.fontFamily === 'custom' && terminal.customFontFamily
                                                    ? (terminal.customFontFamily.toLowerCase().includes('monospace')
                                                        ? terminal.customFontFamily.replace(/,?\s*monospace\s*$/, ', "Maple Mono NF CN (Subset)", monospace')
                                                        : `${terminal.customFontFamily}, "Maple Mono NF CN (Subset)", monospace`)
                                                    : terminal.fontFamily === 'jetbrains' ? '"JetBrainsMono Nerd Font", "JetBrains Mono NF (Subset)", "Maple Mono NF CN (Subset)", monospace'
                                                        : terminal.fontFamily === 'meslo' ? '"MesloLGM Nerd Font", "MesloLGM NF (Subset)", "Maple Mono NF CN (Subset)", monospace'
                                                            : terminal.fontFamily === 'maple' ? '"Maple Mono NF CN (Subset)", "Maple Mono NF", monospace'
                                                                : terminal.fontFamily === 'cascadia' ? '"Cascadia Code NF", "Cascadia Code", "Maple Mono NF CN (Subset)", monospace'
                                                                    : terminal.fontFamily === 'consolas' ? 'Consolas, "Maple Mono NF CN (Subset)", monospace'
                                                                        : terminal.fontFamily === 'menlo' ? 'Menlo, Monaco, "Maple Mono NF CN (Subset)", monospace'
                                                                            : '"Maple Mono NF CN (Subset)", monospace',
                                                fontSize: `${terminal.fontSize}px`,
                                                lineHeight: terminal.lineHeight,
                                            }}
                                        >
                                            <div>ABCDEFG abcdefg 0123456789</div>
                                            <div className="text-zinc-400">{'-> => == != <= >= {}'}</div>
                                            <div className="text-emerald-400">天地玄黄 The quick brown fox</div>
                                            <div className="text-amber-400" style={{ letterSpacing: '0.1em' }}>       󰊤  </div>
                                        </div>
                                    </div>

                                    <Separator className="opacity-50" />

                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.terminal.font_size')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.font_size_hint')}</p>
                                        </div>
                                        <div className="flex items-center gap-3">
                                            <Input
                                                type="range"
                                                min="8"
                                                max="32"
                                                step="1"
                                                value={terminal.fontSize}
                                                onChange={(e) => updateTerminal('fontSize', parseInt(e.target.value))}
                                                className="w-32"
                                            />
                                            <div className="flex items-center gap-1">
                                                <Input
                                                    type="number"
                                                    value={terminal.fontSize}
                                                    onChange={(e) => updateTerminal('fontSize', parseInt(e.target.value))}
                                                    className="w-16"
                                                />
                                                <span className="text-xs text-theme-text-muted">px</span>
                                            </div>
                                        </div>
                                    </div>

                                    <Separator className="opacity-50" />

                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.terminal.line_height')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.line_height_hint')}</p>
                                        </div>
                                        <Input
                                            type="number"
                                            step="0.1"
                                            min="0.8"
                                            max="3"
                                            value={terminal.lineHeight}
                                            onChange={(e) => updateTerminal('lineHeight', parseFloat(e.target.value))}
                                            className="w-20"
                                        />
                                    </div>

                                    <Separator className="opacity-50" />

                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.terminal.renderer')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.renderer_hint')}</p>
                                        </div>
                                        <Select
                                            value={terminal.renderer}
                                            onValueChange={(val) => updateTerminal('renderer', val as RendererType)}
                                        >
                                            <SelectTrigger className="w-[200px]">
                                                <SelectValue />
                                            </SelectTrigger>
                                            <SelectContent>
                                                <SelectItem value="auto">{t('settings_view.terminal.renderer_auto')}</SelectItem>
                                                <SelectItem value="webgl">WebGL</SelectItem>
                                                <SelectItem value="canvas">Canvas</SelectItem>
                                            </SelectContent>
                                        </Select>
                                    </div>

                                    <Separator className="opacity-50" />

                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.terminal.adaptive_renderer')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.adaptive_renderer_hint')}</p>
                                        </div>
                                        <Select
                                            value={terminal.adaptiveRenderer ?? 'auto'}
                                            onValueChange={(val) => updateTerminal('adaptiveRenderer', val as AdaptiveRendererMode)}
                                        >
                                            <SelectTrigger className="w-[200px]">
                                                <SelectValue />
                                            </SelectTrigger>
                                            <SelectContent>
                                                <SelectItem value="auto">{t('settings_view.terminal.adaptive_renderer_auto')}</SelectItem>
                                                <SelectItem value="always-60">{t('settings_view.terminal.adaptive_renderer_always60')}</SelectItem>
                                                <SelectItem value="off">{t('settings_view.terminal.adaptive_renderer_off')}</SelectItem>
                                            </SelectContent>
                                        </Select>
                                    </div>

                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.terminal.show_fps_overlay')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.show_fps_overlay_hint')}</p>
                                        </div>
                                        <Checkbox
                                            id="show-fps-overlay"
                                            checked={terminal.showFpsOverlay ?? false}
                                            onCheckedChange={(checked) => updateTerminal('showFpsOverlay', checked as boolean)}
                                        />
                                    </div>
                                </div>
                            </div>

                            {/* Cursor Section */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.terminal.cursor')}</h4>
                                <div className="space-y-5">
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.terminal.cursor_style')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.cursor_style_hint')}</p>
                                        </div>
                                        <Select
                                            value={terminal.cursorStyle}
                                            onValueChange={(val) => updateTerminal('cursorStyle', val as CursorStyle)}
                                        >
                                            <SelectTrigger className="w-[160px]">
                                                <SelectValue />
                                            </SelectTrigger>
                                            <SelectContent>
                                                <SelectItem value="block">{t('settings_view.terminal.cursor_block')}</SelectItem>
                                                <SelectItem value="underline">{t('settings_view.terminal.cursor_underline')}</SelectItem>
                                                <SelectItem value="bar">{t('settings_view.terminal.cursor_bar')}</SelectItem>
                                            </SelectContent>
                                        </Select>
                                    </div>

                                    <Separator className="opacity-50" />

                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.terminal.cursor_blink')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.cursor_blink_hint')}</p>
                                        </div>
                                        <Checkbox
                                            id="blink"
                                            checked={terminal.cursorBlink}
                                            onCheckedChange={(checked) => updateTerminal('cursorBlink', checked as boolean)}
                                        />
                                    </div>
                                </div>
                            </div>

                            {/* Input Safety Section */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.terminal.input_safety')}</h4>
                                <div className="flex items-center justify-between">
                                    <div>
                                        <Label className="text-theme-text">{t('settings_view.terminal.paste_protection')}</Label>
                                        <p className="text-xs text-theme-text-muted mt-0.5">
                                            {t('settings_view.terminal.paste_protection_hint')}
                                        </p>
                                    </div>
                                    <Checkbox
                                        id="paste-protection"
                                        checked={terminal.pasteProtection}
                                        onCheckedChange={(checked) => updateTerminal('pasteProtection', checked as boolean)}
                                    />
                                </div>
                                <div className="flex items-center justify-between mt-4">
                                    <div>
                                        <Label className="text-theme-text">{t('settings_view.terminal.osc52_clipboard')}</Label>
                                        <p className="text-xs text-theme-text-muted mt-0.5">
                                            {t('settings_view.terminal.osc52_clipboard_hint')}
                                        </p>
                                    </div>
                                    <Checkbox
                                        id="osc52-clipboard"
                                        checked={terminal.osc52Clipboard}
                                        onCheckedChange={(checked) => updateTerminal('osc52Clipboard', checked as boolean)}
                                    />
                                </div>
                            </div>

                            {/* Buffer Section */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.terminal.buffer')}</h4>
                                <div className="flex items-center justify-between">
                                    <div>
                                        <Label className="text-theme-text">{t('settings_view.terminal.scrollback')}</Label>
                                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.scrollback_hint')}</p>
                                    </div>
                                    <Input
                                        type="number"
                                        value={terminal.scrollback}
                                        onChange={(e) => updateTerminal('scrollback', parseInt(e.target.value))}
                                        min={100}
                                        max={50000}
                                        className="w-28"
                                    />
                                </div>
                            </div>
                        </div>
                    )}

                    {activeTab === 'appearance' && (
                        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
                            <div>
                                <h3 className="text-2xl font-medium text-theme-text mb-2">{t('settings_view.appearance.title')}</h3>
                                <p className="text-theme-text-muted">{t('settings_view.appearance.description')}</p>
                            </div>
                            <Separator />

                            {/* Theme Section */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <div className="flex items-center justify-between mb-4">
                                    <h4 className="text-sm font-medium text-theme-text uppercase tracking-wider">{t('settings_view.appearance.theme')}</h4>
                                    <div className="flex gap-2">
                                        <Button
                                            variant="outline"
                                            size="sm"
                                            className="h-7 text-xs text-theme-text border-theme-border"
                                            onClick={async () => {
                                                try {
                                                    const selected = await openFileDialog({
                                                        multiple: false,
                                                        filters: [{ name: 'JSON', extensions: ['json'] }],
                                                    });
                                                    if (!selected || typeof selected !== 'string') return;
                                                    const { readTextFile } = await import('@tauri-apps/plugin-fs');
                                                    const content = await readTextFile(selected);
                                                    const { theme: imported } = importTheme(content);
                                                    toastSuccess(t('settings_view.appearance.theme_import_success', { name: imported.name }));
                                                } catch (e: unknown) {
                                                    toastError(t('settings_view.appearance.theme_import_error', { error: e instanceof Error ? e.message : String(e) }));
                                                }
                                            }}
                                        >
                                            <Upload className="w-3 h-3 mr-1" />
                                            {t('settings_view.appearance.theme_import')}
                                        </Button>
                                        {isCustomTheme(terminal.theme) && (
                                            <Button
                                                variant="outline"
                                                size="sm"
                                                className="h-7 text-xs text-theme-text border-theme-border"
                                                onClick={() => {
                                                    const json = exportTheme(terminal.theme);
                                                    if (!json) return;
                                                    const blob = new Blob([json], { type: 'application/json' });
                                                    const url = URL.createObjectURL(blob);
                                                    const a = document.createElement('a');
                                                    a.href = url;
                                                    a.download = `${formatThemeName(terminal.theme).replace(/\s+/g, '-').toLowerCase()}.oxtheme.json`;
                                                    a.click();
                                                    URL.revokeObjectURL(url);
                                                    toastSuccess(t('settings_view.appearance.theme_export_success'));
                                                }}
                                            >
                                                <Download className="w-3 h-3 mr-1" />
                                                {t('settings_view.appearance.theme_export')}
                                            </Button>
                                        )}
                                        {isCustomTheme(terminal.theme) && (
                                            <Button
                                                variant="outline"
                                                size="sm"
                                                className="h-7 text-xs text-theme-text border-theme-border"
                                                onClick={() => { setEditingThemeId(terminal.theme); setThemeEditorOpen(true); }}
                                            >
                                                {t('settings_view.custom_theme.edit')}
                                            </Button>
                                        )}
                                        <Button
                                            variant="outline"
                                            size="sm"
                                            className="h-7 text-xs text-theme-text border-theme-border"
                                            onClick={() => { setEditingThemeId(null); setThemeEditorOpen(true); }}
                                        >
                                            <Plus className="w-3 h-3 mr-1" />
                                            {t('settings_view.custom_theme.create')}
                                        </Button>
                                    </div>
                                </div>
                                <div className="space-y-4">
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.appearance.color_theme')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.appearance.color_theme_hint')}</p>
                                        </div>
                                        <Select
                                            value={terminal.theme}
                                            onValueChange={(val) => updateTerminal('theme', val)}
                                        >
                                            <SelectTrigger className="w-[200px] text-theme-text">
                                                <SelectValue placeholder="Select theme">
                                                    {formatThemeName(terminal.theme)}
                                                </SelectValue>
                                            </SelectTrigger>
                                            <SelectContent className="bg-theme-bg-panel border-theme-border max-h-[300px]">
                                                {/* Custom Themes Group */}
                                                {Object.keys(getCustomThemes()).length > 0 && (
                                                    <>
                                                        <SelectGroup>
                                                            <SelectLabel className="text-theme-text-muted text-xs uppercase tracking-wider px-2 py-1.5 font-bold whitespace-normal break-words">{t('settings_view.appearance.theme_group_custom')}</SelectLabel>
                                                            {Object.keys(getCustomThemes()).sort().map((key) => (
                                                                <SelectItem key={key} value={key} className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text pl-4">
                                                                    {formatThemeName(key)}
                                                                </SelectItem>
                                                            ))}
                                                        </SelectGroup>
                                                        <SelectSeparator className="bg-zinc-700 my-1" />
                                                    </>
                                                )}

                                                {/* Oxide Themes Group */}
                                                <SelectGroup>
                                                    <SelectLabel className="text-theme-text-muted text-xs uppercase tracking-wider px-2 py-1.5 font-bold whitespace-normal break-words">{t('settings_view.appearance.theme_group_oxide')}</SelectLabel>
                                                    {[
                                                        'azurite', 'bismuth', 'chromium-oxide', 'cobalt', 'cuprite',
                                                        'hematite', 'malachite', 'magnetite', 'ochre', 'oxide',
                                                        'paper-oxide', 'silver-oxide', 'verdigris'
                                                    ].map((key) => (
                                                        <SelectItem key={key} value={key} className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text pl-4">
                                                            {formatThemeName(key)}
                                                        </SelectItem>
                                                    ))}
                                                </SelectGroup>

                                                <SelectSeparator className="bg-zinc-700 my-1" />

                                                {/* Classic Themes Group */}
                                                <SelectGroup>
                                                    <SelectLabel className="text-theme-text-muted text-xs uppercase tracking-wider px-2 py-1.5 font-bold whitespace-normal break-words">{t('settings_view.appearance.theme_group_classic')}</SelectLabel>
                                                    {Object.keys(themes)
                                                        .filter(key => ![
                                                            'azurite', 'bismuth', 'chromium-oxide', 'cobalt', 'cuprite',
                                                            'hematite', 'malachite', 'magnetite', 'ochre', 'oxide',
                                                            'paper-oxide', 'silver-oxide', 'verdigris'
                                                        ].includes(key))
                                                        .sort()
                                                        .map(key => (
                                                            <SelectItem key={key} value={key} className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text pl-4">
                                                                {formatThemeName(key)}
                                                            </SelectItem>
                                                        ))}
                                                </SelectGroup>
                                            </SelectContent>
                                        </Select>
                                    </div>
                                    <ThemePreview themeName={terminal.theme} />
                                </div>
                            </div>

                            {/* Layout & UI Customization Section */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.appearance.layout')}</h4>
                                <div className="space-y-5">
                                    {/* UI Density */}
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.appearance.density')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.appearance.density_hint')}</p>
                                        </div>
                                        <Select
                                            value={appearance.uiDensity}
                                            onValueChange={(val) => updateAppearance('uiDensity', val as UiDensity)}
                                        >
                                            <SelectTrigger className="w-[180px] text-theme-text">
                                                <SelectValue />
                                            </SelectTrigger>
                                            <SelectContent className="bg-theme-bg-panel border-theme-border">
                                                <SelectItem value="compact" className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text">{t('settings_view.appearance.density_compact')}</SelectItem>
                                                <SelectItem value="comfortable" className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text">{t('settings_view.appearance.density_comfortable')}</SelectItem>
                                                <SelectItem value="spacious" className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text">{t('settings_view.appearance.density_spacious')}</SelectItem>
                                            </SelectContent>
                                        </Select>
                                    </div>

                                    {/* Border Radius */}
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.appearance.border_radius')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.appearance.border_radius_hint')}</p>
                                        </div>
                                        <div className="flex items-center gap-2">
                                            <svg width="24" height="24" viewBox="0 0 24 24" className="flex-shrink-0">
                                                <path
                                                    d={(() => {
                                                        const s = 24;
                                                        const r = Math.min(localBorderRadius, s / 2);
                                                        if (r <= 0) return 'M0,0H24V24H0Z';
                                                        // Squircle — continuous-curvature bezier: curve extends 1.28× radius along edge
                                                        const p = Math.min(r * 1.28, s / 2);
                                                        const cp = p * 0.64; // flatter than circle (0.552)
                                                        return [
                                                            `M${p},0`,
                                                            `L${s - p},0`,
                                                            `C${s - p + cp},0 ${s},${p - cp} ${s},${p}`,
                                                            `L${s},${s - p}`,
                                                            `C${s},${s - p + cp} ${s - p + cp},${s} ${s - p},${s}`,
                                                            `L${p},${s}`,
                                                            `C${p - cp},${s} 0,${s - p + cp} 0,${s - p}`,
                                                            `L0,${p}`,
                                                            `C0,${p - cp} ${p - cp},0 ${p},0`,
                                                            'Z'
                                                        ].join(' ');
                                                    })()}
                                                    className="fill-theme-bg-hover stroke-theme-border"
                                                    strokeWidth={1}
                                                />
                                            </svg>
                                            <input
                                                type="range"
                                                min={0}
                                                max={16}
                                                value={localBorderRadius}
                                                onChange={(e) => handleBorderRadiusChange(parseInt(e.target.value))}
                                                className="w-28 accent-orange-500"
                                            />
                                            <span className="text-xs text-theme-text-muted w-12 text-right">
                                                {localBorderRadius}{t('settings_view.appearance.border_radius_unit')}
                                            </span>
                                        </div>
                                    </div>

                                    {/* UI Font */}
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.appearance.ui_font')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.appearance.ui_font_hint')}</p>
                                        </div>
                                        <Input
                                            value={localUiFont}
                                            onChange={(e) => handleUiFontChange(e.target.value)}
                                            placeholder={t('settings_view.appearance.ui_font_placeholder')}
                                            className="w-[180px]"
                                        />
                                    </div>

                                    {/* Animation Speed */}
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.appearance.animation')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.appearance.animation_hint')}</p>
                                        </div>
                                        <Select
                                            value={appearance.animationSpeed}
                                            onValueChange={(val) => updateAppearance('animationSpeed', val as AnimationSpeed)}
                                        >
                                            <SelectTrigger className="w-[180px] text-theme-text">
                                                <SelectValue />
                                            </SelectTrigger>
                                            <SelectContent className="bg-theme-bg-panel border-theme-border">
                                                <SelectItem value="off" className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text">{t('settings_view.appearance.animation_off')}</SelectItem>
                                                <SelectItem value="reduced" className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text">{t('settings_view.appearance.animation_reduced')}</SelectItem>
                                                <SelectItem value="normal" className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text">{t('settings_view.appearance.animation_normal')}</SelectItem>
                                                <SelectItem value="fast" className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text">{t('settings_view.appearance.animation_fast')}</SelectItem>
                                            </SelectContent>
                                        </Select>
                                    </div>

                                    {/* Frosted Glass */}
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.appearance.frosted_glass')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.appearance.frosted_glass_hint')}</p>
                                        </div>
                                        <Select
                                            value={appearance.frostedGlass}
                                            onValueChange={(val) => updateAppearance('frostedGlass', val as FrostedGlassMode)}
                                        >
                                            <SelectTrigger className="w-[180px] text-theme-text">
                                                <SelectValue />
                                            </SelectTrigger>
                                            <SelectContent className="bg-theme-bg-panel border-theme-border">
                                                <SelectItem value="off" className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text">{t('settings_view.appearance.frosted_glass_off')}</SelectItem>
                                                <SelectItem value="css" className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text">{t('settings_view.appearance.frosted_glass_css')}</SelectItem>
                                                <SelectItem value="native" className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text">{t('settings_view.appearance.frosted_glass_native')}</SelectItem>
                                            </SelectContent>
                                        </Select>
                                    </div>
                                </div>
                            </div>

                            {/* Background Image Section */}
                            <BackgroundImageSection terminal={terminal} updateTerminal={updateTerminal} />

                            {/* Theme Editor Modal */}
                            <ThemeEditorModal
                                open={themeEditorOpen}
                                onOpenChange={setThemeEditorOpen}
                                editThemeId={editingThemeId}
                                baseThemeId={isCustomTheme(terminal.theme) ? undefined : terminal.theme}
                            />
                        </div>
                    )}

                    {activeTab === 'connections' && (
                        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
                            <div>
                                <h3 className="text-2xl font-medium text-theme-text mb-2">{t('settings_view.connections.title')}</h3>
                                <p className="text-theme-text-muted">{t('settings_view.connections.description')}</p>
                            </div>
                            <Separator />
                            <div className="grid grid-cols-2 gap-8 max-w-2xl">
                                <div className="grid gap-2">
                                    <Label>{t('settings_view.connections.default_username')}</Label>
                                    <Input
                                        value={connectionDefaults.username}
                                        onChange={(e) => updateConnectionDefaults('username', e.target.value)}
                                    />
                                </div>
                                <div className="grid gap-2">
                                    <Label>{t('settings_view.connections.default_port')}</Label>
                                    <Input
                                        value={connectionDefaults.port}
                                        onChange={(e) => updateConnectionDefaults('port', parseInt(e.target.value) || 22)}
                                    />
                                </div>
                            </div>

                            <div className="pt-8">
                                <h3 className="text-xl font-medium text-theme-text mb-2">{t('settings_view.connections.groups.title')}</h3>
                                <p className="text-sm text-theme-text-muted mb-4">{t('settings_view.connections.groups.description')}</p>
                                <Separator className="mb-4" />

                                <div className="flex gap-2 mb-4 max-w-md">
                                    <Input
                                        placeholder={t('settings_view.connections.groups.new_placeholder')}
                                        value={newGroup}
                                        onChange={(e) => setNewGroup(e.target.value)}
                                    />
                                    <Button onClick={handleCreateGroup} disabled={!newGroup}>
                                        <Plus className="h-4 w-4 mr-1" /> {t('settings_view.connections.groups.add')}
                                    </Button>
                                </div>

                                <div className="space-y-2 max-w-md">
                                    {groups.map(group => (
                                        <div key={group} className="flex items-center justify-between p-3 bg-theme-bg-panel rounded-md border border-theme-border">
                                            <span className="text-sm">{group}</span>
                                            <Button size="icon" variant="ghost" className="h-8 w-8 text-theme-text-muted hover:text-red-400" onClick={() => handleDeleteGroup(group)}>
                                                <Trash2 className="h-4 w-4" />
                                            </Button>
                                        </div>
                                    ))}
                                </div>
                            </div>

                            <div className="pt-8">
                                <h3 className="text-xl font-medium text-theme-text mb-2">{t('settings_view.connections.ssh_config.title')}</h3>
                                <p className="text-sm text-theme-text-muted mb-4">{t('settings_view.connections.ssh_config.description')}</p>
                                <Separator className="mb-4" />

                                <div className="h-64 overflow-y-auto border border-theme-border rounded-md bg-theme-bg-panel p-2 max-w-2xl">
                                    {sshHosts.map(host => (
                                        <div key={host.alias} className="flex items-center justify-between p-3 hover:bg-theme-bg-hover rounded-md border border-transparent hover:border-theme-border mb-1">
                                            <div className="flex flex-col">
                                                <span className="text-sm font-medium">{host.alias}</span>
                                                <span className="text-xs text-theme-text-muted">{host.user}@{host.hostname}:{host.port}</span>
                                            </div>
                                            <Button size="sm" variant="secondary" onClick={() => handleImportHost(host.alias)}>
                                                <FolderInput className="h-4 w-4 mr-1" /> {t('settings_view.connections.ssh_config.import')}
                                            </Button>
                                        </div>
                                    ))}
                                    {sshHosts.length === 0 && (
                                        <div className="text-center py-12 text-theme-text-muted text-sm">
                                            {t('settings_view.connections.ssh_config.no_hosts')}
                                        </div>
                                    )}
                                </div>
                            </div>
                        </div>
                    )}

                    {activeTab === 'ssh' && (
                        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
                            <div>
                                <h3 className="text-2xl font-medium text-theme-text mb-2">{t('settings_view.ssh_keys.title')}</h3>
                                <p className="text-theme-text-muted">{t('settings_view.ssh_keys.description')}</p>
                            </div>
                            <Separator />

                            <div className="space-y-3 max-w-3xl">
                                {keys.map(key => (
                                    <div key={key.name} className="flex items-center justify-between p-4 bg-theme-bg-panel border border-theme-border rounded-md">
                                        <div className="flex items-center gap-4">
                                            <div className="p-2 bg-theme-bg rounded-full">
                                                <Key className="h-5 w-5 text-theme-accent" />
                                            </div>
                                            <div className="flex flex-col">
                                                <span className="text-sm font-medium text-theme-text">{key.name}</span>
                                                <span className="text-xs text-theme-text-muted">{key.key_type} · {key.path}</span>
                                            </div>
                                        </div>
                                        {key.has_passphrase && (
                                            <span className="text-xs bg-yellow-900/30 text-yellow-500 px-2 py-1 rounded border border-yellow-900/50">{t('settings_view.ssh_keys.encrypted')}</span>
                                        )}
                                    </div>
                                ))}
                                {keys.length === 0 && (
                                    <div className="text-center py-12 text-theme-text-muted border border-dashed border-theme-border rounded-md">
                                        {t('settings_view.ssh_keys.no_keys')}
                                    </div>
                                )}
                            </div>
                        </div>
                    )}

                    {activeTab === 'ai' && (
                        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
                            <div>
                                <h3 className="text-2xl font-medium text-theme-text mb-2">{t('settings_view.ai.title')}</h3>
                                <p className="text-theme-text-muted">{t('settings_view.ai.description')}</p>
                            </div>
                            <Separator />

                            {/* AI Settings Section */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.general')}</h4>

                                {/* Enable Toggle - Standard Layout */}
                                <div className="flex items-center justify-between mb-6">
                                    <div>
                                        <Label className="text-theme-text">{t('settings_view.ai.enable')}</Label>
                                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.ai.enable_hint')}</p>
                                    </div>
                                    <Checkbox
                                        id="ai-enabled"
                                        checked={ai.enabled}
                                        onCheckedChange={(checked) => {
                                            if (checked && !ai.enabledConfirmed) {
                                                setShowAiConfirm(true);
                                            } else {
                                                updateAi('enabled', !!checked);
                                            }
                                        }}
                                    />
                                </div>

                                {/* Privacy Note - Integrating subtly */}
                                <div className="mb-6 p-3 rounded bg-theme-bg-panel/50 border border-zinc-800">
                                    <p className="text-xs text-theme-text-muted leading-relaxed">
                                        <span className="font-semibold text-theme-text-muted">{t('settings_view.ai.privacy_notice')}:</span> {t('settings_view.ai.privacy_text')}
                                    </p>
                                </div>

                                <Separator className="my-6 opacity-50" />

                                {/* API Configuration - Multi-Provider */}
                                <div className={ai.enabled ? "" : "opacity-50 pointer-events-none"}>
                                    <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.provider_settings')}</h4>

                                    {/* Provider Cards */}
                                    <div className="space-y-3 max-w-3xl mb-6">
                                        {ai.providers.map((provider) => (
                                            <div
                                                key={provider.id}
                                                className={cn(
                                                    "rounded-lg border p-4 transition-colors",
                                                    provider.id === ai.activeProviderId
                                                        ? "border-theme-accent/50 bg-theme-accent/5"
                                                        : "border-theme-border bg-theme-bg"
                                                )}
                                            >
                                                <div className="flex items-center justify-between mb-3">
                                                    <div className="flex items-center gap-2">
                                                        <span className="font-medium text-sm text-theme-text">{provider.name}</span>
                                                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-theme-bg-panel text-theme-text-muted uppercase tracking-wider">
                                                            {provider.type}
                                                        </span>
                                                        {provider.id === ai.activeProviderId && (
                                                            <span className="text-[10px] px-1.5 py-0.5 rounded bg-theme-accent/20 text-theme-accent font-medium">
                                                                {t('settings_view.ai.active')}
                                                            </span>
                                                        )}
                                                    </div>
                                                    <div className="flex items-center gap-1">
                                                        {provider.id !== ai.activeProviderId && (
                                                            <Button
                                                                variant="ghost"
                                                                size="sm"
                                                                className="text-xs"
                                                                onClick={() => setActiveProvider(provider.id)}
                                                            >
                                                                {t('settings_view.ai.set_active')}
                                                            </Button>
                                                        )}
                                                        <Checkbox
                                                            checked={provider.enabled}
                                                            onCheckedChange={(checked) => updateProvider(provider.id, { enabled: !!checked })}
                                                        />
                                                        {provider.id.startsWith('custom-') && (
                                                            <Button
                                                                variant="ghost"
                                                                size="sm"
                                                                className="text-red-400 hover:text-red-300 hover:bg-red-400/10 h-7 w-7 p-0"
                                                                onClick={async () => {
                                                                    if (await confirmDialog({ title: t('settings_view.ai.remove_provider_confirm', { name: provider.name }), variant: 'danger' })) {
                                                                        api.deleteAiProviderApiKey(provider.id).catch(() => { });
                                                                        removeProvider(provider.id);
                                                                    }
                                                                }}
                                                            >
                                                                <Trash2 className="h-3.5 w-3.5" />
                                                            </Button>
                                                        )}
                                                    </div>
                                                </div>

                                                <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs">
                                                    <div className="grid gap-1">
                                                        <Label className="text-xs text-theme-text-muted">{t('settings_view.ai.base_url')}</Label>
                                                        <Input
                                                            value={provider.baseUrl}
                                                            onChange={(e) => updateProvider(provider.id, { baseUrl: e.target.value })}
                                                            className="bg-theme-bg h-8 text-xs"
                                                        />
                                                    </div>
                                                    <div className="grid gap-1">
                                                        <Label className="text-xs text-theme-text-muted">{t('settings_view.ai.default_model')}</Label>
                                                        <Input
                                                            value={provider.defaultModel}
                                                            onChange={(e) => updateProvider(provider.id, { defaultModel: e.target.value })}
                                                            className="bg-theme-bg h-8 text-xs"
                                                        />
                                                    </div>
                                                </div>

                                                {/* Models list + refresh */}
                                                <div className="mt-3">
                                                    <div className="flex items-center justify-between mb-1">
                                                        <Label className="text-xs text-theme-text-muted">
                                                            {t('settings_view.ai.available_models')} ({provider.models.length})
                                                        </Label>
                                                        <Button
                                                            variant="ghost"
                                                            size="sm"
                                                            className="h-6 px-2 text-[10px] gap-1"
                                                            disabled={refreshingModels === provider.id}
                                                            onClick={async () => {
                                                                // Guard: check key before fetching models
                                                                if (provider.type !== 'ollama') {
                                                                    try {
                                                                        const hasKey = await api.hasAiProviderApiKey(provider.id);
                                                                        if (!hasKey) {
                                                                            toastError(t('ai.model_selector.no_key_warning'));
                                                                            return;
                                                                        }
                                                                    } catch { /* proceed anyway */ }
                                                                }
                                                                setRefreshingModels(provider.id);
                                                                try {
                                                                    const models = await refreshProviderModels(provider.id);
                                                                    console.log(`[Settings] Fetched ${models.length} models for ${provider.name}`);
                                                                } catch (e) {
                                                                    console.error('[Settings] Failed to refresh models:', e);
                                                                    toastError(t('settings_view.ai.refresh_failed', { error: String(e) }));
                                                                } finally {
                                                                    setRefreshingModels(null);
                                                                }
                                                            }}
                                                        >
                                                            <RefreshCw className={cn("w-3 h-3", refreshingModels === provider.id && "animate-spin")} />
                                                            {t('settings_view.ai.refresh_models')}
                                                        </Button>
                                                    </div>
                                                    {provider.models.length > 0 && (
                                                        <div className="flex flex-wrap gap-1">
                                                            {provider.models.slice(0, 12).map((model) => (
                                                                <span
                                                                    key={model}
                                                                    className="text-[10px] px-1.5 py-0.5 rounded border border-theme-border/50 bg-theme-bg text-theme-text-muted cursor-pointer hover:text-theme-text hover:border-theme-border transition-colors"
                                                                    onClick={() => updateProvider(provider.id, { defaultModel: model })}
                                                                    title={t('settings_view.ai.click_to_set_default')}
                                                                >
                                                                    {model}
                                                                </span>
                                                            ))}
                                                            {provider.models.length > 12 && (
                                                                <span className="text-[10px] px-1.5 py-0.5 text-theme-text-muted">
                                                                    +{provider.models.length - 12}
                                                                </span>
                                                            )}
                                                        </div>
                                                    )}
                                                </div>

                                                {/* Provider API Key */}
                                                {provider.type !== 'ollama' && (
                                                    <div className="mt-3">
                                                        <ProviderKeyInput providerId={provider.id} />
                                                    </div>
                                                )}
                                            </div>
                                        ))}
                                    </div>

                                    {/* Add Custom Provider */}
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        className="mb-6"
                                        onClick={() => {
                                            const id = `custom-${Date.now()}`;
                                            addProvider({
                                                id,
                                                type: 'openai_compatible',
                                                name: t('settings_view.ai.custom_provider'),
                                                baseUrl: 'https://',
                                                defaultModel: '',
                                                models: [],
                                                enabled: true,
                                                createdAt: Date.now(),
                                            });
                                        }}
                                    >
                                        + {t('settings_view.ai.add_provider')}
                                    </Button>

                                    <Separator className="my-6 opacity-50" />

                                    <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.context_controls')}</h4>
                                    <div className="grid grid-cols-1 md:grid-cols-2 gap-6 max-w-3xl">
                                        <div className="grid gap-2">
                                            <Label>{t('settings_view.ai.max_context')}</Label>
                                            <Select
                                                value={ai.contextMaxChars.toString()}
                                                onValueChange={(v) => updateAi('contextMaxChars', parseInt(v))}
                                            >
                                                <SelectTrigger className="bg-theme-bg">
                                                    <SelectValue />
                                                </SelectTrigger>
                                                <SelectContent>
                                                    <SelectItem value="2000">{t('settings_view.ai.chars_2000')}</SelectItem>
                                                    <SelectItem value="4000">{t('settings_view.ai.chars_4000')}</SelectItem>
                                                    <SelectItem value="8000">{t('settings_view.ai.chars_8000')}</SelectItem>
                                                    <SelectItem value="16000">{t('settings_view.ai.chars_16000')}</SelectItem>
                                                    <SelectItem value="32000">{t('settings_view.ai.chars_32000')}</SelectItem>
                                                </SelectContent>
                                            </Select>
                                            <p className="text-xs text-theme-text-muted">{t('settings_view.ai.max_context_hint')}</p>
                                        </div>
                                        <div className="grid gap-2">
                                            <Label>{t('settings_view.ai.buffer_history')}</Label>
                                            <Select
                                                value={ai.contextVisibleLines.toString()}
                                                onValueChange={(v) => updateAi('contextVisibleLines', parseInt(v))}
                                            >
                                                <SelectTrigger className="bg-theme-bg">
                                                    <SelectValue />
                                                </SelectTrigger>
                                                <SelectContent>
                                                    <SelectItem value="50">{t('settings_view.ai.lines_50')}</SelectItem>
                                                    <SelectItem value="100">{t('settings_view.ai.lines_100')}</SelectItem>
                                                    <SelectItem value="200">{t('settings_view.ai.lines_200')}</SelectItem>
                                                    <SelectItem value="400">{t('settings_view.ai.lines_400')}</SelectItem>
                                                </SelectContent>
                                            </Select>
                                            <p className="text-xs text-theme-text-muted">{t('settings_view.ai.buffer_history_hint')}</p>
                                        </div>
                                    </div>

                                    <Separator className="my-6 opacity-50" />

                                    <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.system_prompt_title')}</h4>
                                    <div className="max-w-3xl grid gap-2">
                                        <Label>{t('settings_view.ai.custom_system_prompt')}</Label>
                                        <textarea
                                            value={ai.customSystemPrompt || ''}
                                            onChange={(e) => updateAi('customSystemPrompt', e.target.value)}
                                            placeholder={t('settings_view.ai.system_prompt_placeholder')}
                                            rows={4}
                                            className="w-full bg-theme-bg border border-theme-border rounded-md px-3 py-2 text-sm text-theme-text placeholder-theme-text-muted/40 resize-y min-h-[80px] max-h-[200px] focus:outline-none focus:ring-1 focus:ring-theme-accent/40"
                                        />
                                        <p className="text-xs text-theme-text-muted">{t('settings_view.ai.system_prompt_hint')}</p>
                                    </div>

                                    <Separator className="my-6 opacity-50" />

                                    <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.max_response_tokens')}</h4>
                                    <div className="max-w-3xl grid gap-2">
                                        <p className="text-xs text-theme-text-muted mb-2">{t('settings_view.ai.max_response_tokens_hint')}</p>
                                        {ai.activeProviderId && ai.activeModel && (
                                          <div className="flex items-center gap-3">
                                            <Label className="shrink-0 text-xs">{ai.activeModel}:</Label>
                                            <input
                                              type="number"
                                              min={256}
                                              max={65536}
                                              step={256}
                                              value={ai.modelMaxResponseTokens?.[ai.activeProviderId]?.[ai.activeModel] ?? ''}
                                              placeholder="Auto"
                                              onChange={(e) => {
                                                const val = e.target.value ? parseInt(e.target.value, 10) : undefined;
                                                const existing = ai.modelMaxResponseTokens ?? {};
                                                const providerOverrides = existing[ai.activeProviderId!] ?? {};
                                                const updated = { ...existing, [ai.activeProviderId!]: { ...providerOverrides } };
                                                if (val && val >= 256) {
                                                  updated[ai.activeProviderId!][ai.activeModel!] = val;
                                                } else {
                                                  delete updated[ai.activeProviderId!][ai.activeModel!];
                                                }
                                                updateAi('modelMaxResponseTokens', updated);
                                              }}
                                              className="w-32 bg-theme-bg border border-theme-border rounded-md px-2 py-1 text-sm text-theme-text placeholder-theme-text-muted/40 focus:outline-none focus:ring-1 focus:ring-theme-accent/40"
                                            />
                                          </div>
                                        )}
                                    </div>
                                </div>

                                <Separator className="my-6 opacity-50" />

                                {/* Tool Use Settings */}
                                <div className={ai.enabled ? "" : "opacity-50 pointer-events-none"}>
                                    <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider flex items-center gap-2">
                                        <Wrench className="w-4 h-4" />
                                        {t('settings_view.ai.tool_use')}
                                    </h4>

                                    {/* Enable Tool Use */}
                                    <div className="flex items-center justify-between mb-4">
                                        <div>
                                            <Label className="text-theme-text">{t('settings_view.ai.tool_use_enabled')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.ai.tool_use_enabled_hint')}</p>
                                        </div>
                                        <Checkbox
                                            id="tool-use-enabled"
                                            checked={ai.toolUse?.enabled ?? false}
                                            onCheckedChange={(checked) => {
                                                updateAi('toolUse', { ...(ai.toolUse ?? { enabled: false, autoApproveTools: {}, disabledTools: [] }), enabled: !!checked });
                                            }}
                                        />
                                    </div>

                                    {/* Per-tool approval — only visible when tool use enabled */}
                                    <div className={ai.toolUse?.enabled ? "space-y-5 ml-4 pl-4 border-l border-theme-border/30" : "opacity-40 pointer-events-none space-y-5 ml-4 pl-4 border-l border-theme-border/30"}>
                                        <p className="text-xs text-theme-text-muted">{t('settings_view.ai.tool_use_approve_hint')}</p>

                                        {/* Approve All / Revoke All buttons */}
                                        <div className="flex gap-2">
                                            <button
                                                type="button"
                                                onClick={() => {
                                                    const all: Record<string, boolean> = {};
                                                    for (const g of TOOL_GROUPS) {
                                                        for (const name of [...g.readOnly, ...g.write]) all[name] = true;
                                                    }
                                                    updateAi('toolUse', { ...(ai.toolUse ?? { enabled: false, autoApproveTools: {}, disabledTools: [] }), autoApproveTools: all });
                                                }}
                                                className="text-xs px-3 py-1 rounded border border-theme-border text-theme-text-muted hover:bg-theme-bg-hover/50 transition-colors cursor-pointer"
                                            >
                                                {t('settings_view.ai.tool_use_approve_all')}
                                            </button>
                                            <button
                                                type="button"
                                                onClick={() => {
                                                    const none: Record<string, boolean> = {};
                                                    for (const g of TOOL_GROUPS) {
                                                        for (const name of [...g.readOnly, ...g.write]) none[name] = false;
                                                    }
                                                    updateAi('toolUse', { ...(ai.toolUse ?? { enabled: false, autoApproveTools: {}, disabledTools: [] }), autoApproveTools: none });
                                                }}
                                                className="text-xs px-3 py-1 rounded border border-theme-border text-theme-text-muted hover:bg-theme-bg-hover/50 transition-colors cursor-pointer"
                                            >
                                                {t('settings_view.ai.tool_use_approve_none')}
                                            </button>
                                        </div>

                                        {/* Tool groups */}
                                        {TOOL_GROUPS.map((group) => {
                                            const GroupIcon = TOOL_GROUP_ICONS[group.groupKey] ?? Wrench;
                                            const approveTools = ai.toolUse?.autoApproveTools ?? {};
                                            const toggleTool = (toolName: string) => {
                                                const next = { ...approveTools, [toolName]: !approveTools[toolName] };
                                                updateAi('toolUse', { ...(ai.toolUse ?? { enabled: false, autoApproveTools: {}, disabledTools: [] }), autoApproveTools: next });
                                            };
                                            const renderToolButton = (toolName: string) => {
                                                const Icon = TOOL_ICON_MAP[toolName] ?? Wrench;
                                                const checked = approveTools[toolName] === true;
                                                const isWrite = WRITE_TOOLS.has(toolName);
                                                return (
                                                    <button
                                                        key={toolName}
                                                        type="button"
                                                        aria-pressed={checked}
                                                        onClick={() => toggleTool(toolName)}
                                                        className={cn(
                                                            "flex items-center gap-2 rounded-md border px-3 py-2 text-xs transition-colors cursor-pointer select-none",
                                                            checked
                                                                ? isWrite
                                                                    ? "border-amber-500/60 bg-amber-500/10 text-amber-400"
                                                                    : "border-theme-accent/60 bg-theme-accent/10 text-theme-accent"
                                                                : "border-theme-border bg-theme-bg-panel/30 text-theme-text-muted hover:border-theme-border hover:bg-theme-bg-hover/50"
                                                        )}
                                                    >
                                                        <Icon className="size-3.5 shrink-0" />
                                                        <span className="truncate">{t(`tool_use.tool_names.${toolName}`, { defaultValue: toolName })}</span>
                                                    </button>
                                                );
                                            };
                                            return (
                                                <div key={group.groupKey}>
                                                    <div className="flex items-center gap-1.5 mb-2">
                                                        <GroupIcon className="size-3.5 text-theme-text-muted" />
                                                        <span className="text-xs font-medium text-theme-text uppercase tracking-wider">
                                                            {t(`settings_view.ai.tool_use_group_${group.groupKey}`)}
                                                        </span>
                                                    </div>
                                                    {group.readOnly.length > 0 && (
                                                        <div className="mb-2">
                                                            <span className="text-[10px] text-theme-text-muted/60 uppercase tracking-widest">
                                                                {t('settings_view.ai.tool_use_subgroup_read_only')}
                                                            </span>
                                                            <div className="grid grid-cols-3 gap-1.5 mt-1">
                                                                {group.readOnly.map(renderToolButton)}
                                                            </div>
                                                        </div>
                                                    )}
                                                    {group.write.length > 0 && (
                                                        <div>
                                                            <span className="text-[10px] text-amber-400/70 uppercase tracking-widest">
                                                                {t('settings_view.ai.tool_use_subgroup_write')}
                                                            </span>
                                                            <div className="grid grid-cols-3 gap-1.5 mt-1">
                                                                {group.write.map(renderToolButton)}
                                                            </div>
                                                        </div>
                                                    )}
                                                </div>
                                            );
                                        })}

                                        {/* Write tools warning */}
                                        {(() => {
                                            const approveTools = ai.toolUse?.autoApproveTools ?? {};
                                            const anyWriteApproved = [...WRITE_TOOLS].some(name => approveTools[name] === true);
                                            return anyWriteApproved ? (
                                                <div className="p-3 rounded bg-amber-500/10 border border-amber-500/20">
                                                    <p className="text-xs text-amber-400 leading-relaxed">
                                                        <span className="font-semibold">⚠</span> {t('settings_view.ai.tool_use_write_warning')}
                                                    </p>
                                                </div>
                                            ) : null;
                                        })()}
                                    </div>
                                </div>
                            </div>
                        </div>
                    )}

                    {activeTab === 'local' && (
                        <LocalTerminalSettings />
                    )}

                    {activeTab === 'reconnect' && (
                        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
                            <div>
                                <h3 className="text-2xl font-medium text-theme-text mb-2">{t('settings_view.reconnect.title')}</h3>
                                <p className="text-theme-text-muted">{t('settings_view.reconnect.description')}</p>
                            </div>
                            <Separator />

                            {/* Auto-reconnect toggle */}
                            <div className="flex items-center justify-between max-w-2xl">
                                <div className="grid gap-1">
                                    <Label>{t('settings_view.reconnect.enabled')}</Label>
                                    <p className="text-xs text-theme-text-muted">{t('settings_view.reconnect.enabled_hint')}</p>
                                </div>
                                <Checkbox
                                    checked={reconnect?.enabled ?? true}
                                    onCheckedChange={(checked) => updateReconnect('enabled', !!checked)}
                                />
                            </div>

                            <Separator />

                            {/* Retry strategy settings */}
                            <div className={cn('space-y-6 transition-opacity', !(reconnect?.enabled ?? true) && 'opacity-40 pointer-events-none')}>
                                <h4 className="text-lg font-medium text-theme-text">{t('settings_view.reconnect.strategy')}</h4>

                                <div className="grid grid-cols-2 gap-8 max-w-2xl">
                                    {/* Max attempts */}
                                    <div className="grid gap-2">
                                        <Label>{t('settings_view.reconnect.max_attempts')}</Label>
                                        <p className="text-xs text-theme-text-muted">{t('settings_view.reconnect.max_attempts_hint')}</p>
                                        <Select
                                            value={String(reconnect?.maxAttempts ?? 5)}
                                            onValueChange={(v) => updateReconnect('maxAttempts', parseInt(v))}
                                        >
                                            <SelectTrigger className="w-full">
                                                <SelectValue />
                                            </SelectTrigger>
                                            <SelectContent>
                                                {[1, 2, 3, 5, 8, 10, 15, 20].map((n) => (
                                                    <SelectItem key={n} value={String(n)}>{n}</SelectItem>
                                                ))}
                                            </SelectContent>
                                        </Select>
                                    </div>

                                    {/* Base delay */}
                                    <div className="grid gap-2">
                                        <Label>{t('settings_view.reconnect.base_delay')}</Label>
                                        <p className="text-xs text-theme-text-muted">{t('settings_view.reconnect.base_delay_hint')}</p>
                                        <Select
                                            value={String(reconnect?.baseDelayMs ?? 1000)}
                                            onValueChange={(v) => updateReconnect('baseDelayMs', parseInt(v))}
                                        >
                                            <SelectTrigger className="w-full">
                                                <SelectValue />
                                            </SelectTrigger>
                                            <SelectContent>
                                                {[
                                                    { v: 500, l: '0.5s' },
                                                    { v: 1000, l: '1s' },
                                                    { v: 2000, l: '2s' },
                                                    { v: 3000, l: '3s' },
                                                    { v: 5000, l: '5s' },
                                                    { v: 10000, l: '10s' },
                                                ].map(({ v, l }) => (
                                                    <SelectItem key={v} value={String(v)}>{l}</SelectItem>
                                                ))}
                                            </SelectContent>
                                        </Select>
                                    </div>
                                </div>

                                <div className="grid grid-cols-2 gap-8 max-w-2xl">
                                    {/* Max delay */}
                                    <div className="grid gap-2">
                                        <Label>{t('settings_view.reconnect.max_delay')}</Label>
                                        <p className="text-xs text-theme-text-muted">{t('settings_view.reconnect.max_delay_hint')}</p>
                                        <Select
                                            value={String(reconnect?.maxDelayMs ?? 15000)}
                                            onValueChange={(v) => updateReconnect('maxDelayMs', parseInt(v))}
                                        >
                                            <SelectTrigger className="w-full">
                                                <SelectValue />
                                            </SelectTrigger>
                                            <SelectContent>
                                                {[
                                                    { v: 5000, l: '5s' },
                                                    { v: 10000, l: '10s' },
                                                    { v: 15000, l: '15s' },
                                                    { v: 30000, l: '30s' },
                                                    { v: 60000, l: '60s' },
                                                ].map(({ v, l }) => (
                                                    <SelectItem key={v} value={String(v)}>{l}</SelectItem>
                                                ))}
                                            </SelectContent>
                                        </Select>
                                    </div>
                                </div>

                                {/* Backoff formula explanation */}
                                <div className="p-4 bg-theme-bg-panel/50 border border-theme-border/50 rounded-md max-w-2xl">
                                    <p className="text-xs text-theme-text-muted leading-relaxed">
                                        {t('settings_view.reconnect.formula_hint')}
                                    </p>
                                </div>
                            </div>
                        </div>
                    )}

                    {activeTab === 'help' && (
                        <HelpAboutSection />
                    )}

                    {activeTab === 'sftp' && (
                        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
                            <div>
                                <h3 className="text-2xl font-medium text-theme-text mb-2">{t('settings_view.sftp.title')}</h3>
                                <p className="text-theme-text-muted">{t('settings_view.sftp.description')}</p>
                            </div>
                            <Separator />

                            {/* Concurrent Transfers */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <div className="flex items-center justify-between mb-2">
                                    <div>
                                        <Label className="text-theme-text">{t('settings_view.sftp.concurrent')}</Label>
                                        <p className="text-xs text-theme-text-muted mt-0.5">
                                            {t('settings_view.sftp.concurrent_hint')}
                                        </p>
                                    </div>
                                    <Select
                                        value={(sftp?.maxConcurrentTransfers ?? 3).toString()}
                                        onValueChange={(v) => updateSftp('maxConcurrentTransfers', parseInt(v))}
                                    >
                                        <SelectTrigger className="w-[180px]">
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                            {[1, 2, 3, 4, 5, 6, 8, 10].map(num => (
                                                <SelectItem key={num} value={num.toString()}>
                                                    {t('settings_view.sftp.transfer_count', { count: num })}
                                                </SelectItem>
                                            ))}
                                        </SelectContent>
                                    </Select>
                                </div>
                            </div>

                            {/* Bandwidth Limit */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <div className="space-y-4">
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label htmlFor="speed-limit-enabled" className="text-theme-text">{t('settings_view.sftp.bandwidth')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.sftp.bandwidth_hint')}</p>
                                        </div>
                                        <Checkbox
                                            id="speed-limit-enabled"
                                            checked={sftp?.speedLimitEnabled ?? false}
                                            onCheckedChange={(checked) => updateSftp('speedLimitEnabled', !!checked)}
                                        />
                                    </div>

                                    {sftp?.speedLimitEnabled && (
                                        <div className="pt-2 flex items-center justify-between animate-in fade-in slide-in-from-top-1 duration-200">
                                            <div>
                                                <Label className="text-theme-text text-sm">{t('settings_view.sftp.speed_limit')}</Label>
                                            </div>
                                            <Input
                                                type="number"
                                                className="w-[180px]"
                                                value={sftp?.speedLimitKBps ?? 0}
                                                onChange={(e) => {
                                                    const value = parseInt(e.target.value) || 0;
                                                    updateSftp('speedLimitKBps', Math.max(0, value));
                                                }}
                                                min={0}
                                                step={100}
                                                placeholder="0 = unlimited"
                                            />
                                        </div>
                                    )}
                                </div>
                            </div>

                            {/* Conflict Resolution */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <div className="flex items-center justify-between mb-2">
                                    <div>
                                        <Label className="text-theme-text">{t('settings_view.sftp.conflict')}</Label>
                                        <p className="text-xs text-theme-text-muted mt-0.5">
                                            {t('settings_view.sftp.conflict_hint')}
                                        </p>
                                    </div>
                                    <Select
                                        value={sftp?.conflictAction ?? 'ask'}
                                        onValueChange={(v) => updateSftp('conflictAction', v as 'ask' | 'overwrite' | 'skip' | 'rename')}
                                    >
                                        <SelectTrigger className="w-[180px]">
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                            <SelectItem value="ask">{t('settings_view.sftp.conflict_ask')}</SelectItem>
                                            <SelectItem value="overwrite">{t('settings_view.sftp.conflict_overwrite')}</SelectItem>
                                            <SelectItem value="skip">{t('settings_view.sftp.conflict_skip')}</SelectItem>
                                            <SelectItem value="rename">{t('settings_view.sftp.conflict_rename')}</SelectItem>
                                        </SelectContent>
                                    </Select>
                                </div>
                            </div>
                        </div>
                    )}

                    {activeTab === 'ide' && (
                        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
                            <div>
                                <h3 className="text-2xl font-medium text-theme-text mb-2">{t('settings_view.ide.title', 'IDE Mode (Mini)')}</h3>
                                <p className="text-theme-text-muted">{t('settings_view.ide.description', 'Configure the built-in code editor behavior.')}</p>
                            </div>
                            <Separator />

                            {/* Auto-save */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <div className="flex items-center justify-between">
                                    <div>
                                        <Label className="text-theme-text">{t('settings_view.ide.auto_save', 'Auto Save')}</Label>
                                        <p className="text-xs text-theme-text-muted mt-0.5">
                                            {t('settings_view.ide.auto_save_hint', 'Automatically save files when switching tabs or losing focus.')}
                                        </p>
                                    </div>
                                    <Checkbox
                                        checked={ide?.autoSave ?? false}
                                        onCheckedChange={(checked) => updateIde('autoSave', checked === true)}
                                    />
                                </div>
                            </div>

                            {/* Editor Font & Spacing */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5 space-y-4">
                                <h4 className="text-sm font-medium text-theme-text uppercase tracking-wider">
                                    {t('settings_view.ide.editor_typography', 'Editor Typography')}
                                </h4>
                                <p className="text-xs text-theme-text-muted">
                                    {t('settings_view.ide.editor_typography_hint', 'Override terminal font size and line height for the code editor. Leave at "Follow Terminal" to use terminal settings.')}
                                </p>

                                {/* Font Size */}
                                <div className="flex items-center justify-between">
                                    <div>
                                        <Label className="text-theme-text">{t('settings_view.ide.font_size', 'Font Size')}</Label>
                                        <p className="text-xs text-theme-text-muted mt-0.5">
                                            {t('settings_view.ide.font_size_hint', 'Editor font size in pixels. Empty = follow terminal.')}
                                        </p>
                                    </div>
                                    <div className="flex items-center gap-2">
                                        <Input
                                            type="number"
                                            min="8"
                                            max="32"
                                            step="1"
                                            value={ide?.fontSize ?? ''}
                                            placeholder={String(terminal.fontSize)}
                                            onChange={(e) => {
                                                const v = e.target.value;
                                                updateIde('fontSize', v === '' ? null : Math.min(32, Math.max(8, parseInt(v) || 14)));
                                            }}
                                            className="w-20"
                                        />
                                        <span className="text-xs text-theme-text-muted">px</span>
                                    </div>
                                </div>

                                <Separator className="opacity-50" />

                                {/* Line Height */}
                                <div className="flex items-center justify-between">
                                    <div>
                                        <Label className="text-theme-text">{t('settings_view.ide.line_height', 'Line Height')}</Label>
                                        <p className="text-xs text-theme-text-muted mt-0.5">
                                            {t('settings_view.ide.line_height_hint', 'Editor line spacing. Empty = follow terminal.')}
                                        </p>
                                    </div>
                                    <Input
                                        type="number"
                                        step="0.1"
                                        min="0.8"
                                        max="3"
                                        value={ide?.lineHeight ?? ''}
                                        placeholder={String(terminal.lineHeight)}
                                        onChange={(e) => {
                                            const v = e.target.value;
                                            updateIde('lineHeight', v === '' ? null : Math.min(3, Math.max(0.8, parseFloat(v) || 1.2)));
                                        }}
                                        className="w-20"
                                    />
                                </div>
                            </div>

                            {/* Remote Agent */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5 space-y-4">
                                <h4 className="text-sm font-medium text-theme-text uppercase tracking-wider">
                                    {t('settings_view.ide.agent_title', 'Remote Agent')}
                                </h4>
                                <p className="text-xs text-theme-text-muted">
                                    {t('settings_view.ide.agent_description', 'OxideTerm can deploy a lightweight agent binary to remote hosts for enhanced IDE performance. The agent provides POSIX-native file operations, real-time file watching, and faster search — all running locally on the remote server.')}
                                </p>
                                <div className="space-y-3 text-xs">
                                    <div className="flex items-start gap-2 text-theme-text-muted">
                                        <div className="w-1 h-1 rounded-full bg-emerald-400 mt-1.5 shrink-0" />
                                        <span>{t('settings_view.ide.agent_feature_atomic', 'Atomic file writes (no data loss on network disruption)')}</span>
                                    </div>
                                    <div className="flex items-start gap-2 text-theme-text-muted">
                                        <div className="w-1 h-1 rounded-full bg-emerald-400 mt-1.5 shrink-0" />
                                        <span>{t('settings_view.ide.agent_feature_watch', 'Real-time file watching via inotify (instant refresh)')}</span>
                                    </div>
                                    <div className="flex items-start gap-2 text-theme-text-muted">
                                        <div className="w-1 h-1 rounded-full bg-emerald-400 mt-1.5 shrink-0" />
                                        <span>{t('settings_view.ide.agent_feature_hash', 'Hash-based conflict detection (prevents overwriting external changes)')}</span>
                                    </div>
                                    <div className="flex items-start gap-2 text-theme-text-muted">
                                        <div className="w-1 h-1 rounded-full bg-emerald-400 mt-1.5 shrink-0" />
                                        <span>{t('settings_view.ide.agent_feature_search', 'Server-side grep and deep directory tree loading')}</span>
                                    </div>
                                </div>
                                <div className="pt-2 border-t border-theme-border/50">
                                    <div className="flex items-center justify-between">
                                        <div>
                                            <Label className="text-theme-text text-xs">{t('settings_view.ide.agent_supported', 'Supported Architectures')}</Label>
                                            <p className="text-xs text-theme-text-muted mt-0.5">x86_64, aarch64 (Linux)</p>
                                        </div>
                                        <span className="text-xs text-theme-text-muted bg-theme-bg-panel px-2 py-1 rounded border border-theme-border/50">
                                            ~1 MB
                                        </span>
                                    </div>
                                </div>
                                <p className="text-xs text-theme-text-muted italic">
                                    {t('settings_view.ide.agent_auto_hint', 'The agent is deployed automatically when opening IDE mode on a supported Linux host. No manual configuration needed. Unsupported architectures fall back to SFTP seamlessly.')}
                                </p>

                                <Separator className="opacity-50" />

                                {/* Agent Mode */}
                                <div className="flex items-center justify-between">
                                    <div>
                                        <Label className="text-theme-text">{t('settings_view.ide.agent_mode_label', 'Agent Deploy Policy')}</Label>
                                        <p className="text-xs text-theme-text-muted mt-0.5">
                                            {t('settings_view.ide.agent_mode_hint', 'Control whether the agent is deployed to remote hosts.')}
                                        </p>
                                    </div>
                                    <Select
                                        value={ide?.agentMode ?? 'ask'}
                                        onValueChange={(value) => updateIde('agentMode', value as 'ask' | 'enabled' | 'disabled')}
                                    >
                                        <SelectTrigger className="w-40">
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                            <SelectItem value="ask">{t('settings_view.ide.agent_mode_ask', 'Ask Every Time')}</SelectItem>
                                            <SelectItem value="enabled">{t('settings_view.ide.agent_mode_enabled', 'Always Enable')}</SelectItem>
                                            <SelectItem value="disabled">{t('settings_view.ide.agent_mode_disabled', 'SFTP Only')}</SelectItem>
                                        </SelectContent>
                                    </Select>
                                </div>
                            </div>

                            {/* Agent Transparency & Privacy */}
                            <div className="rounded-lg border border-blue-500/20 bg-blue-500/5 p-5 space-y-3">
                                <h4 className="text-sm font-medium text-theme-text flex items-center gap-2">
                                    <Shield className="h-4 w-4 text-blue-400" />
                                    {t('settings_view.ide.agent_transparency_title', 'Transparency & Privacy')}
                                </h4>
                                <div className="space-y-2.5 text-xs text-theme-text-muted">
                                    <div className="flex items-start gap-2">
                                        <div className="w-1 h-1 rounded-full bg-blue-400 mt-1.5 shrink-0" />
                                        <span>
                                            <span className="text-theme-text font-medium">{t('settings_view.ide.agent_path_label', 'Deploy Path')}:</span>{' '}
                                            {t('settings_view.ide.agent_path_detail', 'The agent binary is placed at ~/.oxideterm/oxideterm-agent in the remote user\'s home directory.')}
                                        </span>
                                    </div>
                                    <div className="flex items-start gap-2">
                                        <div className="w-1 h-1 rounded-full bg-blue-400 mt-1.5 shrink-0" />
                                        <span>
                                            <span className="text-theme-text font-medium">{t('settings_view.ide.agent_lifecycle_label', 'Lifecycle')}:</span>{' '}
                                            {t('settings_view.ide.agent_lifecycle_detail', 'Deployed on first IDE mode open and persists between sessions. Automatically updated when a new version is available. Can be safely deleted at any time — it will be re-deployed when needed.')}
                                        </span>
                                    </div>
                                    <div className="flex items-start gap-2">
                                        <div className="w-1 h-1 rounded-full bg-blue-400 mt-1.5 shrink-0" />
                                        <span>
                                            <span className="text-theme-text font-medium">{t('settings_view.ide.agent_privacy_label', 'Privacy')}:</span>{' '}
                                            {t('settings_view.ide.agent_privacy_detail', 'The agent is a standalone binary that communicates exclusively with OxideTerm over the existing SSH connection (stdio). It makes no third-party network connections, sends no telemetry, and collects no data.')}
                                        </span>
                                    </div>
                                </div>
                            </div>
                        </div>
                    )}
                </div>
            </div>

            {/* AI Enable Confirmation Dialog */}
            <Dialog open={showAiConfirm} onOpenChange={setShowAiConfirm}>
                <DialogContent className="max-w-md">
                    <DialogHeader>
                        <DialogTitle>{t('settings_view.ai_confirm.title')}</DialogTitle>
                        <DialogDescription>
                            {t('settings_view.ai_confirm.description')}
                        </DialogDescription>
                    </DialogHeader>

                    <div className="p-4 space-y-4">
                        <p className="text-sm text-theme-text">
                            {t('settings_view.ai_confirm.intro')}
                        </p>
                        <div className="space-y-2 text-xs text-theme-text-muted bg-theme-bg-panel/30 p-3 rounded border border-theme-border/50">
                            <div className="flex items-start gap-2">
                                <div className="w-1 h-1 rounded-full bg-zinc-500 mt-1.5 shrink-0"></div>
                                <p>{t('settings_view.ai_confirm.point_local')}</p>
                            </div>
                            <div className="flex items-start gap-2">
                                <div className="w-1 h-1 rounded-full bg-zinc-500 mt-1.5 shrink-0"></div>
                                <p>{t('settings_view.ai_confirm.point_no_server')}</p>
                            </div>
                            <div className="flex items-start gap-2">
                                <div className="w-1 h-1 rounded-full bg-zinc-500 mt-1.5 shrink-0"></div>
                                <p>{t('settings_view.ai_confirm.point_context')}</p>
                            </div>
                        </div>
                    </div>

                    <DialogFooter>
                        <Button variant="ghost" onClick={() => setShowAiConfirm(false)}>{t('settings_view.ai_confirm.cancel')}</Button>
                        <Button
                            onClick={() => {
                                updateAi('enabled', true);
                                updateAi('enabledConfirmed', true);
                                setShowAiConfirm(false);
                            }}
                        >
                            {t('settings_view.ai_confirm.enable')}
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
            {ConfirmDialog}
        </div>
    );
};
