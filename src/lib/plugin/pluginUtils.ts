/**
 * Shared Plugin Utilities
 *
 * Common helpers used across plugin system modules.
 * Extracted to avoid code duplication and circular imports.
 */

import type { SshConnectionInfo } from '../../types';
import type { ConnectionSnapshot } from '../../types/plugin';

/**
 * Convert SshConnectionInfo to a frozen ConnectionSnapshot for plugin consumption.
 * All fields are deep-frozen to prevent mutation by plugin code.
 */
export function toSnapshot(conn: SshConnectionInfo): ConnectionSnapshot {
  return Object.freeze({
    id: conn.id,
    host: conn.host,
    port: conn.port,
    username: conn.username,
    state: conn.state,
    refCount: conn.refCount,
    keepAlive: conn.keepAlive,
    createdAt: conn.createdAt,
    lastActive: conn.lastActive,
    terminalIds: Object.freeze([...conn.terminalIds]),
    parentConnectionId: conn.parentConnectionId,
  });
}
