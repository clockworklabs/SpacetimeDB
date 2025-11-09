import { useCallback, useEffect, useRef } from 'react';
import type { InferTypeOfRow } from '../lib/type_builders';
import type { UntypedReducerDef } from '../sdk/reducers';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { Prettify } from '../lib/type_util';

export function useReducer<ReducerDef extends UntypedReducerDef>(
  reducerDef: ReducerDef
): (params: Prettify<InferTypeOfRow<ReducerDef['params']>>) => void {
  const { getConnection, isActive } = useSpacetimeDB();
  const reducerName = reducerDef.accessorName;

  // Holds calls made before the connection exists
  const queueRef = useRef<
    Array<Prettify<InferTypeOfRow<ReducerDef['params']>>>
  >([]);

  // Flush when we finally have a connection
  useEffect(() => {
    const conn = getConnection();
    if (!conn) {
      return;
    }
    const fn = (conn.reducers as any)[reducerName] as (
      p: InferTypeOfRow<ReducerDef['params']>
    ) => void;
    if (queueRef.current.length) {
      const pending = queueRef.current.splice(0);
      for (const params of pending) fn(params);
    }
  }, [getConnection, reducerName, isActive]);

  return useCallback(
    (params: Prettify<InferTypeOfRow<ReducerDef['params']>>) => {
      const conn = getConnection();
      if (!conn) {
        queueRef.current.push(params);
        return;
      }
      const fn = (conn.reducers as any)[reducerName] as (
        p: Prettify<InferTypeOfRow<ReducerDef['params']>>
      ) => void;
      return fn(params);
    },
    [getConnection, reducerName]
  );
}
