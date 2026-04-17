import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createMutableSelectorStore } from '@/test/helpers/mockStore';

const apiMocks = vi.hoisted(() => ({
  getGroups: vi.fn().mockResolvedValue([]),
  isAgentAvailable: vi.fn().mockResolvedValue(true),
  sshPreflight: vi.fn(),
  preflightTreeNode: vi.fn(),
  sshRemoveHostKey: vi.fn().mockResolvedValue(undefined),
  testConnection: vi.fn(),
}));

const appStoreState = vi.hoisted(() => ({
  modals: { newConnection: true },
  toggleModal: vi.fn(),
  createTab: vi.fn(),
  quickConnectData: null as null | {
    alias?: string;
    name?: string;
    host: string;
    port: number;
    username: string;
    authType?: 'key' | 'agent' | 'certificate';
    keyPath?: string | null;
    certPath?: string | null;
    proxyChain?: Array<{
      id: string;
      host: string;
      port: number;
      username: string;
      auth_type: 'password' | 'key' | 'default_key' | 'agent';
      key_path?: string;
    }>;
  },
}));

const sessionTreeState = vi.hoisted(() => ({
  addRootNode: vi.fn(),
  connectNode: vi.fn(),
  createTerminalForNode: vi.fn(),
  expandManualPreset: vi.fn(),
  removeNode: vi.fn(),
}));

const toastState = vi.hoisted(() => ({
  error: vi.fn(),
  success: vi.fn(),
}));

const settingsStoreMock = vi.hoisted(() => ({
  getState: vi.fn(() => ({
    settings: {
      terminal: {
        scrollback: 3500,
      },
    },
  })),
}));

vi.mock('@/lib/api', () => ({ api: apiMocks }));
vi.mock('@/store/appStore', () => ({
  useAppStore: createMutableSelectorStore(appStoreState),
}));
vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: createMutableSelectorStore(sessionTreeState),
}));
vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: settingsStoreMock,
  deriveBackendHotLines: (scrollback: number) => Math.min(12000, Math.max(5000, scrollback * 2)),
}));
vi.mock('@/hooks/useToast', () => ({
  useToast: () => toastState,
}));
vi.mock('@/components/modals/AddJumpServerDialog', () => ({
  AddJumpServerDialog: () => null,
}));
vi.mock('@/components/modals/HostKeyConfirmDialog', () => ({
  HostKeyConfirmDialog: ({
    open,
    status,
    onAccept,
    onRemoveSavedKey,
  }: {
    open: boolean;
    status: { status: string } | null;
    onAccept?: (persist: boolean) => void;
    onRemoveSavedKey?: () => void;
  }) => (
    open ? <div>
      <div data-testid="host-key-dialog-state">{status?.status}</div>
      {status?.status === 'changed' ? <button onClick={onRemoveSavedKey}>remove-saved-key</button> : null}
      {status?.status === 'unknown' ? <button onClick={() => onAccept?.(true)}>accept-host-key</button> : null}
    </div> : null
  ),
}));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

import { NewConnectionModal } from '@/components/modals/NewConnectionModal';

