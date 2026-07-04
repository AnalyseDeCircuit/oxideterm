import { describe, expect, it } from 'vitest';

import {
  DEFAULT_AI_EXECUTION_PROFILE_ID,
  normalizeExecutionProfiles,
  type AiExecutionProfile,
} from '@/lib/ai/profiles';

const inheritedToolUse = {
  enabled: true,
  autoApproveTools: { run_command: true },
  disabledTools: [],
  maxRounds: 10,
};

function defaultProfile(overrides: Partial<AiExecutionProfile> = {}): AiExecutionProfile {
  return {
    id: DEFAULT_AI_EXECUTION_PROFILE_ID,
    name: 'Default',
    backend: 'provider',
    providerId: 'provider-1',
    acpAgentId: null,
    model: 'model-1',
    reasoningEffort: 'auto',
    createdAt: 1,
    updatedAt: 1,
    ...overrides,
  };
}

describe('AI execution profiles', () => {
  it('creates fallback profiles that inherit global tool settings', () => {
    const config = normalizeExecutionProfiles({
      providerId: 'provider-1',
      model: 'model-1',
      reasoningEffort: 'auto',
      toolUse: inheritedToolUse,
    });

    expect(config.profiles[0].toolUse).toBeUndefined();
  });

  it('migrates legacy default profiles to inherit global tool settings', () => {
    const config = normalizeExecutionProfiles({
      config: {
        defaultProfileId: DEFAULT_AI_EXECUTION_PROFILE_ID,
        profiles: [
          defaultProfile({
            toolUse: {
              enabled: false,
              autoApproveTools: {},
              disabledTools: [],
              maxRounds: 10,
            },
          }),
        ],
      },
      providerId: 'provider-1',
      model: 'model-1',
      reasoningEffort: 'auto',
      toolUse: inheritedToolUse,
    });

    expect(config.profiles[0].toolUse).toBeUndefined();
  });

  it('migrates legacy default profiles with generated approval details', () => {
    const config = normalizeExecutionProfiles({
      config: {
        defaultProfileId: DEFAULT_AI_EXECUTION_PROFILE_ID,
        profiles: [
          defaultProfile({
            toolUse: {
              enabled: false,
              autoApproveTools: {
                list_targets: true,
                select_target: true,
                observe_terminal: true,
                run_command: false,
              },
              disabledTools: [],
              maxRounds: 20,
            },
          }),
        ],
      },
      providerId: 'provider-1',
      model: 'model-1',
      reasoningEffort: 'auto',
      toolUse: inheritedToolUse,
    });

    expect(config.profiles[0].toolUse).toBeUndefined();
  });

  it('keeps explicit profile tool policies with approval details', () => {
    const toolUse = {
      enabled: false,
      autoApproveTools: { run_command: false },
      disabledTools: [],
      maxRounds: 10,
    };
    const config = normalizeExecutionProfiles({
      config: {
        defaultProfileId: DEFAULT_AI_EXECUTION_PROFILE_ID,
        profiles: [defaultProfile({ id: 'custom-profile', toolUse })],
      },
      providerId: 'provider-1',
      model: 'model-1',
      reasoningEffort: 'auto',
      toolUse: inheritedToolUse,
    });

    expect(config.profiles[0].toolUse).toEqual(toolUse);
  });
});
