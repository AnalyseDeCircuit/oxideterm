/**
 * Plugin Event Bridge
 *
 * A simple pub/sub event bus for plugin system events.
 * Bridges appStore state changes to plugin lifecycle events.
 * All handlers are called via queueMicrotask() to avoid blocking state updates.
 */

import { toSnapshot } from './pluginUtils';

type EventHandler = (data: unknown) => void;

class PluginEventBridge {
  private handlers = new Map<string, Set<EventHandler>>();

  /**
   * Subscribe to an event. Returns an unsubscribe function.
   */
  on(event: string, handler: EventHandler): () => void {
    if (!this.handlers.has(event)) {
      this.handlers.set(event, new Set());
    }
    this.handlers.get(event)!.add(handler);

    return () => {
      const set = this.handlers.get(event);
      if (set) {
        set.delete(handler);
        if (set.size === 0) this.handlers.delete(event);
      }
    };
  }

  /**
   * Emit an event to all subscribers.
   * Handlers are called asynchronously via queueMicrotask().
   */
  emit(event: string, data: unknown): void {
    const set = this.handlers.get(event);
    if (!set || set.size === 0) return;

    for (const handler of set) {
      queueMicrotask(() => {
        try {
          handler(data);
        } catch (err) {
          console.error(`[PluginEventBridge] Error in handler for "${event}":`, err);
        }
      });
    }
  }

  /**
   * Remove all handlers (used during cleanup).
   */
  clear(): void {
    this.handlers.clear();
  }
}

/** Singleton event bridge instance */
export const pluginEventBridge = new PluginEventBridge();

/**
 * Wire appStore connection state changes → plugin events.
 * Call once at app startup. Returns an unsubscribe function.
 *
 * Accepts the useAppStore reference to avoid `require()` (not available in ESM/Vite).
 */
export function setupConnectionBridge(
  useAppStore: typeof import('../../store/appStore').useAppStore,
): () => void {
  let prevConnections = new Map(useAppStore.getState().connections);

  const unsubscribe = useAppStore.subscribe((state) => {
    const curr = state.connections;

    // Detect new, changed, and removed connections
    for (const [id, conn] of curr) {
      const prev = prevConnections.get(id);
      const snapshot = toSnapshot(conn);

      if (!prev) {
        // New connection appeared
        if (conn.state === 'active') {
          pluginEventBridge.emit('connection:connect', snapshot);
        }
      } else if (prev.state !== conn.state) {
        // State transition
        if (conn.state === 'active' && prev.state !== 'active') {
          // Became active — was it a reconnect or a fresh connect?
          const wasReconnecting = prev.state === 'reconnecting' || prev.state === 'link_down' || typeof prev.state === 'object';
          if (wasReconnecting) {
            pluginEventBridge.emit('connection:reconnect', snapshot);
          } else {
            pluginEventBridge.emit('connection:connect', snapshot);
          }
        } else if (conn.state === 'reconnecting' || conn.state === 'link_down' || typeof conn.state === 'object') {
          pluginEventBridge.emit('connection:link_down', snapshot);
        } else if (conn.state === 'idle' && prev.state === 'active') {
          // idle = live SSH connection with no terminals; not a disconnect
          pluginEventBridge.emit('connection:idle', snapshot);
        } else if (conn.state === 'disconnected' || conn.state === 'disconnecting') {
          // Emit disconnect for any prev state (active, link_down, reconnecting, etc.)
          pluginEventBridge.emit('connection:disconnect', snapshot);
        }
      }

      // Detect session (terminal) changes
      if (prev) {
        const prevTerminals = new Set(prev.terminalIds);
        const currTerminals = new Set(conn.terminalIds);

        // New sessions
        for (const tid of currTerminals) {
          if (!prevTerminals.has(tid)) {
            pluginEventBridge.emit('session:created', { sessionId: tid, connectionId: id });
          }
        }
        // Removed sessions
        for (const tid of prevTerminals) {
          if (!currTerminals.has(tid)) {
            pluginEventBridge.emit('session:closed', { sessionId: tid });
          }
        }
      }
    }

    // Detect removed connections
    for (const [id, prev] of prevConnections) {
      if (!curr.has(id)) {
        // Only emit disconnect if we haven't already emitted one for the
        // state transition (e.g. active → disconnected → removed).
        if (prev.state !== 'disconnected' && prev.state !== 'disconnecting') {
          pluginEventBridge.emit('connection:disconnect', toSnapshot(prev));
        }
      }
    }

    prevConnections = new Map(curr);
  });

  return unsubscribe;
}
