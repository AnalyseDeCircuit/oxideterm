import { describe, expect, it, beforeEach } from 'vitest';
import { useEventLogStore } from '@/store/eventLogStore';

function getStore() {
  return useEventLogStore.getState();
}

function reset() {
  useEventLogStore.setState({
    entries: [],
    isOpen: false,
    panelSize: 25,
    filter: { severity: 'all', category: 'all', search: '' },
    _nextId: 1,
    unreadCount: 0,
    unreadErrors: 0,
  });
}

const baseEntry = {
  severity: 'info' as const,
  category: 'connection' as const,
  title: 'Connected',
  source: 'test',
};

describe('eventLogStore', () => {
  beforeEach(reset);

  describe('addEntry', () => {
    it('adds an entry with auto-generated id and timestamp', () => {
      getStore().addEntry(baseEntry);
      const { entries } = getStore();

      expect(entries).toHaveLength(1);
      expect(entries[0].id).toBe(1);
      expect(entries[0].timestamp).toBeGreaterThan(0);
      expect(entries[0].title).toBe('Connected');
    });

    it('increments id monotonically', () => {
      getStore().addEntry(baseEntry);
      getStore().addEntry(baseEntry);
      const { entries } = getStore();

      expect(entries[0].id).toBe(1);
      expect(entries[1].id).toBe(2);
    });

    it('increments unreadCount when panel is closed', () => {
      getStore().addEntry(baseEntry);
      getStore().addEntry(baseEntry);

      expect(getStore().unreadCount).toBe(2);
    });

    it('does not increment unreadCount when panel is open', () => {
      getStore().openPanel();
      getStore().addEntry(baseEntry);

      expect(getStore().unreadCount).toBe(0);
    });

    it('tracks unreadErrors for error severity', () => {
      getStore().addEntry({ ...baseEntry, severity: 'error' });
      getStore().addEntry({ ...baseEntry, severity: 'info' });

      expect(getStore().unreadErrors).toBe(1);
    });

    it('caps at MAX_ENTRIES (500)', () => {
      for (let i = 0; i < 510; i++) {
        getStore().addEntry({ ...baseEntry, title: `Event ${i}` });
      }
      const { entries } = getStore();

      expect(entries.length).toBe(500);
      // First entry should be Event 10 (0-9 evicted)
      expect(entries[0].title).toBe('Event 10');
    });
  });

  describe('clear', () => {
    it('removes all entries and resets counters', () => {
      getStore().addEntry(baseEntry);
      getStore().addEntry({ ...baseEntry, severity: 'error' });
      getStore().clear();

      expect(getStore().entries).toHaveLength(0);
      expect(getStore().unreadCount).toBe(0);
      expect(getStore().unreadErrors).toBe(0);
    });
  });

  describe('togglePanel', () => {
    it('opens panel and resets unread', () => {
      getStore().addEntry(baseEntry);
      getStore().togglePanel();

      expect(getStore().isOpen).toBe(true);
      expect(getStore().unreadCount).toBe(0);
    });

    it('closes panel on second toggle', () => {
      getStore().togglePanel(); // open
      getStore().togglePanel(); // close

      expect(getStore().isOpen).toBe(false);
    });

    it('resets filter when opening', () => {
      getStore().setFilter({ severity: 'error' });
      getStore().togglePanel(); // open

      expect(getStore().filter).toEqual({
        severity: 'all',
        category: 'all',
        search: '',
      });
    });
  });

  describe('openPanel / closePanel', () => {
    it('openPanel clears unread and resets filter', () => {
      getStore().addEntry({ ...baseEntry, severity: 'error' });
      getStore().openPanel();

      expect(getStore().isOpen).toBe(true);
      expect(getStore().unreadCount).toBe(0);
      expect(getStore().unreadErrors).toBe(0);
    });

    it('closePanel hides panel', () => {
      getStore().openPanel();
      getStore().closePanel();

      expect(getStore().isOpen).toBe(false);
    });
  });

  describe('setPanelSize', () => {
    it('updates panel size', () => {
      getStore().setPanelSize(40);
      expect(getStore().panelSize).toBe(40);
    });
  });

  describe('setFilter', () => {
    it('merges partial filter', () => {
      getStore().setFilter({ severity: 'error' });
      expect(getStore().filter.severity).toBe('error');
      expect(getStore().filter.category).toBe('all'); // unchanged
    });

    it('updates search string', () => {
      getStore().setFilter({ search: 'timeout' });
      expect(getStore().filter.search).toBe('timeout');
    });
  });

  describe('markRead', () => {
    it('resets unread counters', () => {
      getStore().addEntry(baseEntry);
      getStore().addEntry({ ...baseEntry, severity: 'error' });
      getStore().markRead();

      expect(getStore().unreadCount).toBe(0);
      expect(getStore().unreadErrors).toBe(0);
    });
  });
});
