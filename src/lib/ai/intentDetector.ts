// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * # Intent Detector
 *
 * Frontend-only regex/keyword classifier that detects user intent from
 * parsed input. The detected intent can be used to:
 *
 * 1. Provide smarter default context gathering
 * 2. Pre-select appropriate tool categories
 * 3. Improve the system prompt with role guidance
 *
 * This is a lightweight heuristic — it doesn't need to be perfect since
 * the LLM does the actual reasoning. The goal is to give 80/20 accuracy
 * to optimize context assembly before the LLM call.
 */

import type { ParsedInput } from './inputParser';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export type IntentType =
  | 'execute'      // User wants to run/do something
  | 'explain'      // User wants to understand something
  | 'troubleshoot' // User is dealing with errors/problems
  | 'create'       // User wants to write/generate something
  | 'explore'      // User wants to discover/find/list
  | 'configure'    // User wants to set up/modify settings
  | 'general';     // Default fallback

export type Intent = {
  type: IntentType;
  confidence: number; // 0.0 - 1.0
  /** System prompt hint to improve LLM's role understanding */
  systemHint: string;
};

// ═══════════════════════════════════════════════════════════════════════════
// Pattern Definitions
// ═══════════════════════════════════════════════════════════════════════════

type IntentPattern = {
  type: IntentType;
  /** Patterns matched against the cleaned text (case-insensitive) */
  patterns: RegExp[];
  /** Confidence when matched */
  confidence: number;
  systemHint: string;
};

const INTENT_PATTERNS: IntentPattern[] = [
  {
    type: 'execute',
    patterns: [
      /^(run|execute|start|stop|restart|kill|deploy|install|uninstall)\b/i,
      /\b(run this|execute this|do this|make it)\b/i,
      /\b(ssh into|connect to|log ?in)\b/i,
      /^(sudo|apt|yum|brew|pip|npm|pnpm|cargo|docker|kubectl|systemctl)\b/i,
    ],
    confidence: 0.85,
    systemHint: 'The user wants to execute an action. Focus on providing actionable commands and confirming before executing anything destructive.',
  },
  {
    type: 'explain',
    patterns: [
      /^(explain|what is|what are|what does|how does|why does|why is)\b/i,
      /^(tell me about|describe|walk me through)\b/i,
      /\b(mean|meaning|purpose|difference between)\b/i,
      /\?\s*$/,  // Ends with question mark
    ],
    confidence: 0.8,
    systemHint: 'The user wants an explanation. Provide clear, educational answers with examples where helpful.',
  },
  {
    type: 'troubleshoot',
    patterns: [
      /^(fix|debug|troubleshoot|diagnose|why.*(fail|error|crash|broken))/i,
      /\b(error|fail(ed|ing|ure)?|crash(ed|ing)?|broken|not working|can'?t|unable)\b/i,
      /\b(issue|problem|bug|wrong|weird|strange)\b/i,
      /\b(permission denied|connection refused|timeout|not found|no such)\b/i,
    ],
    confidence: 0.9,
    systemHint: 'The user is troubleshooting a problem. Analyze error messages carefully, suggest diagnostic commands, and provide step-by-step fixes.',
  },
  {
    type: 'create',
    patterns: [
      /^(create|write|generate|make|build|set up|setup|init)\b/i,
      /^(add|new|draft|compose)\b/i,
      /\b(script|config|file|template|dockerfile|makefile|pipeline)\b/i,
      /\b(write me|generate a|create a|make a)\b/i,
    ],
    confidence: 0.85,
    systemHint: 'The user wants to create or generate something. Provide complete, production-ready code or configurations.',
  },
  {
    type: 'explore',
    patterns: [
      /^(find|search|list|show|display|get|check|look|where)\b/i,
      /^(ls|cat|grep|find|locate|which|type|file)\b/i,
      /\b(how many|count|size|status|info|version)\b/i,
    ],
    confidence: 0.75,
    systemHint: 'The user wants to discover or inspect information. Use appropriate tools to gather and present the requested data.',
  },
  {
    type: 'configure',
    patterns: [
      /^(configure|config|set|change|modify|update|edit|adjust|tune)\b/i,
      /\b(settings?|config(uration)?|preference|option|parameter)\b/i,
      /\b(enable|disable|toggle|switch|turn (on|off))\b/i,
    ],
    confidence: 0.8,
    systemHint: 'The user wants to modify settings or configuration. Identify the specific setting, explain the change, and confirm before applying.',
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Slash Command → Intent Mapping
// ═══════════════════════════════════════════════════════════════════════════

const SLASH_TO_INTENT: Record<string, IntentType> = {
  explain: 'explain',
  fix: 'troubleshoot',
};

// ═══════════════════════════════════════════════════════════════════════════
// Detector
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Detect user intent from parsed input.
 *
 * Priority:
 * 1. Slash command → direct intent mapping (highest confidence)
 * 2. Regex pattern matching on cleaned text
 * 3. Fallback to 'general'
 */
export function detectIntent(parsed: ParsedInput): Intent {
  // 1. Slash command takes priority
  if (parsed.slashCommand) {
    const mappedType = SLASH_TO_INTENT[parsed.slashCommand.name];
    if (mappedType) {
      const pattern = INTENT_PATTERNS.find(p => p.type === mappedType);
      return {
        type: mappedType,
        confidence: 0.95,
        systemHint: pattern?.systemHint || '',
      };
    }
  }

  // 2. Pattern matching on cleaned text
  const text = parsed.cleanText.trim();
  if (!text) {
    return { type: 'general', confidence: 0.5, systemHint: '' };
  }

  let bestMatch: Intent = { type: 'general', confidence: 0.5, systemHint: '' };

  for (const intentPattern of INTENT_PATTERNS) {
    for (const pattern of intentPattern.patterns) {
      if (pattern.test(text)) {
        if (intentPattern.confidence > bestMatch.confidence) {
          bestMatch = {
            type: intentPattern.type,
            confidence: intentPattern.confidence,
            systemHint: intentPattern.systemHint,
          };
        }
        break; // One match per intent category is enough
      }
    }
  }

  return bestMatch;
}
