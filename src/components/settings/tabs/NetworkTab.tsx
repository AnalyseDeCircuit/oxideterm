// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { KeyRound, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import { api } from '@/lib/api';
import { cn } from '@/lib/utils';
import type {
    NetworkSettings,
    SettingsUpstreamProxyAuth,
    SettingsUpstreamProxyConfig,
    SettingsUpstreamProxyProtocol,
} from '@/store/settingsStore';

type NetworkTabProps = {
    network?: NetworkSettings;
    updateNetwork: <K extends keyof NetworkSettings>(key: K, value: NetworkSettings[K]) => void;
};

const defaultUpstreamProxy = (): SettingsUpstreamProxyConfig => ({
    protocol: 'socks5',
    host: '127.0.0.1',
    port: 1080,
    auth: { type: 'none' },
    remoteDns: true,
    noProxy: '',
});

export const NetworkTab = ({ network, updateNetwork }: NetworkTabProps) => {
    const { t } = useTranslation();
    const effectiveNetwork = network ?? { upstreamProxy: null, upstreamProxyDisclaimerAccepted: false };
    const proxy = effectiveNetwork.upstreamProxy;
    const passwordAuth = proxy?.auth.type === 'password' ? proxy.auth : null;
    const [passwordDraft, setPasswordDraft] = useState('');
    const [savingPassword, setSavingPassword] = useState(false);
    const [passwordError, setPasswordError] = useState('');

    const updateProxy = (nextProxy: SettingsUpstreamProxyConfig | null) => {
        updateNetwork('upstreamProxy', nextProxy);
    };

    const patchProxy = (patch: Partial<SettingsUpstreamProxyConfig>) => {
        updateProxy({ ...(proxy ?? defaultUpstreamProxy()), ...patch });
    };

    const setAuth = (auth: SettingsUpstreamProxyAuth) => {
        patchProxy({ auth });
    };

    const handleSavePassword = async () => {
        if (!passwordAuth || !passwordDraft) return;
        setSavingPassword(true);
        setPasswordError('');
        try {
            // Submit the proxy password only at this explicit keychain boundary.
            const { keychainId } = await api.saveGlobalUpstreamProxyPassword(passwordDraft);
            setAuth({ ...passwordAuth, keychain_id: keychainId });
            setPasswordDraft('');
        } catch (error) {
            setPasswordError(error instanceof Error ? error.message : String(error));
        } finally {
            setSavingPassword(false);
        }
    };

    const handleRemovePassword = async () => {
        if (!passwordAuth?.keychain_id && !passwordDraft) return;
        setSavingPassword(true);
        setPasswordError('');
        try {
            await api.deleteGlobalUpstreamProxyPassword();
            setAuth({ type: 'password', username: passwordAuth?.username ?? '' });
            setPasswordDraft('');
        } catch (error) {
            setPasswordError(error instanceof Error ? error.message : String(error));
        } finally {
            setSavingPassword(false);
        }
    };

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
            <div>
                <h3 className="text-2xl font-medium text-theme-text-heading mb-2">{t('settings_view.network.title')}</h3>
                <p className="text-theme-text-muted">{t('settings_view.network.description')}</p>
            </div>
            <Separator />

            <div className="space-y-5 max-w-2xl">
                <div className="flex items-center justify-between">
                    <div className="grid gap-1">
                        <Label>{t('settings_view.network.disclaimer')}</Label>
                        <p className="text-xs text-theme-text-muted">{t('settings_view.network.disclaimer_hint')}</p>
                    </div>
                    <Checkbox
                        checked={effectiveNetwork.upstreamProxyDisclaimerAccepted}
                        onCheckedChange={(checked) => updateNetwork('upstreamProxyDisclaimerAccepted', !!checked)}
                    />
                </div>

                <div className="flex items-center justify-between">
                    <div className="grid gap-1">
                        <Label>{t('settings_view.network.enabled')}</Label>
                        <p className="text-xs text-theme-text-muted">{t('settings_view.network.enabled_hint')}</p>
                    </div>
                    <Checkbox
                        checked={!!proxy}
                        disabled={!effectiveNetwork.upstreamProxyDisclaimerAccepted}
                        onCheckedChange={(checked) => updateProxy(checked ? defaultUpstreamProxy() : null)}
                    />
                </div>
            </div>

            <Separator />

            <div className={cn('space-y-6 transition-opacity', !proxy && 'opacity-40 pointer-events-none')}>
                <h4 className="text-lg font-medium text-theme-text-heading">{t('settings_view.network.proxy')}</h4>

                <div className="grid grid-cols-2 gap-8 max-w-2xl">
                    <div className="grid gap-2">
                        <Label>{t('settings_view.network.protocol')}</Label>
                        <p className="text-xs text-theme-text-muted">{t('settings_view.network.protocol_hint')}</p>
                        <Select
                            value={proxy?.protocol ?? 'socks5'}
                            onValueChange={(value) => patchProxy({ protocol: value as SettingsUpstreamProxyProtocol })}
                        >
                            <SelectTrigger className="w-full">
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="socks5">{t('settings_view.network.protocol_socks5')}</SelectItem>
                                <SelectItem value="http_connect">{t('settings_view.network.protocol_http_connect')}</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>

                    <div className="grid gap-2">
                        <Label>{t('settings_view.network.port')}</Label>
                        <p className="text-xs text-theme-text-muted">{t('settings_view.network.port_hint')}</p>
                        <Input
                            type="number"
                            value={proxy?.port ?? 1080}
                            onChange={(event) => {
                                const value = Number.parseInt(event.target.value, 10);
                                patchProxy({ port: Number.isFinite(value) ? Math.min(65535, Math.max(1, value)) : 1080 });
                            }}
                            min={1}
                            max={65535}
                        />
                    </div>
                </div>

                <div className="grid gap-2 max-w-2xl">
                    <Label>{t('settings_view.network.host')}</Label>
                    <p className="text-xs text-theme-text-muted">{t('settings_view.network.host_hint')}</p>
                    <Input
                        value={proxy?.host ?? ''}
                        onChange={(event) => patchProxy({ host: event.target.value })}
                        placeholder="127.0.0.1"
                    />
                </div>

                <div className="grid gap-2 max-w-2xl">
                    <Label>{t('settings_view.network.no_proxy')}</Label>
                    <p className="text-xs text-theme-text-muted">{t('settings_view.network.no_proxy_hint')}</p>
                    <Input
                        value={proxy?.noProxy ?? ''}
                        onChange={(event) => patchProxy({ noProxy: event.target.value })}
                        placeholder="localhost,127.0.0.1,*.internal"
                    />
                </div>

                <div className="flex items-center justify-between max-w-2xl">
                    <div className="grid gap-1">
                        <Label>{t('settings_view.network.remote_dns')}</Label>
                        <p className="text-xs text-theme-text-muted">{t('settings_view.network.remote_dns_hint')}</p>
                    </div>
                    <Checkbox
                        checked={proxy?.remoteDns ?? true}
                        onCheckedChange={(checked) => patchProxy({ remoteDns: !!checked })}
                    />
                </div>

                <Separator />

                <div className="grid grid-cols-2 gap-8 max-w-2xl">
                    <div className="grid gap-2">
                        <Label>{t('settings_view.network.auth')}</Label>
                        <p className="text-xs text-theme-text-muted">{t('settings_view.network.auth_hint')}</p>
                        <Select
                            value={proxy?.auth.type ?? 'none'}
                            onValueChange={(value) => {
                                setPasswordDraft('');
                                setPasswordError('');
                                setAuth(value === 'password' ? { type: 'password', username: passwordAuth?.username ?? '' } : { type: 'none' });
                            }}
                        >
                            <SelectTrigger className="w-full">
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="none">{t('settings_view.network.auth_none')}</SelectItem>
                                <SelectItem value="password">{t('settings_view.network.auth_password')}</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>

                    {passwordAuth && (
                        <div className="grid gap-2">
                            <Label>{t('settings_view.network.username')}</Label>
                            <p className="text-xs text-theme-text-muted">{t('settings_view.network.username_hint')}</p>
                            <Input
                                value={passwordAuth.username}
                                onChange={(event) => setAuth({ ...passwordAuth, username: event.target.value })}
                            />
                        </div>
                    )}
                </div>

                {passwordAuth && (
                    <div className="grid gap-2 max-w-2xl">
                        <Label>{t('settings_view.network.password')}</Label>
                        <p className="text-xs text-theme-text-muted">
                            {passwordAuth.keychain_id
                                ? t('settings_view.network.password_saved_hint')
                                : t('settings_view.network.password_hint')}
                        </p>
                        <div className="flex gap-2">
                            <Input
                                type="password"
                                value={passwordDraft}
                                onChange={(event) => setPasswordDraft(event.target.value)}
                                placeholder={passwordAuth.keychain_id ? t('settings_view.network.password_saved_placeholder') : ''}
                            />
                            <Button disabled={!passwordDraft || savingPassword} onClick={handleSavePassword}>
                                <KeyRound className="h-4 w-4 mr-2" />
                                {savingPassword ? t('settings_view.network.saving') : t('settings_view.network.save_password')}
                            </Button>
                            <Button variant="ghost" disabled={savingPassword || (!passwordAuth.keychain_id && !passwordDraft)} onClick={handleRemovePassword}>
                                <Trash2 className="h-4 w-4 mr-2" />
                                {t('settings_view.network.remove_password')}
                            </Button>
                        </div>
                        {passwordError && <p className="text-xs text-red-400">{passwordError}</p>}
                    </div>
                )}
            </div>
        </div>
    );
};
