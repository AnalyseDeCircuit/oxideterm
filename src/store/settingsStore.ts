// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

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
import { api } from '../lib/api';
import { themes, getTerminalTheme, isCustomTheme, applyCustomThemeCSS, clearCustomThemeCSS } from '../lib/themes';
import { useToastStore } from '../hooks/useToast';
import { getFontFamilyCSS } from '../components/fileManager/fontUtils';
import i18n from '../i18n';
import { DEFAULT_PROVIDERS } from '../lib/ai/providers';
import { platform } from '../lib/platform';
import { sanitizeHighlightRules } from '../lib/terminal/highlightPattern';
import type { HighlightRule } from '../types';
import type { AiReasoningEffort } from '../lib/ai/providers';

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

const DEFAULT_TERMINAL_SCROLLBACK = 3000;
const TERMINAL_SCROLLBACK_MIN = 500;
const TERMINAL_SCROLLBACK_MAX = 20_000;
const BACKEND_HOT_BUFFER_MIN = 5_000;
const BACKEND_HOT_BUFFER_MAX = 12_000;
const IN_BAND_TRANSFER_CHUNK_MIN = 64 * 1024;
const IN_BAND_TRANSFER_CHUNK_MAX = 8 * 1024 * 1024;
const IN_BAND_TRANSFER_FILE_COUNT_MIN = 1;
const IN_BAND_TRANSFER_FILE_COUNT_MAX = 10_000;
const IN_BAND_TRANSFER_TOTAL_BYTES_MIN = 100 * 1024 * 1024;
const IN_BAND_TRANSFER_TOTAL_BYTES_MAX = 100 * 1024 * 1024 * 1024;

function clampTerminalScrollback(scrollback: number): number {
  if (!Number.isFinite(scrollback)) {
    return DEFAULT_TERMINAL_SCROLLBACK;
  }
  return Math.min(
    TERMINAL_SCROLLBACK_MAX,
    Math.max(TERMINAL_SCROLLBACK_MIN, Math.round(scrollback)),
  );
}

export function deriveBackendHotLines(scrollback: number): number {
  const normalizedScrollback = clampTerminalScrollback(scrollback);
  return Math.min(
    BACKEND_HOT_BUFFER_MAX,
    Math.max(BACKEND_HOT_BUFFER_MIN, normalizedScrollback * 2),
  );
}

function clampFiniteInteger(value: unknown, fallback: number, min: number, max: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return fallback;
  }

  return Math.min(max, Math.max(min, Math.round(value)));
}

function normalizeTerminalEncoding(value: unknown): TerminalEncoding {
  if (typeof value !== 'string') return 'utf-8';
  const normalized = value.toLowerCase().replace(/_/g, '-');
  switch (normalized) {
    case 'utf-8':
    case 'gbk':
    case 'gb18030':
    case 'big5':
    case 'shift-jis':
    case 'shift_jis':
    case 'euc-jp':
    case 'euc-kr':
    case 'windows-1252':
      return normalized === 'shift-jis' ? 'shift_jis' : normalized as TerminalEncoding;
    default:
      return 'utf-8';
  }
}

// ============================================================================
// Types
// ============================================================================

/** Renderer type */
export type RendererType = 'auto' | 'webgl' | 'canvas';

/** Adaptive renderer mode (Dynamic Refresh Rate) */
export type AdaptiveRendererMode = 'auto' | 'always-60' | 'off';

export type TerminalEncoding =
  | 'utf-8'
  | 'gbk'
  | 'gb18030'
  | 'big5'
  | 'shift_jis'
  | 'euc-jp'
  | 'euc-kr'
  | 'windows-1252';

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
export type UpdateChannel = 'stable' | 'beta';

export interface GeneralSettings {
  language: Language;
  updateChannel: UpdateChannel;
}

/** Terminal background image fit mode */
export type BackgroundFit = 'cover' | 'contain' | 'fill' | 'tile';

export type InBandTransferProvider = 'trzsz';

export interface InBandTransferSettings {
  enabled: boolean;
  provider: InBandTransferProvider;
  allowDirectory: boolean;
  maxChunkBytes: number;
  maxFileCount: number;
  maxTotalBytes: number;
}

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
  terminalEncoding: TerminalEncoding;
  adaptiveRenderer: AdaptiveRendererMode; // Dynamic refresh rate: auto/always-60/off
  showFpsOverlay: boolean;               // Show FPS/tier debug overlay on terminal
  pasteProtection: boolean; // Confirm before pasting multi-line content
  smartCopy: boolean; // Ctrl+C copies selection on Windows/Linux, otherwise passes SIGINT
  osc52Clipboard: boolean;  // Allow remote programs to write system clipboard via OSC 52
  copyOnSelect: boolean; // Copy terminal selection to the system clipboard when it stabilizes
  middleClickPaste: boolean; // Paste clipboard contents on middle-click when mouse tracking is inactive
  selectionRequiresShift: boolean; // Require Shift + drag before starting text selection
  // Background image settings
  backgroundEnabled: boolean;        // Master toggle — false = no bg image anywhere
  backgroundImage: string | null;    // Stored image path (app_data_dir/backgrounds/...)
  backgroundOpacity: number;         // Image opacity 0.03-0.5 (default 0.15)
  backgroundBlur: number;            // Blur in px 0-20 (default 0)
  backgroundFit: BackgroundFit;      // How the image fills the terminal area
  backgroundEnabledTabs: string[];   // Which tab types show the background image
  highlightRules: HighlightRule[];
  inBandTransfer: InBandTransferSettings;
}

/** Buffer settings (used by backend) */
export interface BufferSettings {
  maxLines: number;          // Legacy persisted mirror of derived backend hot-buffer lines
}

/** UI density control */
export type UiDensity = 'compact' | 'comfortable' | 'spacious';

/** Animation speed control */
export type AnimationSpeed = 'off' | 'reduced' | 'normal' | 'fast';

/** Frosted glass mode */
export type FrostedGlassMode = 'off' | 'css' | 'native';

/** Appearance settings */
export interface AppearanceSettings {
  sidebarCollapsedDefault: boolean;
  uiDensity: UiDensity;              // UI spacing density
  borderRadius: number;               // Global border-radius base (0-16 px)
  uiFontFamily: string;               // Custom UI font family (empty = system default)
  animationSpeed: AnimationSpeed;     // Animation speed multiplier
  frostedGlass: FrostedGlassMode;     // Frosted glass effect mode
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
  // Zen mode
  zenMode: boolean;
}

/** AI thinking display style */
export type AiThinkingStyle = 'detailed' | 'compact';

/** AI context source */
export type AiContextSource = 'selection' | 'visible' | 'command';

