import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { useSpacetimeDB } from './useSpacetimeDB';
import { type EventContextInterface } from '../sdk/db_connection_impl';
import type { ConnectionState } from './connection_state';
import type { UntypedRemoteModule } from '../sdk/spacetime_module';
import type { RowType, UntypedTableDef } from '../lib/table';
import type { Prettify } from '../lib/type_util';
import {
  type Query,
  toSql,
  type BooleanExpr,
  evaluateBooleanExpr,
  getQueryAccessorName,
  getQueryWhereClause,
} from '../lib/query';

export interface UseTableCallbacks<RowType> {
  onInsert?: (row: RowType) => void;
  onDelete?: (row: RowType) => void;
  onUpdate?: (oldRow: RowType, newRow: RowType) => void;
  /** Whether the subscription is active. Defaults to `true`. */
  enabled?: boolean;
}

type MembershipChange = 'enter' | 'leave' | 'stayIn' | 'stayOut';

function classifyMembership(
  whereExpr: BooleanExpr<any> | undefined,
  oldRow: Record<string, any>,
  newRow: Record<string, any>
): MembershipChange {
  if (!whereExpr) return 'stayIn';
  const oldIn = evaluateBooleanExpr(whereExpr, oldRow);
  const newIn = evaluateBooleanExpr(whereExpr, newRow);
  if (oldIn && !newIn) return 'leave';
  if (!oldIn && newIn) return 'enter';
  if (oldIn && newIn) return 'stayIn';
  return 'stayOut';
}

/**
 * React hook to subscribe to a table in SpacetimeDB and receive live updates.
 *
 * - `useTable(query, callbacks?)` — reads from the nearest `<SpacetimeDBProvider>`.
 * - `useTable(key, query, callbacks?)` — reads the connection labelled `key`
 *   from the nearest `<SpacetimeDBMultiProvider>`.
 *
 * The query argument accepts either a table reference (`tables.user`) or a
 * filtered query (`tables.user.where(r => r.online.eq(true))`).
 *
 * @returns A tuple of [rows, isReady].
 *
 * @example
 * ```tsx
 * // Single-module app
 * const [rows, isReady] = useTable(tables.user);
 *
 * // Multi-module app
 * const [apps] = useTable('launcher', tables.app);
 * const [users] = useTable('admin', tables.user);
 * ```
 */
