/**
 * Unified Settings Store (v2)
 * 
 * Single Source of Truth for all user preferences and UI state.
 * 
 * Design Principles:
 * 1. All settings read/write through this store
 * 2. Immediate persistence on every change (no beforeunload dependency)
 * 3. Legacy format detection and cleanup (no migration, reset to defaults)
 * 4. Zustand with subscribeWithSelector for reactive updates
 */

import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import { themes } from '../lib/themes';
import { useToastStore } from '../hooks/useToast';
import { getFontFamilyCSS } from '../components/fileManager/fontUtils';
import i18n from '../i18n';
import { DEFAULT_PROVIDERS } from '../lib/ai/providers';

// ============================================================================
// Constants
// ============================================================================

/** Settings data version, used to detect legacy formats */
const SETTINGS_VERSION = 2;

/** localStorage key */
const STORAGE_KEY = 'oxide-settings-v2';

/** Legacy localStorage keys to clean up */
const LEGACY_KEYS = [
  'oxide-settings',
  'oxide-ui-state',
  'oxide-tree-expanded',
  'oxide-focused-node',
] as const;

// ============================================================================
// Types
// ============================================================================

/** Renderer type */
export type RendererType = 'auto' | 'webgl' | 'canvas';

/** 
 * Font family options - "双轨制" (Dual-Track System)
 * 
 * v1.4.0+: Extended with dual-track font system
 * 
 * 预设轨道 (Preset Track):
 * - jetbrains: JetBrains Mono NF (Subset) (bundled woff2 fallback)
 * - meslo: MesloLGM NF (Subset) (bundled woff2 fallback)
 * - maple: Maple Mono NF CN (Subset) (bundled, CJK optimized)
 * - cascadia: Cascadia Code (system, Windows)
 * - consolas: Consolas (system, Windows)
 * - menlo: Menlo (system, macOS)
 * 
 * 自定义轨道 (Custom Track):
 * - custom: User-defined font stack via customFontFamily field
 */
export type FontFamily = 
  | 'jetbrains'   // JetBrains Mono Nerd Font (内置保底)
  | 'meslo'       // Meslo Nerd Font (内置保底)
  | 'maple'       // Maple Mono NF CN (内置，CJK 优化)
  | 'cascadia'    // Cascadia Code (系统字体)
  | 'consolas'    // Consolas (系统字体)
  | 'menlo'       // Menlo (系统字体)
  | 'custom';     // 自定义字体栈

/** Cursor style options */
export type CursorStyle = 'block' | 'underline' | 'bar';

/** Sidebar section options (string allows plugin:* dynamic sections) */
export type SidebarSection = 'sessions' | 'saved' | 'sftp' | 'forwards' | 'connections' | 'ai' | (string & {});

/** Language options */
export type Language = 'zh-CN' | 'en' | 'fr-FR' | 'ja' | 'es-ES' | 'pt-BR' | 'vi' | 'ko' | 'de' | 'it' | 'zh-TW';

/** General settings */
export interface GeneralSettings {
  language: Language;
}

/** Terminal background image fit mode */
export type BackgroundFit = 'cover' | 'contain' | 'fill' | 'tile';

/** Terminal settings */
export interface TerminalSettings {
  theme: string;
  fontFamily: FontFamily;
  customFontFamily: string; // 自定义轨道: user-defined font stack (e.g. "'Sarasa Fixed SC', monospace")
  fontSize: number;        // 8-32
  lineHeight: number;      // 0.8-3.0
  cursorStyle: CursorStyle;
  cursorBlink: boolean;
  scrollback: number;      // xterm scrollback lines
  renderer: RendererType;
  pasteProtection: boolean; // Confirm before pasting multi-line content
  osc52Clipboard: boolean;  // Allow remote programs to write system clipboard via OSC 52
  // Background image settings
  backgroundImage: string | null;    // Stored image path (app_data_dir/backgrounds/...)
  backgroundOpacity: number;         // Image opacity 0.03-0.5 (default 0.15)
  backgroundBlur: number;            // Blur in px 0-20 (default 0)
  backgroundFit: BackgroundFit;      // How the image fills the terminal area
  backgroundEnabledTabs: string[];   // Which tab types show the background image
}

