// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { Clipboard, Copy } from 'lucide-react';
import { useEffect } from 'react';

export type TerminalContextMenuState = {
  x: number;
  y: number;
  canCopy: boolean;
};

type TerminalContextMenuProps = {
  menu: TerminalContextMenuState | null;
  copyLabel: string;
  pasteLabel: string;
  onCopy: () => void;
  onPaste: () => void;
  onClose: () => void;
};

const MENU_WIDTH = 176;
const MENU_ITEM_HEIGHT = 34;
const MENU_PADDING = 6;
const VIEWPORT_MARGIN = 8;

function clampMenuPosition(x: number, y: number) {
  // Keep the menu inside the current viewport so right-clicking near an edge
  // does not place actions under the window chrome or outside the WebView.
  const maxX = window.innerWidth - MENU_WIDTH - VIEWPORT_MARGIN;
  const maxY = window.innerHeight - (MENU_ITEM_HEIGHT * 2 + MENU_PADDING * 2) - VIEWPORT_MARGIN;

  return {
    x: Math.max(VIEWPORT_MARGIN, Math.min(x, Math.max(VIEWPORT_MARGIN, maxX))),
    y: Math.max(VIEWPORT_MARGIN, Math.min(y, Math.max(VIEWPORT_MARGIN, maxY))),
  };
}

export function TerminalContextMenu({
  menu,
  copyLabel,
  pasteLabel,
  onCopy,
  onPaste,
  onClose,
}: TerminalContextMenuProps) {
  useEffect(() => {
    if (!menu) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [menu, onClose]);

  if (!menu) return null;

  const position = clampMenuPosition(menu.x, menu.y);

  return (
    <div className="fixed inset-0 z-50" onMouseDown={onClose}>
      <div
        className="fixed rounded-md border border-theme-border bg-theme-bg-panel p-1 shadow-xl"
        style={{ left: position.x, top: position.y, width: MENU_WIDTH }}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <button
          type="button"
          disabled={!menu.canCopy}
          className="flex h-[34px] w-full items-center gap-2 rounded px-2 text-left text-sm text-theme-text hover:bg-theme-bg-hover disabled:cursor-not-allowed disabled:opacity-45"
          onClick={() => {
            onCopy();
            onClose();
          }}
        >
          <Copy className="h-4 w-4" aria-hidden="true" />
          <span>{copyLabel}</span>
        </button>
        <button
          type="button"
          className="flex h-[34px] w-full items-center gap-2 rounded px-2 text-left text-sm text-theme-text hover:bg-theme-bg-hover"
          onClick={() => {
            onPaste();
            onClose();
          }}
        >
          <Clipboard className="h-4 w-4" aria-hidden="true" />
          <span>{pasteLabel}</span>
        </button>
      </div>
    </div>
  );
}
