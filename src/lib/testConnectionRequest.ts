// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type {
  SavedConnectionForConnect,
  SavedConnectionProxyHopForConnect,
  TestConnectionProxyHop,
  TestConnectionRequest,
} from './api';
import type { ProxyHopConfig } from '@/types';
import { findUnsupportedProxyHopAuth } from './proxyHopSupport';

type ManualTestConnectionInput = {
  host: string;
  port: number;
  username: string;
  name?: string;
  authType: 'password' | 'key' | 'default_key' | 'agent' | 'certificate';
  password?: string | null;
  keyPath?: string | null;
  certPath?: string | null;
  passphrase?: string | null;
  trustHostKey?: boolean;
  expectedHostKeyFingerprint?: string;
  proxyChain?: Array<ManualProxyHopInput> | null;
};

type ManualProxyHopInput = {
  host: string;
  port: number;
  username: string;
  authType: 'password' | 'key' | 'default_key' | 'agent' | 'certificate';
  password?: string | null;
  keyPath?: string | null;
  certPath?: string | null;
  passphrase?: string | null;
};

function buildProxyHopRequest(input: ManualProxyHopInput): TestConnectionProxyHop {
  switch (input.authType) {
    case 'password': {
      if (!input.password) {
        throw new Error(`Password is required for proxy hop ${input.username}@${input.host}`);
      }
      return {
        host: input.host,
        port: input.port,
        username: input.username,
        auth_type: 'password',
        password: input.password,
      };
    }
    case 'key': {
      if (!input.keyPath) {
        throw new Error(`SSH key path is required for proxy hop ${input.username}@${input.host}`);
      }
      return {
        host: input.host,
        port: input.port,
        username: input.username,
        auth_type: 'key',
        key_path: input.keyPath,
        passphrase: input.passphrase ?? undefined,
      };
    }
    case 'default_key':
      return {
        host: input.host,
        port: input.port,
        username: input.username,
        auth_type: 'default_key',
        passphrase: input.passphrase ?? undefined,
      };
    case 'certificate': {
      if (!input.keyPath) {
        throw new Error(`SSH key path is required for proxy hop ${input.username}@${input.host}`);
      }
      if (!input.certPath) {
        throw new Error(`Certificate path is required for proxy hop ${input.username}@${input.host}`);
      }
      return {
        host: input.host,
        port: input.port,
        username: input.username,
        auth_type: 'certificate',
        key_path: input.keyPath,
        cert_path: input.certPath,
        passphrase: input.passphrase ?? undefined,
      };
    }
    case 'agent':
      return {
        host: input.host,
        port: input.port,
        username: input.username,
        auth_type: 'agent',
      };
    default:
      throw new Error(`Unsupported proxy hop authentication type ${String(input.authType)}`);
  }
}

function normalizeProxyHopInput(
  proxyHop: ProxyHopConfig | SavedConnectionProxyHopForConnect,
): ManualProxyHopInput {
  return {
    host: proxyHop.host,
    port: proxyHop.port,
    username: proxyHop.username,
    authType: proxyHop.auth_type as ManualProxyHopInput['authType'],
    password: proxyHop.password,
    keyPath: proxyHop.key_path,
    certPath: proxyHop.cert_path,
    passphrase: proxyHop.passphrase,
  };
}

export function requiresSavedConnectionPasswordPrompt(
  connection: Pick<SavedConnectionForConnect, 'auth_type' | 'password'>,
): boolean {
  return connection.auth_type === 'password' && !connection.password;
}

export function buildTestConnectionRequest(
  input: ManualTestConnectionInput,
): TestConnectionRequest {
  const base = {
    host: input.host,
    port: input.port,
    username: input.username,
    name: input.name,
    trust_host_key: input.trustHostKey,
    expected_host_key_fingerprint: input.expectedHostKeyFingerprint,
    proxy_chain: input.proxyChain?.length ? input.proxyChain.map(buildProxyHopRequest) : undefined,
  };

  switch (input.authType) {
    case 'password': {
      if (!input.password) {
        throw new Error('Password is required for password authentication');
      }
      return {
        ...base,
        auth_type: 'password',
        password: input.password,
      };
    }
    case 'key': {
      if (!input.keyPath) {
        throw new Error('SSH key path is required for key authentication');
      }
      return {
        ...base,
        auth_type: 'key',
        key_path: input.keyPath,
        passphrase: input.passphrase ?? undefined,
      };
    }
    case 'default_key': {
      return {
        ...base,
        auth_type: 'default_key',
        passphrase: input.passphrase ?? undefined,
      };
    }
    case 'certificate': {
      if (!input.keyPath) {
        throw new Error('SSH key path is required for certificate authentication');
      }
      if (!input.certPath) {
        throw new Error('Certificate path is required for certificate authentication');
      }
      return {
        ...base,
        auth_type: 'certificate',
        key_path: input.keyPath,
        cert_path: input.certPath,
        passphrase: input.passphrase ?? undefined,
      };
    }
    case 'agent':
    default:
      return {
        ...base,
        auth_type: 'agent',
      };
  }
}

export function buildSavedConnectionTestRequest(
  connection: SavedConnectionForConnect,
): TestConnectionRequest {
  const unsupportedProxyHop = findUnsupportedProxyHopAuth(connection.proxy_chain);
  if (unsupportedProxyHop) {
    throw new Error(
      unsupportedProxyHop.reason === 'keyboard_interactive'
        ? `Proxy hop ${unsupportedProxyHop.hopIndex} does not support keyboard-interactive authentication`
        : `Proxy hop ${unsupportedProxyHop.hopIndex} uses unsupported authentication type ${unsupportedProxyHop.authType}`,
    );
  }

  return buildTestConnectionRequest({
    host: connection.host,
    port: connection.port,
    username: connection.username,
    name: connection.name,
    authType: connection.auth_type === 'key' && !connection.key_path
      ? 'default_key'
      : connection.auth_type,
    password: connection.password,
    keyPath: connection.key_path,
    certPath: connection.cert_path,
    passphrase: connection.passphrase,
    proxyChain: connection.proxy_chain.map(normalizeProxyHopInput),
  });
}