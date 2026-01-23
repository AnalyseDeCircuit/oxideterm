/**
 * Terminal Registry
 * 
 * A global registry for terminal buffer access functions.
 * This allows the AI chat to retrieve terminal context without complex event systems.
 * 
 * Key changes for Split Pane support:
 * - Key changed from sessionId to paneId
 * - Added activePaneId tracking for focus management
 * - Unified SSH and Local terminal registration
 */

type BufferGetter = () => string;

interface TerminalEntry {
  getter: BufferGetter;
  registeredAt: number;
  tabId: string;
  sessionId: string;                                // Original session ID for reference
  terminalType: 'terminal' | 'local_terminal';      // SSH or Local
}

// Registry now uses paneId as key (supports split panes)
const registry = new Map<string, TerminalEntry>();

// Track the currently active (focused) pane across the entire app
let activePaneId: string | null = null;

// Entries older than 5 minutes are considered stale (safety net)
const MAX_AGE_MS = 5 * 60 * 1000;

/**
 * Register a terminal's buffer getter function
 * @param paneId - The unique pane ID (for split panes) or sessionId (for single pane)
 * @param tabId - The tab ID associated with this terminal
 * @param sessionId - The terminal session ID
 * @param terminalType - Whether this is SSH or Local terminal
 * @param getter - Function that returns the terminal buffer content
 */
export function registerTerminalBuffer(
  paneId: string, 
  tabId: string, 
  sessionId: string,
  terminalType: 'terminal' | 'local_terminal',
  getter: BufferGetter
): void {
  registry.set(paneId, {
    getter,
    registeredAt: Date.now(),
    tabId,
    sessionId,
    terminalType,
  });
  
  // Auto-set as active if it's the first registration
  if (activePaneId === null) {
    activePaneId = paneId;
  }
}

/**
 * Unregister a terminal's buffer getter
 * @param paneId - The pane ID to unregister
 */
export function unregisterTerminalBuffer(paneId: string): void {
  registry.delete(paneId);
  
  // Clear activePaneId if it was the unregistered one
  if (activePaneId === paneId) {
    // Try to find another pane to activate
    const remaining = Array.from(registry.keys());
    activePaneId = remaining.length > 0 ? remaining[0] : null;
  }
}

/**
 * Set the currently active (focused) pane
 * @param paneId - The pane ID that received focus
 */
export function setActivePaneId(paneId: string | null): void {
  if (paneId === null || registry.has(paneId)) {
    activePaneId = paneId;
  } else {
    console.warn('[TerminalRegistry] setActivePaneId: paneId not found in registry:', paneId);
  }
}

/**
 * Get the currently active (focused) pane ID
 */
export function getActivePaneId(): string | null {
  return activePaneId;
}

/**
 * Get terminal buffer content by pane ID
 * @param paneId - The pane ID
 * @param expectedTabId - Optional: verify the entry belongs to this tab
 * @returns Buffer content or null if not found/invalid
 */
export function getTerminalBuffer(paneId: string, expectedTabId?: string): string | null {
  const entry = registry.get(paneId);
  if (!entry) return null;
  
  // Validate tab ID if provided (prevents cross-tab context leakage)
  if (expectedTabId && entry.tabId !== expectedTabId) {
    console.warn('[TerminalRegistry] Tab ID mismatch, skipping stale entry');
    return null;
  }
  
  // Check if entry is too old (safety net for edge cases)
  if (Date.now() - entry.registeredAt > MAX_AGE_MS) {
    console.warn('[TerminalRegistry] Entry expired, removing stale entry');
    registry.delete(paneId);
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
 * Get the active pane's terminal buffer content
 * Convenience method for AI context retrieval
 * @param expectedTabId - Optional: verify the entry belongs to this tab
 * @returns Buffer content or null if no active pane
 */
export function getActiveTerminalBuffer(expectedTabId?: string): string | null {
  if (!activePaneId) return null;
  return getTerminalBuffer(activePaneId, expectedTabId);
}

/**
 * Get entry metadata for the active pane (useful for AI to know terminal type)
 */
export function getActivePaneMetadata(): { sessionId: string; terminalType: 'terminal' | 'local_terminal'; tabId: string } | null {
  if (!activePaneId) return null;
  const entry = registry.get(activePaneId);
  if (!entry) return null;
  return {
    sessionId: entry.sessionId,
    terminalType: entry.terminalType,
    tabId: entry.tabId,
  };
}

/**
 * Check if a pane is registered
 */
export function hasTerminal(paneId: string): boolean {
  return registry.has(paneId);
}

/**
 * Find pane ID by session ID (useful for backward compatibility)
 * @param sessionId - The session ID to look up
 * @returns The pane ID or null if not found
 */
export function findPaneBySessionId(sessionId: string): string | null {
  for (const [paneId, entry] of registry) {
    if (entry.sessionId === sessionId) {
      return paneId;
    }
  }
  return null;
}

/**
 * Get all pane IDs for a given tab
 * @param tabId - The tab ID
 * @returns Array of pane IDs
 */
export function getPanesForTab(tabId: string): string[] {
  const panes: string[] = [];
  for (const [paneId, entry] of registry) {
    if (entry.tabId === tabId) {
      panes.push(paneId);
    }
  }
  return panes;
}

/**
 * Refresh the timestamp for a terminal entry (call on terminal activity)
 */
export function touchTerminalEntry(paneId: string): void {
  const entry = registry.get(paneId);
  if (entry) {
    entry.registeredAt = Date.now();
  }
}

/**
 * Clear all entries (useful for testing or app reset)
 */
export function clearRegistry(): void {
  registry.clear();
  activePaneId = null;
}

/**
 * Debug: Get registry stats
 */
export function getRegistryStats(): { count: number; activePaneId: string | null; paneIds: string[] } {
  return {
    count: registry.size,
    activePaneId,
    paneIds: Array.from(registry.keys()),
  };
}

