// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { Terminal } from '@xterm/xterm';
import { matchAction } from '@/lib/keybindingRegistry';
import { platform } from '@/lib/platform';
import { writeSystemClipboardText } from '@/lib/clipboardSupport';

type Disposable = { dispose: () => void };

type TerminalSmartCopyOptions = {
  isActive: () => boolean;
  isEnabled: () => boolean;
  onPasteShortcut?: () => void;
};

function isSmartCopyShortcut(event: KeyboardEvent): boolean {
  if (event.type !== 'keydown') return false;
  if (!(platform.isWindows || platform.isLinux)) return false;
  if (!event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return false;
  return event.key.toLowerCase() === 'c';
}


function fallbackCopySelection(): void {
  if (typeof document.execCommand !== 'function') {
    console.warn('[Terminal] Clipboard fallback is unavailable in this environment');
    return;
  }

  try {
    const copied = document.execCommand('copy');
    if (!copied) {
      console.warn('[Terminal] Fallback copy did not report success');
    }
  } catch (error) {
    console.warn('[Terminal] Fallback copy failed:', error);
  }
}

function copySelection(selection: string): void {
  if (!selection) return;

  void writeSystemClipboardText(selection).then((written) => {
    if (!written) {
      fallbackCopySelection();
    }
  });
}

function consumeKeyboardEvent(event: KeyboardEvent): void {
  event.preventDefault();
  event.stopPropagation();
}

export function attachTerminalSmartCopy(
  term: Terminal,
  options: TerminalSmartCopyOptions,
): Disposable {
  // xterm currently supports a single custom key handler per terminal.
  // We install smart copy once during terminal setup and remove it during the
  // same component cleanup path, so restoring the default pass-through handler
  // is safe as long as no other feature attaches a second custom handler.
  term.attachCustomKeyEventHandler((event) => {
    if (!options.isActive()) {
      return true;
    }

    if (options.isEnabled() && isSmartCopyShortcut(event)) {
      if (!term.hasSelection()) {
        return true;
      }

      const selection = term.getSelection();
      if (!selection) {
        return true;
      }

      consumeKeyboardEvent(event);
      copySelection(selection);
      return false;
    }

    if (options.onPasteShortcut && matchAction(event, 'terminal') === 'terminal.paste') {
      consumeKeyboardEvent(event);
      // Only the initial keydown should trigger paste. Matching keyup events
      // still need to be consumed so the native paste path does not run later.
      if (event.type === 'keydown' && !event.repeat) {
        options.onPasteShortcut();
      }
      return false;
    }

    return true;
  });

  return {
    dispose: () => {
      term.attachCustomKeyEventHandler(() => true);
    },
  };
}