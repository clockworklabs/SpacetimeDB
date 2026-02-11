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
  type Value,
  type Expr,
  type ColumnsFromRow,
  eq,
  and,
  or,
  isEq,
  isAnd,
  isOr,
  evaluate,
  toString,
  where,
  classifyMembership,
} from '../lib/filter';

export { eq, and, or, isEq, isAnd, isOr, where };
export type { Value, Expr };

export interface UseTableCallbacks<RowType> {
  onInsert?: (row: RowType) => void;
  onDelete?: (row: RowType) => void;
  onUpdate?: (oldRow: RowType, newRow: RowType) => void;
}

/**
 * React hook to subscribe to a table in SpacetimeDB and receive live updates as rows are inserted, updated, or deleted.
 *
 * This hook returns a snapshot of the table's rows, filtered by an optional `where` clause, and provides a loading state
 * until the initial subscription is applied. It also allows you to specify callbacks for row insertions, deletions, and updates.
 *
 * The hook must be used within a component tree wrapped by `SpacetimeDBProvider`.
 *
 * Overloads:
 * - `useTable(tableName, where, callbacks?)`: Subscribe to a table with a filter and optional callbacks.
 * - `useTable(tableName, callbacks?)`: Subscribe to a table without a filter, with optional callbacks.
 *
 * @template DbConnection The type of the SpacetimeDB connection.
 * @template RowType The type of the table row.
 * @template TableName The name of the table.
 *
 * @param tableName - The name of the table to subscribe to.
 * @param whereClauseOrCallbacks - (Optional) Either a filter expression (where clause) or the callbacks object.
 * @param callbacks - (Optional) Callbacks for row insert, delete, and update events.
 *
 * @returns A snapshot object containing the current rows and the subscription state (`'loading'` or `'ready'`).
 *
 * @throws Error if the hook is used outside of a `SpacetimeDBProvider`.
 *
 * @example
 * ```tsx
 * const { rows, state } = useTable('users', where(eq('isActive', true)), {
 *   onInsert: (row) => console.log('Inserted:', row),
 *   onDelete: (row) => console.log('Deleted:', row),
 *   onUpdate: (oldRow, newRow) => console.log('Updated:', oldRow, newRow),
 * });
 * ```
 */
export function useTable<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  where: Expr<ColumnsFromRow<RowType<TableDef>>>,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [readonly Prettify<RowType<TableDef>>[], boolean];

/**
 * React hook to subscribe to a table in SpacetimeDB and receive live updates as rows are inserted, updated, or deleted.
 *
 * This hook returns a snapshot of the table's rows, filtered by an optional `where` clause, and provides a loading state
 * until the initial subscription is applied. It also allows you to specify callbacks for row insertions, deletions, and updates.
 *
 * The hook must be used within a component tree wrapped by `SpacetimeDBProvider`.
 *
 * Overloads:
 * - `useTable(tableName, where, callbacks?)`: Subscribe to a table with a filter and optional callbacks.
 * - `useTable(tableName, callbacks?)`: Subscribe to a table without a filter, with optional callbacks.
 *
 * @template DbConnection The type of the SpacetimeDB connection.
 * @template RowType The type of the table row.
 * @template TableName The name of the table.
 *
 * @param tableName - The name of the table to subscribe to.
 * @param whereClauseOrCallbacks - (Optional) Either a filter expression (where clause) or the callbacks object.
 * @param callbacks - (Optional) Callbacks for row insert, delete, and update events.
 *
 * @returns A snapshot object containing the current rows and the subscription state (`'loading'` or `'ready'`).
 *
 * @throws Error if the hook is used outside of a `SpacetimeDBProvider`.
 *
 * @example
 * ```tsx
 * const { rows, state } = useTable('users', where(eq('isActive', true)), {
 *   onInsert: (row) => console.log('Inserted:', row),
 *   onDelete: (row) => console.log('Deleted:', row),
 *   onUpdate: (oldRow, newRow) => console.log('Updated:', oldRow, newRow),
 * });
 * ```
 */
export function useTable<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [readonly Prettify<RowType<TableDef>>[], boolean];

