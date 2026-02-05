import React, { useCallback, useRef, useEffect } from 'react';
import { Panel, Group, Separator } from 'react-resizable-panels';
import { PaneNode, PaneLeaf, PaneGroup as PaneGroupType } from '../../types';
import { TerminalPane } from './TerminalPane';
import { cn } from '../../lib/utils';

/** Debounce delay for resize events (ms) */
const RESIZE_DEBOUNCE_MS = 150;

interface SplitTerminalContainerProps {
  tabId: string;
  rootPane: PaneNode;
  activePaneId?: string;
  onPaneFocus: (paneId: string) => void;
  onPaneClose?: (paneId: string) => void;
  onPaneSizesChange?: (groupId: string, sizes: number[]) => void;
}

/**
 * SplitTerminalContainer - Recursively renders the pane layout tree
 * 
 * Uses react-resizable-panels for the split functionality.
 * Handles both single pane (PaneLeaf) and split layouts (PaneGroup).
 */
export const SplitTerminalContainer: React.FC<SplitTerminalContainerProps> = ({
  tabId,
  rootPane,
  activePaneId,
  onPaneFocus,
  onPaneClose,
  onPaneSizesChange,
}) => {
  return (
    <div className="h-full w-full">
      <PaneRenderer
        node={rootPane}
        tabId={tabId}
        activePaneId={activePaneId}
        onPaneFocus={onPaneFocus}
        onPaneClose={onPaneClose}
        onPaneSizesChange={onPaneSizesChange}
        isRoot
      />
    </div>
  );
};

interface PaneRendererProps {
  node: PaneNode;
  tabId: string;
  activePaneId?: string;
  onPaneFocus: (paneId: string) => void;
  onPaneClose?: (paneId: string) => void;
  onPaneSizesChange?: (groupId: string, sizes: number[]) => void;
  isRoot?: boolean;
}

/**
 * PaneRenderer - Recursive component that renders a single node in the pane tree
 */
const PaneRenderer: React.FC<PaneRendererProps> = ({
  node,
  tabId,
  activePaneId,
  onPaneFocus,
  onPaneClose,
  onPaneSizesChange,
  isRoot = false,
}) => {
  // Type guard for leaf nodes
  if (node.type === 'leaf') {
    return (
      <TerminalPane
        pane={node as PaneLeaf}
        tabId={tabId}
        isActive={node.id === activePaneId}
        onFocus={onPaneFocus}
        onClose={onPaneClose}
      />
    );
  }

  // Group node - render Group with children
  const group = node as PaneGroupType;
  const orientation = group.direction === 'horizontal' ? 'horizontal' : 'vertical';

  // Build defaultLayout from sizes or equal distribution
  const defaultLayout: { [panelId: string]: number } = {};
  group.children.forEach((child, index) => {
    defaultLayout[child.id] = group.sizes?.[index] ?? 100 / group.children.length;
  });

  // Debounced layout change handler to avoid excessive state updates during resize
  const debounceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Cleanup debounce timer on unmount to prevent memory leak
  useEffect(() => {
    return () => {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }
    };
  }, []);
  
  const handleLayoutChanged = useCallback(
    (layout: { [panelId: string]: number }) => {
      // Clear any pending debounce
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
      
      // Debounce the state update
      debounceTimerRef.current = setTimeout(() => {
        const sizes = group.children.map(child => layout[child.id] ?? 0);
        onPaneSizesChange?.(group.id, sizes);
        debounceTimerRef.current = null;
      }, RESIZE_DEBOUNCE_MS);
    },
    [group.id, group.children, onPaneSizesChange]
  );

  return (
    <Group
      orientation={orientation}
      defaultLayout={defaultLayout}
      onLayoutChanged={handleLayoutChanged}
      className={cn('h-full w-full', isRoot && 'p-1')}
    >
      {group.children.map((child, index) => (
        <React.Fragment key={child.id}>
          <Panel
            id={child.id}
            defaultSize={defaultLayout[child.id]}
            minSize={10}
            className="relative"
          >
            <PaneRenderer
              node={child}
              tabId={tabId}
              activePaneId={activePaneId}
              onPaneFocus={onPaneFocus}
              onPaneClose={onPaneClose}
              onPaneSizesChange={onPaneSizesChange}
            />
          </Panel>

          {/* Resize handle between panels */}
          {index < group.children.length - 1 && (
            <Separator
              className={cn(
                'group relative',
                orientation === 'horizontal'
                  ? 'w-1 cursor-col-resize'
                  : 'h-1 cursor-row-resize'
              )}
            >
              {/* Visual indicator */}
              <div
                className={cn(
                  'absolute transition-all duration-150',
                  orientation === 'horizontal'
                    ? 'inset-y-0 left-0 right-0 bg-zinc-700 group-hover:bg-[#FF6B35] group-active:bg-[#FF6B35]'
                    : 'inset-x-0 top-0 bottom-0 bg-zinc-700 group-hover:bg-[#FF6B35] group-active:bg-[#FF6B35]'
                )}
              />
            </Separator>
          )}
        </React.Fragment>
      ))}
    </Group>
  );
};

export default SplitTerminalContainer;
