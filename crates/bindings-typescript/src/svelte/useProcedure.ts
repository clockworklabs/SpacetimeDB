import { getByAccessorPath } from '../lib/accessor_path.ts';
import { onDestroy } from 'svelte';
import { get } from 'svelte/store';
import type { UntypedProcedureDef } from '../sdk/procedures.ts';
import { useSpacetimeDB } from './useSpacetimeDB.ts';
import type {
  ProcedureParamsType,
  ProcedureReturnType,
} from '../sdk/type_utils.ts';

export function useProcedure<ProcedureDef extends UntypedProcedureDef>(
  procedureDef: ProcedureDef
): (
  ...params: ProcedureParamsType<ProcedureDef>
) => Promise<ProcedureReturnType<ProcedureDef>> {
  const connectionStore = useSpacetimeDB();
  const procedureName = procedureDef.accessorName;

  // Holds calls made before the connection exists
  const queue: {
    params: ProcedureParamsType<ProcedureDef>;
    resolve: (val: ProcedureReturnType<ProcedureDef>) => void;
    reject: (err: unknown) => void;
  }[] = [];

  // Flush when we finally have a connection
  const unsubscribe = connectionStore.subscribe(state => {
    const conn = state.getConnection();
    if (!conn || !queue.length) return;

    const fn = getByAccessorPath(conn.procedures, procedureName) as (
      ...p: ProcedureParamsType<ProcedureDef>
    ) => Promise<ProcedureReturnType<ProcedureDef>>;
    for (const item of queue.splice(0)) {
      fn(...item.params).then(item.resolve, item.reject);
    }
  });

  onDestroy(() => {
    unsubscribe();
  });

  return (...params: ProcedureParamsType<ProcedureDef>) => {
    const conn = get(connectionStore).getConnection();
    if (!conn) {
      return new Promise<ProcedureReturnType<ProcedureDef>>(
        (resolve, reject) => {
          queue.push({ params, resolve, reject });
        }
      );
    }
    const fn = getByAccessorPath(conn.procedures, procedureName) as (
      ...p: ProcedureParamsType<ProcedureDef>
    ) => Promise<ProcedureReturnType<ProcedureDef>>;
    return fn(...params);
  };
}