export interface AiMemorySettings {
  enabled: boolean;
  content: string;
}

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
  /** Request-level reasoning/thinking effort. Provider adapters map or ignore unsupported values. */
  reasoningEffort: AiReasoningEffort;
  /** Per-provider reasoning/thinking overrides. Missing value inherits `reasoningEffort`. */
  reasoningProviderOverrides?: Record<string, AiReasoningEffort>;
  /** Per-model reasoning/thinking overrides. Shape: { [providerId]: { [modelId]: effort } } */
  reasoningModelOverrides?: Record<string, Record<string, AiReasoningEffort>>;
  /** Whether thinking blocks are expanded by default */
  thinkingDefaultExpanded: boolean;
  /** Cached model context window sizes from provider APIs.
   * Scoped by provider id to prevent collisions when two providers share model names.
   * Shape: { [providerId]: { [modelId]: tokenCount } }
   */
  modelContextWindows?: Record<string, Record<string, number>>;
  /** User-configured context window overrides per model.
   * Takes highest priority over API cache and built-in patterns.
   * Shape: { [providerId]: { [modelId]: tokenCount } }
   */
  userContextWindows?: Record<string, Record<string, number>>;
  /** Custom system prompt override (empty = use default) */
  customSystemPrompt?: string;
  /** Long-lived user preferences explicitly saved by the user */
  memory?: AiMemorySettings;
  /**
   * Per-model maximum response tokens override.
   * Shape: { [providerId]: { [modelId]: tokenCount } }
   * If set, overrides the dynamic `responseReserve()` calculation.
   */
  modelMaxResponseTokens?: Record<string, Record<string, number>>;
  /** Tool use (function calling) settings */
  toolUse?: {
    /** Master switch for tool use — default false */
    enabled: boolean;
    /**
     * Per-tool auto-approve map.
     * Key = tool name, value = true means auto-approve (no confirmation).
     * Tools not in the map require manual approval.
     */
    autoApproveTools: Record<string, boolean>;
    /**
     * Globally disabled tools — these are never sent to the LLM.
     * Orthogonal to autoApproveTools (which controls auto-approval).
     */
    disabledTools: string[];
  };
  /** Context sources to auto-inject into AI system prompt */
  contextSources?: {
    /** Include IDE editor context (active file, language, cursor, code snippet) */
    ide: boolean;
    /** Include SFTP file browser context (CWD, selected files) */
    sftp: boolean;
  };
  /** Configured MCP servers */
  mcpServers?: import('../lib/ai/mcp/mcpTypes').McpServerConfig[];
  /** Global embedding provider/model (separate from chat provider) */
  embeddingConfig?: import('../types').EmbeddingConfig;
  /** Agent role configuration (planner/reviewer can use different provider/model) */
  agentRoles?: import('../types').AgentRolesConfig;
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
  directoryParallelism: number;     // Parallel file workers inside recursive directory transfers (1-16)
  speedLimitEnabled: boolean;      // Enable bandwidth limiting
  speedLimitKBps: number;          // Speed limit in KB/s (0 = unlimited)
  conflictAction: 'ask' | 'overwrite' | 'skip' | 'rename';  // Default conflict resolution
}

export interface IdeSettings {
  autoSave: boolean;  // Auto-save dirty tabs on tab switch / window blur
  fontSize: number | null;    // null = follow terminal setting (8-32)
  lineHeight: number | null;  // null = follow terminal setting (0.8-3.0)
  agentMode: 'ask' | 'enabled' | 'disabled';  // Remote agent deployment policy
  wordWrap: boolean;  // Enable word wrapping for long lines
}

/** Auto-reconnect strategy settings */
export interface ReconnectSettings {
  enabled: boolean;              // Master toggle for auto-reconnect
  maxAttempts: number;           // Max retry attempts (1-20)
  baseDelayMs: number;           // Base retry delay in ms (500-10000)
  maxDelayMs: number;            // Max retry delay cap in ms (5000-60000)
}

export interface ConnectionPoolSettings {
  idleTimeoutSecs: number;
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
  ide?: IdeSettings;
  reconnect?: ReconnectSettings;
  connectionPool?: ConnectionPoolSettings;
  experimental?: ExperimentalSettings;
  /** Whether the first-run onboarding wizard has been completed or dismissed */
  onboardingCompleted?: boolean;
  /** Command palette MRU — most recently used command IDs (max 20) */
  commandPaletteMru?: string[];
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

const isWindows = platform.isWindows;

// ============================================================================
// Default Values
// ============================================================================

const defaultGeneralSettings: GeneralSettings = {
  language: 'zh-CN',  // Default to Chinese
  updateChannel: 'stable',
};

const defaultTerminalSettings: TerminalSettings = {
  theme: 'default',
  fontFamily: 'jetbrains',
  customFontFamily: '',  // 自定义轨道为空时不生效
  fontSize: 14,
  lineHeight: 1.2,
  cursorStyle: 'block',
  cursorBlink: true,
  scrollback: DEFAULT_TERMINAL_SCROLLBACK,
  renderer: isWindows ? 'canvas' : 'auto',
  terminalEncoding: 'utf-8',
  adaptiveRenderer: 'auto',  // Dynamic refresh rate: auto = three-tier adaptive
  showFpsOverlay: false,      // Hidden by default; user enables for diagnostics
  pasteProtection: true,  // Default enabled for safety
  smartCopy: true,
  osc52Clipboard: true,  // Default enabled: allow remote programs to write system clipboard via OSC 52
  copyOnSelect: false,
  middleClickPaste: false,
  selectionRequiresShift: false,
  // Background image defaults
  backgroundEnabled: true,
  backgroundImage: null,
  backgroundOpacity: 0.15,
  backgroundBlur: 0,
  backgroundFit: 'cover',
  backgroundEnabledTabs: ['terminal', 'local_terminal'],
  highlightRules: [],
  inBandTransfer: {
    enabled: false,
    provider: 'trzsz',
    allowDirectory: true,
    maxChunkBytes: 1024 * 1024,
    maxFileCount: 1024,
    maxTotalBytes: 10 * 1024 * 1024 * 1024,
  },
};

const defaultBufferSettings: BufferSettings = {
  maxLines: deriveBackendHotLines(DEFAULT_TERMINAL_SCROLLBACK),
};

const defaultAppearanceSettings: AppearanceSettings = {
  sidebarCollapsedDefault: false,
  uiDensity: 'comfortable',
  borderRadius: 6,
  uiFontFamily: '',
  animationSpeed: 'normal',
  frostedGlass: 'off',
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
  // Zen mode
  zenMode: false,
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
  reasoningEffort: 'auto',
  reasoningProviderOverrides: {},
  reasoningModelOverrides: {},
  thinkingDefaultExpanded: false,    // Default: collapsed for less noise
  customSystemPrompt: '',            // Default: use built-in prompt
  memory: {
    enabled: true,
    content: '',
  },
  toolUse: {
    enabled: false,                  // Default: disabled until user opts in
    autoApproveTools: {
      // Read-only tools: auto-approve by default
      read_file: true,
      list_directory: true,
      grep_search: true,
      git_status: true,
      list_tabs: true,
      list_sessions: true,
      get_terminal_buffer: true,
      search_terminal: true,
      await_terminal_output: true,
      list_connections: true,
      list_port_forwards: true,
      get_detected_ports: true,
      get_connection_health: true,
      sftp_list_dir: true,
      sftp_read_file: true,
      sftp_stat: true,
      sftp_get_cwd: true,
      ide_get_open_files: true,
      ide_get_file_content: true,
      ide_get_project_info: true,
      // Local terminal (read-only)
      local_list_shells: true,
      local_get_terminal_info: true,
      local_get_drives: true,
      // Settings (read-only)
      get_settings: true,
      // Connection pool (read-only)
      get_pool_stats: true,
      // Connection monitor (read-only)
      get_all_health: true,
      get_resource_metrics: true,
      // Session manager (all read-only)
      list_saved_connections: true,
      search_saved_connections: true,
      get_session_tree: true,
      // Plugin manager (read-only)
      list_plugins: true,
      // Write tools: require approval by default
      terminal_exec: false,
      write_file: false,
      create_port_forward: false,
      stop_port_forward: false,
      // IDE (write)
      ide_replace_string: false,
      ide_insert_text: false,
      ide_open_file: false,
      ide_create_file: false,
      // SFTP (write)
      sftp_write_file: false,
      // Local terminal (write)
      local_exec: false,
      // Settings (write)
      update_setting: false,
      // Connection pool (write)
      set_pool_config: false,
      // Meta tools
      send_control_sequence: false,
      batch_exec: false,
      // Session manager (write)
      connect_saved_session: false,
      // TUI interaction (experimental — disabled by default)
      read_screen: false,
      send_keys: false,
      send_mouse: false,
    },
    disabledTools: [],
  },
  contextSources: {
    ide: true,
    sftp: true,
  },
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
  directoryParallelism: 4,
  speedLimitEnabled: false,
  speedLimitKBps: 0,
  conflictAction: 'ask',
};

const defaultIdeSettings: IdeSettings = {
  autoSave: false,
  fontSize: null,
  lineHeight: null,
  agentMode: 'ask',
  wordWrap: false,
};

const defaultReconnectSettings: ReconnectSettings = {
  enabled: true,
  maxAttempts: 5,
  baseDelayMs: 1000,
  maxDelayMs: 15000,
};

const defaultConnectionPoolSettings: ConnectionPoolSettings = {
  idleTimeoutSecs: 1800,
};

function syncConnectionPoolToBackend(connectionPool: ConnectionPoolSettings): void {
  import('../lib/api').then(({ api }) => {
    api.sshGetPoolConfig()
      .then((current) => api.sshSetPoolConfig({
        ...current,
        idleTimeoutSecs: connectionPool.idleTimeoutSecs,
      }))
      .catch((err) => {
        console.error('Failed to sync connection pool settings to backend:', err);
      });
  });
}

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
    ide: { ...defaultIdeSettings },
    reconnect: { ...defaultReconnectSettings },
    connectionPool: { ...defaultConnectionPoolSettings },
    experimental: { virtualSessionProxy: false },
    onboardingCompleted: false,
  };
}

