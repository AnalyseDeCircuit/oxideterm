// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, expect, it, vi } from 'vitest';
import {
  installPreserveScrollbackEd3Handler,
  shouldPreserveScrollbackForEraseInDisplay,
} from '@/lib/terminal/preserveScrollback';

describe('preserveScrollback', () => {
  it('only handles ED3 erase saved lines', () => {
    expect(shouldPreserveScrollbackForEraseInDisplay([3])).toBe(true);
    expect(shouldPreserveScrollbackForEraseInDisplay([0])).toBe(false);
    expect(shouldPreserveScrollbackForEraseInDisplay([1])).toBe(false);
    expect(shouldPreserveScrollbackForEraseInDisplay([2])).toBe(false);
    expect(shouldPreserveScrollbackForEraseInDisplay([])).toBe(false);
  });

  it('does not treat nested subparams as ED3', () => {
    expect(shouldPreserveScrollbackForEraseInDisplay([[3]])).toBe(false);
    expect(shouldPreserveScrollbackForEraseInDisplay([[3], 0])).toBe(false);
  });

  it('registers a CSI J handler and returns the disposable', () => {
    const dispose = vi.fn();
    let registeredHandler: ((params: (number | number[])[]) => boolean | Promise<boolean>) | undefined;
    const registerCsiHandler = vi.fn((_id, handler) => {
      registeredHandler = handler;
      return { dispose };
    });

    const disposable = installPreserveScrollbackEd3Handler({
      parser: { registerCsiHandler },
    } as never);

    expect(registerCsiHandler).toHaveBeenCalledWith({ final: 'J' }, expect.any(Function));
    expect(registeredHandler?.([3])).toBe(true);
    expect(registeredHandler?.([2])).toBe(false);

    disposable.dispose();
    expect(dispose).toHaveBeenCalled();
  });
});
