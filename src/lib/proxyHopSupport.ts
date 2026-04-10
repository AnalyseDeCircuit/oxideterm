import type { SavedConnectionProxyHopForConnect } from './api';
import type { ProxyHopConfig } from '@/types';

export type SupportedProxyHopAuthType = 'password' | 'key' | 'default_key' | 'agent';

export type UnsupportedProxyHopAuth = {
  hopIndex: number;
  host: string;
  username: string;
  authType: string;
  reason: 'keyboard_interactive' | 'unsupported_auth_type';
};

type ProxyHopLike = Pick<
  SavedConnectionProxyHopForConnect | ProxyHopConfig,
  'host' | 'username' | 'auth_type'
>;

export function isSupportedProxyHopAuthType(authType: string): authType is SupportedProxyHopAuthType {
  return authType === 'password'
    || authType === 'key'
    || authType === 'default_key'
    || authType === 'agent';
}

export function findUnsupportedProxyHopAuth(proxyChain: ProxyHopLike[]): UnsupportedProxyHopAuth | null {
  for (let index = 0; index < proxyChain.length; index += 1) {
    const hop = proxyChain[index];
    if (hop.auth_type === 'keyboard_interactive') {
      return {
        hopIndex: index + 1,
        host: hop.host,
        username: hop.username,
        authType: hop.auth_type,
        reason: 'keyboard_interactive',
      };
    }

    if (!isSupportedProxyHopAuthType(hop.auth_type)) {
      return {
        hopIndex: index + 1,
        host: hop.host,
        username: hop.username,
        authType: hop.auth_type,
        reason: 'unsupported_auth_type',
      };
    }
  }

  return null;
}