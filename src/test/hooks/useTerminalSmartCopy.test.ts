import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Terminal } from '@xterm/xterm';
import { attachTerminalSmartCopy } from '@/hooks/useTerminalSmartCopy';
import { setOverrides } from '@/lib/keybindingRegistry';
import { writeSystemClipboardText } from '@/lib/clipboardSupport';

vi.mock('@/lib/clipboardSupport', () => ({
  writeSystemClipboardText: vi.fn().mockResolvedValue(true),
}));

vi.mock('@/lib/platform', () => ({
  platform: {
    isWindows: true,
    isLinux: false,
    isMac: false,
  },
}));

type Handler = (event: KeyboardEvent) => boolean;

function createTerminalMock() {
  let handler: Handler | null = null;
  let selectionHandler: (() => void) | null = null;

  return {
    term: {
      attachCustomKeyEventHandler: vi.fn((nextHandler: Handler) => {
        handler = nextHandler;
      }),
      onSelectionChange: vi.fn((nextHandler: () => void) => {
        selectionHandler = nextHandler;
        return { dispose: vi.fn() };
      }),
      hasSelection: vi.fn(() => false),
      getSelection: vi.fn(() => ''),
      modes: { mouseTrackingMode: 'none' },
    } as unknown as Terminal,
    getHandler: () => handler,
    triggerSelectionChange: () => selectionHandler?.(),
  };
}

function createShortcutEvent(init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent('keydown', init);
  vi.spyOn(event, 'preventDefault');
  vi.spyOn(event, 'stopPropagation');
  return event;
}

