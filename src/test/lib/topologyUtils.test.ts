import { describe, expect, it, beforeEach } from 'vitest';
import {
  buildTopologyTree,
  getNodeColor,
  calculateTreeLayout,
  topologyCache,
} from '@/lib/topologyUtils';
import type { FlatNode, TreeNodeState } from '@/types';

/** Helper to create a FlatNode test fixture */
function makeFlatNode(
  id: string,
  overrides: Partial<FlatNode> = {}
): FlatNode {
  return {
    id,
    parentId: null,
    depth: 0,
    host: '192.168.1.1',
    port: 22,
    username: 'root',
    displayName: null,
    state: { status: 'connected' },
    hasChildren: false,
    isLastChild: true,
    originType: 'saved',
    terminalSessionId: null,
    sftpSessionId: null,
    sshConnectionId: null,
    ...overrides,
  } as FlatNode;
}

describe('buildTopologyTree', () => {
  it('returns empty array for empty input', () => {
    expect(buildTopologyTree([])).toEqual([]);
  });

  it('converts single root node', () => {
    const nodes = [makeFlatNode('n1', { username: 'admin', host: 'server1' })];
    const tree = buildTopologyTree(nodes);

    expect(tree).toHaveLength(1);
    expect(tree[0].id).toBe('n1');
    expect(tree[0].host).toBe('server1');
    expect(tree[0].username).toBe('admin');
    expect(tree[0].status).toBe('connected');
    expect(tree[0].children).toEqual([]);
  });

  it('uses displayName when present', () => {
    const nodes = [
      makeFlatNode('n1', { displayName: 'My Server', username: 'root', host: 'host' }),
    ];
    const tree = buildTopologyTree(nodes);
    expect(tree[0].name).toBe('My Server');
  });

  it('falls back to username@host when no displayName', () => {
    const nodes = [
      makeFlatNode('n1', { displayName: null, username: 'admin', host: 'db.local' }),
    ];
    const tree = buildTopologyTree(nodes);
    expect(tree[0].name).toBe('admin@db.local');
  });

  it('builds parent-child relationships', () => {
    const nodes = [
      makeFlatNode('parent', { hasChildren: true }),
      makeFlatNode('child', { parentId: 'parent', depth: 1 }),
    ];
    const tree = buildTopologyTree(nodes);

    expect(tree).toHaveLength(1);
    expect(tree[0].id).toBe('parent');
    expect(tree[0].children).toHaveLength(1);
    expect(tree[0].children[0].id).toBe('child');
  });

  it('handles multi-level nesting', () => {
    const nodes = [
      makeFlatNode('root', { hasChildren: true }),
      makeFlatNode('mid', { parentId: 'root', depth: 1, hasChildren: true }),
      makeFlatNode('leaf', { parentId: 'mid', depth: 2 }),
    ];
    const tree = buildTopologyTree(nodes);

    expect(tree).toHaveLength(1);
    expect(tree[0].children[0].id).toBe('mid');
    expect(tree[0].children[0].children[0].id).toBe('leaf');
  });

  it('promotes orphans to root level', () => {
    const nodes = [
      makeFlatNode('orphan', { parentId: 'missing-parent', depth: 1 }),
    ];
    const tree = buildTopologyTree(nodes);

    expect(tree).toHaveLength(1);
    expect(tree[0].id).toBe('orphan');
  });

  it('handles multiple root nodes', () => {
    const nodes = [
      makeFlatNode('r1'),
      makeFlatNode('r2'),
      makeFlatNode('r3'),
    ];
    const tree = buildTopologyTree(nodes);
    expect(tree).toHaveLength(3);
  });

  it('maps all status values correctly', () => {
    const statuses: TreeNodeState[] = [
      { status: 'connected' },
      { status: 'connecting' },
      { status: 'disconnected' },
      { status: 'failed', error: 'timeout' },
      { status: 'pending' },
    ];

    statuses.forEach((state) => {
      const nodes = [makeFlatNode('n', { state })];
      const tree = buildTopologyTree(nodes);
      expect(tree[0].status).toBe(state.status);
    });
  });
});

