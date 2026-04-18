// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import React, { useCallback, useEffect, useState } from 'react'
import ReactDOM from 'react-dom/client'
import { useTranslation } from 'react-i18next'
import { AlertTriangle, Loader2 } from 'lucide-react'
import App from './App'
import './styles.css'
import { i18nReady } from './i18n'
import './bootstrap/initKeybindings'
import { initializeSettings } from './store/settingsStore'
import { Button } from '@/components/ui/button'
import { PortableBootstrapShell } from '@/components/bootstrap/PortableBootstrapShell'
import { api } from '@/lib/api'
import type { PortableInfoResponse, PortableStatusResponse } from './types'

// Dev-only: register fault injection API (window.__faultInjection)
import './lib/faultInjection'

// Initialize settings (including theme) before rendering
// This loads from oxide-settings-v2, applies theme, and cleans up legacy keys
initializeSettings()

type PortableBootstrapSnapshot = {
  info: PortableInfoResponse;
  status: PortableStatusResponse;
}

function BootstrapGateApp() {
  const { t } = useTranslation();
  const [snapshot, setSnapshot] = useState<PortableBootstrapSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const loadPortableBootstrap = useCallback(async () => {
    setLoading(true);
    setLoadError(null);

    try {
      const [status, info] = await Promise.all([
        api.getPortableStatus(),
        api.getPortableInfo(),
      ]);
      setSnapshot({ info, status });
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : String(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadPortableBootstrap();
  }, [loadPortableBootstrap]);

  if (loading) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-theme-bg px-6 text-theme-text">
        <div className="w-full max-w-lg rounded-3xl border border-theme-border bg-theme-bg-card p-8 text-center shadow-2xl">
          <Loader2 className="mx-auto h-8 w-8 animate-spin text-theme-accent" />
          <h1 className="mt-5 text-2xl font-semibold text-theme-text-heading">
            {t('common.portable_bootstrap.loading_title')}
          </h1>
          <p className="mt-3 text-sm leading-6 text-theme-text-muted">
            {t('common.portable_bootstrap.loading_description')}
          </p>
        </div>
      </div>
    );
  }

  if (loadError || !snapshot) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-theme-bg px-6 text-theme-text">
        <div className="w-full max-w-xl rounded-3xl border border-theme-border bg-theme-bg-card p-8 shadow-2xl">
          <div className="flex items-center gap-3 text-red-300">
            <AlertTriangle className="h-6 w-6" />
            <h1 className="text-2xl font-semibold text-theme-text-heading">
              {t('common.portable_bootstrap.load_failed_title')}
            </h1>
          </div>
          <p className="mt-3 text-sm leading-6 text-theme-text-muted">
            {t('common.portable_bootstrap.load_failed_description')}
          </p>
          {loadError && (
            <pre className="mt-4 overflow-x-auto rounded-2xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-xs text-red-200">
              {loadError}
            </pre>
          )}
          <div className="mt-6 flex justify-end">
            <Button onClick={() => void loadPortableBootstrap()}>
              {t('common.retry')}
            </Button>
          </div>
        </div>
      </div>
    );
  }

  if (snapshot.status.isPortable && !snapshot.status.canLaunchApp) {
    return (
      <PortableBootstrapShell
        info={snapshot.info}
        status={snapshot.status}
        onReady={(status) => {
          setSnapshot((current) => current ? { ...current, status } : current);
        }}
      />
    );
  }

  return <App portableStatus={snapshot.status} />;
}

function mountApp() {
  const root = ReactDOM.createRoot(document.getElementById('root')!);

  root.render(
    <React.StrictMode>
      <BootstrapGateApp />
    </React.StrictMode>,
  );

  window.addEventListener('beforeunload', () => {
    root.unmount();
  });
}

// Wait for i18n resources to load before rendering
i18nReady.then(() => {
  mountApp()
}).catch((err) => {
  console.error('Failed to initialize i18n:', err)
  // 降级渲染：即使 i18n 加载失败也要呈现界面（翻译字符串会显示 key）
  mountApp()
})
