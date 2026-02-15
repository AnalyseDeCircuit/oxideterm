import { ITheme } from '@xterm/xterm';

// ============================================================================
// Custom Theme Types
// ============================================================================

/** App UI variables that accompany each theme */
export type AppUiColors = {
  // ── Background Layer (背景层级) ──
  bg: string;
  bgPanel: string;
  bgHover: string;
  bgActive: string;
  bgSecondary: string;
  // ── Text Layer (文字层级) ──
  text: string;
  textMuted: string;
  textSecondary: string;
  // ── Border Layer (边框层级) ──
  border: string;
  borderStrong: string;
  divider: string;
  // ── Accent Layer (强调色) ──
  accent: string;
  accentHover: string;
  accentText: string;
  accentSecondary: string;
  // ── Semantic Colors (功能色) ──
  success: string;
  warning: string;
  error: string;
  info: string;
};

/** A user-created custom theme (terminal colors + app UI colors) */
export type CustomTheme = {
  name: string;            // Display name
  terminalColors: ITheme;  // xterm.js colors
  uiColors: AppUiColors;   // App chrome colors
};

/** localStorage key for custom themes */
const CUSTOM_THEMES_KEY = 'oxide-custom-themes';

// ============================================================================
// Custom Theme Persistence
// ============================================================================

/** Load custom themes from localStorage */
function loadCustomThemes(): Record<string, CustomTheme> {
  try {
    const raw = localStorage.getItem(CUSTOM_THEMES_KEY);
    if (!raw) return {};
    return JSON.parse(raw) as Record<string, CustomTheme>;
  } catch {
    return {};
  }
}

/** Persist custom themes to localStorage */
function saveCustomThemes(ct: Record<string, CustomTheme>): void {
  try {
    localStorage.setItem(CUSTOM_THEMES_KEY, JSON.stringify(ct));
  } catch (e) {
    console.error('[Themes] Failed to persist custom themes:', e);
  }
}

/** In-memory registry (loaded once, mutated by CRUD helpers) */
let customThemesRegistry: Record<string, CustomTheme> = loadCustomThemes();

// ============================================================================
// Custom Theme CRUD
// ============================================================================

/** Get all custom themes */
export function getCustomThemes(): Record<string, CustomTheme> {
  return customThemesRegistry;
}

/** Save/update a custom theme (id = slug key) */
export function saveCustomTheme(id: string, theme: CustomTheme): void {
  customThemesRegistry = { ...customThemesRegistry, [id]: theme };
  saveCustomThemes(customThemesRegistry);
}

/** Delete a custom theme */
export function deleteCustomTheme(id: string): void {
  const { [id]: _, ...rest } = customThemesRegistry;
  customThemesRegistry = rest;
  saveCustomThemes(customThemesRegistry);
}

/** Check if a theme id belongs to custom themes */
export function isCustomTheme(id: string): boolean {
  return id.startsWith('custom:');
}

// ============================================================================
// Unified Theme Resolution
// ============================================================================

/** Get the terminal ITheme for any theme (built-in or custom) */
export function getTerminalTheme(themeId: string): ITheme {
  if (isCustomTheme(themeId)) {
    const ct = customThemesRegistry[themeId];
    if (ct) return ct.terminalColors;
  }
  return themes[themeId] || themes.default;
}

/** Get the AppUiColors for a custom theme (returns null for built-in themes) */
export function getCustomUiColors(themeId: string): AppUiColors | null {
  if (isCustomTheme(themeId)) {
    const ct = customThemesRegistry[themeId];
    if (ct) return ct.uiColors;
  }
  return null;
}

/** Get all theme names: built-in + custom  */
export function getAllThemeNames(): string[] {
  return [...Object.keys(themes), ...Object.keys(customThemesRegistry)];
}

/**
 * Derive AppUiColors from an ITheme's terminal colors.
 * Used as default when creating a new custom theme from a built-in base.
 */