describe('getNodeColor', () => {
  it('returns green for connected', () => {
    expect(getNodeColor('connected')).toBe('#4CAF50');
  });

  it('returns yellow for connecting', () => {
    expect(getNodeColor('connecting')).toBe('#FFC107');
  });

  it('returns red for failed', () => {
    expect(getNodeColor('failed')).toBe('#F44336');
  });

  it('returns grey for pending', () => {
    expect(getNodeColor('pending')).toBe('#9E9E9E');
  });

  it('returns grey for disconnected', () => {
    expect(getNodeColor('disconnected')).toBe('#9E9E9E');
  });
});

describe('calculateTreeLayout', () => {
  it('positions a single root node', () => {
    const tree = buildTopologyTree([makeFlatNode('r')]);
    const layers = calculateTreeLayout(tree);

    expect(layers).toHaveLength(1);
    expect(layers[0]).toHaveLength(1);
    expect(layers[0][0].y).toBe(50); // depth=0 → y = 50 + 0*80
  });

  it('creates separate layers for different depths', () => {
    const tree = buildTopologyTree([
      makeFlatNode('root', { hasChildren: true }),
      makeFlatNode('child', { parentId: 'root', depth: 1 }),
    ]);
    const layers = calculateTreeLayout(tree);

    expect(layers.length).toBeGreaterThanOrEqual(2);
    expect(layers[0]).toHaveLength(1);
    expect(layers[1]).toHaveLength(1);
    expect(layers[1][0].y).toBeGreaterThan(layers[0][0].y);
  });

  it('assigns width and height from options', () => {
    const tree = buildTopologyTree([makeFlatNode('r')]);
    const layers = calculateTreeLayout(tree, { nodeWidth: 200, nodeHeight: 60 });

    expect(layers[0][0].width).toBe(200);
    expect(layers[0][0].height).toBe(60);
  });

  it('lays out multiple siblings', () => {
    const tree = buildTopologyTree([
      makeFlatNode('root', { hasChildren: true }),
      makeFlatNode('c1', { parentId: 'root', depth: 1 }),
      makeFlatNode('c2', { parentId: 'root', depth: 1 }),
      makeFlatNode('c3', { parentId: 'root', depth: 1 }),
    ]);
    const layers = calculateTreeLayout(tree);

    expect(layers[1]).toHaveLength(3);
    // siblings should have distinct x positions
    const xs = layers[1].map(n => n.x);
    expect(new Set(xs).size).toBe(3);
  });
});

describe('topologyCache', () => {
  beforeEach(() => {
    topologyCache.invalidate();
    topologyCache.resetStats();
  });

  it('returns same tree on repeated calls with same data', () => {
    const nodes = [makeFlatNode('n1')];
    const tree1 = topologyCache.buildWithCache(nodes);
    const tree2 = topologyCache.buildWithCache(nodes);

    expect(tree1).toBe(tree2); // same reference
    expect(topologyCache.getStats().hitCount).toBe(1);
    expect(topologyCache.getStats().missCount).toBe(1);
  });

  it('rebuilds when node data changes', () => {
    const nodes1 = [makeFlatNode('n1', { state: { status: 'connected' } })];
    const nodes2 = [makeFlatNode('n1', { state: { status: 'disconnected' } })];

    const tree1 = topologyCache.buildWithCache(nodes1);
    const tree2 = topologyCache.buildWithCache(nodes2);

    expect(tree1).not.toBe(tree2);
    expect(topologyCache.getStats().missCount).toBe(2);
  });

  it('invalidate forces rebuild', () => {
    const nodes = [makeFlatNode('n1')];
    topologyCache.buildWithCache(nodes);
    topologyCache.invalidate();
    topologyCache.buildWithCache(nodes);

    expect(topologyCache.getStats().missCount).toBe(2);
  });

  it('hit rate calculation', () => {
    const nodes = [makeFlatNode('n1')];
    topologyCache.buildWithCache(nodes);
    topologyCache.buildWithCache(nodes);
    topologyCache.buildWithCache(nodes);

    const stats = topologyCache.getStats();
    expect(stats.hitCount).toBe(2);
    expect(stats.missCount).toBe(1);
    // hitRate = 2/3 * 100 ≈ 66.67
    expect(stats.hitRate).toBeCloseTo(66.67, 0);
  });
});
