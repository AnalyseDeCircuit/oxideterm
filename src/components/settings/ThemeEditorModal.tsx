/**
 * ThemeEditorModal — Create / Edit custom terminal + app themes.
 *
 * Features:
 *  • Duplicate from any built-in or custom theme
 *  • Color pickers for all 22 terminal fields (ITheme) + 8 app UI fields
 *  • Live preview with a mini terminal simulation
 *  • Name editing with slug auto-generation
 *  • Delete custom theme
 */

import { useState, useCallback, useMemo, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import type { ITheme } from '@xterm/xterm';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../ui/select';
import { Trash2, Copy, Save } from 'lucide-react';
import { cn } from '../../lib/utils';
import {
  themes,
  type CustomTheme,
  type AppUiColors,
  getCustomThemes,
  saveCustomTheme,
  deleteCustomTheme,
  deriveUiColorsFromTerminal,
  getTerminalTheme,
} from '../../lib/themes';
import { useSettingsStore } from '../../store/settingsStore';

// ============================================================================
// Types
// ============================================================================

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** If set, we're editing an existing custom theme */
  editThemeId?: string | null;
  /** Base theme to duplicate from when creating new */
  baseThemeId?: string;
};

// Terminal color fields with labels
const TERMINAL_COLOR_FIELDS: { key: keyof ITheme; labelKey: string }[] = [
  { key: 'background', labelKey: 'bg' },
  { key: 'foreground', labelKey: 'fg' },
  { key: 'cursor', labelKey: 'cursor' },
  { key: 'selectionBackground', labelKey: 'selection' },
  { key: 'black', labelKey: 'black' },
  { key: 'red', labelKey: 'red' },
  { key: 'green', labelKey: 'green' },
  { key: 'yellow', labelKey: 'yellow' },
  { key: 'blue', labelKey: 'blue' },
  { key: 'magenta', labelKey: 'magenta' },
  { key: 'cyan', labelKey: 'cyan' },
  { key: 'white', labelKey: 'white' },
  { key: 'brightBlack', labelKey: 'bright_black' },
  { key: 'brightRed', labelKey: 'bright_red' },
  { key: 'brightGreen', labelKey: 'bright_green' },
  { key: 'brightYellow', labelKey: 'bright_yellow' },
  { key: 'brightBlue', labelKey: 'bright_blue' },
  { key: 'brightMagenta', labelKey: 'bright_magenta' },
  { key: 'brightCyan', labelKey: 'bright_cyan' },
  { key: 'brightWhite', labelKey: 'bright_white' },
];

type UiColorSection = {
  titleKey: string;
  fields: { key: keyof AppUiColors; labelKey: string }[];
};

const UI_COLOR_SECTIONS: UiColorSection[] = [
  {
    titleKey: 'section_background',
    fields: [
      { key: 'bg', labelKey: 'ui_bg' },
      { key: 'bgPanel', labelKey: 'ui_panel' },
      { key: 'bgHover', labelKey: 'ui_hover' },
      { key: 'bgActive', labelKey: 'ui_active' },
      { key: 'bgSecondary', labelKey: 'ui_bg_secondary' },
    ],
  },
  {
    titleKey: 'section_text',
    fields: [
      { key: 'text', labelKey: 'ui_text' },
      { key: 'textMuted', labelKey: 'ui_text_muted' },
      { key: 'textSecondary', labelKey: 'ui_text_secondary' },
    ],
  },
  {
    titleKey: 'section_border',
    fields: [
      { key: 'border', labelKey: 'ui_border' },
      { key: 'borderStrong', labelKey: 'ui_border_strong' },
      { key: 'divider', labelKey: 'ui_divider' },
    ],
  },
  {
    titleKey: 'section_accent',
    fields: [
      { key: 'accent', labelKey: 'ui_accent' },
      { key: 'accentHover', labelKey: 'ui_accent_hover' },
      { key: 'accentText', labelKey: 'ui_accent_text' },
      { key: 'accentSecondary', labelKey: 'ui_accent_secondary' },
    ],
  },
  {
    titleKey: 'section_semantic',
    fields: [
      { key: 'success', labelKey: 'ui_success' },
      { key: 'warning', labelKey: 'ui_warning' },
      { key: 'error', labelKey: 'ui_error' },
      { key: 'info', labelKey: 'ui_info' },
    ],
  },
];

