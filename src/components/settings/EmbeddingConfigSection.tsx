// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronDown, ChevronRight, SlidersHorizontal, Sparkles } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { cn } from '@/lib/utils';
import {
  getEmbeddingProviderCandidates,
  requiresEmbeddingApiKey,
  resolveEmbeddingProvider,
} from '@/lib/ai/embeddingConfig';
import { api } from '@/lib/api';
import type { AiSettings } from '@/store/settingsStore';

type EmbeddingConfigSectionProps = {
  ai: AiSettings;
  updateAi: <K extends keyof AiSettings>(key: K, value: AiSettings[K]) => void;
  expanded: boolean;
  onExpandedChange: (expanded: boolean) => void;
};

export function EmbeddingConfigSection({
  ai,
  updateAi,
  expanded,
  onExpandedChange,
}: EmbeddingConfigSectionProps) {
  const { t } = useTranslation();
  const baseResolved = resolveEmbeddingProvider(ai);
  const [hasApiKey, setHasApiKey] = useState<boolean | null>(null);

  useEffect(() => {
    const provider = baseResolved.providerConfig;
    if (!provider || !requiresEmbeddingApiKey(provider)) {
      setHasApiKey(null);
      return;
    }

    let cancelled = false;
    const refreshKeyState = () => {
      setHasApiKey(null);
      api.hasAiProviderApiKey(provider.id)
        .then((hasKey) => {
          if (!cancelled) {
            setHasApiKey(hasKey);
          }
        })
        .catch(() => {
          if (!cancelled) {
            setHasApiKey(false);
          }
        });
    };

    refreshKeyState();
    window.addEventListener('ai-api-key-updated', refreshKeyState);
    return () => {
      cancelled = true;
      window.removeEventListener('ai-api-key-updated', refreshKeyState);
    };
  }, [baseResolved.providerConfig?.id, baseResolved.providerConfig?.type]);

  const resolved = resolveEmbeddingProvider(ai, { hasApiKey });
  const embeddingProvider = resolved.providerConfig;
  const embeddingProviders = getEmbeddingProviderCandidates(ai);
  const modelPlaceholder = embeddingProvider?.type === 'ollama'
    ? 'nomic-embed-text'
    : 'text-embedding-3-small';
  const semanticStatus = (() => {
    if (resolved.reason === 'ready' && embeddingProvider) {
      return t('settings_view.knowledge.semantic_search_using', {
        provider: embeddingProvider.name,
        model: resolved.model,
      });
    }
    if (resolved.reason === 'unsupported_provider') {
      return t('settings_view.knowledge.embedding_provider_unsupported');
    }
    if (resolved.reason === 'missing_model') {
      return t('settings_view.knowledge.semantic_search_missing_model');
    }
    if (resolved.reason === 'missing_api_key') {
      return t('settings_view.knowledge.embedding_api_key_missing');
    }
    if (embeddingProvider && requiresEmbeddingApiKey(embeddingProvider) && hasApiKey === null) {
      return t('settings_view.knowledge.embedding_api_key_checking');
    }
    return t('settings_view.knowledge.semantic_search_not_configured');
  })();

  return (
    <div className="rounded-lg border border-theme-border bg-theme-bg-card/80">
      <div className="flex flex-col gap-3 p-4 md:flex-row md:items-center md:justify-between">
        <div className="flex items-start gap-3">
          <div className="mt-0.5 rounded-md border border-theme-border bg-theme-bg px-2 py-1">
            <Sparkles className="h-4 w-4 text-theme-accent" />
          </div>
          <div>
            <div className="text-sm font-medium text-theme-text">
              {t('settings_view.knowledge.semantic_search')}
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-theme-text-muted">
              <span>{t('settings_view.knowledge.keyword_search_ready')}</span>
              <span
                className={cn(
                  'rounded-full px-2 py-0.5',
                  resolved.reason === 'ready'
                    ? 'bg-emerald-500/10 text-emerald-400'
                    : 'bg-yellow-500/10 text-yellow-400',
                )}
              >
                {semanticStatus}
              </span>
            </div>
          </div>
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => onExpandedChange(!expanded)}
          className="self-start md:self-auto"
        >
          <SlidersHorizontal className="mr-1.5 h-3.5 w-3.5" />
          {t('settings_view.knowledge.configure_embeddings')}
          {expanded ? <ChevronDown className="ml-1.5 h-3.5 w-3.5" /> : <ChevronRight className="ml-1.5 h-3.5 w-3.5" />}
        </Button>
      </div>

      {expanded && (
        <div className="border-t border-theme-border p-4">
          <p className="mb-4 text-sm text-theme-text-muted">
            {t('settings_view.knowledge.semantic_search_description')}
          </p>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 max-w-3xl">
            <div className="grid gap-1">
              <Label className="text-xs text-theme-text-muted">
                {t('settings_view.ai.embedding_provider')}
              </Label>
              <Select
                value={ai.embeddingConfig?.providerId ?? '__default__'}
                onValueChange={(value) => updateAi('embeddingConfig', {
                  ...ai.embeddingConfig,
                  providerId: value === '__default__' ? null : value,
                  model: ai.embeddingConfig?.model ?? '',
                })}
              >
                <SelectTrigger className="bg-theme-bg h-8 text-xs">
                  <SelectValue placeholder={t('settings_view.ai.embedding_provider_placeholder')} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="__default__">
                    {t('settings_view.knowledge.auto_embedding_provider')}
                  </SelectItem>
                  {embeddingProviders.map((provider) => (
                    <SelectItem key={provider.id} value={provider.id}>
                      {provider.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-1">
              <Label className="text-xs text-theme-text-muted">
                {t('settings_view.ai.embedding_model')}
              </Label>
              <Input
                value={ai.embeddingConfig?.model ?? ''}
                onChange={(event) => updateAi('embeddingConfig', {
                  ...ai.embeddingConfig,
                  providerId: ai.embeddingConfig?.providerId ?? null,
                  model: event.target.value,
                })}
                className="bg-theme-bg h-8 text-xs"
                placeholder={modelPlaceholder}
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
