// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { Terminal } from '@xterm/xterm';
import { matchAction } from '@/lib/keybindingRegistry';
import { platform } from '@/lib/platform';

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

function isNativeTerminalPasteShortcut(event: KeyboardEvent): boolean {
  if (event.type !== 'keydown') return false;

  if (platform.isMac) {
    return event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'v';
  }

  if (platform.isWindows || platform.isLinux) {
    return event.ctrlKey && event.shiftKey && !event.metaKey && !event.altKey && event.key.toLowerCase() === 'v';
  }

  return false;
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
  if (!navigator.clipboard?.writeText) {
    fallbackCopySelection();
    return;
  }

  navigator.clipboard.writeText(selection).catch((error) => {
    console.warn('[Terminal] Clipboard write failed, falling back to document.copy:', error);
    fallbackCopySelection();
  });
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

      copySelection(selection);
      return false;
    }

    if (options.onPasteShortcut && matchAction(event, 'terminal') === 'terminal.paste') {
      if (isNativeTerminalPasteShortcut(event)) {
        return true;
      }

      options.onPasteShortcut();
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