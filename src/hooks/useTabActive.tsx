/**
 * Tab Active Context
 * 
 * Provides a way for any component within a tab to know if its tab is currently active.
 * 
 * AppLayout wraps each tab's content in <TabActiveProvider value={isActive}>.
 * Components that register window-level event listeners (keydown, etc.) MUST check
 * this context before handling events to prevent inactive tabs from intercepting input.
 * 
 * Usage:
 *   const isTabActive = useIsTabActive();
 *   useEffect(() => {
 *     if (!isTabActive) return;
 *     const handler = (e: KeyboardEvent) => { ... };
 *     window.addEventListener('keydown', handler);
 *     return () => window.removeEventListener('keydown', handler);
 *   }, [isTabActive]);
 */

import { createContext, useContext } from 'react';

const TabActiveContext = createContext<boolean>(true);

export const TabActiveProvider = TabActiveContext.Provider;

/**
 * Returns whether the enclosing tab is currently the active (visible) tab.
 * Defaults to `true` when used outside a TabActiveProvider (e.g., in modals).
 */
export function useIsTabActive(): boolean {
  return useContext(TabActiveContext);
}
