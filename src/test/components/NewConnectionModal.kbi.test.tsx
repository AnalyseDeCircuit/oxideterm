import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMutableSelectorStore } from '@/test/helpers/mockStore';

const apiMocks = vi.hoisted(() => ({
  getGroups: vi.fn().mockResolvedValue([]),
  getConnections: vi.fn().mockResolvedValue([]),
  isAgentAvailable: vi.fn().mockResolvedValue(true),
  saveConnection: vi.fn(),
  sshPreflight: vi.fn(),
  // These modal tests exercise direct connects unless a case explicitly supplies a proxy chain.
  resolveUpstreamProxyForConnect: vi.fn().mockResolvedValue(null),
  serialListPorts: vi.fn(),
  saveSerialProfile: vi.fn(),
}));

const appStoreState = vi.hoisted(() => ({
  modals: { newConnection: true },
  toggleModal: vi.fn(),
  createTab: vi.fn(),
  quickConnectData: null as null | { host: string; port: number; username: string },
}));

const sessionTreeState = vi.hoisted(() => ({
  addRootNode: vi.fn(),
  connectNode: vi.fn(),
  createTerminalForNode: vi.fn(),
}));

const localTerminalState = vi.hoisted(() => ({
  createSerialTerminal: vi.fn(),
}));

