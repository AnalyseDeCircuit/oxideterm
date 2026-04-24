import { describe, expect, it } from 'vitest';
import {
  detectChunkedMarker,
  getBackgroundFitStyles,
  hexToRgba,
  isTerminalContainerRenderable,
  isSoftwareWebglRenderer,
  resolveTerminalDimensions,
  shouldAutoFocusTerminal,
  shouldFocusTerminalFromClick,
} from '@/lib/terminalHelpers';

describe('hexToRgba', () => {
  it('converts black', () => {
    expect(hexToRgba('#000000', 1)).toBe('rgba(0, 0, 0, 1)');
  });

  it('converts white', () => {
    expect(hexToRgba('#FFFFFF', 1)).toBe('rgba(255, 255, 255, 1)');
  });

  it('converts with alpha', () => {
    expect(hexToRgba('#FF0000', 0.5)).toBe('rgba(255, 0, 0, 0.5)');
  });

  it('handles zero alpha', () => {
    expect(hexToRgba('#123456', 0)).toBe('rgba(18, 52, 86, 0)');
  });

  it('handles lowercase hex', () => {
    expect(hexToRgba('#ff8800', 1)).toBe('rgba(255, 136, 0, 1)');
  });

  it('handles mixed case', () => {
    expect(hexToRgba('#aAbBcC', 0.8)).toBe('rgba(170, 187, 204, 0.8)');
  });
});

describe('getBackgroundFitStyles', () => {
  it('returns cover styles', () => {
    const styles = getBackgroundFitStyles('cover');
    expect(styles).toEqual({ objectFit: 'cover', width: '100%', height: '100%' });
  });

  it('returns contain styles', () => {
    const styles = getBackgroundFitStyles('contain');
    expect(styles).toEqual({ objectFit: 'contain', width: '100%', height: '100%' });
  });

  it('returns fill styles', () => {
    const styles = getBackgroundFitStyles('fill');
    expect(styles).toEqual({ objectFit: 'fill', width: '100%', height: '100%' });
  });

  it('returns empty for tile', () => {
    const styles = getBackgroundFitStyles('tile');
    expect(styles).toEqual({});
  });
});

describe('isTerminalContainerRenderable', () => {
  it('returns false for disconnected elements', () => {
    const container = document.createElement('div');
    expect(isTerminalContainerRenderable(container)).toBe(false);
  });

  it('returns false for zero-sized containers', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    Object.defineProperty(container, 'getBoundingClientRect', {
      value: () => ({ width: 0, height: 0 }),
    });

    expect(isTerminalContainerRenderable(container)).toBe(false);
    container.remove();
  });

  it('returns true for visible containers', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    Object.defineProperty(container, 'getBoundingClientRect', {
      value: () => ({ width: 800, height: 600 }),
    });

    expect(isTerminalContainerRenderable(container)).toBe(true);
    container.remove();
  });
});

describe('resolveTerminalDimensions', () => {
  it('prefers fit dimensions for visible containers', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    Object.defineProperty(container, 'getBoundingClientRect', {
      value: () => ({ width: 800, height: 600 }),
    });

    expect(
      resolveTerminalDimensions(
        container,
        { cols: 80, rows: 24 },
        { proposeDimensions: () => ({ cols: 120, rows: 40 }) },
      ),
    ).toEqual({ cols: 120, rows: 40 });

    container.remove();
  });

  it('falls back to the last stable xterm size for hidden containers', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    Object.defineProperty(container, 'getBoundingClientRect', {
      value: () => ({ width: 0, height: 0 }),
    });

    expect(
      resolveTerminalDimensions(
        container,
        { cols: 132, rows: 36 },
        { proposeDimensions: () => ({ cols: 1, rows: 1 }) },
      ),
    ).toEqual({ cols: 132, rows: 36 });

    container.remove();
  });

  it('returns null for invalid dimensions', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    Object.defineProperty(container, 'getBoundingClientRect', {
      value: () => ({ width: 800, height: 600 }),
    });

    expect(
      resolveTerminalDimensions(
        container,
        { cols: 0, rows: 0 },
        { proposeDimensions: () => ({ cols: 0, rows: 0 }) },
      ),
    ).toBeNull();

    container.remove();
  });
});

