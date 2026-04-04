import { describe, expect, it } from 'vitest';
import { hexToRgba, getBackgroundFitStyles } from '@/lib/terminalHelpers';

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
