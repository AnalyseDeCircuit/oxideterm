// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSettingsStore } from '@/store/settingsStore';
import { useTabBgActive } from '@/hooks/useTabBackground';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import {
    Dialog,
    DialogContent,
    DialogTitle,
    DialogDescription,
    DialogHeader,
    DialogFooter
} from '@/components/ui/dialog';
import { BookOpen, Code2, HardDrive, HelpCircle, Key, Keyboard, Monitor, Shield, Sparkles, Square, Terminal as TerminalIcon, WifiOff } from 'lucide-react';
import { DocumentManager } from '@/components/settings/DocumentManager';
import { KeybindingEditorSection } from '@/components/settings/KeybindingEditorSection';
import { useToast } from '@/hooks/useToast';
import { useConfirm } from '@/hooks/useConfirm';
import { LocalTerminalSettings } from '@/components/settings/LocalTerminalSettings';
import { HelpAboutSection } from '@/components/settings/HelpAboutSection';
import { GeneralTab } from '@/components/settings/tabs/GeneralTab';
import { TerminalTab } from '@/components/settings/tabs/TerminalTab';
import { AppearanceTab } from '@/components/settings/tabs/AppearanceTab';
import { ConnectionsTab } from '@/components/settings/tabs/ConnectionsTab';
import { SshTab } from '@/components/settings/tabs/SshTab';
import { ReconnectTab } from '@/components/settings/tabs/ReconnectTab';
import { SftpTab } from '@/components/settings/tabs/SftpTab';
import { IdeTab } from '@/components/settings/tabs/IdeTab';
import { AiTab } from '@/components/settings/tabs/AiTab';
import { api } from '@/lib/api';
import type { PortableStatusResponse } from '@/types';

