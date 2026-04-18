// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { HardDrive, KeyRound, LockKeyhole, ShieldCheck } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { api } from '@/lib/api';
import type { PortableInfoResponse, PortableStatusResponse } from '@/types';
import { PortableSetupDialog } from '@/components/bootstrap/PortableSetupDialog';
import { PortableUnlockDialog } from '@/components/bootstrap/PortableUnlockDialog';

type PortableBootstrapShellProps = {
  info: PortableInfoResponse;
  status: PortableStatusResponse;
  onReady: (status: PortableStatusResponse) => void;
};

export function PortableBootstrapShell({ info, status, onReady }: PortableBootstrapShellProps) {
  const { t } = useTranslation();
  const [setupOpen, setSetupOpen] = useState(false);
  const [unlockOpen, setUnlockOpen] = useState(false);
  const [pendingAction, setPendingAction] = useState<'setup' | 'unlock' | null>(null);
  const [setupError, setSetupError] = useState<string | null>(null);
  const [unlockError, setUnlockError] = useState<string | null>(null);

  useEffect(() => {
    setSetupOpen(false);
    setUnlockOpen(false);
    setSetupError(null);
    setUnlockError(null);
    setPendingAction(null);
  }, [status.status]);

  const handleSetup = async (password: string) => {
    setPendingAction('setup');
    setSetupError(null);

    try {
      const nextStatus = await api.setupPortableKeystore(password);
      onReady(nextStatus);
    } catch (error) {
      setSetupError(error instanceof Error ? error.message : String(error));
    } finally {
      setPendingAction(null);
    }
  };

  const handleUnlock = async (password: string) => {
    setPendingAction('unlock');
    setUnlockError(null);

    try {
      const nextStatus = await api.unlockPortableKeystore(password);
      onReady(nextStatus);
    } catch (error) {
      setUnlockError(error instanceof Error ? error.message : String(error));
    } finally {
      setPendingAction(null);
    }
  };

  const isNeedsSetup = status.status === 'needsSetup';
  const statusLabel = isNeedsSetup
    ? t('common.portable_bootstrap.status_needs_setup')
    : t('common.portable_bootstrap.status_locked');

  return (
    <div className="min-h-screen bg-theme-bg text-theme-text">
      <div className="relative isolate min-h-screen overflow-hidden">
        <div className="absolute inset-0 opacity-70 [background:radial-gradient(circle_at_top_left,rgba(16,185,129,0.18),transparent_34%),radial-gradient(circle_at_bottom_right,rgba(59,130,246,0.14),transparent_38%)]" />
        <div className="relative mx-auto flex min-h-screen max-w-6xl items-center px-6 py-12">
          <div className="grid w-full gap-6 lg:grid-cols-[1.2fr_0.8fr]">
            <section className="rounded-3xl border border-theme-border bg-theme-bg-card/95 p-8 shadow-2xl backdrop-blur-sm">
              <div className="inline-flex items-center gap-2 rounded-full border border-theme-border/80 bg-theme-bg-elevated px-3 py-1 text-xs font-medium uppercase tracking-[0.22em] text-theme-text-muted">
                <ShieldCheck className="h-3.5 w-3.5 text-emerald-400" />
                {t('common.portable_bootstrap.title')}
              </div>

              <div className="mt-6 max-w-2xl space-y-4">
                <h1 className="text-4xl font-semibold tracking-tight text-theme-text-heading">
                  {t('common.portable_bootstrap.title')}
                </h1>
                <p className="text-base leading-7 text-theme-text-muted">
                  {t('common.portable_bootstrap.subtitle')}
                </p>
              </div>

              <div className="mt-8 grid gap-4 md:grid-cols-2">
                <div className="rounded-2xl border border-theme-border bg-theme-bg-elevated/80 p-4">
                  <div className="flex items-center gap-2 text-sm font-medium text-theme-text">
                    <HardDrive className="h-4 w-4 text-theme-accent" />
                    {t('common.portable_bootstrap.data_dir_label')}
                  </div>
                  <p className="mt-3 break-all rounded-xl bg-theme-bg px-3 py-2 font-mono text-xs text-theme-text-muted">
                    {info.dataDir}
                  </p>
                </div>

                <div className="rounded-2xl border border-theme-border bg-theme-bg-elevated/80 p-4">
                  <div className="flex items-center gap-2 text-sm font-medium text-theme-text">
                    <KeyRound className="h-4 w-4 text-theme-accent" />
                    {t('common.portable_bootstrap.keystore_path_label')}
                  </div>
                  <p className="mt-3 break-all rounded-xl bg-theme-bg px-3 py-2 font-mono text-xs text-theme-text-muted">
                    {status.keystorePath || t('common.portable_bootstrap.keystore_pending')}
                  </p>
                </div>
              </div>

              <div className="mt-4 rounded-2xl border border-theme-border bg-theme-bg-elevated/60 p-4">
                <div className="text-sm font-medium text-theme-text">
                  {t('common.portable_bootstrap.marker_path_label')}
                </div>
                <p className="mt-3 break-all rounded-xl bg-theme-bg px-3 py-2 font-mono text-xs text-theme-text-muted">
                  {info.markerPath}
                </p>
              </div>
            </section>

            <section className="space-y-6 rounded-3xl border border-theme-border bg-theme-bg-card/95 p-8 shadow-2xl backdrop-blur-sm">
              <div>
                <div className="text-xs font-medium uppercase tracking-[0.2em] text-theme-text-muted">
                  {t('common.portable_bootstrap.state_label')}
                </div>
                <div className="mt-3 inline-flex items-center gap-2 rounded-full border border-theme-border bg-theme-bg-elevated px-3 py-1.5 text-sm font-medium text-theme-text">
                  <LockKeyhole className="h-4 w-4 text-amber-400" />
                  {statusLabel}
                </div>
              </div>

              <div className="rounded-2xl border border-theme-border bg-theme-bg-elevated/70 p-5">
                <h2 className="text-lg font-medium text-theme-text-heading">
                  {isNeedsSetup
                    ? t('common.portable_bootstrap.setup_title')
                    : t('common.portable_bootstrap.unlock_title')}
                </h2>
                <p className="mt-2 text-sm leading-6 text-theme-text-muted">
                  {isNeedsSetup
                    ? t('common.portable_bootstrap.setup_description')
                    : t('common.portable_bootstrap.unlock_description')}
                </p>

                <Button
                  className="mt-5 w-full"
                  onClick={() => (isNeedsSetup ? setSetupOpen(true) : setUnlockOpen(true))}
                >
                  {isNeedsSetup
                    ? t('common.portable_bootstrap.setup_cta')
                    : t('common.portable_bootstrap.unlock_cta')}
                </Button>
              </div>

              <div className="rounded-2xl border border-theme-border bg-theme-bg-elevated/50 p-5">
                <div className="text-sm font-medium text-theme-text-heading">
                  {t('common.portable_bootstrap.manual_updates_title')}
                </div>
                <p className="mt-2 text-sm leading-6 text-theme-text-muted">
                  {t('common.portable_bootstrap.manual_updates_hint')}
                </p>
              </div>
            </section>
          </div>
        </div>
      </div>

      <PortableSetupDialog
        open={setupOpen}
        pending={pendingAction === 'setup'}
        errorMessage={setupError}
        onOpenChange={setSetupOpen}
        onSubmit={handleSetup}
      />

      <PortableUnlockDialog
        open={unlockOpen}
        pending={pendingAction === 'unlock'}
        errorMessage={unlockError}
        onOpenChange={setUnlockOpen}
        onSubmit={handleUnlock}
      />
    </div>
  );
}