/** Convert a color string (hex or rgba) to a pure hex for <input type="color"> */
function toHex6(color: string | undefined): string {
  if (!color) return '#000000';
  // Already hex
  if (/^#[0-9a-fA-F]{6}$/.test(color)) return color;
  if (/^#[0-9a-fA-F]{3}$/.test(color)) {
    const [, r, g, b] = color.match(/^#(.)(.)(.)$/)!;
    return `#${r}${r}${g}${g}${b}${b}`;
  }
  // rgba — parse and ignore alpha
  const match = color.match(/rgba?\(\s*(\d+),\s*(\d+),\s*(\d+)/);
  if (match) {
    const [, r, g, b] = match;
    return `#${Number(r).toString(16).padStart(2, '0')}${Number(g).toString(16).padStart(2, '0')}${Number(b).toString(16).padStart(2, '0')}`;
  }
  return '#000000';
}

/** Generate a slug from theme name */
function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9\u4e00-\u9fff]+/g, '-')
    .replace(/^-|-$/g, '')
    || 'untitled';
}

// ============================================================================
// ColorSwatch — compact color picker
// ============================================================================

const ColorSwatch = ({
  color,
  onChange,
  label,
}: {
  color: string;
  onChange: (hex: string) => void;
  label: string;
}) => {
  const { t } = useTranslation();
  const hex6 = toHex6(color);
  const [editing, setEditing] = useState(false);
  const [inputVal, setInputVal] = useState(hex6);

  // Sync input when color changes externally
  useEffect(() => { if (!editing) setInputVal(hex6); }, [hex6, editing]);

  const commitInput = () => {
    setEditing(false);
    const v = inputVal.trim();
    // Accept "#abc", "#aabbcc", "aabbcc", "abc"
    if (/^#?[0-9a-fA-F]{6}$/.test(v)) {
      onChange(v.startsWith('#') ? v : `#${v}`);
    } else if (/^#?[0-9a-fA-F]{3}$/.test(v)) {
      const raw = v.replace('#', '');
      onChange(`#${raw[0]}${raw[0]}${raw[1]}${raw[1]}${raw[2]}${raw[2]}`);
    }
    // Otherwise revert
    setInputVal(hex6);
  };

  return (
    <div className="flex items-center gap-2 group">
      <label className="relative cursor-pointer" title={label}>
        <input
          type="color"
          value={hex6}
          onChange={(e) => onChange(e.target.value)}
          className="absolute inset-0 opacity-0 cursor-pointer w-full h-full"
        />
        <div
          className="w-7 h-7 rounded border border-theme-border/60 shadow-sm transition-transform group-hover:scale-110"
          style={{ backgroundColor: hex6 }}
        />
      </label>
      <div className="flex flex-col min-w-0">
        <span className="text-[10px] text-theme-text-muted truncate leading-tight">{label}</span>
        {editing ? (
          <input
            autoFocus
            value={inputVal}
            onChange={(e) => setInputVal(e.target.value)}
            onBlur={commitInput}
            onKeyDown={(e) => { if (e.key === 'Enter') commitInput(); if (e.key === 'Escape') { setEditing(false); setInputVal(hex6); } }}
            className="w-[72px] h-4 text-[10px] font-mono bg-theme-bg border border-theme-border rounded px-0.5 text-theme-text outline-none focus:border-theme-accent"
            placeholder="#RRGGBB"
          />
        ) : (
          <span
            className="text-[10px] font-mono text-theme-text/70 leading-tight cursor-text hover:text-theme-accent transition-colors"
            onClick={() => { setEditing(true); setInputVal(hex6); }}
            title={t('settings_view.custom_theme.click_to_edit_hex')}
          >
            {hex6}
          </span>
        )}
      </div>
    </div>
  );
};

