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
 * - Added selection getter for AI context injection
 */

type BufferGetter = () => string;
type SelectionGetter = () => string;
type TerminalWriter = (data: string) => void;

interface TerminalEntry {
  getter: BufferGetter;
  selectionGetter?: SelectionGetter;                // Optional: get current selection
  writer?: TerminalWriter;                          // Optional: write data to terminal's transport
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
 * @param selectionGetter - Optional: Function that returns the current selection
 * @param writer - Optional: Function that writes data to the terminal's transport (WebSocket/PTY)
 */
export function registerTerminalBuffer(
  paneId: string, 
  tabId: string, 
  sessionId: string,
  terminalType: 'terminal' | 'local_terminal',
  getter: BufferGetter,
  selectionGetter?: SelectionGetter,
  writer?: TerminalWriter,
): void {
  registry.set(paneId, {
    getter,
    selectionGetter,
    writer,
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
 * Result of gathering all pane buffers
 */
export interface GatheredPaneContext {
  paneId: string;
  sessionId: string;
  terminalType: 'terminal' | 'local_terminal';
  buffer: string;
  isActive: boolean;
}

/**
 * Gather buffers from ALL panes in a tab (for AI "cross-pane vision")
 * This allows AI to analyze content across multiple split panes simultaneously.
 * 
 * @param tabId - The tab ID to gather context from
 * @param maxCharsPerPane - Optional: limit characters per pane
 * @returns Array of pane contexts with their buffers
 */
export function gatherAllPaneContexts(tabId: string, maxCharsPerPane?: number): GatheredPaneContext[] {
  const results: GatheredPaneContext[] = [];
  
  for (const [paneId, entry] of registry) {
    if (entry.tabId !== tabId) continue;
    
    try {
      let buffer = entry.getter();
      if (buffer && maxCharsPerPane && buffer.length > maxCharsPerPane) {
        // Keep the most recent content (tail)
        buffer = buffer.slice(-maxCharsPerPane);
      }
      
      if (buffer) {
        results.push({
          paneId,
          sessionId: entry.sessionId,
          terminalType: entry.terminalType,
          buffer,
          isActive: paneId === activePaneId,
        });
      }
    } catch (e) {
      console.error(`[TerminalRegistry] Failed to get buffer for pane ${paneId}:`, e);
    }
  }
  
  return results;
}

/**
 * Get combined context from all panes as a formatted string
 * Useful for directly passing to AI prompt
 * 
 * @param tabId - The tab ID
 * @param maxCharsPerPane - Optional: limit per pane
 * @param separator - Separator between panes (default: visual divider)
 * @returns Formatted string with all pane contents
 */
export function getCombinedPaneContext(
  tabId: string, 
  maxCharsPerPane?: number,
  separator: string = '\n\n--- Pane {index} ({type}) ---\n\n'
): string {
  const contexts = gatherAllPaneContexts(tabId, maxCharsPerPane);
  
  if (contexts.length === 0) {
    return '';
  }
  
  if (contexts.length === 1) {
    return contexts[0].buffer;
  }
  
  // Sort: active pane first, then by paneId for consistency
  contexts.sort((a, b) => {
    if (a.isActive !== b.isActive) return a.isActive ? -1 : 1;
    return a.paneId.localeCompare(b.paneId);
  });
  
  return contexts.map((ctx, index) => {
    const label = ctx.isActive ? 'active' : `pane ${index + 1}`;
    const typeName = ctx.terminalType === 'terminal' ? 'SSH' : 'Local';
    const header = separator
      .replace('{index}', String(index + 1))
      .replace('{type}', `${typeName}, ${label}`);
    return header + ctx.buffer;
  }).join('');
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

// ═══════════════════════════════════════════════════════════════════════════
// Selection Support (for AI Sidebar Context Injection)
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Get the current selection from a specific pane
 * @param paneId - The pane ID
 * @returns Selection text or null if not available
 */
export function getTerminalSelection(paneId: string): string | null {
  const entry = registry.get(paneId);
  if (!entry?.selectionGetter) return null;
  
  try {
    return entry.selectionGetter() || null;
  } catch (e) {
    console.error('[TerminalRegistry] Failed to get selection:', e);
    return null;
  }
}

/**
 * Get the active pane's current selection
 * Convenience method for AI sidebar context retrieval
 * @returns Selection text or null if no active pane or no selection
 */
export function getActiveTerminalSelection(): string | null {
  if (!activePaneId) return null;
  return getTerminalSelection(activePaneId);
}

/**
 * Update the selection getter for an existing entry
 * (Useful when the terminal instance is created after initial registration)
 * @param paneId - The pane ID
 * @param selectionGetter - Function that returns the current selection
 */
export function updateSelectionGetter(paneId: string, selectionGetter: SelectionGetter): void {
  const entry = registry.get(paneId);
  if (entry) {
    entry.selectionGetter = selectionGetter;
  }
}

/**
 * Write data to a terminal's transport layer (WebSocket/PTY).
 * Used by the plugin system's writeToTerminal API.
 * @param paneId - The pane ID
 * @param data - Text data to send to the terminal
 * @returns true if write succeeded, false if no writer registered
 */
export function writeToTerminal(paneId: string, data: string): boolean {
  const entry = registry.get(paneId);
  if (!entry?.writer) return false;

  try {
    entry.writer(data);
    return true;
  } catch (e) {
    console.error('[TerminalRegistry] Failed to write to terminal:', e);
    return false;
  }
}

