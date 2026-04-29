// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiReasoningEffort } from './providers';

export type AiExecutionProfile = {
  id: string;
  name: string;
  providerId: string | null;
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
    providerId: input.providerId,
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
  const fallback = createDefaultExecutionProfile(input);
  const existingProfiles = Array.isArray(input.config?.profiles) ? input.config.profiles : [];
  const profiles = existingProfiles.length > 0 ? existingProfiles : [fallback];
  const defaultProfileId = input.config?.defaultProfileId && profiles.some((profile) => profile.id === input.config?.defaultProfileId)
    ? input.config.defaultProfileId
    : profiles[0]?.id ?? fallback.id;
  return { defaultProfileId, profiles };
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
