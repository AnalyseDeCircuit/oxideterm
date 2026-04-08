import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { FileList } from '@/components/fileManager/FileList';
import type { FileInfo } from '@/components/fileManager/types';

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 28,
    getVirtualItems: () => Array.from({ length: count }, (_, index) => ({
      index,
      key: index,
      start: index * 28,
    })),
  }),
}));

function makeFile(overrides: Partial<FileInfo> = {}): FileInfo {
  return {
    name: 'folder',
    path: 'C:\\Users\\tester\\folder',
    file_type: 'Directory',
    size: 0,
    modified: 0,
    permissions: '',
    ...overrides,
  };
}

function renderFileList(props: Partial<React.ComponentProps<typeof FileList>> = {}) {
  return render(
    <FileList
      title="Local"
      files={[makeFile()]}
      path="C:\\Users\\tester"
      isRemote={false}
      active
      loading={false}
      error={null}
      selected={new Set()}
      lastSelected={null}
      onSelect={vi.fn()}
      onSelectAll={vi.fn()}
      onClearSelection={vi.fn()}
      onNavigate={vi.fn()}
      onRefresh={vi.fn()}
      t={(key) => key}
      {...props}
    />,
  );
}

describe('FileList', () => {
  it('navigates local Windows directories without mixing separators', () => {
    const onNavigate = vi.fn();
    renderFileList({ onNavigate });

    fireEvent.doubleClick(screen.getByText('folder'));

    expect(onNavigate).toHaveBeenCalledWith('C:\\Users\\tester\\folder');
  });

  it('shows the inline error state for permission failures and supports refresh', () => {
    const onRefresh = vi.fn();
    renderFileList({ files: [], error: 'Permission denied', onRefresh });

    expect(screen.getByText('fileManager.error')).toBeInTheDocument();
    expect(screen.getByText('Permission denied')).toBeInTheDocument();

    fireEvent.click(screen.getAllByRole('button', { name: /fileManager.refresh/i })[1]);

    expect(onRefresh).toHaveBeenCalled();
  });
});