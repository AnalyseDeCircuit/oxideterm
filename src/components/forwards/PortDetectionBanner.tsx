// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Port Detection Banner
 *
 * Shows a compact notification when new listening ports are detected on the remote host.
 * Users can one-click forward a port or dismiss the notification.
 *
 * Lives inside ForwardsView, above the existing content.
 */

import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Radio, X, ArrowRight } from 'lucide-react';
import { Button } from '../ui/button';
import { api } from '../../lib/api';
import { useToast } from '../../hooks/useToast';
import type { DetectedPort } from '../../types';

const DEFAULT_FORWARD_HOST = 'localhost';

const assertForwardCreated = (response: { success: boolean; error?: string }): void => {
  if (!response.success) {
    throw new Error(response.error || 'Failed to create port forward');
  }
};

interface PortDetectionBannerProps {
  /** Newly detected ports to show in the banner */
  newPorts: DetectedPort[];
  /** NodeId for creating the forward */
  nodeId: string;
  /** Dismiss a single port */
  onDismiss: (port: number) => void;
  /** Callback after forward is successfully created */
  onForwardCreated?: () => void;
}

export const PortDetectionBanner: React.FC<PortDetectionBannerProps> = ({
  newPorts,
  nodeId,
  onDismiss,
  onForwardCreated,
}) => {
  const { t } = useTranslation();
  const { toast } = useToast();

  const handleForward = useCallback(
    async (port: DetectedPort) => {
      try {
        const response = await api.nodeCreateForward({
          node_id: nodeId,
          forward_type: 'local',
          bind_address: DEFAULT_FORWARD_HOST,
          bind_port: port.port,
          target_host: DEFAULT_FORWARD_HOST,
          target_port: port.port,
          description: port.process_name
            ? `${port.process_name} (${t('forwards.detection.auto')})`
            : `${t('forwards.detection.port')} ${port.port} (${t('forwards.detection.auto')})`,
          check_health: true,
        });
        assertForwardCreated(response);
        onDismiss(port.port);
        onForwardCreated?.();
        toast({
          title: t('forwards.detection.forwarded'),
          description: `${DEFAULT_FORWARD_HOST}:${port.port} → remote:${port.port}`,
        });
      } catch (error) {
        toast({
          title: t('forwards.detection.forwardError'),
          description: error instanceof Error ? error.message : String(error),
          variant: 'error',
        });
      }
    },
    [nodeId, onDismiss, onForwardCreated, toast, t]
  );

  if (newPorts.length === 0) return null;

  return (
    <div className="space-y-1.5">
      {newPorts.map((port) => (
        <div
          key={port.port}
          className="flex items-center justify-between gap-3 px-3 py-2 rounded-md border border-blue-500/30 bg-blue-500/5 text-sm"
        >
          <div className="flex items-center gap-2 min-w-0">
            <Radio className="h-3.5 w-3.5 text-blue-400 shrink-0" />
            <span className="text-theme-text truncate">
              {t('forwards.detection.detected')}
              {' '}
              <span className="font-mono font-medium text-blue-300">
                :{port.port}
              </span>
              {port.process_name && (
                <span className="text-theme-text-muted ml-1">
                  ({port.process_name}
                  {port.pid ? ` #${port.pid}` : ''})
                </span>
              )}
            </span>
          </div>
          <div className="flex items-center gap-1 shrink-0">
            <Button
              size="sm"
              variant="ghost"
              className="h-6 px-2 text-xs gap-1 text-blue-400 hover:text-blue-300"
              onClick={() => handleForward(port)}
            >
              <ArrowRight className="h-3 w-3" />
              {t('forwards.detection.forward')}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-6 w-6 p-0 text-theme-text-muted hover:text-theme-text-muted"
              onClick={() => onDismiss(port.port)}
            >
              <X className="h-3 w-3" />
            </Button>
          </div>
        </div>
      ))}
    </div>
  );
};
