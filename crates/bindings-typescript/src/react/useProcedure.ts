import { useCallback, useEffect, useRef } from 'react';
import type { UntypedProcedureDef } from '../sdk/procedures';
import { useSpacetimeDB } from './useSpacetimeDB';
import type {
  ProcedureParamsType,
  ProcedureReturnType,
} from '../sdk/type_utils';

export function useProcedure<ProcedureDef extends UntypedProcedureDef>(
  procedureDef: ProcedureDef
): (
  ...params: ProcedureParamsType<ProcedureDef>
) => Promise<ProcedureReturnType<ProcedureDef>> {
  const { getConnection, isActive } = useSpacetimeDB();
  const procedureName = procedureDef.accessorName;

  // Holds calls made before the connection exists
  const queueRef = useRef<
    {
      params: ProcedureParamsType<ProcedureDef>;
      resolve: (val: any) => void;
      reject: (err: unknown) => void;
    }[]
  >([]);

  // Flush when we finally have a connection
  useEffect(() => {
    const conn = getConnection();
    if (!conn) {
      return;
    }
    const fn = (conn.procedures as any)[procedureName] as (
      ...p: ProcedureParamsType<ProcedureDef>
    ) => Promise<ProcedureReturnType<ProcedureDef>>;
    if (queueRef.current.length) {
      const pending = queueRef.current.splice(0);
      for (const item of pending) {
        fn(...item.params).then(item.resolve, item.reject);
      }
    }
  }, [getConnection, procedureName, isActive]);

  return useCallback(
    (...params: ProcedureParamsType<ProcedureDef>) => {
      const conn = getConnection();
      if (!conn) {
        return new Promise<ProcedureReturnType<ProcedureDef>>(
          (resolve, reject) => {
            queueRef.current.push({ params, resolve, reject });
          }
        );
      }
      const fn = (conn.procedures as any)[procedureName] as (
        ...p: ProcedureParamsType<ProcedureDef>
      ) => Promise<ProcedureReturnType<ProcedureDef>>;
      return fn(...params);
    },
    [getConnection, procedureName]
  );
}
