/**
 * Enhanced Topology Visualization Component
 *
 * Features:
 * - D3-force layout (避免节点重叠)
 * - Zoom & Pan (缩放平移)
 * - Double-click node menu (双击菜单)
 * - Enhanced animations (状态动画)
 */

import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import * as d3Zoom from 'd3-zoom';
import * as d3Selection from 'd3-selection';
import { ExternalLink, Terminal, FolderOpen } from 'lucide-react';
import { cn } from '../../lib/utils';
import {
  type TopologyNode,
  type ForceLayoutNode,
  forceLayoutCache,
} from '../../lib/topologyUtils';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useAppStore } from '../../store/appStore';

// ------------------------------------------------------------------
// Theme Constants
// ------------------------------------------------------------------

const THEME = {
  colors: {
    connected: '#22c55e',      // neon green
    connecting: '#eab308',     // neon yellow
    failed: '#ef4444',         // neon red
    disconnected: '#71717a',   // zinc-500
    pending: '#f59e0b',        // amber
  },
  node: {
    width: 140,
    height: 50,
  }
};

const getStatusColor = (status: string) => {
  switch (status) {
    case 'connected': return THEME.colors.connected;
    case 'connecting': return THEME.colors.connecting;
    case 'disconnected':
    case 'closed': return THEME.colors.disconnected;
    case 'failed': return THEME.colors.failed;
    default: return THEME.colors.pending;
  }
};

// ------------------------------------------------------------------
// Types
// ------------------------------------------------------------------

interface TopologyViewEnhancedProps {
  nodes: TopologyNode[];
  width?: number;
  height?: number;
}

interface NodeMenuState {
  isOpen: boolean;
  nodeId: string | null;
  x: number;
  y: number;
}

// ------------------------------------------------------------------
// Sub-components
// ------------------------------------------------------------------

/**
 * Connection Line with Gradient and Flow Animation
 */
const ConnectionLine: React.FC<{
  source: ForceLayoutNode;
  target: ForceLayoutNode;
  isActive: boolean;
}> = ({ source, target, isActive }) => {
  const gradientId = `grad-${source.id}-${target.id}`;
  const sourceColor = getStatusColor(source.status);
  const targetColor = getStatusColor(target.status);

  // Cubic Bezier curve
  const deltaY = target.y - source.y;
  const cp1 = { x: source.x, y: source.y + deltaY * 0.4 };
  const cp2 = { x: target.x, y: target.y - deltaY * 0.4 };
  const pathData = `M ${source.x} ${source.y + 25} C ${cp1.x} ${cp1.y}, ${cp2.x} ${cp2.y}, ${target.x} ${target.y - 25}`;

  return (
    <g className="transition-opacity duration-300">
      <defs>
        <linearGradient
          id={gradientId}
          gradientUnits="userSpaceOnUse"
          x1={source.x} y1={source.y}
          x2={target.x} y2={target.y}
        >
          <stop offset="0%" stopColor={sourceColor} />
          <stop offset="100%" stopColor={targetColor} />
        </linearGradient>
      </defs>

      {/* Glow effect for active lines */}
      {isActive && (
        <path
          d={pathData}
          fill="none"
          stroke={sourceColor}
          strokeWidth="6"
          strokeOpacity="0.15"
          strokeLinecap="round"
        />
      )}

      {/* Main Gradient Line */}
      <path
        d={pathData}
        fill="none"
        stroke={`url(#${gradientId})`}
        strokeWidth={isActive ? 2.5 : 1.5}
        strokeLinecap="round"
        strokeOpacity={isActive ? 1 : 0.4}
      />

      {/* Flow Animation Particle */}
      {isActive && (
        <circle r="3" fill="white" opacity="0.8">
          <animateMotion dur="2s" repeatCount="indefinite" path={pathData} />
        </circle>
      )}
    </g>
  );
};

/**
 * Node Card with Animations
 * 
 * Features:
 * - Connecting: pulsing glow
 * - Success: brief green flash when transitioning to connected
 * - Down: shake animation
 */
