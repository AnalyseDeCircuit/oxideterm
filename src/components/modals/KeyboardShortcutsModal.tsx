// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState, useMemo, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Dialog, DialogContent, DialogTitle } from '../ui/dialog';
import { Search, Keyboard } from 'lucide-react';
import { platform } from '@/lib/platform';
import { getShortcutCategories } from '@/lib/shortcuts';
import { useSettingsStore } from '@/store/settingsStore';
import { getFontFamilyCSS } from '@/components/fileManager/fontUtils';

// ============================================================================
// KeyboardShortcutsModal — ⌘/ (Ctrl+/) to open
// ============================================================================

type KeyboardShortcutsModalProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export const KeyboardShortcutsModal = ({ open, onOpenChange }: KeyboardShortcutsModalProps) => {
  const { t } = useTranslation();
  const [query, setQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);
  const isMac = platform.isMac;

  const categories = useMemo(() => getShortcutCategories(t), [t]);

  // Terminal font for shortcut key display
  const { fontFamily, customFontFamily } = useSettingsStore(
    (s) => s.settings.terminal
  );
  const terminalFontCSS = useMemo(
    () =>
      fontFamily === 'custom' && customFontFamily
        ? customFontFamily
        : getFontFamilyCSS(fontFamily),
    [fontFamily, customFontFamily]
  );

  // Filter by search query
  const filtered = useMemo(() => {
    if (!query.trim()) return categories;
    const q = query.toLowerCase();
    return categories
      .map((cat) => ({
        ...cat,
        shortcuts: cat.shortcuts.filter(
          (s) =>
            s.label.toLowerCase().includes(q) ||
            s.mac.toLowerCase().includes(q) ||
            s.other.toLowerCase().includes(q)
        ),
      }))
      .filter((cat) => cat.shortcuts.length > 0);
  }, [categories, query]);

  // Focus search on open
  useEffect(() => {
    if (open) {
      setQuery('');
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[600px] p-0 gap-0 overflow-hidden">
        {/* Hidden title for accessibility */}
        <DialogTitle className="sr-only">{t('shortcuts_modal.title')}</DialogTitle>

        {/* Search bar */}
        <div className="flex items-center border-b border-theme-border px-4">
          <Search className="h-4 w-4 text-theme-text-muted shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t('shortcuts_modal.search_placeholder')}
            className="flex-1 h-11 px-3 bg-transparent text-sm text-theme-text placeholder:text-theme-text-muted outline-none border-0"
          />
          <kbd className="px-1.5 py-0.5 rounded-sm bg-theme-bg border border-theme-border text-theme-text-muted text-[10px] shrink-0" style={{ fontFamily: terminalFontCSS }}>
            {isMac ? '⌘/' : 'Ctrl+/'}
          </kbd>
        </div>

        {/* Shortcuts list */}
        <div className="max-h-[420px] overflow-y-auto py-2 px-4">
          {filtered.length === 0 ? (
            <div className="py-10 text-center text-sm text-theme-text-muted">
              {t('shortcuts_modal.no_results')}
            </div>
          ) : (
            <div className="space-y-4">
              {filtered.map((cat) => (
                <section key={cat.id}>
                  <h3 className="text-xs font-medium text-theme-text-muted uppercase tracking-wider mb-2 flex items-center gap-1.5">
                    <Keyboard className="h-3 w-3" />
                    {cat.title}
                  </h3>
                  <div className="space-y-0.5">
                    {cat.shortcuts.map((shortcut, i) => (
                      <div
                        key={i}
                        className={`flex items-center justify-between py-1.5 ${
                          i < cat.shortcuts.length - 1 ? 'border-b border-theme-border/20' : ''
                        }`}
                      >
                        <span className="text-sm text-theme-text">{shortcut.label}</span>
                        <kbd className="px-2 py-0.5 rounded-sm bg-theme-bg-panel border border-theme-border text-theme-text-muted text-xs" style={{ fontFamily: terminalFontCSS }}>
                          {isMac ? shortcut.mac : shortcut.other}
                        </kbd>
                      </div>
                    ))}
                  </div>
                </section>
              ))}
            </div>
          )}
        </div>

        {/* Footer hint */}
        <div className="flex items-center justify-between px-4 py-3 border-t border-theme-border bg-theme-bg-panel text-[11px] text-theme-text-muted">
          <span>{t('shortcuts_modal.footer_hint')}</span>
          <span style={{ fontFamily: terminalFontCSS }}>{filtered.reduce((n, c) => n + c.shortcuts.length, 0)} {t('shortcuts_modal.shortcut_count')}</span>
        </div>
      </DialogContent>
    </Dialog>
  );
};
