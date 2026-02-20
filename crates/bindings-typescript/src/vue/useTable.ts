import {
  onUnmounted,
  readonly,
  ref,
  shallowRef,
  watch,
  type DeepReadonly,
  type Ref,
} from 'vue';
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
 * Vue composable to subscribe to a table in SpacetimeDB and receive live updates.
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
  query: { toSql(): string } & Record<string, any>,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [
  DeepReadonly<Ref<readonly Prettify<RowType<TableDef>>[]>>,
  DeepReadonly<Ref<boolean>>,
] {
  type Row = RowType<TableDef>;
  const accessorName = getQueryAccessorName(query);
  const whereExpr = getQueryWhereClause(query);
  const querySql = query.toSql();

  let conn;
  try {
    conn = useSpacetimeDB();
  } catch {
    throw new Error(
      'Could not find SpacetimeDB client! Did you forget to add a ' +
        '`SpacetimeDBProvider`? `useTable` must be used in a Vue component tree ' +
        'under a `SpacetimeDBProvider` component.'
    );
  }

  const rows = shallowRef<readonly Prettify<Row>[]>([]);
  const isReady = ref(false);

  let latestTransactionEvent: any = null;
  let unsubscribeFromTable: (() => void) | null = null;
  let subscriptionHandle: { unsubscribe: () => void } | null = null;

  const computeFilteredRows = (): readonly Prettify<Row>[] => {
    const connection = conn.getConnection();
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
    const connection = conn.getConnection();
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
        rows.value = computeFilteredRows();
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
        rows.value = computeFilteredRows();
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
        rows.value = computeFilteredRows();
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
    const connection = conn.getConnection();
    if (!connection) return;

    subscriptionHandle = connection
      .subscriptionBuilder()
      .onApplied(() => {
        isReady.value = true;
        rows.value = computeFilteredRows();
      })
      .subscribe(querySql);
  };

  watch(
    () => conn.isActive,
    isActive => {
      // Clean up existing listeners and subscriptions first
      if (unsubscribeFromTable) {
        unsubscribeFromTable();
        unsubscribeFromTable = null;
      }
      if (subscriptionHandle) {
        subscriptionHandle.unsubscribe();
        subscriptionHandle = null;
      }

      if (isActive) {
        unsubscribeFromTable = setupTableListeners() || null;
        setupSubscription();
        rows.value = computeFilteredRows();
      } else {
        isReady.value = false;
        rows.value = [];
      }
    },
    { immediate: true }
  );

  onUnmounted(() => {
    unsubscribeFromTable?.();
    subscriptionHandle?.unsubscribe();
    latestTransactionEvent = null;
  });

  return [readonly(rows), readonly(isReady)];
}
