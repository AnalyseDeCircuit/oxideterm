import { describe, expect, it } from 'vitest';
import { resolveChatEmbeddingApiKey } from '@/lib/ai/providerKeyScope';

describe('provider key scoping', () => {
  it('uses no key for providers that do not require one', () => {
    expect(resolveChatEmbeddingApiKey({
      embeddingProviderId: 'local-embedding',
      activeProviderId: 'chat-provider',
      activeProviderApiKey: null,
      embeddingRequiresApiKey: false,
      embeddingMode: 'auto',
    })).toEqual({ kind: 'no-key' });
  });

  it('reuses the active provider key for same-provider embeddings', () => {
    expect(resolveChatEmbeddingApiKey({
      embeddingProviderId: 'provider-1',
      activeProviderId: 'provider-1',
      activeProviderApiKey: 'sk-active',
      embeddingRequiresApiKey: true,
      embeddingMode: 'auto',
    })).toEqual({ kind: 'use-key', apiKey: 'sk-active' });
  });

  it('refuses to unlock an auto-selected non-active embedding provider during chat send', () => {
    expect(resolveChatEmbeddingApiKey({
      embeddingProviderId: 'embedding-provider',
      activeProviderId: 'chat-provider',
      activeProviderApiKey: 'sk-chat',
      embeddingRequiresApiKey: true,
      embeddingMode: 'auto',
    })).toEqual({ kind: 'skip' });
  });

  it('allows loading exactly one explicitly configured non-active embedding provider', () => {
    expect(resolveChatEmbeddingApiKey({
      embeddingProviderId: 'embedding-provider',
      activeProviderId: 'chat-provider',
      activeProviderApiKey: 'sk-chat',
      embeddingRequiresApiKey: true,
      embeddingMode: 'configured',
    })).toEqual({ kind: 'load-provider-key', providerId: 'embedding-provider' });
  });

  it('does not fake a key when the active provider key is unavailable', () => {
    expect(resolveChatEmbeddingApiKey({
      embeddingProviderId: 'provider-1',
      activeProviderId: 'provider-1',
      activeProviderApiKey: null,
      embeddingRequiresApiKey: true,
      embeddingMode: 'auto',
    })).toEqual({ kind: 'skip' });
  });
});
