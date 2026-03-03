import { onDestroy } from 'svelte';
import { writable, get, type Readable } from 'svelte/store';
import { useSpacetimeDB } from './useSpacetimeDB';
import type { EventContextInterface } from '../sdk/db_connection_impl';
import type { UntypedRemoteModule } from '../sdk/spacetime_module';
import type { RowType, UntypedTableDef } from '../lib/table';
import type { Prettify } from '../lib/type_util';
import {
  type BooleanExpr,
  evaluateBooleanExpr,
  getQueryAccessorName,
  getQueryWhereClause,
  type Query,
  toSql,
} from '../lib/query';

export interface UseTableCallbacks<RowType> {
  onInsert?: (row: RowType) => void;
  onDelete?: (row: RowType) => void;
  onUpdate?: (oldRow: RowType, newRow: RowType) => void;
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
 * Svelte hook to subscribe to a table in SpacetimeDB and receive live updates.
 *
 * Accepts a query builder expression as the first argument:
 * - `tables.user` — subscribe to all rows
 * - `tables.user.where(r => r.online.eq(true))` — subscribe with a filter
 *
 * @param query - A query builder expression (table reference or filtered query).
 * @param callbacks - Optional callbacks for row insert, delete, and update events.
 * @returns A tuple of [rows, isReady].
 */
export function useTable<TableDef extends UntypedTableDef>(
  query: Query<TableDef>,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [Readable<readonly Prettify<RowType<TableDef>>[]>, Readable<boolean>] {
  type Row = RowType<TableDef>;
  const accessorName = getQueryAccessorName(query);
  const whereExpr = getQueryWhereClause(query);
  const querySql = toSql(query);

  let connectionStore;
  try {
    connectionStore = useSpacetimeDB();
  } catch {
    throw new Error(
      'Could not find SpacetimeDB client! Did you forget to call ' +
        '`createSpacetimeDBProvider`? `useTable` must be used in a Svelte component tree ' +
        'under a component that called `createSpacetimeDBProvider`.'
    );
  }

  const rows = writable<readonly Prettify<Row>[]>([]);
  const isReady = writable(false);

  let latestTransactionEvent: any = null;
  let unsubscribeFromTable: (() => void) | null = null;
  let subscriptionHandle: { unsubscribe: () => void } | null = null;

  const computeFilteredRows = (): readonly Prettify<Row>[] => {
    const state = get(connectionStore);
    const connection = state.getConnection();
    if (!connection) return [];

    const table = connection.db[accessorName];
    if (!table) return [];

    const allRows = Array.from(table.iter()) as Row[];
    if (whereExpr) {
      return allRows.filter(row =>
        evaluateBooleanExpr(whereExpr, row as Record<string, any>)
      ) as Prettify<Row>[];
    }
    return allRows as Prettify<Row>[];
  };

  const setupTableListeners = () => {
    const state = get(connectionStore);
    const connection = state.getConnection();
    if (!connection) return;

    const table = connection.db[accessorName];
    if (!table) return;

    const onInsert = (
      eventCtx: EventContextInterface<UntypedRemoteModule>,
      row: any
    ) => {
      if (whereExpr && !evaluateBooleanExpr(whereExpr, row)) return;
      callbacks?.onInsert?.(row);

      if (
        eventCtx.event !== latestTransactionEvent ||
        !latestTransactionEvent
      ) {
        latestTransactionEvent = eventCtx.event;
        rows.set(computeFilteredRows());
      }
    };

    const onDelete = (
      eventCtx: EventContextInterface<UntypedRemoteModule>,
      row: any
    ) => {
      if (whereExpr && !evaluateBooleanExpr(whereExpr, row)) return;
      callbacks?.onDelete?.(row);

      if (
        eventCtx.event !== latestTransactionEvent ||
        !latestTransactionEvent
      ) {
        latestTransactionEvent = eventCtx.event;
        rows.set(computeFilteredRows());
      }
    };

    const onUpdate = (
      eventCtx: EventContextInterface<UntypedRemoteModule>,
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

      if (
        eventCtx.event !== latestTransactionEvent ||
        !latestTransactionEvent
      ) {
        latestTransactionEvent = eventCtx.event;
        rows.set(computeFilteredRows());
      }
    };

    table.onInsert(onInsert);
    table.onDelete(onDelete);
    table.onUpdate?.(onUpdate);

    return () => {
      table.removeOnInsert(onInsert);
      table.removeOnDelete(onDelete);
      table.removeOnUpdate?.(onUpdate);
    };
  };

  const setupSubscription = () => {
    const state = get(connectionStore);
    const connection = state.getConnection();
    if (!connection) return;

    subscriptionHandle = connection
      .subscriptionBuilder()
      .onApplied(() => {
        isReady.set(true);
        rows.set(computeFilteredRows());
      })
      .subscribe(querySql);
  };

  const unsubscribeConnection = connectionStore.subscribe(state => {
    // clean up existing listeners and subscriptions first
    if (unsubscribeFromTable) {
      unsubscribeFromTable();
      unsubscribeFromTable = null;
    }
    if (subscriptionHandle) {
      subscriptionHandle.unsubscribe();
      subscriptionHandle = null;
    }

    if (state.isActive) {
      unsubscribeFromTable = setupTableListeners() || null;
      setupSubscription();
      rows.set(computeFilteredRows());
    } else {
      isReady.set(false);
      rows.set([]);
    }
  });

  onDestroy(() => {
    unsubscribeConnection();
    unsubscribeFromTable?.();
    subscriptionHandle?.unsubscribe();
    latestTransactionEvent = null;
  });

  return [{ subscribe: rows.subscribe }, { subscribe: isReady.subscribe }];
}
