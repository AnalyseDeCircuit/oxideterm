import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMutableSelectorStore } from '@/test/helpers/mockStore';

const connectToSavedMock = vi.hoisted(() => vi.fn());
const continueConnectToSavedPlanMock = vi.hoisted(() => vi.fn());
const toastMock = vi.hoisted(() => vi.fn());

const sessionManagerState = vi.hoisted(() => ({
  connections: [{
    id: 'conn-1',
    name: 'Test Conn',
    group: null,
    host: 'example.com',
    port: 22,
    username: 'tester',
    auth_type: 'password',
    key_path: null,
    cert_path: null,
    created_at: '2026-01-01T00:00:00Z',
    last_used_at: null,
    color: null,
    tags: [],
    proxy_chain: [],
  }],
  allConnections: [{
    id: 'conn-1',
    name: 'Test Conn',
    group: null,
    host: 'example.com',
    port: 22,
    username: 'tester',
    auth_type: 'password',
    key_path: null,
    cert_path: null,
    created_at: '2026-01-01T00:00:00Z',
    last_used_at: null,
    color: null,
    tags: [],
    proxy_chain: [],
  }],
  groups: [],
  loading: false,
  folderTree: [],
  ungroupedCount: 1,
  selectedGroup: null as string | null,
  setSelectedGroup: vi.fn(),
  expandedGroups: new Set<string>(),
  toggleExpand: vi.fn(),
  expandPath: vi.fn(),
  searchQuery: '',
  setSearchQuery: vi.fn(),
  sortField: 'last_used_at',
  sortDirection: 'desc' as const,
  toggleSort: vi.fn(),
  selectedIds: new Set<string>(),
  toggleSelect: vi.fn(),
  toggleSelectAll: vi.fn(),
  clearSelection: vi.fn(),
  refresh: vi.fn().mockResolvedValue(undefined),
}));

const appStoreState = vi.hoisted(() => ({
  createTab: vi.fn(),
}));

vi.mock('@/components/sessionManager/useSessionManager', () => ({
  useSessionManager: () => sessionManagerState,
}));

vi.mock('@/hooks/useToast', () => ({
  useToast: () => ({ toast: toastMock }),
}));

vi.mock('@/hooks/useConfirm', () => ({
  useConfirm: () => ({
    confirm: vi.fn().mockResolvedValue(true),
    ConfirmDialog: null,
  }),
}));

vi.mock('@/hooks/useTabBackground', () => ({
  useTabBgActive: () => false,
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: createMutableSelectorStore(appStoreState),
}));

vi.mock('@/lib/connectToSaved', () => ({
  connectToSaved: connectToSavedMock,
  continueConnectToSavedPlan: continueConnectToSavedPlanMock,
}));

vi.mock('@/components/sessionManager/FolderTree', () => ({
  FolderTree: ({ onRequestCreateGroup }: { onRequestCreateGroup?: () => void }) => (
    <button onClick={onRequestCreateGroup}>new-group</button>
  ),
}));

vi.mock('@/components/sessionManager/ManagerToolbar', () => ({
  ManagerToolbar: () => <div>toolbar</div>,
}));

vi.mock('@/components/sessionManager/ConnectionTable', () => ({
  ConnectionTable: ({ onConnect, onDelete, onTestConnection, connections }: { onConnect: (id: string) => void; onDelete: (conn: { id: string; name: string }) => void; onTestConnection?: (conn: { id: string; name: string }) => void; connections: Array<{ id: string; name: string }> }) => (
    <>
      <button onClick={() => onConnect('conn-1')}>connect-row</button>
      <button onClick={() => onTestConnection?.(connections[0])}>test-row</button>
      <button onClick={() => onDelete(connections[0])}>delete-row</button>
    </>
  ),
}));

vi.mock('@/components/modals/EditConnectionModal', () => ({
  EditConnectionModal: ({ open, connection, action, onSubmit }: { open: boolean; connection: { id: string; name?: string; host?: string; port?: number; username?: string } | null; action?: 'connect' | 'test'; onSubmit?: (payload: { connection: { id: string; name: string; host: string; port: number; username: string }; authType: 'password'; password: string }) => Promise<void> }) => (
    open ? (
      <div>
        <div data-testid="connect-modal" data-action={action ?? 'connect'}>{connection?.id}</div>
        <button onClick={() => connection && onSubmit?.({
          connection: {
            id: connection.id,
            name: connection.name ?? 'Test Conn',
            host: connection.host ?? 'example.com',
            port: connection.port ?? 22,
            username: connection.username ?? 'tester',
          },
          authType: 'password',
          password: 'secret',
        })}>submit-connect-modal</button>
      </div>
    ) : null
  ),
}));