export function useTable<TableDef extends UntypedTableDef>(
  query: Query<TableDef>,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [readonly Prettify<RowType<TableDef>>[], boolean];
export function useTable<TableDef extends UntypedTableDef>(
  key: string,
  query: Query<TableDef>,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [readonly Prettify<RowType<TableDef>>[], boolean];
export function useTable<TableDef extends UntypedTableDef>(
  queryOrKey: Query<TableDef> | string,
  queryOrCallbacks?:
    | Query<TableDef>
    | UseTableCallbacks<Prettify<RowType<TableDef>>>,
  maybeCallbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [readonly Prettify<RowType<TableDef>>[], boolean] {
  const keyed = typeof queryOrKey === 'string';
  const key: string | undefined = keyed ? (queryOrKey as string) : undefined;
  const query: Query<TableDef> = keyed
    ? (queryOrCallbacks as Query<TableDef>)
    : (queryOrKey as Query<TableDef>);
  const callbacks: UseTableCallbacks<Prettify<RowType<TableDef>>> | undefined =
    keyed
      ? maybeCallbacks
      : (queryOrCallbacks as
          | UseTableCallbacks<Prettify<RowType<TableDef>>>
          | undefined);

  const connectionState: ConnectionState = useSpacetimeDB(key);
  return useTableInternal<TableDef>(connectionState, query, callbacks);
}

function useTableInternal<TableDef extends UntypedTableDef>(
  connectionState: ConnectionState,
  query: Query<TableDef>,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [readonly Prettify<RowType<TableDef>>[], boolean] {
  type UseTableRowType = RowType<TableDef>;
  const enabled = callbacks?.enabled ?? true;
  const accessorName = getQueryAccessorName(query);
  const whereExpr = getQueryWhereClause(query);

  const [subscribeApplied, setSubscribeApplied] = useState(false);

  const querySql = toSql(query);

  const latestTransactionEventId = useRef<string | null>(null);
  const lastSnapshotRef = useRef<
    [readonly Prettify<UseTableRowType>[], boolean] | null
  >(null);

  const computeSnapshot = useCallback((): [
    readonly Prettify<UseTableRowType>[],
    boolean,
  ] => {
    if (!enabled) {
      return [[], true];
    }
    const connection = connectionState.getConnection();
    if (!connection) {
      return [[], false];
    }
    const table = connection.db[accessorName];
    const result: readonly Prettify<UseTableRowType>[] = whereExpr
      ? (Array.from(table.iter()).filter(row =>
          evaluateBooleanExpr(whereExpr, row as Record<string, any>)
        ) as Prettify<UseTableRowType>[])
      : (Array.from(table.iter()) as Prettify<UseTableRowType>[]);
    return [result, subscribeApplied];
    // TODO: investigating refactoring so that this is no longer necessary, as we have had genuine bugs with missed deps.
    // See https://github.com/clockworklabs/SpacetimeDB/pull/4580.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connectionState, accessorName, querySql, subscribeApplied, enabled]);

  useEffect(() => {
    lastSnapshotRef.current = null;
  }, [computeSnapshot]);

  useEffect(() => {
    if (!enabled) {
      setSubscribeApplied(false);
      return;
    }
    const connection = connectionState.getConnection()!;
    if (connectionState.isActive && connection) {
      const cancel = connection
        .subscriptionBuilder()
        .onApplied(() => {
          setSubscribeApplied(true);
        })
        .subscribe(querySql);
      return () => {
        cancel.unsubscribe();
      };
    }
  }, [querySql, connectionState.isActive, connectionState, enabled]);

  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      if (!enabled) {
        return () => {};
      }

      const onInsert = (
        ctx: EventContextInterface<UntypedRemoteModule>,
        row: any
      ) => {
        if (whereExpr && !evaluateBooleanExpr(whereExpr, row)) {
          return;
        }
        callbacks?.onInsert?.(row);
        if (ctx.event.id !== latestTransactionEventId.current) {
          latestTransactionEventId.current = ctx.event.id;
          lastSnapshotRef.current = computeSnapshot();
          onStoreChange();
        }
      };

      const onDelete = (
        ctx: EventContextInterface<UntypedRemoteModule>,
        row: any
      ) => {
        if (whereExpr && !evaluateBooleanExpr(whereExpr, row)) {
          return;
        }
        callbacks?.onDelete?.(row);
        if (ctx.event.id !== latestTransactionEventId.current) {
          latestTransactionEventId.current = ctx.event.id;
          lastSnapshotRef.current = computeSnapshot();
          onStoreChange();
        }
      };

      const onUpdate = (
        ctx: EventContextInterface<UntypedRemoteModule>,
        oldRow: any,
        newRow: any
      ) => {
        const change = classifyMembership(whereExpr, oldRow, newRow);

        switch (change) {
          case 'leave':
            callbacks?.onDelete?.(oldRow);
            break;
          case 'enter':
            callbacks?.onInsert?.(newRow);
            break;
          case 'stayIn':
            callbacks?.onUpdate?.(oldRow, newRow);
            break;
          case 'stayOut':
            return;
        }

        if (ctx.event.id !== latestTransactionEventId.current) {
          latestTransactionEventId.current = ctx.event.id;
          lastSnapshotRef.current = computeSnapshot();
          onStoreChange();
        }
      };

      const connection = connectionState.getConnection();
      if (!connection) {
        return () => {};
      }

      const table = connection.db[accessorName];
      table.onInsert(onInsert);
      table.onDelete(onDelete);
      table.onUpdate?.(onUpdate);

      return () => {
        table.removeOnInsert(onInsert);
        table.removeOnDelete(onDelete);
        table.removeOnUpdate?.(onUpdate);
      };
    },
    // TODO: investigating refactoring so that this is no longer necessary, as we have had genuine bugs with missed deps.
    // See https://github.com/clockworklabs/SpacetimeDB/pull/4580.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [
      connectionState,
      accessorName,
      querySql,
      computeSnapshot,
      callbacks?.onDelete,
      callbacks?.onInsert,
      callbacks?.onUpdate,
      enabled,
    ]
  );

  const getSnapshot = useCallback((): [
    readonly Prettify<UseTableRowType>[],
    boolean,
  ] => {
    if (!lastSnapshotRef.current) {
      lastSnapshotRef.current = computeSnapshot();
    }
    return lastSnapshotRef.current;
  }, [computeSnapshot]);

  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