const NodeCard: React.FC<{
  node: ForceLayoutNode;
  isHovered: boolean;
  isDimmed: boolean;
  onMouseEnter: () => void;
  onMouseLeave: () => void;
  onDoubleClick: (e: React.MouseEvent) => void;
}> = ({ node, isHovered, isDimmed, onMouseEnter, onMouseLeave, onDoubleClick }) => {
  const prevStatusRef = useRef(node.status);
  const [showSuccessFlash, setShowSuccessFlash] = useState(false);

  const statusColor = getStatusColor(node.status);
  const halfWidth = THEME.node.width / 2;
  const halfHeight = THEME.node.height / 2;

  const isDown = node.status === 'disconnected' || node.status === 'failed';
  const isConnecting = node.status === 'connecting';
  const isConnected = node.status === 'connected';

  // Detect status transition to 'connected' -> trigger success flash
  useEffect(() => {
    const wasConnecting = prevStatusRef.current === 'connecting';
    const nowConnected = node.status === 'connected';

    if (wasConnecting && nowConnected) {
      setShowSuccessFlash(true);
      const timer = setTimeout(() => setShowSuccessFlash(false), 800);
      return () => clearTimeout(timer);
    }

    prevStatusRef.current = node.status;
  }, [node.status]);

  return (
    <g
      className="topo-node-enter"
      style={{
        transform: `scale(${isDimmed ? 0.9 : 1})`,
        opacity: isDimmed ? 0.3 : 1,
        transition: 'transform 0.4s cubic-bezier(0.34, 1.56, 0.64, 1), opacity 0.3s ease',
        transformOrigin: `${node.x}px ${node.y}px`,
      }}
    >
      <foreignObject
        x={node.x - halfWidth}
        y={node.y - halfHeight}
        width={THEME.node.width}
        height={THEME.node.height}
        style={{ overflow: 'visible' }}
      >
        <div
          className={cn(
            "w-full h-full rounded-lg transition-all duration-200 ease-out select-none cursor-pointer",
            // Glassmorphism Base
            "bg-theme-bg-panel/20 backdrop-blur-md border border-theme-border/50",
            // Hover State
            isHovered && "border-theme-accent/50 shadow-[0_0_20px_color-mix(in_srgb,_var(--theme-accent)_15%,_transparent)] scale-105",
            // LinkDown State
            isDown && "grayscale-[0.6] border-red-500/40",
            // Connecting pulse
            isConnecting && "topo-connecting-glow",
          )}
          style={isConnected ? {
            boxShadow: `0 0 15px ${statusColor}30`,
          } : undefined}
          onMouseEnter={onMouseEnter}
          onMouseLeave={onMouseLeave}
          onDoubleClick={onDoubleClick}
        >
          <div className="flex flex-col justify-center items-center h-full w-full relative">

            {/* Status Indicator */}
            <div className="flex items-center gap-2 mb-0.5">
              <div
                className={cn(
                  "w-2 h-2 rounded-full",
                  (isDown || isConnecting) && "topo-dot-pulse",
                )}
                style={{
                  backgroundColor: statusColor,
                  animationDuration: isDown ? '0.8s' : '1.2s',
                }}
              />

              {/* Node Name */}
              <span className="text-theme-text text-[11px] font-semibold tracking-wide truncate max-w-[100px]">
                {node.name}
              </span>
            </div>

            {/* Host */}
            <span className="text-[9px] font-mono text-theme-text-muted opacity-70 truncate max-w-[120px]">
              {node.host}
            </span>

            {/* Error shake animation */}
            {isDown && (
              <div className="absolute inset-0 rounded-lg border border-red-500/30 pointer-events-none topo-shake" />
            )}

            {/* Success flash animation */}
            {showSuccessFlash && (
              <div
                className="absolute inset-0 rounded-lg pointer-events-none topo-success-flash"
                style={{
                  background: `radial-gradient(circle, ${THEME.colors.connected}40 0%, transparent 70%)`,
                  boxShadow: `0 0 30px ${THEME.colors.connected}60`,
                }}
              />
            )}
          </div>
        </div>
      </foreignObject>
    </g>
  );
};

/**
 * Node Action Menu (appears on double-click)
 */
