// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { invoke } from '@tauri-apps/api/core';
import { create } from 'zustand';

export const QUICK_COMMANDS_SCHEMA_VERSION = 1;

const MAX_CATEGORIES = 100;
const MAX_COMMANDS = 1000;
const MAX_ID_LEN = 128;
const MAX_NAME_LEN = 160;
const MAX_COMMAND_LEN = 4096;
const MAX_DESCRIPTION_LEN = 1024;
const MAX_HOST_PATTERN_LEN = 256;

export type QuickCommandIcon = 'terminal' | 'server' | 'folder' | 'docker' | 'zap';
export type QuickCommandImportStrategy = 'rename' | 'skip' | 'replace' | 'merge';

export interface QuickCommandCategory {
  id: string;
  name: string;
  icon: QuickCommandIcon;
}

export interface QuickCommand {
  id: string;
  name: string;
  command: string;
  category: string;
  description?: string;
  hostPattern?: string;
  createdAt: number;
  updatedAt: number;
}

export type QuickCommandsSnapshot = {
  version: number;
  categories: QuickCommandCategory[];
  commands: QuickCommand[];
  updatedAt: number;
};

export type QuickCommandsImportResult = {
  imported: number;
  skipped: number;
  errors: string[];
};

interface QuickCommandsState {
  categories: QuickCommandCategory[];
  commands: QuickCommand[];
  hydrated: boolean;
  loading: boolean;
  lastPersistError: string | null;
  hydrate: () => Promise<void>;
  upsertCommand: (command: QuickCommandDraft) => QuickCommand;
  deleteCommand: (id: string) => void;
  upsertCategory: (category: QuickCommandCategoryDraft) => QuickCommandCategory;
  deleteCategory: (id: string) => boolean;
  resetDefaults: () => void;
  applySnapshot: (snapshot: QuickCommandsSnapshot, strategy: QuickCommandImportStrategy) => QuickCommandsImportResult;
}

export type QuickCommandDraft = Omit<QuickCommand, 'id' | 'createdAt' | 'updatedAt'> & {
  id?: string;
};

export type QuickCommandCategoryDraft = Omit<QuickCommandCategory, 'id'> & {
  id?: string;
};

export const DEFAULT_QUICK_COMMAND_CATEGORIES: QuickCommandCategory[] = [
  { id: 'system', name: 'System', icon: 'server' },
  { id: 'network', name: 'Network', icon: 'terminal' },
  { id: 'files', name: 'Files', icon: 'folder' },
  { id: 'docker', name: 'Docker', icon: 'docker' },
  { id: 'custom', name: 'Custom', icon: 'zap' },
];

export const DEFAULT_QUICK_COMMANDS: QuickCommand[] = [
  commandSeed('qc-pwd', 'Print Working Directory', 'pwd', 'files', 'Show the current directory.'),
  commandSeed('qc-ls-la', 'List Files', 'ls -la', 'files', 'List files with details.'),
  commandSeed('qc-df-h', 'Disk Usage', 'df -h', 'system', 'Show mounted filesystem usage.'),
  commandSeed('qc-free-h', 'Memory Usage', 'free -h', 'system', 'Show memory usage.'),
  commandSeed('qc-uptime', 'Uptime', 'uptime', 'system', 'Show uptime and load average.'),
  commandSeed('qc-whoami', 'Current User', 'whoami', 'system', 'Show the current user.'),
  commandSeed('qc-ip-addr', 'IP Addresses', 'ip addr', 'network', 'Show network interface addresses.'),
  commandSeed('qc-ifconfig', 'Interface Config', 'ifconfig', 'network', 'Show network interfaces on systems without iproute2.'),
  commandSeed('qc-docker-ps', 'Docker Containers', 'docker ps', 'docker', 'List running containers.'),
  commandSeed('qc-git-status', 'Git Status', 'git status', 'files', 'Show repository status.'),
  commandSeed('qc-journal-errors', 'Recent Journal Errors', 'journalctl -xe --no-pager', 'system', 'Show recent system journal errors.'),
];

