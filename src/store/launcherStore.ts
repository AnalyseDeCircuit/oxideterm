/**
 * Launcher Store
 *
 * Global state for the platform application launcher.
 * - macOS: lists installed .app bundles from /Applications
 * - Windows: lists WSL distros (reuses wsl_graphics_list_distros)
 */

import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { platform } from '../lib/platform';

// ── Types ────────────────────────────────────────────────────────────────────

export interface AppEntry {
  name: string;
  path: string;
  bundleId: string | null;
  iconPath: string | null;
}

/** Response from launcher_list_apps */
interface LauncherListResponse {
  apps: AppEntry[];
  iconDir: string | null;
}

export interface WslDistro {
  name: string;
  is_default: boolean;
  is_running: boolean;
}

interface LauncherStore {
  /** macOS: list of installed applications */
  apps: AppEntry[];
  /** macOS: icon cache directory (asset-protocol-granted) */
  iconDir: string | null;
  /** Windows: list of WSL distros */
  wslDistros: WslDistro[];
  /** Current search query */
  searchQuery: string;
  /** Whether the initial scan is in progress */
  loading: boolean;
  /** Error message if scan failed */
  error: string | null;

  /** Load apps (platform-aware) */
  loadApps: () => Promise<void>;
  /** Launch an app by path (macOS) */
  launchApp: (path: string) => Promise<void>;
  /** Launch a WSL distro (Windows) */
  launchWsl: (distro: string) => Promise<void>;
  /** Update search query */
  setSearch: (query: string) => void;
}

export const useLauncherStore = create<LauncherStore>((set, get) => ({
  apps: [],
  iconDir: null,
  wslDistros: [],
  searchQuery: '',
  loading: false,
  error: null,

  loadApps: async () => {
    if (get().loading) return;
    set({ loading: true, error: null });
    try {
      if (platform.isMac) {
        const resp = await invoke<LauncherListResponse>('launcher_list_apps');
        set({ apps: resp.apps, iconDir: resp.iconDir, loading: false });
      } else if (platform.isWindows) {
        const distros = await invoke<WslDistro[]>('wsl_graphics_list_distros');
        set({ wslDistros: distros, loading: false });
      } else {
        set({ loading: false });
      }
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },

  launchApp: async (path: string) => {
    try {
      await invoke('launcher_launch_app', { path });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  launchWsl: async (distro: string) => {
    try {
      await invoke('launcher_wsl_launch', { distro });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  setSearch: (query: string) => set({ searchQuery: query }),
}));
