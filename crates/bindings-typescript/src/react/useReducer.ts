import { useCallback, useEffect, useRef } from 'react';
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
): (...params: ParamsType<ReducerDef>) => Promise<void> {
  const { getConnection, isActive } = useSpacetimeDB();
  const reducerName = reducerDef.accessorName;

  // Holds calls made before the connection exists
  const queueRef = useRef<
    {
      params: ParamsType<ReducerDef>;
      resolve: () => void;
      reject: (err: unknown) => void;
    }[]
  >([]);

  // Flush when we finally have a connection
  useEffect(() => {
    const conn = getConnection();
    if (!conn) {
      return;
    }
    const fn = (conn.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDef>
    ) => Promise<void>;
    if (queueRef.current.length) {
      const pending = queueRef.current.splice(0);
      for (const item of pending) {
        fn(...item.params).then(item.resolve, item.reject);
      }
    }
  }, [getConnection, reducerName, isActive]);

  return useCallback(
    (...params: ParamsType<ReducerDef>) => {
      const conn = getConnection();
      if (!conn) {
        return new Promise<void>((resolve, reject) => {
          queueRef.current.push({ params, resolve, reject });
        });
      }
      const fn = (conn.reducers as any)[reducerName] as (
        ...p: ParamsType<ReducerDef>
      ) => Promise<void>;
      return fn(...params);
    },
    [getConnection, reducerName]
  );
}
