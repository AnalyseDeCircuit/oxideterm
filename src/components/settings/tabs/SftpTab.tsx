// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useTranslation } from 'react-i18next';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import type { SftpSettings } from '@/store/settingsStore';

type SftpTabProps = {
    sftp?: SftpSettings;
    updateSftp: <K extends keyof SftpSettings>(key: K, value: SftpSettings[K]) => void;
};

export const SftpTab = ({ sftp, updateSftp }: SftpTabProps) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
            <div>
                <h3 className="text-2xl font-medium text-theme-text-heading mb-2">{t('settings_view.sftp.title')}</h3>
                <p className="text-theme-text-muted">{t('settings_view.sftp.description')}</p>
            </div>
            <Separator />

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <div className="space-y-5">
                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.sftp.concurrent')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">
                                {t('settings_view.sftp.concurrent_hint')}
                            </p>
                        </div>
                        <Select
                            value={(sftp?.maxConcurrentTransfers ?? 3).toString()}
                            onValueChange={(value) => updateSftp('maxConcurrentTransfers', parseInt(value, 10))}
                        >
                            <SelectTrigger className="w-[180px]">
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                {[1, 2, 3, 4, 5, 6, 8, 10].map((count) => (
                                    <SelectItem key={count} value={count.toString()}>
                                        {t('settings_view.sftp.transfer_count', { count })}
                                    </SelectItem>
                                ))}
                            </SelectContent>
                        </Select>
                    </div>
                    <Separator />
                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.sftp.directory_parallelism')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">
                                {t('settings_view.sftp.directory_parallelism_hint')}
                            </p>
                        </div>
                        <Select
                            value={(sftp?.directoryParallelism ?? 4).toString()}
                            onValueChange={(value) => updateSftp('directoryParallelism', parseInt(value, 10))}
                        >
                            <SelectTrigger className="w-[180px]">
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                {[1, 2, 3, 4, 5, 6, 8, 10, 12, 16].map((count) => (
                                    <SelectItem key={count} value={count.toString()}>
                                        {t('settings_view.sftp.transfer_count', { count })}
                                    </SelectItem>
                                ))}
                            </SelectContent>
                        </Select>
                    </div>
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <div className="space-y-4">
                    <div className="flex items-center justify-between">
                        <div>
                            <Label htmlFor="speed-limit-enabled" className="text-theme-text">{t('settings_view.sftp.bandwidth')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.sftp.bandwidth_hint')}</p>
                        </div>
                        <Checkbox
                            id="speed-limit-enabled"
                            checked={sftp?.speedLimitEnabled ?? false}
                            onCheckedChange={(checked) => updateSftp('speedLimitEnabled', !!checked)}
                        />
                    </div>

                    {sftp?.speedLimitEnabled && (
                        <div className="pt-2 flex items-center justify-between animate-in fade-in slide-in-from-top-1 duration-200">
                            <div>
                                <Label className="text-theme-text text-sm">{t('settings_view.sftp.speed_limit')}</Label>
                            </div>
                            <Input
                                type="number"
                                className="w-[180px]"
                                value={sftp?.speedLimitKBps ?? 0}
                                onChange={(event) => {
                                    const value = parseInt(event.target.value, 10) || 0;
                                    updateSftp('speedLimitKBps', Math.max(0, value));
                                }}
                                min={0}
                                step={100}
                                placeholder="0 = unlimited"
                            />
                        </div>
                    )}
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <div className="flex items-center justify-between mb-2">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.sftp.conflict')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">
                            {t('settings_view.sftp.conflict_hint')}
                        </p>
                    </div>
                    <Select
                        value={sftp?.conflictAction ?? 'ask'}
                        onValueChange={(value) => updateSftp('conflictAction', value as SftpSettings['conflictAction'])}
                    >
                        <SelectTrigger className="w-[180px]">
                            <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                            <SelectItem value="ask">{t('settings_view.sftp.conflict_ask')}</SelectItem>
                            <SelectItem value="overwrite">{t('settings_view.sftp.conflict_overwrite')}</SelectItem>
                            <SelectItem value="skip">{t('settings_view.sftp.conflict_skip')}</SelectItem>
                            <SelectItem value="rename">{t('settings_view.sftp.conflict_rename')}</SelectItem>
                        </SelectContent>
                    </Select>
                </div>
            </div>
        </div>
    );
};
