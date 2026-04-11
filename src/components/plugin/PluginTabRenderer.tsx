// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Plugin Tab Renderer
 *
 * Wraps plugin-provided tab components in an ErrorBoundary.
 * Looks up the component from pluginStore.tabViews by pluginTabId.
 */

import { useTranslation } from 'react-i18next';
import i18n from 'i18next';
import { usePluginStore } from '../../store/pluginStore';
import { ErrorBoundary } from '../ErrorBoundary';
import type { Tab } from '../../types';

type PluginTabRendererProps = {
  pluginTabId: string;
  tab: Tab;
};

export function PluginTabRenderer({ pluginTabId }: PluginTabRendererProps) {
  const { t } = useTranslation();
  const tabView = usePluginStore((state) => state.tabViews.get(pluginTabId));
  const language = i18n.language;

  if (!tabView) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        <div className="text-center space-y-2">
          <p className="text-lg">🧩</p>
          <p>{t('plugin.tab_not_available', 'Plugin tab view not available')}</p>
          <p className="text-xs opacity-60">{pluginTabId}</p>
        </div>
      </div>
    );
  }

  const Component = tabView.component;

  return (
    <ErrorBoundary
      fallback={
        <div className="flex items-center justify-center h-full text-destructive">
          <div className="text-center space-y-2">
            <p className="text-lg">⚠️</p>
            <p>{t('plugin.tab_crashed', 'Plugin tab crashed')}</p>
            <p className="text-xs opacity-60">{pluginTabId}</p>
          </div>
        </div>
      }
    >
      <Component
        tabId={tabView.tabId}
        pluginId={tabView.pluginId}
        language={language}
      />
    </ErrorBoundary>
  );
}
