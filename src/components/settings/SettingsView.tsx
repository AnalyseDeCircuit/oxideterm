import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { getVersion } from '@tauri-apps/api/app';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { open as openFileDialog } from '@tauri-apps/plugin-dialog';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore, type RendererType, type FontFamily, type CursorStyle, type Language, type BackgroundFit } from '../../store/settingsStore';
import { useTabBgActive } from '../../hooks/useTabBackground';
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
import { Monitor, Key, Terminal as TerminalIcon, Shield, Plus, Trash2, FolderInput, Sparkles, Square, HardDrive, HelpCircle, Github, ExternalLink, Keyboard, RefreshCw, ImageIcon, X } from 'lucide-react';
import { api } from '../../lib/api';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { SshKeyInfo, SshHostInfo } from '../../types';
import { themes } from '../../lib/themes';
import { platform } from '../../lib/platform';
import { cn } from '../../lib/utils';

const formatThemeName = (key: string) => {
    return key.split('-')
        .map(word => word.charAt(0).toUpperCase() + word.slice(1))
        .join(' ');
};

const ThemePreview = ({ themeName }: { themeName: string }) => {
    const theme = themes[themeName] || themes.default;

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
                                if (confirm(t('settings_view.ai.remove_confirm'))) {
                                    try {
                                        await api.deleteAiProviderApiKey(providerId);
                                        setHasKey(false);
                                        window.dispatchEvent(new CustomEvent('ai-api-key-updated'));
                                    } catch (e) {
                                        alert(t('settings_view.ai.remove_failed', { error: e }));
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
                                    alert(t('settings_view.ai.save_failed', { error: e }));
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
                        <kbd className="px-2 py-1 bg-theme-bg-hover rounded text-xs text-theme-text-muted border border-theme-border">⌘T</kbd>
                    </div>
                    <Separator className="opacity-30" />
                    <div className="flex items-center justify-between py-2">
                        <span className="text-theme-text">{t('settings_view.local_terminal.new_shell_launcher')}</span>
                        <kbd className="px-2 py-1 bg-theme-bg-hover rounded text-xs text-theme-text-muted border border-theme-border">⌘⇧T</kbd>
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
                                    <Square className="h-4 w-4 text-theme-text-muted" />
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

    useEffect(() => {
        getVersion().then(setAppVersion).catch(() => setAppVersion('1.4.0'));
    }, []);

    const isMac = platform.isMac;
    
    // Define shortcuts by category with platform-specific keys
    const shortcutCategories = [
        {
            title: t('settings_view.help.category_app'),
            shortcuts: [
                { label: t('settings_view.help.shortcut_new_tab'), mac: '⌘T', other: 'Ctrl+T' },
                { label: t('settings_view.help.shortcut_shell_launcher'), mac: '⌘⇧T', other: 'Ctrl+Shift+T' },
                { label: t('settings_view.help.shortcut_next_tab'), mac: '⌘}', other: 'Ctrl+Tab' },
                { label: t('settings_view.help.shortcut_prev_tab'), mac: '⌘{', other: 'Ctrl+Shift+Tab' },
            ],
        },
        {
            title: t('settings_view.help.category_terminal'),
            shortcuts: [
                { label: t('settings_view.help.shortcut_find'), mac: '⌘F', other: 'Ctrl+Shift+F' },
                { label: t('settings_view.help.shortcut_ai_panel'), mac: '⌘I', other: 'Ctrl+Shift+I' },
                { label: t('settings_view.help.shortcut_close_panel'), mac: 'Esc', other: 'Esc' },
            ],
        },
        {
            title: t('settings_view.help.category_split'),
            shortcuts: [
                { label: t('settings_view.help.shortcut_split_h'), mac: '⌘⇧E', other: 'Ctrl+Shift+E' },
                { label: t('settings_view.help.shortcut_split_v'), mac: '⌘⇧D', other: 'Ctrl+Shift+D' },
                { label: t('settings_view.help.shortcut_close_pane'), mac: '⌘⇧W', other: 'Ctrl+Shift+W' },
                { label: t('settings_view.help.shortcut_nav_pane'), mac: '⌘⌥Arrow', other: 'Ctrl+Alt+Arrow' },
            ],
        },
        {
            title: t('settings_view.help.category_file_manager'),
            shortcuts: [
                { label: t('settings_view.help.shortcut_select_all'), mac: '⌘A', other: 'Ctrl+A' },
                { label: t('settings_view.help.shortcut_copy'), mac: '⌘C', other: 'Ctrl+C' },
                { label: t('settings_view.help.shortcut_cut'), mac: '⌘X', other: 'Ctrl+X' },
                { label: t('settings_view.help.shortcut_paste'), mac: '⌘V', other: 'Ctrl+V' },
                { label: t('settings_view.help.shortcut_rename'), mac: 'F2', other: 'F2' },
                { label: t('settings_view.help.shortcut_delete'), mac: 'Delete', other: 'Delete' },
                { label: t('settings_view.help.shortcut_quick_look'), mac: 'Space', other: 'Space' },
                { label: t('settings_view.help.shortcut_open'), mac: 'Enter', other: 'Enter' },
            ],
        },
        {
            title: t('settings_view.help.category_sftp'),
            shortcuts: [
                { label: t('settings_view.help.shortcut_select_all'), mac: '⌘A', other: 'Ctrl+A' },
                { label: t('settings_view.help.shortcut_quick_look'), mac: 'Space', other: 'Space' },
                { label: t('settings_view.help.shortcut_sftp_enter_dir'), mac: 'Enter', other: 'Enter' },
                { label: t('settings_view.help.shortcut_sftp_upload'), mac: '→', other: '→' },
                { label: t('settings_view.help.shortcut_sftp_download'), mac: '←', other: '←' },
                { label: t('settings_view.help.shortcut_rename'), mac: 'F2', other: 'F2' },
                { label: t('settings_view.help.shortcut_delete'), mac: 'Delete', other: 'Delete' },
            ],
        },
        {
            title: t('settings_view.help.category_editor'),
            shortcuts: [
                { label: t('settings_view.help.shortcut_save'), mac: '⌘S', other: 'Ctrl+S' },
                { label: t('settings_view.help.shortcut_close'), mac: 'Esc', other: 'Esc' },
            ],
        },
    ];

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
                            <p className="text-xs text-theme-text-muted mt-0.5 mb-2">{t('settings_view.terminal.bg_tabs_hint')}</p>
                            <div className="flex flex-col gap-1.5">
                                {([
                                    ['terminal', t('settings_view.terminal.bg_tab_terminal')],
                                    ['local_terminal', t('settings_view.terminal.bg_tab_local')],
                                    ['sftp', t('settings_view.terminal.bg_tab_sftp')],
                                    ['forwards', t('settings_view.terminal.bg_tab_forwards')],
                                    ['settings', t('settings_view.terminal.bg_tab_settings')],
                                    ['ide', t('settings_view.terminal.bg_tab_ide')],
                                    ['connection_monitor', t('settings_view.terminal.bg_tab_monitor')],
                                    ['connection_pool', t('settings_view.terminal.bg_tab_connections')],
                                    ['topology', t('settings_view.terminal.bg_tab_topology')],
                                    ['file_manager', t('settings_view.terminal.bg_tab_files')],
                                    ['session_manager', t('settings_view.terminal.bg_tab_sessions')],
                                    // Launcher: only macOS has a transparent-capable launcher; Windows uses WSLg (opaque VNC canvas)
                                    ...(platform.isMac ? [['launcher', t('settings_view.terminal.bg_tab_launcher')] as const] : []),
                                    ['plugin_manager', t('settings_view.terminal.bg_tab_plugins')],
                                ] as const).map(([type, label]) => {
                                    const enabledTabs = terminal.backgroundEnabledTabs ?? ['terminal', 'local_terminal'];
                                    const checked = enabledTabs.includes(type);
                                    return (
                                        <label key={type} className="flex items-center gap-2 text-sm text-theme-text cursor-pointer hover:text-theme-text-bright py-0.5">
                                            <Checkbox
                                                checked={checked}
                                                onCheckedChange={(v) => {
                                                    const next = v
                                                        ? [...enabledTabs, type]
                                                        : enabledTabs.filter((t: string) => t !== type);
                                                    updateTerminal('backgroundEnabledTabs', next);
                                                }}
                                            />
                                            {label}
                                        </label>
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

export const SettingsView = () => {
    const { t } = useTranslation();
    const bgActive = useTabBgActive('settings');
    const [activeTab, setActiveTab] = useState('general');

    // Use unified settings store
    const { settings, updateTerminal, updateConnectionDefaults, updateAi, updateSftp, setLanguage, addProvider, removeProvider, updateProvider, setActiveProvider, refreshProviderModels } = useSettingsStore();
    const { general, terminal, connectionDefaults, ai, sftp } = settings;

    // AI enable confirmation dialog
    const [showAiConfirm, setShowAiConfirm] = useState(false);
    const [refreshingModels, setRefreshingModels] = useState<string | null>(null);

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
            alert(t('settings_view.errors.create_group_failed', { error: e }));
        }
    };

    const handleDeleteGroup = async (name: string) => {
        try {
            await api.deleteGroup(name);
            const updatedGroups = await api.getGroups();
            setGroups(updatedGroups);
        } catch (e) {
            console.error('Failed to delete group:', e);
            alert(t('settings_view.errors.delete_group_failed', { error: e }));
        }
    };

    const handleImportHost = async (alias: string) => {
        try {
            const imported = await api.importSshHost(alias);
            alert(t('settings_view.errors.import_success', { name: imported.name }));
            // Remove from list to show it's imported
            setSshHosts(prev => prev.filter(h => h.alias !== alias));
            // Refresh saved connections in sidebar
            const { loadSavedConnections } = useAppStore.getState();
            await loadSavedConnections();
        } catch (e) {
            console.error('Failed to import SSH host:', e);
            alert(t('settings_view.errors.import_failed', { error: e }));
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
                    <Button
                        variant={activeTab === 'general' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('general')}
                    >
                        <Monitor className="h-4 w-4" /> {t('settings.general.title')}
                    </Button>
                    <Button
                        variant={activeTab === 'terminal' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('terminal')}
                    >
                        <TerminalIcon className="h-4 w-4" /> {t('settings.terminal.title')}
                    </Button>
                    <Button
                        variant={activeTab === 'sftp' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('sftp')}
                    >
                        <HardDrive className="h-4 w-4" /> {t('settings_view.tabs.sftp')}
                    </Button>
                    <Button
                        variant={activeTab === 'appearance' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('appearance')}
                    >
                        <Monitor className="h-4 w-4" /> {t('settings_view.tabs.appearance')}
                    </Button>
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
                        variant={activeTab === 'ai' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('ai')}
                    >
                        <Sparkles className="h-4 w-4" /> {t('settings_view.tabs.ai')}
                    </Button>
                    <Button
                        variant={activeTab === 'local' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal"
                        onClick={() => setActiveTab('local')}
                    >
                        <Square className="h-4 w-4" /> {t('settings_view.tabs.local')}
                    </Button>
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
                                                    className="w-16 text-center"
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
                                            className="w-20 text-center"
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
                                        className="w-28 text-center"
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
                                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.appearance.theme')}</h4>
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
                                                <SelectGroup>
                                                    <SelectLabel className="text-theme-text-muted text-xs uppercase tracking-wider px-2 py-1.5 font-bold whitespace-normal break-words">{t('settings_view.appearance.theme_group_oxide')}</SelectLabel>
                                                    {['oxide', 'verdigris', 'magnetite', 'cobalt', 'ochre', 'silver-oxide', 'cuprite', 'chromium-oxide', 'paper-oxide'].map((key) => (
                                                        <SelectItem key={key} value={key} className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text pl-4">
                                                            {formatThemeName(key)}
                                                        </SelectItem>
                                                    ))}
                                                </SelectGroup>

                                                <SelectSeparator className="bg-zinc-700 my-1" />

                                                <SelectGroup>
                                                    <SelectLabel className="text-theme-text-muted text-xs uppercase tracking-wider px-2 py-1.5 font-bold whitespace-normal break-words">{t('settings_view.appearance.theme_group_classic')}</SelectLabel>
                                                    {Object.keys(themes)
                                                        .filter(key => !['oxide', 'verdigris', 'magnetite', 'cobalt', 'ochre', 'silver-oxide', 'cuprite', 'chromium-oxide', 'paper-oxide'].includes(key))
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

                            {/* Layout Section */}
                            <div className="rounded-lg border border-theme-border bg-theme-bg-panel/50 p-5">
                                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.appearance.layout')}</h4>
                                <p className="text-xs text-theme-text-muted">
                                    {t('settings_view.appearance.layout_hint')}
                                </p>
                            </div>

                            {/* Background Image Section */}
                            <BackgroundImageSection terminal={terminal} updateTerminal={updateTerminal} />
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
                                                                onClick={() => {
                                                                    if (confirm(t('settings_view.ai.remove_provider_confirm', { name: provider.name }))) {
                                                                        api.deleteAiProviderApiKey(provider.id).catch(() => {});
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
                                                                            alert(t('ai.model_selector.no_key_warning'));
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
                                                                    alert(t('settings_view.ai.refresh_failed', { error: String(e) }));
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
                                </div>
                            </div>
                        </div>
                    )}

                    {activeTab === 'local' && (
                        <LocalTerminalSettings />
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
        </div>
    );
};
