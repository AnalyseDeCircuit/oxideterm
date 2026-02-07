/**
 * Plugin Terminal Hooks
 *
 * Provides pipeline functions for running plugin input interceptors
 * and output processors. Used by TerminalView.tsx.
 *
 * Design:
 * - Input pipeline is synchronous, fail-open (exception → pass original data)
 * - Output pipeline is synchronous, fail-open
 * - Any interceptor returning null suppresses input entirely
 * - Circuit breaker: plugins exceeding error limits are auto-disabled
 * - Time budget: per-hook calls exceeding HOOK_BUDGET_MS emit a warning
 *   and count toward the circuit breaker
 */

import { usePluginStore } from '../../store/pluginStore';
import { trackPluginError } from './pluginLoader';

/** Maximum time (ms) a single hook handler should take before warning */
const HOOK_BUDGET_MS = 5;

/**
 * Run the input interceptor pipeline.
 *
 * @param data - Raw input string from term.onData
 * @param sessionId - Terminal session ID
 * @returns Modified string, or null if a plugin suppresses input
 */
export function runInputPipeline(data: string, sessionId: string): string | null {
  const interceptors = usePluginStore.getState().inputInterceptors;
  if (interceptors.length === 0) return data;

  let current: string | null = data;

  for (const entry of interceptors) {
    if (current === null) return null;

    try {
      const t0 = performance.now();
      current = entry.handler(current, { sessionId });
      const elapsed = performance.now() - t0;

      if (elapsed > HOOK_BUDGET_MS) {
        console.warn(
          `[PluginTerminalHooks] Input interceptor (plugin: ${entry.pluginId}) took ${elapsed.toFixed(1)}ms (budget: ${HOOK_BUDGET_MS}ms)`,
        );
        // Slow hooks count toward circuit breaker
        if (trackPluginError(entry.pluginId)) {
          import('./pluginLoader').then(({ unloadPlugin }) => unloadPlugin(entry.pluginId));
        }
      }
    } catch (err) {
      console.error(`[PluginTerminalHooks] Input interceptor error (plugin: ${entry.pluginId}):`, err);

      // Circuit breaker check
      if (trackPluginError(entry.pluginId)) {
        // Auto-disable will be handled by the loader
        import('./pluginLoader').then(({ unloadPlugin }) => unloadPlugin(entry.pluginId));
      }

      // Fail-open: continue with the current (unmodified) data
    }
  }

  return current;
}

/**
 * Run the output processor pipeline.
 *
 * @param data - Raw output bytes (copy of MSG_TYPE_DATA payload)
 * @param sessionId - Terminal session ID
 * @returns Modified Uint8Array
 */
export function runOutputPipeline(data: Uint8Array, sessionId: string): Uint8Array {
  const processors = usePluginStore.getState().outputProcessors;
  if (processors.length === 0) return data;

  let current = data;

  for (const entry of processors) {
    try {
      const t0 = performance.now();
      current = entry.handler(current, { sessionId });
      const elapsed = performance.now() - t0;

      if (elapsed > HOOK_BUDGET_MS) {
        console.warn(
          `[PluginTerminalHooks] Output processor (plugin: ${entry.pluginId}) took ${elapsed.toFixed(1)}ms (budget: ${HOOK_BUDGET_MS}ms)`,
        );
        if (trackPluginError(entry.pluginId)) {
          import('./pluginLoader').then(({ unloadPlugin }) => unloadPlugin(entry.pluginId));
        }
      }
    } catch (err) {
      console.error(`[PluginTerminalHooks] Output processor error (plugin: ${entry.pluginId}):`, err);

      // Circuit breaker check
      if (trackPluginError(entry.pluginId)) {
        import('./pluginLoader').then(({ unloadPlugin }) => unloadPlugin(entry.pluginId));
      }

      // Fail-open: continue with previous data unchanged
    }
  }

  return current;
}

/**
 * Match a keyboard event against registered plugin shortcuts.
 *
 * @returns The handler function if matched, undefined otherwise
 */
export function matchPluginShortcut(event: KeyboardEvent): (() => void) | undefined {
  const shortcuts = usePluginStore.getState().shortcuts;
  if (shortcuts.size === 0) return undefined;

  // Build normalized key from event
  const parts: string[] = [];
  if (event.ctrlKey || event.metaKey) parts.push('ctrl');
  if (event.shiftKey) parts.push('shift');
  if (event.altKey) parts.push('alt');
  parts.push(event.key.toLowerCase());
  const normalizedKey = parts.sort().join('+');

  return shortcuts.get(normalizedKey)?.handler;
}
