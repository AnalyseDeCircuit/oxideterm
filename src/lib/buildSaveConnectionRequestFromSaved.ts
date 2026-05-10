import type { SavedConnectionForConnect } from '@/lib/api';
import type { ConnectionInfo, SaveConnectionRequest } from '@/types';

type SaveableConnectionMetadata = Pick<
  ConnectionInfo,
  | 'id'
  | 'name'
  | 'group'
  | 'host'
  | 'port'
  | 'username'
  | 'color'
  | 'tags'
  | 'agent_forwarding'
  | 'post_connect_command'
>;

type SaveConnectionOverrides = Partial<Omit<SaveConnectionRequest, 'proxy_chain'>>;

function hasOverride<Key extends keyof SaveConnectionOverrides>(
  overrides: SaveConnectionOverrides,
  key: Key,
): overrides is SaveConnectionOverrides & Required<Pick<SaveConnectionOverrides, Key>> {
  return Object.prototype.hasOwnProperty.call(overrides, key);
}

export function buildSaveConnectionRequestFromSaved(
  connection: SaveableConnectionMetadata,
  saved: SavedConnectionForConnect,
  overrides: SaveConnectionOverrides = {},
): SaveConnectionRequest {
  return {
    id: hasOverride(overrides, 'id') ? overrides.id : connection.id,
    name: hasOverride(overrides, 'name') ? overrides.name : connection.name,
    group: hasOverride(overrides, 'group') ? overrides.group : connection.group,
    host: hasOverride(overrides, 'host') ? overrides.host : connection.host,
    port: hasOverride(overrides, 'port') ? overrides.port : connection.port,
    username: hasOverride(overrides, 'username') ? overrides.username : connection.username,
    auth_type: hasOverride(overrides, 'auth_type') ? overrides.auth_type : saved.auth_type,
    password: hasOverride(overrides, 'password') ? overrides.password : saved.password,
    key_path: hasOverride(overrides, 'key_path') ? overrides.key_path : saved.key_path,
    cert_path: hasOverride(overrides, 'cert_path') ? overrides.cert_path : saved.cert_path,
    passphrase: hasOverride(overrides, 'passphrase') ? overrides.passphrase : saved.passphrase,
    color: hasOverride(overrides, 'color')
      ? overrides.color
      : connection.color ?? undefined,
    tags: hasOverride(overrides, 'tags') ? overrides.tags : connection.tags,
    agent_forwarding: hasOverride(overrides, 'agent_forwarding')
      ? overrides.agent_forwarding
      : connection.agent_forwarding ?? saved.agent_forwarding,
    post_connect_command: hasOverride(overrides, 'post_connect_command')
      ? overrides.post_connect_command
      : connection.post_connect_command ?? saved.post_connect_command ?? null,
    proxy_chain: saved.proxy_chain.length
      ? saved.proxy_chain.map((hop) => ({
          host: hop.host,
          port: hop.port,
          username: hop.username,
          auth_type: hop.auth_type,
          password: hop.password,
          key_path: hop.key_path,
          cert_path: hop.cert_path,
          passphrase: hop.passphrase,
          agent_forwarding: hop.agent_forwarding,
        }))
      : undefined,
  };
}
