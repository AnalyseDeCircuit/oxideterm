/**
 * Terminal Registry
 * 
 * A global registry for terminal buffer access functions.
 * This allows the AI chat to retrieve terminal context without complex event systems.
 */

type BufferGetter = () => string;

interface TerminalEntry {
  getter: BufferGetter;
  registeredAt: number;
  tabId: string;
}

const registry = new Map<string, TerminalEntry>();

// Entries older than 5 minutes are considered stale (safety net)
const MAX_AGE_MS = 5 * 60 * 1000;

/**
 * Register a terminal's buffer getter function
 * @param sessionId - The terminal session ID
 * @param tabId - The tab ID associated with this terminal
 * @param getter - Function that returns the terminal buffer content
 */
export function registerTerminalBuffer(sessionId: string, tabId: string, getter: BufferGetter): void {
  registry.set(sessionId, {
    getter,
    registeredAt: Date.now(),
    tabId,
  });
}

/**
 * Unregister a terminal's buffer getter
 */
export function unregisterTerminalBuffer(sessionId: string): void {
  registry.delete(sessionId);
}

/**
 * Get terminal buffer content by session ID
 * @param sessionId - The terminal session ID
 * @param expectedTabId - Optional: verify the entry belongs to this tab
 * @returns Buffer content or null if not found/invalid
 */
export function getTerminalBuffer(sessionId: string, expectedTabId?: string): string | null {
  const entry = registry.get(sessionId);
  if (!entry) return null;
  
  // Validate tab ID if provided (prevents cross-tab context leakage)
  if (expectedTabId && entry.tabId !== expectedTabId) {
    console.warn('[TerminalRegistry] Tab ID mismatch, skipping stale entry');
    return null;
  }
  
  // Check if entry is too old (safety net for edge cases)
  if (Date.now() - entry.registeredAt > MAX_AGE_MS) {
    console.warn('[TerminalRegistry] Entry expired, removing stale entry');
    registry.delete(sessionId);
    return null;
  }
  
  try {
    return entry.getter();
  } catch (e) {
    console.error('[TerminalRegistry] Failed to get terminal buffer:', e);
    return null;
  }
}

/**
 * Check if a terminal is registered
 */
export function hasTerminal(sessionId: string): boolean {
  return registry.has(sessionId);
}

/**
 * Refresh the timestamp for a terminal entry (call on terminal activity)
 */
export function touchTerminalEntry(sessionId: string): void {
  const entry = registry.get(sessionId);
  if (entry) {
    entry.registeredAt = Date.now();
  }
}
