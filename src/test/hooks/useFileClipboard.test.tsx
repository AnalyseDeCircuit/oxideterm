import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { copyFile, mkdir, readDir, rename } from '@tauri-apps/plugin-fs';
import { useFileClipboard } from '@/components/fileManager/hooks/useFileClipboard';
import type { FileInfo } from '@/components/fileManager/types';

function makeFile(overrides: Partial<FileInfo> = {}): FileInfo {
  return {
    name: 'report.txt',
    path: 'C:\\src\\report.txt',
    file_type: 'File',
    size: 12,
    modified: 0,
    permissions: '',
    ...overrides,
  };
}

describe('useFileClipboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(copyFile).mockResolvedValue(undefined as never);
    vi.mocked(rename).mockResolvedValue(undefined as never);
    vi.mocked(mkdir).mockResolvedValue(undefined as never);
    vi.mocked(readDir).mockResolvedValue([] as never);
  });

  it('copies a Windows file without mixing path separators', async () => {
    const { result } = renderHook(() => useFileClipboard());

    act(() => {
      result.current.copy([makeFile()], 'C:\\src');
    });

    await act(async () => {
      await result.current.paste('D:\\dest');
    });

    expect(copyFile).toHaveBeenCalledWith('C:\\src\\report.txt', 'D:\\dest\\report.txt');
  });

  it('recursively copies Windows directories using normalized child paths', async () => {
    const directory = makeFile({ name: 'folder', path: 'C:\\src\\folder', file_type: 'Directory' });
    vi.mocked(readDir).mockImplementation(async (path: string) => {
      if (path === 'C:\\src\\folder') {
        return [{ name: 'nested.txt', isDirectory: false }] as never;
      }
      return [] as never;
    });

    const { result } = renderHook(() => useFileClipboard());

    act(() => {
      result.current.copy([directory], 'C:\\src');
    });

    await act(async () => {
      await result.current.paste('D:\\dest');
    });

    expect(mkdir).toHaveBeenCalledWith('D:\\dest\\folder', { recursive: true });
    expect(copyFile).toHaveBeenCalledWith('C:\\src\\folder\\nested.txt', 'D:\\dest\\folder\\nested.txt');
  });

  it('treats symlinked directories as leaf entries to avoid recursive loops', async () => {
    const directory = makeFile({ name: 'folder', path: 'C:\\src\\folder', file_type: 'Directory' });
    vi.mocked(readDir).mockImplementation(async (path: string) => {
      if (path === 'C:\\src\\folder') {
        return [{ name: 'linked-dir', isDirectory: true, isSymlink: true }] as never;
      }
      return [] as never;
    });

    const { result } = renderHook(() => useFileClipboard());

    act(() => {
      result.current.copy([directory], 'C:\\src');
    });

    await act(async () => {
      await result.current.paste('D:\\dest');
    });

    expect(readDir).toHaveBeenCalledTimes(2);
    expect(copyFile).toHaveBeenCalledWith('C:\\src\\folder\\linked-dir', 'D:\\dest\\folder\\linked-dir');
  });

  it('treats cut+paste in the same directory as a no-op and clears the clipboard', async () => {
    const { result } = renderHook(() => useFileClipboard());

    act(() => {
      result.current.cut([makeFile()], 'C:\\src');
    });

    await act(async () => {
      await result.current.paste('C:\\src');
    });

    expect(rename).not.toHaveBeenCalled();
    expect(result.current.hasClipboard).toBe(false);
    expect(result.current.clipboardMode).toBeNull();
  });

  it('treats equivalent Windows paths as the same directory for cut+paste', async () => {
    const { result } = renderHook(() => useFileClipboard());

    act(() => {
      result.current.cut([makeFile()], 'C:\\Src\\');
    });

    await act(async () => {
      await result.current.paste('c:/Src');
    });

    expect(rename).not.toHaveBeenCalled();
    expect(result.current.hasClipboard).toBe(false);
  });

  it('retries with a numbered suffix when the destination already exists', async () => {
    vi.mocked(copyFile)
      .mockRejectedValueOnce(new Error('EEXIST: destination already exists'))
      .mockResolvedValueOnce(undefined as never);

    const { result } = renderHook(() => useFileClipboard());

    act(() => {
      result.current.copy([makeFile()], 'C:\\src');
    });

    await act(async () => {
      await result.current.paste('D:\\dest');
    });

    expect(copyFile).toHaveBeenNthCalledWith(1, 'C:\\src\\report.txt', 'D:\\dest\\report.txt');
    expect(copyFile).toHaveBeenNthCalledWith(2, 'C:\\src\\report.txt', 'D:\\dest\\report (1).txt');
  });

  it('rejects pasting a folder into itself or one of its descendants', async () => {
    const onError = vi.fn();
    const directory = makeFile({ name: 'folder', path: 'C:\\src\\folder', file_type: 'Directory' });
    const { result } = renderHook(() => useFileClipboard({ onError }));

    act(() => {
      result.current.copy([directory], 'C:\\src');
    });

    await act(async () => {
      await result.current.paste('C:\\src\\folder\\nested');
    });

    expect(mkdir).not.toHaveBeenCalled();
    expect(copyFile).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith('Paste Error', expect.stringContaining('Cannot copy folder into itself or a subdirectory'));
  });

  it('reports partial paste failures and keeps processing remaining files', async () => {
    const onError = vi.fn();
    const onSuccess = vi.fn();
    vi.mocked(copyFile)
      .mockRejectedValueOnce(new Error('EACCES: permission denied'))
      .mockResolvedValueOnce(undefined as never);

    const { result } = renderHook(() => useFileClipboard({ onError, onSuccess }));

    act(() => {
      result.current.copy([
        makeFile({ name: 'blocked.txt', path: 'C:\\src\\blocked.txt' }),
        makeFile({ name: 'ok.txt', path: 'C:\\src\\ok.txt' }),
      ], 'C:\\src');
    });

    await act(async () => {
      await result.current.paste('D:\\dest');
    });

    expect(copyFile).toHaveBeenNthCalledWith(1, 'C:\\src\\blocked.txt', 'D:\\dest\\blocked.txt');
    expect(copyFile).toHaveBeenNthCalledWith(2, 'C:\\src\\ok.txt', 'D:\\dest\\ok.txt');
    expect(onSuccess).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith('Paste Error', expect.stringContaining('blocked.txt'));
  });
});