// ============================================================================
// ThemeEditorModal
// ============================================================================

export const ThemeEditorModal = ({ open, onOpenChange, editThemeId, baseThemeId }: Props) => {
  const { t } = useTranslation();
  const updateTerminal = useSettingsStore((s) => s.updateTerminal);

  // ── State ──
  const [name, setName] = useState('');
  const [termColors, setTermColors] = useState<ITheme>({});
  const [uiColors, setUiColors] = useState<AppUiColors>({
    // Background
    bg: '#09090b', bgPanel: '#18181b', bgHover: '#27272a',
    bgActive: '#3f3f46', bgSecondary: '#1c1c20',
    // Text
    text: '#f4f4f5', textMuted: '#a1a1aa', textSecondary: '#71717a',
    // Border
    border: '#27272a', borderStrong: '#3f3f46', divider: '#27272a',
    // Accent
    accent: '#ea580c', accentHover: '#c2410c',
    accentText: '#ffffff', accentSecondary: '#f97316',
    // Semantic
    success: '#22c55e', warning: '#eab308', error: '#ef4444', info: '#3b82f6',
  });
  const [activeSection, setActiveSection] = useState<'terminal' | 'ui'>('terminal');

  // Load existing theme or duplicate from base
  useEffect(() => {
    if (!open) return;

    if (editThemeId) {
      // Editing existing custom theme
      const custom = getCustomThemes()[editThemeId];
      if (custom) {
        setName(custom.name);
        setTermColors({ ...custom.terminalColors });
        setUiColors({ ...custom.uiColors });
      }
    } else {
      // Creating new from base
      const baseId = baseThemeId || 'default';
      const baseTerminal = getTerminalTheme(baseId);
      setName(t('settings_view.custom_theme.new_theme_name'));
      setTermColors({ ...baseTerminal });
      setUiColors(deriveUiColorsFromTerminal(baseTerminal));
    }
    setActiveSection('terminal');
  }, [open, editThemeId, baseThemeId, t]);

  const isEditing = !!editThemeId;

  // ── Color updaters ──
  const updateTermColor = useCallback((key: keyof ITheme, value: string) => {
    setTermColors((prev) => ({ ...prev, [key]: value }));
  }, []);

  const updateUiColor = useCallback((key: keyof AppUiColors, value: string) => {
    setUiColors((prev) => ({ ...prev, [key]: value }));
  }, []);

  // ── Auto-derive UI colors from terminal when terminal bg/fg/cursor change ──
  const handleAutoDerive = useCallback(() => {
    setUiColors(deriveUiColorsFromTerminal(termColors));
  }, [termColors]);

  // ── Save ──
  const handleSave = useCallback(() => {
    const id = isEditing ? editThemeId! : `custom:${slugify(name)}`;
    const theme: CustomTheme = {
      name,
      terminalColors: termColors,
      uiColors,
    };
    saveCustomTheme(id, theme);
    // Apply the theme
    updateTerminal('theme', id);
    onOpenChange(false);
  }, [name, termColors, uiColors, isEditing, editThemeId, updateTerminal, onOpenChange]);

  // ── Delete ──
  const handleDelete = useCallback(() => {
    if (!editThemeId) return;
    deleteCustomTheme(editThemeId);
    updateTerminal('theme', 'default');
    onOpenChange(false);
  }, [editThemeId, updateTerminal, onOpenChange]);

  // ── Duplicate from built-in list ──
  const allBuiltInKeys = useMemo(() => Object.keys(themes).sort(), []);

  const handleDuplicate = useCallback((sourceId: string) => {
    const source = getTerminalTheme(sourceId);
    setTermColors({ ...source });
    setUiColors(deriveUiColorsFromTerminal(source));
  }, []);

  // ── Preview ──
  const previewTerminal = useMemo(() => {
    const bg = (termColors.background as string) || '#09090b';
    const fg = (termColors.foreground as string) || '#f4f4f5';
    const cursor = (termColors.cursor as string) || '#ea580c';
    const red = (termColors.red as string) || '#ef4444';
    const green = (termColors.green as string) || '#22c55e';
    const yellow = (termColors.yellow as string) || '#eab308';
    const blue = (termColors.blue as string) || '#3b82f6';
    const magenta = (termColors.magenta as string) || '#d946ef';
    const cyan = (termColors.cyan as string) || '#06b6d4';

    return (
      <div className="rounded border border-theme-border overflow-hidden" style={{ backgroundColor: bg }}>
        {/* Title bar */}
        <div className="flex items-center gap-1.5 px-3 py-1.5" style={{ backgroundColor: uiColors.bgPanel }}>
          <div className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: red }} />
          <div className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: yellow }} />
          <div className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: green }} />
          <span className="ml-2 text-[10px]" style={{ color: uiColors.textMuted }}>Terminal — {name}</span>
        </div>
        {/* Content */}
        <div className="p-3 font-mono text-xs space-y-1" style={{ color: fg }}>
          <div>
            <span style={{ color: green }}>user@oxide</span>
            <span style={{ color: fg }}>:</span>
            <span style={{ color: blue }}>~/projects</span>
            <span style={{ color: fg }}>$ </span>
            <span style={{ color: magenta }}>git</span> status
          </div>
          <div style={{ color: yellow }}>On branch main</div>
          <div style={{ color: cyan }}>Changes not staged for commit:</div>
          <div>
            <span style={{ color: red }}>  modified: </span>
            <span style={{ color: fg }}>src/main.rs</span>
          </div>
          <div className="flex items-center">
            <span style={{ color: green }}>user@oxide</span>
            <span style={{ color: fg }}>:</span>
            <span style={{ color: blue }}>~</span>
            <span style={{ color: fg }}>$ </span>
            <span className="w-2 h-4 animate-pulse inline-block" style={{ backgroundColor: cursor }} />
          </div>
        </div>
        {/* UI chrome preview bar */}
        <div className="flex items-center gap-2 px-3 py-1.5 border-t" style={{ backgroundColor: uiColors.bg, borderColor: uiColors.border }}>
          <div className="text-[9px] px-1.5 py-0.5 rounded" style={{ backgroundColor: uiColors.accent, color: uiColors.accentText }}>Active</div>
          <div className="text-[9px] px-1.5 py-0.5 rounded" style={{ backgroundColor: uiColors.bgHover, color: uiColors.textMuted }}>Hover</div>
          <div className="text-[9px] px-1.5 py-0.5 rounded border" style={{ backgroundColor: uiColors.bgPanel, color: uiColors.text, borderColor: uiColors.border }}>Panel</div>
          <div className="ml-auto flex items-center gap-1">
            <div className="w-2 h-2 rounded-full" style={{ backgroundColor: uiColors.success }} />
            <div className="w-2 h-2 rounded-full" style={{ backgroundColor: uiColors.warning }} />
            <div className="w-2 h-2 rounded-full" style={{ backgroundColor: uiColors.error }} />
            <div className="w-2 h-2 rounded-full" style={{ backgroundColor: uiColors.info }} />
          </div>
        </div>
      </div>
    );
  }, [termColors, uiColors, name]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[85vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle className="text-theme-text text-base">
            {isEditing
              ? t('settings_view.custom_theme.edit_title')
              : t('settings_view.custom_theme.create_title')}
          </DialogTitle>
          <DialogDescription className="text-theme-text-muted text-xs">
            {t('settings_view.custom_theme.description')}
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto px-4 py-3 space-y-4">
          {/* ── Name + Duplicate From ── */}
          <div className="flex gap-3 items-end">
            <div className="flex-1">
              <Label className="text-theme-text text-xs">{t('settings_view.custom_theme.name')}</Label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="mt-1 h-8 text-sm bg-theme-bg border-theme-border text-theme-text"
                placeholder={t('settings_view.custom_theme.name_placeholder')}
              />
            </div>
            {!isEditing && (
              <div className="w-[180px]">
                <Label className="text-theme-text text-xs">{t('settings_view.custom_theme.duplicate_from')}</Label>
                <Select onValueChange={handleDuplicate}>
                  <SelectTrigger className="mt-1 h-8 text-xs text-theme-text">
                    <SelectValue placeholder={t('settings_view.custom_theme.select_base')} />
                  </SelectTrigger>
                  <SelectContent className="bg-theme-bg-panel border-theme-border max-h-[200px]">
                    {allBuiltInKeys.map((key) => (
                      <SelectItem key={key} value={key} className="text-theme-text text-xs focus:bg-theme-bg-hover focus:text-theme-text">
                        {key.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ')}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}
          </div>

          {/* ── Live Preview ── */}
          {previewTerminal}

          {/* ── Section Tabs ── */}
          <div className="flex border-b border-theme-border">
            <button
              className={cn(
                'px-3 py-1.5 text-xs font-medium transition-colors',
                activeSection === 'terminal'
                  ? 'text-theme-accent border-b-2 border-theme-accent'
                  : 'text-theme-text-muted hover:text-theme-text'
              )}
              onClick={() => setActiveSection('terminal')}
            >
              {t('settings_view.custom_theme.terminal_colors')}
            </button>
            <button
              className={cn(
                'px-3 py-1.5 text-xs font-medium transition-colors',
                activeSection === 'ui'
                  ? 'text-theme-accent border-b-2 border-theme-accent'
                  : 'text-theme-text-muted hover:text-theme-text'
              )}
              onClick={() => setActiveSection('ui')}
            >
              {t('settings_view.custom_theme.ui_colors')}
            </button>
          </div>

          {/* ── Terminal Colors Grid ── */}
          {activeSection === 'terminal' && (
            <div className="grid grid-cols-4 gap-x-4 gap-y-3">
              {TERMINAL_COLOR_FIELDS.map(({ key, labelKey }) => (
                <ColorSwatch
                  key={key}
                  color={(termColors[key] as string) || '#000000'}
                  onChange={(hex) => updateTermColor(key, hex)}
                  label={t(`settings_view.custom_theme.colors.${labelKey}`)}
                />
              ))}
            </div>
          )}

          {/* ── UI Colors — Grouped Sections ── */}
          {activeSection === 'ui' && (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-xs text-theme-text-muted">{t('settings_view.custom_theme.ui_colors_hint')}</span>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 text-xs text-theme-text border-theme-border"
                  onClick={handleAutoDerive}
                >
                  <Copy className="w-3 h-3 mr-1" />
                  {t('settings_view.custom_theme.auto_derive')}
                </Button>
              </div>
              {UI_COLOR_SECTIONS.map((section) => (
                <div key={section.titleKey} className="space-y-2">
                  <h4 className="text-[11px] font-medium text-theme-text-muted uppercase tracking-wider border-b border-theme-border/40 pb-1">
                    {t(`settings_view.custom_theme.${section.titleKey}`)}
                  </h4>
                  <div className="grid grid-cols-4 gap-x-4 gap-y-3">
                    {section.fields.map(({ key, labelKey }) => (
                      <ColorSwatch
                        key={key}
                        color={uiColors[key]}
                        onChange={(hex) => updateUiColor(key, hex)}
                        label={t(`settings_view.custom_theme.colors.${labelKey}`)}
                      />
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        <DialogFooter>
          <div className="flex items-center justify-between w-full">
            <div>
              {isEditing && (
                <Button
                  variant="outline"
                  size="sm"
                  className="text-red-400 border-red-400/30 hover:bg-red-400/10 h-8 text-xs"
                  onClick={handleDelete}
                >
                  <Trash2 className="w-3 h-3 mr-1" />
                  {t('settings_view.custom_theme.delete')}
                </Button>
              )}
            </div>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                className="h-8 text-xs text-theme-text border-theme-border"
                onClick={() => onOpenChange(false)}
              >
                {t('settings_view.custom_theme.cancel')}
              </Button>
              <Button
                size="sm"
                className="h-8 text-xs"
                onClick={handleSave}
                disabled={!name.trim()}
              >
                <Save className="w-3 h-3 mr-1" />
                {t('settings_view.custom_theme.save')}
              </Button>
            </div>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
