import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const commandBarStateMock = vi.hoisted(() => ({
  value: 'ls',
  submitCommand: vi.fn(),
  setValue: vi.fn(),
  setFocused: vi.fn(),
  setInputComposing: vi.fn(),
  acceptSuggestion: vi.fn(),
  revealHistorySuggestions: vi.fn(),
  suggestions: [] as unknown[],
}));

const quickCommandsMock = vi.hoisted(() => ({
  categories: [{ id: 'system', name: 'System', icon: 'server' }] as Array<{ id: string; name: string; icon: string }>,
  commands: [] as Array<{ id: string; name: string; command: string; category: string; description?: string; createdAt: number; updatedAt: number }>,
  upsertCommand: vi.fn(),
  deleteCommand: vi.fn(),
  upsertCategory: vi.fn(),
  deleteCategory: vi.fn(),
  hydrate: vi.fn(),
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock('lucide-react', () => ({
  ChevronRight: () => null,
  ChevronDown: () => null,
  ChevronUp: () => null,
  Check: () => null,
  Container: () => null,
  FilePlay: () => null,
  Folder: () => null,
  GitBranch: () => null,
  Monitor: () => null,
  Pencil: () => null,
  Play: () => null,
  Plus: () => null,
  Radio: () => null,
  Save: () => null,
  Search: () => null,
  Server: () => null,
  SplitSquareHorizontal: () => null,
  SplitSquareVertical: () => null,
  Square: () => null,
  Trash2: () => null,
  Circle: () => null,
  X: () => null,
  Zap: () => null,
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readTextFile: vi.fn(),
}));

vi.mock('@/hooks/useTerminalCommandBarState', () => ({
  useTerminalCommandBarState: () => ({
    value: commandBarStateMock.value,
    setValue: commandBarStateMock.setValue,
    cursorIndex: commandBarStateMock.value.length,
    setCursorIndex: vi.fn(),
    focused: true,
    setFocused: commandBarStateMock.setFocused,
    inputComposing: false,
    setInputComposing: commandBarStateMock.setInputComposing,
    ghostText: '',
    suggestions: commandBarStateMock.suggestions,
    revealHistorySuggestions: commandBarStateMock.revealHistorySuggestions,
    acceptSuggestion: commandBarStateMock.acceptSuggestion,
    submitCommand: commandBarStateMock.submitCommand,
    cwd: '/tmp',
    targetLabel: 'local',
    chips: {
      broadcastEnabled: false,
      broadcastTargetCount: 0,
      isRecording: false,
      gitBranch: null,
    },
  }),
}));

vi.mock('@/hooks/useConfirm', () => ({
  useConfirm: () => ({
    confirm: vi.fn(() => Promise.resolve(true)),
    ConfirmDialog: null,
  }),
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: (selector: (state: unknown) => unknown) => selector({
    settings: {
      terminal: {
        commandBar: {
          quickCommandsEnabled: true,
          quickCommandsConfirmBeforeRun: false,
          quickCommandsShowToast: false,
          focusHandoffCommands: ['vim'],
        },
      },
    },
  }),
}));

vi.mock('@/store/quickCommandsStore', () => ({
  DEFAULT_QUICK_COMMAND_CATEGORIES: [
    { id: 'system', name: 'System', icon: 'server' },
    { id: 'network', name: 'Network', icon: 'terminal' },
    { id: 'files', name: 'Files', icon: 'folder' },
    { id: 'docker', name: 'Docker', icon: 'docker' },
    { id: 'custom', name: 'Custom', icon: 'zap' },
  ],
  matchQuickCommandHostPattern: vi.fn(() => true),
  useQuickCommandsStore: (selector: (state: unknown) => unknown) => selector({
    categories: quickCommandsMock.categories,
    commands: quickCommandsMock.commands,
    upsertCommand: quickCommandsMock.upsertCommand,
    deleteCommand: quickCommandsMock.deleteCommand,
    upsertCategory: quickCommandsMock.upsertCategory,
    deleteCategory: quickCommandsMock.deleteCategory,
    hydrate: quickCommandsMock.hydrate,
  }),
}));

vi.mock('@/hooks/useToast', () => ({
  useToastStore: {
    getState: () => ({ addToast: vi.fn() }),
  },
}));

vi.mock('@/components/layout/TabBarTerminalActions', () => ({
  BroadcastDropdown: () => null,
}));

vi.mock('@/lib/terminalRegistry', () => ({
  getAllEntries: vi.fn(() => []),
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: () => ({
    sessions: new Map(),
    tabs: [{ id: 'tab-1' }],
    splitPane: vi.fn(),
    getPaneCount: vi.fn(() => 1),
  }),
}));

vi.mock('@/store/broadcastStore', () => ({
  useBroadcastStore: (selector: (state: unknown) => unknown) => selector({
    enabled: false,
    targets: new Set<string>(),
    toggleTarget: vi.fn(),
    disable: vi.fn(),
  }),
}));

vi.mock('@/store/localTerminalStore', () => ({
  useLocalTerminalStore: (selector: (state: unknown) => unknown) => selector({
    createTerminal: vi.fn(),
  }),
}));

vi.mock('@/store/recordingStore', () => ({
  useRecordingStore: (selector: (state: unknown) => unknown) => selector({
    openPlayer: vi.fn(),
    stopRecording: vi.fn(),
    discardRecording: vi.fn(),
    isRecording: vi.fn(() => false),
  }),
}));

import { TerminalCommandBar } from '@/components/terminal/TerminalCommandBar';

describe('TerminalCommandBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    commandBarStateMock.value = 'ls';
    commandBarStateMock.suggestions = [
      {
        kind: 'history',
        label: 'ls -l',
        insertText: 'ls -l',
        source: 'history',
        executable: true,
        replacement: { start: 0, end: 2 },
        score: 2,
      },
      {
        kind: 'history',
        label: 'ls -s',
        insertText: 'ls -s',
        source: 'history',
        executable: true,
        replacement: { start: 0, end: 2 },
        score: 1,
      },
    ];
    commandBarStateMock.revealHistorySuggestions.mockResolvedValue(0);
    commandBarStateMock.submitCommand.mockReturnValue(true);
    quickCommandsMock.categories = [{ id: 'system', name: 'System', icon: 'server' }];
    quickCommandsMock.commands = [];
    quickCommandsMock.upsertCategory.mockReturnValue({ id: 'new-group', name: 'Ops', icon: 'zap' });
    quickCommandsMock.deleteCategory.mockReturnValue(true);
    quickCommandsMock.hydrate.mockResolvedValue(undefined);
  });

  it('keeps the popup closed while typing until the user explicitly opens suggestions', () => {
    render(
      <TerminalCommandBar
        paneId="pane-1"
        sessionId="session-1"
        tabId="tab-1"
        terminalType="local_terminal"
        isActive
        sendInput={vi.fn()}
        focusTerminal={vi.fn()}
      />,
    );

    expect(screen.queryByText('ls -l')).not.toBeInTheDocument();

    const input = screen.getByPlaceholderText('terminal.command_bar.command_placeholder');
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(commandBarStateMock.submitCommand).toHaveBeenCalledWith(undefined);
  });

  it('keeps Shift+Enter as a manual newline gesture instead of submitting', () => {
    render(
      <TerminalCommandBar
        paneId="pane-1"
        sessionId="session-1"
        tabId="tab-1"
        terminalType="local_terminal"
        isActive
        sendInput={vi.fn()}
        focusTerminal={vi.fn()}
      />,
    );

    const input = screen.getByPlaceholderText('terminal.command_bar.command_placeholder');
    fireEvent.keyDown(input, { key: 'Enter', shiftKey: true });

    expect(commandBarStateMock.submitCommand).not.toHaveBeenCalled();
  });

  it('keeps focus in the Command Bar after submitting a normal command', () => {
    const focusTerminal = vi.fn();
    render(
      <TerminalCommandBar
        paneId="pane-1"
        sessionId="session-1"
        tabId="tab-1"
        terminalType="local_terminal"
        isActive
        sendInput={vi.fn()}
        focusTerminal={focusTerminal}
      />,
    );

    const input = screen.getByPlaceholderText('terminal.command_bar.command_placeholder');
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(commandBarStateMock.submitCommand).toHaveBeenCalledWith(undefined);
    expect(focusTerminal).not.toHaveBeenCalled();
  });

  it('returns focus to the terminal after submitting a TUI command', () => {
    commandBarStateMock.value = 'vim';
    const focusTerminal = vi.fn();
    render(
      <TerminalCommandBar
        paneId="pane-1"
        sessionId="session-1"
        tabId="tab-1"
        terminalType="local_terminal"
        isActive
        sendInput={vi.fn()}
        focusTerminal={focusTerminal}
      />,
    );

    const input = screen.getByPlaceholderText('terminal.command_bar.command_placeholder');
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(commandBarStateMock.submitCommand).toHaveBeenCalledWith(undefined);
    expect(focusTerminal).toHaveBeenCalled();
  });

  it('submits the highlighted suggestion when Enter is pressed with suggestions open', () => {
    render(
      <TerminalCommandBar
        paneId="pane-1"
        sessionId="session-1"
        tabId="tab-1"
        terminalType="local_terminal"
        isActive
        sendInput={vi.fn()}
        focusTerminal={vi.fn()}
      />,
    );

    const input = screen.getByPlaceholderText('terminal.command_bar.command_placeholder');
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(commandBarStateMock.submitCommand).toHaveBeenCalledWith('ls -s');
  });

  it('accepts non-executable completions on Enter without submitting', async () => {
    commandBarStateMock.suggestions = [{
      kind: 'option',
      label: '-l',
      insertText: '-l',
      source: 'fig',
      executable: false,
      replacement: { start: 3, end: 3 },
      score: 1,
    }];
    commandBarStateMock.submitCommand.mockClear();
    vi.mocked(commandBarStateMock.acceptSuggestion).mockReturnValue(true);
    render(
      <TerminalCommandBar
        paneId="pane-1"
        sessionId="session-1"
        tabId="tab-1"
        terminalType="local_terminal"
        isActive
        sendInput={vi.fn()}
        focusTerminal={vi.fn()}
      />,
    );

    const input = screen.getByPlaceholderText('terminal.command_bar.command_placeholder');
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(commandBarStateMock.acceptSuggestion).toHaveBeenCalledWith(commandBarStateMock.suggestions[0]);
    expect(commandBarStateMock.submitCommand).not.toHaveBeenCalled();
  });

  it('uses ArrowUp on an empty suggestion list to explicitly recall history', async () => {
    commandBarStateMock.suggestions = [];
    commandBarStateMock.revealHistorySuggestions.mockResolvedValue(2);
    render(
      <TerminalCommandBar
        paneId="pane-1"
        sessionId="session-1"
        tabId="tab-1"
        terminalType="local_terminal"
        isActive
        sendInput={vi.fn()}
        focusTerminal={vi.fn()}
      />,
    );

    const input = screen.getByPlaceholderText('terminal.command_bar.command_placeholder');
    fireEvent.keyDown(input, { key: 'ArrowUp' });

    await waitFor(() => expect(commandBarStateMock.revealHistorySuggestions).toHaveBeenCalled());
  });

  it('silences suggestions while IME composition is active and resumes after commit', async () => {
    render(
      <TerminalCommandBar
        paneId="pane-1"
        sessionId="session-1"
        tabId="tab-1"
        terminalType="local_terminal"
        isActive
        sendInput={vi.fn()}
        focusTerminal={vi.fn()}
      />,
    );

    const input = screen.getByPlaceholderText('terminal.command_bar.command_placeholder');
    fireEvent.compositionStart(input);
    fireEvent.change(input, { target: { value: 'ls', selectionStart: 2 } });

    expect(commandBarStateMock.setInputComposing).toHaveBeenCalledWith(true);
    expect(commandBarStateMock.submitCommand).not.toHaveBeenCalled();

    fireEvent.compositionEnd(input, { data: 'ls' });

    expect(commandBarStateMock.setValue).toHaveBeenCalledWith('ls');
    await waitFor(() => expect(commandBarStateMock.setInputComposing).toHaveBeenCalledWith(false));
  });

  it('inserts a quick command from the Command Bar popover without executing it', () => {
    quickCommandsMock.commands = [{
      id: 'qc-test',
      name: 'List Files',
      command: 'ls -la',
      category: 'system',
      description: 'List files',
      createdAt: 0,
      updatedAt: 0,
    }];

    render(
      <TerminalCommandBar
        paneId="pane-1"
        sessionId="session-1"
        tabId="tab-1"
        terminalType="local_terminal"
        isActive
        sendInput={vi.fn()}
        focusTerminal={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByTitle('terminal.quick_commands.open'));
    fireEvent.click(screen.getByText('List Files'));

    expect(commandBarStateMock.setValue).toHaveBeenCalledWith('ls -la');
    expect(commandBarStateMock.submitCommand).not.toHaveBeenCalledWith('ls -la');
  });

  it('closes the quick command popover when clicking outside the Command Bar', async () => {
    quickCommandsMock.commands = [{
      id: 'qc-test',
      name: 'List Files',
      command: 'ls -la',
      category: 'system',
      description: 'List files',
      createdAt: 0,
      updatedAt: 0,
    }];

    render(
      <TerminalCommandBar
        paneId="pane-1"
        sessionId="session-1"
        tabId="tab-1"
        terminalType="local_terminal"
        isActive
        sendInput={vi.fn()}
        focusTerminal={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByTitle('terminal.quick_commands.open'));
    expect(screen.getByText('List Files')).toBeInTheDocument();

    fireEvent.pointerDown(document.body);

    await waitFor(() => expect(screen.queryByText('List Files')).not.toBeInTheDocument());
  });

  it('can create a custom quick command group from the popover', () => {
    render(
      <TerminalCommandBar
        paneId="pane-1"
        sessionId="session-1"
        tabId="tab-1"
        terminalType="local_terminal"
        isActive
        sendInput={vi.fn()}
        focusTerminal={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByTitle('terminal.quick_commands.open'));
    fireEvent.click(screen.getByTitle('terminal.quick_commands.add_group'));
    fireEvent.change(screen.getByPlaceholderText('terminal.quick_commands.group_name_placeholder'), {
      target: { value: 'Ops' },
    });
    fireEvent.click(screen.getByText('terminal.quick_commands.save_group'));

    expect(quickCommandsMock.upsertCategory).toHaveBeenCalledWith({
      id: undefined,
      name: 'Ops',
      icon: 'zap',
    });
  });
});
