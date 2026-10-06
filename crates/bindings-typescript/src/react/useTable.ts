import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { useSpacetimeDB } from './useSpacetimeDB.ts';
import { type EventContextInterface } from '../sdk/db_connection_impl.ts';
import type { ConnectionState } from './connection_state.ts';
import type { UntypedRemoteModule } from '../sdk/spacetime_module.ts';
import type { RowType, UntypedTableDef } from '../lib/table.ts';
import type { Prettify } from '../lib/type_util.ts';
import {
  type Query,
  toSql,
  type BooleanExpr,
  evaluateBooleanExpr,
  getQueryAccessorName,
  getQueryWhereClause,
} from '../lib/query.ts';

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
 * Accepts a query builder expression as the first argument:
 * - `tables.user` — subscribe to all rows
 * - `tables.user.where(r => r.online.eq(true))` — subscribe with a filter
 *
 * @param query - A query builder expression (table reference or filtered query).
 * @param callbacks - Optional callbacks for row insert, delete, and update events.
 * @returns A tuple of [rows, isReady].
 *
 * @example
 * ```tsx
 * const [rows, isReady] = useTable(tables.user);
 * const [onlineUsers, isReady] = useTable(
 *   tables.user.where(r => r.online.eq(true)),
 *   { onInsert: (row) => console.log('New user:', row) }
 * );
 * ```
 */
export function useTable<TableDef extends UntypedTableDef>(
  query: Query<TableDef>,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [readonly Prettify<RowType<TableDef>>[], boolean] {
  type UseTableRowType = RowType<TableDef>;
  const enabled = callbacks?.enabled ?? true;
  const accessorName = getQueryAccessorName(query);
  const whereExpr = getQueryWhereClause(query);

  const [appliedConnectionId, setAppliedConnectionId] = useState<string | null>(
    null
  );
  let connectionState: ConnectionState | undefined;
  try {
    connectionState = useSpacetimeDB();
  } catch {
    throw new Error(
      'Could not find SpacetimeDB client! Did you forget to add a ' +
        '`SpacetimeDBProvider`? `useTable` must be used in the React component tree ' +
        'under a `SpacetimeDBProvider` component.'
    );
  }

  const querySql = toSql(query);
  const connection = connectionState.getConnection();

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
    if (!connection) {
      return [[], false];
    }
    const table = connection.db[accessorName];
    const result: readonly Prettify<UseTableRowType>[] = whereExpr
      ? (Array.from(table.iter()).filter(row =>
          evaluateBooleanExpr(whereExpr, row as Record<string, any>)
        ) as Prettify<UseTableRowType>[])
      : (Array.from(table.iter()) as Prettify<UseTableRowType>[]);
    return [
      result,
      connectionState.isActive &&
        appliedConnectionId === connectionState.connectionId.toHexString(),
    ];
    // TODO: investigating refactoring so that this is no longer necessary, as we have had genuine bugs with missed deps.
    // See https://github.com/clockworklabs/SpacetimeDB/pull/4580.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    connection,
    connectionState,
    accessorName,
    querySql,
    appliedConnectionId,
    enabled,
  ]);

  const lastComputeSnapshotRef = useRef(computeSnapshot);

  useEffect(() => {
    setAppliedConnectionId(null);
    if (!enabled || !connection) return;

    const handle = connection
      .subscriptionBuilder()
      .onApplied(() => {
        setAppliedConnectionId(connection.connectionId.toHexString());
      })
      .onError(() => setAppliedConnectionId(null))
      .subscribe(querySql);
    return () => handle.unsubscribe();
  }, [querySql, connection, enabled]);

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
            return; // no-op
        }

        if (ctx.event.id !== latestTransactionEventId.current) {
          latestTransactionEventId.current = ctx.event.id;
          lastSnapshotRef.current = computeSnapshot();
          onStoreChange();
        }
      };

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
      connection,
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
    if (lastComputeSnapshotRef.current !== computeSnapshot) {
      lastComputeSnapshotRef.current = computeSnapshot;
      lastSnapshotRef.current = null;
    }
    if (!lastSnapshotRef.current) {
      lastSnapshotRef.current = computeSnapshot();
    }
    return lastSnapshotRef.current;
  }, [computeSnapshot]);

  // SSR fallback can be the same getter
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
