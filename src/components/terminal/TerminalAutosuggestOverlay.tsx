// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { TerminalAutosuggestPosition } from '@/lib/terminal/autosuggest';

export function TerminalGhostSuggestion({
  text,
  position,
}: {
  text: string | null;
  position: TerminalAutosuggestPosition | null;
}) {
  if (!text || !position) return null;
  return (
    <div
      className="pointer-events-none absolute z-20 whitespace-pre font-mono text-theme-text-muted/45"
      style={{
        left: position.left,
        top: position.top,
        lineHeight: `${position.lineHeight}px`,
      }}
      aria-hidden="true"
    >
      {text}
    </div>
  );
}
