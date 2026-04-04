import { describe, expect, it } from 'vitest';
import { isTerminalReservedKey } from '@/hooks/useTerminalKeyboard';

/** Helper to create a minimal KeyboardEvent-like object */
function makeKeyEvent(overrides: Partial<KeyboardEvent>): KeyboardEvent {
  return {
    key: '',
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    ...overrides,
  } as KeyboardEvent;
}

describe('isTerminalReservedKey', () => {
  describe('function keys', () => {
    it('reserves F1-F12', () => {
      for (let i = 1; i <= 12; i++) {
        expect(isTerminalReservedKey(makeKeyEvent({ key: `F${i}` }))).toBe(true);
      }
    });

    it('does not reserve F0 or F13', () => {
      expect(isTerminalReservedKey(makeKeyEvent({ key: 'F0' }))).toBe(false);
      expect(isTerminalReservedKey(makeKeyEvent({ key: 'F13' }))).toBe(false);
    });
  });

  describe('Alt combinations', () => {
    it('reserves Alt+key', () => {
      expect(
        isTerminalReservedKey(makeKeyEvent({ key: 'x', altKey: true }))
      ).toBe(true);
    });

    it('does not reserve Ctrl+Alt or Meta+Alt', () => {
      expect(
        isTerminalReservedKey(makeKeyEvent({ key: 'x', altKey: true, ctrlKey: true }))
      ).toBe(false);
      expect(
        isTerminalReservedKey(makeKeyEvent({ key: 'x', altKey: true, metaKey: true }))
      ).toBe(false);
    });
  });

  describe('Ctrl + letter', () => {
    it('reserves Ctrl+a through Ctrl+z (excluding i, m, t)', () => {
      const reserved = 'abcdefghjklnopqrsuvwxyz'.split('');
      for (const key of reserved) {
        expect(
          isTerminalReservedKey(makeKeyEvent({ key, ctrlKey: true }))
        ).toBe(true);
      }
    });

    it('does NOT reserve Ctrl+i, Ctrl+m, Ctrl+t', () => {
      for (const key of ['i', 'm', 't']) {
        expect(
          isTerminalReservedKey(makeKeyEvent({ key, ctrlKey: true }))
        ).toBe(false);
      }
    });

    it('does not reserve Ctrl+Shift+letter', () => {
      expect(
        isTerminalReservedKey(makeKeyEvent({ key: 'a', ctrlKey: true, shiftKey: true }))
      ).toBe(false);
    });

    it('does not reserve Meta+letter (Cmd on macOS)', () => {
      expect(
        isTerminalReservedKey(makeKeyEvent({ key: 'c', metaKey: true }))
      ).toBe(false);
    });
  });

  describe('non-reserved keys', () => {
    it('does not reserve plain letter', () => {
      expect(isTerminalReservedKey(makeKeyEvent({ key: 'a' }))).toBe(false);
    });

    it('does not reserve Enter', () => {
      expect(isTerminalReservedKey(makeKeyEvent({ key: 'Enter' }))).toBe(false);
    });

    it('does not reserve Escape', () => {
      expect(isTerminalReservedKey(makeKeyEvent({ key: 'Escape' }))).toBe(false);
    });
  });
});
