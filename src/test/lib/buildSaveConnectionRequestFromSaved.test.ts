import { describe, expect, it } from 'vitest';

import type { SavedConnectionForConnect } from '@/lib/api';
import { buildSaveConnectionRequestFromSaved } from '@/lib/buildSaveConnectionRequestFromSaved';
import type { ConnectionInfo } from '@/types';

const connection: ConnectionInfo = {
  id: 'conn-1',
  name: 'Production',
  group: 'Ops',
  host: 'prod.internal',
  port: 22,
  username: 'alice',
  auth_type: 'certificate',
  key_path: '/tmp/id_ed25519',
  cert_path: '/tmp/id_ed25519-cert.pub',
  created_at: '2024-01-01T00:00:00.000Z',
  last_used_at: null,
  color: null,
  tags: ['team:red'],
  agent_forwarding: true,
  post_connect_command: 'cd /srv/prod',
  proxy_chain: [],
};

const saved: SavedConnectionForConnect = {
  name: 'Production',
  host: 'prod.internal',
  port: 22,
  username: 'alice',
  auth_type: 'certificate',
  key_path: '/tmp/id_ed25519',
  cert_path: '/tmp/id_ed25519-cert.pub',
  passphrase: 'secret',
  agent_forwarding: true,
  post_connect_command: 'cd /srv/prod',
  proxy_chain: [],
};

describe('buildSaveConnectionRequestFromSaved', () => {
  it('preserves an explicit undefined id override for duplicates', () => {
    const request = buildSaveConnectionRequestFromSaved(connection, saved, {
      id: undefined,
      name: 'Production (Copy)',
    });

    expect(request.id).toBeUndefined();
    expect(request.name).toBe('Production (Copy)');
  });

  it('preserves an explicit null group override when moving to ungrouped', () => {
    const request = buildSaveConnectionRequestFromSaved(connection, saved, {
      group: null,
    });

    expect(request.group).toBeNull();
  });

  it('preserves the saved post-connect command', () => {
    const request = buildSaveConnectionRequestFromSaved(connection, saved);

    expect(request.post_connect_command).toBe('cd /srv/prod');
  });
});