vi.mock('@/components/modals/HostKeyConfirmDialog', () => ({
  HostKeyConfirmDialog: ({
    open,
    host,
    port,
    status,
    onAccept,
    onRemoveSavedKey,
  }: {
    open: boolean;
    host: string;
    port: number;
    status?: { status: string } | null;
    onAccept?: (persist: boolean) => void;
    onRemoveSavedKey?: () => void;
  }) => (
    open ? <div data-testid="host-key-dialog">
      {host}:{port}
      {status?.status === 'changed' ? <button onClick={onRemoveSavedKey}>remove-saved-key</button> : null}
      {status?.status === 'unknown' ? <button onClick={() => onAccept?.(true)}>accept-host-key</button> : null}
    </div> : null
  ),
}));

vi.mock('@/components/modals/EditConnectionPropertiesModal', () => ({
  EditConnectionPropertiesModal: ({ open, connection }: { open: boolean; connection: { id: string } | null }) => (
    open ? <div data-testid="properties-modal">{connection?.id}</div> : null
  ),
}));

vi.mock('@/components/modals/OxideExportModal', () => ({
  OxideExportModal: () => null,
}));

vi.mock('@/components/modals/OxideImportModal', () => ({
  OxideImportModal: () => null,
}));

vi.mock('@/lib/api', () => ({
  api: {
    saveConnection: vi.fn(),
    createGroup: vi.fn(),
    deleteConnection: vi.fn(),
    getSavedConnectionForConnect: vi.fn(),
    sshPreflight: vi.fn(),
    sshRemoveHostKey: vi.fn(),
    testConnection: vi.fn(),
  },
}));

