import { onDestroy } from 'svelte';
import { get } from 'svelte/store';
import type { InferTypeOfRow } from '../lib/type_builders';
import type { UntypedReducerDef } from '../sdk/reducers';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { Prettify } from '../lib/type_util';

type IsEmptyObject<T> = [keyof T] extends [never] ? true : false;
type MaybeParams<T> = IsEmptyObject<T> extends true ? [] : [params: T];

type ParamsType<R extends UntypedReducerDef> = MaybeParams<
  Prettify<InferTypeOfRow<R['params']>>
>;

export function useReducer<ReducerDef extends UntypedReducerDef>(
  reducerDef: ReducerDef
): (...params: ParamsType<ReducerDef>) => void {
  const connectionStore = useSpacetimeDB();
  const reducerName = reducerDef.accessorName;

  // Holds calls made before the connection exists
  const queueRef: ParamsType<ReducerDef>[] = [];

  // Flush when we finally have a connection
  const unsubscribe = connectionStore.subscribe(state => {
    const conn = state.getConnection();
    if (!conn) return;

    const fn = (conn.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDef>
    ) => void;
    if (queueRef.length) {
      const pending = queueRef.splice(0);
      for (const params of pending) {
        fn(...params);
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
      queueRef.push(params);
      return;
    }
    const fn = (conn.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDef>
    ) => void;
    return fn(...params);
  };
}
