import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

import {
  DEFAULT_QUICK_COMMAND_CATEGORIES,
  DEFAULT_QUICK_COMMANDS,
  applyImportedQuickCommandsSnapshot,
  matchQuickCommandHostPattern,
  useQuickCommandsStore,
} from '@/store/quickCommandsStore';

describe('quickCommandsStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invokeMock.mockResolvedValue(null);
    useQuickCommandsStore.setState({
      categories: DEFAULT_QUICK_COMMAND_CATEGORIES,
      commands: DEFAULT_QUICK_COMMANDS,
      hydrated: false,
      loading: false,
      lastPersistError: null,
    });
  });

  it('ships read-only starter commands and enum icons', () => {
    const state = useQuickCommandsStore.getState();

    expect(state.categories.map((category) => category.icon)).toEqual([
      'server',
      'terminal',
      'folder',
      'docker',
      'zap',
    ]);
    expect(state.commands.some((command) => command.command === 'ls -la')).toBe(true);
    expect(state.commands.some((command) => /rm\s+-|systemctl\s+restart/.test(command.command))).toBe(false);
  });

  it('upserts and deletes persisted commands', () => {
    const created = useQuickCommandsStore.getState().upsertCommand({
      name: 'Status',
      command: 'git status',
      category: 'files',
      description: 'Repo status',
    });

    expect(useQuickCommandsStore.getState().commands.find((command) => command.id === created.id)).toMatchObject({
      name: 'Status',
      command: 'git status',
    });

    useQuickCommandsStore.getState().deleteCommand(created.id);

    expect(useQuickCommandsStore.getState().commands.find((command) => command.id === created.id)).toBeUndefined();
    expect(invokeMock).toHaveBeenCalledWith('save_quick_commands', expect.any(Object));
  });

  it('hydrates commands from the JSON storage command', async () => {
    invokeMock.mockResolvedValueOnce({
      version: 1,
      categories: [{ id: 'ops', name: 'Ops', icon: 'zap' }],
      commands: [{
        id: 'qc-ops',
        name: 'Ops Status',
        command: 'uptime',
        category: 'ops',
        createdAt: 1,
        updatedAt: 1,
      }],
      updatedAt: 1,
    });

    await useQuickCommandsStore.getState().hydrate();

    expect(useQuickCommandsStore.getState().categories).toEqual([{ id: 'ops', name: 'Ops', icon: 'zap' }]);
    expect(useQuickCommandsStore.getState().commands[0]).toMatchObject({ name: 'Ops Status', command: 'uptime' });
  });

  it('creates, renames, and safely deletes custom categories', () => {
    const created = useQuickCommandsStore.getState().upsertCategory({
      name: 'Production',
      icon: 'server',
    });

    expect(useQuickCommandsStore.getState().categories.find((category) => category.id === created.id)).toMatchObject({
      name: 'Production',
      icon: 'server',
    });

    useQuickCommandsStore.getState().upsertCategory({
      id: created.id,
      name: 'Prod',
      icon: 'zap',
    });

    expect(useQuickCommandsStore.getState().categories.find((category) => category.id === created.id)).toMatchObject({
      name: 'Prod',
      icon: 'zap',
    });

    expect(useQuickCommandsStore.getState().deleteCategory('system')).toBe(false);
    expect(useQuickCommandsStore.getState().deleteCategory(created.id)).toBe(true);
    expect(useQuickCommandsStore.getState().categories.find((category) => category.id === created.id)).toBeUndefined();
  });

  it('does not delete a category that still owns commands', () => {
    const category = useQuickCommandsStore.getState().upsertCategory({
      name: 'Production',
      icon: 'server',
    });
    useQuickCommandsStore.getState().upsertCommand({
      name: 'Status',
      command: 'git status',
      category: category.id,
    });

    expect(useQuickCommandsStore.getState().deleteCategory(category.id)).toBe(false);
    expect(useQuickCommandsStore.getState().categories.find((candidate) => candidate.id === category.id)).toBeTruthy();
  });

  it('matches host patterns against target display fields using wildcard semantics', () => {
    expect(matchQuickCommandHostPattern('*.prod', ['api.prod'])).toBe(true);
    expect(matchQuickCommandHostPattern('root@*', ['root@192.168.1.10'])).toBe(true);
    expect(matchQuickCommandHostPattern('*.prod', ['dev.local'])).toBe(false);
    expect(matchQuickCommandHostPattern(undefined, ['dev.local'])).toBe(true);
  });

  it('imports Quick Commands with rename conflict handling', () => {
    const result = applyImportedQuickCommandsSnapshot(JSON.stringify({
      version: 1,
      categories: [{ id: 'files', name: 'Files', icon: 'folder' }],
      commands: [{
        id: 'qc-ls-la',
        name: 'List Files',
        command: 'ls -lah',
        category: 'files',
        createdAt: 1,
        updatedAt: 1,
      }],
      updatedAt: 1,
    }), 'rename');

    expect(result.errors).toEqual([]);
    expect(result.imported).toBeGreaterThan(0);
    expect(useQuickCommandsStore.getState().commands.some((command) => (
      command.name.startsWith('List Files (Imported)')
      && command.command === 'ls -lah'
    ))).toBe(true);
  });
});
