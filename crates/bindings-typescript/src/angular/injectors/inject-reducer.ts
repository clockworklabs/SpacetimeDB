import type { ParamsType } from '../../sdk';
import type { UntypedReducerDef } from '../../sdk/reducers';
import { injectSpacetimeDB } from './inject-spacetimedb';
import { injectSpacetimeDBConnected } from './inject-spacetimedb-connected';
import { DestroyRef, effect, inject } from '@angular/core';

export function injectReducer<ReducerDef extends UntypedReducerDef>(
  reducerDef: ReducerDef
) {
  const conn = injectSpacetimeDB();
  const isActive = injectSpacetimeDBConnected();
  const destroyRef = inject(DestroyRef);

  const queue: ParamsType<ReducerDef>[] = [];
  const reducerName = reducerDef.accessorName;

  effect(() => {
    if (!isActive()) {
      return;
    }

    const callReducer = (conn.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDef>
    ) => void;

    if (queue.length) {
      const pending = queue.splice(0);
      for (const params of pending) {
        callReducer(...params);
      }
    }
  });

  destroyRef.onDestroy(() => {
    queue.splice(0);
  });

  return (...params: ParamsType<ReducerDef>) => {
    if (!isActive()) {
      queue.push(params);
      return;
    }

    const callReducer = (conn.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDef>
    ) => void;

    return callReducer(...params);
  };
}
