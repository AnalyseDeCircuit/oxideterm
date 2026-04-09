import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/platform', () => ({
  platform: {
    isMac: false,
  },
}));

const STORAGE_KEY = 'oxideterm_keybindings';

async function loadModules() {
  const [{ useKeybindingStore }, registry] = await Promise.all([
    import('@/store/keybindingStore'),
    import('@/lib/keybindingRegistry'),
  ]);

  return {
    useKeybindingStore,
    getBinding: registry.getBinding,
  };
}

describe('keybindingStore', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    localStorage.clear();
  });

  it('loads valid overrides and skips malformed entries from localStorage', async () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

    localStorage.setItem(STORAGE_KEY, JSON.stringify({
      'app.newTerminal': {
        other: { key: 'y', ctrl: true, shift: false, alt: false, meta: false },
      },
      'app.settings': {
        other: { key: 'x', ctrl: true },
      },
      'app.unknown': {
        other: { key: 'q', ctrl: true, shift: false, alt: false, meta: false },
      },
    }));

    const { useKeybindingStore, getBinding } = await loadModules();

    const overrides = useKeybindingStore.getState().overrides;
    expect(overrides.size).toBe(1);
    expect(overrides.has('app.newTerminal')).toBe(true);
    expect(overrides.has('app.settings')).toBe(false);
    expect(getBinding('app.newTerminal')).toEqual({
      key: 'y',
      ctrl: true,
      shift: false,
      alt: false,
      meta: false,
    });
    expect(warnSpy).toHaveBeenCalledTimes(2);
  });

  it('clears corrupt persisted payloads and falls back to defaults', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    localStorage.setItem(STORAGE_KEY, '[]');

    const { useKeybindingStore, getBinding } = await loadModules();

    expect(useKeybindingStore.getState().overrides.size).toBe(0);
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
    expect(getBinding('app.newTerminal')).toEqual({
      key: 't',
      ctrl: true,
      shift: false,
      alt: false,
      meta: false,
    });
    expect(errorSpy).toHaveBeenCalledOnce();
  });

  it('persists and reloads terminal.paste overrides for Windows/Linux remaps', async () => {
    const { useKeybindingStore, getBinding } = await loadModules();

    useKeybindingStore.getState().setBinding('terminal.paste', 'other', {
      key: 'v',
      ctrl: true,
      shift: false,
      alt: false,
      meta: false,
    });

    expect(getBinding('terminal.paste')).toEqual({
      key: 'v',
      ctrl: true,
      shift: false,
      alt: false,
      meta: false,
    });

    vi.resetModules();

    const reloaded = await loadModules();
    expect(reloaded.getBinding('terminal.paste')).toEqual({
      key: 'v',
      ctrl: true,
      shift: false,
      alt: false,
      meta: false,
    });
  });
});