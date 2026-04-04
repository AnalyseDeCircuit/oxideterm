// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * AutoRouteModal - Auto-route target selector
 * 
 * Topology is auto-generated from saved connections:
 * - Nodes: Each saved connection becomes a node
 * - Edges: Inferred from proxy_chain configuration
 * 
 * Select a target node, and the system will automatically compute
 * the optimal jump host path using Dijkstra's algorithm.
 */

import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '../../store/appStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { RadioGroup, RadioGroupItem } from '../ui/radio-group';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogFooter
} from '../ui/dialog';
import { api } from '../../lib/api';
import type { TopologyNodeInfo } from '../../types';
import { 
  Network, 
  Server, 
  RefreshCw,
  AlertCircle,
  Info
} from 'lucide-react';

export const AutoRouteModal = () => {
  const { t } = useTranslation();
  const { modals, toggleModal } = useAppStore();
  const { connectNode, expandAutoRoute } = useSessionTreeStore();
  
  const [loading, setLoading] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [nodes, setNodes] = useState<TopologyNodeInfo[]>([]);
  const [selectedNode, setSelectedNode] = useState<TopologyNodeInfo | null>(null);
  const [displayName, setDisplayName] = useState('');
  const [error, setError] = useState<string | null>(null);

  // Load topology when modal opens
  useEffect(() => {
    if (modals.autoRoute) {
      loadTopology();
    }
  }, [modals.autoRoute]);

  const loadTopology = async () => {
    setLoading(true);
    setError(null);
    try {
      const topologyNodes = await api.getTopologyNodes();
      // Filter out local node
      setNodes(topologyNodes.filter(n => !n.isLocal));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleSelectNode = (node: TopologyNodeInfo) => {
    setSelectedNode(node);
    setDisplayName(node.displayName || `${node.username}@${node.host}`);
  };

  const handleConnect = async () => {
    if (!selectedNode) return;
    
    setConnecting(true);
    setError(null);
    try {
      // 1. Expand auto-route (store method will refresh tree)
      const result = await expandAutoRoute({
        targetId: selectedNode.id,
        displayName: displayName || undefined,
      });
      
      console.log('Auto-route expanded:', result);
      
      // 2. Connect target node (will auto-connect all parent nodes)
      await connectNode(result.targetNodeId);
      
      // 3. Close dialog
      toggleModal('autoRoute', false);
      
      // Reset state
      setSelectedNode(null);
      setDisplayName('');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setConnecting(false);
    }
  };

  const handleClose = (open: boolean) => {
    if (!open) {
      setSelectedNode(null);
      setDisplayName('');
      setError(null);
    }
    toggleModal('autoRoute', open);
  };

  return (
    <Dialog open={modals.autoRoute} onOpenChange={handleClose}>
      <DialogContent className="max-w-lg max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Network className="h-5 w-5" />
            {t('modals.auto_route.title')}
          </DialogTitle>
          <DialogDescription>
            {t('modals.auto_route.description')}
          </DialogDescription>
        </DialogHeader>

        <div className="px-4 py-2">
        {/* Loading state */}
        {loading && (
          <div className="flex items-center justify-center py-8">
            <RefreshCw className="h-6 w-6 animate-spin text-theme-text-muted" />
            <span className="ml-2 text-theme-text-muted">{t('modals.auto_route.loading')}</span>
          </div>
        )}

        {/* No saved connections */}
        {!loading && nodes.length === 0 && (
          <div className="space-y-4 py-4">
            <div className="flex items-start gap-3 p-4 rounded-lg bg-blue-500/10 border border-blue-500/20">
              <Info className="h-5 w-5 text-blue-500 mt-0.5" />
              <div className="space-y-1">
                <p className="text-sm font-medium">{t('modals.auto_route.no_connections')}</p>
                <p className="text-xs text-theme-text-muted">
                  {t('modals.auto_route.no_connections_hint')}
                </p>
              </div>
            </div>
          </div>
        )}

        {/* Node list */}
        {!loading && nodes.length > 0 && (
          <div className="space-y-4">
            {/* Error message */}
            {error && (
              <div className="flex items-start gap-2 p-3 rounded-lg bg-red-500/10 border border-red-500/20">
                <AlertCircle className="h-4 w-4 text-red-500 mt-0.5" />
                <p className="text-xs text-red-400">{error}</p>
              </div>
            )}

            {/* Node selection */}
            <div className="space-y-2">
              <Label>{t('modals.auto_route.select_target')}</Label>
              <RadioGroup
                value={selectedNode?.id ?? ''}
                onValueChange={(value) => {
                  const node = nodes.find(n => n.id === value);
                  if (node) handleSelectNode(node);
                }}
                className="max-h-60 overflow-y-auto rounded-md border border-theme-border"
              >
                {nodes.map((node, index) => (
                  <label
                    key={node.id}
                    htmlFor={`node-${node.id}`}
                    className={`flex items-center gap-3 px-3 py-2.5 cursor-pointer transition-colors
                      ${index < nodes.length - 1 ? 'border-b border-theme-border/50' : ''}
                      ${selectedNode?.id === node.id
                        ? 'bg-theme-accent/10'
                        : 'hover:bg-theme-bg-hover'
                      }`}
                  >
                    <RadioGroupItem value={node.id} id={`node-${node.id}`} />
                    <Server className="h-4 w-4 shrink-0 text-theme-text-muted" />
                    <div className="flex-1 min-w-0">
                      <p className="text-sm font-medium truncate text-theme-text">
                        {node.displayName || node.id}
                      </p>
                      <p className="text-xs text-theme-text-muted truncate">
                        {node.username}@{node.host}:{node.port}
                      </p>
                    </div>
                  </label>
                ))}
              </RadioGroup>
            </div>

            {/* Selected node details */}
            {selectedNode && (
              <div className="space-y-4 p-4 rounded-md bg-theme-bg-panel border border-theme-border">
                <div className="space-y-2">
                  <Label htmlFor="displayName">{t('modals.auto_route.display_name')}</Label>
                  <Input
                    id="displayName"
                    value={displayName}
                    onChange={(e) => setDisplayName(e.target.value)}
                    placeholder={selectedNode.displayName || selectedNode.id}
                  />
                </div>

                <div className="space-y-1">
                  <p className="text-xs text-theme-text-muted">{t('modals.auto_route.connection_info')}</p>
                  <div className="text-sm font-mono bg-theme-bg/50 rounded px-2 py-1 text-theme-text">
                    {selectedNode.username}@{selectedNode.host}:{selectedNode.port}
                  </div>
                </div>

                <div className="space-y-1">
                  <p className="text-xs text-theme-text-muted">{t('modals.auto_route.authentication')}</p>
                  <div className="text-sm flex items-center gap-2 text-theme-text">
                    {selectedNode.authType === 'agent' && t('modals.auto_route.auth_agent')}
                    {selectedNode.authType === 'key' && t('modals.auto_route.auth_key')}
                    {selectedNode.authType === 'password' && (
                      <span className="text-amber-500">{t('modals.auto_route.auth_password_warning')}</span>
                    )}
                  </div>
                </div>

                {/* Neighbors info */}
                {selectedNode.neighbors && selectedNode.neighbors.length > 0 && (
                  <div className="space-y-1">
                    <p className="text-xs text-theme-text-muted">{t('modals.auto_route.can_reach')}</p>
                    <div className="flex flex-wrap gap-1">
                      {selectedNode.neighbors.map((neighbor) => (
                        <span
                          key={neighbor}
                          className="text-xs px-2 py-0.5 rounded bg-theme-bg/50 text-theme-text-muted"
                        >
                          {neighbor}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>
        )}
        </div>

        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => handleClose(false)}
            disabled={connecting}
          >
            {t('modals.auto_route.cancel')}
          </Button>
          <Button
            onClick={handleConnect}
            disabled={!selectedNode || connecting || selectedNode?.authType === 'password'}
          >
            {connecting ? (
              <>
                <RefreshCw className="h-4 w-4 mr-2 animate-spin" />
                {t('modals.auto_route.connecting')}
              </>
            ) : (
              t('modals.auto_route.connect')
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
