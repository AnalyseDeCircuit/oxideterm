// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { api } from './api';
import { findUnsupportedProxyHopAuth } from './proxyHopSupport';
import { notifyConnectionIssue } from './notificationCenter';
import {
  cleanupSessionTreeConnectPlan,
  continueSessionTreeConnectPlan,
  type SessionTreeConnectPlan,
} from './sessionTreeConnectPlan';
import { requiresSavedConnectionPasswordPrompt } from './testConnectionRequest';
import { useSessionTreeStore } from '../store/sessionTreeStore';
import { useAppStore } from '../store/appStore';
import type { HostKeyStatus, UnifiedFlatNode } from '../types';
import type { ToastVariant } from '../hooks/useToast';

export type ConnectToSavedOptions = {
  createTab: (type: 'terminal', sessionId: string) => void;
  toast: (props: { title: string; description: string; variant?: ToastVariant }) => void;
  t: (key: string, options?: Record<string, unknown>) => string;
  onError?: (connectionId: string, reason?: 'missing-password' | 'connect-failed') => void;
  onHostKeyChallenge?: (challenge: ConnectToSavedHostKeyChallenge) => void;
};

export type ConnectToSavedResult = {
  nodeId: string;
  sessionId: string;
};

export type PendingSavedConnectionPlan = {
  connectionId: string;
  plan: SessionTreeConnectPlan;
};

export type ConnectToSavedHostKeyChallenge = {
  pendingPlan: PendingSavedConnectionPlan;
  host: string;
  port: number;
  status: Extract<HostKeyStatus, { status: 'unknown' } | { status: 'changed' }>;
};

async function finalizeConnectedSavedNode(
  connectionId: string,
  nodeId: string,
  options: ConnectToSavedOptions,
): Promise<ConnectToSavedResult> {
  const { createTab } = options;
  let sessionId: string;

  const updatedNode = useSessionTreeStore.getState().getNode(nodeId);
  const terminalIds = updatedNode?.runtime?.terminalIds || [];

  if (terminalIds.length > 0) {
    sessionId = terminalIds[0];
    const existingTab = useAppStore.getState().tabs.find(tab => tab.sessionId === terminalIds[0] && tab.type === 'terminal');
    if (existingTab) {
      useAppStore.setState({ activeTabId: existingTab.id });
    } else {
      createTab('terminal', terminalIds[0]);
    }
  } else {
    const { createTerminalForNode } = useSessionTreeStore.getState();
    const terminalId = await createTerminalForNode(nodeId);
    createTab('terminal', terminalId);
    sessionId = terminalId;
  }

  await api.markConnectionUsed(connectionId);
  return { nodeId, sessionId };
}

async function runSavedConnectionPlan(
  pendingPlan: PendingSavedConnectionPlan,
  options: ConnectToSavedOptions,
): Promise<ConnectToSavedResult | null> {
  const challenge = await continueSessionTreeConnectPlan(pendingPlan.plan);

  if (challenge) {
    options.onHostKeyChallenge?.({
      pendingPlan: { ...pendingPlan, plan: challenge.plan },
      host: challenge.step.host,
      port: challenge.step.port,
      status: challenge.status,
    });
    return null;
  }

  if (pendingPlan.plan.steps.length > 1) {
    options.toast({
      title: options.t('connections.toast.proxy_chain_established'),
      description: options.t('connections.toast.proxy_chain_desc', { depth: pendingPlan.plan.steps.length }),
      variant: 'success',
    });
  }

  return finalizeConnectedSavedNode(pendingPlan.connectionId, pendingPlan.plan.targetNodeId, options);
}

function shouldSuppressSavedConnectionError(errorMsg: string) {
  return errorMsg.includes('already connecting')
    || errorMsg.includes('already connected')
    || errorMsg.includes('CHAIN_LOCK_BUSY')
    || errorMsg.includes('NODE_LOCK_BUSY');
}

async function handleSavedConnectionFailure(
  connectionId: string,
  error: unknown,
  options: ConnectToSavedOptions,
  pendingPlan?: PendingSavedConnectionPlan,
): Promise<null> {
  const { t, onError } = options;
  const errorMsg = String(error);

  console.error('Failed to connect to saved connection:', error);

  if (pendingPlan) {
    await cleanupSessionTreeConnectPlan(pendingPlan.plan).catch((cleanupError) => {
      console.warn('Failed to clean up partial saved connection plan:', cleanupError);
    });
  }

  if (!shouldSuppressSavedConnectionError(errorMsg)) {
    notifyConnectionIssue({
      title: t('connection.errors.generic_title', { defaultValue: 'Connection Error' }),
      body: errorMsg,
      severity: 'error',
      dedupeKey: `saved-connection-failed:${connectionId}`,
    });
    onError?.(connectionId, 'connect-failed');
  }

  return null;
}

