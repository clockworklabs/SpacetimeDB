import { useCallback, useEffect, useRef } from "react";
import type { InferTypeOfRow } from "../lib/type_builders";
import type { UntypedReducerDef } from "../sdk/reducers";
import { useSpacetimeDB, } from "./useSpacetimeDB";

export function useReducer<ReducerDef extends UntypedReducerDef>(
  reducerDef: ReducerDef,
): (params: InferTypeOfRow<ReducerDef['params']>) => void {
  const { getConnection, isActive } = useSpacetimeDB();
  const reducerName = reducerDef.accessorName;

  // Holds calls made before the connection exists
  const queueRef = useRef<
    Array<InferTypeOfRow<ReducerDef['params']>>
  >([]);

  // Flush when we finally have a connection
  useEffect(() => {
    const conn = getConnection();
    if (!conn) {
      return;
    }
    const fn =
      (conn.reducers as any)[reducerName] as
        (p: InferTypeOfRow<ReducerDef['params']>) => void;
    if (queueRef.current.length) {
      const pending = queueRef.current.splice(0);
      for (const params of pending) fn(params);
    }
  }, [getConnection, reducerName, isActive]);

  return useCallback((
    params: InferTypeOfRow<ReducerDef['params']>
  ) => {
    const conn = getConnection();
    if (!conn) {
      queueRef.current.push(params);
      return;
    }
    const fn =
      (conn.reducers as any)[reducerName] as
        (p: InferTypeOfRow<ReducerDef['params']>) => void;
    return fn(params);
  }, [getConnection, reducerName]);
}