export const SettingsView = () => {
    const { t } = useTranslation();
    const { success: toastSuccess, error: toastError } = useToast();
    const { confirm: confirmDialog, ConfirmDialog } = useConfirm();
    const bgActive = useTabBgActive('settings');
    const [activeTab, setActiveTab] = useState('general');
    const [portableStatus, setPortableStatus] = useState<PortableStatusResponse | null | undefined>(undefined);

    const { settings, updateTerminal, updateAppearance, updateConnectionDefaults, updateAi, updateSftp, updateIde, updateReconnect, updateConnectionPool, setLanguage, addProvider, removeProvider, updateProvider, setActiveProvider, refreshProviderModels, setUserContextWindow } = useSettingsStore();
    const { general, terminal, appearance, connectionDefaults, ai, sftp, ide, reconnect } = settings;
    const [showAiConfirm, setShowAiConfirm] = useState(false);
    const [refreshingModels, setRefreshingModels] = useState<string | null>(null);

    useEffect(() => {
        let cancelled = false;

        api.getPortableStatus()
            .then((status) => {
                if (!cancelled) {
                    setPortableStatus(status);
                }
            })
            .catch((error) => {
                console.warn('Failed to load portable status:', error);
            });

        return () => {
            cancelled = true;
        };
    }, []);

    return (
        <div className={`flex h-full w-full text-theme-text ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
            {/* Sidebar */}
            <div className="w-56 bg-theme-bg-panel border-r border-theme-border flex flex-col pt-6 pb-4 min-h-0">
                <div className="px-5 mb-6">
                    <h2 className="text-xl font-semibold text-theme-text-heading">{t('settings_view.title')}</h2>
                    {portableStatus?.isPortable && (
                        <div className="mt-3 rounded-lg border border-theme-border bg-theme-bg-elevated/80 px-3 py-2">
                            <div className="text-sm font-medium text-theme-text">{t('settings_view.portable_badge')}</div>
                            <div className="mt-1 text-xs leading-5 text-theme-text-muted">{t('settings_view.portable_description')}</div>
                        </div>
                    )}
                </div>
                <div className="space-y-1 px-3 flex-1 overflow-y-auto min-h-0">
                    {/* ── 基础 ── */}
                    <Button
                        variant={activeTab === 'general' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('general')}
                    >
                        <Monitor className="h-4 w-4" /> {t('settings.general.title')}
                    </Button>

                    <Separator className="!my-2" />

                    {/* ── 终端（字体/光标/缓冲区 → 主题/背景 → 本地 shell） ── */}
                    <Button
                        variant={activeTab === 'terminal' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('terminal')}
                    >
                        <TerminalIcon className="h-4 w-4" /> {t('settings.terminal.title')}
                    </Button>
                    <Button
                        variant={activeTab === 'appearance' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('appearance')}
                    >
                        <Monitor className="h-4 w-4" /> {t('settings_view.tabs.appearance')}
                    </Button>
                    <Button
                        variant={activeTab === 'local' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('local')}
                    >
                        <Square className="h-4 w-4" /> {t('settings_view.tabs.local')}
                    </Button>

                    <Separator className="!my-2" />

                    {/* ── 连接（默认/分组 → 密钥 → 重连策略） ── */}
                    <Button
                        variant={activeTab === 'connections' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('connections')}
                    >
                        <Shield className="h-4 w-4" /> {t('settings_view.tabs.connections')}
                    </Button>
                    <Button
                        variant={activeTab === 'ssh' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('ssh')}
                    >
                        <Key className="h-4 w-4" /> {t('settings_view.tabs.ssh')}
                    </Button>
                    <Button
                        variant={activeTab === 'reconnect' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('reconnect')}
                    >
                        <WifiOff className="h-4 w-4" /> {t('settings_view.tabs.reconnect')}
                    </Button>

                    <Separator className="!my-2" />

                    {/* ── 功能（文件传输 → 编辑器 → AI） ── */}
                    <Button
                        variant={activeTab === 'sftp' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('sftp')}
                    >
                        <HardDrive className="h-4 w-4" /> {t('settings_view.tabs.sftp')}
                    </Button>
                    <Button
                        variant={activeTab === 'ide' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('ide')}
                    >
                        <Code2 className="h-4 w-4" /> {t('settings_view.tabs.ide', 'IDE')}
                    </Button>
                    <Button
                        variant={activeTab === 'ai' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('ai')}
                    >
                        <Sparkles className="h-4 w-4" /> {t('settings_view.tabs.ai')}
                    </Button>
                    <Button
                        variant={activeTab === 'knowledge' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('knowledge')}
                    >
                        <BookOpen className="h-4 w-4" /> {t('settings_view.tabs.knowledge')}
                    </Button>
                    <Button
                        variant={activeTab === 'keybindings' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('keybindings')}
                    >
                        <Keyboard className="h-4 w-4" /> {t('settings_view.tabs.keybindings')}
                    </Button>

                    <Separator className="!my-2" />

                    {/* ── 帮助 ── */}
                    <Button
                        variant={activeTab === 'help' ? 'secondary' : 'ghost'}
                        className="w-full justify-start gap-3 h-10 font-normal rounded-md"
                        onClick={() => setActiveTab('help')}
                    >
                        <HelpCircle className="h-4 w-4" /> {t('settings_view.tabs.help')}
                    </Button>
                </div>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-y-auto">
                <div className="max-w-4xl mx-auto p-10">
                    {activeTab === 'general' && <GeneralTab general={general} setLanguage={setLanguage} />}

                    {activeTab === 'terminal' && <TerminalTab terminal={terminal} updateTerminal={updateTerminal} />}

                    {activeTab === 'appearance' && (
                        <AppearanceTab
                            terminal={terminal}
                            appearance={appearance}
                            updateTerminal={updateTerminal}
                            updateAppearance={updateAppearance}
                        />
                    )}

                    {activeTab === 'connections' && (
                        <ConnectionsTab
                            connectionDefaults={connectionDefaults}
                            updateConnectionDefaults={updateConnectionDefaults}
                            idleTimeoutSecs={settings.connectionPool?.idleTimeoutSecs ?? 1800}
                            updateConnectionPool={updateConnectionPool}
                        />
                    )}

                    {activeTab === 'ssh' && <SshTab />}

                    {activeTab === 'ai' && (
                        <AiTab
                            ai={ai}
                            updateAi={updateAi}
                            addProvider={addProvider}
                            removeProvider={removeProvider}
                            updateProvider={updateProvider}
                            setActiveProvider={setActiveProvider}
                            refreshProviderModels={refreshProviderModels}
                            setUserContextWindow={setUserContextWindow}
                            refreshingModels={refreshingModels}
                            setRefreshingModels={setRefreshingModels}
                            onRequestEnableAiConfirm={() => setShowAiConfirm(true)}
                        />
                    )}

                    {activeTab === 'knowledge' && (
                        <DocumentManager />
                    )}

                    {activeTab === 'local' && (
                        <LocalTerminalSettings />
                    )}

                    {activeTab === 'reconnect' && <ReconnectTab reconnect={reconnect} updateReconnect={updateReconnect} />}

                    {activeTab === 'help' && (
                        <HelpAboutSection isPortableMode={portableStatus ? portableStatus.isPortable : portableStatus ?? null} />
                    )}

                    {activeTab === 'keybindings' && (
                        <KeybindingEditorSection
                            onToastSuccess={toastSuccess}
                            onToastError={toastError}
                            onConfirm={confirmDialog}
                        />
                    )}

                    {activeTab === 'sftp' && (
                        <SftpTab sftp={sftp} updateSftp={updateSftp} />
                    )}

                    {activeTab === 'ide' && (
                        <IdeTab ide={ide} terminal={terminal} updateIde={updateIde} />
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
                                <div className="w-1 h-1 rounded-full bg-theme-text-muted mt-1.5 shrink-0"></div>
                                <p>{t('settings_view.ai_confirm.point_local')}</p>
                            </div>
                            <div className="flex items-start gap-2">
                                <div className="w-1 h-1 rounded-full bg-theme-text-muted mt-1.5 shrink-0"></div>
                                <p>{t('settings_view.ai_confirm.point_no_server')}</p>
                            </div>
                            <div className="flex items-start gap-2">
                                <div className="w-1 h-1 rounded-full bg-theme-text-muted mt-1.5 shrink-0"></div>
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
