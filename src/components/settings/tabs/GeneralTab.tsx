// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open as openFileDialog } from '@tauri-apps/plugin-dialog';
import { ArrowDownToLine, ArrowUpToLine, Loader2, TerminalSquare, Trash2 } from 'lucide-react';
import { OxideExportModal } from '@/components/modals/OxideExportModal';
import { OxideImportModal } from '@/components/modals/OxideImportModal';
import { PortableBiometricBindingDialog } from '@/components/settings/portable/PortableBiometricBindingDialog';
import { PortablePasswordChangeDialog } from '@/components/settings/portable/PortablePasswordChangeDialog';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import { useConfirm } from '@/hooks/useConfirm';
import { useToast } from '@/hooks/useToast';
import { api } from '@/lib/api';
import type { DataDirInfo, PortableMigrationSummaryResponse, PortableStatusResponse } from '@/types';
import type { GeneralSettings, Language } from '@/store/settingsStore';

type CliStatus = {
    bundled: boolean;
    installed: boolean;
    install_path: string | null;
    bundle_path: string | null;
    app_version: string;
    matches_bundled: boolean | null;
    needs_reinstall: boolean;
};

type GeneralTabProps = {
    general: GeneralSettings;
    setLanguage: (language: Language) => void;
    portableStatus: PortableStatusResponse | null;
};

