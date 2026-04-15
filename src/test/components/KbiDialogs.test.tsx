import { act, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

import { GlobalKbiDialog } from '@/components/modals/GlobalKbiDialog';
import { KbiDialog } from '@/components/modals/KbiDialog';

type TauriEvent<T> = {
  payload: T;
};

type EventCallback<T = unknown> = (event: TauriEvent<T>) => void;

const eventListeners = new Map<string, EventCallback[]>();

function emitTauriEvent<T>(eventName: string, payload: T) {
  act(() => {
    for (const callback of eventListeners.get(eventName) ?? []) {
      callback({ payload });
    }
  });
}

describe('KBI dialogs', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventListeners.clear();

    vi.mocked(invoke).mockResolvedValue(undefined as never);
    vi.mocked(listen).mockImplementation(((eventName: string, callback: EventCallback) => {
      const listeners = eventListeners.get(eventName) ?? [];
      listeners.push(callback);
      eventListeners.set(eventName, listeners);

      return Promise.resolve(() => {
        const current = eventListeners.get(eventName) ?? [];
        eventListeners.set(
          eventName,
          current.filter((entry) => entry !== callback),
        );
      });
    }) as typeof listen);
  });

  it('KbiDialog ignores chained prompts and unrelated results', async () => {
    const onSuccess = vi.fn();
    const onFailure = vi.fn();

    render(<KbiDialog onSuccess={onSuccess} onFailure={onFailure} />);

    await waitFor(() => {
      expect(listen).toHaveBeenCalledTimes(2);
    });

    emitTauriEvent('ssh_kbi_prompt', {
      authFlowId: 'chained-flow',
      name: 'ignored',
      instructions: 'ignored',
      prompts: [{ prompt: 'Ignored prompt', echo: false }],
      chained: true,
    });

    expect(screen.queryByLabelText('Ignored prompt')).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();

    emitTauriEvent('ssh_kbi_prompt', {
      authFlowId: 'standalone-flow',
      name: 'OTP',
      instructions: 'Enter your code',
      prompts: [{ prompt: 'One-time code', echo: false }],
      chained: false,
    });

    expect(await screen.findByLabelText('One-time code')).toBeInTheDocument();

    emitTauriEvent('ssh_kbi_result', {
      authFlowId: 'other-flow',
      success: true,
      sessionId: 'session-2',
      wsPort: 1421,
      wsToken: 'token-2',
    });

    expect(onSuccess).not.toHaveBeenCalled();
    expect(screen.getByLabelText('One-time code')).toBeInTheDocument();
    expect(onFailure).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it('KbiDialog cancels overlapping standalone prompts', async () => {
    render(<KbiDialog onSuccess={vi.fn()} onFailure={vi.fn()} />);

    await waitFor(() => {
      expect(listen).toHaveBeenCalledTimes(2);
    });

    emitTauriEvent('ssh_kbi_prompt', {
      authFlowId: 'flow-a',
      name: 'OTP',
      instructions: 'Enter code A',
      prompts: [{ prompt: 'Prompt A', echo: false }],
      chained: false,
    });

    expect(await screen.findByLabelText('Prompt A')).toBeInTheDocument();
    vi.mocked(invoke).mockClear();

    emitTauriEvent('ssh_kbi_prompt', {
      authFlowId: 'flow-b',
      name: 'OTP',
      instructions: 'Enter code B',
      prompts: [{ prompt: 'Prompt B', echo: false }],
      chained: false,
    });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledTimes(1);
      expect(invoke).toHaveBeenCalledWith('ssh_kbi_cancel', {
        request: { authFlowId: 'flow-b' },
      });
    });

    expect(screen.getByLabelText('Prompt A')).toBeInTheDocument();
    expect(screen.queryByLabelText('Prompt B')).not.toBeInTheDocument();
  });

  it('GlobalKbiDialog handles standalone prompts and closes on matching success', async () => {
    render(<GlobalKbiDialog />);

    await waitFor(() => {
      expect(listen).toHaveBeenCalledTimes(2);
    });

    emitTauriEvent('ssh_kbi_prompt', {
      authFlowId: 'standalone-a',
      name: 'OTP',
      instructions: 'Enter standalone code A',
      prompts: [{ prompt: 'Standalone prompt A', echo: false }],
      chained: false,
    });

    expect(await screen.findByLabelText('Standalone prompt A')).toBeInTheDocument();

    emitTauriEvent('ssh_kbi_result', {
      authFlowId: 'standalone-a',
      success: true,
    });

    await waitFor(() => {
      expect(screen.queryByLabelText('Standalone prompt A')).not.toBeInTheDocument();
    });
  });
});