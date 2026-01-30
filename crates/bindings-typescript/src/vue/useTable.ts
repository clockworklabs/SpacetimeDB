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

export function useTable<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  where: Expr<ColumnsFromRow<RowType<TableDef>>>,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [
  DeepReadonly<Ref<readonly Prettify<RowType<TableDef>>[]>>,
  DeepReadonly<Ref<boolean>>,
];

export function useTable<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [
  DeepReadonly<Ref<readonly Prettify<RowType<TableDef>>[]>>,
  DeepReadonly<Ref<boolean>>,
];

export function useTable<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  whereClauseOrCallbacks?:
    | Expr<ColumnsFromRow<RowType<TableDef>>>
    | UseTableCallbacks<RowType<TableDef>>,
  callbacks?: UseTableCallbacks<RowType<TableDef>>
): [
  DeepReadonly<Ref<readonly Prettify<RowType<TableDef>>[]>>,
  DeepReadonly<Ref<boolean>>,
] {
  type Row = RowType<TableDef>;
  const tableName = tableDef.name;
  const accessorName = tableDef.accessorName;

  let whereClause: Expr<ColumnsFromRow<Row>> | undefined;
  if (
    whereClauseOrCallbacks &&
    typeof whereClauseOrCallbacks === 'object' &&
    'type' in whereClauseOrCallbacks
  ) {
    whereClause = whereClauseOrCallbacks as Expr<ColumnsFromRow<Row>>;
  } else {
    callbacks = whereClauseOrCallbacks as UseTableCallbacks<Row> | undefined;
  }

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

  const query =
    `SELECT * FROM ${tableName}` +
    (whereClause ? ` WHERE ${toString(tableDef, whereClause)}` : '');

  let latestTransactionEvent: any = null;
  let unsubscribeFromTable: (() => void) | null = null;
  let subscriptionHandle: { unsubscribe: () => void } | null = null;

  const computeFilteredRows = (): readonly Prettify<Row>[] => {
    const connection = conn.getConnection();
    if (!connection) return [];

    const table = connection.db[accessorName];
    if (!table) return [];

    const allRows = Array.from(table.iter()) as Row[];
    if (whereClause) {
      return allRows.filter(row =>
        evaluate(whereClause, row as Record<string, any>)
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
      if (whereClause && !evaluate(whereClause, row)) return;
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
      if (whereClause && !evaluate(whereClause, row)) return;
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
      .subscribe(query);
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