export async function continueConnectToSavedPlan(
  pendingPlan: PendingSavedConnectionPlan,
  options: ConnectToSavedOptions,
): Promise<ConnectToSavedResult | null> {
  try {
    return await runSavedConnectionPlan(pendingPlan, options);
  } catch (error) {
    return handleSavedConnectionFailure(pendingPlan.connectionId, error, options, pendingPlan);
  }
}

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
): Promise<ConnectToSavedResult | null> {
  const { toast, t, onError } = options;

  const mapAuthType = (authType: string): 'password' | 'key' | 'agent' | 'certificate' | undefined => {
    if (authType === 'agent') return 'agent';
    if (authType === 'certificate') return 'certificate';
    if (authType === 'key') return 'key';
    if (authType === 'password') return 'password';
    return undefined; // default_key
  };

  const mapPresetAuthType = (authType: string): 'password' | 'key' | 'agent' | 'certificate' => {
    if (authType === 'agent') return 'agent';
    if (authType === 'certificate') return 'certificate';
    if (authType === 'key') return 'key';
    if (authType === 'password') return 'password';
    return 'key'; // default_key fallback to key
  };

  try {
    const savedConn = await api.getSavedConnectionForConnect(connectionId);

    // ========== Proxy Chain 支持 ==========
    if (savedConn.proxy_chain && savedConn.proxy_chain.length > 0) {
      const unsupportedProxyHop = findUnsupportedProxyHopAuth(savedConn.proxy_chain);
      if (unsupportedProxyHop) {
        const description = unsupportedProxyHop.reason === 'keyboard_interactive'
          ? t('connections.toast.proxy_hop_kbi_unsupported', { hop: unsupportedProxyHop.hopIndex })
          : t('connections.toast.proxy_hop_auth_unsupported', {
            hop: unsupportedProxyHop.hopIndex,
            authType: unsupportedProxyHop.authType,
          });

        toast({
          title: t('connections.toast.proxy_chain_invalid'),
          description,
          variant: 'error',
        });

        notifyConnectionIssue({
          title: t('connections.toast.proxy_chain_invalid'),
          body: description,
          severity: 'error',
          dedupeKey: `saved-connection-proxy-invalid:${connectionId}`,
        });

        onError?.(connectionId, 'connect-failed');
        return null;
      }

      const { expandManualPreset } = useSessionTreeStore.getState();

      const hops = savedConn.proxy_chain.map((hop: { host: string; port: number; username: string; auth_type: string; password?: string; key_path?: string; cert_path?: string; passphrase?: string; agent_forwarding: boolean }) => ({
        host: hop.host,
        port: hop.port,
        username: hop.username,
        authType: mapPresetAuthType(hop.auth_type),
        password: hop.password,
        keyPath: hop.key_path,
        certPath: hop.cert_path,
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
        certPath: savedConn.cert_path,
        passphrase: savedConn.passphrase,
        agentForwarding: savedConn.agent_forwarding,
      };

      const request = {
        savedConnectionId: connectionId,
        hops,
        target,
      };

      const expandResult = await expandManualPreset(request);

      return continueConnectToSavedPlan({
        connectionId,
        plan: {
          targetNodeId: expandResult.targetNodeId,
          cleanupNodeId: expandResult.targetNodeId,
          currentIndex: 0,
          steps: expandResult.pathNodeIds.map((nodeId, index) => {
            const endpoint = index < hops.length ? hops[index] : target;
            return {
              nodeId,
              host: endpoint.host,
              port: endpoint.port,
            };
          }),
        },
      }, options);
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
    if (!canReuseActiveNode && requiresSavedConnectionPasswordPrompt(savedConn)) {
      onError?.(connectionId, 'missing-password');
      return null;
    }

    let nodeId: string;

    let sessionId: string;

    if (existingNode) {
      nodeId = existingNode.id;
      useSessionTreeStore.setState({ selectedNodeId: nodeId });

      if (existingNode.runtime.status === 'idle' || existingNode.runtime.status === 'error') {
        return continueConnectToSavedPlan({
          connectionId,
          plan: {
            targetNodeId: nodeId,
            currentIndex: 0,
            steps: [{ nodeId, host: savedConn.host, port: savedConn.port }],
          },
        }, options);
      }
    } else {
      nodeId = await addRootNode({
        host: savedConn.host,
        port: savedConn.port,
        username: savedConn.username,
        authType: mapAuthType(savedConn.auth_type),
        password: savedConn.password,
        keyPath: savedConn.key_path,
        certPath: savedConn.cert_path,
        passphrase: savedConn.passphrase,
        displayName: savedConn.name,
        agentForwarding: savedConn.agent_forwarding,
      });

      return continueConnectToSavedPlan({
        connectionId,
        plan: {
          targetNodeId: nodeId,
          cleanupNodeId: nodeId,
          currentIndex: 0,
          steps: [{ nodeId, host: savedConn.host, port: savedConn.port }],
        },
      }, options);
    }

    return finalizeConnectedSavedNode(connectionId, nodeId, options);
  } catch (error) {
    return handleSavedConnectionFailure(connectionId, error, options);
  }
}
