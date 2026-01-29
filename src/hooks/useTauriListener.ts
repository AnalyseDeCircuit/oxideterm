/**
 * Safe Tauri event listener hook
 * 
 * Properly handles the async nature of Tauri's listen() API to prevent
 * memory leaks when components unmount before the listener is registered.
 * 
 * Problem: Tauri's listen() returns a Promise<UnlistenFn>. If the component
 * unmounts before the Promise resolves, calling `unlisten.then(fn => fn())`
 * in cleanup will still execute but the listener may already be leaked.
 * 
 * Solution: Track mount state and clean up immediately if Promise resolves
 * after unmount.
 */

import { useEffect, useRef, useCallback } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

/**
 * Hook for safely listening to a single Tauri event
 * 
 * @param event - The event name to listen for
 * @param handler - The callback function to handle events
 * @param deps - Additional dependencies that should trigger re-subscription
 * 
 * @example
 * ```tsx
 * useTauriListener('connection_status_changed', (payload) => {
 *   console.log('Status:', payload.status);
 * });
 * ```
 */
export function useTauriListener<T>(
  event: string,
  handler: (payload: T) => void,
  deps: React.DependencyList = []
): void {
  // Use ref to always have access to latest handler without re-subscribing
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let mounted = true;
    let unlisten: UnlistenFn | null = null;

    listen<T>(event, (e) => {
      if (mounted) {
        handlerRef.current(e.payload);
      }
    }).then((fn) => {
      if (mounted) {
        unlisten = fn;
      } else {
        // Component unmounted before listener was registered, clean up immediately
        fn();
      }
    }).catch((error) => {
      console.error(`[useTauriListener] Failed to listen to ${event}:`, error);
    });

    return () => {
      mounted = false;
      unlisten?.();
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [event, ...deps]);
}

/**
 * Hook for safely listening to multiple Tauri events
 * 
 * @param listeners - Array of [eventName, handler] tuples
 * @param deps - Additional dependencies that should trigger re-subscription
 * 
 * @example
 * ```tsx
 * useTauriListeners([
 *   ['connection_status_changed', handleStatus],
 *   ['connection_reconnect_progress', handleProgress],
 * ]);
 * ```
 */
export function useTauriListeners<T extends Record<string, unknown>>(
  listeners: Array<[string, (payload: T[keyof T]) => void]>,
  deps: React.DependencyList = []
): void {
  // Store handlers in refs to avoid re-subscription on handler changes
  const handlersRef = useRef(listeners);
  handlersRef.current = listeners;

  useEffect(() => {
    let mounted = true;
    const unlisteners: UnlistenFn[] = [];

    const setupListeners = async () => {
      for (const [event, _handler] of handlersRef.current) {
        try {
          const unlisten = await listen(event, (e) => {
            if (mounted) {
              // Find the current handler for this event
              const handler = handlersRef.current.find(([ev]) => ev === event)?.[1];
              handler?.(e.payload as T[keyof T]);
            }
          });

          if (mounted) {
            unlisteners.push(unlisten);
          } else {
            // Component unmounted, clean up immediately
            unlisten();
          }
        } catch (error) {
          console.error(`[useTauriListeners] Failed to listen to ${event}:`, error);
        }
      }
    };

    setupListeners();

    return () => {
      mounted = false;
      unlisteners.forEach((unlisten) => unlisten());
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}

/**
 * Creates a stable callback that sets up a Tauri listener and returns cleanup
 * Useful for imperative listener setup in complex scenarios
 * 
 * @returns A function that when called, sets up the listener and returns cleanup
 */
export function useTauriListenerCallback<T>(
  event: string,
  handler: (payload: T) => void
): () => () => void {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  return useCallback(() => {
    let mounted = true;
    let unlisten: UnlistenFn | null = null;

    listen<T>(event, (e) => {
      if (mounted) {
        handlerRef.current(e.payload);
      }
    }).then((fn) => {
      if (mounted) {
        unlisten = fn;
      } else {
        fn();
      }
    });

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [event]);
}
