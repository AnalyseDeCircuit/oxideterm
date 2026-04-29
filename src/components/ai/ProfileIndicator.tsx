// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { Layers } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select';
import { useAiChatStore } from '@/store/aiChatStore';
import { useSettingsStore } from '@/store/settingsStore';

export function ProfileIndicator() {
  const { t } = useTranslation();
  const activeConversationId = useAiChatStore((state) => state.activeConversationId);
  const conversation = useAiChatStore((state) => state.conversations.find((item) => item.id === state.activeConversationId));
  const setConversationProfile = useAiChatStore((state) => state.setConversationProfile);
  const profilesConfig = useSettingsStore((state) => state.settings.ai.executionProfiles);
  const profiles = profilesConfig?.profiles ?? [];
  const defaultProfileId = profilesConfig?.defaultProfileId ?? profiles[0]?.id;
  const value = conversation?.profileId ?? conversation?.sessionMetadata?.profileId ?? defaultProfileId ?? 'default';
  const activeProfile = profiles.find((profile) => profile.id === value);

  if (!activeConversationId || profiles.length <= 1) {
    return null;
  }

  const activeProfileName = activeProfile?.name ?? t('ai.profile.default', { defaultValue: 'Default' });

  return (
    <Select
      value={value}
      onValueChange={(profileId) => void setConversationProfile(activeConversationId, profileId)}
    >
      <SelectTrigger
        aria-label={t('ai.profile.select', { defaultValue: 'Select execution profile' })}
        title={t('ai.profile.active', {
          defaultValue: 'Execution profile: {{profile}}',
          profile: activeProfileName,
        })}
        className="h-7 w-7 shrink-0 rounded-full border-theme-border/40 bg-theme-bg-card/60 p-0 text-theme-accent/80 hover:bg-theme-bg-hover/70 hover:text-theme-accent"
      >
        <Layers className="h-3.5 w-3.5" />
      </SelectTrigger>
      <SelectContent>
        {profiles.map((profile) => (
          <SelectItem key={profile.id} value={profile.id}>
            <span className="flex items-center gap-2">
              <span>{profile.name}</span>
              {profile.id === defaultProfileId ? (
                <span className="text-xs text-theme-text-muted">
                  {t('ai.profile.default_badge', { defaultValue: 'Default' })}
                </span>
              ) : null}
            </span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
