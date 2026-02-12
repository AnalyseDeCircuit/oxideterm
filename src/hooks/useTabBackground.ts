import { useSettingsStore } from '../store/settingsStore';

/**
 * Returns whether background image is active for a given tab type.
 * Views use this to conditionally make their root container transparent
 * so the background image layer (rendered by TabBackgroundWrapper in AppLayout)
 * can show through.
 */
export function useTabBgActive(tabType: string): boolean {
  const backgroundEnabled = useSettingsStore((s) => s.settings.terminal.backgroundEnabled);
  const backgroundImage = useSettingsStore((s) => s.settings.terminal.backgroundImage);
  const enabledTabs = useSettingsStore((s) => s.settings.terminal.backgroundEnabledTabs) ?? ['terminal', 'local_terminal'];
  return backgroundEnabled !== false && !!backgroundImage && enabledTabs.includes(tabType);
}