/** Buffer settings (used by backend) */
export interface BufferSettings {
  maxLines: number;          // Backend ScrollBuffer max lines
  saveOnDisconnect: boolean; // Save buffer on disconnect
}

/** Appearance settings */
export interface AppearanceSettings {
  sidebarCollapsedDefault: boolean;
}

/** Connection defaults */
export interface ConnectionDefaults {
  username: string;
  port: number;
}

/** Tree UI state (persisted for UX, but pruned on rawNodes sync) */
export interface TreeUIState {
  expandedIds: string[];
  focusedNodeId: string | null;
}

/** Sidebar UI state */
export interface SidebarUIState {
  collapsed: boolean;
  activeSection: SidebarSection;
  width: number;  // Sidebar width in pixels (200-600)
  // AI sidebar (right side)
  aiSidebarCollapsed: boolean;
  aiSidebarWidth: number;  // AI sidebar width in pixels (280-500)
}

/** AI thinking display style */
export type AiThinkingStyle = 'detailed' | 'compact';

/** AI context source */
export type AiContextSource = 'selection' | 'visible' | 'command';

/** AI settings */
export interface AiSettings {
  enabled: boolean;
  enabledConfirmed: boolean;  // User has confirmed the privacy notice
  // Legacy single-provider fields (kept for migration)
  baseUrl: string;
  model: string;
  // Multi-provider support
  providers: import('../types').AiProvider[];
  activeProviderId: string | null;
  activeModel: string | null;
  // Context settings
  contextMaxChars: number;      // Max characters to send
  contextVisibleLines: number;  // Max visible lines to capture
  /** Thinking block display style: detailed (full) or compact (collapsed) */
  thinkingStyle: AiThinkingStyle;
  /** Whether thinking blocks are expanded by default */
  thinkingDefaultExpanded: boolean;
}

/** Local terminal settings */
export interface LocalTerminalSettings {
  defaultShellId: string | null;  // User's preferred default shell ID
  recentShellIds: string[];       // Recently used shell IDs (max 5)
  defaultCwd: string | null;      // Default working directory
  // Shell profile loading
  loadShellProfile: boolean;      // Whether to load shell profile ($PROFILE for PowerShell, ~/.bashrc etc.)
  // Oh My Posh support (Windows)
  ohMyPoshEnabled: boolean;       // Enable Oh My Posh integration
  ohMyPoshTheme: string | null;   // Path to OMP theme file (.omp.json)
  // Custom environment variables for shell
  customEnvVars: Record<string, string>;
}

/** SFTP transfer settings */
export interface SftpSettings {
  maxConcurrentTransfers: number;  // Max concurrent transfers (1-10)
  speedLimitEnabled: boolean;      // Enable bandwidth limiting
  speedLimitKBps: number;          // Speed limit in KB/s (0 = unlimited)
  conflictAction: 'ask' | 'overwrite' | 'skip' | 'rename';  // Default conflict resolution
}

/** Complete settings structure */
export interface PersistedSettingsV2 {
  version: 2;
  general: GeneralSettings;
  terminal: TerminalSettings;
  buffer: BufferSettings;
  appearance: AppearanceSettings;
  connectionDefaults: ConnectionDefaults;
  treeUI: TreeUIState;
  sidebarUI: SidebarUIState;
  ai: AiSettings;
  localTerminal?: LocalTerminalSettings;
  sftp?: SftpSettings;
  experimental?: ExperimentalSettings;
}

/** Experimental feature flags */
export interface ExperimentalSettings {
  /**
   * @deprecated Since Cycle 1 — nodeId-based proxy is now the only path.
   * Kept for settings schema compatibility; value is ignored at runtime.
   */
  virtualSessionProxy: boolean;
}

// ============================================================================
// Platform Detection
// ============================================================================

const isWindows = typeof navigator !== 'undefined'
  && navigator.platform.toLowerCase().includes('win');

// ============================================================================
// Default Values
// ============================================================================

const defaultGeneralSettings: GeneralSettings = {
  language: 'zh-CN',  // Default to Chinese
};

