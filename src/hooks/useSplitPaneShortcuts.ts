/**
 * Split Pane Keyboard Shortcuts Hook
 * 
 * Handles keyboard shortcuts for terminal split pane operations:
 * - Cmd+Shift+E (Mac) / Ctrl+Shift+E (Win/Linux): Split horizontal
 * - Cmd+Shift+D (Mac) / Ctrl+Shift+D (Win/Linux): Split vertical
 * - Cmd+Option+Arrow (Mac) / Ctrl+Alt+Arrow (Win/Linux): Navigate between panes
 * - Cmd+Shift+W (Mac) / Ctrl+Shift+W (Win/Linux): Close current pane
 */

import { useEffect, useCallback, useRef } from 'react';
import { useAppStore } from '../store/appStore';
import { useLocalTerminalStore } from '../store/localTerminalStore';
import { platform } from '../lib/platform';
import { SplitDirection, MAX_PANES_PER_TAB, PaneNode } from '../types';

interface UseSplitPaneShortcutsOptions {
  /** Whether shortcuts are enabled (typically when a terminal tab is active) */
  enabled: boolean;
}

/**
 * Get all leaf pane IDs in order (left-to-right, top-to-bottom)
 */
function getAllLeafPaneIds(node: PaneNode): string[] {
  if (node.type === 'leaf') {
    return [node.id];
  }
  return node.children.flatMap(child => getAllLeafPaneIds(child));
}

export function useSplitPaneShortcuts({ enabled }: UseSplitPaneShortcutsOptions) {
  const tabs = useAppStore((s) => s.tabs);
  const activeTabId = useAppStore((s) => s.activeTabId);
  const splitPane = useAppStore((s) => s.splitPane);
  const closePane = useAppStore((s) => s.closePane);
  const setActivePaneId = useAppStore((s) => s.setActivePaneId);
  const getPaneCount = useAppStore((s) => s.getPaneCount);
  
  const createTerminal = useLocalTerminalStore((s) => s.createTerminal);
  
  // Use ref to avoid stale closures
  const stateRef = useRef({ tabs, activeTabId });
  stateRef.current = { tabs, activeTabId };

  const handleSplit = useCallback(async (direction: SplitDirection) => {
    const { tabs, activeTabId } = stateRef.current;
    if (!activeTabId) return;
    
    const currentTab = tabs.find(t => t.id === activeTabId);
    if (!currentTab) return;
    
    // Only allow split for terminal tabs
    if (currentTab.type !== 'terminal' && currentTab.type !== 'local_terminal') return;
    
    // Check pane limit
    const paneCount = getPaneCount(activeTabId);
    if (paneCount >= MAX_PANES_PER_TAB) {
      console.log(`[SplitPane] Max panes reached (${MAX_PANES_PER_TAB})`);
      return;
    }

    try {
      if (currentTab.type === 'local_terminal') {
        // Create new local terminal session
        const newSession = await createTerminal();
        splitPane(activeTabId, direction, newSession.id, 'local_terminal');
      } else if (currentTab.type === 'terminal') {
        // SSH terminal split - TODO: implement session cloning
        console.log('[SplitPane] SSH terminal split not yet implemented');
        // Would need: const clonedSession = await cloneSession(currentTab.sessionId);
        // splitPane(activeTabId, direction, clonedSession.id, 'terminal');
      }
    } catch (err) {
      console.error('[SplitPane] Failed to split pane:', err);
    }
  }, [splitPane, createTerminal, getPaneCount]);

  const handleClosePane = useCallback(() => {
    const { tabs, activeTabId } = stateRef.current;
    if (!activeTabId) return;
    
    const currentTab = tabs.find(t => t.id === activeTabId);
    if (!currentTab?.activePaneId || !currentTab.rootPane) return;
    
    // Don't close the last pane
    const paneCount = getPaneCount(activeTabId);
    if (paneCount <= 1) {
      console.log('[SplitPane] Cannot close last pane');
      return;
    }
    
    closePane(activeTabId, currentTab.activePaneId);
  }, [closePane, getPaneCount]);

  const handleNavigate = useCallback((direction: 'left' | 'right' | 'up' | 'down') => {
    const { tabs, activeTabId } = stateRef.current;
    if (!activeTabId) return;
    
    const currentTab = tabs.find(t => t.id === activeTabId);
    if (!currentTab?.rootPane || !currentTab.activePaneId) return;
    
    const allPaneIds = getAllLeafPaneIds(currentTab.rootPane);
    if (allPaneIds.length <= 1) return;
    
    const currentIndex = allPaneIds.indexOf(currentTab.activePaneId);
    if (currentIndex === -1) return;
    
    let newIndex: number;
    
    // Simple navigation: left/up = previous, right/down = next
    // TODO: Could implement 2D spatial navigation based on actual pane positions
    if (direction === 'left' || direction === 'up') {
      newIndex = currentIndex > 0 ? currentIndex - 1 : allPaneIds.length - 1;
    } else {
      newIndex = currentIndex < allPaneIds.length - 1 ? currentIndex + 1 : 0;
    }
    
    const newPaneId = allPaneIds[newIndex];
    setActivePaneId(activeTabId, newPaneId);
    
    // Focus the terminal in the new pane
    // The focus will happen automatically via the TerminalPane component
  }, [setActivePaneId]);

  useEffect(() => {
    if (!enabled) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Skip if window lost OS-level focus
      if (!document.hasFocus()) return;

      const isMac = platform.isMac;
      const cmdOrCtrl = isMac ? e.metaKey : e.ctrlKey;
      const altOrOption = e.altKey;

      // Split shortcuts: Cmd/Ctrl + Shift + E/D
      if (cmdOrCtrl && e.shiftKey && !altOrOption) {
        const key = e.key.toLowerCase();
        
        if (key === 'e') {
          e.preventDefault();
          e.stopPropagation();
          handleSplit('horizontal');
          return;
        }
        
        if (key === 'd') {
          e.preventDefault();
          e.stopPropagation();
          handleSplit('vertical');
          return;
        }
        
        // Close pane: Cmd/Ctrl + Shift + W
        if (key === 'w') {
          e.preventDefault();
          e.stopPropagation();
          handleClosePane();
          return;
        }
      }

      // Navigation shortcuts: Cmd/Ctrl + Option/Alt + Arrow
      if (cmdOrCtrl && altOrOption && !e.shiftKey) {
        const key = e.key.toLowerCase();
        
        if (key === 'arrowleft') {
          e.preventDefault();
          e.stopPropagation();
          handleNavigate('left');
          return;
        }
        
        if (key === 'arrowright') {
          e.preventDefault();
          e.stopPropagation();
          handleNavigate('right');
          return;
        }
        
        if (key === 'arrowup') {
          e.preventDefault();
          e.stopPropagation();
          handleNavigate('up');
          return;
        }
        
        if (key === 'arrowdown') {
          e.preventDefault();
          e.stopPropagation();
          handleNavigate('down');
          return;
        }
      }
    };

    // Use capture phase to intercept before terminal
    window.addEventListener('keydown', handleKeyDown, { capture: true });
    return () => window.removeEventListener('keydown', handleKeyDown, { capture: true });
  }, [enabled, handleSplit, handleClosePane, handleNavigate]);
}
