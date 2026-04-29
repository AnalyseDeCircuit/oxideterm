// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Smart Port Detection Hook
 *
 * Listens for `port-detected:{connectionId}` events from the Rust profiler's
 * port scanner. Maintains a list of newly detected ports that the user hasn't
 * dismissed, enabling VS Code-like "forward this port?" notifications.
 *
 * Usage:
 * ```tsx
 * const { newPorts, allPorts, dismissPort } = usePortDetection(connectionId);
 * ```
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api } from '../lib/api';
import type { DetectedPort, PortDetectionEvent } from '../types';

export interface UsePortDetectionResult {
  /** Ports that are newly detected and not yet dismissed by the user */
  newPorts: DetectedPort[];
  /** All currently listening ports on the remote host */
  allPorts: DetectedPort[];
  /** Dismiss a port (won't be shown again until profiler restart) */
  dismissPort: (port: number) => void;
  /** Whether port detection is active (receiving data) */
  isActive: boolean;
  /** Whether the first backend scan/poll has completed */
  hasScanned: boolean;
}

/**
 * Hook to receive smart port detection events from the backend profiler.
 *
 * @param connectionId - The SSH connection ID to listen for (from topologyResolver)
 */
export function usePortDetection(connectionId: string | undefined): UsePortDetectionResult {
  const [newPorts, setNewPorts] = useState<DetectedPort[]>([]);
  const [allPorts, setAllPorts] = useState<DetectedPort[]>([]);
  const [isActive, setIsActive] = useState(false);
  const [hasScanned, setHasScanned] = useState(false);
  const dismissedRef = useRef<Set<number>>(new Set());
  const prevConnectionIdRef = useRef<string | undefined>(undefined);

  // P2 fix: clear dismiss state when connectionId changes
  if (connectionId !== prevConnectionIdRef.current) {
    prevConnectionIdRef.current = connectionId;
    dismissedRef.current = new Set();
  }

  // Dismiss a port locally and tell the backend to ignore it
  const dismissPort = useCallback(
    (port: number) => {
      dismissedRef.current.add(port);
      setNewPorts((prev) => prev.filter((p) => p.port !== port));
      if (connectionId) {
        api.ignoreDetectedPort(connectionId, port).catch(() => {});
      }
    },
    [connectionId]
  );

  useEffect(() => {
    if (!connectionId) return;

    let mounted = true;
    let unlisten: (() => void) | null = null;

    const eventName = `port-detected:${connectionId}`;

    // Ensure the resource profiler is running (it hosts the port scanner).
    // Idempotent: if already running, this is a no-op on the backend.
    let profilerStarted = false;
    const ensureProfiler = async () => {
      if (profilerStarted) return;
      try {
        await api.startResourceProfiler(connectionId);
        profilerStarted = true;
      } catch {
        // Connection may not be ready yet — will retry on next poll
      }
    };
    ensureProfiler();

    // Poll detected ports from backend (initial scan is silent — no event emitted)
    const pollPorts = async () => {
      // Retry profiler start if previous attempt failed
      if (!profilerStarted) {
        await ensureProfiler();
      }
      try {
        const ports = await api.getDetectedPorts(connectionId);
        if (!mounted) return;
        // Always update — including empty array to clear stale data
        setAllPorts(ports);
        setHasScanned(true);
        if (ports.length > 0) {
          setIsActive(true);
        }
      } catch {
        // Profiler may not be running yet
      }
    };

    // Initial poll + periodic refresh (profiler samples every 10s)
    pollPorts();
    const pollTimer = setInterval(pollPorts, 12_000);

    const setup = async () => {
      try {
        const fn = await listen<PortDetectionEvent>(eventName, (event) => {
          if (!mounted) return;
          const payload = event.payload;

          setIsActive(true);
          setHasScanned(true);
          setAllPorts(payload.all_ports);

          // Filter out dismissed ports and accumulate new ones
          const dismissed = dismissedRef.current;
          const visibleNew = payload.new_ports.filter(
            (p) => !dismissed.has(p.port)
          );

          if (visibleNew.length > 0) {
            setNewPorts((prev) => {
              // Merge: add new ports that aren't already in the list
              const existing = new Set(prev.map((p) => p.port));
              const added = visibleNew.filter((p) => !existing.has(p.port));
              return [...prev, ...added];
            });
          }

          // Remove closed ports from the notification list
          if (payload.closed_ports.length > 0) {
            const closedSet = new Set(payload.closed_ports.map((p) => p.port));
            setNewPorts((prev) => prev.filter((p) => !closedSet.has(p.port)));
          }
        });

        if (!mounted) {
          fn();
          return;
        }
        unlisten = fn;
      } catch (error) {
        console.error('[usePortDetection] Failed to setup listener:', error);
      }
    };

    setup();

    return () => {
      mounted = false;
      unlisten?.();
      clearInterval(pollTimer);
      setNewPorts([]);
      setAllPorts([]);
      setIsActive(false);
      setHasScanned(false);
    };
  }, [connectionId]);

  return { newPorts, allPorts, dismissPort, isActive, hasScanned };
}