const defaultTerminalSettings: TerminalSettings = {
  theme: 'default',
  fontFamily: 'jetbrains',
  customFontFamily: '',  // 自定义轨道为空时不生效
  fontSize: 14,
  lineHeight: 1.2,
  cursorStyle: 'block',
  cursorBlink: true,
  scrollback: 3000,
  renderer: isWindows ? 'canvas' : 'auto',
  pasteProtection: true,  // Default enabled for safety
  osc52Clipboard: false,  // Opt-in: user must explicitly enable OSC 52 clipboard bridge
  // Background image defaults
  backgroundImage: null,
  backgroundOpacity: 0.15,
  backgroundBlur: 0,
  backgroundFit: 'cover',
  backgroundEnabledTabs: ['terminal', 'local_terminal'],
};

const defaultBufferSettings: BufferSettings = {
  maxLines: 30000,
  saveOnDisconnect: true,
};

const defaultAppearanceSettings: AppearanceSettings = {
  sidebarCollapsedDefault: false,
};

const defaultConnectionDefaults: ConnectionDefaults = {
  username: 'root',
  port: 22,
};

const defaultTreeUIState: TreeUIState = {
  expandedIds: [],
  focusedNodeId: null,
};

const defaultSidebarUIState: SidebarUIState = {
  collapsed: false,
  activeSection: 'sessions',
  width: 300,  // Default sidebar width
  // AI sidebar defaults
  aiSidebarCollapsed: true,  // Start collapsed
  aiSidebarWidth: 340,       // Default AI sidebar width
};

const defaultAiSettings: AiSettings = {
  enabled: false,
  enabledConfirmed: false,
  baseUrl: 'https://api.openai.com/v1',
  model: 'gpt-4o-mini',
  providers: [],   // Populated on first migration
  activeProviderId: null,
  activeModel: null,
  contextMaxChars: 8000,
  contextVisibleLines: 120,
  thinkingStyle: 'detailed',         // Default: show full thinking content
  thinkingDefaultExpanded: false,    // Default: collapsed for less noise
};

const defaultLocalTerminalSettings: LocalTerminalSettings = {
  defaultShellId: null,
  recentShellIds: [],
  defaultCwd: null,
  loadShellProfile: true,       // Default: load profile for complete shell environment
  ohMyPoshEnabled: false,       // Default: disabled
  ohMyPoshTheme: null,          // No theme selected
  customEnvVars: {},            // No custom env vars
};

const defaultSftpSettings: SftpSettings = {
  maxConcurrentTransfers: 3,
  speedLimitEnabled: false,
  speedLimitKBps: 0,
  conflictAction: 'ask',
};

function createDefaultSettings(): PersistedSettingsV2 {
  return {
    version: 2,
    general: { ...defaultGeneralSettings },
    terminal: { ...defaultTerminalSettings },
    buffer: { ...defaultBufferSettings },
    appearance: { ...defaultAppearanceSettings },
    connectionDefaults: { ...defaultConnectionDefaults },
    treeUI: { ...defaultTreeUIState },
    sidebarUI: { ...defaultSidebarUIState },
    ai: { ...defaultAiSettings },
    localTerminal: { ...defaultLocalTerminalSettings },
    sftp: { ...defaultSftpSettings },
    experimental: { virtualSessionProxy: false },
  };
}

// ============================================================================
// Persistence Helpers
// ============================================================================

/** Merge saved settings with defaults (handles version upgrades with new fields) */
function mergeWithDefaults(saved: Partial<PersistedSettingsV2>): PersistedSettingsV2 {
  const defaults = createDefaultSettings();
  return {
    version: 2,
    general: { ...defaults.general, ...saved.general },
    terminal: { ...defaults.terminal, ...saved.terminal },
    buffer: { ...defaults.buffer, ...saved.buffer },
    appearance: { ...defaults.appearance, ...saved.appearance },
    connectionDefaults: { ...defaults.connectionDefaults, ...saved.connectionDefaults },
    treeUI: { ...defaults.treeUI, ...saved.treeUI },
    sidebarUI: { ...defaults.sidebarUI, ...saved.sidebarUI },
    ai: { ...defaults.ai, ...saved.ai },
    localTerminal: saved.localTerminal
      ? { ...defaults.localTerminal!, ...saved.localTerminal }
      : defaults.localTerminal,
    sftp: saved.sftp
      ? { ...defaults.sftp!, ...saved.sftp }
      : defaults.sftp,
  };
}

