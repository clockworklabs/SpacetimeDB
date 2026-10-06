import { onDestroy } from 'svelte';
import { get } from 'svelte/store';
import type { InferTypeOfParams } from '../lib/type_builders';
import type { UntypedReducerDecl } from '../sdk/reducers';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { Prettify } from '../lib/type_util';

type IsEmptyObject<T> = [keyof T] extends [never] ? true : false;
type MaybeParams<T> = IsEmptyObject<T> extends true ? [] : [params: T];

type ParamsType<R extends UntypedReducerDecl> = MaybeParams<
  Prettify<InferTypeOfParams<R['params']>>
>;

export function useReducer<ReducerDecl extends UntypedReducerDecl>(
  reducerDef: ReducerDecl
): (...params: ParamsType<ReducerDecl>) => Promise<void> {
  const connectionStore = useSpacetimeDB();
  const reducerName = reducerDef.accessorName;

  // Holds calls made before the connection exists
  const queueRef: {
    params: ParamsType<ReducerDecl>;
    resolve: () => void;
    reject: (err: unknown) => void;
  }[] = [];

  // Flush when we finally have a connection
  const unsubscribe = connectionStore.subscribe(state => {
    const conn = state.getConnection();
    if (!conn) return;

    const fn = (conn.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDecl>
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

  return (...params: ParamsType<ReducerDecl>) => {
    const state = get(connectionStore);
    const conn = state.getConnection();
    if (!conn) {
      return new Promise<void>((resolve, reject) => {
        queueRef.push({ params, resolve, reject });
      });
    }
    const fn = (conn.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDecl>
    ) => Promise<void>;
    return fn(...params);
  };
}
