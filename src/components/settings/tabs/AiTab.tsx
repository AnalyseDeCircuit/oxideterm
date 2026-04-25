// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState, type ElementType } from 'react';
import { useTranslation } from 'react-i18next';
import { Activity, Brain, ChevronDown, ChevronRight, CirclePlus, CircleStop, Code2, FileCode, FileText, FlaskConical, FolderInput, FolderOpen, FolderSearch, GitBranch, HardDrive, Info, Keyboard, ListTree, Monitor, MousePointer2, Network, Pen, Puzzle, Radio, RefreshCw, Search, Settings, Terminal as TerminalIcon, Wrench, X } from 'lucide-react';
import { McpServersPanel } from '@/components/settings/McpServersPanel';
import { ProviderKeyInput } from '@/components/settings/ProviderKeyInput';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import { useConfirm } from '@/hooks/useConfirm';
import { useToast } from '@/hooks/useToast';
import { TOOL_GROUPS, WRITE_TOOLS, EXPERIMENTAL_TOOLS } from '@/lib/ai/tools';
import { getModelContextWindowInfo } from '@/lib/ai/tokenUtils';
import { api } from '@/lib/api';
import { cn } from '@/lib/utils';
import type { AiProvider } from '@/types';
import type { AiSettings } from '@/store/settingsStore';

const TOOL_ICON_MAP: Record<string, ElementType> = {
    terminal_exec: TerminalIcon,
    read_file: FileText,
    write_file: Pen,
    list_directory: FolderOpen,
    grep_search: Search,
    git_status: GitBranch,
    list_sessions: Network,
    get_terminal_buffer: TerminalIcon,
    search_terminal: Search,
    list_connections: Network,
    list_port_forwards: Radio,
    get_detected_ports: Radio,
    get_connection_health: Activity,
    create_port_forward: CirclePlus,
    stop_port_forward: CircleStop,
    sftp_list_dir: FolderSearch,
    sftp_read_file: HardDrive,
    sftp_stat: Info,
    sftp_get_cwd: HardDrive,
    ide_get_open_files: FileCode,
    ide_get_file_content: FileCode,
    ide_get_project_info: Code2,
    ide_apply_edit: Pen,
    local_list_shells: TerminalIcon,
    local_get_terminal_info: ListTree,
    local_exec: TerminalIcon,
    local_get_drives: HardDrive,
    get_settings: Settings,
    update_setting: Settings,
    get_pool_stats: Activity,
    set_pool_config: Settings,
    get_all_health: Activity,
    get_resource_metrics: Activity,
    list_saved_connections: Network,
    search_saved_connections: Search,
    get_session_tree: ListTree,
    list_plugins: Puzzle,
    read_screen: Monitor,
    send_keys: Keyboard,
    send_mouse: MousePointer2,
};

const TOOL_GROUP_ICONS: Record<string, ElementType> = {
    terminal: TerminalIcon,
    session: Network,
    infrastructure: Radio,
    sftp: FolderInput,
    ide: Code2,
    local_terminal: TerminalIcon,
    settings: Settings,
    connection_pool: Activity,
    connection_monitor: Activity,
    session_manager: Network,
    plugin_manager: Puzzle,
    tui_interaction: Monitor,
};

type AiTabProps = {
    ai: AiSettings;
    updateAi: <K extends keyof AiSettings>(key: K, value: AiSettings[K]) => void;
    addProvider: (provider: AiProvider) => void;
    removeProvider: (providerId: string) => void;
    updateProvider: (providerId: string, patch: Partial<AiSettings['providers'][number]>) => void;
    setActiveProvider: (providerId: string) => void;
    refreshProviderModels: (providerId: string) => Promise<string[]>;
    setUserContextWindow: (providerId: string, model: string, value: number | null) => void;
    refreshingModels: string | null;
    setRefreshingModels: (providerId: string | null) => void;
    onRequestEnableAiConfirm: () => void;
};

