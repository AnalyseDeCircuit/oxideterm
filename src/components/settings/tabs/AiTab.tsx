// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Brain, ChevronDown, ChevronRight, Copy, Plus, RefreshCw, Trash2, Wrench, X } from 'lucide-react';
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
import { getModelContextWindowInfo } from '@/lib/ai/tokenUtils';
import { api } from '@/lib/api';
import { cn } from '@/lib/utils';
import type { AiProvider, AiProviderType } from '@/types';
import type { AiReasoningEffort } from '@/lib/ai/providers';
import {
    DEFAULT_AI_TOOL_MAX_ROUNDS,
    MAX_AI_TOOL_MAX_ROUNDS,
    MIN_AI_TOOL_MAX_ROUNDS,
    normalizeAiToolMaxRounds,
    type AiSettings,
} from '@/store/settingsStore';
import { createDefaultExecutionProfile, type AiExecutionProfile, type AiExecutionProfilesConfig } from '@/lib/ai/profiles';

type AiTabProps = {
    ai: AiSettings;
    updateAi: <K extends keyof AiSettings>(key: K, value: AiSettings[K]) => void;
    addProvider: (provider: AiProvider) => void;
    removeProvider: (providerId: string) => void;
    updateProvider: (providerId: string, patch: Partial<AiSettings['providers'][number]>) => void;
    setActiveProvider: (providerId: string) => void;
    refreshProviderModels: (providerId: string) => Promise<string[]>;
    setUserContextWindow: (providerId: string, model: string, value: number | null) => void;
    setProviderReasoningEffort: (providerId: string, value: AiReasoningEffort | null) => void;
    setModelReasoningEffort: (providerId: string, model: string, value: AiReasoningEffort | null) => void;
    refreshingModels: string | null;
    setRefreshingModels: (providerId: string | null) => void;
    onRequestEnableAiConfirm: () => void;
};

type ProviderTemplate = {
    type: AiProviderType;
    nameKey: string;
    baseUrl: string;
    defaultModel: string;
};

type ToolPolicyItem = {
    label: string;
    checked: boolean;
    locked?: boolean;
    onChange?: (checked: boolean) => void;
};

type ToolPolicyGroup = {
    title: string;
    description: string;
    className?: string;
    items: ToolPolicyItem[];
};

const PROVIDER_TEMPLATES: ProviderTemplate[] = [
    {
        type: 'openai_compatible',
        nameKey: 'settings_view.ai.provider_template_openai_compatible',
        baseUrl: 'https://',
        defaultModel: '',
    },
    {
        type: 'deepseek',
        nameKey: 'settings_view.ai.provider_template_deepseek',
        baseUrl: 'https://api.deepseek.com',
        defaultModel: 'deepseek-v4-flash',
    },
    {
        type: 'openai',
        nameKey: 'settings_view.ai.provider_template_openai',
        baseUrl: 'https://api.openai.com/v1',
        defaultModel: 'gpt-4o-mini',
    },
    {
        type: 'anthropic',
        nameKey: 'settings_view.ai.provider_template_anthropic',
        baseUrl: 'https://api.anthropic.com',
        defaultModel: 'claude-sonnet-4-20250514',
    },
    {
        type: 'gemini',
        nameKey: 'settings_view.ai.provider_template_gemini',
        baseUrl: 'https://generativelanguage.googleapis.com/v1beta',
        defaultModel: 'gemini-2.0-flash',
    },
    {
        type: 'ollama',
        nameKey: 'settings_view.ai.provider_template_ollama',
        baseUrl: 'http://localhost:11434',
        defaultModel: '',
    },
];

const REASONING_EFFORTS: AiReasoningEffort[] = ['auto', 'off', 'low', 'medium', 'high', 'max'];
const INHERIT_REASONING = '__inherit__';

type ReasoningSelectValue = AiReasoningEffort | typeof INHERIT_REASONING;

function reasoningValueOrNull(value: string): AiReasoningEffort | null {
    return value === INHERIT_REASONING ? null : value as AiReasoningEffort;
}

