// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiSettings } from '../../../store/settingsStore';
import {
  isOrchestratorToolName,
  orchestratorApprovalKeyForTool,
  orchestratorRiskForTool,
} from './definitions';
import type { AiActionRisk } from './types';

export type AiPolicyDecisionKind = 'allow' | 'require_approval' | 'deny';

export type AiPolicyDecision = {
  decision: AiPolicyDecisionKind;
  risk: AiActionRisk;
  reasonCode: string;
  reasonTextKey: string;
  matchedPolicyKey: string;
  approvalMode: 'default' | 'bypass';
  profileId?: string;
};

export function resolveAiPolicyDecision(input: {
  toolName: string;
  args?: Record<string, unknown>;
  aiSettings: AiSettings;
  safetyMode?: 'default' | 'bypass';
  profileId?: string;
}): AiPolicyDecision {
  const args = input.args ?? {};
  const toolUse = input.aiSettings.toolUse;
  const disabledTools = new Set(toolUse?.disabledTools ?? []);
  const risk = isOrchestratorToolName(input.toolName)
    ? orchestratorRiskForTool(input.toolName, args)
    : 'write';
  const matchedPolicyKey = isOrchestratorToolName(input.toolName)
    ? orchestratorApprovalKeyForTool(input.toolName, args)
    : input.toolName;
  const approvalMode = input.safetyMode === 'bypass' ? 'bypass' : 'default';

  if (disabledTools.has(input.toolName) || disabledTools.has(matchedPolicyKey)) {
    return {
      decision: 'deny',
      risk,
      reasonCode: 'tool_disabled',
      reasonTextKey: 'ai.tool_use.policy_reason_tool_disabled',
      matchedPolicyKey,
      approvalMode,
      profileId: input.profileId,
    };
  }

  if (risk === 'read') {
    return {
      decision: 'allow',
      risk,
      reasonCode: 'read_only_auto_allowed',
      reasonTextKey: 'ai.tool_use.policy_reason_read_only',
      matchedPolicyKey,
      approvalMode,
      profileId: input.profileId,
    };
  }

  if (risk === 'credential') {
    return {
      decision: 'require_approval',
      risk,
      reasonCode: 'credential_requires_user',
      reasonTextKey: 'ai.tool_use.policy_reason_credential',
      matchedPolicyKey,
      approvalMode,
      profileId: input.profileId,
    };
  }

  if (risk === 'destructive') {
    if (approvalMode === 'bypass') {
      return {
        decision: 'allow',
        risk,
        reasonCode: 'bypass_destructive_allowed',
        reasonTextKey: 'ai.tool_use.policy_reason_bypass',
        matchedPolicyKey,
        approvalMode,
        profileId: input.profileId,
      };
    }
    return {
      decision: 'require_approval',
      risk,
      reasonCode: 'destructive_requires_approval',
      reasonTextKey: 'ai.tool_use.policy_reason_destructive',
      matchedPolicyKey,
      approvalMode,
      profileId: input.profileId,
    };
  }

  const autoApproveTools = toolUse?.autoApproveTools ?? {};
  if (autoApproveTools[matchedPolicyKey] === true) {
    return {
      decision: 'allow',
      risk,
      reasonCode: 'auto_approved',
      reasonTextKey: 'ai.tool_use.policy_reason_auto_approved',
      matchedPolicyKey,
      approvalMode,
      profileId: input.profileId,
    };
  }

  return {
    decision: 'require_approval',
    risk,
    reasonCode: 'policy_requires_approval',
    reasonTextKey: 'ai.tool_use.policy_reason_requires_approval',
    matchedPolicyKey,
    approvalMode,
    profileId: input.profileId,
  };
}
