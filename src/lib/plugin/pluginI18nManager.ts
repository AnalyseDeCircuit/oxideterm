// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Plugin i18n Manager
 *
 * Wraps i18next to provide plugin-scoped translation functions.
 * Plugin keys are automatically prefixed: `plugin.{pluginId}.{key}`
 * Plugin locale files are loaded via `i18n.addResourceBundle()`.
 */

import i18n from 'i18next';
import { useAppStore } from '../../store/appStore';
import { usePluginStore } from '../../store/pluginStore';

export function createPluginI18nManager(pluginId: string) {
  const prefix = `plugin.${pluginId}.`;

  return {
    /** Translate a key (auto-prefixed with plugin namespace) */
    t(key: string, params?: Record<string, string | number>): string {
      const fullKey = `${prefix}${key}`;
      const result = i18n.t(fullKey, params as Record<string, string>);
      // i18next returns the full key string on failed lookup — detect and fall back
      return (result && result !== fullKey) ? result : key;
    },

    /** Get current language */
    getLanguage(): string {
      return i18n.language;
    },

    /** Subscribe to language changes */
    onLanguageChange(handler: (lang: string) => void): () => void {
      const callback = (lng: string) => {
        try { handler(lng); } catch { /* swallow */ }
      };
      i18n.on('languageChanged', callback);
      return () => {
        i18n.off('languageChanged', callback);
      };
    },
  };
}

/**
 * Load plugin locale resources into i18next.
 * Called during plugin loading if the plugin provides a locales directory.
 */
export async function loadPluginI18n(
  pluginId: string,
  locales: Record<string, Record<string, string>>,
): Promise<void> {
  for (const [lang, translations] of Object.entries(locales)) {
    // Nest under `plugin.{pluginId}` namespace
    const nested: Record<string, unknown> = { plugin: { [pluginId]: translations } };
    i18n.addResourceBundle(lang, 'translation', nested, true, true);
  }
}

/**
 * Remove plugin locale resources from i18next.
 */
export function removePluginI18n(pluginId: string): void {
  // i18next doesn't have a clean removeResourceBundle API for nested keys.
  // We overwrite the plugin namespace with empty object for each language.
  for (const lang of Object.keys(i18n.store.data)) {
    const nested: Record<string, unknown> = { plugin: { [pluginId]: {} } };
    i18n.addResourceBundle(lang, 'translation', nested, true, true);
  }
}

/**
 * Resolve a plugin tab title dynamically.
 * Called on every render by TabBar to keep titles in sync with the current language.
 *
 * @param compositeKey - "pluginId:tabId" composite key from Tab.pluginTabId
 * @returns Resolved translated title, or null if unavailable
 */
export function resolvePluginTabTitle(compositeKey: string): string | null {
  const colonIdx = compositeKey.indexOf(':');
  if (colonIdx < 0) return null;

  const pluginId = compositeKey.slice(0, colonIdx);
  const tabId = compositeKey.slice(colonIdx + 1);

  // Look up manifest from store
  const pluginInfo = usePluginStore.getState().getPlugin(pluginId);
  if (!pluginInfo?.manifest) return null;

  const tabDef = pluginInfo.manifest.contributes?.tabs?.find(
    (t: { id: string; title?: string }) => t.id === tabId,
  );
  if (!tabDef?.title) return null;

  // Try translating via plugin-scoped key
  const fullKey = `plugin.${pluginId}.${tabDef.title}`;
  const result = i18n.t(fullKey);
  // i18next returns the key itself on failed lookup
  if (result && result !== fullKey) return result;

  // Fallback: use the raw manifest title (at least it's human-readable)
  return tabDef.title;
}

/**
 * Refresh stored titles for all open plugin tabs after a language change.
 * Some consumers read Tab.title directly instead of resolving titles at render time.
 */
export function refreshOpenPluginTabTitles(): void {
  useAppStore.setState((state) => ({
    tabs: state.tabs.map((tab) => {
      if (tab.type !== 'plugin' || !tab.pluginTabId) {
        return tab;
      }

      const resolvedTitle = resolvePluginTabTitle(tab.pluginTabId);
      if (!resolvedTitle || resolvedTitle === tab.title) {
        return tab;
      }

      return {
        ...tab,
        title: resolvedTitle,
      };
    }),
  }));
}
