// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState, useEffect, useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Dialog, DialogContent, DialogTitle } from '../ui/dialog';
import { Button } from '../ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { Slider } from '../ui/slider';
import { useSettingsStore, type FontFamily, type Language } from '../../store/settingsStore';
import { api, type CliCompanionStatus } from '../../lib/api';
import { useAppStore } from '../../store/appStore';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { platform } from '../../lib/platform';
import { getFontFamilyCSS } from '../fileManager/fontUtils';
import { getTerminalTheme } from '../../lib/themes';
import {
  Download,
  Check,
  Terminal,
  Plus,
  Loader2,
  ArrowUpDown,
  Shield,
  RefreshCw,
  ArrowRight,
  ArrowLeft,
  Globe,
  Palette,
  Command,
  Sparkles,
  SquareTerminal,
  Bot,
  Route,
  Keyboard,
  Waypoints,
  FolderOpen,
  Server,
  FileCode,
  Zap,
  Lock,
  Cpu,
  Puzzle,
  Lightbulb,
  HardDrive,
  Rocket,
  Monitor,
  ScrollText,
  AlertTriangle,
  Brain,
  ExternalLink,
} from 'lucide-react';

// ============================================================================
// Onboarding Wizard — Multi-step welcome with settings configuration
// ============================================================================

/** Curated themes for the onboarding picker (dark + light variety) */
const ONBOARDING_THEMES = [
  'default', 'oxide', 'dracula', 'nord',
  'catppuccin-mocha', 'tokyo-night', 'paper-oxide', 'rose-pine',
] as const;

/** Language display labels */
const LANGUAGE_OPTIONS: { value: Language; label: string }[] = [
  { value: 'en', label: 'English' },
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁體中文' },
  { value: 'ja', label: '日本語' },
  { value: 'ko', label: '한국어' },
  { value: 'fr-FR', label: 'Français' },
  { value: 'de', label: 'Deutsch' },
  { value: 'es-ES', label: 'Español' },
  { value: 'it', label: 'Italiano' },
  { value: 'pt-BR', label: 'Português (BR)' },
  { value: 'vi', label: 'Tiếng Việt' },
];

/** Font display labels */
const FONT_OPTIONS: { value: FontFamily; label: string; bundled: boolean }[] = [
  { value: 'jetbrains', label: 'JetBrains Mono NF (Subset)', bundled: true },
  { value: 'meslo', label: 'MesloLGM NF (Subset)', bundled: true },
  { value: 'maple', label: 'Maple Mono NF CN (Subset)', bundled: true },
  { value: 'cascadia', label: 'Cascadia Code', bundled: false },
  { value: 'consolas', label: 'Consolas', bundled: false },
  { value: 'menlo', label: 'Menlo', bundled: false },
];

const TOTAL_STEPS = 7; // 0..6

/** Mini terminal preview for theme cards */
const ThemeCard = ({
  themeId,
  selected,
  onClick,
}: {
  themeId: string;
  selected: boolean;
  onClick: () => void;
}) => {
  const theme = getTerminalTheme(themeId);
  const displayName = themeId.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');

  return (
    <button
      onClick={onClick}
      className={`group rounded-md border-2 overflow-hidden transition-all ${
        selected
          ? 'border-[var(--theme-accent)] ring-1 ring-[var(--theme-accent)]/30'
          : 'border-theme-border hover:border-theme-border-strong'
      }`}
    >
      <div className="p-2.5" style={{ backgroundColor: theme.background }}>
        <div className="flex gap-1.5 mb-1.5">
          <div className="w-2 h-2 rounded-full" style={{ backgroundColor: theme.red }} />
          <div className="w-2 h-2 rounded-full" style={{ backgroundColor: theme.yellow }} />
          <div className="w-2 h-2 rounded-full" style={{ backgroundColor: theme.green }} />
        </div>
        <div className="font-mono text-[10px] space-y-0.5 text-left" style={{ color: theme.foreground }}>
          <div>$ echo <span style={{ color: theme.green }}>"hi"</span></div>
          <div style={{ color: theme.blue }}>~</div>
        </div>
      </div>
      <div className="px-2.5 py-1.5 bg-theme-bg-panel border-t border-theme-border">
        <span className="text-[11px] font-medium text-theme-text">{displayName}</span>
      </div>
    </button>
  );
};

/** Font preview with configurable font family */
const FontPreviewBlock = ({ fontFamily, fontSize }: { fontFamily: string; fontSize: number }) => {
  return (
    <div
      className="rounded-md border border-theme-border bg-theme-bg-sunken p-4"
      style={{ fontFamily: getFontFamilyCSS(fontFamily), fontSize: `${fontSize}px` }}
    >
      <div className="text-theme-text leading-relaxed">
        <div>ABCDEFG abcdefg 0123456789</div>
        <div className="text-theme-text-muted">{'-> => == != <= >= {}'}</div>
        <div className="text-emerald-400">天地玄黄 The quick brown fox</div>
      </div>
    </div>
  );
};

