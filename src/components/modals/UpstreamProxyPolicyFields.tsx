// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useTranslation } from 'react-i18next';
import { Checkbox } from '../ui/checkbox';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import type {
  SavedUpstreamProxyAuth,
  SavedUpstreamProxyConfig,
  SavedUpstreamProxyPolicy,
  SavedUpstreamProxyProtocol,
} from '../../types';

type UpstreamProxyPolicyFieldsProps = {
  value: SavedUpstreamProxyPolicy;
  onChange: (value: SavedUpstreamProxyPolicy) => void;
};

export const defaultSavedUpstreamProxyPolicy = (): SavedUpstreamProxyPolicy => ({ mode: 'use_global' });

export const defaultSavedUpstreamProxyConfig = (): SavedUpstreamProxyConfig => ({
  protocol: 'socks5',
  host: '127.0.0.1',
  port: 1080,
  auth: { type: 'none' },
  remoteDns: true,
  noProxy: '',
});

export function upstreamProxyForConnectFromPolicy(policy: SavedUpstreamProxyPolicy) {
  if (policy.mode !== 'custom') {
    return undefined;
  }
  const { proxy } = policy;
  return {
    protocol: proxy.protocol,
    host: proxy.host,
    port: proxy.port,
    auth: proxy.auth.type === 'password'
      ? { type: 'password' as const, username: proxy.auth.username, password: proxy.auth.password }
      : { type: 'none' as const },
    remoteDns: proxy.remoteDns,
    noProxy: proxy.noProxy,
  };
}

export const UpstreamProxyPolicyFields = ({ value, onChange }: UpstreamProxyPolicyFieldsProps) => {
  const { t } = useTranslation();
  const customProxy = value.mode === 'custom' ? value.proxy : null;

  const setMode = (mode: SavedUpstreamProxyPolicy['mode']) => {
    if (mode === 'custom') {
      onChange({ mode, proxy: customProxy ?? defaultSavedUpstreamProxyConfig() });
      return;
    }
    onChange({ mode });
  };

  const patchProxy = (patch: Partial<SavedUpstreamProxyConfig>) => {
    onChange({ mode: 'custom', proxy: { ...(customProxy ?? defaultSavedUpstreamProxyConfig()), ...patch } });
  };

  const setAuth = (auth: SavedUpstreamProxyAuth) => {
    patchProxy({ auth });
  };

  return (
    <div className="grid gap-4 border-t border-theme-border pt-4">
      <div className="grid gap-2">
        <Label>{t('modals.upstream_proxy.policy')}</Label>
        <Select value={value.mode} onValueChange={(mode) => setMode(mode as SavedUpstreamProxyPolicy['mode'])}>
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="use_global">{t('modals.upstream_proxy.use_global')}</SelectItem>
            <SelectItem value="direct">{t('modals.upstream_proxy.direct')}</SelectItem>
            <SelectItem value="custom">{t('modals.upstream_proxy.custom')}</SelectItem>
          </SelectContent>
        </Select>
        <p className="text-xs text-theme-text-muted">{t('modals.upstream_proxy.policy_hint')}</p>
      </div>

      {customProxy && (
        <div className="grid gap-4">
          <div className="grid grid-cols-2 gap-4">
            <div className="grid gap-2">
              <Label>{t('settings_view.network.protocol')}</Label>
              <Select
                value={customProxy.protocol}
                onValueChange={(protocol) => patchProxy({ protocol: protocol as SavedUpstreamProxyProtocol })}
              >
                <SelectTrigger>
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
              <Input
                type="number"
                value={customProxy.port}
                min={1}
                max={65535}
                onChange={(event) => {
                  const parsed = Number.parseInt(event.target.value, 10);
                  patchProxy({ port: Number.isFinite(parsed) ? Math.min(65535, Math.max(1, parsed)) : 1080 });
                }}
              />
            </div>
          </div>

          <div className="grid gap-2">
            <Label>{t('settings_view.network.host')}</Label>
            <Input value={customProxy.host} onChange={(event) => patchProxy({ host: event.target.value })} placeholder="127.0.0.1" />
          </div>

          <div className="grid gap-2">
            <Label>{t('settings_view.network.no_proxy')}</Label>
            <Input
              value={customProxy.noProxy}
              onChange={(event) => patchProxy({ noProxy: event.target.value })}
              placeholder="localhost,127.0.0.1,*.internal"
            />
          </div>

          <div className="flex items-center space-x-2">
            <Checkbox
              id="upstream-remote-dns"
              checked={customProxy.remoteDns}
              onCheckedChange={(checked) => patchProxy({ remoteDns: !!checked })}
            />
            <Label htmlFor="upstream-remote-dns" className="font-normal">{t('settings_view.network.remote_dns')}</Label>
          </div>

          <div className="grid gap-2">
            <Label>{t('settings_view.network.auth')}</Label>
            <Select
              value={customProxy.auth.type}
              onValueChange={(authType) => {
                if (authType === 'password') {
                  setAuth({ type: 'password', username: customProxy.auth.type === 'password' ? customProxy.auth.username : '' });
                } else {
                  setAuth({ type: 'none' });
                }
              }}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">{t('settings_view.network.auth_none')}</SelectItem>
                <SelectItem value="password">{t('settings_view.network.auth_password')}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {customProxy.auth.type === 'password' && (
            <div className="grid gap-4">
              <div className="grid gap-2">
                <Label>{t('settings_view.network.username')}</Label>
                <Input
                  value={customProxy.auth.username}
                  onChange={(event) => setAuth({ ...customProxy.auth, username: event.target.value })}
                />
              </div>
              <div className="grid gap-2">
                <Label>{t('settings_view.network.password')}</Label>
                <Input
                  type="password"
                  value={customProxy.auth.password ?? ''}
                  placeholder={customProxy.auth.keychain_id ? t('settings_view.network.password_saved_placeholder') : ''}
                  onChange={(event) => {
                    // The draft password is sent only in the explicit save request.
                    setAuth({ ...customProxy.auth, password: event.target.value || undefined });
                  }}
                />
                <p className="text-xs text-theme-text-muted">
                  {customProxy.auth.keychain_id
                    ? t('settings_view.network.password_saved_hint')
                    : t('settings_view.network.password_hint')}
                </p>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
