import { describe, expect, it } from 'vitest';

import {
  buildSavedConnectionTestRequest,
  buildTestConnectionRequest,
  requiresSavedConnectionPasswordPrompt,
} from '@/lib/testConnectionRequest';

describe('testConnectionRequest helpers', () => {
  it('detects saved password connections without stored passwords', () => {
    expect(requiresSavedConnectionPasswordPrompt({ auth_type: 'password', password: undefined })).toBe(true);
    expect(requiresSavedConnectionPasswordPrompt({ auth_type: 'key', password: undefined })).toBe(false);
  });

  it('maps saved key auth without key_path to default_key test request', () => {
    expect(buildSavedConnectionTestRequest({
      name: 'Default Key',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'key',
      passphrase: 'secret',
      agent_forwarding: false,
      proxy_chain: [],
    })).toEqual({
      name: 'Default Key',
      host: 'example.com',
      port: 22,
      username: 'tester',
      auth_type: 'default_key',
      passphrase: 'secret',
    });
  });

  it('builds password test requests without nullable fields', () => {
    expect(buildTestConnectionRequest({
      host: 'example.com',
      port: 22,
      username: 'tester',
      authType: 'password',
      password: 'top-secret',
    })).toEqual({
      host: 'example.com',
      port: 22,
      username: 'tester',
      name: undefined,
      auth_type: 'password',
      password: 'top-secret',
    });
  });

  it('includes proxy-chain hops in saved connection test requests', () => {
    expect(buildSavedConnectionTestRequest({
      name: 'Jump Target',
      host: 'target.example.com',
      port: 22,
      username: 'target-user',
      auth_type: 'agent',
      agent_forwarding: false,
      proxy_chain: [
        {
          host: 'jump-1.example.com',
          port: 22,
          username: 'jump1',
          auth_type: 'password',
          password: 'secret',
          agent_forwarding: false,
        },
        {
          host: 'jump-2.example.com',
          port: 2222,
          username: 'jump2',
          auth_type: 'key',
          key_path: '/tmp/id_jump2',
          passphrase: 'pp',
          agent_forwarding: false,
        },
      ],
    })).toEqual({
      name: 'Jump Target',
      host: 'target.example.com',
      port: 22,
      username: 'target-user',
      auth_type: 'agent',
      proxy_chain: [
        {
          host: 'jump-1.example.com',
          port: 22,
          username: 'jump1',
          auth_type: 'password',
          password: 'secret',
        },
        {
          host: 'jump-2.example.com',
          port: 2222,
          username: 'jump2',
          auth_type: 'key',
          key_path: '/tmp/id_jump2',
          passphrase: 'pp',
        },
      ],
    });
  });

  it('rejects keyboard-interactive proxy hops before building a saved test request', () => {
    expect(() => buildSavedConnectionTestRequest({
      name: 'Jump Target',
      host: 'target.example.com',
      port: 22,
      username: 'target-user',
      auth_type: 'agent',
      agent_forwarding: false,
      proxy_chain: [
        {
          host: 'jump-1.example.com',
          port: 22,
          username: 'jump1',
          auth_type: 'keyboard_interactive',
          agent_forwarding: false,
        },
      ],
    } as never)).toThrow('Proxy hop 1 does not support keyboard-interactive authentication');
  });

  it('fails closed when a manual proxy hop carries an unsupported auth type', () => {
    expect(() => buildTestConnectionRequest({
      host: 'target.example.com',
      port: 22,
      username: 'target-user',
      authType: 'agent',
      proxyChain: [
        {
          host: 'jump-1.example.com',
          port: 22,
          username: 'jump1',
          authType: 'keyboard_interactive' as never,
        },
      ],
    })).toThrow('Unsupported proxy hop authentication type keyboard_interactive');
  });

  it('rejects key and certificate requests when required paths are missing', () => {
    expect(() => buildTestConnectionRequest({
      host: 'example.com',
      port: 22,
      username: 'tester',
      authType: 'key',
    })).toThrow('SSH key path is required for key authentication');

    expect(() => buildTestConnectionRequest({
      host: 'example.com',
      port: 22,
      username: 'tester',
      authType: 'certificate',
      keyPath: '/tmp/id_ed25519',
    })).toThrow('Certificate path is required for certificate authentication');
  });
});