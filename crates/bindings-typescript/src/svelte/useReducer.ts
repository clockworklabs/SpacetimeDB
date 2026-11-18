import { getByAccessorPath } from '../lib/accessor_path.ts';
import { onDestroy } from 'svelte';
import { get } from 'svelte/store';
import type { UntypedReducerDef } from '../sdk/reducers.ts';
import type { ParamsType } from '../sdk/type_utils.ts';
import { useSpacetimeDB } from './useSpacetimeDB.ts';

export function useReducer<ReducerDef extends UntypedReducerDef>(
  reducerDef: ReducerDef
): (...params: ParamsType<ReducerDef>) => Promise<void> {
  const connectionStore = useSpacetimeDB();
  const reducerName = reducerDef.accessorName;

  // Holds calls made before the connection exists
  const queueRef: {
    params: ParamsType<ReducerDef>;
    resolve: () => void;
    reject: (err: unknown) => void;
  }[] = [];

  // Flush when we finally have a connection
  const unsubscribe = connectionStore.subscribe(state => {
    const conn = state.getConnection();
    if (!conn) return;

    const fn = getByAccessorPath(conn.reducers, reducerName) as (
      ...p: ParamsType<ReducerDef>
    ) => Promise<void>;
    if (queueRef.length) {
      const pending = queueRef.splice(0);
      for (const item of pending) {
        fn(...item.params).then(item.resolve, item.reject);
      }
    }
  });

  onDestroy(() => {
    unsubscribe();
  });

  return (...params: ParamsType<ReducerDef>) => {
    const state = get(connectionStore);
    const conn = state.getConnection();
    if (!conn) {
      return new Promise<void>((resolve, reject) => {
        queueRef.push({ params, resolve, reject });
      });
    }
    const fn = getByAccessorPath(conn.reducers, reducerName) as (
      ...p: ParamsType<ReducerDef>
    ) => Promise<void>;
    return fn(...params);
  };
}
