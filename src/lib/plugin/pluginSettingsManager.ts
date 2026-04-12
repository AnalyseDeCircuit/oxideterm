// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Plugin Settings Manager
 *
 * Manages per-plugin settings with localStorage persistence.
 * Settings are declared in plugin.json contributes.settings.
 * Storage key pattern: `oxide-plugin-{pluginId}-setting-{settingId}`
 */

import type { PluginManifest, PluginSettingDef } from '../../types/plugin';

const SETTING_PREFIX = 'oxide-plugin-';
const SETTING_SEPARATOR = '-setting-';

export type PluginSettingSnapshotEntry = {
  storageKey: string;
  serializedValue: string;
};

function stableHashString(value: string): string {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return `fnv1a-${(hash >>> 0).toString(16)}`;
}

type RegisteredManager = {
  notifyImportedSetting: (key: string, value: unknown) => void;
};

const managerRegistry = new Map<string, Set<RegisteredManager>>();

function settingKey(pluginId: string, key: string): string {
  return `${SETTING_PREFIX}${pluginId}-setting-${key}`;
}

export function parseSettingStorageKey(storageKey: string): { pluginId: string; settingId: string } | null {
  if (!storageKey.startsWith(SETTING_PREFIX)) {
    return null;
  }

  const remainder = storageKey.slice(SETTING_PREFIX.length);
  const separatorIndex = remainder.indexOf(SETTING_SEPARATOR);
  if (separatorIndex === -1) {
    return null;
  }

  const pluginId = remainder.slice(0, separatorIndex);
  const settingId = remainder.slice(separatorIndex + SETTING_SEPARATOR.length);
  if (!pluginId || !settingId) {
    return null;
  }

  return { pluginId, settingId };
}

export function collectPluginSettingsSnapshot(): PluginSettingSnapshotEntry[] {
  const entries: PluginSettingSnapshotEntry[] = [];

  for (let index = 0; index < localStorage.length; index += 1) {
    const storageKey = localStorage.key(index);
    if (!storageKey || !parseSettingStorageKey(storageKey)) {
      continue;
    }

    const serializedValue = localStorage.getItem(storageKey);
    if (serializedValue === null) {
      continue;
    }

    entries.push({ storageKey, serializedValue });
  }

  entries.sort((left, right) => left.storageKey.localeCompare(right.storageKey));
  return entries;
}

export function buildPluginSettingsRevisionMap(
  entries: readonly PluginSettingSnapshotEntry[] = collectPluginSettingsSnapshot(),
): Record<string, string> {
  const groupedEntries = new Map<string, PluginSettingSnapshotEntry[]>();

  for (const entry of entries) {
    const parsed = parseSettingStorageKey(entry.storageKey);
    if (!parsed) {
      continue;
    }

    if (!groupedEntries.has(parsed.pluginId)) {
      groupedEntries.set(parsed.pluginId, []);
    }
    groupedEntries.get(parsed.pluginId).push(entry);
  }

  return Object.fromEntries(
    Array.from(groupedEntries.entries())
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([pluginId, pluginEntries]) => {
        const normalized = pluginEntries
          .slice()
          .sort((left, right) => left.storageKey.localeCompare(right.storageKey))
          .map((entry) => [entry.storageKey, entry.serializedValue]);
        return [pluginId, stableHashString(JSON.stringify(normalized))];
      }),
  );
}

export function applyImportedPluginSettingsSnapshot(
  entries: readonly PluginSettingSnapshotEntry[],
): number {
  let appliedCount = 0;

  for (const entry of entries) {
    const parsed = parseSettingStorageKey(entry.storageKey);
    if (!parsed) {
      continue;
    }

    try {
      localStorage.setItem(entry.storageKey, entry.serializedValue);
    } catch {
      continue;
    }

    appliedCount += 1;

    let decodedValue: unknown;
    try {
      decodedValue = JSON.parse(entry.serializedValue);
    } catch {
      continue;
    }

    const managers = managerRegistry.get(parsed.pluginId);
    if (!managers) {
      continue;
    }

    for (const manager of managers) {
      try {
        manager.notifyImportedSetting(parsed.settingId, decodedValue);
      } catch {
        // ignore plugin handler failures during import replay
      }
    }
  }

  return appliedCount;
}

type ChangeHandler = (newValue: unknown) => void;

export function createPluginSettingsManager(pluginId: string, manifest: PluginManifest) {
  const declaredSettings = new Map<string, PluginSettingDef>();
  for (const def of manifest.contributes?.settings ?? []) {
    declaredSettings.set(def.id, def);
  }

  const changeHandlers = new Map<string, Set<ChangeHandler>>();
  const registeredManager: RegisteredManager = {
    notifyImportedSetting(key: string, value: unknown): void {
      const handlers = changeHandlers.get(key);
      if (!handlers) {
        return;
      }

      for (const handler of handlers) {
        try { handler(value); } catch { /* swallow */ }
      }
    },
  };

  if (!managerRegistry.has(pluginId)) {
    managerRegistry.set(pluginId, new Set());
  }
  managerRegistry.get(pluginId)!.add(registeredManager);

  return {
    get<T>(key: string): T {
      const def = declaredSettings.get(key);
      const storageKey = settingKey(pluginId, key);
      try {
        const raw = localStorage.getItem(storageKey);
        if (raw !== null) return JSON.parse(raw) as T;
      } catch { /* fall through to default */ }
      // Return declared default or undefined
      return (def?.default as T) ?? (undefined as T);
    },

    set<T>(key: string, value: T): void {
      const storageKey = settingKey(pluginId, key);
      try {
        localStorage.setItem(storageKey, JSON.stringify(value));
      } catch { /* swallow */ }

      // Notify change handlers
      const handlers = changeHandlers.get(key);
      if (handlers) {
        for (const handler of handlers) {
          try { handler(value); } catch { /* swallow */ }
        }
      }
    },

    onChange(key: string, handler: ChangeHandler): () => void {
      if (!changeHandlers.has(key)) {
        changeHandlers.set(key, new Set());
      }
      changeHandlers.get(key)!.add(handler);

      return () => {
        const set = changeHandlers.get(key);
        if (set) {
          set.delete(handler);
          if (set.size === 0) changeHandlers.delete(key);
        }
      };
    },

    /** Get all declared settings with their current values */
    getAllSettings(): Array<{ def: PluginSettingDef; value: unknown }> {
      return Array.from(declaredSettings.values()).map((def) => ({
        def,
        value: this.get(def.id),
      }));
    },
  };
}
