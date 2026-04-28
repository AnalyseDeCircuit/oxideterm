// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { ShieldAlert, ShieldCheck, Settings } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useAiChatStore } from '../../store/aiChatStore';
import { useConfirm } from '../../hooks/useConfirm';
import { cn } from '../../lib/utils';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';

type AiSafetyModeIndicatorProps = {
  onOpenToolSettings: () => void;
};

export function AiSafetyModeIndicator({ onOpenToolSettings }: AiSafetyModeIndicatorProps) {
  const { t } = useTranslation();
  const activeConversationId = useAiChatStore((s) => s.activeConversationId);
  const mode = useAiChatStore((s) => (
    activeConversationId ? s.safetyModeByConversationId[activeConversationId] ?? 'default' : 'default'
  ));
  const setConversationSafetyMode = useAiChatStore((s) => s.setConversationSafetyMode);
  const { confirm, ConfirmDialog } = useConfirm();
  const isBypass = mode === 'bypass';

  const setMode = async (nextMode: 'default' | 'bypass') => {
    if (!activeConversationId) return;
    if (nextMode === 'default') {
      setConversationSafetyMode(activeConversationId, 'default');
      return;
    }
    if (mode === 'bypass') return;

    const approved = await confirm({
      title: t('ai.safety_mode.confirm_title'),
      description: t('ai.safety_mode.confirm_description'),
      confirmLabel: t('ai.safety_mode.confirm_enable'),
      cancelLabel: t('ai.safety_mode.confirm_cancel'),
      variant: 'danger',
    });
    if (approved) {
      setConversationSafetyMode(activeConversationId, 'bypass');
    }
  };

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className={cn(
              'flex shrink-0 items-center gap-1 rounded-md px-1 py-0.5 text-[10px] font-medium leading-none transition-colors whitespace-nowrap',
              isBypass
                ? 'border border-amber-500/30 bg-amber-500/10 text-amber-300 hover:bg-amber-500/15'
                : 'text-theme-text-muted hover:bg-theme-accent/10 hover:text-theme-text',
            )}
            title={isBypass ? t('ai.safety_mode.bypass_title') : t('ai.safety_mode.default_title')}
            aria-label={isBypass ? t('ai.safety_mode.bypass_title') : t('ai.safety_mode.default_title')}
          >
            {isBypass
              ? <ShieldAlert className="h-2.5 w-2.5 shrink-0 text-amber-300" />
              : <ShieldCheck className="h-2.5 w-2.5 shrink-0 text-theme-accent" />}
            <span>{isBypass ? t('ai.safety_mode.bypass_label') : t('ai.safety_mode.default_label')}</span>
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" side="top" className="w-64">
          <DropdownMenuLabel className="text-xs">
            {t('ai.safety_mode.menu_title')}
          </DropdownMenuLabel>
          <DropdownMenuItem
            onSelect={() => void setMode('default')}
            className="flex items-start gap-2 text-xs"
          >
            <ShieldCheck className="mt-0.5 h-3.5 w-3.5 shrink-0 text-theme-accent" />
            <span className="flex min-w-0 flex-col gap-0.5">
              <span className="font-medium text-theme-text">{t('ai.safety_mode.default_mode')}</span>
              <span className="text-[10px] leading-relaxed text-theme-text-muted">{t('ai.safety_mode.default_desc')}</span>
            </span>
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={() => void setMode('bypass')}
            className="flex items-start gap-2 text-xs"
          >
            <ShieldAlert className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-300" />
            <span className="flex min-w-0 flex-col gap-0.5">
              <span className="font-medium text-amber-300">{t('ai.safety_mode.bypass_mode')}</span>
              <span className="text-[10px] leading-relaxed text-theme-text-muted">{t('ai.safety_mode.bypass_desc')}</span>
            </span>
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={onOpenToolSettings} className="flex items-center gap-2 text-xs">
            <Settings className="h-3.5 w-3.5 text-theme-text-muted" />
            {t('ai.safety_mode.open_settings')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      {ConfirmDialog}
    </>
  );
}