function commandSeed(
  id: string,
  name: string,
  command: string,
  category: string,
  description: string,
): QuickCommand {
  return {
    id,
    name,
    command,
    category,
    description,
    createdAt: 0,
    updatedAt: 0,
  };
}

function newId(): string {
  return `qc-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function newCategoryId(): string {
  return `qcg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function snapshotFromState(categories: QuickCommandCategory[], commands: QuickCommand[]): QuickCommandsSnapshot {
  return {
    version: QUICK_COMMANDS_SCHEMA_VERSION,
    categories,
    commands,
    updatedAt: Date.now(),
  };
}

async function persistSnapshot(categories: QuickCommandCategory[], commands: QuickCommand[]): Promise<void> {
  await invoke('save_quick_commands', {
    snapshot: snapshotFromState(categories, commands),
  });
}

export function getQuickCommandsSnapshot(): QuickCommandsSnapshot {
  const { categories, commands } = useQuickCommandsStore.getState();
  return snapshotFromState(categories, commands);
}

export function exportQuickCommandsSnapshot(): string | null {
  try {
    return JSON.stringify(getQuickCommandsSnapshot());
  } catch (error) {
    console.error('[QuickCommands] Failed to serialize snapshot:', error);
    return null;
  }
}

export function parseQuickCommandsSnapshot(snapshotJson: string): { snapshot: QuickCommandsSnapshot | null; error: string | null } {
  try {
    const parsed = JSON.parse(snapshotJson) as unknown;
    return { snapshot: sanitizeSnapshot(parsed), error: null };
  } catch (error) {
    return { snapshot: null, error: error instanceof Error ? error.message : String(error) };
  }
}

export function applyImportedQuickCommandsSnapshot(
  snapshotJson: string,
  strategy: QuickCommandImportStrategy,
): QuickCommandsImportResult {
  const parsed = parseQuickCommandsSnapshot(snapshotJson);
  if (!parsed.snapshot) {
    return { imported: 0, skipped: 0, errors: [parsed.error ?? 'Invalid Quick Commands snapshot'] };
  }
  return useQuickCommandsStore.getState().applySnapshot(parsed.snapshot, strategy);
}

function sanitizeSnapshot(value: unknown): QuickCommandsSnapshot {
  if (!value || typeof value !== 'object') {
    throw new Error('Quick Commands snapshot must be an object');
  }
  const candidate = value as Partial<QuickCommandsSnapshot>;
  if (candidate.version !== QUICK_COMMANDS_SCHEMA_VERSION) {
    throw new Error(`Unsupported Quick Commands schema version ${String(candidate.version)}`);
  }
  const categories = sanitizeCategories(candidate.categories);
  const commands = sanitizeCommands(candidate.commands, categories);
  return {
    version: QUICK_COMMANDS_SCHEMA_VERSION,
    categories,
    commands,
    updatedAt: typeof candidate.updatedAt === 'number' ? candidate.updatedAt : Date.now(),
  };
}

function sanitizeCategories(value: unknown): QuickCommandCategory[] {
  if (!Array.isArray(value)) throw new Error('Quick Commands categories must be an array');
  if (value.length > MAX_CATEGORIES) throw new Error(`Quick Commands category count exceeds ${MAX_CATEGORIES}`);
  const seen = new Set<string>();
  const categories = value
    .filter((category): category is QuickCommandCategory => (
      category
      && typeof category.id === 'string'
      && typeof category.name === 'string'
      && isQuickCommandIcon(category.icon)
    ))
    .map((category) => ({
      id: boundedRequired(category.id, MAX_ID_LEN),
      name: boundedRequired(category.name, MAX_NAME_LEN),
      icon: category.icon,
    }))
    .filter((category) => {
      if (seen.has(category.id)) return false;
      seen.add(category.id);
      return true;
    });
  return categories.length > 0 ? categories : DEFAULT_QUICK_COMMAND_CATEGORIES;
}

function sanitizeCommands(value: unknown, categories: QuickCommandCategory[]): QuickCommand[] {
  if (!Array.isArray(value)) throw new Error('Quick Commands commands must be an array');
  if (value.length > MAX_COMMANDS) throw new Error(`Quick Commands command count exceeds ${MAX_COMMANDS}`);
  const categoryIds = new Set(categories.map((category) => category.id));
  return value
    .filter((command): command is QuickCommand => (
      command
      && typeof command.id === 'string'
      && typeof command.name === 'string'
      && typeof command.command === 'string'
      && typeof command.category === 'string'
      && typeof command.createdAt === 'number'
      && typeof command.updatedAt === 'number'
    ))
    .map((command) => ({
      id: boundedRequired(command.id, MAX_ID_LEN),
      name: boundedRequired(command.name, MAX_NAME_LEN),
      command: boundedRequired(command.command, MAX_COMMAND_LEN),
      category: categoryIds.has(command.category) ? command.category : 'custom',
      description: boundedOptional(command.description, MAX_DESCRIPTION_LEN),
      hostPattern: boundedOptional(command.hostPattern, MAX_HOST_PATTERN_LEN),
      createdAt: command.createdAt,
      updatedAt: command.updatedAt,
    }));
}

function boundedRequired(value: string, maxLength: number): string {
  const trimmed = value.trim();
  if (!trimmed) throw new Error('Quick Commands required field cannot be empty');
  if (trimmed.length > maxLength) throw new Error(`Quick Commands field exceeds ${maxLength} characters`);
  return trimmed;
}

function boundedOptional(value: unknown, maxLength: number): string | undefined {
  if (typeof value !== 'string') return undefined;
  const trimmed = value.trim();
  if (!trimmed) return undefined;
  if (trimmed.length > maxLength) throw new Error(`Quick Commands field exceeds ${maxLength} characters`);
  return trimmed;
}

function isQuickCommandIcon(value: unknown): value is QuickCommandIcon {
  return value === 'terminal' || value === 'server' || value === 'folder' || value === 'docker' || value === 'zap';
}

export function matchQuickCommandHostPattern(pattern: string | undefined, targetFields: Array<string | null | undefined>): boolean {
  const normalizedPattern = pattern?.trim();
  if (!normalizedPattern) return true;
  const escaped = normalizedPattern.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*');
  const regex = new RegExp(`^${escaped}$`, 'i');
  return targetFields.some((field) => typeof field === 'string' && regex.test(field));
}

export const useQuickCommandsStore = create<QuickCommandsState>((set, get) => ({
  categories: DEFAULT_QUICK_COMMAND_CATEGORIES,
  commands: DEFAULT_QUICK_COMMANDS,
  hydrated: false,
  loading: false,
  lastPersistError: null,

  hydrate: async () => {
    if (get().loading || get().hydrated) return;
    set({ loading: true, lastPersistError: null });
    try {
      const snapshot = await invoke<QuickCommandsSnapshot | null>('load_quick_commands');
      if (snapshot) {
        const sanitized = sanitizeSnapshot(snapshot);
        set({
          categories: sanitized.categories,
          commands: sanitized.commands,
          hydrated: true,
          loading: false,
          lastPersistError: null,
        });
      } else {
        set({ hydrated: true, loading: false, lastPersistError: null });
      }
    } catch (error) {
      console.error('[QuickCommands] Failed to hydrate JSON store:', error);
      set({
        categories: DEFAULT_QUICK_COMMAND_CATEGORIES,
        commands: DEFAULT_QUICK_COMMANDS,
        hydrated: true,
        loading: false,
        lastPersistError: error instanceof Error ? error.message : String(error),
      });
    }
  },

  upsertCommand: (draft) => {
    const now = Date.now();
    const existing = draft.id ? get().commands.find((command) => command.id === draft.id) : undefined;
    const command: QuickCommand = {
      id: draft.id ?? newId(),
      name: draft.name.trim(),
      command: draft.command.trim(),
      category: draft.category || 'custom',
      description: draft.description?.trim() || undefined,
      hostPattern: draft.hostPattern?.trim() || undefined,
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
    };
    set((state) => {
      const commands = existing
        ? state.commands.map((candidate) => candidate.id === command.id ? command : candidate)
        : [...state.commands, command];
      void persistSnapshot(state.categories, commands)
        .then(() => useQuickCommandsStore.setState({ lastPersistError: null }))
        .catch((error) => useQuickCommandsStore.setState({ lastPersistError: String(error) }));
      return { commands };
    });
    return command;
  },

  deleteCommand: (id) => set((state) => {
    const commands = state.commands.filter((command) => command.id !== id);
    void persistSnapshot(state.categories, commands)
      .then(() => useQuickCommandsStore.setState({ lastPersistError: null }))
      .catch((error) => useQuickCommandsStore.setState({ lastPersistError: String(error) }));
    return { commands };
  }),

  upsertCategory: (draft) => {
    const existing = draft.id ? get().categories.find((category) => category.id === draft.id) : undefined;
    const category: QuickCommandCategory = {
      id: draft.id ?? newCategoryId(),
      name: draft.name.trim(),
      icon: draft.icon,
    };
    set((state) => {
      const categories = existing
        ? state.categories.map((candidate) => candidate.id === category.id ? category : candidate)
        : [...state.categories, category];
      void persistSnapshot(categories, state.commands)
        .then(() => useQuickCommandsStore.setState({ lastPersistError: null }))
        .catch((error) => useQuickCommandsStore.setState({ lastPersistError: String(error) }));
      return { categories };
    });
    return category;
  },

  deleteCategory: (id) => {
    const state = get();
    const isDefaultCategory = DEFAULT_QUICK_COMMAND_CATEGORIES.some((category) => category.id === id);
    const isInUse = state.commands.some((command) => command.category === id);
    if (isDefaultCategory || isInUse) return false;
    const categories = state.categories.filter((category) => category.id !== id);
    void persistSnapshot(categories, state.commands)
      .then(() => useQuickCommandsStore.setState({ lastPersistError: null }))
      .catch((error) => useQuickCommandsStore.setState({ lastPersistError: String(error) }));
    set({ categories });
    return true;
  },

  resetDefaults: () => {
    void persistSnapshot(DEFAULT_QUICK_COMMAND_CATEGORIES, DEFAULT_QUICK_COMMANDS)
      .then(() => useQuickCommandsStore.setState({ lastPersistError: null }))
      .catch((error) => useQuickCommandsStore.setState({ lastPersistError: String(error) }));
    set({
      categories: DEFAULT_QUICK_COMMAND_CATEGORIES,
      commands: DEFAULT_QUICK_COMMANDS,
    });
  },

  applySnapshot: (snapshot, strategy) => {
    const current = get();
    const result = mergeQuickCommandsSnapshot(
      { categories: current.categories, commands: current.commands },
      snapshot,
      strategy,
    );
    void persistSnapshot(result.categories, result.commands)
      .then(() => useQuickCommandsStore.setState({ lastPersistError: null }))
      .catch((error) => useQuickCommandsStore.setState({ lastPersistError: String(error) }));
    set({
      categories: result.categories,
      commands: result.commands,
    });
    return {
      imported: result.imported,
      skipped: result.skipped,
      errors: [],
    };
  },
}));

function mergeQuickCommandsSnapshot(
  current: Pick<QuickCommandsState, 'categories' | 'commands'>,
  incoming: QuickCommandsSnapshot,
  strategy: QuickCommandImportStrategy,
): Pick<QuickCommandsState, 'categories' | 'commands'> & { imported: number; skipped: number } {
  const now = Date.now();
  let imported = 0;
  let skipped = 0;
  let categories = current.categories.map((category) => ({ ...category }));
  let commands = current.commands.map((command) => ({ ...command }));
  const categoryRemap = new Map<string, string>();

  for (const importedCategory of incoming.categories) {
    const conflict = findCategoryConflict(categories, importedCategory);
    if (!conflict) {
      categories = [...categories, importedCategory];
      categoryRemap.set(importedCategory.id, importedCategory.id);
      continue;
    }

    if (strategy === 'skip') {
      categoryRemap.set(importedCategory.id, conflict.id);
      skipped += 1;
      continue;
    }

    if (strategy === 'rename') {
      const renamed = {
        ...importedCategory,
        id: newCategoryId(),
        name: uniqueCategoryName(categories, `${importedCategory.name} (Imported)`),
      };
      categories = [...categories, renamed];
      categoryRemap.set(importedCategory.id, renamed.id);
      imported += 1;
      continue;
    }

    const nextCategory = {
      ...conflict,
      name: importedCategory.name,
      icon: importedCategory.icon,
    };
    categories = categories.map((category) => category.id === conflict.id ? nextCategory : category);
    categoryRemap.set(importedCategory.id, conflict.id);
    imported += 1;
  }

  const categoryIds = new Set(categories.map((category) => category.id));
  for (const importedCommand of incoming.commands) {
    const remappedCategory = categoryRemap.get(importedCommand.category) ?? importedCommand.category;
    const category = categoryIds.has(remappedCategory) ? remappedCategory : 'custom';
    const commandWithCategory = { ...importedCommand, category };
    const conflict = findCommandConflict(commands, commandWithCategory);
    if (!conflict) {
      commands = [...commands, commandWithCategory];
      imported += 1;
      continue;
    }

    if (strategy === 'skip') {
      skipped += 1;
      continue;
    }

    if (strategy === 'rename') {
      commands = [...commands, {
        ...commandWithCategory,
        id: newId(),
        name: uniqueCommandName(commands, category, `${commandWithCategory.name} (Imported)`),
      }];
      imported += 1;
      continue;
    }

    const replacement: QuickCommand = strategy === 'merge'
      ? {
          ...conflict,
          name: commandWithCategory.name,
          command: commandWithCategory.command,
          category,
          description: commandWithCategory.description,
          hostPattern: commandWithCategory.hostPattern,
          updatedAt: now,
        }
      : {
          ...commandWithCategory,
          id: conflict.id,
          createdAt: commandWithCategory.createdAt,
          updatedAt: now,
        };
    commands = commands.map((command) => command.id === conflict.id ? replacement : command);
    imported += 1;
  }

  return { categories, commands, imported, skipped };
}

function findCategoryConflict(categories: QuickCommandCategory[], category: QuickCommandCategory): QuickCommandCategory | undefined {
  const normalizedName = category.name.trim().toLowerCase();
  return categories.find((candidate) => (
    candidate.id === category.id || candidate.name.trim().toLowerCase() === normalizedName
  ));
}

function findCommandConflict(commands: QuickCommand[], command: QuickCommand): QuickCommand | undefined {
  const normalizedName = command.name.trim().toLowerCase();
  return commands.find((candidate) => (
    candidate.id === command.id
    || (candidate.category === command.category && candidate.name.trim().toLowerCase() === normalizedName)
  ));
}

function uniqueCategoryName(categories: QuickCommandCategory[], desiredName: string): string {
  const existing = new Set(categories.map((category) => category.name.trim().toLowerCase()));
  return uniqueName(desiredName, existing);
}

function uniqueCommandName(commands: QuickCommand[], category: string, desiredName: string): string {
  const existing = new Set(
    commands
      .filter((command) => command.category === category)
      .map((command) => command.name.trim().toLowerCase()),
  );
  return uniqueName(desiredName, existing);
}

function uniqueName(desiredName: string, existingLowerNames: Set<string>): string {
  if (!existingLowerNames.has(desiredName.trim().toLowerCase())) return desiredName;
  for (let index = 2; index < 1000; index += 1) {
    const candidate = `${desiredName} (${index})`;
    if (!existingLowerNames.has(candidate.trim().toLowerCase())) return candidate;
  }
  return `${desiredName} (${Date.now()})`;
}
