// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { IDisposable, Terminal } from '@xterm/xterm';

type CsiParams = readonly (number | number[])[];

export function shouldPreserveScrollbackForEraseInDisplay(params: CsiParams): boolean {
  return params[0] === 3;
}

export function installPreserveScrollbackEd3Handler(term: Pick<Terminal, 'parser'>): IDisposable {
  return term.parser.registerCsiHandler({ final: 'J' }, (params) => {
    // CSI 3 J (ED3) erases saved lines. Claude/Codex/OpenCode-style TUIs may
    // emit it during redraw, which would destroy the live xterm scrollback.
    // Only swallow ED3; CSI 0J/1J/2J are normal screen erases and must keep
    // flowing to xterm's default handler so clear/redraw behavior stays intact.
    return shouldPreserveScrollbackForEraseInDisplay(params);
  });
}