/** Migrate AI settings to multi-provider format */
function migrateAiProviders(settings: PersistedSettingsV2): PersistedSettingsV2 {
  const ai = settings.ai;

  // Already migrated
  if (ai.providers && ai.providers.length > 0) {
    return settings;
  }

  console.log('[SettingsStore] Migrating AI settings to multi-provider format');

  const providers: import('../types').AiProvider[] = DEFAULT_PROVIDERS.map(
    (cfg) => ({
      id: `builtin-${cfg.type}`,
      type: cfg.type,
      name: cfg.name,
      baseUrl: cfg.baseUrl,
      defaultModel: cfg.defaultModel,
      models: cfg.models,
      enabled: true,
      createdAt: Date.now(),
    })
  );

  // If user had a custom baseUrl, create an openai_compatible provider for it
  const defaultOpenAiUrl = 'https://api.openai.com/v1';
  if (ai.baseUrl && ai.baseUrl !== defaultOpenAiUrl) {
    const customProvider: import('../types').AiProvider = {
      id: `custom-migrated-${Date.now()}`,
      type: 'openai_compatible',
      name: 'Custom (Migrated)',
      baseUrl: ai.baseUrl,
      defaultModel: ai.model || 'gpt-4o-mini',
      models: [ai.model || 'gpt-4o-mini'],
      enabled: true,
      createdAt: Date.now(),
    };
    providers.unshift(customProvider);
  }

  // Set active provider: if user had custom URL, use that; otherwise OpenAI
  const activeProviderId = ai.baseUrl && ai.baseUrl !== defaultOpenAiUrl
    ? providers[0].id
    : 'builtin-openai';

  const newSettings: PersistedSettingsV2 = {
    ...settings,
    ai: {
      ...ai,
      providers,
      activeProviderId,
      activeModel: ai.model || 'gpt-4o-mini',
    },
  };

  persistSettings(newSettings);
  return newSettings;
}

/** Load settings from localStorage, detect and clean legacy formats */
function loadSettings(): PersistedSettingsV2 {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed.version === SETTINGS_VERSION) {
        // Valid v2 format, merge with defaults for any new fields
        const settings = mergeWithDefaults(parsed);
        // Migrate: ensure providers array exists
        return migrateAiProviders(settings);
      }
    }

    // Check for legacy formats and clean them up
    const hasLegacy = LEGACY_KEYS.some(key => localStorage.getItem(key) !== null);
    if (hasLegacy) {
      console.warn('[SettingsStore] Detected legacy settings format. Clearing and using defaults.');
      LEGACY_KEYS.forEach(key => localStorage.removeItem(key));
    }
  } catch (e) {
    console.error('[SettingsStore] Failed to load settings:', e);
  }

  const defaults = createDefaultSettings();
  return migrateAiProviders(defaults);
}

/** Persist settings to localStorage */
function persistSettings(settings: PersistedSettingsV2): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch (e) {
    console.error('[SettingsStore] Failed to persist settings:', e);
  }
}

// ============================================================================
// Store Interface
// ============================================================================

interface SettingsStore {
  // State
  settings: PersistedSettingsV2;

  // Actions - Category updates
  updateGeneral: <K extends keyof GeneralSettings>(key: K, value: GeneralSettings[K]) => void;
  updateTerminal: <K extends keyof TerminalSettings>(key: K, value: TerminalSettings[K]) => void;
  updateBuffer: <K extends keyof BufferSettings>(key: K, value: BufferSettings[K]) => void;
  updateAppearance: <K extends keyof AppearanceSettings>(key: K, value: AppearanceSettings[K]) => void;
  updateConnectionDefaults: <K extends keyof ConnectionDefaults>(key: K, value: ConnectionDefaults[K]) => void;
  updateAi: <K extends keyof AiSettings>(key: K, value: AiSettings[K]) => void;
  // Provider management
  addProvider: (provider: import('../types').AiProvider) => void;
  removeProvider: (providerId: string) => void;
  updateProvider: (providerId: string, updates: Partial<import('../types').AiProvider>) => void;
  setActiveProvider: (providerId: string, model?: string) => void;
  refreshProviderModels: (providerId: string) => Promise<string[]>;
  updateLocalTerminal: <K extends keyof LocalTerminalSettings>(key: K, value: LocalTerminalSettings[K]) => void;
  updateSftp: <K extends keyof SftpSettings>(key: K, value: SftpSettings[K]) => void;

