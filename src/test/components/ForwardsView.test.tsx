import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMutableSelectorStore } from '@/test/helpers/mockStore';

const apiMocks = vi.hoisted(() => ({
  nodeListForwards: vi.fn().mockResolvedValue([]),
  nodeGetForwardStats: vi.fn().mockResolvedValue(null),
  nodeCreateForward: vi.fn().mockResolvedValue({ success: true }),
  nodeDeleteForward: vi.fn().mockResolvedValue({ success: true }),
  nodeRestartForward: vi.fn().mockResolvedValue({ success: true }),
  nodeStopForward: vi.fn().mockResolvedValue({ success: true }),
  nodeForwardJupyter: vi.fn().mockResolvedValue({ success: true }),
  nodeForwardTensorboard: vi.fn().mockResolvedValue({ success: true }),
  nodeForwardVscode: vi.fn().mockResolvedValue({ success: true }),
  nodeUpdateForward: vi.fn().mockResolvedValue({ success: true }),
}));

const forwardEventsMock = vi.hoisted(() => vi.fn());

const nodeStateMock = vi.hoisted(() => ({
  value: {
    state: { readiness: 'ready' as 'ready' | 'connecting' },
    ready: true,
    generation: 1,
  },
}));

const sessionTreeState = vi.hoisted(() => ({
  getNode: vi.fn(),
  getTerminalsForNode: vi.fn(() => [] as string[]),
}));

const toastMock = vi.hoisted(() => ({ toast: vi.fn() }));
const confirmMock = vi.hoisted(() => ({ confirm: vi.fn().mockResolvedValue(true) }));

vi.mock('@/lib/api', () => ({
  api: apiMocks,
}));

vi.mock('@/hooks/useNodeState', () => ({
  useNodeState: () => nodeStateMock.value,
}));

vi.mock('@/hooks/useForwardEvents', () => ({
  useForwardEvents: forwardEventsMock,
}));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: createMutableSelectorStore(sessionTreeState),
}));

vi.mock('@/hooks/useToast', () => ({
  useToast: () => toastMock,
}));

vi.mock('@/hooks/useConfirm', () => ({
  useConfirm: () => ({
    confirm: confirmMock.confirm,
    ConfirmDialog: null,
  }),
}));

vi.mock('@/hooks/useTabBackground', () => ({
  useTabBgActive: () => false,
}));

vi.mock('@/hooks/usePortDetection', () => ({
  usePortDetection: () => ({
    newPorts: [],
    allPorts: [],
    dismissPort: vi.fn(),
  }),
}));

vi.mock('@/components/forwards/PortDetectionBanner', () => ({
  PortDetectionBanner: () => null,
}));

