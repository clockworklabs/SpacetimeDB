import { useCallback, useEffect, useRef } from 'react';
import type { InferTypeOfRow } from '../lib/type_builders';
import type { UntypedReducerDef } from '../sdk/reducers';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { Prettify } from '../lib/type_util';

type IsEmptyObject<T> = [keyof T] extends [never] ? true : false;
type MaybeParams<T> = IsEmptyObject<T> extends true ? [] : [params: T];

type ParamsType<R extends UntypedReducerDef> = MaybeParams<Prettify<
  InferTypeOfRow<R['params']>
>>;

export function useReducer<ReducerDef extends UntypedReducerDef>(
  reducerDef: ReducerDef
): (...params: ParamsType<ReducerDef>) => void {
  const { getConnection, isActive } = useSpacetimeDB();
  const reducerName = reducerDef.accessorName;

  // Holds calls made before the connection exists
  const queueRef = useRef<
    ParamsType<ReducerDef>[]
  >([]);

  // Flush when we finally have a connection
  useEffect(() => {
    const conn = getConnection();
    if (!conn) {
      return;
    }
    const fn = (conn.reducers as any)[reducerName] as (
      p: ParamsType<ReducerDef>
    ) => void;
    if (queueRef.current.length) {
      const pending = queueRef.current.splice(0);
      for (const params of pending) {
        fn(params);
      }
    }
  }, [getConnection, reducerName, isActive]);

  return useCallback(
    (...params: ParamsType<ReducerDef>) => {
      const conn = getConnection();
      if (!conn) {
        queueRef.current.push(params);
        return;
      }
      const fn = (conn.reducers as any)[reducerName] as (
        p: ParamsType<ReducerDef>
      ) => void;
      return fn(params);
    },
    [getConnection, reducerName]
  );
}