export function deriveUiColorsFromTerminal(t: ITheme): AppUiColors {
  const bg = (t.background as string) || '#09090b';
  const fg = (t.foreground as string) || '#f4f4f5';
  const cursor = (t.cursor as string) || '#ea580c';
  const muted = (t.brightBlack as string) || '#a1a1aa';

  // Lighten/darken helper
  const shift = (hex: string, amount: number): string => {
    const clamp = (v: number) => Math.max(0, Math.min(255, v));
    const r = clamp(parseInt(hex.slice(1, 3), 16) + amount);
    const g = clamp(parseInt(hex.slice(3, 5), 16) + amount);
    const b = clamp(parseInt(hex.slice(5, 7), 16) + amount);
    return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
  };

  // Mix two hex colors
  const mix = (c1: string, c2: string, ratio = 0.5): string => {
    const r = Math.round(parseInt(c1.slice(1, 3), 16) * ratio + parseInt(c2.slice(1, 3), 16) * (1 - ratio));
    const g = Math.round(parseInt(c1.slice(3, 5), 16) * ratio + parseInt(c2.slice(3, 5), 16) * (1 - ratio));
    const b = Math.round(parseInt(c1.slice(5, 7), 16) * ratio + parseInt(c2.slice(5, 7), 16) * (1 - ratio));
    return `#${Math.min(255,r).toString(16).padStart(2, '0')}${Math.min(255,g).toString(16).padStart(2, '0')}${Math.min(255,b).toString(16).padStart(2, '0')}`;
  };

  return {
    // Background
    bg,
    bgPanel: shift(bg, 15),
    bgHover: shift(bg, 30),
    bgActive: shift(bg, 40),
    bgSecondary: shift(bg, 10),
    // Text
    text: fg,
    textMuted: muted,
    textSecondary: mix(fg, muted, 0.5),
    // Border
    border: shift(bg, 30),
    borderStrong: mix(cursor, fg, 0.6),
    divider: shift(bg, 20),
    // Accent
    accent: cursor,
    accentHover: shift(cursor, -20),
    accentText: mix(cursor, bg, 0.7),
    accentSecondary: muted,
    // Semantic
    success: (t.green as string) || '#22c55e',
    warning: (t.yellow as string) || '#eab308',
    error: (t.red as string) || '#ef4444',
    info: (t.blue as string) || '#3b82f6',
  };
}

/** All CSS custom properties mapped from AppUiColors */
const UI_CSS_PROPS: [keyof AppUiColors, string][] = [
  // Background
  ['bg', '--theme-bg'],
  ['bgPanel', '--theme-bg-panel'],
  ['bgHover', '--theme-bg-hover'],
  ['bgActive', '--theme-bg-active'],
  ['bgSecondary', '--theme-bg-secondary'],
  // Text
  ['text', '--theme-text'],
  ['textMuted', '--theme-text-muted'],
  ['textSecondary', '--theme-text-secondary'],
  // Border
  ['border', '--theme-border'],
  ['borderStrong', '--theme-border-strong'],
  ['divider', '--theme-divider'],
  // Accent
  ['accent', '--theme-accent'],
  ['accentHover', '--theme-accent-hover'],
  ['accentText', '--theme-accent-text'],
  ['accentSecondary', '--theme-accent-secondary'],
  // Semantic
  ['success', '--theme-success'],
  ['warning', '--theme-warning'],
  ['error', '--theme-error'],
  ['info', '--theme-info'],
];

/**
 * Apply custom theme CSS variables to the document.
 * For custom themes, we inject CSS variables dynamically.
 */
export function applyCustomThemeCSS(themeId: string): void {
  const uiColors = getCustomUiColors(themeId);
  if (!uiColors) return;
  
  const root = document.documentElement;
  for (const [key, prop] of UI_CSS_PROPS) {
    if (uiColors[key]) {
      root.style.setProperty(prop, uiColors[key]);
    }
  }
}

/** Clear any inline custom theme CSS variables */
export function clearCustomThemeCSS(): void {
  const root = document.documentElement;
  for (const [, prop] of UI_CSS_PROPS) {
    root.style.removeProperty(prop);
  }
}

// ============================================================================
// Built-in Themes
// ============================================================================

