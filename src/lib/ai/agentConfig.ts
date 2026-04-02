// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Agent configuration constants — extracted from agentOrchestrator for reuse.
 *
 * These can be overridden via settings.ai.agentConfig in the future.
 */

/** Maximum parallel tool calls the executor may issue per round */
export const MAX_TOOL_CALLS_PER_ROUND = 8;

/** Max bytes of tool output to include in the LLM context per call */
export const MAX_OUTPUT_BYTES = 8192;

/** Stop execution after this many consecutive rounds with zero tool calls */
export const MAX_EMPTY_ROUNDS = 3;

/** Start condensing assistant messages after this round number */
export const CONDENSE_AFTER_ROUND = 2;

/** Keep this many most-recent messages un-condensed */
export const CONDENSE_KEEP_RECENT = 3;

/** Warn when token usage exceeds this fraction of the context window */
export const CONTEXT_OVERFLOW_RATIO = 0.9;

/** Default review interval (rounds between reviewer checks) */
export const DEFAULT_REVIEW_INTERVAL = 5;

/** Maximum steps stored in agentStore before older ones are truncated */
export const MAX_STEPS = 200;