export function useTable<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  whereClauseOrCallbacks?:
    | Expr<ColumnsFromRow<RowType<TableDef>>>
    | UseTableCallbacks<RowType<TableDef>>,
  callbacks?: UseTableCallbacks<RowType<TableDef>>
): [readonly Prettify<RowType<TableDef>>[], boolean] {
  type UseTableRowType = RowType<TableDef>;
  const tableName = tableDef.sourceName;
  const accessorName = tableDef.accessorName;
  let whereClause: Expr<ColumnsFromRow<UseTableRowType>> | undefined;
  if (
    whereClauseOrCallbacks &&
    typeof whereClauseOrCallbacks === 'object' &&
    'type' in whereClauseOrCallbacks
  ) {
    whereClause = whereClauseOrCallbacks as Expr<
      ColumnsFromRow<UseTableRowType>
    >;
  } else {
    callbacks = whereClauseOrCallbacks as
      | UseTableCallbacks<UseTableRowType>
      | undefined;
  }
  const [subscribeApplied, setSubscribeApplied] = useState(false);
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

  const query =
    `SELECT * FROM ${tableName}` +
    (whereClause ? ` WHERE ${toString(tableDef, whereClause)}` : '');

  const latestTransactionEvent = useRef<any>(null);
  const lastSnapshotRef = useRef<
    [readonly Prettify<UseTableRowType>[], boolean] | null
  >(null);

  const whereKey = whereClause ? toString(tableDef, whereClause) : '';

  const computeSnapshot = useCallback((): [
    readonly Prettify<UseTableRowType>[],
    boolean,
  ] => {
    const connection = connectionState.getConnection();
    if (!connection) {
      return [[], false];
    }
    const table = connection.db[accessorName];
    const result: readonly Prettify<UseTableRowType>[] = whereClause
      ? (Array.from(table.iter()).filter(row =>
          evaluate(whereClause, row as UseTableRowType)
        ) as Prettify<UseTableRowType>[])
      : (Array.from(table.iter()) as Prettify<UseTableRowType>[]);
    return [result, subscribeApplied];
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connectionState, accessorName, whereKey, subscribeApplied]);

  useEffect(() => {
    const connection = connectionState.getConnection()!;
    if (connectionState.isActive && connection) {
      const cancel = connection
        .subscriptionBuilder()
        .onApplied(() => {
          setSubscribeApplied(true);
        })
        .subscribe(query);
      return () => {
        cancel.unsubscribe();
      };
    }
  }, [query, connectionState.isActive, connectionState]);

  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      const onInsert = (
        ctx: EventContextInterface<UntypedRemoteModule>,
        row: any
      ) => {
        if (whereClause && !evaluate(whereClause, row)) {
          return;
        }
        callbacks?.onInsert?.(row);
        if (
          ctx.event !== latestTransactionEvent.current ||
          !latestTransactionEvent.current
        ) {
          latestTransactionEvent.current = ctx.event;
          lastSnapshotRef.current = computeSnapshot();
          onStoreChange();
        }
      };

      const onDelete = (
        ctx: EventContextInterface<UntypedRemoteModule>,
        row: any
      ) => {
        if (whereClause && !evaluate(whereClause, row)) {
          return;
        }
        callbacks?.onDelete?.(row);
        if (
          ctx.event !== latestTransactionEvent.current ||
          !latestTransactionEvent.current
        ) {
          latestTransactionEvent.current = ctx.event;
          lastSnapshotRef.current = computeSnapshot();
          onStoreChange();
        }
      };

      const onUpdate = (
        ctx: EventContextInterface<UntypedRemoteModule>,
        oldRow: any,
        newRow: any
      ) => {
        const change = classifyMembership(whereClause, oldRow, newRow);

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

        if (
          ctx.event !== latestTransactionEvent.current ||
          !latestTransactionEvent.current
        ) {
          latestTransactionEvent.current = ctx.event;
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [
      connectionState,
      accessorName,
      whereKey,
      callbacks?.onDelete,
      callbacks?.onInsert,
      callbacks?.onUpdate,
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

  // SSR fallback can be the same getter
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
