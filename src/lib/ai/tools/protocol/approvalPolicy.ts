// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { ToolCapability, ToolRisk } from './types';
import { inferToolRisk, isHighRiskToolRisk } from './risk';

export interface ToolApprovalPolicyInput {
  toolName: string;
  args?: Record<string, unknown>;
  capability?: ToolCapability;
  autoApproveTools?: Record<string, boolean>;
  readOnlyTools?: ReadonlySet<string>;
  autonomyLevel?: 'supervised' | 'balanced' | 'autonomous';
}

export interface ToolApprovalDecision {
  risk: ToolRisk;
  autoApprove: boolean;
  requiresApproval: boolean;
  reason: 'supervised' | 'high-risk' | 'tool-auto-approved' | 'read-only' | 'autonomous' | 'manual';
}

export function decideToolApproval(input: ToolApprovalPolicyInput): ToolApprovalDecision {
  const args = input.args ?? {};
  const risk = inferToolRisk(input.toolName, args, input.capability);
  const hasExplicitToolSetting = Object.prototype.hasOwnProperty.call(input.autoApproveTools ?? {}, input.toolName);

  if (input.autonomyLevel === 'supervised') {
    return { risk, autoApprove: false, requiresApproval: true, reason: 'supervised' };
  }

  if (isHighRiskToolRisk(risk)) {
    return { risk, autoApprove: false, requiresApproval: true, reason: 'high-risk' };
  }

  if (input.autoApproveTools?.[input.toolName] === true) {
    return { risk, autoApprove: true, requiresApproval: false, reason: 'tool-auto-approved' };
  }

  if (hasExplicitToolSetting) {
    return { risk, autoApprove: false, requiresApproval: true, reason: 'manual' };
  }

  if (input.autonomyLevel === 'autonomous') {
    return { risk, autoApprove: true, requiresApproval: false, reason: 'autonomous' };
  }

  if (input.readOnlyTools?.has(input.toolName)) {
    return { risk, autoApprove: true, requiresApproval: false, reason: 'read-only' };
  }

  return { risk, autoApprove: false, requiresApproval: true, reason: 'manual' };
}
