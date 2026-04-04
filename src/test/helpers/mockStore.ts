type Selector<T, R = unknown> = (value: T) => R;

type SelectorStore<T extends object> = {
  (selector?: Selector<T>): unknown;
  getState: () => T;
};

type MutableSelectorStore<T extends object> = SelectorStore<T> & {
  setState: (patch: Partial<T> | ((current: T) => Partial<T>)) => void;
  subscribe: {
    (listener: (state: T) => void): () => void;
    <R>(selector: Selector<T, R>, listener: (slice: R, previousSlice: R) => void): () => void;
  };
};

export function createSelectorStore<T extends object>(state: T): SelectorStore<T> {
  const store = ((selector?: Selector<T>) => (selector ? selector(state) : state)) as SelectorStore<T>;
  store.getState = () => state;
  return store;
}

export function createMutableSelectorStore<T extends object>(state: T): MutableSelectorStore<T> {
  const listeners = new Set<(state: T) => void>();
  const selectorListeners = new Set<{
    selector: Selector<T>;
    listener: (slice: unknown, previousSlice: unknown) => void;
    previousSlice: unknown;
  }>();

  const store = createSelectorStore(state) as MutableSelectorStore<T>;

  store.setState = (patch) => {
    const nextPatch = typeof patch === 'function' ? patch(state) : patch;
    Object.assign(state, nextPatch);

    for (const listener of listeners) {
      listener(state);
    }

    for (const entry of selectorListeners) {
      const nextSlice = entry.selector(state);
      if (nextSlice !== entry.previousSlice) {
        const previousSlice = entry.previousSlice;
        entry.previousSlice = nextSlice;
        entry.listener(nextSlice, previousSlice);
      }
    }
  };

  const subscribe = ((selectorOrListener: Selector<T> | ((state: T) => void), maybeListener?: (slice: unknown, previousSlice: unknown) => void) => {
    if (maybeListener) {
      const entry = {
        selector: selectorOrListener as Selector<T>,
        listener: maybeListener,
        previousSlice: (selectorOrListener as Selector<T>)(state),
      };
      selectorListeners.add(entry);
      return () => selectorListeners.delete(entry);
    }

    const listener = selectorOrListener as (state: T) => void;
    listeners.add(listener);
    return () => listeners.delete(listener);
  }) as MutableSelectorStore<T>['subscribe'];

  store.subscribe = subscribe;
  return store;
}