export const AiTab = ({
    ai,
    updateAi,
    addProvider,
    removeProvider,
    updateProvider,
    setActiveProvider,
    refreshProviderModels,
    setUserContextWindow,
    refreshingModels,
    setRefreshingModels,
    onRequestEnableAiConfirm,
}: AiTabProps) => {
    const { t } = useTranslation();
    const { error: toastError } = useToast();
    const { confirm, ConfirmDialog } = useConfirm();
    const [contextWindowsExpanded, setContextWindowsExpanded] = useState(true);
    const [collapsedContextProviders, setCollapsedContextProviders] = useState<Record<string, boolean>>({});
    const [expandedProviders, setExpandedProviders] = useState<Record<string, boolean>>({});
    const [expandedProviderModels, setExpandedProviderModels] = useState<Record<string, boolean>>({});
    const [toolUseExpanded, setToolUseExpanded] = useState(true);
    const memory = ai.memory ?? { enabled: true, content: '' };
    const toolUse = ai.toolUse ?? { enabled: false, autoApproveTools: {}, disabledTools: [] };
    const allToolNames = TOOL_GROUPS.flatMap((group) => [...group.readOnly, ...group.write]);
    const approvedToolCount = allToolNames.filter((name) => toolUse.autoApproveTools?.[name] === true).length;

    return (
        <>
            <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
                <div>
                    <h3 className="text-2xl font-medium text-theme-text-heading mb-2">{t('settings_view.ai.title')}</h3>
                    <p className="text-theme-text-muted">{t('settings_view.ai.description')}</p>
                </div>
                <Separator />

                <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                    <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.general')}</h4>

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
                                    onRequestEnableAiConfirm();
                                } else {
                                    updateAi('enabled', !!checked);
                                }
                            }}
                        />
                    </div>

                    <div className="mb-6 p-3 rounded bg-theme-bg-card border border-theme-border">
                        <p className="text-xs text-theme-text-muted leading-relaxed">
                            <span className="font-semibold text-theme-text-muted">{t('settings_view.ai.privacy_notice')}:</span> {t('settings_view.ai.privacy_text')}
                        </p>
                    </div>

                    <Separator className="my-6 opacity-50" />

                    <div className={ai.enabled ? '' : 'opacity-50 pointer-events-none'}>
                        <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.provider_settings')}</h4>

                        <div className="space-y-3 max-w-3xl mb-6">
                            {ai.providers.map((provider) => {
                                const isActiveProvider = provider.id === ai.activeProviderId;
                                const isExpanded = expandedProviders[provider.id] ?? isActiveProvider;
                                const modelsExpanded = expandedProviderModels[provider.id] === true;
                                const visibleModels = modelsExpanded ? provider.models : provider.models.slice(0, 8);
                                const hiddenModelCount = Math.max(0, provider.models.length - visibleModels.length);

                                const refreshModels = async () => {
                                    if (provider.type !== 'ollama' && provider.type !== 'openai_compatible') {
                                        try {
                                            const hasKey = await api.hasAiProviderApiKey(provider.id);
                                            if (!hasKey) {
                                                toastError(t('ai.model_selector.no_key_warning'));
                                                return;
                                            }
                                        } catch {
                                        }
                                    }

                                    setRefreshingModels(provider.id);
                                    try {
                                        await refreshProviderModels(provider.id);
                                    } catch (error) {
                                        console.error('[Settings] Failed to refresh models:', error);
                                        toastError(t('settings_view.ai.refresh_failed', { error: String(error) }));
                                    } finally {
                                        setRefreshingModels(null);
                                    }
                                };

                                return (
                                <div
                                    key={provider.id}
                                    className={cn(
                                        'rounded-lg border transition-colors',
                                        isActiveProvider
                                            ? 'border-theme-accent/60 bg-theme-accent/5'
                                            : 'border-theme-border/70 bg-theme-bg/70',
                                    )}
                                >
                                    <div
                                        role="button"
                                        tabIndex={0}
                                        className="flex w-full items-start justify-between gap-4 p-4 text-left"
                                        onClick={() => setExpandedProviders((current) => ({
                                            ...current,
                                            [provider.id]: !(current[provider.id] ?? isActiveProvider),
                                        }))}
                                        onKeyDown={(event) => {
                                            if (event.key === 'Enter' || event.key === ' ') {
                                                event.preventDefault();
                                                setExpandedProviders((current) => ({
                                                    ...current,
                                                    [provider.id]: !(current[provider.id] ?? isActiveProvider),
                                                }));
                                            }
                                        }}
                                        aria-expanded={isExpanded}
                                    >
                                        <div className="min-w-0 flex-1">
                                            <div className="flex flex-wrap items-center gap-2">
                                                <span className="font-medium text-sm text-theme-text">{provider.name}</span>
                                                <span className="text-[10px] px-1.5 py-0.5 rounded bg-theme-bg-panel text-theme-text-muted uppercase tracking-wider">{provider.type}</span>
                                                {isActiveProvider && (
                                                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-theme-accent/20 text-theme-accent font-medium">
                                                        {t('settings_view.ai.active')}
                                                    </span>
                                                )}
                                                <span className={cn(
                                                    'text-[10px] px-1.5 py-0.5 rounded font-medium',
                                                    provider.enabled
                                                        ? 'bg-emerald-500/10 text-emerald-400'
                                                        : 'bg-theme-border/20 text-theme-text-muted',
                                                )}>
                                                    {provider.enabled ? t('settings_view.ai.provider_enabled') : t('settings_view.ai.provider_disabled')}
                                                </span>
                                            </div>
                                            <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-theme-text-muted">
                                                <span className="truncate max-w-[260px]">{t('settings_view.ai.default_model')}: <span className="font-mono text-theme-text-muted/90">{provider.defaultModel || '—'}</span></span>
                                                <span>{t('settings_view.ai.provider_models_summary', { count: provider.models.length })}</span>
                                                {provider.type !== 'ollama' && <span>{t('settings_view.ai.api_key')}: {t('settings_view.ai.api_key_stored')}</span>}
                                            </div>
                                        </div>
                                        <div className="flex shrink-0 items-center gap-2">
                                            {!isActiveProvider && (
                                                <span
                                                    role="button"
                                                    tabIndex={0}
                                                    onClick={(event) => {
                                                        event.stopPropagation();
                                                        setActiveProvider(provider.id);
                                                    }}
                                                    onKeyDown={(event) => {
                                                        if (event.key === 'Enter' || event.key === ' ') {
                                                            event.preventDefault();
                                                            event.stopPropagation();
                                                            setActiveProvider(provider.id);
                                                        }
                                                    }}
                                                    className="rounded-full border border-theme-border px-2.5 py-1 text-[11px] text-theme-text-muted hover:border-theme-accent/60 hover:text-theme-accent transition-colors"
                                                >
                                                    {t('settings_view.ai.set_active')}
                                                </span>
                                            )}
                                            {isExpanded
                                                ? <ChevronDown className="h-4 w-4 text-theme-text-muted" />
                                                : <ChevronRight className="h-4 w-4 text-theme-text-muted" />}
                                        </div>
                                    </div>

                                    {isExpanded && (
                                        <div className="border-t border-theme-border/30 px-4 pb-4 pt-3">
                                            <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
                                                <label className="flex items-center gap-2 text-xs text-theme-text-muted cursor-pointer">
                                                    <Checkbox checked={provider.enabled} onCheckedChange={(checked) => updateProvider(provider.id, { enabled: !!checked })} />
                                                    {t('settings_view.ai.provider_enabled')}
                                                </label>
                                                <div className="flex items-center gap-2">
                                                    <Button
                                                        variant="ghost"
                                                        size="sm"
                                                        className="h-7 px-2 text-[10px] gap-1"
                                                        disabled={refreshingModels === provider.id}
                                                        onClick={refreshModels}
                                                    >
                                                        <RefreshCw className={cn('w-3 h-3', refreshingModels === provider.id && 'animate-spin')} />
                                                        {t('settings_view.ai.refresh_models')}
                                                    </Button>
                                                    {provider.id.startsWith('custom-') && (
                                                        <Button
                                                            variant="ghost"
                                                            size="sm"
                                                            className="h-7 px-2 text-xs text-red-400 hover:text-red-300 hover:bg-red-400/10"
                                                            onClick={async () => {
                                                                if (await confirm({ title: t('settings_view.ai.remove_provider_confirm', { name: provider.name }), variant: 'danger' })) {
                                                                    api.deleteAiProviderApiKey(provider.id).catch(() => {});
                                                                    removeProvider(provider.id);
                                                                }
                                                            }}
                                                        >
                                                            {t('settings_view.ai.remove')}
                                                        </Button>
                                                    )}
                                                </div>
                                            </div>

                                            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs">
                                                <div className="grid gap-1">
                                                    <Label className="text-xs text-theme-text-muted">{t('settings_view.ai.base_url')}</Label>
                                                    <Input
                                                        value={provider.baseUrl}
                                                        onChange={(event) => updateProvider(provider.id, { baseUrl: event.target.value })}
                                                        className="bg-theme-bg h-8 text-xs"
                                                        placeholder={provider.type === 'openai_compatible' ? 'http://localhost:1234/v1' : undefined}
                                                    />
                                                </div>
                                                <div className="grid gap-1">
                                                    <Label className="text-xs text-theme-text-muted">{t('settings_view.ai.default_model')}</Label>
                                                    <Input
                                                        value={provider.defaultModel}
                                                        onChange={(event) => updateProvider(provider.id, { defaultModel: event.target.value })}
                                                        className="bg-theme-bg h-8 text-xs"
                                                    />
                                                </div>
                                            </div>

                                            <div className="mt-3">
                                                <div className="flex items-center justify-between mb-1">
                                                    <Label className="text-xs text-theme-text-muted">{t('settings_view.ai.available_models')} ({provider.models.length})</Label>
                                                    {provider.models.length > 8 && (
                                                        <button
                                                            type="button"
                                                            className="text-[10px] text-theme-accent hover:underline"
                                                            onClick={() => setExpandedProviderModels((current) => ({
                                                                ...current,
                                                                [provider.id]: !current[provider.id],
                                                            }))}
                                                        >
                                                            {modelsExpanded
                                                                ? t('settings_view.ai.show_fewer_models')
                                                                : t('settings_view.ai.show_all_models', { count: provider.models.length })}
                                                        </button>
                                                    )}
                                                </div>
                                                {provider.models.length > 0 && (
                                                    <div className="flex flex-wrap gap-1">
                                                        {visibleModels.map((model) => (
                                                            <span
                                                                key={model}
                                                                className={cn(
                                                                    'text-[10px] px-1.5 py-0.5 rounded border bg-theme-bg text-theme-text-muted cursor-pointer hover:text-theme-text hover:border-theme-border transition-colors',
                                                                    provider.defaultModel === model ? 'border-theme-accent/60 text-theme-accent bg-theme-accent/10' : 'border-theme-border/50',
                                                                )}
                                                                onClick={() => updateProvider(provider.id, { defaultModel: model })}
                                                                title={t('settings_view.ai.click_to_set_default')}
                                                            >
                                                                {model}
                                                            </span>
                                                        ))}
                                                        {hiddenModelCount > 0 && <span className="text-[10px] px-1.5 py-0.5 text-theme-text-muted">+{hiddenModelCount}</span>}
                                                    </div>
                                                )}
                                            </div>

                                            {provider.type !== 'ollama' && (
                                                <div className="mt-3">
                                                    <ProviderKeyInput providerId={provider.id} />
                                                </div>
                                            )}
                                        </div>
                                    )}
                                </div>
                                );
                            })}
                        </div>

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
                        <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.embedding_title')}</h4>
                        <p className="text-xs text-theme-text-muted mb-4">{t('settings_view.ai.embedding_description')}</p>
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 max-w-3xl mb-6">
                            <div className="grid gap-1">
                                <Label className="text-xs text-theme-text-muted">{t('settings_view.ai.embedding_provider')}</Label>
                                <Select
                                    value={ai.embeddingConfig?.providerId ?? '__default__'}
                                    onValueChange={(value) => updateAi('embeddingConfig', { ...ai.embeddingConfig, providerId: value === '__default__' ? null : value, model: ai.embeddingConfig?.model ?? '' })}
                                >
                                    <SelectTrigger className="bg-theme-bg h-8 text-xs">
                                        <SelectValue placeholder={t('settings_view.ai.embedding_provider_placeholder')} />
                                    </SelectTrigger>
                                    <SelectContent>
                                        <SelectItem value="__default__">{t('settings_view.ai.embedding_provider_default')}</SelectItem>
                                        {ai.providers.filter((provider) => provider.enabled && provider.type !== 'anthropic').map((provider) => (
                                            <SelectItem key={provider.id} value={provider.id}>{provider.name}</SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            </div>
                            <div className="grid gap-1">
                                <Label className="text-xs text-theme-text-muted">{t('settings_view.ai.embedding_model')}</Label>
                                <Input
                                    value={ai.embeddingConfig?.model ?? ''}
                                    onChange={(event) => updateAi('embeddingConfig', { ...ai.embeddingConfig, providerId: ai.embeddingConfig?.providerId ?? null, model: event.target.value })}
                                    className="bg-theme-bg h-8 text-xs"
                                    placeholder={(() => {
                                        const embeddingProvider = ai.providers.find((provider) => provider.id === ai.embeddingConfig?.providerId);
                                        if (embeddingProvider?.type === 'ollama') return 'nomic-embed-text';
                                        return 'text-embedding-3-small';
                                    })()}
                                />
                            </div>
                        </div>

                        <Separator className="my-6 opacity-50" />

                        <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.context_controls')}</h4>
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-6 max-w-3xl">
                            <div className="grid gap-2">
                                <Label>{t('settings_view.ai.max_context')}</Label>
                                <Select value={ai.contextMaxChars.toString()} onValueChange={(value) => updateAi('contextMaxChars', parseInt(value, 10))}>
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
                                <Select value={ai.contextVisibleLines.toString()} onValueChange={(value) => updateAi('contextVisibleLines', parseInt(value, 10))}>
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

                        <div className="mt-6 max-w-3xl">
                            <h5 className="text-xs font-medium text-theme-text-muted mb-3 uppercase tracking-wider">{t('settings_view.ai.context_sources')}</h5>
                            <div className="space-y-3">
                                <label className="flex items-center gap-3 cursor-pointer">
                                    <input
                                        type="checkbox"
                                        checked={ai.contextSources?.ide !== false}
                                        onChange={(event) => updateAi('contextSources', { ide: event.target.checked, sftp: ai.contextSources?.sftp !== false })}
                                        className="rounded border-theme-border"
                                    />
                                    <div>
                                        <span className="text-sm text-theme-text">{t('settings_view.ai.context_source_ide')}</span>
                                        <p className="text-xs text-theme-text-muted">{t('settings_view.ai.context_source_ide_hint')}</p>
                                    </div>
                                </label>
                                <label className="flex items-center gap-3 cursor-pointer">
                                    <input
                                        type="checkbox"
                                        checked={ai.contextSources?.sftp !== false}
                                        onChange={(event) => updateAi('contextSources', { ide: ai.contextSources?.ide !== false, sftp: event.target.checked })}
                                        className="rounded border-theme-border"
                                    />
                                    <div>
                                        <span className="text-sm text-theme-text">{t('settings_view.ai.context_source_sftp')}</span>
                                        <p className="text-xs text-theme-text-muted">{t('settings_view.ai.context_source_sftp_hint')}</p>
                                    </div>
                                </label>
                            </div>
                        </div>

                        <Separator className="my-6 opacity-50" />

                        <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.system_prompt_title')}</h4>
                        <div className="max-w-3xl grid gap-2">
                            <Label>{t('settings_view.ai.custom_system_prompt')}</Label>
                            <textarea
                                value={ai.customSystemPrompt || ''}
                                onChange={(event) => updateAi('customSystemPrompt', event.target.value)}
                                placeholder={t('settings_view.ai.system_prompt_placeholder')}
                                rows={4}
                                className="w-full bg-theme-bg border border-theme-border rounded-md px-3 py-2 text-sm text-theme-text placeholder-theme-text-muted/40 resize-y min-h-[80px] max-h-[200px] focus:outline-none focus:ring-1 focus:ring-theme-accent/40"
                            />
                            <p className="text-xs text-theme-text-muted">{t('settings_view.ai.system_prompt_hint')}</p>
                        </div>

                        <Separator className="my-6 opacity-50" />

                        <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider flex items-center gap-2">
                            <Brain className="w-4 h-4" />
                            {t('settings_view.ai.memory_title')}
                        </h4>
                        <div className="max-w-3xl grid gap-3">
                            <div className="flex items-center justify-between gap-4">
                                <div>
                                    <Label>{t('settings_view.ai.memory_enabled')}</Label>
                                    <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.ai.memory_enabled_hint')}</p>
                                </div>
                                <Checkbox
                                    id="ai-memory-enabled"
                                    checked={memory.enabled}
                                    onCheckedChange={(checked) => updateAi('memory', { ...memory, enabled: !!checked })}
                                />
                            </div>
                            <textarea
                                value={memory.content}
                                onChange={(event) => updateAi('memory', { ...memory, content: event.target.value })}
                                placeholder={t('settings_view.ai.memory_placeholder')}
                                rows={5}
                                className="w-full bg-theme-bg border border-theme-border rounded-md px-3 py-2 text-sm text-theme-text placeholder-theme-text-muted/40 resize-y min-h-[120px] max-h-[260px] focus:outline-none focus:ring-1 focus:ring-theme-accent/40"
                            />
                            <div className="flex items-start justify-between gap-3">
                                <p className="text-xs text-theme-text-muted leading-relaxed">{t('settings_view.ai.memory_hint')}</p>
                                <Button
                                    variant="ghost"
                                    size="sm"
                                    className="shrink-0 text-xs"
                                    disabled={!memory.content.trim()}
                                    onClick={() => updateAi('memory', { ...memory, content: '' })}
                                >
                                    {t('settings_view.ai.memory_clear')}
                                </Button>
                            </div>
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
                                        onChange={(event) => {
                                            const value = event.target.value ? parseInt(event.target.value, 10) : undefined;
                                            const existing = ai.modelMaxResponseTokens ?? {};
                                            const providerOverrides = existing[ai.activeProviderId!] ?? {};
                                            const updated = { ...existing, [ai.activeProviderId!]: { ...providerOverrides } };
                                            if (value && value >= 256) {
                                                updated[ai.activeProviderId!][ai.activeModel!] = value;
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

                        <Separator className="my-6 opacity-50" />

                        <div className={ai.enabled ? '' : 'opacity-50 pointer-events-none'}>
                            <button
                                type="button"
                                className="mb-4 flex w-full max-w-3xl items-start justify-between gap-3 text-left"
                                onClick={() => setContextWindowsExpanded((current) => !current)}
                                aria-expanded={contextWindowsExpanded}
                            >
                                <div>
                                    <h4 className="text-sm font-medium text-theme-text mb-2 uppercase tracking-wider">{t('settings_view.ai.model_context_windows')}</h4>
                                    <p className="text-xs text-theme-text-muted">{t('settings_view.ai.model_context_windows_hint')}</p>
                                </div>
                                {contextWindowsExpanded
                                    ? <ChevronDown className="mt-0.5 h-4 w-4 shrink-0 text-theme-text-muted" />
                                    : <ChevronRight className="mt-0.5 h-4 w-4 shrink-0 text-theme-text-muted" />}
                            </button>

                            {contextWindowsExpanded && (ai.providers.every((provider) => provider.models.length === 0) ? (
                                <p className="text-xs text-theme-text-muted italic">{t('settings_view.ai.model_context_windows_empty')}</p>
                            ) : (
                                <div className="space-y-4 max-w-3xl">
                                    {ai.providers.filter((provider) => provider.models.length > 0).map((provider) => {
                                        const providerCollapsed = collapsedContextProviders[provider.id] === true;
                                        const userOverrideCount = provider.models.filter((model) => !!ai.userContextWindows?.[provider.id]?.[model]).length;

                                        return (
                                        <div key={provider.id}>
                                            <button
                                                type="button"
                                                className="mb-1 flex w-full items-center justify-between gap-3 rounded px-1 py-1 text-left text-theme-text-muted hover:bg-theme-bg-hover/40 hover:text-theme-text transition-colors"
                                                onClick={() => setCollapsedContextProviders((current) => ({
                                                    ...current,
                                                    [provider.id]: !current[provider.id],
                                                }))}
                                                aria-expanded={!providerCollapsed}
                                            >
                                                <span className="text-[10px] font-bold tracking-wider uppercase">{provider.name}</span>
                                                <span className="flex items-center gap-2 text-[10px] normal-case tracking-normal">
                                                    <span>
                                                        {t('settings_view.ai.ctx_provider_summary', {
                                                            count: provider.models.length,
                                                            overrides: userOverrideCount,
                                                        })}
                                                    </span>
                                                    {providerCollapsed
                                                        ? <ChevronRight className="h-3.5 w-3.5 shrink-0" />
                                                        : <ChevronDown className="h-3.5 w-3.5 shrink-0" />}
                                                </span>
                                            </button>
                                            <div className={cn('border border-theme-border/30 rounded-md overflow-hidden', providerCollapsed && 'hidden')}>
                                                {provider.models.map((model, index) => {
                                                    const info = getModelContextWindowInfo(model, ai.modelContextWindows, provider.id, ai.userContextWindows);
                                                    const hasUserOverride = !!ai.userContextWindows?.[provider.id]?.[model];

                                                    return (
                                                        <div key={model} className={cn('flex items-center gap-2 px-3 py-1.5', index > 0 && 'border-t border-theme-border/20', hasUserOverride && 'bg-theme-accent/5')}>
                                                            <span className="text-xs text-theme-text-muted font-mono flex-1 truncate min-w-0" title={model}>{model}</span>
                                                            <span
                                                                className={cn(
                                                                    'text-[9px] px-1.5 py-0.5 rounded shrink-0 font-medium',
                                                                    info.source === 'user' && 'text-blue-400 bg-blue-400/10',
                                                                    info.source === 'api' && 'text-emerald-400 bg-emerald-400/10',
                                                                    info.source === 'name' && 'text-cyan-400 bg-cyan-400/10',
                                                                    (info.source === 'pattern' || info.source === 'default') && 'text-theme-text-muted/70 bg-theme-border/20',
                                                                )}
                                                            >
                                                                {t(`settings_view.ai.ctx_source_${info.source}`)}
                                                            </span>
                                                            <Input
                                                                type="number"
                                                                min={1024}
                                                                max={10485760}
                                                                step={1024}
                                                                value={ai.userContextWindows?.[provider.id]?.[model] ?? info.value}
                                                                onChange={(event) => {
                                                                    const value = parseInt(event.target.value, 10);
                                                                    if (!Number.isNaN(value) && value >= 1024) {
                                                                        setUserContextWindow(provider.id, model, value);
                                                                    }
                                                                }}
                                                                className="w-28 h-7 bg-theme-bg border-theme-border text-xs text-right shrink-0"
                                                            />
                                                            <div className="w-4 shrink-0 flex items-center justify-center">
                                                                {hasUserOverride && (
                                                                    <button onClick={() => setUserContextWindow(provider.id, model, null)} title={t('settings_view.ai.ctx_reset')} className="text-theme-text-muted/60 hover:text-theme-text">
                                                                        <X className="w-3 h-3" />
                                                                    </button>
                                                                )}
                                                            </div>
                                                        </div>
                                                    );
                                                })}
                                            </div>
                                        </div>
                                        );
                                    })}
                                </div>
                            ))}
                        </div>
                    </div>

                    <Separator className="my-6 opacity-50" />

                    <div className={ai.enabled ? '' : 'opacity-50 pointer-events-none'}>
                        <div className="mb-4 flex items-center justify-between gap-3">
                            <h4 className="text-sm font-medium text-theme-text uppercase tracking-wider flex items-center gap-2">
                                <Wrench className="w-4 h-4" />
                                {t('settings_view.ai.tool_use')}
                            </h4>
                            <button
                                type="button"
                                onClick={() => setToolUseExpanded((expanded) => !expanded)}
                                className="inline-flex items-center gap-1.5 rounded-md border border-theme-border px-2.5 py-1 text-xs text-theme-text-muted hover:bg-theme-bg-hover/50 hover:text-theme-text transition-colors cursor-pointer"
                                aria-expanded={toolUseExpanded}
                                aria-controls="ai-tool-use-details"
                            >
                                {toolUseExpanded ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
                                {toolUseExpanded ? t('settings_view.ai.tool_use_collapse') : t('settings_view.ai.tool_use_expand')}
                            </button>
                        </div>

                        <div className="flex items-center justify-between mb-4">
                            <div>
                                <Label className="text-theme-text">{t('settings_view.ai.tool_use_enabled')}</Label>
                                <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.ai.tool_use_enabled_hint')}</p>
                            </div>
                            <Checkbox
                                id="tool-use-enabled"
                                checked={toolUse.enabled}
                                onCheckedChange={(checked) => updateAi('toolUse', { ...toolUse, enabled: !!checked })}
                            />
                        </div>

                        {!toolUseExpanded && (
                            <div className="ml-4 border-l border-theme-border/30 pl-4">
                                <p className="text-xs text-theme-text-muted">
                                    {t('settings_view.ai.tool_use_collapsed_summary', {
                                        approved: approvedToolCount,
                                        total: allToolNames.length,
                                    })}
                                </p>
                            </div>
                        )}

                        {toolUseExpanded && (
                        <div
                            id="ai-tool-use-details"
                            className={toolUse.enabled ? 'space-y-5 ml-4 pl-4 border-l border-theme-border/30' : 'opacity-40 pointer-events-none space-y-5 ml-4 pl-4 border-l border-theme-border/30'}
                        >
                            <p className="text-xs text-theme-text-muted">{t('settings_view.ai.tool_use_approve_hint')}</p>

                            <div className="flex gap-2">
                                <button
                                    type="button"
                                    onClick={() => {
                                        const all: Record<string, boolean> = {};
                                        for (const group of TOOL_GROUPS) {
                                            for (const name of [...group.readOnly, ...group.write]) {
                                                if (!EXPERIMENTAL_TOOLS.has(name)) all[name] = true;
                                            }
                                        }
                                        const current = toolUse.autoApproveTools ?? {};
                                        for (const name of EXPERIMENTAL_TOOLS) {
                                            if (current[name] !== undefined) all[name] = current[name];
                                        }
                                        updateAi('toolUse', { ...toolUse, autoApproveTools: all });
                                    }}
                                    className="text-xs px-3 py-1 rounded border border-theme-border text-theme-text-muted hover:bg-theme-bg-hover/50 transition-colors cursor-pointer"
                                >
                                    {t('settings_view.ai.tool_use_approve_all')}
                                </button>
                                <button
                                    type="button"
                                    onClick={() => {
                                        const none: Record<string, boolean> = {};
                                        for (const group of TOOL_GROUPS) {
                                            for (const name of [...group.readOnly, ...group.write]) {
                                                if (!EXPERIMENTAL_TOOLS.has(name)) none[name] = false;
                                            }
                                        }
                                        const current = toolUse.autoApproveTools ?? {};
                                        for (const name of EXPERIMENTAL_TOOLS) {
                                            if (current[name] !== undefined) none[name] = current[name];
                                        }
                                        updateAi('toolUse', { ...toolUse, autoApproveTools: none });
                                    }}
                                    className="text-xs px-3 py-1 rounded border border-theme-border text-theme-text-muted hover:bg-theme-bg-hover/50 transition-colors cursor-pointer"
                                >
                                    {t('settings_view.ai.tool_use_approve_none')}
                                </button>
                            </div>

                            {TOOL_GROUPS.map((group) => {
                                const GroupIcon = TOOL_GROUP_ICONS[group.groupKey] ?? Wrench;
                                const approveTools = toolUse.autoApproveTools ?? {};
                                const toggleTool = (toolName: string) => {
                                    const next = { ...approveTools, [toolName]: !approveTools[toolName] };
                                    updateAi('toolUse', { ...toolUse, autoApproveTools: next });
                                };

                                const renderToolButton = (toolName: string) => {
                                    const Icon = TOOL_ICON_MAP[toolName] ?? Wrench;
                                    const checked = approveTools[toolName] === true;
                                    const isWrite = WRITE_TOOLS.has(toolName);
                                    const isExperimental = EXPERIMENTAL_TOOLS.has(toolName);
                                    return (
                                        <button
                                            key={toolName}
                                            type="button"
                                            aria-pressed={checked}
                                            onClick={() => toggleTool(toolName)}
                                            className={cn(
                                                'flex items-center gap-2 rounded-md border px-3 py-2 text-xs transition-colors cursor-pointer select-none',
                                                checked
                                                    ? isExperimental
                                                        ? 'border-purple-500/60 bg-purple-500/10 text-purple-400'
                                                        : isWrite
                                                            ? 'border-amber-500/60 bg-amber-500/10 text-amber-400'
                                                            : 'border-theme-accent/60 bg-theme-accent/10 text-theme-accent'
                                                    : 'border-theme-border bg-theme-bg-panel/30 text-theme-text-muted hover:border-theme-border hover:bg-theme-bg-hover/50',
                                            )}
                                        >
                                            <Icon className="size-3.5 shrink-0" />
                                            <span className="truncate">{t(`ai.tool_use.tool_names.${toolName}`, { defaultValue: toolName })}</span>
                                            {isExperimental && <FlaskConical className="size-3 shrink-0 text-purple-400/70" />}
                                        </button>
                                    );
                                };

                                const isExperimentalGroup = [...group.readOnly, ...group.write].some((name) => EXPERIMENTAL_TOOLS.has(name));
                                return (
                                    <div key={group.groupKey}>
                                        <div className="flex items-center gap-1.5 mb-2">
                                            <GroupIcon className="size-3.5 text-theme-text-muted" />
                                            <span className="text-xs font-medium text-theme-text uppercase tracking-wider">{t(`settings_view.ai.tool_use_group_${group.groupKey}`)}</span>
                                            {isExperimentalGroup && (
                                                <span className="text-[9px] px-1.5 py-0.5 rounded-full bg-purple-500/15 text-purple-400 font-medium uppercase tracking-wider">
                                                    {t('settings_view.ai.experimental')}
                                                </span>
                                            )}
                                        </div>
                                        {group.readOnly.length > 0 && (
                                            <div className="mb-2">
                                                <span className="text-[10px] text-theme-text-muted/60 uppercase tracking-widest">{t('settings_view.ai.tool_use_subgroup_read_only')}</span>
                                                <div className="grid grid-cols-3 gap-1.5 mt-1">{group.readOnly.map(renderToolButton)}</div>
                                            </div>
                                        )}
                                        {group.write.length > 0 && (
                                            <div>
                                                <span className="text-[10px] text-amber-400/70 uppercase tracking-widest">{t('settings_view.ai.tool_use_subgroup_write')}</span>
                                                <div className="grid grid-cols-3 gap-1.5 mt-1">{group.write.map(renderToolButton)}</div>
                                            </div>
                                        )}
                                    </div>
                                );
                            })}

                            {(() => {
                                const approveTools = toolUse.autoApproveTools ?? {};
                                const anyWriteApproved = [...WRITE_TOOLS].some((name) => approveTools[name] === true);
                                return anyWriteApproved ? (
                                    <div className="p-3 rounded bg-amber-500/10 border border-amber-500/20">
                                        <p className="text-xs text-amber-400 leading-relaxed"><span className="font-semibold">⚠</span> {t('settings_view.ai.tool_use_write_warning')}</p>
                                    </div>
                                ) : null;
                            })()}
                        </div>
                        )}
                    </div>
                </div>
            </div>

            <McpServersPanel />
            {ConfirmDialog}
        </>
    );
};