export const themes: Record<string, ITheme> = {
  default: {
    background: '#09090b', // Neutral deep void
    foreground: '#f4f4f5', // Neutral text
    cursor: '#ea580c',     // Orange cursor
    selectionBackground: 'rgba(234, 88, 12, 0.3)',
    black: '#09090b',
    red: '#ef4444',
    green: '#22c55e',
    yellow: '#eab308',
    blue: '#3b82f6',
    magenta: '#d946ef',
    cyan: '#06b6d4',
    white: '#f4f4f5',
    brightBlack: '#71717a',
    brightRed: '#f87171',
    brightGreen: '#4ade80',
    brightYellow: '#facc15',
    brightBlue: '#60a5fa',
    brightMagenta: '#e879f9',
    brightCyan: '#22d3ee',
    brightWhite: '#ffffff',
  },
  oxide: {
    background: '#331a0d', // Vibrant rust-orange background
    foreground: '#fef3e2', // Warm cream text
    cursor: '#FF6B00',     // Vibrant Rust Orange (iPhone 17 Pro inspired)
    selectionBackground: 'rgba(255, 107, 0, 0.35)',
    black: '#331a0d',
    red: '#ff6b6b',
    green: '#51cf66',
    yellow: '#ffd43b',
    blue: '#4dabf7',
    magenta: '#e599f7',
    cyan: '#3bc9db',
    white: '#fef3e2',
    brightBlack: '#8b6f47',
    brightRed: '#ff8787',
    brightGreen: '#8ce99a',
    brightYellow: '#ffe066',
    brightBlue: '#74c0fc',
    brightMagenta: '#eebefa',
    brightCyan: '#66d9e8',
    brightWhite: '#ffffff',
  },
  dracula: {
    background: '#282a36',
    foreground: '#f8f8f2',
    cursor: '#bd93f9', // Using dracula purple for cursor
    selectionBackground: '#44475a',
    black: '#21222c',
    red: '#ff5555',
    green: '#50fa7b',
    yellow: '#f1fa8c',
    blue: '#8be9fd',
    magenta: '#ff79c6',
    cyan: '#8be9fd',
    white: '#f8f8f2',
    brightBlack: '#6272a4',
    brightRed: '#ff6e6e',
    brightGreen: '#69ff94',
    brightYellow: '#ffffa5',
    brightBlue: '#d6acff',
    brightMagenta: '#ff92df',
    brightCyan: '#a4ffff',
    brightWhite: '#ffffff',
  },
  nord: {
    background: '#2e3440',
    foreground: '#d8dee9',
    cursor: '#88c0d0',
    selectionBackground: '#4c566a',
    black: '#3b4252',
    red: '#bf616a',
    green: '#a3be8c',
    yellow: '#ebcb8b',
    blue: '#81a1c1',
    magenta: '#b48ead',
    cyan: '#88c0d0',
    white: '#e5e9f0',
    brightBlack: '#4c566a',
    brightRed: '#bf616a',
    brightGreen: '#a3be8c',
    brightYellow: '#ebcb8b',
    brightBlue: '#81a1c1',
    brightMagenta: '#b48ead',
    brightCyan: '#8fbcbb',
    brightWhite: '#eceff4',
  },
  'solarized-dark': {
    background: '#002b36',
    foreground: '#839496',
    cursor: '#93a1a1',
    selectionBackground: '#073642',
    black: '#073642',
    red: '#dc322f',
    green: '#859900',
    yellow: '#b58900',
    blue: '#268bd2',
    magenta: '#d33682',
    cyan: '#2aa198',
    white: '#eee8d5',
    brightBlack: '#002b36',
    brightRed: '#cb4b16',
    brightGreen: '#586e75',
    brightYellow: '#657b83',
    brightBlue: '#839496',
    brightMagenta: '#6c71c4',
    brightCyan: '#93a1a1',
    brightWhite: '#fdf6e3',
  },
  'one-dark': {
    background: '#282c34',
    foreground: '#abb2bf',
    cursor: '#528bff',
    selectionBackground: '#3e4451',
    black: '#282c34',
    red: '#e06c75',
    green: '#98c379',
    yellow: '#e5c07b',
    blue: '#61afef',
    magenta: '#c678dd',
    cyan: '#56b6c2',
    white: '#abb2bf',
    brightBlack: '#5c6370',
    brightRed: '#e06c75',
    brightGreen: '#98c379',
    brightYellow: '#e5c07b',
    brightBlue: '#61afef',
    brightMagenta: '#c678dd',
    brightCyan: '#56b6c2',
    brightWhite: '#ffffff',
  },
  monokai: {
    background: '#272822',
    foreground: '#f8f8f2',
    cursor: '#f8f8f0',
    selectionBackground: '#49483e',
    black: '#272822',
    red: '#f92672',
    green: '#a6e22e',
    yellow: '#f4bf75',
    blue: '#66d9ef',
    magenta: '#ae81ff',
    cyan: '#a1efe4',
    white: '#f8f8f2',
    brightBlack: '#75715e',
    brightRed: '#f92672',
    brightGreen: '#a6e22e',
    brightYellow: '#f4bf75',
    brightBlue: '#66d9ef',
    brightMagenta: '#ae81ff',
    brightCyan: '#a1efe4',
    brightWhite: '#f9f8f5',
  },
  'catppuccin-mocha': {
    background: '#1e1e2e',
    foreground: '#cdd6f4',
    cursor: '#f5e0dc',
    selectionBackground: '#585b70',
    black: '#45475a',
    red: '#f38ba8',
    green: '#a6e3a1',
    yellow: '#f9e2af',
    blue: '#89b4fa',
    magenta: '#f5c2e7',
    cyan: '#94e2d5',
    white: '#bac2de',
    brightBlack: '#585b70',
    brightRed: '#f38ba8',
    brightGreen: '#a6e3a1',
    brightYellow: '#f9e2af',
    brightBlue: '#89b4fa',
    brightMagenta: '#f5c2e7',
    brightCyan: '#94e2d5',
    brightWhite: '#a6adc8',
  },

  'github-dark': {
    background: '#0d1117',
    foreground: '#c9d1d9',
    cursor: '#58a6ff',
    selectionBackground: 'rgba(56, 139, 253, 0.4)',
    black: '#484f58',
    red: '#ff7b72',
    green: '#3fb950',
    yellow: '#d29922',
    blue: '#58a6ff',
    magenta: '#bc8cff',
    cyan: '#39c5cf',
    white: '#b1bac4',
    brightBlack: '#6e7681',
    brightRed: '#ffa198',
    brightGreen: '#56d364',
    brightYellow: '#e3b341',
    brightBlue: '#79c0ff',
    brightMagenta: '#d2a8ff',
    brightCyan: '#56d4dd',
    brightWhite: '#f0f6fc',
  },
  verdigris: {
    background: '#1C312C',
    foreground: '#2DD4BF',
    cursor: '#00FFFF',
    selectionBackground: 'rgba(45, 212, 191, 0.3)',
    black: '#0F201D',
    red: '#FF6B6B',
    green: '#2DD4BF',
    yellow: '#FCD34D',
    blue: '#38BDF8',
    magenta: '#A78BFA',
    cyan: '#22D3EE',
    white: '#F0FDFA',
    brightBlack: '#334F49',
    brightRed: '#F87171',
    brightGreen: '#5EEAD4',
    brightYellow: '#FDE047',
    brightBlue: '#7DD3FC',
    brightMagenta: '#C4B5FD',
    brightCyan: '#67E8F9',
    brightWhite: '#FFFFFF',
  },
  'silver-oxide': {
    background: '#1c1c1c',
    foreground: '#dcdcdc',
    cursor: '#9b88ff',
    selectionBackground: 'rgba(155, 136, 255, 0.3)',
    black: '#1c1c1c',
    red: '#a67f7f',
    green: '#8fa68f',
    yellow: '#a6a67f',
    blue: '#9b88ff',
    magenta: '#b0a3ff',
    cyan: '#88c0d0',
    white: '#dcdcdc',
    brightBlack: '#4a4a4a',
    brightRed: '#d69e9e',
    brightGreen: '#b8d6b8',
    brightYellow: '#d6d6a3',
    brightBlue: '#b3a6ff',
    brightMagenta: '#d1c7ff',
    brightCyan: '#a3d9e8',
    brightWhite: '#ffffff',
  },
  cuprite: {
    background: '#221212',
    foreground: '#f5d0c5',
    cursor: '#ff4d4d',
    selectionBackground: 'rgba(255, 77, 77, 0.3)',
    black: '#221212',
    red: '#ff4d4d',
    green: '#8b9460', // Warm olive
    yellow: '#ffcc66',
    blue: '#d46c6c', // Muted red/magenta
    magenta: '#ff8888',
    cyan: '#e0a0a0',
    white: '#f5d0c5',
    brightBlack: '#4a3030',
    brightRed: '#ff6666',
    brightGreen: '#a8b375',
    brightYellow: '#ffe099',
    brightBlue: '#f08080',
    brightMagenta: '#ffaaaa',
    brightCyan: '#f5c0c0',
    brightWhite: '#ffffff',
  },
  'chromium-oxide': {
    background: '#0e1a12',
    foreground: '#b8d8be',
    cursor: '#00ff41',
    selectionBackground: 'rgba(0, 255, 65, 0.3)',
    black: '#0e1a12',
    red: '#ff6b6b',
    green: '#00ff41',
    yellow: '#dfff00',
    blue: '#0088ff',
    magenta: '#bd93f9',
    cyan: '#00ffff',
    white: '#b8d8be',
    brightBlack: '#16261b',
    brightRed: '#ff8585',
    brightGreen: '#33ff66',
    brightYellow: '#eaff33',
    brightBlue: '#33a0ff',
    brightMagenta: '#ceadfa',
    brightCyan: '#33ffff',
    brightWhite: '#d6e9da',
  },
  'paper-oxide': {
    background: '#f4f0e6', // Warm parchment paper
    foreground: '#3c3c3c', // Soft charcoal text
    cursor: '#8d6e63',     // Copper oxide brown
    selectionBackground: 'rgba(141, 110, 99, 0.2)',
    black: '#3c3c3c',
    red: '#d32f2f',
    green: '#388e3c',
    yellow: '#fbc02d',
    blue: '#1976d2',
    magenta: '#7b1fa2',
    cyan: '#0097a7',
    white: '#ece8dd',
    brightBlack: '#787878',
    brightRed: '#ef5350',
    brightGreen: '#66bb6a',
    brightYellow: '#fff176',
    brightBlue: '#42a5f5',
    brightMagenta: '#ab47bc',
    brightCyan: '#26c6da',
    brightWhite: '#ffffff',
  },
  magnetite: {
    background: '#1A1A1A',
    foreground: '#E5E7EB',
    cursor: '#4682B4',
    selectionBackground: 'rgba(70, 130, 180, 0.3)',
    black: '#262626',
    red: '#EF4444',
    green: '#10B981',
    yellow: '#F59E0B',
    blue: '#4682B4',
    magenta: '#9333EA',
    cyan: '#06B6D4',
    white: '#F3F4F6',
    brightBlack: '#404040',
    brightRed: '#F87171',
    brightGreen: '#34D399',
    brightYellow: '#FBBF24',
    brightBlue: '#60A5FA',
    brightMagenta: '#A855F7',
    brightCyan: '#22D3EE',
    brightWhite: '#FFFFFF',
  },
  cobalt: {
    background: '#0F172A', // Deep Slate/Blue
    foreground: '#CBD5E1',
    cursor: '#3B82F6',
    selectionBackground: 'rgba(59, 130, 246, 0.3)',
    black: '#1E293B',
    red: '#F43F5E',
    green: '#10B981',
    yellow: '#EAB308',
    blue: '#3B82F6',
    magenta: '#8B5CF6',
    cyan: '#0EA5E9',
    white: '#E2E8F0',
    brightBlack: '#334155',
    brightRed: '#FB7185',
    brightGreen: '#34D399',
    brightYellow: '#FACC15',
    brightBlue: '#60A5FA',
    brightMagenta: '#A78BFA',
    brightCyan: '#38BDF8',
    brightWhite: '#F8FAFC',
  },
  ochre: {
    background: '#1C1917',
    foreground: '#FDE047', // Yellowish text
    cursor: '#EA580C',     // Orange/Rust
    selectionBackground: 'rgba(234, 88, 12, 0.3)',
    black: '#292524',
    red: '#EF4444',
    green: '#84CC16',
    yellow: '#EAB308',
    blue: '#0EA5E9',
    magenta: '#D946EF',
    cyan: '#14B8A6',
    white: '#F5F5F4',
    brightBlack: '#44403C',
    brightRed: '#F87171',
    brightGreen: '#A3E635',
    brightYellow: '#FACC15',
    brightBlue: '#38BDF8',
    brightMagenta: '#E879F9',
    brightCyan: '#2DD4BF',
    brightWhite: '#FFFFFF',
  },
  'tokyo-night': {
    background: '#1a1b26',
    foreground: '#c0caf5',
    cursor: '#c0caf5',
    selectionBackground: '#515c7e',
    black: '#15161e',
    red: '#f7768e',
    green: '#9ece6a',
    yellow: '#e0af68',
    blue: '#7aa2f7',
    magenta: '#bb9af7',
    cyan: '#7dcfff',
    white: '#a9b1d6',
    brightBlack: '#414868',
    brightRed: '#f7768e',
    brightGreen: '#9ece6a',
    brightYellow: '#e0af68',
    brightBlue: '#7aa2f7',
    brightMagenta: '#bb9af7',
    brightCyan: '#7dcfff',
    brightWhite: '#c0caf5',
  },
  'gruvbox-dark': {
    background: '#282828',
    foreground: '#ebdbb2',
    cursor: '#ebdbb2',
    selectionBackground: '#665c54',
    black: '#282828',
    red: '#cc241d',
    green: '#98971a',
    yellow: '#d79921',
    blue: '#458588',
    magenta: '#b16286',
    cyan: '#689d6a',
    white: '#a89984',
    brightBlack: '#928374',
    brightRed: '#fb4934',
    brightGreen: '#b8bb26',
    brightYellow: '#fabd2f',
    brightBlue: '#83a598',
    brightMagenta: '#d3869b',
    brightCyan: '#8ec07c',
    brightWhite: '#ebdbb2',
  },
  'rose-pine': {
    background: '#191724',
    foreground: '#e0def4',
    cursor: '#524f67',
    selectionBackground: '#403d52',
    black: '#26233a',
    red: '#eb6f92',
    green: '#31748f',
    yellow: '#f6c177',
    blue: '#9ccfd8',
    magenta: '#c4a7e7',
    cyan: '#ebbcba',
    white: '#e0def4',
    brightBlack: '#6e6a86',
    brightRed: '#eb6f92',
    brightGreen: '#31748f',
    brightYellow: '#f6c177',
    brightBlue: '#9ccfd8',
    brightMagenta: '#c4a7e7',
    brightCyan: '#ebbcba',
    brightWhite: '#524f67',
  },
  kanagawa: {
    background: '#1F1F28',
    foreground: '#DCD7BA',
    cursor: '#C8C093',
    selectionBackground: '#2D4F67',
    black: '#090618',
    red: '#C34043',
    green: '#76946A',
    yellow: '#C0A36E',
    blue: '#7E9CD8',
    magenta: '#957FB8',
    cyan: '#6A9589',
    white: '#C8C093',
    brightBlack: '#727169',
    brightRed: '#E82424',
    brightGreen: '#98BB6C',
    brightYellow: '#E6C384',
    brightBlue: '#7FB4CA',
    brightMagenta: '#938AA9',
    brightCyan: '#7AA89F',
    brightWhite: '#DCD7BA',
  },
  'synthwave-84': {
    background: '#2b213a',
    foreground: '#ffffff',
    cursor: '#f97e72', // Radish
    selectionBackground: '#5c4f75',
    black: '#2b213a',
    red: '#fe4450', // Neon Red
    green: '#72f1b8', // Neon Green
    yellow: '#fede5d', // Neon Yellow
    blue: '#03edf9', // Neon Cyan/Blue
    magenta: '#ff7edb', // Neon Pink
    cyan: '#03edf9', // Same as blue for synthwave vibe
    white: '#ffffff',
    brightBlack: '#6b5e87', // Muted purple
    brightRed: '#fe4450',
    brightGreen: '#72f1b8',
    brightYellow: '#fede5d',
    brightBlue: '#36f9f6', // Bright Cyan
    brightMagenta: '#ff7edb',
    brightCyan: '#36f9f6',
    brightWhite: '#ffffff', // Pure White (Glow)
  },
  azurite: {
    background: '#091A2E', // Deep Azure Blue
    foreground: '#C0D8F0', // Pale Blue-White
    cursor: '#0066CC',     // Vivid Azure
    selectionBackground: 'rgba(0, 102, 204, 0.3)',
    black: '#0D2238',
    red: '#FF6B6B',
    green: '#48CFAD',
    yellow: '#FFCE54',
    blue: '#0066CC',
    magenta: '#AC92EC',
    cyan: '#4FC1E9',
    white: '#F5F7FA',
    brightBlack: '#434A54',
    brightRed: '#FF6B6B',
    brightGreen: '#48CFAD',
    brightYellow: '#FFCE54',
    brightBlue: '#5D9CEC',
    brightMagenta: '#AC92EC',
    brightCyan: '#4FC1E9',
    brightWhite: '#FFFFFF',
  },
  malachite: {
    background: '#0B231A', // Deep Mineral Green
    foreground: '#E0F2E9', // Minty White
    cursor: '#10B981',     // Vibrant Emerald
    selectionBackground: 'rgba(16, 185, 129, 0.3)',
    black: '#064E3B',
    red: '#EF4444',
    green: '#10B981',
    yellow: '#F59E0B',
    blue: '#3B82F6',
    magenta: '#8B5CF6',
    cyan: '#06B6D4',
    white: '#ECFDF5',
    brightBlack: '#065F46',
    brightRed: '#F87171',
    brightGreen: '#34D399',
    brightYellow: '#FBBF24',
    brightBlue: '#60A5FA',
    brightMagenta: '#A78BFA',
    brightCyan: '#22D3EE',
    brightWhite: '#FFFFFF',
  },
  hematite: {
    background: '#1C1C1E', // Dark Steel Grey
    foreground: '#D1D1D6', // Metallic Silver
    cursor: '#FF3B30',     // Iron Red
    selectionBackground: 'rgba(255, 59, 48, 0.3)',
    black: '#2C2C2E',
    red: '#FF3B30',
    green: '#34C759',
    yellow: '#FFD60A',
    blue: '#0A84FF',
    magenta: '#BF5AF2',
    cyan: '#64D2FF',
    white: '#E5E5EA',
    brightBlack: '#3A3A3C',
    brightRed: '#FF453A',
    brightGreen: '#32D74B',
    brightYellow: '#FFD60A',
    brightBlue: '#0A84FF',
    brightMagenta: '#BF5AF2',
    brightCyan: '#64D2FF',
    brightWhite: '#FFFFFF',
  },
  bismuth: {
    background: '#120F1D', // Dark Purple-Black
    foreground: '#E9D5FF', // Lavender
    cursor: '#D946EF',     // Iridescent Pink/Purple
    selectionBackground: 'rgba(217, 70, 239, 0.3)',
    black: '#2E1065',
    red: '#F43F5E',
    green: '#10B981',
    yellow: '#FACC15',
    blue: '#3B82F6',
    magenta: '#D946EF',
    cyan: '#06B6D4',
    white: '#FAF5FF',
    brightBlack: '#4C1D95',
    brightRed: '#FB7185',
    brightGreen: '#34D399',
    brightYellow: '#FDE047',
    brightBlue: '#60A5FA',
    brightMagenta: '#E879F9',
    brightCyan: '#22D3EE',
    brightWhite: '#FFFFFF',
  },
  'fairy-floss': {
    background: '#5a5475', // Purple haze
    foreground: '#f8f8f2',
    cursor: '#ffb86c',
    selectionBackground: '#8076aa',
    black: '#463c57',
    red: '#ff857f',
    green: '#8cfccf', // Mint
    yellow: '#e6c000',
    blue: '#c5a3ff',
    magenta: '#ff857f', // Pinkish
    cyan: '#c2ffdf',
    white: '#f8f8f0',
    brightBlack: '#605770',
    brightRed: '#ffb8d9', // Hot Pink
    brightGreen: '#8cfccf',
    brightYellow: '#e6c000',
    brightBlue: '#c5a3ff',
    brightMagenta: '#ffb8d9',
    brightCyan: '#c2ffdf',
    brightWhite: '#f8f8f0',
  },
  sakura: {
    background: '#2c242a', // Dark warm grey/pink
    foreground: '#e6d2d9',
    cursor: '#ff79c6',     // Bright pink
    selectionBackground: '#5c434f',
    black: '#3f3238',
    red: '#f55d7a', // Sakura Red
    green: '#9ece6a',
    yellow: '#f9f871',
    blue: '#82aaff',
    magenta: '#ff79c6', // Pink
    cyan: '#8be9fd',
    white: '#f8f8f2',
    brightBlack: '#5c434f',
    brightRed: '#ff5555',
    brightGreen: '#50fa7b',
    brightYellow: '#f1fa8c',
    brightBlue: '#6272a4',
    brightMagenta: '#ff92df',
    brightCyan: '#8be9fd',
    brightWhite: '#ffffff',
  },
  'hot-pink': {
    background: '#efdfe5', // Softer/Darker pink for comfort
    foreground: '#8a3a5b', // Readable maroon/pink
    cursor: '#e60073',     // Hot pink
    selectionBackground: 'rgba(230, 0, 115, 0.2)',
    black: '#efdfe5',
    red: '#d00055',
    green: '#00aa55',
    yellow: '#bfa000',
    blue: '#0066cc',
    magenta: '#cc00aa',
    cyan: '#0099aa',
    white: '#8a3a5b',
    brightBlack: '#a05070',
    brightRed: '#ff3388',
    brightGreen: '#33cc88',
    brightYellow: '#dcb000',
    brightBlue: '#3399ff',
    brightMagenta: '#ff66cc',
    brightCyan: '#33ccdd',
    brightWhite: '#401020',
  },
  // ── Spring Rice Theme ──────────────────────────────────────
  // White·Gold·Blue gradient with spring yellow-green as visual center
  // Indigo #191978 / Navy #1e2350 / Sky blue #8ca0b4 / Wheat gold #b49b64 / Spring yellow #dcde78
  'spring-rice': {
    background: '#e8ead0',     // 春日秧田（青黄底）
    foreground: '#1e2350',     // 深藏蓝
    cursor: '#dcde78',         // 春芽黄
    selectionBackground: 'rgba(25, 25, 120, 0.15)', // 靛蓝选区
    black: '#1e2350',          // 深藏蓝
    red: '#a05a3e',            // 深赭
    green: '#3a7a5e',          // 深秧青
    yellow: '#8a8520',         // 深麦黄
    blue: '#191978',           // 深靛蓝
    magenta: '#4a4d8a',        // 深靛紫
    cyan: '#3a6a72',           // 深土青
    white: '#4a6a5a',          // 土青
    brightBlack: '#3a4080',    // 靛灰
    brightRed: '#b86a4a',      // 赭红
    brightGreen: '#4a8a6a',    // 秧苗青
    brightYellow: '#9a9030',   // 麦黄
    brightBlue: '#4a5098',     // 亮靛蓝
    brightMagenta: '#5a5aa0',  // 靛紫
    brightCyan: '#4a7a85',     // 土青
    brightWhite: '#5a7a6a',    // 深土青
  },
  'spring-green': {
    background: '#e2f5e9', // Warmer/Softer Mint
    foreground: '#1a4d33', // Softer Dark Green
    cursor: '#16a34a',     // Vivid Green
    selectionBackground: 'rgba(22, 163, 74, 0.2)',
    black: '#e2f5e9',
    red: '#dc2626',
    green: '#15803d',
    yellow: '#b45309',
    blue: '#2563eb',
    magenta: '#7c3aed',
    cyan: '#0891b2',
    white: '#1a4d33',
    brightBlack: '#a3d9b5',
    brightRed: '#ef4444',
    brightGreen: '#22c55e',
    brightYellow: '#f59e0b',
    brightBlue: '#3b82f6',
    brightMagenta: '#8b5cf6',
    brightCyan: '#06b6d4',
    brightWhite: '#052e16',
  },
};
