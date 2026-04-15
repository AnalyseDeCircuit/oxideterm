import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Terminal } from '@xterm/xterm';

import { attachTerminalSmartCopy } from '@/hooks/useTerminalSmartCopy';
import { setOverrides } from '@/lib/keybindingRegistry';

vi.mock('@/lib/clipboardSupport', () => ({
  writeSystemClipboardText: vi.fn().mockResolvedValue(true),
}));

vi.mock('@/lib/platform', () => ({
  platform: {
    isWindows: false,
    isLinux: false,
    isMac: true,
  },
}));

type Handler = (event: KeyboardEvent) => boolean;

function createTerminalMock() {
  let handler: Handler | null = null;

  return {
    term: {
      attachCustomKeyEventHandler: vi.fn((nextHandler: Handler) => {
        handler = nextHandler;
      }),
      hasSelection: vi.fn(() => false),
      getSelection: vi.fn(() => ''),
    } as unknown as Terminal,
    getHandler: () => handler,
  };
}

function createShortcutEvent(type: 'keydown' | 'keyup', init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent(type, init);
  vi.spyOn(event, 'preventDefault');
  vi.spyOn(event, 'stopPropagation');
  return event;
}

describe('attachTerminalSmartCopy on macOS', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setOverrides(new Map());
  });

  it('consumes default Cmd+V and invokes the paste callback', () => {
    const { term, getHandler } = createTerminalMock();
    const onPasteShortcut = vi.fn();
    const event = createShortcutEvent('keydown', { key: 'v', metaKey: true });

    attachTerminalSmartCopy(term, {
      isActive: () => true,
      isEnabled: () => true,
      onPasteShortcut,
    });

    const handled = getHandler()?.(event);

    expect(handled).toBe(false);
    expect(onPasteShortcut).toHaveBeenCalledOnce();
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(event.stopPropagation).toHaveBeenCalledOnce();
  });

  it('also consumes customized macOS terminal paste bindings', () => {
    const { term, getHandler } = createTerminalMock();
    const onPasteShortcut = vi.fn();
    const event = createShortcutEvent('keydown', { key: 'v', metaKey: true, shiftKey: true });

    setOverrides(new Map([
      ['terminal.paste', {
        mac: { key: 'v', ctrl: false, shift: true, alt: false, meta: true },
      }],
    ]));

    attachTerminalSmartCopy(term, {
      isActive: () => true,
      isEnabled: () => true,
      onPasteShortcut,
    });

    const handled = getHandler()?.(event);

    expect(handled).toBe(false);
    expect(onPasteShortcut).toHaveBeenCalledOnce();
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(event.stopPropagation).toHaveBeenCalledOnce();
  });

  it('consumes the Cmd+V keyup without firing a second paste callback', () => {
    const { term, getHandler } = createTerminalMock();
    const onPasteShortcut = vi.fn();
    const keydown = createShortcutEvent('keydown', { key: 'v', metaKey: true });
    const keyup = createShortcutEvent('keyup', { key: 'v', metaKey: true });

    attachTerminalSmartCopy(term, {
      isActive: () => true,
      isEnabled: () => true,
      onPasteShortcut,
    });

    const keydownHandled = getHandler()?.(keydown);
    const keyupHandled = getHandler()?.(keyup);

    expect(keydownHandled).toBe(false);
    expect(keyupHandled).toBe(false);
    expect(onPasteShortcut).toHaveBeenCalledOnce();
    expect(keydown.preventDefault).toHaveBeenCalledOnce();
    expect(keydown.stopPropagation).toHaveBeenCalledOnce();
    expect(keyup.preventDefault).toHaveBeenCalledOnce();
    expect(keyup.stopPropagation).toHaveBeenCalledOnce();
  });

  it('ignores repeated Cmd+V keydowns while still consuming them', () => {
    const { term, getHandler } = createTerminalMock();
    const onPasteShortcut = vi.fn();
    const repeatKeydown = createShortcutEvent('keydown', { key: 'v', metaKey: true, repeat: true });

    attachTerminalSmartCopy(term, {
      isActive: () => true,
      isEnabled: () => true,
      onPasteShortcut,
    });

    const handled = getHandler()?.(repeatKeydown);

    expect(handled).toBe(false);
    expect(onPasteShortcut).not.toHaveBeenCalled();
    expect(repeatKeydown.preventDefault).toHaveBeenCalledOnce();
    expect(repeatKeydown.stopPropagation).toHaveBeenCalledOnce();
  });
});
