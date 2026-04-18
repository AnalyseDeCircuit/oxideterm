import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMutableSelectorStore } from '@/test/helpers/mockStore';

const ideStoreState = vi.hoisted(() => ({
  nodeId: 'node-1' as string | null,
  project: {
    rootPath: '/srv/app',
    name: 'app',
    isGitRepo: false,
  },
  tabs: [] as Array<{ isDirty?: boolean }>,
  expandedPaths: new Set<string>(['/srv/app', '/srv/app/download']),
  treeRefreshSignal: {},
  changeRootPath: vi.fn(),
  createFile: vi.fn(),
  createFolder: vi.fn(),
  deleteItem: vi.fn(),
  renameItem: vi.fn(),
  getAffectedTabs: vi.fn(() => ({ affected: [], unsaved: [] })),
  openFile: vi.fn(async () => undefined),
  togglePath: vi.fn(),
}));

const agentServiceMocks = vi.hoisted(() => ({
  listDir: vi.fn(),
  symbolIndex: vi.fn().mockResolvedValue(null),
}));

const gitStatusMocks = vi.hoisted(() => ({
  refresh: vi.fn(),
}));

vi.mock('@/store/ideStore', () => ({
  useIdeStore: createMutableSelectorStore(ideStoreState),
  useIdeProject: () => ideStoreState.project,
}));

vi.mock('@/lib/agentService', () => agentServiceMocks);

vi.mock('@/components/ide/hooks/useGitStatus', () => ({
  useGitStatus: () => ({
    status: null,
    getFileStatus: () => undefined,
    refresh: gitStatusMocks.refresh,
    isLoading: false,
  }),
}));

vi.mock('@/hooks/useToast', () => ({
  useToast: () => ({
    toast: vi.fn(),
  }),
}));

vi.mock('@/hooks/useConfirm', () => ({
  useConfirm: () => ({
    confirm: vi.fn().mockResolvedValue(true),
    ConfirmDialog: null,
  }),
}));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: {
    getState: () => ({
      nodeTerminalMap: new Map<string, string[]>(),
    }),
  },
}));

vi.mock('@/lib/terminalRegistry', () => ({
  findPaneBySessionId: vi.fn(() => null),
  writeToTerminal: vi.fn(),
}));

vi.mock('@/components/ide/dialogs/IdeRemoteFolderDialog', () => ({
  IdeRemoteFolderDialog: () => null,
}));

vi.mock('@/components/ide/dialogs/IdeDeleteConfirmDialog', () => ({
  IdeDeleteConfirmDialog: () => null,
}));

vi.mock('@/components/ide/IdeTreeContextMenu', () => ({
  IdeTreeContextMenu: () => null,
}));

vi.mock('@/components/ide/IdeInlineInput', () => ({
  IdeInlineInput: () => null,
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string | Record<string, unknown>) => {
      if (key === 'empty_directory') return '(empty)';
      if (typeof fallback === 'string') return fallback;
      return key;
    },
  }),
}));

import { IdeTree } from '@/components/ide/IdeTree';

describe('IdeTree', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    ideStoreState.project = {
      rootPath: '/srv/app',
      name: 'app',
      isGitRepo: false,
    };
    ideStoreState.nodeId = 'node-1';
    ideStoreState.tabs = [];
    ideStoreState.expandedPaths = new Set(['/srv/app', '/srv/app/download']);
  });

  it('renders a localized empty-directory placeholder for empty folders', async () => {
    agentServiceMocks.listDir.mockImplementation(async (_nodeId: string, path: string) => {
      if (path === '/srv/app') {
        return [
          {
            name: 'download',
            path: '/srv/app/download',
            file_type: 'Directory',
            size: 0,
            modified: 0,
            permissions: '755',
          },
        ];
      }

      if (path === '/srv/app/download') {
        return [];
      }

      return [];
    });

    render(<IdeTree />);

    await waitFor(() => {
      expect(screen.getByText('(empty)')).toBeInTheDocument();
    });
    expect(screen.queryByText('ide.empty_directory')).not.toBeInTheDocument();
  });

  it('renders real child entries instead of the empty placeholder for non-empty folders', async () => {
    agentServiceMocks.listDir.mockImplementation(async (_nodeId: string, path: string) => {
      if (path === '/srv/app') {
        return [
          {
            name: 'download',
            path: '/srv/app/download',
            file_type: 'Directory',
            size: 0,
            modified: 0,
            permissions: '755',
          },
        ];
      }

      if (path === '/srv/app/download') {
        return [
          {
            name: 'report.txt',
            path: '/srv/app/download/report.txt',
            file_type: 'File',
            size: 42,
            modified: 0,
            permissions: '644',
          },
        ];
      }

      return [];
    });

    render(<IdeTree />);

    await waitFor(() => {
      expect(screen.getByText('report.txt')).toBeInTheDocument();
    });
    expect(screen.queryByText('(empty)')).not.toBeInTheDocument();
  });
});