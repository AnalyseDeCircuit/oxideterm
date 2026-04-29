// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

const SECRET_PATTERNS: RegExp[] = [
  /\b(pass(word)?|passwd|token|secret|api[_-]?key|access[_-]?key|private[_-]?key)\b/i,
  /\b(authorization|bearer)\b/i,
  /\b(AWS_SECRET_ACCESS_KEY|GITHUB_TOKEN|OPENAI_API_KEY|ANTHROPIC_API_KEY)\b/i,
  /(?:^|\s)(?:-p|--password|--passphrase)\s*\S+/i,
  /\b\w*(?:token|secret|password|api[_-]?key)\w*\s*=\s*['"]?[^'"\s]+/i,
];

export function isLikelySecretCommand(command: string): boolean {
  const normalized = command.trim();
  if (!normalized) return false;
  return SECRET_PATTERNS.some((pattern) => pattern.test(normalized));
}

