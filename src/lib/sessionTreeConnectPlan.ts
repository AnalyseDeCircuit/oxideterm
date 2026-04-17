import { api } from './api';
import { useSessionTreeStore } from '../store/sessionTreeStore';
import type { HostKeyStatus } from '../types';

export type SessionTreeConnectStep = {
  nodeId: string;
  host: string;
  port: number;
  trustHostKey?: boolean;
  expectedHostKeyFingerprint?: string;
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

const hasAcceptedFingerprint = (step: SessionTreeConnectStep) =>
  typeof step.expectedHostKeyFingerprint === 'string' && typeof step.trustHostKey === 'boolean';

export async function continueSessionTreeConnectPlan(
  plan: SessionTreeConnectPlan,
): Promise<SessionTreeConnectChallenge | null> {
  const { connectNode } = useSessionTreeStore.getState();

  for (let index = plan.currentIndex; index < plan.steps.length; index += 1) {
    const step = plan.steps[index];

    if (hasAcceptedFingerprint(step)) {
      await connectNode(step.nodeId, {
        trustHostKey: step.trustHostKey,
        expectedHostKeyFingerprint: step.expectedHostKeyFingerprint,
      });
      continue;
    }

    // Proxy-chain host keys must be verified hop-by-hop. Later hops may only be
    // reachable after the previous node is already connected, so we cannot
    // preflight the whole chain from the client up front.
    const preflight = await api.preflightTreeNode(step.nodeId);

    switch (preflight.status) {
      case 'verified':
        await connectNode(step.nodeId);
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