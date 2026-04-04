import { describe, expect, it, beforeEach } from 'vitest';
import { topologyResolver } from '@/lib/topologyResolver';

describe('TopologyResolver', () => {
  beforeEach(() => {
    topologyResolver.clear();
  });

  describe('register / getNodeId / getConnectionId', () => {
    it('creates bidirectional mapping', () => {
      topologyResolver.register('conn-1', 'node-1');
      expect(topologyResolver.getNodeId('conn-1')).toBe('node-1');
      expect(topologyResolver.getConnectionId('node-1')).toBe('conn-1');
    });

    it('handles multiple mappings', () => {
      topologyResolver.register('conn-1', 'node-1');
      topologyResolver.register('conn-2', 'node-2');

      expect(topologyResolver.getNodeId('conn-1')).toBe('node-1');
      expect(topologyResolver.getNodeId('conn-2')).toBe('node-2');
      expect(topologyResolver.size()).toBe(2);
    });

    it('overwrites existing mapping for same connectionId', () => {
      topologyResolver.register('conn-1', 'node-1');
      topologyResolver.register('conn-1', 'node-2');

      expect(topologyResolver.getNodeId('conn-1')).toBe('node-2');
    });

    it('returns undefined for unknown connectionId', () => {
      expect(topologyResolver.getNodeId('unknown')).toBeUndefined();
    });

    it('returns undefined for unknown nodeId', () => {
      expect(topologyResolver.getConnectionId('unknown')).toBeUndefined();
    });
  });

  describe('unregister', () => {
    it('removes both directions', () => {
      topologyResolver.register('conn-1', 'node-1');
      topologyResolver.unregister('node-1');

      expect(topologyResolver.getNodeId('conn-1')).toBeUndefined();
      expect(topologyResolver.getConnectionId('node-1')).toBeUndefined();
      expect(topologyResolver.size()).toBe(0);
    });

    it('is silent for unknown nodeId', () => {
      topologyResolver.register('conn-1', 'node-1');
      topologyResolver.unregister('unknown');

      expect(topologyResolver.size()).toBe(1);
    });

    it('only removes the target node', () => {
      topologyResolver.register('conn-1', 'node-1');
      topologyResolver.register('conn-2', 'node-2');
      topologyResolver.unregister('node-1');

      expect(topologyResolver.getNodeId('conn-2')).toBe('node-2');
      expect(topologyResolver.size()).toBe(1);
    });
  });

  describe('handleLinkDown', () => {
    it('returns affected nodeIds', () => {
      topologyResolver.register('conn-1', 'node-1');
      topologyResolver.register('conn-2', 'node-2');
      topologyResolver.register('conn-3', 'node-3');

      const result = topologyResolver.handleLinkDown('conn-1', ['conn-2', 'conn-3']);
      expect(result).toEqual(['node-1', 'node-2', 'node-3']);
    });

    it('skips unknown children', () => {
      topologyResolver.register('conn-1', 'node-1');

      const result = topologyResolver.handleLinkDown('conn-1', ['unknown-child']);
      expect(result).toEqual(['node-1']);
    });

    it('returns empty for unknown connectionId with no children', () => {
      const result = topologyResolver.handleLinkDown('unknown', []);
      expect(result).toEqual([]);
    });
  });

  describe('handleReconnected', () => {
    it('returns nodeId for known connection', () => {
      topologyResolver.register('conn-1', 'node-1');
      expect(topologyResolver.handleReconnected('conn-1')).toBe('node-1');
    });

    it('returns null for unknown connection', () => {
      expect(topologyResolver.handleReconnected('unknown')).toBeNull();
    });
  });

  describe('clear', () => {
    it('removes all mappings', () => {
      topologyResolver.register('conn-1', 'node-1');
      topologyResolver.register('conn-2', 'node-2');
      topologyResolver.clear();

      expect(topologyResolver.size()).toBe(0);
      expect(topologyResolver.getNodeId('conn-1')).toBeUndefined();
    });
  });

  describe('size', () => {
    it('returns 0 when empty', () => {
      expect(topologyResolver.size()).toBe(0);
    });

    it('tracks mapping count', () => {
      topologyResolver.register('conn-1', 'node-1');
      expect(topologyResolver.size()).toBe(1);
      topologyResolver.register('conn-2', 'node-2');
      expect(topologyResolver.size()).toBe(2);
    });
  });
});