describe('isSoftwareWebglRenderer', () => {
  it.each([
    'llvmpipe (LLVM 19.1.7, 256 bits)',
    'Google SwiftShader',
    'softpipe',
    'Mesa OffScreen',
    'D3D12 WARP adapter',
    'SWR rasterizer',
  ])('returns true for software renderer string %s', (renderer) => {
    expect(isSoftwareWebglRenderer(renderer)).toBe(true);
  });

  it.each([
    'NVIDIA GeForce RTX 5060/PCIe/SSE2',
    'ANGLE (NVIDIA, NVIDIA GeForce RTX 5060 Direct3D11 vs_5_0 ps_5_0)',
    'Mesa Intel(R) Arc Graphics (BMG G21)',
    'AMD Radeon RX 7800 XT (radeonsi, navi31, LLVM 18.1.8, DRM 3.61, 6.13.0)',
  ])('returns false for hardware-backed renderer string %s', (renderer) => {
    expect(isSoftwareWebglRenderer(renderer)).toBe(false);
  });
});

describe('detectChunkedMarker', () => {
  it('matches a marker split across adjacent chunks', () => {
    const first = detectChunkedMarker('', 'prefix ::TRZSZ:TR', '::TRZSZ:TRANSFER:');
    expect(first).toEqual({
      matched: false,
      tail: 'prefix ::TRZSZ:TR'.slice(-('::TRZSZ:TRANSFER:'.length - 1)),
    });

    expect(
      detectChunkedMarker(first.tail, 'ANSFER:S:1.1.6:12345678', '::TRZSZ:TRANSFER:'),
    ).toEqual({
      matched: true,
      tail: '',
    });
  });

  it('keeps only the minimum suffix needed for future matching', () => {
    expect(
      detectChunkedMarker('', 'abcdefghijklmnopqrstuvwxyz', '::TRZSZ:TRANSFER:'),
    ).toEqual({
      matched: false,
      tail: 'abcdefghijklmnopqrstuvwxyz'.slice(-('::TRZSZ:TRANSFER:'.length - 1)),
    });
  });
});

describe('shouldAutoFocusTerminal', () => {
  it('returns true when nothing editable is focused outside the terminal', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    Object.defineProperty(container, 'getBoundingClientRect', {
      value: () => ({ width: 800, height: 600 }),
    });

    const button = document.createElement('button');
    document.body.appendChild(button);

    expect(shouldAutoFocusTerminal(container, button)).toBe(true);

    button.remove();
    container.remove();
  });

  it('returns false when an external editable control owns focus', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    Object.defineProperty(container, 'getBoundingClientRect', {
      value: () => ({ width: 800, height: 600 }),
    });

    const input = document.createElement('input');
    document.body.appendChild(input);

    expect(shouldAutoFocusTerminal(container, input)).toBe(false);

    input.remove();
    container.remove();
  });

  it('allows focus transfer from another terminal helper input', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    Object.defineProperty(container, 'getBoundingClientRect', {
      value: () => ({ width: 800, height: 600 }),
    });

    const oldTerminal = document.createElement('div');
    oldTerminal.className = 'xterm';
    const helperInput = document.createElement('textarea');
    oldTerminal.appendChild(helperInput);
    document.body.appendChild(oldTerminal);

    expect(shouldAutoFocusTerminal(container, helperInput)).toBe(true);

    oldTerminal.remove();
    container.remove();
  });
});

describe('shouldFocusTerminalFromClick', () => {
  it('returns false for interactive click targets', () => {
    const button = document.createElement('button');
    expect(shouldFocusTerminalFromClick(button, null)).toBe(false);
  });

  it('returns false when the user is selecting text', () => {
    expect(shouldFocusTerminalFromClick(document.createElement('div'), {
      isCollapsed: false,
      toString: () => 'selected text',
    } as Selection)).toBe(false);
  });

  it('returns true for plain terminal container clicks', () => {
    expect(shouldFocusTerminalFromClick(document.createElement('div'), null)).toBe(true);
  });
});
