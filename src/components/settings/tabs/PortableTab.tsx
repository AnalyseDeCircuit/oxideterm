// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowDownToLine, ArrowUpToLine } from 'lucide-react';
import { OxideExportModal } from '@/components/modals/OxideExportModal';
import { OxideImportModal } from '@/components/modals/OxideImportModal';
import { PortableBiometricBindingDialog } from '@/components/settings/portable/PortableBiometricBindingDialog';
import { PortablePasswordChangeDialog } from '@/components/settings/portable/PortablePasswordChangeDialog';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Separator } from '@/components/ui/separator';
import { useConfirm } from '@/hooks/useConfirm';
import { useToast } from '@/hooks/useToast';
import { api } from '@/lib/api';
import type { PortableMigrationSummaryResponse, PortableStatusResponse } from '@/types';

export const PortableTab = () => {
    const { t } = useTranslation();
    const { success: toastSuccess, error: toastError } = useToast();
    const { confirm, ConfirmDialog } = useConfirm();
    const [migrationSummary, setMigrationSummary] = useState<PortableMigrationSummaryResponse | null>(null);
    const [portableRuntime, setPortableRuntime] = useState<PortableStatusResponse | null>(null);
    const [exportModalOpen, setExportModalOpen] = useState(false);
    const [importModalOpen, setImportModalOpen] = useState(false);
    const [changePasswordOpen, setChangePasswordOpen] = useState(false);
    const [biometricDialogOpen, setBiometricDialogOpen] = useState(false);
    const [portableActionError, setPortableActionError] = useState<string | null>(null);
    const [portableActionPending, setPortableActionPending] = useState<'changePassword' | 'enableBiometric' | 'disableBiometric' | null>(null);

    useEffect(() => {
        let cancelled = false;

        api.getPortableStatus()
            .then((status) => {
                if (!cancelled) {
                    setPortableRuntime(status);
                }
            })
            .catch((error) => {
                console.error('Failed to load portable status:', error);
            });

        api.getPortableMigrationSummary()
            .then((summary) => {
                if (!cancelled) {
                    setMigrationSummary(summary);
                }
            })
            .catch((error) => {
                console.error('Failed to load portable migration summary:', error);
            });

        return () => {
            cancelled = true;
        };
    }, []);

    const portableActivationLabel = portableRuntime?.activation === 'config'
        ? t('settings_view.general.portable_activation_config')
        : portableRuntime?.activation === 'marker'
            ? t('settings_view.general.portable_activation_marker')
            : t('settings_view.general.portable_activation_disabled');
    const isPortableRuntime = portableRuntime?.isPortable ?? migrationSummary?.isPortable ?? false;

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
            <div>
                <h3 className="text-2xl font-medium text-theme-text-heading mb-2">{t('settings_view.general.portable_runtime')}</h3>
                <p className="text-theme-text-muted">
                    {isPortableRuntime
                        ? t('settings_view.portable_description')
                        : t('settings_view.general.portable_runtime_disabled_hint')}
                </p>
            </div>
            <Separator />

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                    {t('settings_view.general.portable_runtime')}
                </h4>
                <div className="space-y-4">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.general.portable_runtime')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">
                            {portableRuntime?.isPortable
                                ? t('settings_view.general.portable_runtime_hint')
                                : t('settings_view.general.portable_runtime_disabled_hint')}
                        </p>
                    </div>

                    {portableRuntime?.isPortable ? (
                        <>
                            <div className="space-y-3 rounded-md border border-theme-border bg-theme-bg p-3">
                                <div>
                                    <p className="text-xs text-theme-text-muted">{t('settings_view.general.portable_root_dir')}</p>
                                    <code className="mt-1 block rounded bg-theme-bg-subtle px-3 py-2 text-xs text-theme-text font-mono break-all">
                                        {portableRuntime.portableRootDir}
                                    </code>
                                </div>
                                <div>
                                    <p className="text-xs text-theme-text-muted">{t('settings_view.general.portable_activation')}</p>
                                    <div className="mt-1 rounded bg-theme-bg-subtle px-3 py-2 text-xs text-theme-text">
                                        {portableActivationLabel}
                                    </div>
                                </div>
                                <div>
                                    <p className="text-xs text-theme-text-muted">{t('settings_view.general.portable_config_path')}</p>
                                    <code className="mt-1 block rounded bg-theme-bg-subtle px-3 py-2 text-xs text-theme-text font-mono break-all">
                                        {portableRuntime.configPath}
                                    </code>
                                </div>
                                <div>
                                    <p className="text-xs text-theme-text-muted">{t('settings_view.general.portable_instance_lock_path')}</p>
                                    <code className="mt-1 block rounded bg-theme-bg-subtle px-3 py-2 text-xs text-theme-text font-mono break-all">
                                        {portableRuntime.instanceLockPath || t('settings_view.general.portable_instance_lock_unavailable')}
                                    </code>
                                </div>
                            </div>

                            <div className="space-y-3 rounded-md border border-theme-border bg-theme-bg p-3">
                                <div>
                                    <Label className="text-theme-text">{t('settings_view.general.portable_biometric')}</Label>
                                    <p className="text-xs text-theme-text-muted mt-0.5">
                                        {portableRuntime.supportsBiometricBinding
                                            ? portableRuntime.hasBiometricBinding
                                                ? t('settings_view.general.portable_biometric_bound')
                                                : t('settings_view.general.portable_biometric_unbound')
                                            : t('settings_view.general.portable_biometric_unsupported')}
                                    </p>
                                </div>

                                <div className="flex flex-wrap gap-3">
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        onClick={() => {
                                            setPortableActionError(null);
                                            setChangePasswordOpen(true);
                                        }}
                                    >
                                        {t('settings_view.general.portable_change_password')}
                                    </Button>

                                    {portableRuntime.supportsBiometricBinding && !portableRuntime.hasBiometricBinding && (
                                        <Button
                                            variant="outline"
                                            size="sm"
                                            onClick={() => {
                                                setPortableActionError(null);
                                                setBiometricDialogOpen(true);
                                            }}
                                        >
                                            {t('settings_view.general.portable_enable_biometric')}
                                        </Button>
                                    )}

                                    {portableRuntime.supportsBiometricBinding && portableRuntime.hasBiometricBinding && (
                                        <Button
                                            variant="outline"
                                            size="sm"
                                            disabled={portableActionPending === 'disableBiometric'}
                                            onClick={async () => {
                                                const confirmed = await confirm({
                                                    title: t('settings_view.general.portable_disable_biometric'),
                                                    description: t('settings_view.general.portable_disable_biometric_confirm'),
                                                });
                                                if (!confirmed) return;

                                                setPortableActionPending('disableBiometric');
                                                setPortableActionError(null);
                                                try {
                                                    const nextStatus = await api.disablePortableBiometricUnlock();
                                                    setPortableRuntime(nextStatus);
                                                    toastSuccess(t('settings_view.general.portable_biometric_disabled'));
                                                } catch (error) {
                                                    setPortableActionError(String(error));
                                                    toastError(String(error));
                                                } finally {
                                                    setPortableActionPending(null);
                                                }
                                            }}
                                        >
                                            {t('settings_view.general.portable_disable_biometric')}
                                        </Button>
                                    )}
                                </div>

                                {portableActionError && (
                                    <div className="rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-300" role="alert">
                                        {portableActionError}
                                    </div>
                                )}
                            </div>
                        </>
                    ) : (
                        <div className="rounded-md border border-theme-border bg-theme-bg px-3 py-3 text-xs text-theme-text-muted">
                            {t('settings_view.general.portable_runtime_disabled_hint')}
                        </div>
                    )}
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                    {t('settings_view.general.portable_migration')}
                </h4>
                <div className="space-y-4">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.general.portable_migration')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">
                            {portableRuntime?.isPortable
                                ? t('settings_view.general.portable_migration_portable_hint')
                                : t('settings_view.general.portable_migration_installed_hint')}
                        </p>
                    </div>

                    {migrationSummary && (
                        <div className="space-y-3 rounded-md border border-theme-border bg-theme-bg p-3">
                            <div>
                                <p className="text-xs text-theme-text-muted">{t('settings_view.general.portable_migration_current_dir')}</p>
                                <code className="mt-1 block rounded bg-theme-bg-subtle px-3 py-2 text-xs text-theme-text font-mono break-all">
                                    {migrationSummary.currentDataDir}
                                </code>
                            </div>
                            <div>
                                <p className="text-xs text-theme-text-muted">{t('settings_view.general.portable_migration_target_dir')}</p>
                                <code className="mt-1 block rounded bg-theme-bg-subtle px-3 py-2 text-xs text-theme-text font-mono break-all">
                                    {migrationSummary.portableDataDir}
                                </code>
                            </div>
                            <p className="text-xs text-theme-text-muted">
                                {t('settings_view.general.portable_migration_secret_summary', {
                                    count: migrationSummary.exportablePortableSecretCount,
                                })}
                            </p>
                        </div>
                    )}

                    <div className="flex flex-wrap gap-3">
                        <Button
                            variant="outline"
                            size="sm"
                            onClick={() => setExportModalOpen(true)}
                        >
                            <ArrowUpToLine className="h-3 w-3 mr-1" />
                            {t('settings_view.general.portable_migration_export')}
                        </Button>
                        <Button
                            variant="outline"
                            size="sm"
                            onClick={() => setImportModalOpen(true)}
                        >
                            <ArrowDownToLine className="h-3 w-3 mr-1" />
                            {t('settings_view.general.portable_migration_import')}
                        </Button>
                    </div>
                </div>
            </div>

            <OxideExportModal isOpen={exportModalOpen} onClose={() => setExportModalOpen(false)} mode="portableMigration" />
            <OxideImportModal isOpen={importModalOpen} onClose={() => setImportModalOpen(false)} mode="portableMigration" />
            <PortablePasswordChangeDialog
                open={changePasswordOpen}
                pending={portableActionPending === 'changePassword'}
                errorMessage={portableActionError}
                onOpenChange={setChangePasswordOpen}
                onSubmit={async (currentPassword, newPassword) => {
                    setPortableActionPending('changePassword');
                    setPortableActionError(null);
                    try {
                        const nextStatus = await api.changePortableKeystorePassword(currentPassword, newPassword);
                        setPortableRuntime(nextStatus);
                        setChangePasswordOpen(false);
                        toastSuccess(t('settings_view.general.portable_password_changed'));
                    } catch (error) {
                        setPortableActionError(String(error));
                        toastError(String(error));
                    } finally {
                        setPortableActionPending(null);
                    }
                }}
            />
            <PortableBiometricBindingDialog
                open={biometricDialogOpen}
                pending={portableActionPending === 'enableBiometric'}
                errorMessage={portableActionError}
                onOpenChange={setBiometricDialogOpen}
                onSubmit={async (password) => {
                    setPortableActionPending('enableBiometric');
                    setPortableActionError(null);
                    try {
                        const nextStatus = await api.enablePortableBiometricUnlock(password);
                        setPortableRuntime(nextStatus);
                        setBiometricDialogOpen(false);
                        toastSuccess(t('settings_view.general.portable_biometric_enabled'));
                    } catch (error) {
                        setPortableActionError(String(error));
                        toastError(String(error));
                    } finally {
                        setPortableActionPending(null);
                    }
                }}
            />
            {ConfirmDialog}
        </div>
    );
};