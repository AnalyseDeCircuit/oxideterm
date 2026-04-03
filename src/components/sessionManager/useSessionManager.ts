// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState, useEffect, useCallback, useMemo } from 'react';
import { api } from '../../lib/api';
import type { ConnectionInfo } from '../../types';

export type SortField = 'name' | 'host' | 'port' | 'username' | 'auth_type' | 'group' | 'last_used_at';
export type SortDirection = 'asc' | 'desc';

export type FolderNode = {
  name: string;
  fullPath: string;
  children: FolderNode[];
  connectionCount: number;
};

const RECENT_LIMIT = 20;

export function useSessionManager() {
  // Data
  const [connections, setConnections] = useState<ConnectionInfo[]>([]);
  const [groups, setGroups] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  // Folder tree
  const [selectedGroup, setSelectedGroup] = useState<string | null>(null);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

  // Table
  const [searchQuery, setSearchQuery] = useState('');
  const [sortField, setSortField] = useState<SortField>('last_used_at');
  const [sortDirection, setSortDirection] = useState<SortDirection>('desc');
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  // Load data
  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [conns, grps] = await Promise.all([
        api.getConnections(),
        api.getGroups(),
      ]);
      setConnections(conns);
      setGroups(grps);
    } catch (err) {
      console.error('Failed to load session manager data:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Refresh after mutations
  const refresh = useCallback(async () => {
    try {
      const [conns, grps] = await Promise.all([
        api.getConnections(),
        api.getGroups(),
      ]);
      setConnections(conns);
      setGroups(grps);
      // Clear selection of deleted items
      setSelectedIds(prev => {
        const validIds = new Set(conns.map(c => c.id));
        const next = new Set<string>();
        for (const id of prev) {
          if (validIds.has(id)) next.add(id);
        }
        return next;
      });
    } catch (err) {
      console.error('Failed to refresh data:', err);
    }
  }, []);

  // Listen for external save events (e.g. NewConnectionModal)
  useEffect(() => {
    const handler = () => { refresh(); };
    window.addEventListener('saved-connections-changed', handler);
    return () => window.removeEventListener('saved-connections-changed', handler);
  }, [refresh]);

  // Build folder tree from groups
  const folderTree = useMemo((): FolderNode[] => {
    const root: FolderNode[] = [];
    const pathMap = new Map<string, FolderNode>();

    // Count connections per group (including nested)
    const groupCounts = new Map<string, number>();
    for (const conn of connections) {
      const g = conn.group || '';
      groupCounts.set(g, (groupCounts.get(g) || 0) + 1);
    }

    // Create nodes for all groups (support "/" nesting)
    const allPaths = new Set<string>();
    for (const g of groups) {
      if (!g) continue;
      // Add all path segments
      const parts = g.split('/');
      for (let i = 1; i <= parts.length; i++) {
        allPaths.add(parts.slice(0, i).join('/'));
      }
    }
    // Also add paths from connections that might not be in groups list
    for (const conn of connections) {
      if (conn.group) {
        const parts = conn.group.split('/');
        for (let i = 1; i <= parts.length; i++) {
          allPaths.add(parts.slice(0, i).join('/'));
        }
      }
    }

    // Sort paths to ensure parents before children
    const sortedPaths = Array.from(allPaths).sort();

    for (const path of sortedPaths) {
      const parts = path.split('/');
      const name = parts[parts.length - 1];

      // Count connections in this group AND all subgroups
      let count = 0;
      for (const [g, c] of groupCounts) {
        if (g === path || g.startsWith(path + '/')) {
          count += c;
        }
      }

      const node: FolderNode = { name, fullPath: path, children: [], connectionCount: count };
      pathMap.set(path, node);

      if (parts.length === 1) {
        root.push(node);
      } else {
        const parentPath = parts.slice(0, -1).join('/');
        const parent = pathMap.get(parentPath);
        if (parent) {
          parent.children.push(node);
        }
      }
    }

    return root;
  }, [connections, groups]);

  // Ungrouped connection count
  const ungroupedCount = useMemo(() => {
    return connections.filter(c => !c.group).length;
  }, [connections]);

  // Filter and sort connections
  const filteredConnections = useMemo(() => {
    let result = connections;

    // Filter by group
    if (selectedGroup === '__ungrouped__') {
      result = result.filter(c => !c.group);
    } else if (selectedGroup === '__recent__') {
      // Sort by last_used_at descending, take top N
      result = result
        .filter(c => c.last_used_at)
        .sort((a, b) => (b.last_used_at || '').localeCompare(a.last_used_at || ''))
        .slice(0, RECENT_LIMIT);
    } else if (selectedGroup) {
      result = result.filter(c =>
        c.group === selectedGroup || c.group?.startsWith(selectedGroup + '/')
      );
    }

    // Filter by search
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      result = result.filter(c =>
        c.name.toLowerCase().includes(q) ||
        c.host.toLowerCase().includes(q) ||
        c.username.toLowerCase().includes(q) ||
        (c.group?.toLowerCase().includes(q)) ||
        c.tags.some(t => t.toLowerCase().includes(q))
      );
    }

    // Sort
    if (sortField) {
      result = [...result].sort((a, b) => {
        let aVal: string | number | null = null;
        let bVal: string | number | null = null;

        switch (sortField) {
          case 'name': aVal = a.name; bVal = b.name; break;
          case 'host': aVal = a.host; bVal = b.host; break;
          case 'port': aVal = a.port; bVal = b.port; break;
          case 'username': aVal = a.username; bVal = b.username; break;
          case 'auth_type': aVal = a.auth_type; bVal = b.auth_type; break;
          case 'group': aVal = a.group || ''; bVal = b.group || ''; break;
          case 'last_used_at': aVal = a.last_used_at || ''; bVal = b.last_used_at || ''; break;
        }

        if (aVal === null || bVal === null) return 0;
        if (typeof aVal === 'number' && typeof bVal === 'number') {
          return sortDirection === 'asc' ? aVal - bVal : bVal - aVal;
        }
        const cmp = String(aVal).localeCompare(String(bVal));
        return sortDirection === 'asc' ? cmp : -cmp;
      });
    }

    return result;
  }, [connections, selectedGroup, searchQuery, sortField, sortDirection]);

  // Sort toggle
  const toggleSort = useCallback((field: SortField) => {
    setSortField(prev => {
      if (prev === field) {
        setSortDirection(d => d === 'asc' ? 'desc' : 'asc');
        return field;
      }
      setSortDirection('asc');
      return field;
    });
  }, []);

  // Selection
  const toggleSelect = useCallback((id: string) => {
    setSelectedIds(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const toggleSelectAll = useCallback(() => {
    setSelectedIds(prev => {
      if (prev.size === filteredConnections.length) {
        return new Set();
      }
      return new Set(filteredConnections.map(c => c.id));
    });
  }, [filteredConnections]);

  const clearSelection = useCallback(() => {
    setSelectedIds(new Set());
  }, []);

  // Toggle folder expand
  const toggleExpand = useCallback((path: string) => {
    setExpandedGroups(prev => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  return {
    // Data
    connections: filteredConnections,
    allConnections: connections,
    groups,
    loading,
    folderTree,
    ungroupedCount,

    // Folder tree
    selectedGroup,
    setSelectedGroup,
    expandedGroups,
    toggleExpand,

    // Table
    searchQuery,
    setSearchQuery,
    sortField,
    sortDirection,
    toggleSort,

    // Selection
    selectedIds,
    toggleSelect,
    toggleSelectAll,
    clearSelection,

    // Actions
    refresh,
    loadData,
  };
}
