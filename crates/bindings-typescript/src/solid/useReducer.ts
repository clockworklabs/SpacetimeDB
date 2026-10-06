import { createEffect } from 'solid-js';
import type { UntypedReducerDecl } from '../sdk/reducers';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { ParamsType } from '../sdk';

export function useReducer<ReducerDecl extends UntypedReducerDecl>(
  reducerDef: ReducerDecl
): (...params: ParamsType<ReducerDecl>) => Promise<void> {
  const { getConnection, isActive } = useSpacetimeDB();
  const reducerName = reducerDef.accessorName;

  // Holds calls made before the connection exists
  const queue: {
    params: ParamsType<ReducerDecl>;
    resolve: () => void;
    reject: (err: unknown) => void;
  }[] = [];

  // Flush when we finally have a connection
  createEffect(() => {
    if (!isActive) return;

    const conn = getConnection();
    if (!conn) return;

    const fn = (conn.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDecl>
    ) => Promise<void>;

    if (queue.length) {
      const pending = queue.splice(0);
      for (const item of pending) {
        fn(...item.params).then(item.resolve, item.reject);
      }
    }
  });

  return (...params: ParamsType<ReducerDecl>) => {
    const conn = getConnection();
    if (!conn) {
      return new Promise<void>((resolve, reject) => {
        queue.push({ params, resolve, reject });
      });
    }
    const fn = (conn.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDecl>
    ) => Promise<void>;
    return fn(...params);
  };
}
