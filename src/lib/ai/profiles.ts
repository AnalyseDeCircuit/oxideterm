// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiReasoningEffort } from './providers';

export type AiExecutionBackend = 'provider' | 'acp';

export type AiExecutionProfile = {
  id: string;
  name: string;
  backend?: AiExecutionBackend;
  providerId: string | null;
  acpAgentId?: string | null;
  model: string | null;
  reasoningEffort: AiReasoningEffort;
  toolUse?: {
    enabled?: boolean;
    autoApproveTools?: Record<string, boolean>;
    disabledTools?: string[];
    maxRounds?: number;
  };
  context?: {
    includeRuntimeChips: boolean;
    includeMemory: boolean;
    includeRag: boolean;
  };
  commandPolicy?: {
    allow?: string[];
    deny?: string[];
  };
  createdAt: number;
  updatedAt: number;
};

export type AiExecutionProfilesConfig = {
  defaultProfileId: string;
  profiles: AiExecutionProfile[];
};

export const DEFAULT_AI_EXECUTION_PROFILE_ID = 'default';

export function createDefaultExecutionProfile(input: {
  providerId: string | null;
  model: string | null;
  reasoningEffort: AiReasoningEffort;
  toolUse?: AiExecutionProfile['toolUse'];
}): AiExecutionProfile {
  const now = Date.now();
  return {
    id: DEFAULT_AI_EXECUTION_PROFILE_ID,
    name: 'Default',
    backend: 'provider',
    providerId: input.providerId,
    acpAgentId: null,
    model: input.model,
    reasoningEffort: input.reasoningEffort,
    toolUse: input.toolUse,
    context: {
      includeRuntimeChips: true,
      includeMemory: true,
      includeRag: true,
    },
    commandPolicy: {
      allow: [],
      deny: [],
    },
    createdAt: now,
    updatedAt: now,
  };
}

export function normalizeExecutionProfiles(input: {
  config?: AiExecutionProfilesConfig;
  providerId: string | null;
  model: string | null;
  reasoningEffort: AiReasoningEffort;
  toolUse?: AiExecutionProfile['toolUse'];
}): AiExecutionProfilesConfig {
  const fallback = createDefaultExecutionProfile({
    providerId: input.providerId,
    model: input.model,
    reasoningEffort: input.reasoningEffort,
  });
  const existingProfiles = Array.isArray(input.config?.profiles) ? input.config.profiles : [];
  const profiles = (existingProfiles.length > 0 ? existingProfiles : [fallback]).map(normalizeExecutionProfile);
  const defaultProfileId = input.config?.defaultProfileId && profiles.some((profile) => profile.id === input.config?.defaultProfileId)
    ? input.config.defaultProfileId
    : profiles[0]?.id ?? fallback.id;
  return { defaultProfileId, profiles };
}

function normalizeExecutionProfile(profile: AiExecutionProfile): AiExecutionProfile {
  const backend: AiExecutionBackend = profile.backend === 'acp' ? 'acp' : 'provider';
  const toolUse = isLegacyDefaultProfileToolUseOverride(profile) ? undefined : profile.toolUse;
  return {
    ...profile,
    // Missing backend is legacy provider-backed profile data.
    backend,
    providerId: backend === 'acp' ? null : profile.providerId,
    acpAgentId: backend === 'acp' ? profile.acpAgentId ?? null : null,
    model: backend === 'acp' ? null : profile.model,
    toolUse,
  };
}

function isLegacyDefaultProfileToolUseOverride(profile: AiExecutionProfile): boolean {
  if (profile.id !== DEFAULT_AI_EXECUTION_PROFILE_ID || !profile.toolUse) return false;

  const disabledTools = profile.toolUse.disabledTools ?? [];

  // Older default profiles stored a disabled tool-use block even though the UI
  // exposes tool calling as a global setting. Some migrated copies also contain
  // the default approval matrix, so the absence of disabled tools is the stable
  // signal that this was generated state rather than a profile-specific policy.
  return profile.toolUse.enabled === false
    && disabledTools.length === 0;
}

export function resolveExecutionProfile(
  config: AiExecutionProfilesConfig | undefined,
  profileId?: string | null,
): AiExecutionProfile | null {
  if (!config?.profiles?.length) return null;
  return config.profiles.find((profile) => profile.id === profileId)
    ?? config.profiles.find((profile) => profile.id === config.defaultProfileId)
    ?? config.profiles[0]
    ?? null;
}
