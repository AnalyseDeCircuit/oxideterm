import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { useFileSelection } from '@/components/fileManager/hooks/useFileSelection';
import type { FileInfo } from '@/components/fileManager/types';

function makeFiles(names: string[]): FileInfo[] {
  return names.map((name, index) => ({
    name,
    path: `/tmp/${name}`,
    file_type: 'File',
    size: index,
    modified: 0,
    permissions: '',
  }));
}

describe('useFileSelection', () => {
  it('supports range selection across the current file list', () => {
    const files = makeFiles(['a.txt', 'b.txt', 'c.txt', 'd.txt']);
    const { result } = renderHook(({ items }) => useFileSelection({ files: items }), {
      initialProps: { items: files },
    });

    act(() => {
      result.current.select('b.txt', false, false);
      result.current.select('d.txt', false, true);
    });

    expect(Array.from(result.current.selected)).toEqual(['b.txt', 'c.txt', 'd.txt']);
  });

  it('falls back to selecting the target when the previous range anchor disappeared', () => {
    const { result, rerender } = renderHook(({ items }) => useFileSelection({ files: items }), {
      initialProps: { items: makeFiles(['a.txt', 'b.txt', 'c.txt']) },
    });

    act(() => {
      result.current.select('b.txt', false, false);
    });

    rerender({ items: makeFiles(['c.txt', 'd.txt']) });

    act(() => {
      result.current.select('d.txt', false, true);
    });

    expect(Array.from(result.current.selected)).toEqual(['d.txt']);
    expect(result.current.lastSelected).toBe('d.txt');
  });

  it('prunes selections that are no longer present after refreshes', () => {
    const { result, rerender } = renderHook(({ items }) => useFileSelection({ files: items }), {
      initialProps: { items: makeFiles(['keep.txt', 'drop.txt']) },
    });

    act(() => {
      result.current.select('keep.txt', false, false);
      result.current.select('drop.txt', true, false);
    });

    rerender({ items: makeFiles(['keep.txt']) });

    expect(Array.from(result.current.selected)).toEqual(['keep.txt']);
    expect(result.current.lastSelected).toBeNull();
  });
});