export const GeneralTab = ({ general, setLanguage, portableStatus }: GeneralTabProps) => {
    const { t } = useTranslation();
    const { success: toastSuccess, error: toastError } = useToast();
    const { confirm, ConfirmDialog } = useConfirm();
    const [dataDirInfo, setDataDirInfo] = useState<DataDirInfo | null>(null);
    const [dataDirLoading, setDataDirLoading] = useState(false);
    const [cliStatus, setCliStatus] = useState<CliStatus | null>(null);
    const [cliLoading, setCliLoading] = useState(false);
    const [migrationSummary, setMigrationSummary] = useState<PortableMigrationSummaryResponse | null>(null);
    const [exportModalOpen, setExportModalOpen] = useState(false);
    const [importModalOpen, setImportModalOpen] = useState(false);
    const [portableRuntime, setPortableRuntime] = useState<PortableStatusResponse | null>(portableStatus);
    const [changePasswordOpen, setChangePasswordOpen] = useState(false);
    const [biometricDialogOpen, setBiometricDialogOpen] = useState(false);
    const [portableActionError, setPortableActionError] = useState<string | null>(null);
    const [portableActionPending, setPortableActionPending] = useState<'changePassword' | 'enableBiometric' | 'disableBiometric' | null>(null);

    useEffect(() => {
        setPortableRuntime(portableStatus);
    }, [portableStatus]);

    const portableActivationLabel = portableRuntime?.activation === 'config'
        ? t('settings_view.general.portable_activation_config')
        : portableRuntime?.activation === 'marker'
            ? t('settings_view.general.portable_activation_marker')
            : t('settings_view.general.portable_activation_disabled');

    useEffect(() => {
        let cancelled = false;

        api.getDataDirectory()
            .then((info) => {
                if (!cancelled) setDataDirInfo(info);
            })
            .catch((error) => {
                console.error('Failed to load data directory info:', error);
            });

        api.cliGetStatus()
            .then((status) => {
                if (!cancelled) setCliStatus(status);
            })
            .catch((error) => {
                console.error('Failed to load CLI status:', error);
            });

        api.getPortableMigrationSummary()
            .then((summary) => {
                if (!cancelled) setMigrationSummary(summary);
            })
            .catch((error) => {
                console.error('Failed to load portable migration summary:', error);
            });

        return () => {
            cancelled = true;
        };
    }, []);

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
            <div>
                <h3 className="text-2xl font-medium text-theme-text-heading mb-2">{t('settings_view.general.title')}</h3>
                <p className="text-theme-text-muted">{t('settings_view.general.description')}</p>
            </div>
            <Separator />

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                    {t('settings_view.general.language')}
                </h4>
                <div className="space-y-5">
                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.general.language')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.general.language_hint')}</p>
                        </div>
                        <Select value={general.language} onValueChange={(value) => setLanguage(value as Language)}>
                            <SelectTrigger className="w-[200px]">
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="de">Deutsch</SelectItem>
                                <SelectItem value="en">English</SelectItem>
                                <SelectItem value="es-ES">Español (España)</SelectItem>
                                <SelectItem value="fr-FR">Français (France)</SelectItem>
                                <SelectItem value="it">Italiano</SelectItem>
                                <SelectItem value="ko">한국어</SelectItem>
                                <SelectItem value="pt-BR">Português (Brasil)</SelectItem>
                                <SelectItem value="vi">Tiếng Việt</SelectItem>
                                <SelectItem value="ja">日本語</SelectItem>
                                <SelectItem value="zh-CN">简体中文</SelectItem>
                                <SelectItem value="zh-TW">繁體中文</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                    {t('settings_view.general.data_directory')}
                </h4>
                <div className="space-y-4">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.general.data_directory')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.general.data_directory_hint')}</p>
                    </div>
                    {dataDirInfo && (
                        <div className="flex items-center gap-3">
                            <code className="flex-1 px-3 py-2 bg-theme-bg-subtle rounded text-sm text-theme-text font-mono truncate">
                                {dataDirInfo.path}
                            </code>
                            {dataDirInfo.can_change && (
                                <Button
                                    variant="outline"
                                    size="sm"
                                    disabled={dataDirLoading}
                                    onClick={async () => {
                                        const selected = await openFileDialog({
                                            directory: true,
                                            title: t('settings_view.general.select_data_directory'),
                                        });
                                        if (!selected || typeof selected !== 'string') return;

                                        setDataDirLoading(true);
                                        try {
                                            const check = await api.checkDataDirectory(selected);
                                            if (check.has_existing_data) {
                                                const proceed = await confirm({
                                                    title: t('settings_view.general.data_directory_conflict'),
                                                    description: t('settings_view.general.data_directory_conflict_detail', {
                                                        files: check.files_found.join(', '),
                                                    }),
                                                });
                                                if (!proceed) return;
                                            }
                                            await api.setDataDirectory(selected);
                                            toastSuccess(t('settings_view.general.data_directory_changed'));
                                            setDataDirInfo((current) => current
                                                ? {
                                                    ...current,
                                                    path: selected,
                                                    is_custom: true,
                                                    is_portable: false,
                                                    can_change: true,
                                                }
                                                : current);
                                        } catch (error) {
                                            toastError(String(error));
                                        } finally {
                                            setDataDirLoading(false);
                                        }
                                    }}
                                >
                                    {t('settings_view.general.change')}
                                </Button>
                            )}
                            {dataDirInfo.can_change && dataDirInfo.is_custom && (
                                <Button
                                    variant="ghost"
                                    size="sm"
                                    disabled={dataDirLoading}
                                    onClick={async () => {
                                        const confirmed = await confirm({
                                            title: t('settings_view.general.reset_data_directory'),
                                            description: t('settings_view.general.reset_data_directory_confirm'),
                                        });
                                        if (!confirmed) return;

                                        setDataDirLoading(true);
                                        try {
                                            await api.resetDataDirectory();
                                            toastSuccess(t('settings_view.general.data_directory_reset'));
                                            setDataDirInfo((current) => current
                                                ? {
                                                    ...current,
                                                    path: current.default_path,
                                                    is_custom: false,
                                                    is_portable: false,
                                                    can_change: true,
                                                }
                                                : current);
                                        } catch (error) {
                                            toastError(String(error));
                                        } finally {
                                            setDataDirLoading(false);
                                        }
                                    }}
                                >
                                    {t('settings_view.general.reset_to_default')}
                                </Button>
                            )}
                        </div>
                    )}
                    <p className="text-xs text-yellow-500">
                        {t('settings_view.general.data_directory_restart_notice')}
                    </p>
                </div>
            </div>

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
                            {portableStatus?.isPortable
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

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">
                    {t('settings_view.general.cli_companion')}
                </h4>
                <div className="space-y-4">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.general.cli_tool')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.general.cli_tool_hint')}</p>
                    </div>

                    {cliStatus && (
                        <div className="space-y-3">
                            <div className="flex items-center gap-3">
                                <div className="flex-1">
                                    <div className="flex items-center gap-2 mb-1">
                                        <TerminalSquare className="h-4 w-4 text-theme-text-muted" />
                                        <span className="text-sm text-theme-text font-mono">oxide</span>
                                        {cliStatus.installed && cliStatus.needs_reinstall ? (
                                            <span className="text-xs px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-500">{t('settings_view.general.cli_reinstall_required')}</span>
                                        ) : cliStatus.installed ? (
                                            <span className="text-xs px-1.5 py-0.5 rounded bg-green-500/10 text-green-500">{t('settings_view.general.cli_installed')}</span>
                                        ) : (
                                            <span className="text-xs px-1.5 py-0.5 rounded bg-theme-bg-subtle text-theme-text-muted">{t('settings_view.general.cli_not_installed')}</span>
                                        )}
                                    </div>
                                    {cliStatus.install_path && (
                                        <code className="text-xs text-theme-text-muted font-mono">{cliStatus.install_path}</code>
                                    )}
                                    {cliStatus.installed && cliStatus.needs_reinstall && (
                                        <p className="text-xs text-amber-500 mt-1">{t('settings_view.general.cli_reinstall_hint', { version: cliStatus.app_version })}</p>
                                    )}
                                </div>
                                {cliStatus.bundled && (!cliStatus.installed || cliStatus.needs_reinstall) && (
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        disabled={cliLoading}
                                        onClick={async () => {
                                            setCliLoading(true);
                                            try {
                                                const message = await api.cliInstall();
                                                toastSuccess(message);
                                                setCliStatus(await api.cliGetStatus());
                                            } catch (error) {
                                                toastError(String(error));
                                            } finally {
                                                setCliLoading(false);
                                            }
                                        }}
                                    >
                                        {cliLoading ? <Loader2 className="h-3 w-3 animate-spin mr-1" /> : <ArrowDownToLine className="h-3 w-3 mr-1" />}
                                        {cliStatus.needs_reinstall ? t('settings_view.general.cli_reinstall') : t('settings_view.general.cli_install')}
                                    </Button>
                                )}
                                {cliStatus.installed && (
                                    <Button
                                        variant="ghost"
                                        size="sm"
                                        disabled={cliLoading}
                                        onClick={async () => {
                                            setCliLoading(true);
                                            try {
                                                const message = await api.cliUninstall();
                                                toastSuccess(message);
                                                setCliStatus(await api.cliGetStatus());
                                            } catch (error) {
                                                toastError(String(error));
                                            } finally {
                                                setCliLoading(false);
                                            }
                                        }}
                                    >
                                        <Trash2 className="h-3 w-3 mr-1" />
                                        {t('settings_view.general.cli_uninstall')}
                                    </Button>
                                )}
                            </div>
                            {!cliStatus.bundled && (
                                <p className="text-xs text-theme-text-muted">
                                    {t('settings_view.general.cli_not_bundled')}
                                </p>
                            )}
                        </div>
                    )}
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