vi.mock('@/components/ui/tooltip', () => ({
  Tooltip: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  TooltipTrigger: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  TooltipContent: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

vi.mock('react-i18next', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react-i18next')>();
  return {
    ...actual,
    useTranslation: () => ({
      t: (key: string) => key,
    }),
  };
});

import { ForwardsView } from '@/components/forwards/ForwardsView';

function makeForward(overrides: Record<string, unknown> = {}) {
  return {
    id: 'forward-1',
    forward_type: 'local',
    bind_address: '127.0.0.1',
    bind_port: 8080,
    target_host: 'localhost',
    target_port: 3000,
    status: 'stopped',
    description: 'forward',
    ...overrides,
  };
}

describe('ForwardsView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    nodeStateMock.value = { state: { readiness: 'ready' }, ready: true, generation: 1 };
    sessionTreeState.getNode.mockReturnValue({ terminalSessionId: 'term-primary' });
    sessionTreeState.getTerminalsForNode.mockReturnValue([]);
    apiMocks.nodeListForwards.mockResolvedValue([]);
  });

  it('subscribes to forward events using the node terminal session id', async () => {
    render(<ForwardsView nodeId="node-1" />);

    await waitFor(() => {
      expect(forwardEventsMock).toHaveBeenCalled();
    });

    expect(forwardEventsMock).toHaveBeenCalledWith(
      expect.objectContaining({
        enabled: true,
        sessionId: 'term-primary',
      }),
    );
  });

  it('falls back to the first mapped terminal when the backend terminal id is missing', async () => {
    sessionTreeState.getNode.mockReturnValue({ terminalSessionId: null });
    sessionTreeState.getTerminalsForNode.mockReturnValue(['term-fallback']);

    render(<ForwardsView nodeId="node-1" />);

    await waitFor(() => {
      expect(forwardEventsMock).toHaveBeenCalled();
    });

    expect(forwardEventsMock).toHaveBeenCalledWith(
      expect.objectContaining({
        enabled: true,
        sessionId: 'term-fallback',
      }),
    );
  });

  it('does not subscribe or fetch forwards before the node is ready', () => {
    nodeStateMock.value = { state: { readiness: 'connecting' }, ready: true, generation: 1 };
    sessionTreeState.getNode.mockReturnValue({ terminalSessionId: null });

    render(<ForwardsView nodeId="node-1" />);

    expect(forwardEventsMock).toHaveBeenCalledWith(
      expect.objectContaining({
        enabled: false,
        sessionId: undefined,
      }),
    );
    expect(apiMocks.nodeListForwards).not.toHaveBeenCalled();
    expect(screen.getByText('forwards.table.no_forwards')).toBeInTheDocument();
  });

  it('blocks create submission when the bind port is invalid', async () => {
    render(<ForwardsView nodeId="node-1" />);

    await act(async () => {
      fireEvent.click(screen.getAllByRole('button', { name: 'forwards.actions.new_forward' })[0]);
    });

    const portInputs = screen.getAllByPlaceholderText('forwards.form.port_placeholder');
    await act(async () => {
      fireEvent.change(portInputs[0], { target: { value: '70000' } });
      fireEvent.change(portInputs[1], { target: { value: '3000' } });
      fireEvent.click(screen.getByRole('button', { name: 'forwards.form.create_forward' }));
    });

    expect(apiMocks.nodeCreateForward).not.toHaveBeenCalled();
    expect(screen.getByText('forwards.form.port_invalid')).toBeInTheDocument();
  });

  it('blocks edit submission when the edited bind port is invalid', async () => {
    apiMocks.nodeListForwards.mockResolvedValue([makeForward()]);

    render(<ForwardsView nodeId="node-1" />);

    await screen.findByRole('button', { name: 'forwards.actions.edit' });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'forwards.actions.edit' }));
    });

    const portInputs = screen.getAllByPlaceholderText('forwards.form.port_placeholder');
    await act(async () => {
      fireEvent.change(portInputs[0], { target: { value: '0' } });
      fireEvent.click(screen.getByRole('button', { name: 'forwards.form.save_changes' }));
    });

    expect(apiMocks.nodeUpdateForward).not.toHaveBeenCalled();
    expect(screen.getByText('forwards.form.port_invalid')).toBeInTheDocument();
  });

  it('submits parsed numeric ports when creating a valid local forward', async () => {
    render(<ForwardsView nodeId="node-1" />);

    await act(async () => {
      fireEvent.click(screen.getAllByRole('button', { name: 'forwards.actions.new_forward' })[0]);
    });

    const hostInputs = screen.getAllByPlaceholderText('forwards.form.host_placeholder');
    const portInputs = screen.getAllByPlaceholderText('forwards.form.port_placeholder');

    await act(async () => {
      fireEvent.change(hostInputs[0], { target: { value: '127.0.0.1' } });
      fireEvent.change(portInputs[0], { target: { value: '8080' } });
      fireEvent.change(hostInputs[1], { target: { value: 'service.internal' } });
      fireEvent.change(portInputs[1], { target: { value: '3000' } });
      fireEvent.click(screen.getByRole('button', { name: 'forwards.form.create_forward' }));
    });

    await waitFor(() => {
      expect(apiMocks.nodeCreateForward).toHaveBeenCalledWith(
        expect.objectContaining({
          node_id: 'node-1',
          bind_address: '127.0.0.1',
          bind_port: 8080,
          target_host: 'service.internal',
          target_port: 3000,
          check_health: true,
        }),
      );
    });
  });

  it('defaults a blank remote bind host to localhost when creating a remote forward', async () => {
    render(<ForwardsView nodeId="node-1" />);

    await act(async () => {
      fireEvent.click(screen.getAllByRole('button', { name: 'forwards.actions.new_forward' })[0]);
    });

    await act(async () => {
      fireEvent.click(screen.getByLabelText('forwards.form.type_remote'));
    });

    const portInputs = screen.getAllByPlaceholderText('forwards.form.port_placeholder');
    await act(async () => {
      fireEvent.change(portInputs[0], { target: { value: '5500' } });
      fireEvent.change(portInputs[1], { target: { value: '5500' } });
      fireEvent.click(screen.getByRole('button', { name: 'forwards.form.create_forward' }));
    });

    await waitFor(() => {
      expect(apiMocks.nodeCreateForward).toHaveBeenCalledWith(
        expect.objectContaining({
          node_id: 'node-1',
          forward_type: 'remote',
          bind_address: 'localhost',
          bind_port: 5500,
          target_host: 'localhost',
          target_port: 5500,
        }),
      );
    });
  });

  it('shows backend create failures returned in ForwardResponse', async () => {
    apiMocks.nodeCreateForward.mockResolvedValueOnce({
      success: false,
      error: 'Connection failed: The request was rejected by the other party',
    });

    render(<ForwardsView nodeId="node-1" />);

    await act(async () => {
      fireEvent.click(screen.getAllByRole('button', { name: 'forwards.actions.new_forward' })[0]);
    });

    const portInputs = screen.getAllByPlaceholderText('forwards.form.port_placeholder');
    await act(async () => {
      fireEvent.change(portInputs[0], { target: { value: '8080' } });
      fireEvent.change(portInputs[1], { target: { value: '3000' } });
      fireEvent.click(screen.getByRole('button', { name: 'forwards.form.create_forward' }));
    });

    expect(await screen.findByText('Connection failed: The request was rejected by the other party')).toBeInTheDocument();
  });

  it('submits parsed numeric ports when editing a stopped forward', async () => {
    apiMocks.nodeListForwards.mockResolvedValue([makeForward()]);

    render(<ForwardsView nodeId="node-1" />);

    await screen.findByRole('button', { name: 'forwards.actions.edit' });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'forwards.actions.edit' }));
    });

    const hostInputs = screen.getAllByPlaceholderText('forwards.form.host_placeholder');
    const portInputs = screen.getAllByPlaceholderText('forwards.form.port_placeholder');

    await act(async () => {
      fireEvent.change(hostInputs[0], { target: { value: '0.0.0.0' } });
      fireEvent.change(portInputs[0], { target: { value: '9090' } });
      fireEvent.change(hostInputs[1], { target: { value: 'new-target.internal' } });
      fireEvent.change(portInputs[1], { target: { value: '4000' } });
      fireEvent.click(screen.getByRole('button', { name: 'forwards.form.save_changes' }));
    });

    await waitFor(() => {
      expect(apiMocks.nodeUpdateForward).toHaveBeenCalledWith(
        expect.objectContaining({
          node_id: 'node-1',
          forward_id: 'forward-1',
          bind_address: '0.0.0.0',
          bind_port: 9090,
          target_host: 'new-target.internal',
          target_port: 4000,
        }),
      );
    });
  });
});
