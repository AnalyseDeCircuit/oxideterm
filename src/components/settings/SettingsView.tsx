import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore, type RendererType, type FontFamily, type CursorStyle, type Language } from '../../store/settingsStore';
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
import { Monitor, Key, Terminal as TerminalIcon, Shield, Plus, Trash2, FolderInput, Sparkles, Square, HardDrive } from 'lucide-react';
import { api } from '../../lib/api';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { SshKeyInfo, SshHostInfo } from '../../types';
import { themes } from '../../lib/themes';

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

export const SettingsView = () => {
    const { t } = useTranslation();
    const [activeTab, setActiveTab] = useState('general');

    // Use unified settings store
    const { settings, updateTerminal, updateConnectionDefaults, updateAi, updateSftp, setLanguage } = useSettingsStore();
    const { general, terminal, connectionDefaults, ai, sftp } = settings;

    // AI enable confirmation dialog
    const [showAiConfirm, setShowAiConfirm] = useState(false);
    const [hasApiKey, setHasApiKey] = useState(false);
    const [apiKeyInput, setApiKeyInput] = useState('');
    const [apiKeySaving, setApiKeySaving] = useState(false);

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
        } else if (activeTab === 'ai') {
            api.hasAiApiKey()
                .then(setHasApiKey)
                .catch((e) => {
                    console.error('Failed to check API key:', e);
                    setHasApiKey(false);
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
        <div className="flex h-full w-full bg-theme-bg text-theme-text">
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
                                                <SelectItem value="jetbrains">JetBrains Mono NF ✓</SelectItem>
                                                <SelectItem value="meslo">MesloLGM NF ✓</SelectItem>
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
                                                    <SelectLabel className="text-theme-text-muted text-xs uppercase tracking-wider px-2 py-1.5 font-bold">Oxide Series</SelectLabel>
                                                    {['oxide', 'verdigris', 'magnetite', 'cobalt', 'ochre', 'silver-oxide', 'cuprite', 'chromium-oxide', 'paper-oxide'].map((key) => (
                                                        <SelectItem key={key} value={key} className="text-theme-text focus:bg-theme-bg-hover focus:text-theme-text pl-4">
                                                            {formatThemeName(key)}
                                                        </SelectItem>
                                                    ))}
                                                </SelectGroup>

                                                <SelectSeparator className="bg-zinc-700 my-1" />

                                                <SelectGroup>
                                                    <SelectLabel className="text-theme-text-muted text-xs uppercase tracking-wider px-2 py-1.5 font-bold">Classic / Other</SelectLabel>
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

                                {/* API Configuration - Using Form/Grid Layout like Connections */}
                                <div className={ai.enabled ? "" : "opacity-50 pointer-events-none"}>
                                    <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.provider_settings')}</h4>

                                    <div className="grid grid-cols-1 md:grid-cols-2 gap-6 max-w-3xl mb-6">
                                        <div className="grid gap-2">
                                            <Label>{t('settings_view.ai.base_url')}</Label>
                                            <Input
                                                value={ai.baseUrl}
                                                onChange={(e) => updateAi('baseUrl', e.target.value)}
                                                placeholder="https://api.openai.com/v1"
                                                className="bg-theme-bg"
                                            />
                                        </div>

                                        <div className="grid gap-2">
                                            <Label>{t('settings_view.ai.model')}</Label>
                                            <Input
                                                value={ai.model}
                                                onChange={(e) => updateAi('model', e.target.value)}
                                                placeholder="gpt-4o-mini"
                                                className="bg-theme-bg"
                                            />
                                        </div>
                                    </div>

                                    <div className="max-w-3xl mb-6">
                                        <div className="grid gap-2">
                                            <Label>{t('settings_view.ai.api_key')}</Label>
                                            <div className="flex gap-2">
                                                {hasApiKey ? (
                                                    <div className="flex-1 flex items-center gap-2">
                                                        <div className="flex-1 h-10 px-3 flex items-center bg-theme-bg-panel/50 border border-theme-border/50 rounded-md text-theme-text-muted text-sm italic">
                                                            ••••••••••••••••••••••••
                                                        </div>
                                                        <Button
                                                            variant="ghost"
                                                            size="sm"
                                                            className="text-red-400 hover:text-red-300 hover:bg-red-400/10"
                                                            onClick={async () => {
                                                                if (confirm(t('settings_view.ai.remove_confirm'))) {
                                                                    try {
                                                                        await api.deleteAiApiKey();
                                                                        setHasApiKey(false);
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
                                                            className="flex-1 bg-theme-bg"
                                                            value={apiKeyInput}
                                                            onChange={(e) => setApiKeyInput(e.target.value)}
                                                        />
                                                        <Button
                                                            variant="secondary"
                                                            disabled={!apiKeyInput.trim() || apiKeySaving}
                                                            onClick={async () => {
                                                                if (!apiKeyInput.trim()) return;
                                                                setApiKeySaving(true);
                                                                try {
                                                                    await api.setAiApiKey(apiKeyInput);
                                                                    setApiKeyInput('');
                                                                    setHasApiKey(true);
                                                                    window.dispatchEvent(new CustomEvent('ai-api-key-updated'));
                                                                } catch (e) {
                                                                    alert(t('settings_view.ai.save_failed', { error: e }));
                                                                } finally {
                                                                    setApiKeySaving(false);
                                                                }
                                                            }}
                                                        >
                                                            {apiKeySaving ? t('settings_view.ai.saving') : t('settings_view.ai.save')}
                                                        </Button>
                                                    </>
                                                )}
                                            </div>
                                            <p className="text-xs text-theme-text-muted">{t('settings_view.ai.api_key_stored')}</p>
                                        </div>
                                    </div>

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
