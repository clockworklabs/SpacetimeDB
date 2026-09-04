import { useCallback, useEffect, useRef } from 'react';
import type { UntypedReducerDef } from '../sdk/reducers';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { ParamsType } from '../sdk';
import type { ConnectionState } from './connection_state';

/**
 * React hook returning a reducer-call function.
 *
 * - `useReducer(reducerDef)` — reads from the nearest `<SpacetimeDBProvider>`.
 * - `useReducer(key, reducerDef)` — reads the connection labelled `key` from
 *   the nearest `<SpacetimeDBMultiProvider>`.
 *
 * Returns `(...args) => Promise<void>`. Calls made before the connection is
 * live are queued and flushed on `onApplied`.
 */
export function useReducer<ReducerDef extends UntypedReducerDef>(
  reducerDef: ReducerDef
): (...params: ParamsType<ReducerDef>) => Promise<void>;
export function useReducer<ReducerDef extends UntypedReducerDef>(
  key: string,
  reducerDef: ReducerDef
): (...params: ParamsType<ReducerDef>) => Promise<void>;
export function useReducer<ReducerDef extends UntypedReducerDef>(
  keyOrDef: string | ReducerDef,
  maybeDef?: ReducerDef
): (...params: ParamsType<ReducerDef>) => Promise<void> {
  const keyed = typeof keyOrDef === 'string';
  const key: string | undefined = keyed ? (keyOrDef as string) : undefined;
  const reducerDef: ReducerDef = keyed
    ? (maybeDef as ReducerDef)
    : (keyOrDef as ReducerDef);

  const connectionState: ConnectionState = useSpacetimeDB(key);
  return useReducerInternal<ReducerDef>(connectionState, reducerDef);
}

function useReducerInternal<ReducerDef extends UntypedReducerDef>(
  connectionState: ConnectionState,
  reducerDef: ReducerDef
): (...params: ParamsType<ReducerDef>) => Promise<void> {
  const { getConnection, isActive } = connectionState;
  const reducerName = reducerDef.accessorName;

  const queueRef = useRef<
    {
      params: ParamsType<ReducerDef>;
      resolve: () => void;
      reject: (err: unknown) => void;
    }[]
  >([]);

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
