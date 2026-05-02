// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export type ChatEmbeddingApiKeyRequest = {
  embeddingProviderId: string;
  activeProviderId?: string | null;
  activeProviderApiKey: string | null;
  embeddingRequiresApiKey: boolean;
  embeddingMode: 'configured' | 'auto';
};

/**
 * A normal chat send may already have unlocked the active provider key.
 * Auto-selected embeddings must not unlock a different provider as a side
 * effect, otherwise one message can trigger surprising biometric/keychain
 * prompts. A user-configured embedding provider is different: that provider is
 * explicitly part of the request and may be loaded once.
 *
 * Decision meanings:
 * - use-key: reuse the already loaded active provider key.
 * - no-key: provider does not require a key.
 * - load-provider-key: explicitly configured embedding provider; load this one key.
 * - skip: auto-selected non-active provider; fall back to keyword/BM25 search.
 */
export type ChatEmbeddingApiKeyDecision =
  | { kind: 'use-key'; apiKey: string }
  | { kind: 'no-key' }
  | { kind: 'load-provider-key'; providerId: string }
  | { kind: 'skip' };

export function resolveChatEmbeddingApiKey(request: ChatEmbeddingApiKeyRequest): ChatEmbeddingApiKeyDecision {
  if (!request.embeddingRequiresApiKey) {
    return { kind: 'no-key' };
  }

  if (request.embeddingProviderId === request.activeProviderId) {
    return request.activeProviderApiKey?.trim()
      ? { kind: 'use-key', apiKey: request.activeProviderApiKey }
      : { kind: 'skip' };
  }

  if (request.embeddingMode === 'configured') {
    return { kind: 'load-provider-key', providerId: request.embeddingProviderId };
  }

  return { kind: 'skip' };
}
