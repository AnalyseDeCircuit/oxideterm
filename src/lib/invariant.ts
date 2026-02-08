/**
 * invariant — Runtime assertion for Virtual Session Proxy invariants
 *
 * In DEV mode, violations are logged with full diagnostic context
 * and optionally throw. In production builds, this function is
 * tree-shaken away by Vite's dead-code elimination.
 *
 * @module lib/invariant
 */

type InvariantContext = Record<string, unknown>;

/**
 * Assert a condition that must always hold.
 * Logs a structured error and throws in dev mode.
 *
 * ```ts
 * invariant(gen >= prev, 'generation must be monotonic', { nodeId, gen, prev });
 * ```
 */
export function invariant(
  condition: boolean,
  message: string,
  context?: InvariantContext,
): asserts condition {
  if (condition) return;

  if (import.meta.env.DEV) {
    console.error(
      `[INVARIANT VIOLATION] ${message}`,
      context ?? {},
    );
    // Throw in dev to surface violations immediately
    throw new Error(`Invariant violation: ${message}`);
  }
}

/**
 * Soft invariant — logs a warning in dev but never throws.
 * Use for conditions that degrade gracefully at runtime.
 */
export function softInvariant(
  condition: boolean,
  message: string,
  context?: InvariantContext,
): void {
  if (condition) return;

  if (import.meta.env.DEV) {
    console.warn(
      `[SOFT INVARIANT] ${message}`,
      context ?? {},
    );
  }
}
