/**
 * Topology Page Component
 *
 * Full page view for the Topology visualization
 * 
 * Enhanced with:
 * - D3-force layout (prevents node overlap)
 * - Zoom & Pan (navigate large topologies)
 * - Double-click menu (quick actions)
 * - State animations (connecting pulse, disconnect shake)
 */

import React, { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { TopologyViewEnhanced } from './TopologyViewEnhanced';
import { buildTopologyTreeCached } from '../../lib/topologyUtils';
import type { TopologyNode } from '../../lib/topologyUtils';
import { useTabBgActive } from '../../hooks/useTabBackground';

export const TopologyPage: React.FC = () => {
    const { t } = useTranslation();
    const bgActive = useTabBgActive('topology');
    const containerRef = useRef<HTMLDivElement>(null);
    const [tree, setTree] = useState<TopologyNode[]>([]);
    const [dimensions, setDimensions] = useState({ width: 800, height: 600 });
    const { rawNodes } = useSessionTreeStore();

    // Track container size
    useEffect(() => {
        const container = containerRef.current;
        if (!container) return;

        const resizeObserver = new ResizeObserver((entries) => {
            const entry = entries[0];
            if (entry) {
                setDimensions({
                    width: entry.contentRect.width,
                    height: entry.contentRect.height,
                });
            }
        });

        resizeObserver.observe(container);
        return () => resizeObserver.disconnect();
    }, []);

    useEffect(() => {
        // Build topology tree from connected nodes
        // Filter primarily for visualization relevance if needed, 
        // but showing all nodes might be useful for a "Matrix" view
        // The original dialog filtered for connected/connecting.
        // We will stick to that to avoid cluttering the matrix with offline nodes
        // unless the user wants to see everything.

        const connectedNodes = rawNodes.filter(
            node => node.state.status === 'connected' || node.state.status === 'connecting'
        );

        const topologyTree = buildTopologyTreeCached(connectedNodes);
        setTree(topologyTree);
    }, [rawNodes]);

    return (
        <div className={`h-full w-full overflow-hidden flex flex-col ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
            <div className="p-6 border-b border-theme-border bg-theme-bg-panel topo-header">
                <h1 className="text-2xl font-bold text-theme-text mb-2">{t('topology.page.title')}</h1>
                <p className="text-theme-text-muted text-sm">{t('topology.page.description')}</p>
            </div>
            <div ref={containerRef} className="flex-1 overflow-hidden relative">
                {tree.length > 0 ? (
                    <TopologyViewEnhanced
                        nodes={tree}
                        width={dimensions.width}
                        height={dimensions.height}
                    />
                ) : (
                    <div className="flex flex-col items-center justify-center h-full text-theme-text-muted">
                        <div className="text-lg">{t('topology.page.no_connections')}</div>
                        <p className="text-sm mt-2 opacity-70">{t('topology.page.connect_hint')}</p>
                    </div>
                )}
            </div>
        </div>
    );
};
