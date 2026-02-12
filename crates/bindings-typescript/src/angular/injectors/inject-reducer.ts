import { assertInInjectionContext, inject, effect } from '@angular/core';
import { SPACETIMEDB_CONNECTION } from '../connection_state';
import type { ParamsType } from '../../sdk';
import type { UntypedReducerDef } from '../../sdk/reducers';

export function injectReducer<ReducerDef extends UntypedReducerDef>(
  reducerDef: ReducerDef
): (...params: ParamsType<ReducerDef>) => void {
  assertInInjectionContext(injectReducer);

  const connState = inject(SPACETIMEDB_CONNECTION);
  const queue: ParamsType<ReducerDef>[] = [];
  const reducerName = reducerDef.accessorName;

  // flush queued calls when connection becomes active
  effect(onCleanup => {
    const state = connState();
    if (!state.isActive) {
      return;
    }

    const connection = state.getConnection();
    if (!connection) {
      return;
    }

    const callReducer = (connection.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDef>
    ) => void;

    if (queue.length) {
      const pending = queue.splice(0);
      for (const params of pending) {
        callReducer(...params);
      }
    }

    onCleanup(() => {
      queue.splice(0);
    });
  });

  return (...params: ParamsType<ReducerDef>) => {
    const state = connState();
    if (!state.isActive) {
      queue.push(params);
      return;
    }

    const connection = state.getConnection();
    if (!connection) {
      queue.push(params);
      return;
    }

    const callReducer = (connection.reducers as any)[reducerName] as (
      ...p: ParamsType<ReducerDef>
    ) => void;

    return callReducer(...params);
  };
}
