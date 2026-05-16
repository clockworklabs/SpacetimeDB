import { createSignal, createEffect, onCleanup } from 'solid-js';
import { useSpacetimeDB } from './useSpacetimeDB';
import { type EventContextInterface } from '../sdk/db_connection_impl';
import type { UntypedRemoteModule } from '../sdk/spacetime_module';
import type { RowType, UntypedTableDef } from '../lib/table';
import type { Prettify } from '../lib/type_util';
import {
  type Query,
  type BooleanExpr,
  toSql,
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
 * SolidJS primitive to subscribe to a table in SpacetimeDB and receive live updates.
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
  const querySql = toSql(query);

  let connectionState: import('./connection_state').ConnectionState;
  try {
    connectionState = useSpacetimeDB();
  } catch {
    throw new Error(
      'Could not find SpacetimeDB client! Did you forget to add a ' +
        '`SpacetimeDBProvider`? `useTable` must be used in the SolidJS component tree ' +
        'under a `SpacetimeDBProvider` component.'
    );
  }

  const [rows, setRows] = createSignal<readonly Prettify<UseTableRowType>[]>(
    [],
    { equals: false }
  );
  const [isReady, setIsReady] = createSignal(false);

  let latestTransactionEventId: string | null = null;

  const computeSnapshot = (): readonly Prettify<UseTableRowType>[] => {
    if (!enabled) {
      return [];
    }
    const connection = connectionState.getConnection();
    if (!connection) {
      return [];
    }
    const table = connection.db[accessorName];
    const result: readonly Prettify<UseTableRowType>[] = whereExpr
      ? (Array.from(table.iter()).filter(row =>
          evaluateBooleanExpr(whereExpr, row as Record<string, any>)
        ) as Prettify<UseTableRowType>[])
      : (Array.from(table.iter()) as Prettify<UseTableRowType>[]);
    return result;
  };

  // Manage SQL subscription
  createEffect(() => {
    if (!enabled) {
      setIsReady(false);
      setRows([]);
      return;
    }

    const connection = connectionState.getConnection();
    if (!connectionState.isActive || !connection) return;

    const cancel = connection
      .subscriptionBuilder()
      .onApplied(() => {
        setIsReady(true);
        setRows(computeSnapshot());
      })
      .subscribe(querySql);

    onCleanup(() => {
      cancel.unsubscribe();
    });
  });

  // Bind to table events
  createEffect(() => {
    if (!enabled) return;

    const connection = connectionState.getConnection();
    if (!connectionState.isActive || !connection) return;

    const table = connection.db[accessorName];

    const onInsert = (
      ctx: EventContextInterface<UntypedRemoteModule>,
      row: any
    ) => {
      if (whereExpr && !evaluateBooleanExpr(whereExpr, row)) {
        return;
      }
      callbacks?.onInsert?.(row);
      if (ctx.event.id !== latestTransactionEventId) {
        latestTransactionEventId = ctx.event.id;
        setRows(computeSnapshot());
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
      if (ctx.event.id !== latestTransactionEventId) {
        latestTransactionEventId = ctx.event.id;
        setRows(computeSnapshot());
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

      if (ctx.event.id !== latestTransactionEventId) {
        latestTransactionEventId = ctx.event.id;
        setRows(computeSnapshot());
      }
    };

    table.onInsert(onInsert);
    table.onDelete(onDelete);
    table.onUpdate?.(onUpdate);

    // Load initial snapshot
    setRows(computeSnapshot());

    onCleanup(() => {
      table.removeOnInsert(onInsert);
      table.removeOnDelete(onDelete);
      table.removeOnUpdate?.(onUpdate);
    });
  });

  return [rows(), isReady()];
}
