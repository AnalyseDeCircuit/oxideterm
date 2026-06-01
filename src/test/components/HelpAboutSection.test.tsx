import { describe, expect, it, vi } from 'vitest';
import type { MouseEvent } from 'react';
import { handleReleaseNotesLinkClick } from '@/components/settings/HelpAboutSection';
import { safeOpenUrl } from '@/lib/safeUrl';

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn().mockResolvedValue('1.4.7'),
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock('@/components/fileManager/fontUtils', () => ({
  getFontFamilyCSS: () => 'monospace',
}));

vi.mock('@/store/updateStore', () => ({
  useUpdateStore: () => ({
    stage: 'idle',
    releaseBody: null,
    checkForUpdate: vi.fn(),
    clearSkippedVersion: vi.fn(),
  }),
}));

vi.mock('@/store/settingsStore', () => {
  const state = {
    settings: {
      general: { updateChannel: 'stable' },
      terminal: { fontFamily: 'jetbrains-mono', customFontFamily: '' },
    },
    updateGeneral: vi.fn(),
  };

  return {
    useSettingsStore: (selector?: (value: typeof state) => unknown) =>
      selector ? selector(state) : state,
  };
});

vi.mock('@/lib/api', () => ({
  api: {
    openLogDirectory: vi.fn(),
  },
}));

vi.mock('@/lib/platform', () => ({
  platform: { isMac: false },
}));

vi.mock('@/lib/shortcuts', () => ({
  getShortcutCategories: () => [],
}));

vi.mock('@/lib/safeUrl', () => ({
  safeOpenUrl: vi.fn().mockResolvedValue(true),
}));

vi.mock('@/components/settings/MemoryDiagnosticsPanel', () => ({
  MemoryDiagnosticsPanel: () => null,
}));

describe('HelpAboutSection', () => {
  it('opens release note links in the system browser instead of navigating the WebView', () => {
    document.body.innerHTML = '<div><a href="https://github.com/AnalyseDeCircuit/oxideterm/releases">changelog</a></div>';
    const link = document.querySelector('a')!;
    const event = {
      target: link,
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
    } as unknown as MouseEvent<HTMLElement>;

    handleReleaseNotesLinkClick(event);

    expect(event.preventDefault).toHaveBeenCalled();
    expect(event.stopPropagation).toHaveBeenCalled();
    expect(safeOpenUrl).toHaveBeenCalledWith('https://github.com/AnalyseDeCircuit/oxideterm/releases');
  });
});
