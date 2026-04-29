// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

const runtimeEpoch = (() => {
  try {
    return crypto.randomUUID();
  } catch {
    return `runtime-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
  }
})();

export function getAiRuntimeEpoch(): string {
  return runtimeEpoch;
}

export function makeAiStateVersion(scope: string, parts: Array<string | number | boolean | null | undefined> = []): string {
  return [
    scope,
    ...parts.map((part) => String(part ?? 'none')),
  ].join(':');
}