describe('NewConnectionModal host key flows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    appStoreState.modals.newConnection = true;
    appStoreState.quickConnectData = null;
    apiMocks.getGroups.mockResolvedValue([]);
    apiMocks.isAgentAvailable.mockResolvedValue(true);
    apiMocks.preflightTreeNode.mockResolvedValue({ status: 'verified' });
    sessionTreeState.addRootNode.mockResolvedValue('node-1');
    sessionTreeState.connectNode.mockResolvedValue(undefined);
    sessionTreeState.createTerminalForNode.mockResolvedValue('terminal-1');
    sessionTreeState.expandManualPreset.mockResolvedValue({
      targetNodeId: 'node-target',
      pathNodeIds: ['node-jump', 'node-target'],
      chainDepth: 2,
    });
  });

  it('keeps the dialog visible if removing a changed key is followed by a preflight error', async () => {
    apiMocks.sshPreflight
      .mockResolvedValueOnce({
        status: 'changed',
        expectedFingerprint: 'SHA256:old',
        actualFingerprint: 'SHA256:new',
        keyType: 'ssh-ed25519',
      })
      .mockResolvedValueOnce({
        status: 'error',
        message: 'network down',
      });

    await act(async () => {
      render(<NewConnectionModal />);
    });

    fireEvent.change(screen.getByLabelText('modals.new_connection.target_host *'), {
      target: { value: 'server.example.com' },
    });
    fireEvent.change(screen.getByLabelText('modals.new_connection.target_username *'), {
      target: { value: 'alice' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'modals.new_connection.test' }));

    await waitFor(() => {
      expect(screen.getByTestId('host-key-dialog-state')).toHaveTextContent('changed');
    });

    fireEvent.click(screen.getByText('remove-saved-key'));

    await waitFor(() => {
      expect(apiMocks.sshRemoveHostKey).toHaveBeenCalledWith({
        host: 'server.example.com',
        port: 22,
        keyType: 'ssh-ed25519',
        expectedFingerprint: 'SHA256:old',
      });
    });

    await waitFor(() => {
      expect(screen.getByTestId('host-key-dialog-state')).toHaveTextContent('error');
    });
  });

  it('shows the host key dialog for an unknown host and continues testing after accept', async () => {
    apiMocks.sshPreflight.mockResolvedValueOnce({
      status: 'unknown',
      fingerprint: 'SHA256:new',
      keyType: 'ssh-ed25519',
    });
    apiMocks.testConnection.mockResolvedValue({
      success: true,
      latency_ms: 42,
      endpoint: { host: 'server.example.com', port: 22 },
      diagnostics: [],
    });

    await act(async () => {
      render(<NewConnectionModal />);
    });

    fireEvent.change(screen.getByLabelText('modals.new_connection.target_host *'), {
      target: { value: 'server.example.com' },
    });
    fireEvent.change(screen.getByLabelText('modals.new_connection.target_username *'), {
      target: { value: 'alice' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'modals.new_connection.test' }));

    await waitFor(() => {
      expect(screen.getByTestId('host-key-dialog-state')).toHaveTextContent('unknown');
    });

    fireEvent.click(screen.getByText('accept-host-key'));

    await waitFor(() => {
      expect(apiMocks.testConnection).toHaveBeenCalledWith(expect.objectContaining({
        host: 'server.example.com',
        port: 22,
        username: 'alice',
        auth_type: 'password',
        password: '',
        trust_host_key: true,
        expected_host_key_fingerprint: 'SHA256:new',
      }));
    });
  });

  it('can continue from changed to unknown and then run the test with the new fingerprint', async () => {
    apiMocks.sshPreflight
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
    apiMocks.testConnection.mockResolvedValue({
      success: true,
      latency_ms: 42,
      endpoint: { host: 'server.example.com', port: 22 },
      diagnostics: [],
    });

    await act(async () => {
      render(<NewConnectionModal />);
    });

    fireEvent.change(screen.getByLabelText('modals.new_connection.target_host *'), {
      target: { value: 'server.example.com' },
    });
    fireEvent.change(screen.getByLabelText('modals.new_connection.target_username *'), {
      target: { value: 'alice' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'modals.new_connection.test' }));

    await waitFor(() => {
      expect(screen.getByTestId('host-key-dialog-state')).toHaveTextContent('changed');
    });

    fireEvent.click(screen.getByText('remove-saved-key'));

    await waitFor(() => {
      expect(screen.getByText('accept-host-key')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText('accept-host-key'));

    await waitFor(() => {
      expect(apiMocks.testConnection).toHaveBeenCalledWith(expect.objectContaining({
        host: 'server.example.com',
        port: 22,
        username: 'alice',
        auth_type: 'password',
        password: '',
        trust_host_key: true,
        expected_host_key_fingerprint: 'SHA256:new',
      }));
    });
  });


  it('prefills auth and proxy chain when opened from resolved ssh config alias', async () => {
    appStoreState.quickConnectData = {
      alias: 'prod',
      name: 'prod',
      host: 'target.internal',
      port: 2200,
      username: 'alice',
      authType: 'certificate',
      keyPath: '/tmp/id_ed25519',
      certPath: '/tmp/id_ed25519-cert.pub',
      proxyChain: [
        {
          id: 'bob@jump.example.com:22',
          host: 'jump.example.com',
          port: 22,
          username: 'bob',
          auth_type: 'key',
          key_path: '/tmp/jump-key',
        },
      ],
    };

    await act(async () => {
      render(<NewConnectionModal />);
    });

    expect(screen.getByDisplayValue('prod')).toBeInTheDocument();
    expect(screen.getByDisplayValue('target.internal')).toBeInTheDocument();
    expect(screen.getByDisplayValue('2200')).toBeInTheDocument();
    expect(screen.getByDisplayValue('alice')).toBeInTheDocument();
    expect(screen.getByDisplayValue('/tmp/id_ed25519')).toBeInTheDocument();
    expect(screen.getByDisplayValue('/tmp/id_ed25519-cert.pub')).toBeInTheDocument();
    expect(screen.getByText('jump.example.com')).toBeInTheDocument();
    expect(screen.getByText('bob')).toBeInTheDocument();
  });
  it('preflights direct connect and forwards the accepted fingerprint into connectNode', async () => {
    apiMocks.sshPreflight
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

    await act(async () => {
      render(<NewConnectionModal />);
    });

    fireEvent.change(screen.getByLabelText('modals.new_connection.target_host *'), {
      target: { value: 'server.example.com' },
    });
    fireEvent.change(screen.getByLabelText('modals.new_connection.target_username *'), {
      target: { value: 'alice' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'modals.new_connection.connect' }));

    await waitFor(() => {
      expect(screen.getByTestId('host-key-dialog-state')).toHaveTextContent('changed');
    });

    fireEvent.click(screen.getByText('remove-saved-key'));

    await waitFor(() => {
      expect(screen.getByText('accept-host-key')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText('accept-host-key'));

    await waitFor(() => {
      expect(sessionTreeState.addRootNode).toHaveBeenCalledWith(expect.objectContaining({
        host: 'server.example.com',
        port: 22,
        username: 'alice',
        authType: 'password',
        password: '',
      }));
      expect(sessionTreeState.connectNode).toHaveBeenCalledWith('node-1', {
        trustHostKey: true,
        expectedHostKeyFingerprint: 'SHA256:new',
      });
    });

    expect(apiMocks.sshRemoveHostKey).toHaveBeenCalledWith({
      host: 'server.example.com',
      port: 22,
      keyType: 'ssh-ed25519',
      expectedFingerprint: 'SHA256:old',
    });
    expect(toastState.error).not.toHaveBeenCalled();
  });

  it('shows a connect error if the real connect fails after accepting an unknown host key', async () => {
    apiMocks.sshPreflight.mockResolvedValueOnce({
      status: 'unknown',
      fingerprint: 'SHA256:new',
      keyType: 'ssh-ed25519',
    });
    sessionTreeState.connectNode.mockRejectedValueOnce(new Error('connect boom'));

    await act(async () => {
      render(<NewConnectionModal />);
    });

    fireEvent.change(screen.getByLabelText('modals.new_connection.target_host *'), {
      target: { value: 'server.example.com' },
    });
    fireEvent.change(screen.getByLabelText('modals.new_connection.target_username *'), {
      target: { value: 'alice' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'modals.new_connection.connect' }));

    await waitFor(() => {
      expect(screen.getByText('accept-host-key')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText('accept-host-key'));

    await waitFor(() => {
      expect(toastState.error).toHaveBeenCalledWith(
        'modals.new_connection.connect_failed',
        'Error: connect boom',
      );
    });

    expect(screen.queryByText('accept-host-key')).not.toBeInTheDocument();
  });

  it('prompts for a later proxy hop host key and resumes the chain after accept', async () => {
    appStoreState.quickConnectData = {
      name: 'proxy-target',
      host: 'target.internal',
      port: 22,
      username: 'alice',
      proxyChain: [
        {
          id: 'jump-1',
          host: 'jump.example.com',
          port: 22,
          username: 'bob',
          auth_type: 'agent',
        },
      ],
    };
    apiMocks.preflightTreeNode
      .mockResolvedValueOnce({ status: 'verified' })
      .mockResolvedValueOnce({
        status: 'unknown',
        fingerprint: 'SHA256:target',
        keyType: 'ssh-ed25519',
      });

    await act(async () => {
      render(<NewConnectionModal />);
    });

    fireEvent.click(screen.getByRole('button', { name: 'modals.new_connection.connect' }));

    await waitFor(() => {
      expect(screen.getByTestId('host-key-dialog-state')).toHaveTextContent('unknown');
    });

    expect(apiMocks.preflightTreeNode).toHaveBeenNthCalledWith(1, 'node-jump');
    expect(apiMocks.preflightTreeNode).toHaveBeenNthCalledWith(2, 'node-target');
    expect(sessionTreeState.connectNode).toHaveBeenNthCalledWith(1, 'node-jump');

    fireEvent.click(screen.getByText('accept-host-key'));

    await waitFor(() => {
      expect(sessionTreeState.connectNode).toHaveBeenNthCalledWith(2, 'node-target', {
        trustHostKey: true,
        expectedHostKeyFingerprint: 'SHA256:target',
      });
    });

    expect(sessionTreeState.createTerminalForNode).toHaveBeenCalledWith('node-target', 120, 40);
  });
});