export const OnboardingModal = () => {
  const { t } = useTranslation();
  const onboardingCompleted = useSettingsStore((s) => s.settings.onboardingCompleted);
  const completeOnboarding = useSettingsStore((s) => s.completeOnboarding);
  const language = useSettingsStore((s) => s.settings.general.language);
  const terminalTheme = useSettingsStore((s) => s.settings.terminal.theme);
  const fontFamily = useSettingsStore((s) => s.settings.terminal.fontFamily);
  const fontSize = useSettingsStore((s) => s.settings.terminal.fontSize);
  const setLanguage = useSettingsStore((s) => s.setLanguage);
  const updateTerminal = useSettingsStore((s) => s.updateTerminal);
  const { toggleModal } = useAppStore();
  const createLocalTerminal = useLocalTerminalStore((s) => s.createTerminal);
  const createTab = useAppStore((s) => s.createTab);

  const [open, setOpen] = useState(false);
  const [step, setStep] = useState(0);
  const [hostCount, setHostCount] = useState<number | null>(null);
  const [importState, setImportState] = useState<'idle' | 'loading' | 'done'>('idle');
  const [importedCount, setImportedCount] = useState(0);
  const [disclaimerAccepted, setDisclaimerAccepted] = useState(false);
  const [cliStatus, setCliStatus] = useState<(CliCompanionStatus & { error?: boolean }) | null>(null);
  const [cliInstalling, setCliInstalling] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!onboardingCompleted) {
      const timer = setTimeout(() => setOpen(true), 300);
      return () => clearTimeout(timer);
    }
  }, [onboardingCompleted]);

  // Reset state when dialog reopens
  useEffect(() => {
    if (open) {
      setStep(0);
      setHostCount(null);
      setImportState('idle');
      setImportedCount(0);
      setDisclaimerAccepted(false);
      setCliStatus(null);
      setCliInstalling(false);
    }
  }, [open]);

  // Scroll to top on step change
  useEffect(() => {
    contentRef.current?.scrollTo(0, 0);
  }, [step]);

  // Scan SSH config hosts when reaching the quick-start step
  useEffect(() => {
    if (!open || step !== 6) return;
    api.listSshConfigHosts()
      .then((hosts) => setHostCount(hosts.filter((h) => h.alias !== '*').length))
      .catch(() => setHostCount(0));
  }, [open, step]);

  // Fetch CLI companion status when reaching the CLI step
  useEffect(() => {
    if (!open || step !== 5) return;
    api.cliGetStatus()
      .then((status) => setCliStatus(status))
      .catch(() => setCliStatus({ bundled: false, installed: false, install_path: null, bundle_path: null, app_version: '0.0.0', matches_bundled: null, needs_reinstall: false, error: true }));
  }, [open, step]);

  const handleClose = useCallback(() => {
    setOpen(false);
    completeOnboarding();
  }, [completeOnboarding]);

  const handleOpenTerminal = useCallback(async () => {
    handleClose();
    try {
      const info = await createLocalTerminal();
      createTab('local_terminal', info.id);
    } catch { /* ignore */ }
  }, [handleClose, createLocalTerminal, createTab]);

  const handleNewConnection = useCallback(() => {
    handleClose();
    toggleModal('newConnection', true);
  }, [handleClose, toggleModal]);

  const handleImportAll = useCallback(async () => {
    setImportState('loading');
    try {
      const hosts = await api.listSshConfigHosts();
      const filtered = hosts.filter((h) => h.alias !== '*');
      let count = 0;
      for (const host of filtered) {
        try {
          await api.importSshHost(host.alias);
          count++;
        } catch { /* skip */ }
      }
      setImportedCount(count);
    } catch { /* ignore */ }
    setImportState('done');
  }, []);

  const handleCliInstall = useCallback(async () => {
    setCliInstalling(true);
    try {
      await api.cliInstall();
      const status = await api.cliGetStatus();
      setCliStatus(status);
    } catch {
      setCliStatus((prev) => prev ? { ...prev, error: true } : prev);
    }
    setCliInstalling(false);
  }, []);

  if (onboardingCompleted) return null;

  const isMac = platform.isMac;

  const importLabel =
    importState === 'done'
      ? t('onboarding.import_ssh_done', { count: importedCount })
      : hostCount === null
        ? t('onboarding.importing')
        : hostCount > 0
          ? t('onboarding.import_ssh_desc', { count: hostCount })
          : t('onboarding.import_ssh_none');

  const canGoNext = step < TOTAL_STEPS - 1;
  const canGoBack = step > 0;

  // ── Step renderers ────────────────────────────────────────────────────────

  /** Step 0 — Welcome + Language */
  const renderWelcome = () => (
    <div className="px-8 pt-8 pb-6 space-y-5">
      <div className="text-center select-none">
        <div className="flex items-center justify-center gap-1">
          <h2 className="text-3xl font-bold tracking-tight text-theme-text empty-brand">
            {t('onboarding.welcome')}
          </h2>
          <span className="inline-block w-[3px] h-[0.7em] rounded-sm bg-theme-text opacity-40 translate-y-[1px]" />
        </div>
        <p className="text-sm text-theme-text-muted mt-2">{t('onboarding.subtitle')}</p>
      </div>

      <div className="rounded-md border border-theme-border bg-theme-bg-panel p-4">
        <p className="text-sm text-theme-text leading-relaxed">{t('onboarding.project_intro')}</p>
      </div>

      {/* Core highlights */}
      <div className="grid grid-cols-2 gap-2">
        {([
          { icon: Zap, key: 'highlight_performance' },
          { icon: Lock, key: 'highlight_security_arch' },
          { icon: Cpu, key: 'highlight_crossplatform' },
          { icon: Puzzle, key: 'highlight_extensible' },
        ] as const).map((item) => (
          <div key={item.key} className="flex gap-2 p-2.5 rounded-md bg-theme-bg-panel/50 border border-theme-border/50">
            <item.icon className="h-3.5 w-3.5 mt-0.5 shrink-0 text-[var(--theme-accent)]" />
            <div className="min-w-0">
              <span className="text-xs font-medium text-theme-text">{t(`onboarding.${item.key}`)}</span>
              <p className="text-[10px] text-theme-text-muted leading-snug mt-0.5">{t(`onboarding.${item.key}_desc`)}</p>
            </div>
          </div>
        ))}
      </div>

      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <Globe className="h-4 w-4 text-[var(--theme-accent)]" />
          <span className="text-sm font-medium text-theme-text">{t('onboarding.select_language')}</span>
        </div>
        <div className="grid grid-cols-4 gap-1.5">
          {LANGUAGE_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              onClick={() => setLanguage(opt.value)}
              className={`px-3 py-2 rounded-sm text-xs font-medium transition-all ${
                language === opt.value
                  ? 'bg-[var(--theme-accent)] text-[var(--theme-accent-text)] shadow-sm'
                  : 'bg-theme-bg-panel border border-theme-border text-theme-text hover:border-theme-border-strong hover:bg-theme-bg-hover'
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );

  /** Step 2 — Appearance (Theme + Font) */
  const renderAppearance = () => (
    <div className="px-8 pt-6 pb-6 space-y-5">
      <div className="flex items-center gap-2">
        <Palette className="h-5 w-5 text-[var(--theme-accent)]" />
        <div>
          <h3 className="text-lg font-semibold text-theme-text">{t('onboarding.appearance_title')}</h3>
          <p className="text-xs text-theme-text-muted">{t('onboarding.appearance_desc')}</p>
        </div>
      </div>

      {/* Theme */}
      <div className="space-y-2.5">
        <span className="text-xs font-medium text-theme-text-muted uppercase tracking-wider">{t('onboarding.select_theme')}</span>
        <div className="grid grid-cols-4 gap-2">
          {ONBOARDING_THEMES.map((id) => (
            <ThemeCard
              key={id}
              themeId={id}
              selected={terminalTheme === id}
              onClick={() => updateTerminal('theme', id)}
            />
          ))}
        </div>
      </div>

      <div className="border-t border-theme-border/50" />

      {/* Font */}
      <div className="space-y-2.5">
        <div className="flex items-center justify-between">
          <span className="text-xs font-medium text-theme-text-muted uppercase tracking-wider">{t('onboarding.select_font')}</span>
          <span className="text-[10px] text-theme-text-muted">{t('onboarding.font_hint')}</span>
        </div>
        <div className="grid grid-cols-2 gap-4 items-start">
          <div className="space-y-3">
            <Select value={fontFamily} onValueChange={(val) => updateTerminal('fontFamily', val as FontFamily)}>
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {FONT_OPTIONS.map((f) => (
                  <SelectItem key={f.value} value={f.value}>
                    {f.label}{f.bundled ? ' ✓' : ''}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm text-theme-text">{t('onboarding.font_size')}</span>
                <span className="text-sm text-theme-text-muted font-mono">{fontSize}px</span>
              </div>
              <Slider
                min={8}
                max={32}
                step={1}
                value={fontSize}
                onChange={(v) => updateTerminal('fontSize', v)}
                className="w-full"
                aria-label={t('onboarding.font_size')}
              />
            </div>
          </div>
          <FontPreviewBlock fontFamily={fontFamily} fontSize={fontSize} />
        </div>
      </div>

      {/* Tip */}
      <div className="flex gap-2.5 p-3 rounded-md bg-[var(--theme-accent)]/5 border border-[var(--theme-accent)]/20">
        <Lightbulb className="h-3.5 w-3.5 mt-0.5 shrink-0 text-[var(--theme-accent)]" />
        <p className="text-xs text-theme-text-muted leading-relaxed">{t('onboarding.tip_settings', { shortcut: isMac ? '⌘,' : 'Ctrl+,' })}</p>
      </div>
    </div>
  );

  /** Step 3 — Core Workflow */
  const renderWorkflow = () => {
    const workflows = [
      { icon: Server, key: 'workflow_connect' },
      { icon: Terminal, key: 'workflow_terminal' },
      { icon: FolderOpen, key: 'workflow_sftp' },
      { icon: Waypoints, key: 'workflow_forwarding' },
      { icon: FileCode, key: 'workflow_ide' },
    ] as const;

    return (
      <div className="px-6 pt-6 pb-6 space-y-4">
        <div className="flex items-center gap-2">
          <Route className="h-5 w-5 text-[var(--theme-accent)]" />
          <div>
            <h3 className="text-lg font-semibold text-theme-text">{t('onboarding.workflow_title')}</h3>
            <p className="text-xs text-theme-text-muted">{t('onboarding.workflow_desc')}</p>
          </div>
        </div>

        <div className="relative space-y-0">
          {workflows.map((item, i) => (
            <div key={item.key} className="flex items-start gap-3 relative">
              {/* Vertical connector line */}
              {i < workflows.length - 1 && (
                <div className="absolute left-[13px] top-[28px] w-px h-[calc(100%-16px)] bg-theme-border" />
              )}
              {/* Step number circle */}
              <div className="relative z-10 flex items-center justify-center w-[28px] h-[28px] rounded-full bg-[var(--theme-accent)]/15 border border-[var(--theme-accent)]/30 shrink-0 mt-1.5">
                <span className="text-[11px] font-bold text-[var(--theme-accent)]">{i + 1}</span>
              </div>
              {/* Content */}
              <div className="flex-1 pb-4">
                <div className="flex items-center gap-2.5 rounded-md border border-theme-border bg-theme-bg-panel p-3">
                  <item.icon className="h-4 w-4 shrink-0 text-[var(--theme-accent)]" />
                  <div className="min-w-0">
                    <span className="text-xs font-medium text-theme-text">{t(`onboarding.${item.key}`)}</span>
                    <p className="text-[11px] text-theme-text-muted leading-snug mt-0.5">{t(`onboarding.${item.key}_desc`)}</p>
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>

        {/* Tip */}
        <div className="flex gap-2.5 p-3 rounded-md bg-[var(--theme-accent)]/5 border border-[var(--theme-accent)]/20">
          <Lightbulb className="h-3.5 w-3.5 mt-0.5 shrink-0 text-[var(--theme-accent)]" />
          <p className="text-xs text-theme-text-muted leading-relaxed">{t('onboarding.tip_multiplexing')}</p>
        </div>
      </div>
    );
  };

  /** Step 6 — Quick Start + Shortcuts */
  const renderQuickStart = () => {
    const mod = isMac ? '⌘' : 'Ctrl';
    const shortcutGroups = [
      {
        titleKey: 'shortcuts_navigation',
        items: [
          { keys: [`${mod}K`], descKey: 'shortcut_command_palette' },
          { keys: [`${mod}N`], descKey: 'shortcut_new_connection' },
          { keys: [`${mod}T`], descKey: 'shortcut_new_tab' },
        ],
      },
      {
        titleKey: 'shortcuts_terminal',
        items: [
          { keys: [`${mod}F`], descKey: 'shortcut_search' },
          { keys: [isMac ? '⌘⇧A' : 'Ctrl+Shift+A'], descKey: 'shortcut_ai_chat' },
          { keys: [`${mod}${isMac ? '⇧' : '+Shift+'}C`], descKey: 'shortcut_copy' },
        ],
      },
      {
        titleKey: 'shortcuts_window',
        items: [
          { keys: [`${mod}E`], descKey: 'shortcut_split_right' },
          { keys: [`${mod}D`], descKey: 'shortcut_split_down' },
          { keys: [`${mod}W`], descKey: 'shortcut_close_tab' },
          { keys: [`${mod}${isMac ? '' : '+'}=`, `${mod}${isMac ? '' : '+'}-`], descKey: 'shortcut_zoom' },
        ],
      },
    ];

    return (
      <div className="px-8 pt-6 pb-6 space-y-5">
        <div className="flex items-center gap-2">
          <Sparkles className="h-5 w-5 text-[var(--theme-accent)]" />
          <div>
            <h3 className="text-lg font-semibold text-theme-text">{t('onboarding.quick_start')}</h3>
            <p className="text-xs text-theme-text-muted">{t('onboarding.quick_start_desc')}</p>
          </div>
        </div>

        <div className="grid grid-cols-3 gap-3">
          <button
            onClick={handleOpenTerminal}
            className="group flex flex-col items-center gap-2.5 px-4 py-5 rounded-md border border-theme-border bg-theme-bg-panel hover:border-[var(--theme-accent)] hover:bg-theme-bg-hover transition-colors"
          >
            <Terminal className="h-6 w-6 text-theme-text-muted group-hover:text-[var(--theme-accent)] transition-colors" />
            <div className="text-center">
              <div className="text-sm font-medium text-theme-text">{t('onboarding.open_terminal')}</div>
              <div className="text-xs text-theme-text-muted mt-1 leading-relaxed">{t('onboarding.open_terminal_desc')}</div>
            </div>
          </button>

          <button
            onClick={handleNewConnection}
            className="group flex flex-col items-center gap-2.5 px-4 py-5 rounded-md border border-theme-border bg-theme-bg-panel hover:border-[var(--theme-accent)] hover:bg-theme-bg-hover transition-colors"
          >
            <Plus className="h-6 w-6 text-theme-text-muted group-hover:text-[var(--theme-accent)] transition-colors" />
            <div className="text-center">
              <div className="text-sm font-medium text-theme-text">{t('onboarding.new_connection')}</div>
              <div className="text-xs text-theme-text-muted mt-1 leading-relaxed">{t('onboarding.new_connection_desc')}</div>
            </div>
          </button>

          <button
            onClick={importState === 'idle' && hostCount ? handleImportAll : undefined}
            disabled={importState !== 'idle' || !hostCount}
            className="group flex flex-col items-center gap-2.5 px-4 py-5 rounded-md border border-theme-border bg-theme-bg-panel hover:border-[var(--theme-accent)] hover:bg-theme-bg-hover disabled:opacity-50 disabled:cursor-default disabled:hover:border-theme-border disabled:hover:bg-theme-bg-panel transition-colors"
          >
            {importState === 'loading' || hostCount === null ? (
              <Loader2 className="h-6 w-6 text-theme-text-muted animate-spin" />
            ) : importState === 'done' ? (
              <Check className="h-6 w-6 text-green-500" />
            ) : (
              <Download className="h-6 w-6 text-theme-text-muted group-hover:text-[var(--theme-accent)] transition-colors" />
            )}
            <div className="text-center">
              <div className="text-sm font-medium text-theme-text">{t('onboarding.import_ssh')}</div>
              <div className="text-xs text-theme-text-muted mt-1 leading-relaxed">{importLabel}</div>
            </div>
          </button>
        </div>

        <div className="border-t border-theme-border/50" />

        {/* Shortcuts */}
        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <Keyboard className="h-4 w-4 text-[var(--theme-accent)]" />
            <span className="text-xs font-medium text-theme-text-muted uppercase tracking-wider">{t('onboarding.shortcuts_title')}</span>
          </div>
          <div className="grid grid-cols-3 gap-4">
            {shortcutGroups.map((group) => (
              <div key={group.titleKey} className="space-y-2">
                <span className="text-[10px] font-semibold text-theme-text-muted uppercase tracking-wider">
                  {t(`onboarding.${group.titleKey}`)}
                </span>
                <div className="space-y-1.5">
                  {group.items.map((item) => (
                    <div key={item.descKey} className="flex items-start gap-2">
                      <div className="flex items-center gap-0.5 shrink-0">
                        {item.keys.map((k, ki) => (
                          <span key={ki}>
                            {ki > 0 && <span className="text-[9px] text-theme-text-muted mx-0.5">/</span>}
                            <kbd className="inline-flex items-center px-1.5 py-0.5 rounded bg-theme-bg border border-theme-border text-theme-text-muted font-mono text-[10px] leading-tight shadow-sm">
                              {k}
                            </kbd>
                          </span>
                        ))}
                      </div>
                      <span className="text-[11px] text-theme-text leading-snug mt-0.5">
                        {t(`onboarding.${item.descKey}`)}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Tip */}
        <div className="flex gap-2.5 p-3 rounded-md bg-[var(--theme-accent)]/5 border border-[var(--theme-accent)]/20">
          <Lightbulb className="h-3.5 w-3.5 mt-0.5 shrink-0 text-[var(--theme-accent)]" />
          <p className="text-xs text-theme-text-muted leading-relaxed">{t('onboarding.tip_shortcuts', { shortcut: isMac ? '⌘/' : 'Ctrl+/' })}</p>
        </div>
      </div>
    );
  };

  /** Step 4 — Features */
  const renderFeatures = () => (
    <div className="px-8 pt-6 pb-6 space-y-4">
      <div className="flex items-center gap-2">
        <Shield className="h-5 w-5 text-[var(--theme-accent)]" />
        <div>
          <h3 className="text-lg font-semibold text-theme-text">{t('onboarding.features')}</h3>
          <p className="text-xs text-theme-text-muted">{t('onboarding.features_desc')}</p>
        </div>
      </div>
      <div className="grid grid-cols-2 gap-2.5">
        {([
          { icon: Command, key: 'cmd_palette', shortcut: isMac ? '⌘K' : 'Ctrl+K', platform: null },
          { icon: Bot, key: 'ai_chat', shortcut: null, platform: null },
          { icon: FolderOpen, key: 'sftp', shortcut: null, platform: null },
          { icon: HardDrive, key: 'local_file_manager', shortcut: null, platform: null },
          { icon: Waypoints, key: 'port_forwarding', shortcut: null, platform: null },
          { icon: RefreshCw, key: 'reconnect', shortcut: null, platform: null },
          { icon: Puzzle, key: 'plugin_system', shortcut: null, platform: null },
          { icon: FileCode, key: 'custom_themes', shortcut: null, platform: null },
          { icon: Rocket, key: 'launchpad', shortcut: null, platform: 'macOS' },
          { icon: Monitor, key: 'wsl_graphics', shortcut: null, platform: 'Windows' },
          { icon: ArrowUpDown, key: 'multiplexing', shortcut: null, platform: null },
          { icon: Shield, key: 'security', shortcut: null, platform: null },
        ] as const).map((item) => (
          <div key={item.key} className="flex gap-2.5 p-3.5 rounded-md border border-theme-border bg-theme-bg-panel">
            <item.icon className="h-4 w-4 mt-0.5 shrink-0 text-[var(--theme-accent)]" />
            <div className="min-w-0">
              <div className="flex items-center gap-1.5">
                <span className="text-xs font-medium text-theme-text">{t(`onboarding.${item.key}`)}</span>
                {item.platform && (
                  <span className="px-1 py-0.5 rounded-sm bg-theme-bg border border-theme-border text-theme-text-muted text-[9px] leading-tight">
                    {item.platform}
                  </span>
                )}
                {item.shortcut && (
                  <kbd className="px-1 py-0.5 rounded-sm bg-theme-bg border border-theme-border text-theme-text-muted font-mono text-[9px] leading-tight">
                    {item.shortcut}
                  </kbd>
                )}
              </div>
              <p className="text-[11px] text-theme-text-muted mt-0.5 leading-relaxed">{t(`onboarding.${item.key}_desc`)}</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  );

  /** Step 5 — CLI Companion */
  const renderCliCompanion = () => (
    <div className="px-8 pt-6 pb-6 space-y-4">
      <div className="flex items-center gap-2">
        <SquareTerminal className="h-5 w-5 text-[var(--theme-accent)]" />
        <div>
          <h3 className="text-lg font-semibold text-theme-text">{t('onboarding.cli_step_title')}</h3>
          <p className="text-xs text-theme-text-muted">{t('onboarding.cli_step_desc')}</p>
        </div>
      </div>

      {/* What is oxt? */}
      <div className="flex gap-3 p-3.5 rounded-md border border-theme-border bg-theme-bg-panel">
        <Terminal className="h-4 w-4 mt-0.5 shrink-0 text-[var(--theme-accent)]" />
        <div className="min-w-0">
          <span className="text-xs font-medium text-theme-text">{t('onboarding.cli_step_what')}</span>
          <p className="text-[11px] text-theme-text-muted mt-0.5 leading-relaxed">{t('onboarding.cli_step_what_text')}</p>
        </div>
      </div>

      {/* Capabilities */}
      <div className="space-y-2">
        <span className="text-xs font-medium text-theme-text-muted">{t('onboarding.cli_step_capabilities')}</span>
        <div className="grid grid-cols-2 gap-2">
          {([
            { icon: Server, key: 'cli_step_cap_sessions' },
            { icon: Waypoints, key: 'cli_step_cap_forward' },
            { icon: Bot, key: 'cli_step_cap_ai' },
            { icon: Cpu, key: 'cli_step_cap_status' },
          ] as const).map((item) => (
            <div key={item.key} className="flex gap-2 p-2.5 rounded-md bg-theme-bg-panel/50 border border-theme-border/50">
              <item.icon className="h-3.5 w-3.5 mt-0.5 shrink-0 text-[var(--theme-accent)]" />
              <span className="text-[11px] text-theme-text leading-snug">{t(`onboarding.${item.key}`)}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Install action */}
      <div className="flex items-center gap-3 p-3.5 rounded-md border border-theme-border bg-theme-bg-panel">
        <div className="flex-1 min-w-0">
          {cliStatus?.installed && cliStatus.needs_reinstall ? (
            <div className="flex items-center gap-2">
              <AlertTriangle className="h-4 w-4 text-amber-400" />
              <div>
                <span className="text-xs font-medium text-amber-400">{t('onboarding.cli_step_reinstall_required')}</span>
                {cliStatus.install_path && (
                  <p className="text-[11px] text-theme-text-muted mt-0.5 font-mono">{t('onboarding.cli_step_installed_at', { path: cliStatus.install_path })}</p>
                )}
                <p className="text-[11px] text-theme-text-muted mt-0.5">{t('onboarding.cli_step_reinstall_hint', { version: cliStatus.app_version })}</p>
              </div>
            </div>
          ) : cliStatus?.installed ? (
            <div className="flex items-center gap-2">
              <Check className="h-4 w-4 text-emerald-400" />
              <div>
                <span className="text-xs font-medium text-emerald-400">{t('onboarding.cli_step_installed')}</span>
                {cliStatus.install_path && (
                  <p className="text-[11px] text-theme-text-muted mt-0.5 font-mono">{t('onboarding.cli_step_installed_at', { path: cliStatus.install_path })}</p>
                )}
              </div>
            </div>
          ) : cliStatus?.error ? (
            <div className="flex items-center gap-2">
              <AlertTriangle className="h-4 w-4 text-amber-400 shrink-0" />
              <span className="text-xs text-theme-text-muted flex-1">{t('onboarding.cli_step_install_error')}</span>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  setCliStatus(null);
                  api.cliGetStatus()
                    .then((status) => setCliStatus(status))
                    .catch(() => setCliStatus({ bundled: false, installed: false, install_path: null, bundle_path: null, app_version: '0.0.0', matches_bundled: null, needs_reinstall: false, error: true }));
                }}
                className="gap-1 shrink-0"
              >
                <RefreshCw className="h-3 w-3" />
                {t('onboarding.cli_step_retry')}
              </Button>
            </div>
          ) : !cliStatus?.bundled ? (
            <span className="text-xs text-theme-text-muted">{t('onboarding.cli_step_not_bundled')}</span>
          ) : !cliStatus?.installed ? (
            <Button
              size="sm"
              onClick={handleCliInstall}
              disabled={cliInstalling}
              className="gap-1.5"
            >
              {cliInstalling ? (
                <>
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {cliStatus?.needs_reinstall ? t('onboarding.cli_step_reinstalling') : t('onboarding.cli_step_installing')}
                </>
              ) : (
                <>
                  <Download className="h-3.5 w-3.5" />
                  {cliStatus?.needs_reinstall ? t('onboarding.cli_step_reinstall') : t('onboarding.cli_step_install')}
                </>
              )}
            </Button>
          ) : null}
        </div>
        {cliStatus?.bundled && cliStatus.needs_reinstall && (
          <Button
            size="sm"
            onClick={handleCliInstall}
            disabled={cliInstalling}
            className="gap-1.5 shrink-0"
          >
            {cliInstalling ? (
              <>
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {t('onboarding.cli_step_reinstalling')}
              </>
            ) : (
              <>
                <Download className="h-3.5 w-3.5" />
                {t('onboarding.cli_step_reinstall')}
              </>
            )}
          </Button>
          )}
      </div>

      {/* Skip hint */}
      <div className="flex gap-2.5 p-3 rounded-md bg-theme-bg-sunken border border-theme-border">
        <Lightbulb className="h-3.5 w-3.5 mt-0.5 shrink-0 text-theme-text-muted" />
        <p className="text-[11px] text-theme-text-muted leading-relaxed">{t('onboarding.cli_step_skip_hint')}</p>
      </div>
    </div>
  );

  /** Step 1 — Disclaimer */
  const renderDisclaimer = () => (
    <div className="px-8 pt-6 pb-6 space-y-4">
      <div className="flex items-center gap-2">
        <ScrollText className="h-5 w-5 text-[var(--theme-accent)]" />
        <div>
          <h3 className="text-lg font-semibold text-theme-text">{t('onboarding.disclaimer_title')}</h3>
          <p className="text-xs text-theme-text-muted">{t('onboarding.disclaimer_desc')}</p>
        </div>
      </div>

      <div className="space-y-3">
        {/* No Warranty */}
        <div className="flex gap-3 p-3.5 rounded-md border border-theme-border bg-theme-bg-panel">
          <AlertTriangle className="h-4 w-4 mt-0.5 shrink-0 text-amber-400" />
          <div className="min-w-0">
            <span className="text-xs font-medium text-theme-text">{t('onboarding.disclaimer_no_warranty')}</span>
            <p className="text-[11px] text-theme-text-muted mt-0.5 leading-relaxed">{t('onboarding.disclaimer_no_warranty_text')}</p>
          </div>
        </div>

        {/* Data & Security */}
        <div className="flex gap-3 p-3.5 rounded-md border border-theme-border bg-theme-bg-panel">
          <Shield className="h-4 w-4 mt-0.5 shrink-0 text-blue-400" />
          <div className="min-w-0">
            <span className="text-xs font-medium text-theme-text">{t('onboarding.disclaimer_data_security')}</span>
            <p className="text-[11px] text-theme-text-muted mt-0.5 leading-relaxed">{t('onboarding.disclaimer_data_security_text')}</p>
          </div>
        </div>

        {/* AI Features */}
        <div className="flex gap-3 p-3.5 rounded-md border border-theme-border bg-theme-bg-panel">
          <Brain className="h-4 w-4 mt-0.5 shrink-0 text-purple-400" />
          <div className="min-w-0">
            <span className="text-xs font-medium text-theme-text">{t('onboarding.disclaimer_ai')}</span>
            <p className="text-[11px] text-theme-text-muted mt-0.5 leading-relaxed">{t('onboarding.disclaimer_ai_text')}</p>
          </div>
        </div>
      </div>

      {/* GPL note */}
      <a
        href="https://github.com/AnalyseDeCircuit/oxideterm/blob/main/DISCLAIMER.md"
        target="_blank"
        rel="noopener noreferrer"
        className="flex gap-2.5 p-3 rounded-md bg-theme-bg-sunken border border-theme-border hover:bg-theme-bg-hover transition-colors group"
      >
        <ExternalLink className="h-3.5 w-3.5 mt-0.5 shrink-0 text-theme-text-muted" />
        <p className="text-[11px] text-theme-text-muted leading-relaxed group-hover:text-theme-text transition-colors">{t('onboarding.disclaimer_gpl_note')}</p>
      </a>

      {/* Accept checkbox */}
      <label className="flex items-start gap-3 p-3 rounded-md border border-theme-border bg-theme-bg-panel cursor-pointer hover:bg-theme-bg-hover transition-colors select-none">
        <input
          type="checkbox"
          checked={disclaimerAccepted}
          onChange={(e) => setDisclaimerAccepted(e.target.checked)}
          className="mt-0.5 rounded border-theme-border accent-[var(--theme-accent)]"
        />
        <span className="text-xs text-theme-text leading-relaxed">{t('onboarding.disclaimer_accept')}</span>
      </label>
    </div>
  );

  const STEP_ICONS = [Globe, ScrollText, Palette, Route, Shield, SquareTerminal, Sparkles];
  const STEP_TITLE_KEYS = ['welcome', 'disclaimer_title', 'appearance_title', 'workflow_title', 'features', 'cli_step_title', 'quick_start'];
  const stepRenderers = [renderWelcome, renderDisclaimer, renderAppearance, renderWorkflow, renderFeatures, renderCliCompanion, renderQuickStart];

  return (
    <Dialog open={open} onOpenChange={(v) => { if (!v && disclaimerAccepted) handleClose(); }}>
      <DialogContent className="sm:max-w-[800px] p-0 gap-0 overflow-hidden" onPointerDownOutside={(e) => { if (!disclaimerAccepted) e.preventDefault(); }} onEscapeKeyDown={(e) => { if (!disclaimerAccepted) e.preventDefault(); }}>
        <DialogTitle className="sr-only">{t('onboarding.welcome')}</DialogTitle>

        {/* ── Progress indicator ─────────────────────────────── */}
        <div className="flex items-center justify-center gap-1.5 pt-5 pb-1 select-none">
          {Array.from({ length: TOTAL_STEPS }, (_, i) => {
            const Icon = STEP_ICONS[i];
            return (
              <button
                key={i}
                onClick={() => { if (i <= 1 || disclaimerAccepted) setStep(i); }}
                aria-disabled={!disclaimerAccepted && i > 1}
                className={`flex items-center justify-center w-7 h-7 rounded-full transition-all focus-visible:ring-2 focus-visible:ring-[var(--theme-accent)] focus-visible:outline-none ${!disclaimerAccepted && i > 1 ? 'opacity-40 cursor-not-allowed ' : ''}${
                  i === step
                    ? 'bg-[var(--theme-accent)] text-[var(--theme-accent-text)] scale-110'
                    : i < step
                      ? 'bg-[var(--theme-accent)]/20 text-[var(--theme-accent)]'
                      : 'bg-theme-bg-panel text-theme-text-muted border border-theme-border'
                }`}
                aria-label={t(`onboarding.${STEP_TITLE_KEYS[i]}`)}
                aria-current={i === step ? 'step' : undefined}
              >
                <Icon className="h-3.5 w-3.5" />
              </button>
            );
          })}
        </div>

        {/* ── Step content ───────────────────────────────────── */}
        <div ref={contentRef} className="overflow-y-auto">
          {stepRenderers[step]()}
        </div>

        {/* ── Footer ─────────────────────────────────────────── */}
        <div className="flex items-center justify-between px-8 py-4 border-t border-theme-border bg-theme-bg-panel">
          <div className="flex items-center gap-2">
            {canGoBack && (
              <Button variant="ghost" size="sm" onClick={() => setStep(step - 1)} className="gap-1.5">
                <ArrowLeft className="h-3.5 w-3.5" />
                {t('onboarding.back')}
              </Button>
            )}
          </div>
          <div className="flex items-center gap-2">
            {canGoNext && disclaimerAccepted && (
              <Button variant="ghost" size="sm" onClick={() => setStep(TOTAL_STEPS - 1)} className="text-theme-text-muted">
                {t('onboarding.skip')}
              </Button>
            )}
            {canGoNext ? (
              <Button size="sm" onClick={() => setStep(step + 1)} disabled={step === 1 && !disclaimerAccepted} className="gap-1.5">
                {t('onboarding.next')}
                <ArrowRight className="h-3.5 w-3.5" />
              </Button>
            ) : (
              <Button size="sm" onClick={handleClose} disabled={!disclaimerAccepted} className="gap-1.5">
                {t('onboarding.start_exploring')}
                <ArrowRight className="h-3.5 w-3.5" />
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
};
