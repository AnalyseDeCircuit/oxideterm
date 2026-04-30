import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

const terminalRegistryMocks = vi.hoisted(() => ({
  beginTerminalCommandMark: vi.fn(),
  broadcastToTargets: vi.fn(),
  subscribeTerminalOutput: vi.fn(() => vi.fn()),
}));

const completionMocks = vi.hoisted(() => ({
  getCommandBarCompletions: vi.fn(() => Promise.resolve([])),
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: (selector: (state: unknown) => unknown) => selector({
    settings: {
      terminal: {
        commandBar: {
          enabled: true,
          gitStatus: false,
          smartCompletion: true,
        },
      },
    },
  }),
}));

vi.mock('@/store/broadcastStore', () => {
  const state = {
    enabled: false,
    targets: new Set<string>(),
  };
  return {
    useBroadcastStore: Object.assign(
      (selector: (candidate: typeof state) => unknown) => selector(state),
      { getState: () => state },
    ),
  };
});

vi.mock('@/store/recordingStore', () => ({
  useRecordingStore: (selector: (state: { isRecording: () => boolean }) => unknown) => selector({
    isRecording: () => false,
  }),
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: (selector: (state: { sessions: Map<string, unknown>; tabs: unknown[] }) => unknown) => selector({
    sessions: new Map([['session-1', { username: 'user', host: 'host' }]]),
    tabs: [],
  }),
}));

vi.mock('@/lib/api', () => ({
  api: {
    localExecCommand: vi.fn(),
  },
}));

vi.mock('@/lib/terminalRegistry', () => ({
  getCwd: vi.fn(() => '/tmp'),
  getCwdHost: vi.fn(() => null),
  getTerminalBuffer: vi.fn(() => ''),
  beginTerminalCommandMark: terminalRegistryMocks.beginTerminalCommandMark,
  broadcastToTargets: terminalRegistryMocks.broadcastToTargets,
  subscribeTerminalOutput: terminalRegistryMocks.subscribeTerminalOutput,
}));

vi.mock('@/lib/terminal/completion', () => ({
  getCommandBarCompletions: completionMocks.getCommandBarCompletions,
}));

import { useTerminalCommandBarState } from '@/hooks/useTerminalCommandBarState';

describe('useTerminalCommandBarState', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    completionMocks.getCommandBarCompletions.mockResolvedValue([]);
  });

  it('submits an explicit suggestion command instead of the typed input', () => {
    const sendInput = vi.fn();
    const { result } = renderHook(() => useTerminalCommandBarState({
      paneId: 'pane-1',
      sessionId: 'session-1',
      tabId: 'tab-1',
      terminalType: 'local_terminal',
      isActive: true,
      sendInput,
    }));

    act(() => {
      result.current.setValue('ls');
    });

    act(() => {
      expect(result.current.submitCommand('ls -s')).toBe(true);
    });

    expect(sendInput).toHaveBeenCalledWith('ls -s\r');
    expect(terminalRegistryMocks.beginTerminalCommandMark).toHaveBeenCalledWith('pane-1', {
      command: 'ls -s',
      source: 'command_bar',
      sessionId: 'session-1',
      cwd: '/tmp',
    });
    expect(result.current.value).toBe('');
  });

  it('normalizes multi-line commands to terminal enter sequences on submit', () => {
    const sendInput = vi.fn();
    const { result } = renderHook(() => useTerminalCommandBarState({
      paneId: 'pane-1',
      sessionId: 'session-1',
      tabId: 'tab-1',
      terminalType: 'local_terminal',
      isActive: true,
      sendInput,
    }));

    act(() => {
      result.current.setValue("cat <<'EOF'\nhello\nEOF");
    });

    act(() => {
      expect(result.current.submitCommand()).toBe(true);
    });

    expect(sendInput).toHaveBeenCalledWith("cat <<'EOF'\rhello\rEOF\r");
  });

  it('clamps stale completion replacement ranges when accepting suggestions', () => {
    const { result } = renderHook(() => useTerminalCommandBarState({
      paneId: 'pane-1',
      sessionId: 'session-1',
      tabId: 'tab-1',
      terminalType: 'local_terminal',
      isActive: true,
      sendInput: vi.fn(),
    }));

    act(() => {
      result.current.setValue('ls');
    });

    act(() => {
      expect(result.current.acceptSuggestion({
        kind: 'option',
        label: '-l',
        insertText: '-l',
        source: 'fig',
        executable: false,
        replacement: { start: 3, end: 99 },
        score: 1,
      })).toBe(true);
    });

    expect(result.current.value).toBe('ls-l');
    expect(result.current.cursorIndex).toBe(4);
  });

  it('does not fetch completions while IME composition is active', async () => {
    const { result } = renderHook(() => useTerminalCommandBarState({
      paneId: 'pane-1',
      sessionId: 'session-1',
      tabId: 'tab-1',
      terminalType: 'local_terminal',
      isActive: true,
      sendInput: vi.fn(),
    }));

    act(() => {
      result.current.setFocused(true);
      result.current.setInputComposing(true);
      result.current.setValue('ls');
    });

    await Promise.resolve();

    expect(completionMocks.getCommandBarCompletions).not.toHaveBeenCalled();
    expect(await result.current.revealHistorySuggestions()).toBe(0);
  });
});
