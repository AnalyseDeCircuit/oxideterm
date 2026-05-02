// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, expect, it } from 'vitest';
import { resolveModelSelectorProviderProbe } from '@/components/ai/ModelSelector';
import type { AiProvider } from '@/types';

const provider = (patch: Partial<AiProvider>): AiProvider => ({
  id: 'provider-1',
  name: 'Provider',
  type: 'openai',
  baseUrl: 'https://api.example.com/v1',
  defaultModel: 'model',
  models: [],
  enabled: true,
  createdAt: 1,
  ...patch,
});

describe('ModelSelector provider probing', () => {
  const isLocalProviderUrl = (baseUrl: string) => baseUrl.includes('localhost');

  it('marks disabled providers without probing', () => {
    expect(resolveModelSelectorProviderProbe(provider({ enabled: false }), isLocalProviderUrl))
      .toEqual({ kind: 'disabled' });
  });

  it('allows secret-free online probes for local providers', () => {
    expect(resolveModelSelectorProviderProbe(provider({ type: 'ollama', baseUrl: 'http://localhost:11434' }), isLocalProviderUrl))
      .toEqual({ kind: 'implicit-key', endpoint: '/api/tags' });
    expect(resolveModelSelectorProviderProbe(provider({ type: 'openai_compatible', baseUrl: 'http://localhost:1234/v1' }), isLocalProviderUrl))
      .toEqual({ kind: 'implicit-key', endpoint: '/models' });
  });

  it('does not authorize background API key reads for cloud providers', () => {
    expect(resolveModelSelectorProviderProbe(provider({ type: 'openai_compatible', baseUrl: 'https://api.example.com/v1' }), isLocalProviderUrl))
      .toEqual({ kind: 'stored-key' });
    expect(resolveModelSelectorProviderProbe(provider({ type: 'anthropic' }), isLocalProviderUrl))
      .toEqual({ kind: 'stored-key' });
  });
});
