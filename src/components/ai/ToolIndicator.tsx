// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * ToolIndicator — lightweight tool-use status entry.
 *
 * Target-first tool planning means the chat UI should not imply that a fixed
 * global tool list is currently exposed to the model. Detailed tool controls
 * live in Settings; this badge only reports the global/session state and opens
 * the relevant settings section.
 */

import { useTranslation } from 'react-i18next';
import { Settings, Wrench } from 'lucide-react';
import { useSettingsStore } from '../../store/settingsStore';
import { cn } from '../../lib/utils';

type ToolIndicatorProps = {
  onOpenSettings: () => void;
};

export const ToolIndicator = ({ onOpenSettings }: ToolIndicatorProps) => {
  const { t } = useTranslation();
  const toolUse = useSettingsStore((s) => s.settings.ai.toolUse);

  const toolUseEnabled = toolUse?.enabled === true;
  const maxRounds = toolUse?.maxRounds ?? 10;
  const autoApprovedCount = Object.values(toolUse?.autoApproveTools ?? {}).filter(Boolean).length;
  const roundsLabel = t('ai.tool_status.rounds_short', {
    count: maxRounds,
    defaultValue: `${maxRounds}`,
  });
  const statusLabel = toolUseEnabled
    ? t('ai.tool_status.enabled_with_rounds', {
      count: maxRounds,
      defaultValue: `${t('ai.tool_status.enabled')} · ${maxRounds}`,
    })
    : t('ai.tool_status.disabled');
  const policyTitle = toolUseEnabled
    ? t('ai.tool_status.open_settings_with_policy', {
      count: maxRounds,
      autoApprovedCount,
      defaultValue: `Tool calls enabled. Max rounds: ${maxRounds}. Auto-approved capability groups: ${autoApprovedCount}. Open AI safety settings.`,
    })
    : t('ai.tool_status.open_settings');

  return (
    <button
      type="button"
      onClick={onOpenSettings}
      className={cn(
        'flex shrink-0 items-center gap-1 rounded-md px-1 py-0.5 text-[10px] font-medium leading-none transition-colors whitespace-nowrap',
        'text-theme-text-muted hover:bg-theme-accent/10 hover:text-theme-text',
        !toolUseEnabled && 'opacity-70',
      )}
      title={policyTitle}
      aria-label={policyTitle}
    >
      <Wrench className={cn('h-2.5 w-2.5 shrink-0', toolUseEnabled ? 'text-theme-accent' : 'text-theme-text-muted')} />
      <span className="sr-only">{statusLabel}</span>
      <span aria-hidden="true">{toolUseEnabled ? roundsLabel : t('ai.tool_status.disabled_short', { defaultValue: t('ai.tool_status.disabled') })}</span>
      <Settings className="h-2.5 w-2.5 shrink-0 opacity-70" />
    </button>
  );
};
