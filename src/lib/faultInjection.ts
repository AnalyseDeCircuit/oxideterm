/**
 * faultInjection — Dev-only fault injection for testing Virtual Session Proxy
 *
 * Provides configurable failure points to test the proxy's resilience.
 * ALL functions are no-ops in production (tree-shaken by Vite).
 *
 * Usage (browser console):
 *   window.__faultInjection.enable('linkDown', { delayMs: 2000 })
 *   window.__faultInjection.enable('sessionRotation')
 *   window.__faultInjection.enable('refreshDelay', { delayMs: 5000 })
 *   window.__faultInjection.enable('tabStorm', { count: 20 })
 *   window.__faultInjection.disable('linkDown')
 *   window.__faultInjection.status()
 *
 * @module lib/faultInjection
 */

export type FaultType =
  | 'linkDown'          // Simulate link_down event
  | 'sessionRotation'   // Simulate sessionId rotation (new PTY)
  | 'refreshDelay'      // Delay refreshConnections response
  | 'tabStorm';         // Rapid tab switch/create storm

interface FaultConfig {
  enabled: boolean;
  delayMs?: number;
  count?: number;
  probability?: number; // 0-1, default 1.0
}

const _faults = new Map<FaultType, FaultConfig>();

/**
 * Enable a fault injection point.
 */
export function enableFault(type: FaultType, opts?: Partial<FaultConfig>): void {
  if (!import.meta.env.DEV) return;
  _faults.set(type, {
    enabled: true,
    delayMs: opts?.delayMs ?? 1000,
    count: opts?.count ?? 10,
    probability: opts?.probability ?? 1.0,
  });
  console.info(`[FaultInjection] Enabled: ${type}`, _faults.get(type));
}

/**
 * Disable a fault injection point.
 */
export function disableFault(type: FaultType): void {
  if (!import.meta.env.DEV) return;
  _faults.delete(type);
  console.info(`[FaultInjection] Disabled: ${type}`);
}

/**
 * Check if a fault is active and should fire (respects probability).
 */
export function shouldFault(type: FaultType): boolean {
  if (!import.meta.env.DEV) return false;
  const conf = _faults.get(type);
  if (!conf?.enabled) return false;
  return Math.random() < (conf.probability ?? 1.0);
}

/**
 * Get the delay for a fault, or 0 if not active.
 */
export function getFaultDelay(type: FaultType): number {
  if (!import.meta.env.DEV) return 0;
  const conf = _faults.get(type);
  if (!conf?.enabled) return 0;
  return conf.delayMs ?? 0;
}

/**
 * Get current status of all faults.
 */
export function faultStatus(): Record<string, FaultConfig> {
  const result: Record<string, FaultConfig> = {};
  _faults.forEach((v, k) => { result[k] = { ...v }; });
  return result;
}

/**
 * Inject a delay if the fault is active. No-op in production.
 */
export async function maybeDelay(type: FaultType): Promise<void> {
  if (!import.meta.env.DEV) return;
  if (!shouldFault(type)) return;
  const ms = getFaultDelay(type);
  if (ms > 0) {
    console.warn(`[FaultInjection] Injecting ${ms}ms delay for ${type}`);
    await new Promise((r) => setTimeout(r, ms));
  }
}

// ============================================================================
// Expose to browser console (dev only)
// ============================================================================

if (import.meta.env.DEV) {
  const api = {
    enable: enableFault,
    disable: disableFault,
    status: () => {
      const s = faultStatus();
      console.table(s);
      return s;
    },
  };

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (window as any).__faultInjection = api;
}
