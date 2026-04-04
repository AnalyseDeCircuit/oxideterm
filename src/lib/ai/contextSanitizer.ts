// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Context Sanitizer — Redact sensitive data before sending to external AI APIs
 *
 * Applies pattern-based redaction to terminal output, tool results, and context
 * snapshots to prevent accidental leakage of secrets, credentials, and tokens.
 *
 * Design principles:
 * - False positives are acceptable (better to over-redact than leak)
 * - Patterns target common secret formats, not arbitrary strings
 * - Redacted text preserves the key name for context (e.g., `AWS_SECRET_KEY=[REDACTED]`)
 * - Single-pass regex for each pattern to keep latency minimal
 */

const REDACTED = '[REDACTED]';

// ═══════════════════════════════════════════════════════════════════════════
// Secret Patterns
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Each entry: [pattern, replacement].
 * Patterns are designed to match `KEY=value` or `KEY: value` forms commonly
 * seen in env exports, config files, and CLI output.
 */
const SECRET_PATTERNS: Array<[RegExp, string | ((...args: string[]) => string)]> = [
  // ── Environment variable exports ──────────────────────────────────────
  // export SECRET_KEY=abc123  →  export SECRET_KEY=[REDACTED]
  [
    /\b(export\s+\w*(?:SECRET|TOKEN|PASSWORD|PASSWD|KEY|CREDENTIAL|AUTH)[A-Z_]*\s*=\s*).+/gi,
    `$1${REDACTED}`,
  ],

  // ── Generic KEY=value assignments ─────────────────────────────────────
  // AWS_SECRET_ACCESS_KEY=abc  →  AWS_SECRET_ACCESS_KEY=[REDACTED]
  // DB_PASSWORD=xyz            →  DB_PASSWORD=[REDACTED]
  // DB_PASSWORD='val with spaces'  →  DB_PASSWORD=[REDACTED]
  // Excludes TypeScript type annotations: `password: string` (word boundary after colon + known type)
  [
    /\b(\w*(?:SECRET|_KEY|TOKEN|PASSWORD|PASSWD|CREDENTIAL|AUTH_TOKEN|API_KEY|APIKEY|ACCESS_KEY|PRIVATE_KEY)\s*[=:]\s*)(['"])([^'"]{8,})\2/gi,
    `$1${REDACTED}`,
  ],
  [
    /\b(\w*(?:SECRET|_KEY|TOKEN|PASSWORD|PASSWD|CREDENTIAL|AUTH_TOKEN|API_KEY|APIKEY|ACCESS_KEY|PRIVATE_KEY)\s*=\s*)([^\s'";\n]{8,})/gi,
    `$1${REDACTED}`,
  ],
  [
    /\b(\w*(?:SECRET|_KEY|TOKEN|PASSWORD|PASSWD|CREDENTIAL|AUTH_TOKEN|API_KEY|APIKEY|ACCESS_KEY|PRIVATE_KEY)\s*:\s*)(?!(?:string|number|boolean|any|unknown|never|void|null|undefined|Buffer|Uint8Array)\b)([^\s'";\n,)}{]{8,})/gi,
    `$1${REDACTED}`,
  ],

  // ── Authorization headers ─────────────────────────────────────────────
  // Authorization: Bearer eyJhb...  →  Authorization: Bearer [REDACTED]
  [
    /\b((?:Authorization|Proxy-Authorization)\s*:\s*(?:Bearer|Basic|Token|Digest)\s+)\S+/gi,
    `$1${REDACTED}`,
  ],

  // ── AWS-style keys (AKIA...) ──────────────────────────────────────────
  [
    /\b(AKIA[0-9A-Z]{16})\b/g,
    REDACTED,
  ],

  // ── Vendor-specific token prefixes (standalone, no KEY= needed) ──────
  // GitHub PAT:      ghp_xxx, gho_xxx, ghu_xxx, ghs_xxx, ghr_xxx, github_pat_xxx
  // OpenAI:          sk-proj-xxx, sk-ant-xxx
  // Stripe:          sk_live_xxx, sk_test_xxx, pk_live_xxx, pk_test_xxx, rk_live_xxx, rk_test_xxx
  // Slack:           xoxb-xxx, xoxp-xxx, xoxo-xxx, xoxa-xxx, xoxs-xxx
  // Anthropic:       sk-ant-xxx
  [
    /\b(gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-proj-[A-Za-z0-9]{20,}|sk-ant-[A-Za-z0-9]{20,}|sk_(?:live|test)_[A-Za-z0-9]{10,}|pk_(?:live|test)_[A-Za-z0-9]{10,}|rk_(?:live|test)_[A-Za-z0-9]{10,}|xox[bpoas]-[A-Za-z0-9\-]{10,})/g,
    REDACTED,
  ],

  // ── Long base64/hex strings that look like tokens (≥40 chars) ─────────
  // Matches standalone tokens common in CI output, curl responses, etc.
  [
    /\b([A-Za-z0-9+/]{40,}={0,2})\b/g,
    (match: string) => {
      // Only redact if it looks like a token (mixed case or with digits)
      if (/[a-z]/.test(match) && /[A-Z]/.test(match) && /[0-9]/.test(match)) {
        return REDACTED;
      }
      return match;
    },
  ],

  // ── Private key blocks ────────────────────────────────────────────────
  [
    /-----BEGIN\s+(?:RSA\s+|EC\s+|DSA\s+|OPENSSH\s+)?PRIVATE\s+KEY-----[\s\S]*?-----END\s+(?:RSA\s+|EC\s+|DSA\s+|OPENSSH\s+)?PRIVATE\s+KEY-----/g,
    `-----BEGIN PRIVATE KEY-----\n${REDACTED}\n-----END PRIVATE KEY-----`,
  ],

  // ── Connection strings with embedded passwords ────────────────────────
  // postgres://user:pass@host  →  postgres://user:[REDACTED]@host
  [
    /((?:postgres|mysql|mongodb|redis|amqp|mssql|sqlite|mariadb|cockroachdb):\/\/[^:]+:)([^@]+)(@)/gi,
    `$1${REDACTED}$3`,
  ],

  // ── JSON "key": "value" for secret-looking keys ──────────────────────
  // "api_key": "sk_live_abc..."  →  "api_key": "[REDACTED]"
  // "password": "hunter2"        →  "password": "[REDACTED]"
  // Also handles: no-space, single-quoted (non-standard JSON)
  [
    /(["'](?:\w*(?:secret|_key|token|password|passwd|credential|auth_token|api_key|apikey|access_key|private_key))["']\s*:\s*)(["'])([^"']{8,})\2/gi,
    `$1$2${REDACTED}$2`,
  ],
];

// ═══════════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Redact sensitive data from text before sending to external AI APIs.
 *
 * Applied after ANSI stripping and output compression, but before
 * the final truncation and API submission.
 */
export function sanitizeForAi(text: string): string {
  if (!text) return text;

  let result = text;
  for (const [pattern, replacement] of SECRET_PATTERNS) {
    if (typeof replacement === 'function') {
      result = result.replace(pattern, replacement);
    } else {
      result = result.replace(pattern, replacement);
    }
  }
  return result;
}

/**
 * Sanitize SSH connection info for AI context.
 * Preserves host for debugging context but redacts username.
 */
export function sanitizeConnectionInfo(_username: string, host: string, port: number): string {
  return `****@${host}:${port}`;
}

/**
 * Last-mile sanitization for API messages before sending to external providers.
 *
 * Applies sanitizeForAi to the `content` field of every message.
 * Returns a new array — does NOT mutate the input.
 *
 * This is the safety net: even if a data source forgot to sanitize,
 * this catch-all pass will redact secrets before they leave the process.
 */
export function sanitizeApiMessages<T extends { role: string; content?: string | null }>(
  messages: T[],
): T[] {
  return messages.map((msg) => {
    if (!msg.content || typeof msg.content !== 'string') return msg;
    const cleaned = sanitizeForAi(msg.content);
    if (cleaned === msg.content) return msg; // no change — avoid allocation
    return { ...msg, content: cleaned };
  });
}
