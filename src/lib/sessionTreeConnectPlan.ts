import { api } from './api';
import { useSessionTreeStore } from '../store/sessionTreeStore';
import type { HostKeyStatus, UpstreamProxyForConnect } from '../types';

export type SessionTreeConnectStep = {
  nodeId: string;
  host: string;
  port: number;
  trustHostKey?: boolean;
  expectedHostKeyFingerprint?: string;
  upstreamProxy?: UpstreamProxyForConnect;
};

export type SessionTreeConnectPlan = {
  targetNodeId: string;
  cleanupNodeId?: string;
  steps: SessionTreeConnectStep[];
  currentIndex: number;
};

export type SessionTreeConnectChallenge = {
  plan: SessionTreeConnectPlan;
  status: Extract<HostKeyStatus, { status: 'unknown' } | { status: 'changed' }>;
  step: SessionTreeConnectStep;
};

export async function buildExistingSessionTreeConnectPlan(
  targetNodeId: string,
  rootUpstreamProxy?: UpstreamProxyForConnect,
): Promise<SessionTreeConnectPlan> {
  const pathNodes = await useSessionTreeStore.getState().getNodePath(targetNodeId);

  if (pathNodes.length === 0) {
    throw new Error(`Node path not found for ${targetNodeId}`);
  }

  const treeStore = useSessionTreeStore.getState();
  const firstDisconnectedIndex = pathNodes.findIndex((pathNode) => {
    const currentNode = treeStore.getRawNode(pathNode.id) ?? pathNode;
    const isConnected = currentNode.state.status === 'connected';
    const hasConnectionId = !!currentNode.sshConnectionId;
    const isLinkDown = treeStore.linkDownNodeIds.has(pathNode.id);

    return !isConnected || !hasConnectionId || isLinkDown;
  });
  const startIndex = firstDisconnectedIndex === -1 ? pathNodes.length : firstDisconnectedIndex;
  const nodesToConnect = pathNodes.slice(startIndex);

  return {
    targetNodeId,
    steps: nodesToConnect.map((node, index) => ({
      nodeId: node.id,
      host: node.host,
      port: node.port,
      // Only the root hop can use the configured upstream proxy. Child hops
      // must preflight over their already-connected parent tunnel.
      upstreamProxy: startIndex === 0 && index === 0 ? rootUpstreamProxy : undefined,
    })),
    currentIndex: 0,
  };
}

const hasAcceptedFingerprint = (step: SessionTreeConnectStep) =>
  typeof step.expectedHostKeyFingerprint === 'string' && typeof step.trustHostKey === 'boolean';

const connectOptionsForStep = (step: SessionTreeConnectStep) => {
  const options = {
    trustHostKey: step.trustHostKey,
    expectedHostKeyFingerprint: step.expectedHostKeyFingerprint,
    upstreamProxy: step.upstreamProxy,
  };

  return Object.values(options).some((value) => value !== undefined) ? options : undefined;
};

async function connectStep(
  connectNode: ReturnType<typeof useSessionTreeStore.getState>['connectNode'],
  step: SessionTreeConnectStep,
) {
  const options = connectOptionsForStep(step);
  if (options) {
    await connectNode(step.nodeId, options);
  } else {
    await connectNode(step.nodeId);
  }
}

export async function continueSessionTreeConnectPlan(
  plan: SessionTreeConnectPlan,
): Promise<SessionTreeConnectChallenge | null> {
  const { connectNode } = useSessionTreeStore.getState();

  for (let index = plan.currentIndex; index < plan.steps.length; index += 1) {
    const step = plan.steps[index];

    if (hasAcceptedFingerprint(step)) {
      await connectStep(connectNode, step);
      continue;
    }

    // Proxy-chain host keys must be verified hop-by-hop. Later hops may only be
    // reachable after the previous node is already connected, so we cannot
    // preflight the whole chain from the client up front.
    const preflight = step.upstreamProxy
      ? await api.preflightTreeNode(step.nodeId, step.upstreamProxy)
      : await api.preflightTreeNode(step.nodeId);

    switch (preflight.status) {
      case 'verified':
        await connectStep(connectNode, step);
        continue;
      case 'unknown':
      case 'changed':
        return {
          plan: { ...plan, currentIndex: index },
          status: preflight,
          step,
        };
      case 'error':
        throw new Error(preflight.message);
      default:
        throw new Error('Unsupported host key preflight status');
    }
  }

  return null;
}

export async function cleanupSessionTreeConnectPlan(
  plan: Pick<SessionTreeConnectPlan, 'cleanupNodeId'> | null | undefined,
): Promise<void> {
  if (!plan?.cleanupNodeId) {
    return;
  }

  await useSessionTreeStore.getState().removeNode(plan.cleanupNodeId);
}
