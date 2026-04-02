// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Agent Roles module — re-exports for role definitions, registry, and runner.
 */

export {
  BUILTIN_PLANNER,
  BUILTIN_EXECUTOR,
  BUILTIN_REVIEWER,
  DEFAULT_PIPELINE,
  getRole,
  getAllRoles,
  registerCustomRole,
  unregisterCustomRole,
} from './builtinRoles';

export {
  streamCompletion,
  runSingleShot,
  processToolCalls,
  shouldAutoApprove,
  createStep,
} from './roleRunner';

export type {
  SingleShotResult,
  CollectedToolCall,
  StreamResult,
  LLMCallConfig,
  ToolCallOutcome,
} from './roleRunner';