function normalizeTerminalSettings(settings: TerminalSettings): TerminalSettings {
  const inBandTransfer = settings.inBandTransfer;
  return {
    ...settings,
    scrollback: clampTerminalScrollback(settings.scrollback),
    terminalEncoding: normalizeTerminalEncoding(settings.terminalEncoding),
    highlightRules: sanitizeHighlightRules(settings.highlightRules),
    inBandTransfer: {
      enabled: inBandTransfer?.enabled === true,
      provider: 'trzsz',
      allowDirectory: inBandTransfer?.allowDirectory !== false,
      maxChunkBytes: clampFiniteInteger(
        inBandTransfer?.maxChunkBytes,
        defaultTerminalSettings.inBandTransfer.maxChunkBytes,
        IN_BAND_TRANSFER_CHUNK_MIN,
        IN_BAND_TRANSFER_CHUNK_MAX,
      ),
      maxFileCount: clampFiniteInteger(
        inBandTransfer?.maxFileCount,
        defaultTerminalSettings.inBandTransfer.maxFileCount,
        IN_BAND_TRANSFER_FILE_COUNT_MIN,
        IN_BAND_TRANSFER_FILE_COUNT_MAX,
      ),
      maxTotalBytes: clampFiniteInteger(
        inBandTransfer?.maxTotalBytes,
        defaultTerminalSettings.inBandTransfer.maxTotalBytes,
        IN_BAND_TRANSFER_TOTAL_BYTES_MIN,
        IN_BAND_TRANSFER_TOTAL_BYTES_MAX,
      ),
    },
  };
}

function areInBandTransferSettingsEqual(
  a: InBandTransferSettings | undefined,
  b: InBandTransferSettings | undefined,
): boolean {
  return a?.enabled === b?.enabled
    && a?.provider === b?.provider
    && a?.allowDirectory === b?.allowDirectory
    && a?.maxChunkBytes === b?.maxChunkBytes
    && a?.maxFileCount === b?.maxFileCount
    && a?.maxTotalBytes === b?.maxTotalBytes;
}

function normalizeHistorySettings(settings: PersistedSettingsV2): PersistedSettingsV2 {
  const terminal = normalizeTerminalSettings(settings.terminal);
  const scrollback = terminal.scrollback;
  return {
    ...settings,
    terminal,
    buffer: {
      ...settings.buffer,
      maxLines: deriveBackendHotLines(scrollback),
    },
  };
}

// ============================================================================
// Persistence Helpers
// ============================================================================

