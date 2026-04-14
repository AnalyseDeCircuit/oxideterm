// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export const TOOL_USE_DISABLED_NEGATIVE_CONSTRAINT = 'TOOL CALLING IS CURRENTLY DISABLED. DO NOT use the tool_code or JSON schema format. If you need a tool, explain to the user why you cannot access it.';

export function getToolUseNegativeConstraint(toolUseEnabled: boolean): string | undefined {
  return toolUseEnabled ? undefined : TOOL_USE_DISABLED_NEGATIVE_CONSTRAINT;
}