vi.mock('react-i18next', () => ({
  initReactI18next: { type: '3rdParty', init: () => undefined },
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

import { SessionManagerPanel } from '@/components/sessionManager/SessionManagerPanel';
import { api } from '@/lib/api';

describe('SessionManagerPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.sshPreflight).mockResolvedValue({ status: 'verified' });
  });

  it('shows saved-connection host key confirmation and resumes via continueConnectToSavedPlan', async () => {
    connectToSavedMock.mockImplementation(async (_id: string, options: { onHostKeyChallenge?: (challenge: unknown) => void }) => {
      options.onHostKeyChallenge?.({
        pendingPlan: {
          connectionId: 'conn-1',
          plan: {
            targetNodeId: 'node-target',
            cleanupNodeId: 'node-target',
            currentIndex: 1,
            steps: [
              { nodeId: 'node-jump', host: 'jump.example.com', port: 22 },
              { nodeId: 'node-target', host: 'target.example.com', port: 22 },
            ],
          },
        },
        host: 'target.example.com',
        port: 22,
        status: {
          status: 'unknown',
          fingerprint: 'SHA256:target',
          keyType: 'ssh-ed25519',
        },
      });
    });
    continueConnectToSavedPlanMock.mockResolvedValue({ nodeId: 'node-target', sessionId: 'term-target' });

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('connect-row'));

    await waitFor(() => {
      expect(screen.getByTestId('host-key-dialog')).toHaveTextContent('target.example.com:22');
    });

    fireEvent.click(screen.getByText('accept-host-key'));

    await waitFor(() => {
      expect(continueConnectToSavedPlanMock).toHaveBeenCalledWith(expect.objectContaining({
        plan: expect.objectContaining({
          currentIndex: 1,
          steps: [
            expect.objectContaining({ nodeId: 'node-jump' }),
            expect.objectContaining({
              nodeId: 'node-target',
              trustHostKey: true,
              expectedHostKeyFingerprint: 'SHA256:target',
            }),
          ],
        }),
      }), expect.any(Object));
    });
  });

  it('opens the connect password modal instead of the properties modal for missing-password failures', async () => {
    connectToSavedMock.mockImplementation(async (_id: string, options: { onError?: (id: string, reason?: 'missing-password' | 'connect-failed') => void }) => {
      options.onError?.('conn-1', 'missing-password');
    });

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('connect-row'));

    await waitFor(() => {
      expect(screen.getByTestId('connect-modal')).toHaveTextContent('conn-1');
    });
    expect(screen.queryByTestId('properties-modal')).toBeNull();
  });

  it('opens the password modal in test mode when testing a saved password connection without a stored password', async () => {
    vi.mocked(api.getSavedConnectionForConnect).mockResolvedValue({
      name: 'Test Conn',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'password',
      agent_forwarding: false,
      proxy_chain: [],
    });

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('test-row'));

    await waitFor(() => {
      expect(screen.getByTestId('connect-modal')).toHaveTextContent('conn-1');
      expect(screen.getByTestId('connect-modal')).toHaveAttribute('data-action', 'test');
    });
    expect(api.testConnection).not.toHaveBeenCalled();
    expect(screen.queryByTestId('properties-modal')).toBeNull();
  });

  it('submits prompted credentials into testConnection after opening the test modal', async () => {
    vi.mocked(api.getSavedConnectionForConnect).mockResolvedValue({
      name: 'Test Conn',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'password',
      agent_forwarding: false,
      proxy_chain: [],
    });
    vi.mocked(api.testConnection).mockResolvedValue({
      success: true,
      elapsedMs: 12,
      diagnostic: {
        phase: 'complete',
        category: 'success',
        summary: 'Connection test succeeded',
        detail: 'Connected successfully',
      },
    });

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('test-row'));

    await waitFor(() => {
      expect(screen.getByTestId('connect-modal')).toHaveAttribute('data-action', 'test');
    });

    fireEvent.click(screen.getByText('submit-connect-modal'));

    await waitFor(() => {
      expect(api.testConnection).toHaveBeenCalledWith({
        host: 'example.com',
        port: 22,
        username: 'tester',
        name: 'Test Conn',
        auth_type: 'password',
        password: 'secret',
      });
    });
  });

  it('shows host key confirmation before running a test on an unknown host', async () => {
    vi.mocked(api.getSavedConnectionForConnect).mockResolvedValue({
      name: 'Test Conn',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'agent',
      agent_forwarding: false,
      proxy_chain: [],
    });
    vi.mocked(api.sshPreflight).mockResolvedValue({
      status: 'unknown',
      fingerprint: 'SHA256:test',
      keyType: 'ssh-ed25519',
    });

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('test-row'));

    await waitFor(() => {
      expect(screen.getByTestId('host-key-dialog')).toHaveTextContent('example.com:22');
    });
    expect(api.testConnection).not.toHaveBeenCalled();
  });

  it('shows host key confirmation before running a test on a changed host key', async () => {
    vi.mocked(api.getSavedConnectionForConnect).mockResolvedValue({
      name: 'Test Conn',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'agent',
      agent_forwarding: false,
      proxy_chain: [],
    });
    vi.mocked(api.sshPreflight).mockResolvedValue({
      status: 'changed',
      expectedFingerprint: 'SHA256:old',
      actualFingerprint: 'SHA256:new',
      keyType: 'ssh-ed25519',
    });

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('test-row'));

    await waitFor(() => {
      expect(screen.getByTestId('host-key-dialog')).toHaveTextContent('example.com:22');
    });
    expect(api.testConnection).not.toHaveBeenCalled();
  });

  it('removes the saved host key and re-runs preflight from the changed-key dialog', async () => {
    vi.mocked(api.getSavedConnectionForConnect).mockResolvedValue({
      name: 'Test Conn',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'agent',
      agent_forwarding: false,
      proxy_chain: [],
    });
    vi.mocked(api.sshPreflight)
      .mockResolvedValueOnce({
        status: 'changed',
        expectedFingerprint: 'SHA256:old',
        actualFingerprint: 'SHA256:new',
        keyType: 'ssh-ed25519',
      })
      .mockResolvedValueOnce({
        status: 'unknown',
        fingerprint: 'SHA256:new',
        keyType: 'ssh-ed25519',
      });

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('test-row'));

    await waitFor(() => {
      expect(screen.getByTestId('host-key-dialog')).toHaveTextContent('example.com:22');
    });

    fireEvent.click(screen.getByText('remove-saved-key'));

    await waitFor(() => {
      expect(api.sshRemoveHostKey).toHaveBeenCalledWith({
        host: 'example.com',
        port: 22,
        keyType: 'ssh-ed25519',
        expectedFingerprint: 'SHA256:old',
      });
    });

    expect(api.sshPreflight).toHaveBeenNthCalledWith(2, {
      host: 'example.com',
      port: 22,
    });
  });

  it('can continue from changed to unknown and then run the test with the new fingerprint', async () => {
    vi.mocked(api.getSavedConnectionForConnect).mockResolvedValue({
      name: 'Test Conn',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'agent',
      agent_forwarding: false,
      proxy_chain: [],
    });
    vi.mocked(api.sshPreflight)
      .mockResolvedValueOnce({
        status: 'changed',
        expectedFingerprint: 'SHA256:old',
        actualFingerprint: 'SHA256:new',
        keyType: 'ssh-ed25519',
      })
      .mockResolvedValueOnce({
        status: 'unknown',
        fingerprint: 'SHA256:new',
        keyType: 'ssh-ed25519',
      });

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('test-row'));

    await waitFor(() => {
      expect(screen.getByTestId('host-key-dialog')).toHaveTextContent('example.com:22');
    });

    fireEvent.click(screen.getByText('remove-saved-key'));

    await waitFor(() => {
      expect(screen.getByText('accept-host-key')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText('accept-host-key'));

    await waitFor(() => {
      expect(api.testConnection).toHaveBeenCalledWith(expect.objectContaining({
        host: 'example.com',
        port: 22,
        trust_host_key: true,
        expected_host_key_fingerprint: 'SHA256:new',
      }));
    });
  });

  it('bypasses direct preflight and sends proxy hops for jump-host tests', async () => {
    vi.mocked(api.getSavedConnectionForConnect).mockResolvedValue({
      name: 'Jump Target',
      host: 'target.example.com',
      port: 22,
      username: 'target-user',
      auth_type: 'agent',
      agent_forwarding: false,
      proxy_chain: [
        {
          host: 'jump-1.example.com',
          port: 22,
          username: 'jump1',
          auth_type: 'password',
          password: 'secret',
          agent_forwarding: false,
        },
      ],
    });
    vi.mocked(api.testConnection).mockResolvedValue({
      success: false,
      elapsedMs: 15,
      diagnostic: {
        phase: 'transport',
        category: 'tunnel',
        summary: 'Tunnel from jump host 1 to the target failed',
        detail: 'mock tunnel failure',
      },
    });

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('test-row'));

    await waitFor(() => {
      expect(api.testConnection).toHaveBeenCalledWith({
        name: 'Jump Target',
        host: 'target.example.com',
        port: 22,
        username: 'target-user',
        auth_type: 'agent',
        proxy_chain: [
          {
            host: 'jump-1.example.com',
            port: 22,
            username: 'jump1',
            auth_type: 'password',
            password: 'secret',
          },
        ],
      });
    });
    expect(api.sshPreflight).not.toHaveBeenCalled();
  });

  it('blocks unsupported proxy-hop keyboard-interactive auth before starting a test', async () => {
    vi.mocked(api.getSavedConnectionForConnect).mockResolvedValue({
      name: 'Jump Target',
      host: 'target.example.com',
      port: 22,
      username: 'target-user',
      auth_type: 'agent',
      agent_forwarding: false,
      proxy_chain: [
        {
          host: 'jump-1.example.com',
          port: 22,
          username: 'jump1',
          auth_type: 'keyboard_interactive',
          agent_forwarding: false,
        },
      ],
    } as never);

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('test-row'));

    await waitFor(() => {
      expect(toastMock).toHaveBeenCalledWith(expect.objectContaining({
        title: 'sessionManager.toast.test_failed',
        description: 'sessionManager.toast.proxy_hop_kbi_unsupported',
        variant: 'error',
      }));
    });
    expect(api.testConnection).not.toHaveBeenCalled();
    expect(api.sshPreflight).not.toHaveBeenCalled();
  });

  it('broadcasts saved connection changes after deleting a connection', async () => {
    const dispatchEventSpy = vi.spyOn(window, 'dispatchEvent');

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('delete-row'));

    await waitFor(() => {
      expect(api.deleteConnection).toHaveBeenCalledWith('conn-1');
      expect(sessionManagerState.refresh).toHaveBeenCalled();
      expect(dispatchEventSpy).toHaveBeenCalled();
    });
  });

  it('creates a group from the folder tree shortcut and selects it', async () => {
    const dispatchEventSpy = vi.spyOn(window, 'dispatchEvent');

    render(<SessionManagerPanel />);
    fireEvent.click(screen.getByText('new-group'));

    fireEvent.change(screen.getByPlaceholderText('sessionManager.folder_tree.new_group_placeholder'), {
      target: { value: 'Production/Core' },
    });
    fireEvent.click(screen.getByText('common.create'));

    await waitFor(() => {
      expect(api.createGroup).toHaveBeenCalledWith('Production/Core');
      expect(sessionManagerState.refresh).toHaveBeenCalled();
      expect(sessionManagerState.expandPath).toHaveBeenCalledWith('Production/Core');
      expect(sessionManagerState.setSelectedGroup).toHaveBeenCalledWith('Production/Core');
      expect(dispatchEventSpy).toHaveBeenCalled();
    });
  });
});