/** Merge saved settings with defaults (handles version upgrades with new fields) */
function mergeWithDefaults(saved: OxidePartialSettingsSnapshot | Partial<PersistedSettingsV2>): PersistedSettingsV2 {
  const defaults = createDefaultSettings();
  return normalizeHistorySettings({
    version: 2,
    general: { ...defaults.general, ...saved.general },
    terminal: { ...defaults.terminal, ...saved.terminal },
    buffer: { ...defaults.buffer, ...saved.buffer },
    appearance: { ...defaults.appearance, ...saved.appearance },
    connectionDefaults: { ...defaults.connectionDefaults, ...saved.connectionDefaults },
    treeUI: { ...defaults.treeUI, ...saved.treeUI },
    sidebarUI: { ...defaults.sidebarUI, ...saved.sidebarUI },
    ai: {
      ...defaults.ai,
      ...saved.ai,
      // Deep merge toolUse.autoApproveTools so new tools get defaults
      memory: {
        enabled: saved.ai?.memory?.enabled ?? defaults.ai.memory?.enabled ?? true,
        content: saved.ai?.memory?.content ?? defaults.ai.memory?.content ?? '',
      },
      toolUse: saved.ai?.toolUse
        ? {
            ...defaults.ai.toolUse,
            ...saved.ai.toolUse,
            autoApproveTools: {
              ...defaults.ai.toolUse?.autoApproveTools,
              ...saved.ai.toolUse.autoApproveTools,
            },
            disabledTools: saved.ai.toolUse.disabledTools ?? [],
          }
        : defaults.ai.toolUse,
    },
    localTerminal: saved.localTerminal
      ? { ...defaults.localTerminal!, ...saved.localTerminal }
      : defaults.localTerminal,
    sftp: saved.sftp
      ? { ...defaults.sftp!, ...saved.sftp }
      : defaults.sftp,
    ide: saved.ide
      ? { ...defaults.ide!, ...saved.ide }
      : defaults.ide,
    reconnect: saved.reconnect
      ? { ...defaults.reconnect!, ...saved.reconnect }
      : defaults.reconnect,
    connectionPool: saved.connectionPool
      ? { ...defaults.connectionPool!, ...saved.connectionPool }
      : defaults.connectionPool,
    experimental: saved.experimental
      ? { ...defaults.experimental, ...saved.experimental }
      : defaults.experimental,
    onboardingCompleted: saved.onboardingCompleted ?? defaults.onboardingCompleted,
    commandPaletteMru: saved.commandPaletteMru ?? defaults.commandPaletteMru,
  });
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
      enabled: cfg.type !== 'ollama',
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

/**
 * Migrate old autoApproveReadOnly/autoApproveAll booleans → per-tool autoApproveTools map.
 * Old format: { enabled, autoApproveReadOnly, autoApproveAll }
 * New format: { enabled, autoApproveTools: Record<string, boolean> }
 */
function migrateToolUseSettings(settings: PersistedSettingsV2): PersistedSettingsV2 {
  const toolUse = settings.ai.toolUse;
  if (!toolUse) return settings;

  // Already migrated: has autoApproveTools
  if ('autoApproveTools' in toolUse && toolUse.autoApproveTools && typeof toolUse.autoApproveTools === 'object') {
    return settings;
  }

  // Old format detected — convert
  const oldReadOnly = (toolUse as Record<string, unknown>).autoApproveReadOnly !== false;
  const oldAll = (toolUse as Record<string, unknown>).autoApproveAll === true;
  const defaults = createDefaultSettings();
  const defaultTools = defaults.ai.toolUse!.autoApproveTools;

  const autoApproveTools: Record<string, boolean> = {};
  for (const [name, defaultVal] of Object.entries(defaultTools)) {
    if (oldAll) {
      autoApproveTools[name] = true;
    } else if (oldReadOnly && defaultVal) {
      autoApproveTools[name] = true;
    } else {
      autoApproveTools[name] = false;
    }
  }

  console.log('[SettingsStore] Migrated toolUse to per-tool approval format');
  const newSettings: PersistedSettingsV2 = {
    ...settings,
    ai: {
      ...settings.ai,
      toolUse: { enabled: toolUse.enabled, autoApproveTools, disabledTools: [] },
    },
  };
  persistSettings(newSettings);
  return newSettings;
}

function syncSftpToBackend(sftp: SftpSettings): void {
  const speedLimit = sftp.speedLimitEnabled ? sftp.speedLimitKBps : 0;
  api.sftpUpdateSettings(
    sftp.maxConcurrentTransfers,
    speedLimit,
    sftp.directoryParallelism,
  ).catch((err) => {
    console.error('Failed to sync SFTP settings to backend:', err);
  });
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
        const migrated = migrateAiProviders(settings);
        // Migrate: convert old autoApproveReadOnly/autoApproveAll to per-tool map
        const migrated2 = migrateToolUseSettings(migrated);
        return normalizeHistorySettings(migrated2);
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
  return normalizeHistorySettings(migrateToolUseSettings(migrateAiProviders(defaults)));
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
  setUserContextWindow: (providerId: string, modelId: string, tokens: number | null) => void;
  setProviderReasoningEffort: (providerId: string, effort: AiReasoningEffort | null) => void;
  setModelReasoningEffort: (providerId: string, modelId: string, effort: AiReasoningEffort | null) => void;
  updateLocalTerminal: <K extends keyof LocalTerminalSettings>(key: K, value: LocalTerminalSettings[K]) => void;
  updateSftp: <K extends keyof SftpSettings>(key: K, value: SftpSettings[K]) => void;
  updateIde: <K extends keyof IdeSettings>(key: K, value: IdeSettings[K]) => void;
  updateReconnect: <K extends keyof ReconnectSettings>(key: K, value: ReconnectSettings[K]) => void;
  updateConnectionPool: <K extends keyof ConnectionPoolSettings>(key: K, value: ConnectionPoolSettings[K]) => void;

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
  toggleAiSidebar: () => boolean;
  // Zen mode
  toggleZenMode: () => void;

  // Onboarding
  completeOnboarding: () => void;
  resetOnboarding: () => void;

  // Command palette MRU
  recordCommandMru: (commandId: string) => void;

  // Actions - Bulk operations
  resetToDefaults: () => void;

  // Selectors (convenience getters)
  getTerminal: () => TerminalSettings;
  getBuffer: () => BufferSettings;
  getTreeUI: () => TreeUIState;
  getSidebarUI: () => SidebarUIState;
  getAi: () => AiSettings;
  getSftp: () => SftpSettings;
  getIde: () => IdeSettings;
  getReconnect: () => ReconnectSettings;
  getConnectionPool: () => ConnectionPoolSettings;
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

      // Dynamically import changeLanguage to avoid circular dependency
      const { changeLanguage } = await import('../i18n');
      await changeLanguage(language);
    },

    // ========== Terminal Settings ==========
    updateTerminal: (key, value) => {
      set((state) => {
        const nextTerminal = { ...state.settings.terminal, [key]: value };
        const normalizedTerminal = normalizeTerminalSettings(nextTerminal);
        if (
          key !== 'inBandTransfer'
          && areInBandTransferSettingsEqual(
            state.settings.terminal.inBandTransfer,
            normalizedTerminal.inBandTransfer,
          )
        ) {
          normalizedTerminal.inBandTransfer = state.settings.terminal.inBandTransfer;
        }
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          terminal: normalizedTerminal,
          buffer: key === 'scrollback'
            ? {
                ...state.settings.buffer,
                maxLines: deriveBackendHotLines(normalizedTerminal.scrollback),
              }
            : state.settings.buffer,
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
      const state = get();
      const currentSftp = state.settings.sftp || defaultSftpSettings;
      const nextSftp = { ...currentSftp, [key]: value };
      const newSettings: PersistedSettingsV2 = {
        ...state.settings,
        sftp: nextSftp,
      };

      persistSettings(newSettings);
      set({ settings: newSettings });

      if (
        key === 'maxConcurrentTransfers' ||
        key === 'directoryParallelism' ||
        key === 'speedLimitEnabled' ||
        key === 'speedLimitKBps'
      ) {
        syncSftpToBackend(nextSftp);
      }
    },

    updateIde: (key, value) => {
      set((state) => {
        const currentIde = state.settings.ide || defaultIdeSettings;
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          ide: { ...currentIde, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // ========== Reconnect Settings ==========
    updateReconnect: (key, value) => {
      set((state) => {
        const currentReconnect = state.settings.reconnect || defaultReconnectSettings;
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          reconnect: { ...currentReconnect, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    updateConnectionPool: (key, value) => {
      set((state) => {
        const currentConnectionPool =
          state.settings.connectionPool || defaultConnectionPoolSettings;
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          connectionPool: { ...currentConnectionPool, [key]: value },
        };
        persistSettings(newSettings);
        syncConnectionPoolToBackend(
          newSettings.connectionPool || defaultConnectionPoolSettings,
        );
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
      const state = get();
      if (!state.settings.ai.enabled) {
        return false;
      }
      set(() => {
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
      return true;
    },

    // ========== Zen Mode ==========
    toggleZenMode: () => {
      set((state) => {
        const sui = state.settings.sidebarUI;
        const entering = !sui.zenMode;
        const newSidebarUI: SidebarUIState = entering
          ? {
              // Enter zen: collapse both sidebars, set zenMode flag
              ...sui,
              zenMode: true,
              collapsed: true,
              aiSidebarCollapsed: true,
            }
          : {
              // Exit zen: restore default open state
              ...sui,
              zenMode: false,
              collapsed: false,
            };
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          sidebarUI: newSidebarUI,
        };
        // Don't persist zen mode — it's a transient UI state
        return { settings: newSettings };
      });
    },

    // ========== Onboarding ==========
    completeOnboarding: () => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          onboardingCompleted: true,
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    resetOnboarding: () => {
      set((state) => {
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          onboardingCompleted: false,
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // ========== Command Palette MRU ==========
    recordCommandMru: (commandId: string) => {
      set((state) => {
        const prev = state.settings.commandPaletteMru ?? [];
        // Move to front, deduplicate, cap at 20
        const next = [commandId, ...prev.filter((id) => id !== commandId)].slice(0, 20);
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          commandPaletteMru: next,
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
            reasoningProviderOverrides: (() => {
              const updated = { ...(ai.reasoningProviderOverrides ?? {}) };
              delete updated[providerId];
              return updated;
            })(),
            reasoningModelOverrides: (() => {
              const updated = { ...(ai.reasoningModelOverrides ?? {}) };
              delete updated[providerId];
              return updated;
            })(),
            userContextWindows: (() => {
              const updated = { ...(ai.userContextWindows ?? {}) };
              delete updated[providerId];
              return updated;
            })(),
            modelMaxResponseTokens: (() => {
              const updated = { ...(ai.modelMaxResponseTokens ?? {}) };
              delete updated[providerId];
              return updated;
            })(),
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
      if (provider.type !== 'ollama' && provider.type !== 'openai_compatible') {
        try { apiKey = await api.getAiProviderApiKey(providerId) || ''; } catch { /* */ }
        if (!apiKey) {
          throw new Error('API key not found for provider');
        }
      } else {
        try { apiKey = await api.getAiProviderApiKey(providerId) || ''; } catch { /* */ }
      }

      const models = await impl.fetchModels({ baseUrl: provider.baseUrl, apiKey });

      // Fetch context window sizes if provider supports it
      let contextWindows: Record<string, number> = {};
      if (impl.fetchModelDetails) {
        try {
          contextWindows = await impl.fetchModelDetails({ baseUrl: provider.baseUrl, apiKey });
        } catch (e) {
          console.warn('[Settings] Failed to fetch model details:', e);
        }
      }

      // Update store — store context windows scoped under providerId to avoid
      // cross-provider collisions when different providers share model names.
      set((s) => {
        const ai = s.settings.ai;
        const updatedProviders = ai.providers.map(p =>
          p.id === providerId ? { ...p, models } : p
        );
        const existingWindows = ai.modelContextWindows ?? {};
        const mergedContextWindows: Record<string, Record<string, number>> = {
          ...existingWindows,
          [providerId]: {
            ...(existingWindows[providerId] ?? {}),
            ...contextWindows,
          },
        };
        const newSettings: PersistedSettingsV2 = {
          ...s.settings,
          ai: { ...ai, providers: updatedProviders, modelContextWindows: mergedContextWindows },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });

      return models;
    },

    setUserContextWindow: (providerId, modelId, tokens) => {
      if (!providerId || !modelId) return;
      set((state) => {
        const ai = state.settings.ai;
        const existing = ai.userContextWindows ?? {};
        const providerOverrides = { ...(existing[providerId] ?? {}) };

        if (tokens !== null && tokens >= 1024 && tokens <= 4_194_304) {
          providerOverrides[modelId] = tokens;
        } else {
          delete providerOverrides[modelId];
        }

        const updated = { ...existing };
        if (Object.keys(providerOverrides).length > 0) {
          updated[providerId] = providerOverrides;
        } else {
          delete updated[providerId];
        }

        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          ai: { ...ai, userContextWindows: updated },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    setProviderReasoningEffort: (providerId, effort) => {
      if (!providerId) return;
      set((state) => {
        const ai = state.settings.ai;
        const updated = { ...(ai.reasoningProviderOverrides ?? {}) };
        if (effort) {
          updated[providerId] = effort;
        } else {
          delete updated[providerId];
        }
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          ai: { ...ai, reasoningProviderOverrides: updated },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    setModelReasoningEffort: (providerId, modelId, effort) => {
      if (!providerId || !modelId) return;
      set((state) => {
        const ai = state.settings.ai;
        const existing = ai.reasoningModelOverrides ?? {};
        const providerOverrides = { ...(existing[providerId] ?? {}) };
        if (effort) {
          providerOverrides[modelId] = effort;
        } else {
          delete providerOverrides[modelId];
        }
        const updated = { ...existing };
        if (Object.keys(providerOverrides).length > 0) {
          updated[providerId] = providerOverrides;
        } else {
          delete updated[providerId];
        }
        const newSettings: PersistedSettingsV2 = {
          ...state.settings,
          ai: { ...ai, reasoningModelOverrides: updated },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },

    // ========== Bulk Operations ==========
    resetToDefaults: () => {
      const newSettings = createDefaultSettings();
      persistSettings(newSettings);
      syncSftpToBackend(newSettings.sftp || defaultSftpSettings);
      syncConnectionPoolToBackend(
        newSettings.connectionPool || defaultConnectionPoolSettings,
      );
      set({ settings: newSettings });
    },

    // ========== Selectors ==========
    getTerminal: () => get().settings.terminal,
    getBuffer: () => get().settings.buffer,
    getTreeUI: () => get().settings.treeUI,
    getSidebarUI: () => get().settings.sidebarUI,
    getAi: () => get().settings.ai,
    getSftp: () => get().settings.sftp || defaultSftpSettings,
    getIde: () => get().settings.ide || defaultIdeSettings,
    getReconnect: () => get().settings.reconnect || defaultReconnectSettings,
    getConnectionPool: () =>
      get().settings.connectionPool || defaultConnectionPoolSettings,
  }))
);

export function exportCurrentSettingsSnapshot(): string | null {
  try {
    return JSON.stringify(useSettingsStore.getState().settings);
  } catch (error) {
    console.error('[SettingsStore] Failed to serialize settings snapshot:', error);
    return null;
  }
}

export const OXIDE_APP_SETTINGS_SECTION_IDS = [
  'general',
  'terminalAppearance',
  'terminalBehavior',
  'appearance',
  'connections',
  'fileAndEditor',
  'ai',
  'localTerminal',
] as const;

export type OxideAppSettingsSectionId = typeof OXIDE_APP_SETTINGS_SECTION_IDS[number];
export type OxideImportedAppSettingsSectionId = OxideAppSettingsSectionId | 'legacy';

type ExportOxideAppSettingsOptions = {
  selectedSections?: OxideAppSettingsSectionId[];
  includeLocalTerminalEnvVars?: boolean;
};

type ApplyImportedSettingsOptions = {
  selectedSections?: string[];
};

type OxidePartialSettingsSnapshot = Omit<
  Partial<PersistedSettingsV2>,
  'general' | 'terminal' | 'appearance' | 'connectionDefaults' | 'localTerminal' | 'sftp' | 'ide' | 'reconnect' | 'connectionPool' | 'ai'
> & {
  general?: Partial<GeneralSettings>;
  terminal?: Partial<TerminalSettings>;
  appearance?: Partial<AppearanceSettings>;
  connectionDefaults?: Partial<ConnectionDefaults>;
  ai?: Partial<AiSettings>;
  localTerminal?: Partial<LocalTerminalSettings>;
  sftp?: Partial<SftpSettings>;
  ide?: Partial<IdeSettings>;
  reconnect?: Partial<ReconnectSettings>;
  connectionPool?: Partial<ConnectionPoolSettings>;
};

type OxideSectionedSettingsEnvelope = {
  format: 'oxide-settings-sections-v1';
  version: 1;
  sectionIds: OxideAppSettingsSectionId[];
  settings: OxidePartialSettingsSnapshot;
};

type ParsedImportedSettingsSnapshot = {
  format: 'legacy' | 'sectioned';
  sectionIds: OxideImportedAppSettingsSectionId[];
  settings: OxidePartialSettingsSnapshot;
};

const OXIDE_APP_SETTINGS_ENVELOPE_FORMAT = 'oxide-settings-sections-v1';
const DEFAULT_OXIDE_APP_SETTINGS_EXPORT_SECTIONS: OxideAppSettingsSectionId[] = [
  'general',
  'terminalAppearance',
  'terminalBehavior',
  'appearance',
  'connections',
  'fileAndEditor',
];

const TERMINAL_APPEARANCE_KEYS: Array<keyof TerminalSettings> = [
  'theme',
  'fontFamily',
  'customFontFamily',
  'fontSize',
  'lineHeight',
  'cursorStyle',
  'cursorBlink',
  'backgroundEnabled',
  'backgroundImage',
  'backgroundOpacity',
  'backgroundBlur',
  'backgroundFit',
  'backgroundEnabledTabs',
];

const TERMINAL_BEHAVIOR_KEYS: Array<keyof TerminalSettings> = [
  'scrollback',
  'renderer',
  'adaptiveRenderer',
  'showFpsOverlay',
  'pasteProtection',
  'smartCopy',
  'osc52Clipboard',
  'copyOnSelect',
  'middleClickPaste',
  'selectionRequiresShift',
  'highlightRules',
  'inBandTransfer',
];

const GENERAL_KEYS: Array<keyof GeneralSettings> = ['language', 'updateChannel'];
const APPEARANCE_KEYS: Array<keyof AppearanceSettings> = ['sidebarCollapsedDefault', 'uiDensity', 'borderRadius', 'uiFontFamily', 'animationSpeed', 'frostedGlass'];
const CONNECTION_DEFAULT_KEYS: Array<keyof ConnectionDefaults> = ['username', 'port'];
const AI_KEYS: Array<keyof AiSettings> = [
  'enabled',
  'enabledConfirmed',
  'baseUrl',
  'model',
  'providers',
  'activeProviderId',
  'activeModel',
  'contextMaxChars',
  'contextVisibleLines',
  'thinkingStyle',
  'reasoningEffort',
  'reasoningProviderOverrides',
  'reasoningModelOverrides',
  'thinkingDefaultExpanded',
  'modelContextWindows',
  'userContextWindows',
  'customSystemPrompt',
  'memory',
  'modelMaxResponseTokens',
  'toolUse',
  'contextSources',
  'mcpServers',
  'embeddingConfig',
  'agentRoles',
];
const RECONNECT_KEYS: Array<keyof ReconnectSettings> = ['enabled', 'maxAttempts', 'baseDelayMs', 'maxDelayMs'];
const CONNECTION_POOL_KEYS: Array<keyof ConnectionPoolSettings> = ['idleTimeoutSecs'];
const SFTP_KEYS: Array<keyof SftpSettings> = [
  'maxConcurrentTransfers',
  'directoryParallelism',
  'speedLimitEnabled',
  'speedLimitKBps',
  'conflictAction',
];
const IDE_KEYS: Array<keyof IdeSettings> = ['autoSave', 'fontSize', 'lineHeight', 'agentMode', 'wordWrap'];
const LOCAL_TERMINAL_KEYS: Array<keyof LocalTerminalSettings> = [
  'defaultShellId',
  'recentShellIds',
  'defaultCwd',
  'loadShellProfile',
  'ohMyPoshEnabled',
  'ohMyPoshTheme',
];

function isOxideAppSettingsSectionId(value: string): value is OxideAppSettingsSectionId {
  return (OXIDE_APP_SETTINGS_SECTION_IDS as readonly string[]).includes(value);
}

function uniqueOxideSectionIds(sectionIds: readonly string[]): OxideAppSettingsSectionId[] {
  return Array.from(new Set(sectionIds.filter(isOxideAppSettingsSectionId)));
}

function pickDefinedFields<T extends object, K extends keyof T>(
  source: Partial<T> | undefined,
  keys: readonly K[],
): Partial<Pick<T, K>> {
  if (!source) {
    return {};
  }

  const entries = keys
    .filter((key) => source[key] !== undefined)
    .map((key) => [key, source[key]] as const);

  return Object.fromEntries(entries) as Partial<Pick<T, K>>;
}

function serializePreviewValue(value: unknown, options?: { envVarNamesOnly?: boolean }): string {
  if (options?.envVarNamesOnly && value && typeof value === 'object' && !Array.isArray(value)) {
    const envVarNames = Object.keys(value as Record<string, string>).sort();
    return envVarNames.length > 0 ? envVarNames.join(', ') : '0';
  }

  return JSON.stringify(value);
}

function buildPreviewValues<T extends object, K extends keyof T>(
  source: Partial<T> | undefined,
  keys: readonly K[],
  prefix?: string,
): Record<string, string> {
  if (!source) {
    return {};
  }

  const preview: Record<string, string> = {};
  for (const key of keys) {
    const value = source[key];
    if (value === undefined) {
      continue;
    }

    const previewKey = prefix ? `${prefix}.${String(key)}` : String(key);
    preview[previewKey] = serializePreviewValue(value);
  }

  return preview;
}

function buildLocalTerminalPreview(source: Partial<LocalTerminalSettings> | undefined): Record<string, string> {
  const preview = buildPreviewValues(source, LOCAL_TERMINAL_KEYS);
  if (source?.customEnvVars !== undefined) {
    preview.customEnvVars = serializePreviewValue(source.customEnvVars, { envVarNamesOnly: true });
  }
  return preview;
}

function buildSectionPreviewValues(
  settings: OxidePartialSettingsSnapshot | Partial<PersistedSettingsV2>,
  sectionId: OxideImportedAppSettingsSectionId,
): Record<string, string> {
  switch (sectionId) {
    case 'general':
      return buildPreviewValues(settings.general, GENERAL_KEYS);
    case 'terminalAppearance':
      return buildPreviewValues(settings.terminal, TERMINAL_APPEARANCE_KEYS);
    case 'terminalBehavior':
      return buildPreviewValues(settings.terminal, TERMINAL_BEHAVIOR_KEYS);
    case 'appearance':
      return buildPreviewValues(settings.appearance, APPEARANCE_KEYS);
    case 'connections':
      return {
        ...buildPreviewValues(settings.connectionDefaults, CONNECTION_DEFAULT_KEYS, 'connectionDefaults'),
        ...buildPreviewValues(settings.reconnect, RECONNECT_KEYS, 'reconnect'),
        ...buildPreviewValues(settings.connectionPool, CONNECTION_POOL_KEYS, 'connectionPool'),
      };
    case 'ai':
      return buildPreviewValues(settings.ai, AI_KEYS, 'ai');
    case 'fileAndEditor':
      return {
        ...buildPreviewValues(settings.sftp, SFTP_KEYS, 'sftp'),
        ...buildPreviewValues(settings.ide, IDE_KEYS, 'ide'),
      };
    case 'localTerminal':
      return buildLocalTerminalPreview(settings.localTerminal);
    case 'legacy':
      return {};
    default:
      return {};
  }
}

export function buildOxideAppSettingsSectionValueMap(
  settings: OxidePartialSettingsSnapshot | Partial<PersistedSettingsV2>,
  sectionIds: readonly string[],
): Record<string, Record<string, string>> {
  return Object.fromEntries(
    sectionIds.map((sectionId) => [sectionId, buildSectionPreviewValues(settings, sectionId as OxideImportedAppSettingsSectionId)]),
  );
}

export function getDefaultOxideAppSettingsExportSections(): OxideAppSettingsSectionId[] {
  return [...DEFAULT_OXIDE_APP_SETTINGS_EXPORT_SECTIONS];
}

export function getAllOxideAppSettingsExportSections(): OxideAppSettingsSectionId[] {
  return [...OXIDE_APP_SETTINGS_SECTION_IDS];
}

function buildOxideSectionedSettingsSnapshot(
  settings: PersistedSettingsV2,
  options?: ExportOxideAppSettingsOptions,
): OxideSectionedSettingsEnvelope | null {
  const sectionIds = options?.selectedSections?.length
    ? uniqueOxideSectionIds(options.selectedSections)
    : getDefaultOxideAppSettingsExportSections();

  if (sectionIds.length === 0) {
    return null;
  }

  const partialSettings: OxidePartialSettingsSnapshot = {};

  for (const sectionId of sectionIds) {
    switch (sectionId) {
      case 'general':
        partialSettings.general = { ...pickDefinedFields(settings.general, GENERAL_KEYS) };
        break;
      case 'terminalAppearance':
        partialSettings.terminal = {
          ...partialSettings.terminal,
          ...pickDefinedFields(settings.terminal, TERMINAL_APPEARANCE_KEYS),
        };
        break;
      case 'terminalBehavior':
        partialSettings.terminal = {
          ...partialSettings.terminal,
          ...pickDefinedFields(settings.terminal, TERMINAL_BEHAVIOR_KEYS),
        };
        break;
      case 'appearance':
        partialSettings.appearance = { ...pickDefinedFields(settings.appearance, APPEARANCE_KEYS) };
        break;
      case 'connections':
        partialSettings.connectionDefaults = { ...pickDefinedFields(settings.connectionDefaults, CONNECTION_DEFAULT_KEYS) };
        if (settings.reconnect) {
          partialSettings.reconnect = { ...pickDefinedFields(settings.reconnect, RECONNECT_KEYS) };
        }
        if (settings.connectionPool) {
          partialSettings.connectionPool = { ...pickDefinedFields(settings.connectionPool, CONNECTION_POOL_KEYS) };
        }
        break;
      case 'ai':
        partialSettings.ai = { ...pickDefinedFields(settings.ai, AI_KEYS) };
        break;
      case 'fileAndEditor':
        if (settings.sftp) {
          partialSettings.sftp = { ...pickDefinedFields(settings.sftp, SFTP_KEYS) };
        }
        if (settings.ide) {
          partialSettings.ide = { ...pickDefinedFields(settings.ide, IDE_KEYS) };
        }
        break;
      case 'localTerminal': {
        if (!settings.localTerminal) {
          break;
        }

        const localTerminalSettings = {
          ...pickDefinedFields(settings.localTerminal, LOCAL_TERMINAL_KEYS),
          ...(options?.includeLocalTerminalEnvVars
            ? { customEnvVars: settings.localTerminal.customEnvVars }
            : {}),
        };

        if (Object.keys(localTerminalSettings).length > 0) {
          partialSettings.localTerminal = localTerminalSettings;
        }
        break;
      }
    }
  }

  return {
    format: OXIDE_APP_SETTINGS_ENVELOPE_FORMAT,
    version: 1,
    sectionIds,
    settings: partialSettings,
  };
}

export function exportOxideAppSettingsSnapshot(options?: ExportOxideAppSettingsOptions): string | null {
  try {
    const envelope = buildOxideSectionedSettingsSnapshot(useSettingsStore.getState().settings, options);
    return envelope ? JSON.stringify(envelope) : null;
  } catch (error) {
    console.error('[SettingsStore] Failed to serialize .oxide settings snapshot:', error);
    return null;
  }
}

function parseImportedSettingsSnapshot(snapshotJson: string): ParsedImportedSettingsSnapshot {
  const parsed = JSON.parse(snapshotJson) as unknown;

  if (
    parsed
    && typeof parsed === 'object'
    && 'format' in parsed
    && (parsed as { format?: unknown }).format === OXIDE_APP_SETTINGS_ENVELOPE_FORMAT
    && 'settings' in parsed
    && (parsed as { settings?: unknown }).settings
    && typeof (parsed as { settings?: unknown }).settings === 'object'
  ) {
    const envelope = parsed as OxideSectionedSettingsEnvelope;
    return {
      format: 'sectioned',
      sectionIds: uniqueOxideSectionIds(envelope.sectionIds),
      settings: envelope.settings,
    };
  }

  return {
    format: 'legacy',
    sectionIds: ['legacy'],
    settings: parsed as OxidePartialSettingsSnapshot,
  };
}

function mergeSelectedOxideSettingsSections(
  currentSettings: PersistedSettingsV2,
  importedSettings: OxidePartialSettingsSnapshot,
  selectedSections: readonly OxideAppSettingsSectionId[],
): PersistedSettingsV2 {
  const nextSettings = mergeWithDefaults(currentSettings);

  for (const sectionId of selectedSections) {
    switch (sectionId) {
      case 'general':
        if (importedSettings.general) {
          nextSettings.general = { ...nextSettings.general, ...importedSettings.general };
        }
        break;
      case 'terminalAppearance':
        nextSettings.terminal = {
          ...nextSettings.terminal,
          ...pickDefinedFields(importedSettings.terminal, TERMINAL_APPEARANCE_KEYS),
        };
        break;
      case 'terminalBehavior':
        nextSettings.terminal = {
          ...nextSettings.terminal,
          ...pickDefinedFields(importedSettings.terminal, TERMINAL_BEHAVIOR_KEYS),
        };
        break;
      case 'appearance':
        if (importedSettings.appearance) {
          nextSettings.appearance = { ...nextSettings.appearance, ...importedSettings.appearance };
        }
        break;
      case 'connections':
        if (importedSettings.connectionDefaults) {
          nextSettings.connectionDefaults = {
            ...nextSettings.connectionDefaults,
            ...importedSettings.connectionDefaults,
          };
        }
        if (importedSettings.reconnect) {
          nextSettings.reconnect = {
            ...(nextSettings.reconnect || defaultReconnectSettings),
            ...importedSettings.reconnect,
          };
        }
        if (importedSettings.connectionPool) {
          nextSettings.connectionPool = {
            ...(nextSettings.connectionPool || defaultConnectionPoolSettings),
            ...importedSettings.connectionPool,
          };
        }
        break;
      case 'ai':
        if (importedSettings.ai) {
          nextSettings.ai = {
            ...nextSettings.ai,
            ...importedSettings.ai,
          };
        }
        break;
      case 'fileAndEditor':
        if (importedSettings.sftp) {
          nextSettings.sftp = {
            ...(nextSettings.sftp || defaultSftpSettings),
            ...importedSettings.sftp,
          };
        }
        if (importedSettings.ide) {
          nextSettings.ide = {
            ...(nextSettings.ide || defaultIdeSettings),
            ...importedSettings.ide,
          };
        }
        break;
      case 'localTerminal':
        if (importedSettings.localTerminal) {
          nextSettings.localTerminal = {
            ...(nextSettings.localTerminal || defaultLocalTerminalSettings),
            ...importedSettings.localTerminal,
          };
        }
        break;
    }
  }

  return normalizeHistorySettings(nextSettings);
}

export async function applyImportedSettingsSnapshot(
  snapshotJson: string,
  options?: ApplyImportedSettingsOptions,
): Promise<boolean> {
  try {
    const parsedSnapshot = parseImportedSettingsSnapshot(snapshotJson);
    const selectedSections = options?.selectedSections?.length
      ? parsedSnapshot.sectionIds.filter((sectionId) => options.selectedSections!.includes(sectionId))
      : parsedSnapshot.sectionIds;

    if (selectedSections.length === 0) {
      return false;
    }

    const normalized = parsedSnapshot.format === 'legacy' || selectedSections.includes('legacy')
      ? normalizeHistorySettings(
          migrateToolUseSettings(migrateAiProviders(mergeWithDefaults(parsedSnapshot.settings))),
        )
      : normalizeHistorySettings(
          migrateToolUseSettings(
            migrateAiProviders(
              mergeSelectedOxideSettingsSections(
                useSettingsStore.getState().settings,
                parsedSnapshot.settings,
                selectedSections as OxideAppSettingsSectionId[],
              ),
            ),
          ),
        );

    persistSettings(normalized);
    useSettingsStore.setState({ settings: normalized });

    const shouldApplyGeneral = parsedSnapshot.format === 'legacy' || selectedSections.includes('general');
    const shouldApplyFileAndEditor = parsedSnapshot.format === 'legacy' || selectedSections.includes('fileAndEditor');
    const shouldApplyConnections = parsedSnapshot.format === 'legacy' || selectedSections.includes('connections');

    if (shouldApplyGeneral) {
      localStorage.setItem('app_lang', normalized.general.language);

      const { changeLanguage } = await import('../i18n');
      await changeLanguage(normalized.general.language);
    }

    if (shouldApplyFileAndEditor) {
      syncSftpToBackend(normalized.sftp || defaultSftpSettings);
    }

    if (shouldApplyConnections) {
      syncConnectionPoolToBackend(
        normalized.connectionPool || defaultConnectionPoolSettings,
      );
    }

    return true;
  } catch (error) {
    console.error('[SettingsStore] Failed to apply imported settings snapshot:', error);
    return false;
  }
}

// ============================================================================
// Event Subscriptions (Side Effects)
// ============================================================================

// Track previous renderer for Toast notification
let previousRenderer: RendererType | null = null;

// Subscribe to theme changes - apply to document
useSettingsStore.subscribe(
  (state) => state.settings.terminal.theme,
  (themeName) => {
    // Validate theme exists (built-in or custom)
    const resolved = getTerminalTheme(themeName);
    if (!resolved && !themes[themeName]) {
      console.warn(`[SettingsStore] Theme "${themeName}" not found, falling back to default`);
      themeName = 'default';
    }

    // Set data-theme attribute for CSS variables
    if (isCustomTheme(themeName)) {
      // Custom themes use inline CSS variables
      document.documentElement.setAttribute('data-theme', 'custom');
      applyCustomThemeCSS(themeName);
    } else {
      clearCustomThemeCSS();
      document.documentElement.setAttribute('data-theme', themeName);
    }

    // Dispatch event for terminal components to update their xterm instances
    window.dispatchEvent(
      new CustomEvent('global-theme-changed', {
        detail: {
          themeName,
          xtermTheme: getTerminalTheme(themeName),
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

// Subscribe to appearance settings changes - apply CSS variables & data attributes
useSettingsStore.subscribe(
  (state) => state.settings.appearance,
  (appearance) => {
    applyAppearanceToDOM(appearance);
  }
);

/** Apply all appearance settings to the DOM (used by subscriber + init) */
function applyAppearanceToDOM(appearance: AppearanceSettings): void {
  const root = document.documentElement;

  // UI Density
  root.setAttribute('data-density', appearance.uiDensity);

  // Border Radius
  const r = appearance.borderRadius;
  root.style.setProperty('--ui-radius', `${r}px`);
  root.style.setProperty('--radius-sm', `max(1px, ${Math.round(r * 0.33)}px)`);
  root.style.setProperty('--radius-md', `${r}px`);
  root.style.setProperty('--radius-lg', `${Math.round(r * 1.33)}px`);

  // UI Font — split comma-separated input into proper CSS font-family fallback chain
  if (appearance.uiFontFamily) {
    const userFonts = appearance.uiFontFamily
      .split(',')
      .map((f) => f.trim())
      .filter(Boolean)
      .map((f) => `"${f}"`)
      .join(', ');
    root.style.setProperty('--font-sans', `${userFonts}, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`);
  } else {
    root.style.removeProperty('--font-sans');
  }

  // Animation Speed
  root.setAttribute('data-animation', appearance.animationSpeed);
  const speedMap: Record<AnimationSpeed, string> = { off: '0', reduced: '2', normal: '1', fast: '0.5' };
  root.style.setProperty('--animation-speed', speedMap[appearance.animationSpeed] || '1');

  // Frosted Glass
  root.setAttribute('data-frosted', appearance.frostedGlass);

  // Native vibrancy — call Tauri backend to apply/remove window vibrancy
  import('@tauri-apps/api/core').then(({ invoke }) => {
    invoke('set_window_vibrancy', { mode: appearance.frostedGlass }).catch((e: unknown) => {
      // Silently ignore on unsupported platforms
      if (appearance.frostedGlass === 'native') {
        console.warn('[Appearance] Failed to set native vibrancy:', e);
      }
    });
  });
}

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
  const currentTheme = settings.terminal.theme;
  const themeName = (themes[currentTheme] || isCustomTheme(currentTheme)) ? currentTheme : 'default';
  if (isCustomTheme(themeName)) {
    document.documentElement.setAttribute('data-theme', 'custom');
    applyCustomThemeCSS(themeName);
  } else {
    document.documentElement.setAttribute('data-theme', themeName);
  }

  // Apply terminal font CSS variable globally
  const { fontFamily, customFontFamily } = settings.terminal;
  const fontCSS = fontFamily === 'custom' && customFontFamily
    ? customFontFamily
    : getFontFamilyCSS(fontFamily);
  document.documentElement.style.setProperty('--terminal-font-family', fontCSS);

  // Apply appearance settings (density, radius, font, animation, frosted glass)
  applyAppearanceToDOM(settings.appearance);

  // Re-apply persisted connection pool settings after backend registry boots.
  syncConnectionPoolToBackend(
    settings.connectionPool || defaultConnectionPoolSettings,
  );

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