const toastState = vi.hoisted(() => ({
  error: vi.fn(),
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
vi.mock('@/store/localTerminalStore', () => ({
  useLocalTerminalStore: createMutableSelectorStore(localTerminalState),
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
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

import { NewConnectionModal } from '@/components/modals/NewConnectionModal';

describe('NewConnectionModal terminal creation flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    appStoreState.modals.newConnection = true;
    appStoreState.quickConnectData = null;
    apiMocks.getGroups.mockResolvedValue([]);
    apiMocks.getConnections.mockResolvedValue([]);
    apiMocks.isAgentAvailable.mockResolvedValue(true);
    apiMocks.sshPreflight.mockResolvedValue({ status: 'verified' });
    apiMocks.resolveUpstreamProxyForConnect.mockResolvedValue(null);
    apiMocks.serialListPorts.mockResolvedValue([]);
    apiMocks.saveSerialProfile.mockResolvedValue({ id: 'profile-1' });
    sessionTreeState.addRootNode.mockResolvedValue('node-kbi');
    sessionTreeState.connectNode.mockResolvedValue(undefined);
    sessionTreeState.createTerminalForNode.mockResolvedValue('term-kbi');
    localTerminalState.createSerialTerminal.mockResolvedValue({
      id: 'serial-1',
      shell: { id: 'serial', label: 'Serial /dev/ttyUSB0', path: '/dev/ttyUSB0', args: [] },
      cols: 120,
      rows: 40,
      running: true,
      detached: false,
      transport: { type: 'serial', portPath: '/dev/ttyUSB0', baudRate: 115200 },
    });
  });

  it('routes keyboard-interactive connects through SessionTree and creates a terminal', async () => {
    await act(async () => {
      render(<NewConnectionModal />);
    });

    fireEvent.change(screen.getByLabelText('modals.new_connection.target_host *'), {
      target: { value: 'server.example.com' },
    });
    fireEvent.change(screen.getByLabelText('modals.new_connection.target_username *'), {
      target: { value: 'alice' },
    });

    const twoFaTab = screen.getByRole('tab', { name: 'modals.new_connection.auth_2fa' });
    fireEvent.mouseDown(twoFaTab);
    fireEvent.click(twoFaTab);
    await waitFor(() => {
      expect(screen.getByText('modals.new_connection.twofa_desc')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('checkbox', { name: 'modals.new_connection.agent_forwarding' }));
    fireEvent.click(screen.getByRole('button', { name: 'modals.new_connection.connect' }));

    await waitFor(() => {
      expect(sessionTreeState.addRootNode).toHaveBeenCalledWith(expect.objectContaining({
        host: 'server.example.com',
        username: 'alice',
        authType: 'keyboard_interactive',
        agentForwarding: true,
      }));
    });
    await waitFor(() => {
      expect(sessionTreeState.connectNode).toHaveBeenCalledWith('node-kbi', { upstreamProxy: undefined });
      expect(sessionTreeState.createTerminalForNode).toHaveBeenCalledWith('node-kbi', 120, 40);
      expect(appStoreState.createTab).toHaveBeenCalledWith('terminal', 'term-kbi');
      expect(appStoreState.toggleModal).toHaveBeenCalledWith('newConnection', false);
    });
  });

  it('creates a terminal for direct password connections too', async () => {
    sessionTreeState.addRootNode.mockResolvedValue('node-password');
    sessionTreeState.createTerminalForNode.mockResolvedValue('term-password');

    await act(async () => {
      render(<NewConnectionModal />);
    });

    fireEvent.change(screen.getByLabelText('modals.new_connection.target_host *'), {
      target: { value: 'password.example.com' },
    });
    fireEvent.change(screen.getByLabelText('modals.new_connection.target_username *'), {
      target: { value: 'bob' },
    });
    fireEvent.change(screen.getByLabelText('modals.new_connection.password'), {
      target: { value: 'secret' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'modals.new_connection.connect' }));

    await waitFor(() => {
      expect(sessionTreeState.addRootNode).toHaveBeenCalledWith(expect.objectContaining({
        host: 'password.example.com',
        username: 'bob',
        authType: 'password',
        password: 'secret',
      }));
      expect(sessionTreeState.connectNode).toHaveBeenCalledWith('node-password', { upstreamProxy: undefined });
      expect(sessionTreeState.createTerminalForNode).toHaveBeenCalledWith('node-password', 120, 40);
      expect(appStoreState.createTab).toHaveBeenCalledWith('terminal', 'term-password');
    });
  });

  it('opens serial terminals from the transport selector', async () => {
    await act(async () => {
      render(<NewConnectionModal />);
    });

    const serialTab = screen.getByRole('tab', { name: 'modals.new_connection.transport_serial' });
    fireEvent.mouseDown(serialTab);
    fireEvent.click(serialTab);

    fireEvent.change(screen.getByLabelText('modals.new_connection.serial_port *'), {
      target: { value: '/dev/ttyUSB0' },
    });
    fireEvent.click(screen.getByRole('checkbox', { name: 'modals.new_connection.save_serial_profile' }));
    fireEvent.change(screen.getByLabelText('modals.new_connection.serial_profile_name'), {
      target: { value: 'Lab console' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'modals.new_connection.serial_open' }));

    await waitFor(() => {
      expect(localTerminalState.createSerialTerminal).toHaveBeenCalledWith(expect.objectContaining({
        portPath: '/dev/ttyUSB0',
        baudRate: 115200,
        dataBits: 8,
        stopBits: 1,
        parity: 'none',
        flowControl: 'none',
        cols: 120,
        rows: 40,
      }));
      expect(apiMocks.saveSerialProfile).toHaveBeenCalledWith(expect.objectContaining({
        name: 'Lab console',
        portPath: '/dev/ttyUSB0',
        baudRate: 115200,
      }));
      expect(appStoreState.createTab).toHaveBeenCalledWith('local_terminal', 'serial-1');
      expect(appStoreState.toggleModal).toHaveBeenCalledWith('newConnection', false);
    });
  });

  it('opens the serial branch from external palette requests', async () => {
    await act(async () => {
      render(<NewConnectionModal />);
    });

    window.dispatchEvent(new CustomEvent('oxideterm:new-connection-transport', {
      detail: { transport: 'serial' },
    }));

    expect(await screen.findByText('modals.new_connection.serial_section_title')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'modals.new_connection.serial_open' })).toBeDisabled();
  });

  it('keeps SSH and Serial draft state when switching transport branches', async () => {
    await act(async () => {
      render(<NewConnectionModal />);
    });

    fireEvent.change(screen.getByLabelText('modals.new_connection.target_host *'), {
      target: { value: 'ssh.example.com' },
    });

    const serialTab = screen.getByRole('tab', { name: 'modals.new_connection.transport_serial' });
    fireEvent.mouseDown(serialTab);
    fireEvent.click(serialTab);
    fireEvent.change(screen.getByLabelText('modals.new_connection.serial_port *'), {
      target: { value: '/dev/ttyUSB1' },
    });

    const sshTab = screen.getByRole('tab', { name: 'modals.new_connection.transport_ssh' });
    fireEvent.mouseDown(sshTab);
    fireEvent.click(sshTab);
    expect(screen.getByLabelText('modals.new_connection.target_host *')).toHaveValue('ssh.example.com');

    fireEvent.mouseDown(serialTab);
    fireEvent.click(serialTab);
    expect(screen.getByLabelText('modals.new_connection.serial_port *')).toHaveValue('/dev/ttyUSB1');
  });

  it('shows inline validation and disables connect for invalid serial baud rates', async () => {
    await act(async () => {
      render(<NewConnectionModal />);
    });

    const serialTab = screen.getByRole('tab', { name: 'modals.new_connection.transport_serial' });
    fireEvent.mouseDown(serialTab);
    fireEvent.click(serialTab);
    fireEvent.change(screen.getByLabelText('modals.new_connection.serial_port *'), {
      target: { value: '/dev/ttyUSB0' },
    });
    fireEvent.change(screen.getByLabelText('modals.new_connection.serial_baud_rate'), {
      target: { value: '0' },
    });

    expect(screen.getByRole('alert')).toHaveTextContent('modals.new_connection.serial_invalid_baud_rate');
    expect(screen.getByRole('button', { name: 'modals.new_connection.serial_open' })).toBeDisabled();
  });
});