export const AiTab = ({
    ai,
    updateAi,
    addProvider,
    removeProvider,
    updateProvider,
    setActiveProvider,
    refreshProviderModels,
    setUserContextWindow,
    setProviderReasoningEffort,
    setModelReasoningEffort,
    refreshingModels,
    setRefreshingModels,
    onRequestEnableAiConfirm,
}: AiTabProps) => {
    const { t } = useTranslation();
    const { error: toastError } = useToast();
    const { confirm, ConfirmDialog } = useConfirm();
    const [contextWindowsExpanded, setContextWindowsExpanded] = useState(true);
    const [collapsedContextProviders, setCollapsedContextProviders] = useState<Record<string, boolean>>({});
    const [modelReasoningExpanded, setModelReasoningExpanded] = useState(false);
    const [collapsedReasoningProviders, setCollapsedReasoningProviders] = useState<Record<string, boolean>>({});
    const [providerSettingsExpanded, setProviderSettingsExpanded] = useState(true);
    const [expandedProviders, setExpandedProviders] = useState<Record<string, boolean>>({});
    const [expandedProviderModels, setExpandedProviderModels] = useState<Record<string, boolean>>({});
    const [toolUseExpanded, setToolUseExpanded] = useState(true);
    const toolUseSectionRef = useRef<HTMLDivElement | null>(null);
    const [newProviderType, setNewProviderType] = useState<AiProviderType>('openai_compatible');
    const memory = ai.memory ?? { enabled: true, content: '' };
    const toolUse = ai.toolUse ?? { enabled: false, autoApproveTools: {}, disabledTools: [], maxRounds: DEFAULT_AI_TOOL_MAX_ROUNDS };
    const toolUseMaxRounds = normalizeAiToolMaxRounds(toolUse.maxRounds);
    const approveTools = toolUse.autoApproveTools ?? {};
    const setToolApproval = (toolName: string, approved: boolean) => {
        updateAi('toolUse', {
            ...toolUse,
            autoApproveTools: { ...approveTools, [toolName]: approved },
        });
    };
    const selectedProviderTemplate = PROVIDER_TEMPLATES.find((template) => template.type === newProviderType) ?? PROVIDER_TEMPLATES[0];
    const profilesConfig: AiExecutionProfilesConfig = ai.executionProfiles ?? {
        defaultProfileId: 'default',
        profiles: [
            createDefaultExecutionProfile({
                providerId: ai.activeProviderId,
                model: ai.activeModel,
                reasoningEffort: ai.reasoningEffort,
                toolUse,
            }),
        ],
    };
    const updateProfiles = (profiles: AiExecutionProfile[], defaultProfileId = profilesConfig.defaultProfileId) => {
        updateAi('executionProfiles', {
            defaultProfileId: profiles.some((profile) => profile.id === defaultProfileId)
                ? defaultProfileId
                : profiles[0]?.id ?? 'default',
            profiles,
        });
    };
    const patchProfile = (profileId: string, patch: Partial<AiExecutionProfile>) => {
        updateProfiles(profilesConfig.profiles.map((profile) => (
            profile.id === profileId ? { ...profile, ...patch, updatedAt: Date.now() } : profile
        )));
    };
    const addProfile = () => {
        const now = Date.now();
        const profile: AiExecutionProfile = {
            id: crypto.randomUUID(),
            name: `Profile ${profilesConfig.profiles.length + 1}`,
            providerId: ai.activeProviderId,
            model: ai.activeModel,
            reasoningEffort: ai.reasoningEffort,
            toolUse: { ...toolUse, autoApproveTools: { ...approveTools } },
            context: { includeRuntimeChips: true, includeMemory: true, includeRag: true },
            commandPolicy: { allow: [], deny: [] },
            createdAt: now,
            updatedAt: now,
        };
        updateProfiles([...profilesConfig.profiles, profile], profile.id);
    };
    const duplicateProfile = (profile: AiExecutionProfile) => {
        const now = Date.now();
        const copy: AiExecutionProfile = {
            ...profile,
            id: crypto.randomUUID(),
            name: `${profile.name} Copy`,
            createdAt: now,
            updatedAt: now,
            toolUse: profile.toolUse ? {
                ...profile.toolUse,
                autoApproveTools: { ...(profile.toolUse.autoApproveTools ?? {}) },
                disabledTools: [...(profile.toolUse.disabledTools ?? [])],
            } : undefined,
            context: profile.context ? { ...profile.context } : undefined,
            commandPolicy: profile.commandPolicy ? {
                allow: [...(profile.commandPolicy.allow ?? [])],
                deny: [...(profile.commandPolicy.deny ?? [])],
            } : undefined,
        };
        updateProfiles([...profilesConfig.profiles, copy], copy.id);
    };
    const deleteProfile = (profileId: string) => {
        if (profilesConfig.profiles.length <= 1) return;
        updateProfiles(profilesConfig.profiles.filter((profile) => profile.id !== profileId));
    };

    useEffect(() => {
        const handleFocusSettingsSection = (event: Event) => {
            const detail = (event as CustomEvent<{ tab?: string; section?: string }>).detail;
            if (detail?.tab !== 'ai' || detail.section !== 'tool-use') {
                return;
            }
            setToolUseExpanded(true);
            window.requestAnimationFrame(() => {
                toolUseSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
            });
        };

        window.addEventListener('oxideterm:focus-settings-section', handleFocusSettingsSection);
        return () => window.removeEventListener('oxideterm:focus-settings-section', handleFocusSettingsSection);
    }, []);

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
                            <div className="mb-6 max-w-3xl rounded-lg border border-theme-border/70 bg-theme-bg/60 p-4">
                                <div className="mb-3 flex items-center justify-between gap-3">
                                    <div>
                                        <h4 className="text-sm font-medium uppercase tracking-wider text-theme-text">
                                            {t('settings_view.ai.execution_profiles', { defaultValue: 'Execution Profiles' })}
                                        </h4>
                                        <p className="mt-1 text-xs text-theme-text-muted">
                                            {t('settings_view.ai.execution_profiles_hint', { defaultValue: 'Bundle model, reasoning, tool policy, context chips, and memory/RAG preferences.' })}
                                        </p>
                                    </div>
                                    <Button type="button" variant="outline" size="sm" onClick={addProfile} className="gap-1.5">
                                        <Plus className="h-3.5 w-3.5" />
                                        {t('settings_view.ai.profile_add', { defaultValue: 'New profile' })}
                                    </Button>
                                </div>
                                <div className="space-y-2">
                                    {profilesConfig.profiles.map((profile) => (
                                        <div key={profile.id} className="rounded-md border border-theme-border/45 bg-theme-bg-card/45 p-3">
                                            <div className="flex flex-wrap items-center gap-2">
                                                <Input
                                                    value={profile.name}
                                                    onChange={(event) => patchProfile(profile.id, { name: event.currentTarget.value })}
                                                    className="h-8 min-w-[180px] flex-1"
                                                />
                                                <Button
                                                    type="button"
                                                    variant={profilesConfig.defaultProfileId === profile.id ? 'default' : 'outline'}
                                                    size="sm"
                                                    onClick={() => updateProfiles(profilesConfig.profiles, profile.id)}
                                                >
                                                    {profilesConfig.defaultProfileId === profile.id
                                                        ? t('settings_view.ai.profile_default', { defaultValue: 'Default' })
                                                        : t('settings_view.ai.profile_set_default', { defaultValue: 'Set default' })}
                                                </Button>
                                                <Button type="button" variant="ghost" size="icon" onClick={() => duplicateProfile(profile)}>
                                                    <Copy className="h-3.5 w-3.5" />
                                                </Button>
                                                <Button
                                                    type="button"
                                                    variant="ghost"
                                                    size="icon"
                                                    disabled={profilesConfig.profiles.length <= 1}
                                                    onClick={() => deleteProfile(profile.id)}
                                                >
                                                    <Trash2 className="h-3.5 w-3.5 text-red-400" />
                                                </Button>
                                            </div>
                                            <div className="mt-3 grid gap-2 md:grid-cols-3">
                                                <Select
                                                    value={profile.providerId ?? 'inherit'}
                                                    onValueChange={(value) => patchProfile(profile.id, {
                                                        providerId: value === 'inherit' ? null : value,
                                                        model: value === 'inherit'
                                                            ? null
                                                            : ai.providers.find((provider) => provider.id === value)?.defaultModel ?? null,
                                                    })}
                                                >
                                                    <SelectTrigger className="h-8">
                                                        <SelectValue />
                                                    </SelectTrigger>
                                                    <SelectContent>
                                                        <SelectItem value="inherit">{t('settings_view.ai.profile_inherit_provider', { defaultValue: 'Use active provider' })}</SelectItem>
                                                        {ai.providers.map((provider) => (
                                                            <SelectItem key={provider.id} value={provider.id}>{provider.name}</SelectItem>
                                                        ))}
                                                    </SelectContent>
                                                </Select>
                                                <Input
                                                    value={profile.model ?? ''}
                                                    placeholder={t('settings_view.ai.profile_inherit_model', { defaultValue: 'Use provider default' })}
                                                    onChange={(event) => patchProfile(profile.id, { model: event.currentTarget.value || null })}
                                                    className="h-8"
                                                />
                                                <Select
                                                    value={profile.reasoningEffort}
                                                    onValueChange={(value) => patchProfile(profile.id, { reasoningEffort: value as AiReasoningEffort })}
                                                >
                                                    <SelectTrigger className="h-8">
                                                        <SelectValue />
                                                    </SelectTrigger>
                                                    <SelectContent>
                                                        {(['auto', 'off', 'low', 'medium', 'high', 'max'] as AiReasoningEffort[]).map((effort) => (
                                                            <SelectItem key={effort} value={effort}>
                                                                {t(`settings_view.ai.reasoning_${effort}`, { defaultValue: effort })}
                                                            </SelectItem>
                                                        ))}
                                                    </SelectContent>
                                                </Select>
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            </div>

	                        <button
	                            type="button"
                            className="mb-4 flex w-full max-w-3xl items-center justify-between gap-3 rounded-md px-1 py-1 text-left text-theme-text-muted hover:bg-theme-bg-hover/40 hover:text-theme-text transition-colors"
                            onClick={() => setProviderSettingsExpanded((current) => !current)}
                            aria-expanded={providerSettingsExpanded}
                        >
                            <div>
                                <h4 className="text-sm font-medium text-theme-text uppercase tracking-wider">{t('settings_view.ai.provider_settings')}</h4>
                                <p className="mt-1 text-xs text-theme-text-muted">
                                    {t('settings_view.ai.provider_settings_summary', { count: ai.providers.length })}
                                </p>
                            </div>
                            {providerSettingsExpanded
                                ? <ChevronDown className="mt-0.5 h-4 w-4 shrink-0" />
                                : <ChevronRight className="mt-0.5 h-4 w-4 shrink-0" />}
                        </button>

                        {providerSettingsExpanded && <div className="space-y-3 max-w-3xl mb-6">
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
                                                    <Label className="text-xs text-theme-text-muted">{t('settings_view.ai.provider_name')}</Label>
                                                    <Input
                                                        value={provider.name}
                                                        onChange={(event) => updateProvider(provider.id, { name: event.target.value })}
                                                        onBlur={(event) => {
                                                            const trimmedName = event.target.value.trim();
                                                            if (trimmedName && trimmedName !== provider.name) {
                                                                updateProvider(provider.id, { name: trimmedName });
                                                            }
                                                        }}
                                                        className="bg-theme-bg h-8 text-xs"
                                                        placeholder={t('settings_view.ai.provider_name')}
                                                    />
                                                </div>
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
                                                <div className="grid gap-1">
                                                    <Label className="text-xs text-theme-text-muted">{t('settings_view.ai.reasoning_provider_default')}</Label>
                                                    <Select
                                                        value={(ai.reasoningProviderOverrides?.[provider.id] ?? INHERIT_REASONING) as ReasoningSelectValue}
                                                        onValueChange={(value) => setProviderReasoningEffort(provider.id, reasoningValueOrNull(value))}
                                                    >
                                                        <SelectTrigger className="bg-theme-bg h-8 text-xs">
                                                            <SelectValue />
                                                        </SelectTrigger>
                                                        <SelectContent>
                                                            <SelectItem value={INHERIT_REASONING}>
                                                                {t('settings_view.ai.reasoning_inherit_global', {
                                                                    value: t(`settings_view.ai.reasoning_${ai.reasoningEffort ?? 'auto'}`),
                                                                })}
                                                            </SelectItem>
                                                            {REASONING_EFFORTS.map((effort) => (
                                                                <SelectItem key={effort} value={effort}>
                                                                    {t(`settings_view.ai.reasoning_${effort}`)}
                                                                </SelectItem>
                                                            ))}
                                                        </SelectContent>
                                                    </Select>
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
                        </div>}

                        {providerSettingsExpanded && <div className="mb-6 flex flex-wrap items-end gap-3">
                            <div className="grid gap-1">
                                <Label className="text-xs text-theme-text-muted">{t('settings_view.ai.provider_template')}</Label>
                                <Select value={newProviderType} onValueChange={(value) => setNewProviderType(value as AiProviderType)}>
                                    <SelectTrigger className="w-56 bg-theme-bg h-8 text-xs">
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {PROVIDER_TEMPLATES.map((template) => (
                                            <SelectItem key={template.type} value={template.type}>
                                                {t(template.nameKey)}
                                            </SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            </div>
                            <Button
                                variant="outline"
                                size="sm"
                                onClick={() => {
                                    const id = `custom-${selectedProviderTemplate.type}-${Date.now()}`;
                                    addProvider({
                                        id,
                                        type: selectedProviderTemplate.type,
                                        name: t(selectedProviderTemplate.nameKey),
                                        baseUrl: selectedProviderTemplate.baseUrl,
                                        defaultModel: selectedProviderTemplate.defaultModel,
                                        models: selectedProviderTemplate.defaultModel ? [selectedProviderTemplate.defaultModel] : [],
                                        enabled: true,
                                        createdAt: Date.now(),
                                    });
                                }}
                            >
                                + {t('settings_view.ai.add_provider')}
                            </Button>
                        </div>}

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

                        <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.ai.reasoning_title')}</h4>
                        <div className="max-w-3xl grid gap-2">
                            <Select
                                value={ai.reasoningEffort ?? 'auto'}
                                onValueChange={(value) => updateAi('reasoningEffort', value as AiReasoningEffort)}
                            >
                                <SelectTrigger className="bg-theme-bg">
                                    <SelectValue />
                                </SelectTrigger>
                                <SelectContent>
                                    {REASONING_EFFORTS.map((effort) => (
                                        <SelectItem key={effort} value={effort}>
                                            {t(`settings_view.ai.reasoning_${effort}`)}
                                        </SelectItem>
                                    ))}
                                </SelectContent>
                            </Select>
                            <p className="text-xs text-theme-text-muted">{t('settings_view.ai.reasoning_hint')}</p>
                        </div>
                        <div className="mt-4 max-w-3xl">
                            <button
                                type="button"
                                className="mb-3 flex w-full items-start justify-between gap-3 rounded-md px-1 py-1 text-left text-theme-text-muted hover:bg-theme-bg-hover/40 hover:text-theme-text transition-colors"
                                onClick={() => setModelReasoningExpanded((current) => !current)}
                                aria-expanded={modelReasoningExpanded}
                            >
                                <div>
                                    <h5 className="text-xs font-medium uppercase tracking-wider text-theme-text">
                                        {t('settings_view.ai.model_reasoning_overrides')}
                                    </h5>
                                    <p className="mt-1 text-xs text-theme-text-muted">{t('settings_view.ai.model_reasoning_overrides_hint')}</p>
                                </div>
                                {modelReasoningExpanded
                                    ? <ChevronDown className="mt-0.5 h-4 w-4 shrink-0" />
                                    : <ChevronRight className="mt-0.5 h-4 w-4 shrink-0" />}
                            </button>

                            {modelReasoningExpanded && (ai.providers.every((provider) => provider.models.length === 0) ? (
                                <p className="text-xs text-theme-text-muted italic">{t('settings_view.ai.model_reasoning_overrides_empty')}</p>
                            ) : (
                                <div className="space-y-4">
                                    {ai.providers.filter((provider) => provider.models.length > 0).map((provider) => {
                                        const providerCollapsed = collapsedReasoningProviders[provider.id] ?? true;
                                        const overrideCount = provider.models.filter((model) => !!ai.reasoningModelOverrides?.[provider.id]?.[model]).length;

                                        return (
                                            <div key={provider.id}>
                                                <button
                                                    type="button"
                                                    className="mb-1 flex w-full items-center justify-between gap-3 rounded px-1 py-1 text-left text-theme-text-muted hover:bg-theme-bg-hover/40 hover:text-theme-text transition-colors"
                                                    onClick={() => setCollapsedReasoningProviders((current) => ({
                                                        ...current,
                                                        [provider.id]: !(current[provider.id] ?? true),
                                                    }))}
                                                    aria-expanded={!providerCollapsed}
                                                >
                                                    <span className="text-[10px] font-bold tracking-wider uppercase">{provider.name}</span>
                                                    <span className="flex items-center gap-2 text-[10px] normal-case tracking-normal">
                                                        <span>
                                                            {t('settings_view.ai.model_reasoning_provider_summary', {
                                                                count: provider.models.length,
                                                                overrides: overrideCount,
                                                            })}
                                                        </span>
                                                        {providerCollapsed
                                                            ? <ChevronRight className="h-3.5 w-3.5 shrink-0" />
                                                            : <ChevronDown className="h-3.5 w-3.5 shrink-0" />}
                                                    </span>
                                                </button>
                                                <div className={cn('border border-theme-border/30 rounded-md overflow-hidden', providerCollapsed && 'hidden')}>
                                                    {provider.models.map((model, index) => (
                                                        <div key={model} className={cn('flex items-center gap-2 px-3 py-1.5', index > 0 && 'border-t border-theme-border/20')}>
                                                            <span className="text-xs text-theme-text-muted font-mono flex-1 truncate min-w-0" title={model}>{model}</span>
                                                            <Select
                                                                value={(ai.reasoningModelOverrides?.[provider.id]?.[model] ?? INHERIT_REASONING) as ReasoningSelectValue}
                                                                onValueChange={(value) => setModelReasoningEffort(provider.id, model, reasoningValueOrNull(value))}
                                                            >
                                                                <SelectTrigger className="w-40 h-7 bg-theme-bg border-theme-border text-[10px] shrink-0">
                                                                    <SelectValue />
                                                                </SelectTrigger>
                                                                <SelectContent>
                                                                    <SelectItem value={INHERIT_REASONING}>{t('settings_view.ai.reasoning_inherit_provider')}</SelectItem>
                                                                    {REASONING_EFFORTS.map((effort) => (
                                                                        <SelectItem key={effort} value={effort}>
                                                                            {t(`settings_view.ai.reasoning_${effort}`)}
                                                                        </SelectItem>
                                                                    ))}
                                                                </SelectContent>
                                                            </Select>
                                                        </div>
                                                    ))}
                                                </div>
                                            </div>
                                        );
                                    })}
                                </div>
                            ))}
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
                                        const providerCollapsed = collapsedContextProviders[provider.id] ?? true;
                                        const userOverrideCount = provider.models.filter((model) => !!ai.userContextWindows?.[provider.id]?.[model]).length;

                                        return (
                                        <div key={provider.id}>
                                            <button
                                                type="button"
                                                className="mb-1 flex w-full items-center justify-between gap-3 rounded px-1 py-1 text-left text-theme-text-muted hover:bg-theme-bg-hover/40 hover:text-theme-text transition-colors"
                                                onClick={() => setCollapsedContextProviders((current) => ({
                                                    ...current,
                                                    [provider.id]: !(current[provider.id] ?? true),
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

                    <div ref={toolUseSectionRef} className={ai.enabled ? '' : 'opacity-50 pointer-events-none'}>
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
                                    {t('settings_view.ai.tool_use_policy_summary')}
                                </p>
                            </div>
                        )}

                        {toolUseExpanded && (
                        <div
                            id="ai-tool-use-details"
                            className={toolUse.enabled ? 'space-y-5 ml-4 pl-4 border-l border-theme-border/30' : 'opacity-40 pointer-events-none space-y-5 ml-4 pl-4 border-l border-theme-border/30'}
                        >
                            <p className="text-xs text-theme-text-muted">
                                {t('settings_view.ai.tool_use_approve_hint')}
                            </p>

                            <div className="rounded-lg border border-theme-border/60 bg-theme-bg-panel/30 p-3">
                                <div className="flex items-center justify-between gap-4">
                                    <div className="min-w-0">
                                        <Label htmlFor="ai-tool-max-rounds" className="text-theme-text">
                                            {t('settings_view.ai.tool_use_max_rounds')}
                                        </Label>
                                        <p className="mt-0.5 text-xs text-theme-text-muted">
                                            {t('settings_view.ai.tool_use_max_rounds_hint')}
                                        </p>
                                    </div>
                                    <Input
                                        id="ai-tool-max-rounds"
                                        type="number"
                                        min={MIN_AI_TOOL_MAX_ROUNDS}
                                        max={MAX_AI_TOOL_MAX_ROUNDS}
                                        step={1}
                                        value={toolUseMaxRounds}
                                        onChange={(event) => {
                                            const next = normalizeAiToolMaxRounds(Number(event.currentTarget.value));
                                            updateAi('toolUse', { ...toolUse, maxRounds: next });
                                        }}
                                        className="h-9 w-24 text-right"
                                    />
                                </div>
                            </div>

                            <div className="grid gap-3 md:grid-cols-2">
                                {([
                                    {
                                        title: t('settings_view.ai.tool_policy_read_title'),
                                        description: t('settings_view.ai.tool_policy_read_desc'),
                                        items: [
                                            {
                                                label: t('settings_view.ai.tool_policy_read_auto'),
                                                checked: true,
                                                locked: true,
                                            },
                                        ],
                                    },
                                    {
                                        title: t('settings_view.ai.tool_policy_execute_title'),
                                        description: t('settings_view.ai.tool_policy_execute_desc'),
                                        items: [
                                            {
                                                label: t('settings_view.ai.tool_policy_execute_run_command'),
                                                checked: approveTools.run_command === true,
                                                onChange: (checked: boolean) => setToolApproval('run_command', checked),
                                            },
                                        ],
                                    },
                                    {
                                        title: t('settings_view.ai.tool_policy_interactive_title'),
                                        description: t('settings_view.ai.tool_policy_interactive_desc'),
                                        items: [
                                            {
                                                label: t('settings_view.ai.tool_policy_interactive_send_input'),
                                                checked: approveTools.send_terminal_input === true,
                                                onChange: (checked: boolean) => setToolApproval('send_terminal_input', checked),
                                            },
                                        ],
                                    },
                                    {
                                        title: t('settings_view.ai.tool_policy_navigation_title'),
                                        description: t('settings_view.ai.tool_policy_navigation_desc'),
                                        items: [
                                            {
                                                label: t('settings_view.ai.tool_policy_connect_target'),
                                                checked: approveTools.connect_target === true,
                                                onChange: (checked: boolean) => setToolApproval('connect_target', checked),
                                            },
                                            {
                                                label: t('settings_view.ai.tool_policy_open_surface'),
                                                checked: approveTools.open_app_surface === true,
                                                onChange: (checked: boolean) => setToolApproval('open_app_surface', checked),
                                            },
                                        ],
                                    },
                                    {
                                        title: t('settings_view.ai.tool_policy_write_title'),
                                        description: t('settings_view.ai.tool_policy_write_desc'),
                                        className: 'md:col-span-2',
                                        items: [
                                            {
                                                label: t('settings_view.ai.tool_policy_write_settings'),
                                                checked: approveTools['write_resource:settings'] === true,
                                                onChange: (checked: boolean) => setToolApproval('write_resource:settings', checked),
                                            },
                                            {
                                                label: t('settings_view.ai.tool_policy_write_file'),
                                                checked: approveTools['write_resource:file'] === true,
                                                onChange: (checked: boolean) => setToolApproval('write_resource:file', checked),
                                            },
                                            {
                                                label: t('settings_view.ai.tool_policy_transfer_resource'),
                                                checked: approveTools.transfer_resource === true,
                                                onChange: (checked: boolean) => setToolApproval('transfer_resource', checked),
                                            },
                                            {
                                                label: t('settings_view.ai.tool_policy_remember_preference'),
                                                checked: approveTools.remember_preference === true,
                                                onChange: (checked: boolean) => setToolApproval('remember_preference', checked),
                                            },
                                        ],
                                    },
                                ] as ToolPolicyGroup[]).map((policy) => (
                                    <div key={policy.title} className={cn('rounded-lg border border-theme-border/60 bg-theme-bg-panel/30 p-3', policy.className)}>
                                        <div className="min-w-0">
                                            <p className="text-sm font-medium text-theme-text">{policy.title}</p>
                                            <p className="mt-1 text-xs leading-relaxed text-theme-text-muted">{policy.description}</p>
                                        </div>
                                        <div className="mt-3 grid gap-2">
                                            {policy.items.map((item) => (
                                                <label
                                                    key={item.label}
                                                    className="flex items-center justify-between gap-3 rounded-md border border-theme-border/30 bg-theme-bg/25 px-2.5 py-2 text-xs text-theme-text-muted"
                                                >
                                                    <span>{item.label}</span>
                                                    <Checkbox
                                                        checked={item.checked}
                                                        disabled={item.locked}
                                                        onCheckedChange={(checked) => item.onChange?.(!!checked)}
                                                    />
                                                </label>
                                            ))}
                                        </div>
                                    </div>
                                ))}
                            </div>

                            <div className="p-3 rounded bg-amber-500/10 border border-amber-500/20">
                                <p className="text-xs text-amber-400 leading-relaxed">
                                    {t('settings_view.ai.tool_policy_warning')}
                                </p>
                            </div>
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
