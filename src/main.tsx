// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import React, { useState } from 'react'
import ReactDOM from 'react-dom/client'
import { AlertTriangle } from 'lucide-react'
import App from './App'
import './styles.css'
import i18n, { i18nReady } from './i18n'
import './bootstrap/initKeybindings'
import { initializeSettings } from './store/settingsStore'
import { Button } from '@/components/ui/button'
import { PortableBootstrapShell } from '@/components/bootstrap/PortableBootstrapShell'
import { api } from '@/lib/api'
import { applyLinuxWebviewProfile } from '@/lib/linuxWebviewProfile'
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

type BootstrapErrorCopy = {
  title: string;
  description: string;
  retryLabel: string;
}

function fetchPortableBootstrapSnapshot(): Promise<PortableBootstrapSnapshot> {
  return Promise.all([
    api.getPortableStatus(),
    api.getPortableInfo(),
  ]).then(([status, info]) => ({ info, status }) satisfies PortableBootstrapSnapshot);
}

function getBootstrapErrorCopy(): BootstrapErrorCopy {
  return {
    title: i18n.t('portable_bootstrap.load_failed_title', {
      defaultValue: 'Portable startup failed',
    }),
    description: i18n.t('portable_bootstrap.load_failed_description', {
      defaultValue: 'OxideTerm could not load portable startup state. Retry to continue.',
    }),
    retryLabel: i18n.t('common.retry', {
      defaultValue: 'Retry',
    }),
  };
}

// Prefetch portable status at module level (runs in parallel with i18n loading)
const portableReady = fetchPortableBootstrapSnapshot();
const linuxWebviewProfileReady = api.getLinuxWebviewProfile()
  .then((profile) => {
    applyLinuxWebviewProfile(profile);
  })
  .catch((err) => {
    console.warn('Failed to load Linux WebView profile:', err);
    applyLinuxWebviewProfile(null);
  });

function BootstrapGateApp({ initialSnapshot }: { initialSnapshot: PortableBootstrapSnapshot }) {
  const [snapshot, setSnapshot] = useState<PortableBootstrapSnapshot>(initialSnapshot);

  if (snapshot.status.isPortable && !snapshot.status.canLaunchApp) {
    return (
      <PortableBootstrapShell
        info={snapshot.info}
        status={snapshot.status}
        onReady={(status) => {
          setSnapshot((current) => ({ ...current, status }));
        }}
      />
    );
  }

  return <App portableStatus={snapshot.status} />;
}

function BootstrapErrorApp({ error, onRetry, copy }: { error: string; onRetry: () => void; copy: BootstrapErrorCopy }) {
  return (
    <div className="flex min-h-screen items-center justify-center bg-theme-bg px-6 text-theme-text">
      <div className="w-full max-w-xl rounded-3xl border border-theme-border bg-theme-bg-card p-8 shadow-2xl">
        <div className="flex items-center gap-3 text-red-300">
          <AlertTriangle className="h-6 w-6" />
          <h1 className="text-2xl font-semibold text-theme-text-heading">
            {copy.title}
          </h1>
        </div>
        <p className="mt-3 text-sm leading-6 text-theme-text-muted">
          {copy.description}
        </p>
        <pre className="mt-4 overflow-x-auto rounded-2xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-xs text-red-200">
          {error}
        </pre>
        <div className="mt-6 flex justify-end">
          <Button onClick={onRetry}>
            {copy.retryLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}

// Single root instance to prevent listener leaks on retry
let activeRoot: ReactDOM.Root | null = null;
let frontendReadySignalSent = false;

function ensureRoot(): ReactDOM.Root {
  if (activeRoot) {
    activeRoot.unmount();
  }
  activeRoot = ReactDOM.createRoot(document.getElementById('root')!);
  return activeRoot;
}

window.addEventListener('beforeunload', () => {
  activeRoot?.unmount();
});

function notifyFrontendReady() {
  if (frontendReadySignalSent) return;
  frontendReadySignalSent = true;

  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      api.frontendReady().catch((err) => {
        frontendReadySignalSent = false;
        console.warn('Failed to notify frontend_ready:', err);
      });
    });
  });
}

function mountApp(snapshot: PortableBootstrapSnapshot) {
  const root = ensureRoot();

  root.render(
    <React.StrictMode>
      <BootstrapGateApp initialSnapshot={snapshot} />
    </React.StrictMode>,
  );

  notifyFrontendReady();
}

function mountError(error: string) {
  const root = ensureRoot();
  const copy = getBootstrapErrorCopy();

  const retry = async () => {
    try {
      const snapshot = await fetchPortableBootstrapSnapshot();
      mountApp(snapshot);
    } catch (err) {
      mountError(err instanceof Error ? err.message : String(err));
    }
  };

  root.render(
    <React.StrictMode>
      <BootstrapErrorApp error={error} onRetry={() => void retry()} copy={copy} />
    </React.StrictMode>,
  );
}

// Preserve the old startup invariant: i18n failure degrades to key rendering,
// but portable bootstrap failure blocks startup and shows an explicit error.
const i18nStartupReady = i18nReady.catch((err) => {
  console.error('Failed to initialize i18n:', err);
});

// Wait for both i18n and portable status before rendering.
// portableReady starts at module load (parallel with i18n), so the
// combined wait is max(i18n, portable) rather than i18n + portable.
Promise.all([i18nStartupReady, portableReady, linuxWebviewProfileReady])
  .then(([, snapshot]) => {
    mountApp(snapshot);
  })
  .catch((err) => {
    console.error('Failed to initialize portable bootstrap:', err);
    const message = err instanceof Error ? err.message : String(err);
    mountError(message);
  })
