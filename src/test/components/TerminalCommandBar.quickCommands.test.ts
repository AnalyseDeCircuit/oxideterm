// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, expect, it } from 'vitest';
import {
  resolveQuickCommandInsertCompletion,
  resolveQuickCommandRunCompletion,
} from '@/components/terminal/TerminalCommandBar';

describe('TerminalCommandBar quick commands', () => {
  it('keeps the quick command palette open when pin mode is active', () => {
    expect(resolveQuickCommandRunCompletion(true, true, true)).toEqual({
      quickCommandsOpen: true,
      shouldFocusInput: false,
    });
  });

  it('closes the quick command palette after a normal run', () => {
    expect(resolveQuickCommandRunCompletion(false, true, true)).toEqual({
      quickCommandsOpen: false,
      shouldFocusInput: false,
    });
  });

  it('returns focus to the command input when an unpinned run stays in the bar', () => {
    expect(resolveQuickCommandRunCompletion(false, true, false)).toEqual({
      quickCommandsOpen: false,
      shouldFocusInput: true,
    });
  });

  it('keeps the quick command palette open after pinned row insert', () => {
    expect(resolveQuickCommandInsertCompletion(true)).toEqual({
      quickCommandsOpen: true,
      shouldFocusInput: true,
    });
  });

  it('closes the quick command palette after unpinned row insert', () => {
    expect(resolveQuickCommandInsertCompletion(false)).toEqual({
      quickCommandsOpen: false,
      shouldFocusInput: true,
    });
  });
});