const NodeActionMenu: React.FC<{
  node: ForceLayoutNode | null;
  x: number;
  y: number;
  onClose: () => void;
  onNavigateToSession: () => void;
  onCreateTerminal: () => void;
  onOpenSftp: () => void;
  t: (key: string) => string;
}> = ({ node, x, y, onClose, onNavigateToSession, onCreateTerminal, onOpenSftp, t }) => {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [onClose]);

  if (!node) return null;

  const isConnected = node.status === 'connected';

  return (
    <div
      ref={menuRef}
      className="fixed z-50 bg-theme-bg-panel/95 backdrop-blur-lg border border-theme-border rounded-lg shadow-2xl overflow-hidden min-w-[180px] topo-menu-enter"
      style={{ left: x, top: y }}
    >
      {/* Header */}
      <div className="px-3 py-2 border-b border-theme-border/50 bg-theme-bg/50">
        <div className="text-xs font-semibold text-theme-text truncate">{node.name}</div>
        <div className="text-[10px] text-theme-text-muted font-mono">{node.host}</div>
      </div>

      {/* Actions */}
      <div className="py-1">
        <button
          onClick={onNavigateToSession}
          className="w-full px-3 py-2 flex items-center gap-2 text-sm text-theme-text-muted hover:text-theme-text hover:bg-theme-accent/10 transition-colors"
        >
          <ExternalLink className="w-4 h-4 text-theme-accent" />
          <span>{t('topology.menu.navigate_session')}</span>
        </button>

        {isConnected && (
          <>
            <button
              onClick={onCreateTerminal}
              className="w-full px-3 py-2 flex items-center gap-2 text-sm text-theme-text-muted hover:text-theme-text hover:bg-theme-accent/10 transition-colors"
            >
              <Terminal className="w-4 h-4 text-green-500" />
              <span>{t('topology.menu.new_terminal')}</span>
            </button>

            <button
              onClick={onOpenSftp}
              className="w-full px-3 py-2 flex items-center gap-2 text-sm text-theme-text-muted hover:text-theme-text hover:bg-theme-accent/10 transition-colors"
            >
              <FolderOpen className="w-4 h-4 text-yellow-500" />
              <span>{t('topology.menu.open_sftp')}</span>
            </button>
          </>
        )}
      </div>

      {/* Close hint */}
      <div className="px-3 py-1.5 border-t border-theme-border/50 bg-theme-bg/30">
        <div className="text-[10px] text-theme-text-muted text-center">{t('topology.menu.close_hint')}</div>
      </div>
    </div>
  );
};

// ------------------------------------------------------------------
// Main Component
// ------------------------------------------------------------------

