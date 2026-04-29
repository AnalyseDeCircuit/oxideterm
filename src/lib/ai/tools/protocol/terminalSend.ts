// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import {
  beginTerminalCommandMark,
  findPaneBySessionId,
  writeToTerminal,
} from '../../../terminalRegistry';

export type TerminalSendKind = 'command' | 'text' | 'keys' | 'control' | 'mouse';

export interface TerminalSendRequest {
  sessionId: string;
  input: string;
  inputKind: TerminalSendKind;
  appendEnter?: boolean;
}

export interface TerminalSendResult {
  ok: boolean;
  paneId?: string;
  bytesSent?: number;
  error?: string;
}

export function terminalSend(request: TerminalSendRequest): TerminalSendResult {
  const paneId = findPaneBySessionId(request.sessionId);
  if (!paneId) {
    return { ok: false, error: `Open terminal session not found: ${request.sessionId}` };
  }

  const payload = request.appendEnter ? `${request.input}\r` : request.input;
  const sent = writeToTerminal(paneId, payload);
  if (!sent) {
    return { ok: false, paneId, error: `Terminal session is not writable: ${request.sessionId}` };
  }
  if (request.inputKind === 'command') {
    beginTerminalCommandMark(paneId, {
      command: request.input,
      source: 'ai',
      sessionId: request.sessionId,
    });
  }

  return {
    ok: true,
    paneId,
    bytesSent: payload.length,
  };
}
