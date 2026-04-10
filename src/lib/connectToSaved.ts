// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { api } from './api';
import { findUnsupportedProxyHopAuth } from './proxyHopSupport';
import { useSessionTreeStore } from '../store/sessionTreeStore';
import { useAppStore } from '../store/appStore';
import type { UnifiedFlatNode } from '../types';
import type { ToastVariant } from '../hooks/useToast';

export type ConnectToSavedOptions = {
  createTab: (type: 'terminal', sessionId: string) => void;
  toast: (props: { title: string; description: string; variant?: ToastVariant }) => void;
  t: (key: string, options?: Record<string, unknown>) => string;
  onError?: (connectionId: string, reason?: 'missing-password' | 'connect-failed') => void;
};

/**
 * Connect to a saved connection configuration.
 *
 * Flow:
 * 1. Get full credentials via getSavedConnectionForConnect
 * 2. proxy_chain → expandManualPreset → connectNodeWithAncestors → createTerminalForNode
 * 3. No proxy_chain → check existing node / addRootNode → connectNodeWithAncestors
 * 4. Open terminal tab, mark connection used
 */
export async function connectToSaved(
  connectionId: string,
  options: ConnectToSavedOptions,
): Promise<void> {
  const { createTab, toast, t, onError } = options;

  const mapAuthType = (authType: string): 'password' | 'key' | 'agent' | undefined => {
    if (authType === 'agent') return 'agent';
    if (authType === 'key') return 'key';
    if (authType === 'password') return 'password';
    return undefined; // default_key
  };

  const mapPresetAuthType = (authType: string): 'password' | 'key' | 'agent' => {
    if (authType === 'agent') return 'agent';
    if (authType === 'key') return 'key';
    if (authType === 'password') return 'password';
    return 'key'; // default_key fallback to key
  };

  try {
    const savedConn = await api.getSavedConnectionForConnect(connectionId);

    const requiresPasswordPrompt = (authType?: string, password?: string) => authType === 'password' && !password;

    // ========== Proxy Chain 支持 ==========
    if (savedConn.proxy_chain && savedConn.proxy_chain.length > 0) {
      const unsupportedProxyHop = findUnsupportedProxyHopAuth(savedConn.proxy_chain);
      if (unsupportedProxyHop) {
        toast({
          title: t('connections.toast.proxy_chain_invalid'),
          description: unsupportedProxyHop.reason === 'keyboard_interactive'
            ? t('connections.toast.proxy_hop_kbi_unsupported', { hop: unsupportedProxyHop.hopIndex })
            : t('connections.toast.proxy_hop_auth_unsupported', {
              hop: unsupportedProxyHop.hopIndex,
              authType: unsupportedProxyHop.authType,
            }),
          variant: 'error',
        });
        onError?.(connectionId, 'connect-failed');
        return;
      }

      const { expandManualPreset, connectNodeWithAncestors, createTerminalForNode } = useSessionTreeStore.getState();

      const hops = savedConn.proxy_chain.map((hop: { host: string; port: number; username: string; auth_type: string; password?: string; key_path?: string; passphrase?: string; agent_forwarding: boolean }) => ({
        host: hop.host,
        port: hop.port,
        username: hop.username,
        authType: mapPresetAuthType(hop.auth_type),
        password: hop.password,
        keyPath: hop.key_path,
        passphrase: hop.passphrase,
        agentForwarding: hop.agent_forwarding,
      }));

      const target = {
        host: savedConn.host,
        port: savedConn.port,
        username: savedConn.username,
        authType: mapPresetAuthType(savedConn.auth_type),
        password: savedConn.password,
        keyPath: savedConn.key_path,
        passphrase: savedConn.passphrase,
        agentForwarding: savedConn.agent_forwarding,
      };

      const request = {
        savedConnectionId: connectionId,
        hops,
        target,
      };

      // Step 1: Expand preset chain into tree nodes (no connections yet)
      const expandResult = await expandManualPreset(request);

      // Step 2: Connect the entire chain using linear connector
      await connectNodeWithAncestors(expandResult.targetNodeId);

      // Step 3: Create terminal for target node and open tab
      const terminalId = await createTerminalForNode(expandResult.targetNodeId);
      createTab('terminal', terminalId);

      toast({
        title: t('connections.toast.proxy_chain_established'),
        description: t('connections.toast.proxy_chain_desc', { depth: expandResult.chainDepth }),
        variant: 'success',
      });

      await api.markConnectionUsed(connectionId);
      return;
    }

    // ========== Direct connection (no proxy_chain) ==========
    const { nodes } = useSessionTreeStore.getState();
    const { addRootNode } = useSessionTreeStore.getState();
    const existingNode = nodes.find((n: UnifiedFlatNode) =>
      n.depth === 0 &&
      n.host === savedConn.host &&
      n.port === savedConn.port &&
      n.username === savedConn.username
    );

    const canReuseActiveNode = !!existingNode && existingNode.runtime.status === 'active';
    if (!canReuseActiveNode && requiresPasswordPrompt(savedConn.auth_type, savedConn.password)) {
      onError?.(connectionId, 'missing-password');
      return;
    }

    let nodeId: string;

    if (existingNode) {
      nodeId = existingNode.id;
      useSessionTreeStore.setState({ selectedNodeId: nodeId });

      if (existingNode.runtime.status === 'idle' || existingNode.runtime.status === 'error') {
        const { connectNodeWithAncestors } = useSessionTreeStore.getState();
        await connectNodeWithAncestors(nodeId);
      }
    } else {
      nodeId = await addRootNode({
        host: savedConn.host,
        port: savedConn.port,
        username: savedConn.username,
        authType: mapAuthType(savedConn.auth_type),
        password: savedConn.password,
        keyPath: savedConn.key_path,
        passphrase: savedConn.passphrase,
        displayName: savedConn.name,
        agentForwarding: savedConn.agent_forwarding,
      });

      const { connectNodeWithAncestors } = useSessionTreeStore.getState();
      await connectNodeWithAncestors(nodeId);
    }

    // Check if target already has a terminal — create one + open tab if not
    const updatedNode = useSessionTreeStore.getState().getNode(nodeId);
    const terminalIds = updatedNode?.runtime?.terminalIds || [];

    if (terminalIds.length > 0) {
      // Reuse existing terminal
      const existingTab = useAppStore.getState().tabs.find(tab => tab.sessionId === terminalIds[0] && tab.type === 'terminal');
      if (existingTab) {
        useAppStore.setState({ activeTabId: existingTab.id });
      } else {
        createTab('terminal', terminalIds[0]);
      }
    } else {
      // Create new terminal
      const { createTerminalForNode } = useSessionTreeStore.getState();
      const terminalId = await createTerminalForNode(nodeId);
      createTab('terminal', terminalId);
    }

    await api.markConnectionUsed(connectionId);
  } catch (error) {
    console.error('Failed to connect to saved connection:', error);
    const errorMsg = String(error);
    if (!errorMsg.includes('already connecting') &&
      !errorMsg.includes('already connected') &&
      !errorMsg.includes('CHAIN_LOCK_BUSY') &&
      !errorMsg.includes('NODE_LOCK_BUSY')) {
      onError?.(connectionId, 'connect-failed');
    }
  }
}
