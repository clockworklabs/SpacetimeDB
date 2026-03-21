import { createEffect } from 'solid-js';
import type { UntypedReducerDef } from '../sdk/reducers';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { ParamsType } from '../sdk';

export function useReducer<ReducerDef extends UntypedReducerDef>(
  reducerDef: ReducerDef
): (...params: ParamsType<ReducerDef>) => Promise<void> {
  const { getConnection, isActive } = useSpacetimeDB();
  const reducerName = reducerDef.accessorName;

  // Queue for calls before connection is ready
  const queue: {
    params: ParamsType<ReducerDef>;
    resolve: () => void;
    reject: (err: unknown) => void;
  }[] = [];

  // Flush queue when connection becomes available
  createEffect(() => {
    if (!isActive) return;

    const conn = getConnection();
    if (!conn) return;

    const fn = (conn.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDef>
    ) => Promise<void>;

    if (queue.length) {
      const pending = queue.splice(0);
      for (const item of pending) {
        fn(...item.params).then(item.resolve, item.reject);
      }
    }
  });

  // Returned reducer caller
  return (...params: ParamsType<ReducerDef>) => {
    const conn = getConnection();

    if (!conn) {
      return new Promise<void>((resolve, reject) => {
        queue.push({ params, resolve, reject });
      });
    }

    const fn = (conn.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDef>
    ) => Promise<void>;

    return fn(...params);
  };
}