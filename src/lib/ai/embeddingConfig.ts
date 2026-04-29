// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiProvider } from '@/types';
import type { AiSettings } from '@/store/settingsStore';
import { getProvider } from '@/lib/ai/providerRegistry';
import type { AiStreamProvider } from '@/lib/ai/providers';

const EMBEDDING_PROVIDER_TYPES = new Set<AiProvider['type']>([
  'openai',
  'openai_compatible',
  'ollama',
]);

const LIKELY_EMBEDDING_MODEL_PATTERN = /(?:embedding|embed|bge|e5|gte|nomic|jina|m3e|sbert|snowflake-arctic-embed)/i;

export type ResolvedEmbeddingProvider = {
  providerConfig: AiProvider | null;
  provider: AiStreamProvider | null;
  model: string;
  mode: 'configured' | 'auto';
  reason: 'ready' | 'no_provider' | 'unsupported_provider' | 'missing_model' | 'missing_api_key';
};

type ResolveEmbeddingOptions = {
  hasApiKey?: boolean | null;
};

export function supportsEmbeddings(provider: AiProvider | null | undefined): boolean {
  if (!provider?.enabled || !EMBEDDING_PROVIDER_TYPES.has(provider.type)) {
    return false;
  }

  return typeof getProvider(provider.type).embedTexts === 'function';
}

export function getEmbeddingProviderCandidates(ai: AiSettings): AiProvider[] {
  return ai.providers.filter(supportsEmbeddings);
}

export function requiresEmbeddingApiKey(provider: AiProvider | null | undefined): boolean {
  if (!provider) {
    return false;
  }
  return provider.type !== 'ollama' && provider.type !== 'openai_compatible';
}

function getEmbeddingModel(ai: AiSettings, provider: AiProvider): string {
  const configuredModel = ai.embeddingConfig?.model?.trim() ?? '';
  if (configuredModel) {
    return configuredModel;
  }

  const providerDefault = provider.defaultModel?.trim() ?? '';
  if (LIKELY_EMBEDDING_MODEL_PATTERN.test(providerDefault)) {
    return providerDefault;
  }

  if (provider.type === 'openai') {
    return 'text-embedding-3-small';
  }

  return '';
}

function embeddingReason(ai: AiSettings, provider: AiProvider, options?: ResolveEmbeddingOptions): ResolvedEmbeddingProvider['reason'] {
  const model = getEmbeddingModel(ai, provider);
  if (!model) {
    return 'missing_model';
  }
  if (requiresEmbeddingApiKey(provider) && options?.hasApiKey === false) {
    return 'missing_api_key';
  }
  return 'ready';
}

export function resolveEmbeddingProvider(ai: AiSettings, options?: ResolveEmbeddingOptions): ResolvedEmbeddingProvider {
  const configuredProviderId = ai.embeddingConfig?.providerId ?? null;
  const configuredProvider = configuredProviderId
    ? ai.providers.find((provider) => provider.id === configuredProviderId) ?? null
    : null;

  if (configuredProviderId) {
    if (!configuredProvider || !supportsEmbeddings(configuredProvider)) {
      return {
        providerConfig: configuredProvider,
        provider: configuredProvider ? getProvider(configuredProvider.type) : null,
        model: ai.embeddingConfig?.model ?? '',
        mode: 'configured',
        reason: 'unsupported_provider',
      };
    }

    const model = getEmbeddingModel(ai, configuredProvider);
    return {
      providerConfig: configuredProvider,
      provider: getProvider(configuredProvider.type),
      model,
      mode: 'configured',
      reason: embeddingReason(ai, configuredProvider, options),
    };
  }

  const activeProvider = ai.providers.find((provider) => provider.id === ai.activeProviderId);
  const candidates = getEmbeddingProviderCandidates(ai);
  const orderedCandidates = [
    ...(supportsEmbeddings(activeProvider) && activeProvider ? [activeProvider] : []),
    ...candidates.filter((provider) => provider.id !== activeProvider?.id),
  ];
  const autoProvider = orderedCandidates.find((provider) => getEmbeddingModel(ai, provider))
    ?? orderedCandidates[0]
    ?? null;

  if (!autoProvider) {
    return {
      providerConfig: null,
      provider: null,
      model: ai.embeddingConfig?.model ?? '',
      mode: 'auto',
      reason: 'no_provider',
    };
  }

  const model = getEmbeddingModel(ai, autoProvider);
  return {
    providerConfig: autoProvider,
    provider: getProvider(autoProvider.type),
    model,
    mode: 'auto',
    reason: embeddingReason(ai, autoProvider, options),
  };
}