describe('attachTerminalSmartCopy', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setOverrides(new Map());
    vi.mocked(writeSystemClipboardText).mockResolvedValue(true);
    vi.useRealTimers();
  });

  it('copies the current selection and consumes Ctrl+C when enabled', () => {
    const { term, getHandler } = createTerminalMock();
    const copyText = vi.mocked(writeSystemClipboardText);
    const hasSelection = vi.mocked(term.hasSelection);
    const getSelection = vi.mocked(term.getSelection);
    const event = createShortcutEvent({ key: 'c', ctrlKey: true });

    hasSelection.mockReturnValue(true);
    getSelection.mockReturnValue('selected output');

    attachTerminalSmartCopy(term, {
      isActive: () => true,
      isEnabled: () => true,
    });

    const handled = getHandler()?.(event);

    expect(handled).toBe(false);
    expect(copyText).toHaveBeenCalledWith('selected output');
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(event.stopPropagation).toHaveBeenCalledOnce();
  });

  it('lets Ctrl+C pass through when nothing is selected', () => {
    const { term, getHandler } = createTerminalMock();
    const copyText = vi.mocked(writeSystemClipboardText);
    const hasSelection = vi.mocked(term.hasSelection);

    hasSelection.mockReturnValue(false);

    attachTerminalSmartCopy(term, {
      isActive: () => true,
      isEnabled: () => true,
    });

    const handled = getHandler()?.(new KeyboardEvent('keydown', { key: 'c', ctrlKey: true }));

    expect(handled).toBe(true);
    expect(copyText).not.toHaveBeenCalled();
  });

  it('lets Ctrl+C pass through when smart copy is disabled', () => {
    const { term, getHandler } = createTerminalMock();
    const copyText = vi.mocked(writeSystemClipboardText);
    const hasSelection = vi.mocked(term.hasSelection);

    hasSelection.mockReturnValue(true);

    attachTerminalSmartCopy(term, {
      isActive: () => true,
      isEnabled: () => false,
    });

    const handled = getHandler()?.(new KeyboardEvent('keydown', { key: 'c', ctrlKey: true }));

    expect(handled).toBe(true);
    expect(copyText).not.toHaveBeenCalled();
  });

  it('lets Ctrl+C pass through when the terminal is inactive', () => {
    const { term, getHandler } = createTerminalMock();
    const copyText = vi.mocked(writeSystemClipboardText);
    const hasSelection = vi.mocked(term.hasSelection);

    hasSelection.mockReturnValue(true);

    attachTerminalSmartCopy(term, {
      isActive: () => false,
      isEnabled: () => true,
    });

    const handled = getHandler()?.(new KeyboardEvent('keydown', { key: 'c', ctrlKey: true }));

    expect(handled).toBe(true);
    expect(copyText).not.toHaveBeenCalled();
  });

  it('lets a customized terminal paste shortcut pass through when the terminal is inactive', () => {
    const { term, getHandler } = createTerminalMock();
    const onPasteShortcut = vi.fn();
    const event = createShortcutEvent({ key: 'v', ctrlKey: true });

    setOverrides(new Map([
      ['terminal.paste', {
        other: { key: 'v', ctrl: true, shift: false, alt: false, meta: false },
      }],
    ]));

    attachTerminalSmartCopy(term, {
      isActive: () => false,
      isEnabled: () => true,
      onPasteShortcut,
    });

    const handled = getHandler()?.(event);

    expect(handled).toBe(true);
    expect(onPasteShortcut).not.toHaveBeenCalled();
    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(event.stopPropagation).not.toHaveBeenCalled();
  });

  it('restores the default pass-through handler on dispose', () => {
    const { term } = createTerminalMock();
    const attachCustomKeyEventHandler = vi.mocked(term.attachCustomKeyEventHandler);

    const disposable = attachTerminalSmartCopy(term, {
      isActive: () => true,
      isEnabled: () => true,
    });

    disposable.dispose();

    expect(attachCustomKeyEventHandler).toHaveBeenCalledTimes(2);
    const restoredHandler = attachCustomKeyEventHandler.mock.calls[1]?.[0] as Handler;
    expect(restoredHandler(new KeyboardEvent('keydown', { key: 'c', ctrlKey: true }))).toBe(true);
  });

  it('consumes the native paste shortcut and invokes the callback (fixes double-paste #62)', () => {
    const { term, getHandler } = createTerminalMock();
    const onPasteShortcut = vi.fn();
    const event = createShortcutEvent({ key: 'v', ctrlKey: true, shiftKey: true });

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

  it('consumes a customized terminal paste shortcut and invokes the callback', () => {
    const { term, getHandler } = createTerminalMock();
    const onPasteShortcut = vi.fn();
    const event = createShortcutEvent({ key: 'v', ctrlKey: true });

    setOverrides(new Map([
      ['terminal.paste', {
        other: { key: 'v', ctrl: true, shift: false, alt: false, meta: false },
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

  it('still lets Ctrl+Shift+V pass through to xterm after remapping terminal paste to Ctrl+V', () => {
    const { term, getHandler } = createTerminalMock();
    const onPasteShortcut = vi.fn();

    setOverrides(new Map([
      ['terminal.paste', {
        other: { key: 'v', ctrl: true, shift: false, alt: false, meta: false },
      }],
    ]));

    attachTerminalSmartCopy(term, {
      isActive: () => true,
      isEnabled: () => true,
      onPasteShortcut,
    });

    const handled = getHandler()?.(new KeyboardEvent('keydown', { key: 'v', ctrlKey: true, shiftKey: true }));

    expect(handled).toBe(true);
    expect(onPasteShortcut).not.toHaveBeenCalled();
  });

  it('copies the selection after it stabilizes when copy-on-select is enabled', async () => {
    vi.useFakeTimers();
    const { term, triggerSelectionChange } = createTerminalMock();
    const copyText = vi.mocked(writeSystemClipboardText);
    const getSelection = vi.mocked(term.getSelection);

    getSelection.mockReturnValue('copied by mouse');

    attachTerminalSmartCopy(term, {
      isActive: () => true,
      isEnabled: () => true,
      isCopyOnSelectEnabled: () => true,
    });

    triggerSelectionChange();
    vi.advanceTimersByTime(119);
    expect(copyText).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    await Promise.resolve();

    expect(copyText).toHaveBeenCalledWith('copied by mouse');
  });

  it('pastes on middle click when the feature is enabled', () => {
    const { term } = createTerminalMock();
    const onPasteShortcut = vi.fn();
    const container = document.createElement('div');

    attachTerminalSmartCopy(term, {
      isActive: () => true,
      isEnabled: () => true,
      isMiddleClickPasteEnabled: () => true,
      onPasteShortcut,
      container,
    });

    const event = new MouseEvent('mouseup', { button: 1, bubbles: true, cancelable: true });
    const preventDefault = vi.spyOn(event, 'preventDefault');
    const stopPropagation = vi.spyOn(event, 'stopPropagation');
    container.dispatchEvent(event);

    expect(onPasteShortcut).toHaveBeenCalledOnce();
    expect(preventDefault).toHaveBeenCalledOnce();
    expect(stopPropagation).toHaveBeenCalledOnce();
  });
});