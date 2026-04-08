import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const apiMocks = vi.hoisted(() => ({
  localGetDrives: vi.fn().mockResolvedValue([]),
}));

vi.mock('@/lib/api', () => ({ api: apiMocks }));
vi.mock('@tauri-apps/api/path', () => ({ homeDir: vi.fn().mockResolvedValue('/Users/tester') }));

import { mkdir, readDir, remove, rename, stat } from '@tauri-apps/plugin-fs';
import { useLocalFiles } from '@/components/fileManager/hooks/useLocalFiles';

describe('useLocalFiles', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(readDir).mockResolvedValue([]);
    vi.mocked(stat).mockResolvedValue({ size: 0, mtime: new Date('2026-01-01T00:00:00Z') } as never);
    vi.mocked(mkdir).mockResolvedValue(undefined as never);
    vi.mocked(remove).mockResolvedValue(undefined as never);
    vi.mocked(rename).mockResolvedValue(undefined as never);
  });

  it('builds Windows child paths with backslashes during refresh', async () => {
    vi.mocked(readDir).mockResolvedValue([
      { name: 'notes.txt', isDirectory: false, isSymlink: false },
    ] as never);

    renderHook(() => useLocalFiles({ initialPath: 'C:\\Users\\tester' }));

    await waitFor(() => {
      expect(stat).toHaveBeenCalledWith('C:\\Users\\tester\\notes.txt');
    });
  });

  it('builds UNC child paths without corrupting the share prefix', async () => {
    vi.mocked(readDir).mockResolvedValue([
      { name: 'notes.txt', isDirectory: false, isSymlink: false },
    ] as never);

    renderHook(() => useLocalFiles({ initialPath: '\\\\server\\share\\docs' }));

    await waitFor(() => {
      expect(stat).toHaveBeenCalledWith('\\\\server\\share\\docs\\notes.txt');
    });
  });

  it('creates, deletes, and renames using the normalized platform path', async () => {
    const { result } = renderHook(() => useLocalFiles({ initialPath: 'C:\\Users\\tester' }));

    await act(async () => {
      await result.current.createFolder('docs');
      await result.current.deleteFiles(['docs']);
      await result.current.renameFile('old.txt', 'new.txt');
    });

    expect(mkdir).toHaveBeenCalledWith('C:\\Users\\tester\\docs');
    expect(remove).toHaveBeenCalledWith('C:\\Users\\tester\\docs', { recursive: true });
    expect(rename).toHaveBeenCalledWith('C:\\Users\\tester\\old.txt', 'C:\\Users\\tester\\new.txt');
  });

  it('ignores stale permission errors after navigating away to a new directory', async () => {
    let rejectLocked: ((reason?: unknown) => void) | undefined;
    const lockedPromise = new Promise<never>((_, reject) => {
      rejectLocked = reject;
    });

    vi.mocked(readDir).mockImplementation((targetPath: string) => {
      if (targetPath === 'C:\\locked') {
        return lockedPromise as never;
      }
      if (targetPath === 'C:\\allowed') {
        return Promise.resolve([{ name: 'ok.txt', isDirectory: false, isSymlink: false }]) as never;
      }
      return Promise.resolve([]) as never;
    });

    const { result } = renderHook(() => useLocalFiles({ initialPath: 'C:\\locked' }));

    act(() => {
      result.current.navigate('C:/allowed');
    });

    await waitFor(() => {
      expect(result.current.path).toBe('C:\\allowed');
      expect(result.current.displayFiles.map(file => file.name)).toEqual(['ok.txt']);
    });

    rejectLocked?.(new Error('Permission denied'));

    await waitFor(() => {
      expect(result.current.error).toBeNull();
      expect(result.current.path).toBe('C:\\allowed');
    });
  });
});