  // Actions - Dedicated language setter with i18n sync
  setLanguage: (language: Language) => void;

  // Actions - Tree UI state
  setTreeExpanded: (ids: string[]) => void;
  toggleTreeNode: (nodeId: string) => void;
  setFocusedNode: (nodeId: string | null) => void;

  // Actions - Sidebar UI state
  setSidebarCollapsed: (collapsed: boolean) => void;
  setSidebarSection: (section: SidebarSection) => void;
  setSidebarWidth: (width: number) => void;
  toggleSidebar: () => void;
  // AI sidebar actions
  setAiSidebarCollapsed: (collapsed: boolean) => void;
  setAiSidebarWidth: (width: number) => void;
  toggleAiSidebar: () => void;

  // Actions - Bulk operations
  resetToDefaults: () => void;

  // Selectors (convenience getters)
  getTerminal: () => TerminalSettings;
  getBuffer: () => BufferSettings;
  getTreeUI: () => TreeUIState;
  getSidebarUI: () => SidebarUIState;
  getAi: () => AiSettings;
  getSftp: () => SftpSettings;
}

// ============================================================================
// Store Implementation
// ============================================================================

export const useSettingsStore = create<SettingsStore>()(
  subscribeWithSelector((set, get) => ({
    settings: loadSettings(),

    // ========== General Settings ==========
    updateGeneral: (key, value) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          general: { ...state.settings.general, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // Language setter with i18n synchronization
    setLanguage: async (language) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          general: { ...state.settings.general, language },
        };
        persistSettings(newSettings);

        // Sync with localStorage for i18n initialization
        localStorage.setItem('app_lang', language);

        return { settings: newSettings };
      });

      // Dynamically import i18n to avoid circular dependency
      const { default: i18n } = await import('../i18n');
      await i18n.changeLanguage(language);
    },

    // ========== Terminal Settings ==========
    updateTerminal: (key, value) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          terminal: { ...state.settings.terminal, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // ========== Buffer Settings ==========
    updateBuffer: (key, value) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          buffer: { ...state.settings.buffer, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // ========== Appearance Settings ==========
    updateAppearance: (key, value) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          appearance: { ...state.settings.appearance, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // ========== Connection Defaults ==========
    updateConnectionDefaults: (key, value) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          connectionDefaults: { ...state.settings.connectionDefaults, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // ========== Local Terminal Settings ==========
    updateLocalTerminal: (key, value) => {
      set((state) => {
        const currentLocalTerminal = state.settings.localTerminal || defaultLocalTerminalSettings;
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          localTerminal: { ...currentLocalTerminal, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // ========== SFTP Settings ==========
    updateSftp: (key, value) => {
      set((state) => {
        const currentSftp = state.settings.sftp || defaultSftpSettings;
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          sftp: { ...currentSftp, [key]: value },
        };
        persistSettings(newSettings);

        // Sync to backend for transfer manager settings
        if (key === 'maxConcurrentTransfers' || key === 'speedLimitEnabled' || key === 'speedLimitKBps') {
          const sftp = newSettings.sftp!;
          // Dynamically import api to avoid circular dependencies
          import('../lib/api').then(({ api }) => {
            api.sftpUpdateSettings(
              sftp.maxConcurrentTransfers,
              sftp.speedLimitEnabled ? sftp.speedLimitKBps : 0
            ).catch((err) => console.error('Failed to sync SFTP settings to backend:', err));
          });
        }

        return { settings: newSettings };
      });
    },

    // ========== Tree UI State ==========
    setTreeExpanded: (ids) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          treeUI: { ...state.settings.treeUI, expandedIds: ids },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    toggleTreeNode: (nodeId) => {
      set((state) => {
        const current = new Set(state.settings.treeUI.expandedIds);
        if (current.has(nodeId)) {
          current.delete(nodeId);
        } else {
          current.add(nodeId);
        }
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          treeUI: { ...state.settings.treeUI, expandedIds: [...current] },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    setFocusedNode: (nodeId) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          treeUI: { ...state.settings.treeUI, focusedNodeId: nodeId },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // ========== Sidebar UI State ==========
    setSidebarCollapsed: (collapsed) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          sidebarUI: { ...state.settings.sidebarUI, collapsed },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    setSidebarSection: (section) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          sidebarUI: { ...state.settings.sidebarUI, activeSection: section },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    toggleSidebar: () => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          sidebarUI: {
            ...state.settings.sidebarUI,
            collapsed: !state.settings.sidebarUI.collapsed
          },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    setSidebarWidth: (width) => {
      // Clamp width between 200 and 600
      const clampedWidth = Math.max(200, Math.min(600, width));
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          sidebarUI: { ...state.settings.sidebarUI, width: clampedWidth },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // ========== AI Sidebar UI State ==========
    setAiSidebarCollapsed: (collapsed) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          sidebarUI: { ...state.settings.sidebarUI, aiSidebarCollapsed: collapsed },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    setAiSidebarWidth: (width) => {
      // Clamp width between 280 and 500
      const clampedWidth = Math.max(280, Math.min(500, width));
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          sidebarUI: { ...state.settings.sidebarUI, aiSidebarWidth: clampedWidth },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    toggleAiSidebar: () => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          sidebarUI: {
            ...state.settings.sidebarUI,
            aiSidebarCollapsed: !state.settings.sidebarUI.aiSidebarCollapsed
          },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // ========== AI Settings ==========
    updateAi: (key, value) => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          ai: { ...state.settings.ai, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    addProvider: (provider) => {
      set((state) => {
        const ai = state.settings.ai;
        const newProviders = [...ai.providers, provider];
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          ai: {
            ...ai,
            providers: newProviders,
            // Auto-activate if first provider
            activeProviderId: ai.activeProviderId || provider.id,
            activeModel: ai.activeModel || provider.defaultModel,
          },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    removeProvider: (providerId) => {
      set((state) => {
        const ai = state.settings.ai;
        const newProviders = ai.providers.filter(p => p.id !== providerId);
        const needsNewActive = ai.activeProviderId === providerId;
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          ai: {
            ...ai,
            providers: newProviders,
            activeProviderId: needsNewActive ? (newProviders[0]?.id ?? null) : ai.activeProviderId,
            activeModel: needsNewActive ? (newProviders[0]?.defaultModel ?? null) : ai.activeModel,
          },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    updateProvider: (providerId, updates) => {
      set((state) => {
        const ai = state.settings.ai;
        const newProviders = ai.providers.map(p =>
          p.id === providerId ? { ...p, ...updates } : p
        );
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          ai: { ...ai, providers: newProviders },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    setActiveProvider: (providerId, model) => {
      set((state) => {
        const ai = state.settings.ai;
        const provider = ai.providers.find(p => p.id === providerId);
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          ai: {
            ...ai,
            activeProviderId: providerId,
            activeModel: model || provider?.defaultModel || ai.activeModel,
          },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    refreshProviderModels: async (providerId) => {
      const state = get();
      const provider = state.settings.ai.providers.find(p => p.id === providerId);
      if (!provider) throw new Error(`Provider ${providerId} not found`);

      const { getProvider } = await import('../lib/ai/providerRegistry');
      const impl = getProvider(provider.type);
      if (!impl.fetchModels) {
        throw new Error(`Provider ${provider.type} does not support model listing`);
      }

      // Resolve API key (provider-specific only)
      const { api } = await import('../lib/api');
      let apiKey = '';
      if (provider.type !== 'ollama') {
        try { apiKey = await api.getAiProviderApiKey(providerId) || ''; } catch { /* */ }
        if (!apiKey) {
          throw new Error('API key not found for provider');
        }
      }

      const models = await impl.fetchModels({ baseUrl: provider.baseUrl, apiKey });

      // Update store
      set((s) => {
        const ai = s.settings.ai;
        const updatedProviders = ai.providers.map(p =>
          p.id === providerId ? { ...p, models } : p
        );
        const newSettings: PersistedSettingsV2 = {
          ...s.settings,
          ai: { ...ai, providers: updatedProviders },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });

      return models;
    },

    // ========== Bulk Operations ==========
    resetToDefaults: () => {
      const newSettings = createDefaultSettings();
      persistSettings(newSettings);
      set({ settings: newSettings });
    },

    // ========== Selectors ==========
    getTerminal: () => get().settings.terminal,
    getBuffer: () => get().settings.buffer,
    getTreeUI: () => get().settings.treeUI,
    getSidebarUI: () => get().settings.sidebarUI,
    getAi: () => get().settings.ai,
    getSftp: () => get().settings.sftp || defaultSftpSettings,
  }))
);

// ============================================================================
// Event Subscriptions (Side Effects)
// ============================================================================

// Track previous renderer for Toast notification
let previousRenderer: RendererType | null = null;

// Subscribe to theme changes - apply to document
useSettingsStore.subscribe(
  (state) => state.settings.terminal.theme,
  (themeName) => {
    // Validate theme exists
    if (!themes[themeName]) {
      console.warn(`[SettingsStore] Theme "${themeName}" not found, falling back to default`);
      themeName = 'default';
    }

    // Set data-theme attribute for CSS variables
    document.documentElement.setAttribute('data-theme', themeName);

    // Dispatch event for terminal components to update their xterm instances
    window.dispatchEvent(
      new CustomEvent('global-theme-changed', {
        detail: {
          themeName,
          xtermTheme: themes[themeName],
        },
      })
    );
  }
);

// Subscribe to font family changes - update CSS variable globally
useSettingsStore.subscribe(
  (state) => ({
    fontFamily: state.settings.terminal.fontFamily,
    customFontFamily: state.settings.terminal.customFontFamily,
  }),
  ({ fontFamily, customFontFamily }) => {
    const fontCSS = fontFamily === 'custom' && customFontFamily
      ? customFontFamily
      : getFontFamilyCSS(fontFamily);
    document.documentElement.style.setProperty('--terminal-font-family', fontCSS);
  },
  { equalityFn: (a, b) => a.fontFamily === b.fontFamily && a.customFontFamily === b.customFontFamily }
);

// Subscribe to renderer changes - show Toast notification
useSettingsStore.subscribe(
  (state) => state.settings.terminal.renderer,
  (renderer) => {
    if (previousRenderer !== null && previousRenderer !== renderer) {
      // Show Toast notification for renderer change
      useToastStore.getState().addToast({
        variant: 'default',
        title: i18n.t('settings.toast.renderer_changed'),
        description: i18n.t('settings.toast.renderer_changed_desc', {
          renderer: i18n.t(`settings.sections.terminal.renderer_${renderer}`)
        }),
        duration: 5000,
      });

      console.debug(`[SettingsStore] Renderer changed: ${previousRenderer} -> ${renderer}`);
    }
    previousRenderer = renderer;
  }
);

// ============================================================================
// Initialization
// ============================================================================

/**
 * Initialize settings on app startup.
 * Call this once in main.tsx or App.tsx.
 */
export function initializeSettings(): void {
  const { settings } = useSettingsStore.getState();

  // Apply theme immediately
  const themeName = themes[settings.terminal.theme] ? settings.terminal.theme : 'default';
  document.documentElement.setAttribute('data-theme', themeName);

  // Apply terminal font CSS variable globally
  const { fontFamily, customFontFamily } = settings.terminal;
  const fontCSS = fontFamily === 'custom' && customFontFamily
    ? customFontFamily
    : getFontFamilyCSS(fontFamily);
  document.documentElement.style.setProperty('--terminal-font-family', fontCSS);

  // Initialize previousRenderer for Toast tracking
  previousRenderer = settings.terminal.renderer;

  console.debug('[SettingsStore] Initialized with settings:', {
    theme: settings.terminal.theme,
    renderer: settings.terminal.renderer,
    sidebarCollapsed: settings.sidebarUI.collapsed,
  });
}

// ============================================================================
// Exports for External Use
// ============================================================================

export { createDefaultSettings, STORAGE_KEY, LEGACY_KEYS };
export type { SettingsStore };
