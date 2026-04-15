import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMutableSelectorStore } from '@/test/helpers/mockStore';

const apiMocks = vi.hoisted(() => ({
  getGroups: vi.fn().mockResolvedValue([]),
  isAgentAvailable: vi.fn().mockResolvedValue(true),
  saveConnection: vi.fn(),
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
    apiMocks.isAgentAvailable.mockResolvedValue(true);
    sessionTreeState.addRootNode.mockResolvedValue('node-kbi');
    sessionTreeState.connectNode.mockResolvedValue(undefined);
    sessionTreeState.createTerminalForNode.mockResolvedValue('term-kbi');
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
      expect(sessionTreeState.connectNode).toHaveBeenCalledWith('node-kbi');
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
      expect(sessionTreeState.connectNode).toHaveBeenCalledWith('node-password');
      expect(sessionTreeState.createTerminalForNode).toHaveBeenCalledWith('node-password', 120, 40);
      expect(appStoreState.createTab).toHaveBeenCalledWith('terminal', 'term-password');
    });
  });
});
