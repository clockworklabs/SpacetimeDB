import {
  useEffect,
  useMemo,
  useSyncExternalStore,
  createContext,
  useRef,
  useCallback,
} from 'react';
import * as React from 'react';
import {
  DbConnectionBuilder,
  type DbConnectionImpl,
} from '../sdk/db_connection_impl';
import {
  ConnectionManager,
  type ConnectionState as ManagedConnectionState,
} from '../sdk/connection_manager';
import { ConnectionId } from '../lib/connection_id';
import type { ConnectionState } from './connection_state';

/**
 * Per-module connection map keyed by the application-chosen label. Each entry
 * has the same shape as `useSpacetimeDB()`'s return value so hooks can accept
 * a key argument transparently.
 */
export type ManagedConnectionStateMap = Map<string, ConnectionState>;

export const SpacetimeDBMultiContext = createContext<
  ManagedConnectionStateMap | undefined
>(undefined);

export interface SpacetimeDBMultiProviderProps {
  /**
   * Map of application-chosen label → connection builder. One retained
   * connection per label. Labels drive the optional `key` argument on
   * `useSpacetimeDB(key)`, `useTable(key, ...)`, `useReducer(key, ...)`.
   *
   * The same underlying pool keys by `(uri, moduleName)` regardless of
   * label, so two `SpacetimeDBMultiProvider`s that refer to the same
   * `(uri, moduleName)` share a single WebSocket.
   *
   * **Do not inline new builders on every render.** Each builder identity
   * should be stable — build them outside the render or inside a `useMemo`.
   * The provider compares entries by `(label, uri, moduleName)` to absorb
   * accidental object-identity churn, but fresh builder objects every render
   * still do wasted work.
   */
  connections: Record<string, DbConnectionBuilder<DbConnectionImpl<any>>>;
  children?: React.ReactNode;
}

type Entry = {
  label: string;
  builder: DbConnectionBuilder<DbConnectionImpl<any>>;
  poolKey: string;
};

/** Fresh per-entry fallback so unrelated labels never collide on connectionId. */
function freshFallbackState(): ManagedConnectionState {
  return {
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
  };
}

/**
 * Mounts multiple SpacetimeDB connections under a single provider, one per
 * application-chosen label. Components inside the tree can read any module by
 * label via the `key` argument on `useSpacetimeDB`, `useTable`, and
 * `useReducer`.
 *
 * Connections are ref-counted by the shared ConnectionManager pool. Two
 * providers that reference the same `(uri, moduleName)` share a single
 * WebSocket; unmounting one provider releases its retain without tearing the
 * socket while the other is alive.
 *
 * StrictMode-safe: cleanup defers through the pool's setTimeout(0).
 */
export function SpacetimeDBMultiProvider({
  connections,
  children,
}: SpacetimeDBMultiProviderProps): React.JSX.Element {
  // Resolve entries with a content-based signature so inline `connections={{...}}`
  // props don't churn retain/release on every render. Two consecutive renders
  // whose (label, uri, moduleName) tuples match reuse the same `entries` array
  // identity, which keeps the effect + snapshot deps stable.
  const entriesRef = useRef<{ signature: string; entries: Entry[] } | null>(
    null
  );
  const entries = useMemo<Entry[]>(() => {
    const raw = Object.entries(connections).map(([label, builder]) => ({
      label,
      builder,
      poolKey: ConnectionManager.getKey(
        builder.getUri(),
        builder.getModuleName()
      ),
    }));
    const signature = raw.map(e => `${e.label}\0${e.poolKey}`).join('\n');
    if (entriesRef.current && entriesRef.current.signature === signature) {
      return entriesRef.current.entries;
    }
    entriesRef.current = { signature, entries: raw };
    return raw;
  }, [connections]);

  // Retain every entry for the lifetime of this provider.
  useEffect(() => {
    for (const { poolKey, builder } of entries) {
      ConnectionManager.retain(poolKey, builder);
    }
    return () => {
      for (const { poolKey } of entries) {
        ConnectionManager.release(poolKey);
      }
    };
  }, [entries]);

  // Subscribe to the union of entries' state so we re-render on any change.
  const subscribe = useCallback(
    (onChange: () => void) => {
      const unsubs = entries.map(({ poolKey }) =>
        ConnectionManager.subscribe(poolKey, onChange)
      );
      return () => {
        for (const u of unsubs) u();
      };
    },
    [entries]
  );

  // Snapshot: per-entry state object. We cache by a synthesized version number
  // so useSyncExternalStore doesn't tear — the Map reference changes only when
  // at least one underlying state ref changes.
  const snapshotRef = useRef<{
    states: ManagedConnectionState[];
    map: ManagedConnectionStateMap;
  } | null>(null);
  const fallbackStatesRef = useRef<Map<string, ManagedConnectionState>>(
    new Map()
  );

  const getSnapshot = useCallback((): ManagedConnectionStateMap => {
    const states = entries.map(({ label, poolKey }) => {
      const pooled = ConnectionManager.getSnapshot(poolKey);
      if (pooled) return pooled;
      // Stable per-label fallback so consumers don't see churning connectionIds
      // before the pool has a real state.
      let fb = fallbackStatesRef.current.get(label);
      if (!fb) {
        fb = freshFallbackState();
        fallbackStatesRef.current.set(label, fb);
      }
      return fb;
    });

    // Return the cached map if every state is reference-equal to the last
    // read. This is what keeps `useSyncExternalStore` stable across renders
    // that don't actually change pool state.
    if (
      snapshotRef.current &&
      snapshotRef.current.states.length === states.length &&
      snapshotRef.current.states.every((s, i) => s === states[i])
    ) {
      return snapshotRef.current.map;
    }

    const map: ManagedConnectionStateMap = new Map();
    for (let i = 0; i < entries.length; i++) {
      const { label, poolKey } = entries[i];
      const state = states[i];
      map.set(label, {
        ...state,
        getConnection: () =>
          ConnectionManager.getConnection<DbConnectionImpl<any>>(poolKey),
      });
    }

    snapshotRef.current = { states, map };
    return map;
  }, [entries]);

  const statusMap = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  return React.createElement(
    SpacetimeDBMultiContext.Provider,
    { value: statusMap },
    children
  );
}