export const TopologyViewEnhanced: React.FC<TopologyViewEnhancedProps> = ({
  nodes: treeNodes,
  width: containerWidth = 800,
  height: containerHeight = 600,
}) => {
  const { t } = useTranslation();
  const svgRef = useRef<SVGSVGElement>(null);
  const gRef = useRef<SVGGElement>(null);

  const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);
  const [menu, setMenu] = useState<NodeMenuState>({ isOpen: false, nodeId: null, x: 0, y: 0 });
  const [transform, setTransform] = useState({ x: 0, y: 0, k: 1 });

  // Stores
  const { selectNode } = useSessionTreeStore();
  const { createTab } = useAppStore();

  // Calculate force layout
  const { nodes } = forceLayoutCache.compute(treeNodes, {
    width: containerWidth,
    height: containerHeight,
    chargeStrength: -500,
    collisionRadius: 90,
    linkDistance: 140,
  });

  // Create node lookup map
  const nodeMap = new Map(nodes.map(n => [n.id, n]));

  // Setup D3 zoom
  useEffect(() => {
    if (!svgRef.current || !gRef.current) return;

    const svg = d3Selection.select(svgRef.current);
    const g = d3Selection.select(gRef.current);

    const zoom = d3Zoom.zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.3, 3])
      .on('zoom', (event) => {
        const { x, y, k } = event.transform;
        setTransform({ x, y, k });
        g.attr('transform', `translate(${x},${y}) scale(${k})`);
      });

    svg.call(zoom);

    // Initial center
    const initialX = containerWidth / 2 - (containerWidth * 0.5);
    const initialY = 50;
    svg.call(zoom.transform, d3Zoom.zoomIdentity.translate(initialX, initialY).scale(0.9));

    return () => {
      svg.on('.zoom', null);
    };
  }, [containerWidth, containerHeight]);

  // Handle double-click on node
  const handleNodeDoubleClick = useCallback((node: ForceLayoutNode, e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();

    // Calculate menu position (account for zoom transform)
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;

    const menuX = Math.min(e.clientX, window.innerWidth - 200);
    const menuY = Math.min(e.clientY, window.innerHeight - 250);

    setMenu({
      isOpen: true,
      nodeId: node.id,
      x: menuX,
      y: menuY,
    });
  }, []);

  // Menu actions
  const handleNavigateToSession = useCallback(() => {
    if (menu.nodeId) {
      selectNode(menu.nodeId);
      // Could also scroll to the node in sidebar
    }
    setMenu({ isOpen: false, nodeId: null, x: 0, y: 0 });
  }, [menu.nodeId, selectNode]);

  const handleCreateTerminal = useCallback(async () => {
    if (!menu.nodeId) return;
    const node = nodeMap.get(menu.nodeId);
    if (!node || node.status !== 'connected') return;

    try {
      const { createTerminalForNode } = useSessionTreeStore.getState();
      const terminalId = await createTerminalForNode(menu.nodeId);

      // Create terminal tab (sessionId is second argument)
      createTab('terminal', terminalId);
    } catch (e) {
      console.error('Failed to create terminal:', e);
    }

    setMenu({ isOpen: false, nodeId: null, x: 0, y: 0 });
  }, [menu.nodeId, nodeMap, createTab]);

  const handleOpenSftp = useCallback(async () => {
    const nodeId = menu.nodeId;
    if (!nodeId) return;
    const node = nodeMap.get(nodeId);
    if (!node || node.status !== 'connected') return;

    try {
      const { openSftpForNode } = useSessionTreeStore.getState();
      const sftpId = await openSftpForNode(nodeId);
      if (!sftpId) return;

      // Create SFTP tab (sessionId is second argument, nodeId in options)
      createTab('sftp', sftpId, { nodeId });
    } catch (e) {
      console.error('Failed to open SFTP:', e);
    }

    setMenu({ isOpen: false, nodeId: null, x: 0, y: 0 });
  }, [menu.nodeId, nodeMap, createTab]);

  const closeMenu = useCallback(() => {
    setMenu({ isOpen: false, nodeId: null, x: 0, y: 0 });
  }, []);

  // Empty state
  if (nodes.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-zinc-500">
        <div className="text-center">
          <div className="text-4xl mb-4 opacity-20">❄️</div>
          <p className="text-sm font-mono tracking-widest uppercase">{t('topology.view.no_matrix')}</p>
          <p className="text-xs mt-2 opacity-50">{t('topology.view.connect_to_populate')}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="w-full h-full overflow-hidden bg-theme-bg rounded-lg relative topo-content">
      {/* Zoom controls */}
      <div className="absolute top-4 right-4 z-10 flex gap-2">
        <div className="px-2 py-1 bg-theme-bg-panel/80 border border-theme-border rounded text-xs text-theme-text-muted font-mono shadow-sm">
          {Math.round(transform.k * 100)}%
        </div>
      </div>

      {/* Instructions */}
      <div className="absolute bottom-4 left-4 z-10 text-[10px] text-theme-text-muted font-mono opacity-60">
        {t('topology.controls.instructions')}
      </div>

      <svg
        ref={svgRef}
        width="100%"
        height="100%"
        viewBox={`0 0 ${containerWidth} ${containerHeight}`}
        className="block cursor-grab active:cursor-grabbing"
      >
        <defs>
          {/* Background Gradient */}
          <radialGradient id="cyber-bg-enhanced" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="var(--theme-bg-panel)" />
            <stop offset="100%" stopColor="var(--theme-bg)" />
          </radialGradient>

          {/* Grid Pattern */}
          <pattern id="cyber-grid-enhanced" x="0" y="0" width="40" height="40" patternUnits="userSpaceOnUse">
            <circle cx="1" cy="1" r="0.5" fill="var(--theme-text-muted)" opacity="0.1" />
          </pattern>
        </defs>

        {/* Background */}
        <rect className="topo-bg-fill" width="100%" height="100%" fill="url(#cyber-bg-enhanced)" />
        <rect className="topo-bg-fill" width="100%" height="100%" fill="url(#cyber-grid-enhanced)" />

        {/* Zoomable group */}
        <g ref={gRef}>
          {/* Connection Lines */}
          {nodes.map(node =>
            node.children.map(child => {
              const childNode = nodeMap.get(child.id);
              if (!childNode) return null;

              const isHoveredConnection = hoveredNodeId === node.id || hoveredNodeId === child.id;
              const isActive = node.status === 'connected' && childNode.status === 'connected';
              const shouldDim = hoveredNodeId !== null && !isHoveredConnection;

              return (
                <g
                  key={`${node.id}-${child.id}`}
                  style={{ opacity: shouldDim ? 0.15 : 1, transition: 'opacity 0.3s' }}
                >
                  <ConnectionLine
                    source={node}
                    target={childNode}
                    isActive={isActive}
                  />
                </g>
              );
            })
          )}

          {/* Nodes */}
          {nodes.map(node => {
            const isHovered = hoveredNodeId === node.id;
            const shouldDim = hoveredNodeId !== null && hoveredNodeId !== node.id;

            return (
              <NodeCard
                key={node.id}
                node={node}
                isHovered={isHovered}
                isDimmed={shouldDim}
                onMouseEnter={() => setHoveredNodeId(node.id)}
                onMouseLeave={() => setHoveredNodeId(null)}
                onDoubleClick={(e) => handleNodeDoubleClick(node, e)}
              />
            );
          })}
        </g>
      </svg>

      {/* Node Action Menu */}
      {menu.isOpen && (
        <NodeActionMenu
          node={menu.nodeId ? nodeMap.get(menu.nodeId) || null : null}
          x={menu.x}
          y={menu.y}
          onClose={closeMenu}
          onNavigateToSession={handleNavigateToSession}
          onCreateTerminal={handleCreateTerminal}
          onOpenSftp={handleOpenSftp}
          t={t}
        />
      )}
    </div>
  );
};

export default TopologyViewEnhanced;
