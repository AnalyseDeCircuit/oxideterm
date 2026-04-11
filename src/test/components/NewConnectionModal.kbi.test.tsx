import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
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
  quickConnectData: null as null | { host: string; port: number; username: string },
}));

const sessionTreeState = vi.hoisted(() => ({
  addRootNode: vi.fn(),
  connectNode: vi.fn(),
  addKbiSession: vi.fn().mockResolvedValue(undefined),
}));

const toastState = vi.hoisted(() => ({
  error: vi.fn(),
}));

const settingsStoreMock = vi.hoisted(() => ({
  getState: vi.fn(() => ({
    settings: {
      buffer: {
        maxLines: 7000,
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

describe('NewConnectionModal KBI flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    appStoreState.modals.newConnection = true;
    appStoreState.quickConnectData = null;
    apiMocks.getGroups.mockResolvedValue([]);
    apiMocks.isAgentAvailable.mockResolvedValue(true);
    vi.mocked(invoke).mockResolvedValue(undefined as never);
  });

  it('passes agentForwarding to ssh_connect_kbi', async () => {
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
      expect(invoke).toHaveBeenCalledWith('ssh_connect_kbi', expect.objectContaining({
        host: 'server.example.com',
        username: 'alice',
        agentForwarding: true,
        maxBufferLines: 7000,
      }));
    });
  });

  it('mounts the real standalone KbiDialog listeners before connect', async () => {
    await act(async () => {
      render(<NewConnectionModal />);
    });

    await waitFor(() => {
      expect(listen).toHaveBeenCalledWith('ssh_kbi_prompt', expect.any(Function));
      expect(listen).toHaveBeenCalledWith('ssh_kbi_result', expect.any(Function));
    });
  });
});
