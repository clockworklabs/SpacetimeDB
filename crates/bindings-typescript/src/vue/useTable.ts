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

export interface UseTableCallbacks<RowType> {
  onInsert?: (row: RowType) => void;
  onDelete?: (row: RowType) => void;
  onUpdate?: (oldRow: RowType, newRow: RowType) => void;
}

export type Value = string | number | boolean;

export type Expr<Column extends string> =
  | { type: 'eq'; key: Column; value: Value }
  | { type: 'and'; children: Expr<Column>[] }
  | { type: 'or'; children: Expr<Column>[] };

export const eq = <Column extends string>(
  key: Column,
  value: Value
): Expr<Column> => ({ type: 'eq', key, value });

export const and = <Column extends string>(
  ...children: Expr<Column>[]
): Expr<Column> => {
  const flat: Expr<Column>[] = [];
  for (const c of children) {
    if (!c) continue;
    if (c.type === 'and') flat.push(...c.children);
    else flat.push(c);
  }
  const pruned = flat.filter(Boolean);
  if (pruned.length === 0) return { type: 'and', children: [] };
  if (pruned.length === 1) return pruned[0];
  return { type: 'and', children: pruned };
};

export const or = <Column extends string>(
  ...children: Expr<Column>[]
): Expr<Column> => {
  const flat: Expr<Column>[] = [];
  for (const c of children) {
    if (!c) continue;
    if (c.type === 'or') flat.push(...c.children);
    else flat.push(c);
  }
  const pruned = flat.filter(Boolean);
  if (pruned.length === 0) return { type: 'or', children: [] };
  if (pruned.length === 1) return pruned[0];
  return { type: 'or', children: pruned };
};

export const isEq = <Column extends string>(
  e: Expr<Column>
): e is Extract<Expr<Column>, { type: 'eq' }> => e.type === 'eq';
export const isAnd = <Column extends string>(
  e: Expr<Column>
): e is Extract<Expr<Column>, { type: 'and' }> => e.type === 'and';
export const isOr = <Column extends string>(
  e: Expr<Column>
): e is Extract<Expr<Column>, { type: 'or' }> => e.type === 'or';

export function evaluate<Column extends string>(
  expr: Expr<Column>,
  row: Record<Column, any>
): boolean {
  switch (expr.type) {
    case 'eq': {
      const v = row[expr.key];
      if (
        typeof v === 'string' ||
        typeof v === 'number' ||
        typeof v === 'boolean'
      ) {
        return v === expr.value;
      }
      return false;
    }
    case 'and':
      return (
        expr.children.length === 0 || expr.children.every(c => evaluate(c, row))
      );
    case 'or':
      return (
        expr.children.length !== 0 && expr.children.some(c => evaluate(c, row))
      );
  }
}

function formatValue(v: Value): string {
  switch (typeof v) {
    case 'string':
      return `'${v.replace(/'/g, "''")}'`;
    case 'number':
      return Number.isFinite(v) ? String(v) : `'${String(v)}'`;
    case 'boolean':
      return v ? 'TRUE' : 'FALSE';
  }
}

function escapeIdent(id: string): string {
  if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(id)) return id;
  return `"${id.replace(/"/g, '""')}"`;
}

function parenthesize(s: string): string {
  if (!s.includes(' AND ') && !s.includes(' OR ')) return s;
  return `(${s})`;
}

export function toString<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  expr: Expr<ColumnsFromRow<RowType<TableDef>>>
): string {
  switch (expr.type) {
    case 'eq': {
      const key = tableDef.columns[expr.key].columnMetadata.name ?? expr.key;
      return `${escapeIdent(key)} = ${formatValue(expr.value)}`;
    }
    case 'and':
      return parenthesize(
        expr.children.map(expr => toString(tableDef, expr)).join(' AND ')
      );
    case 'or':
      return parenthesize(
        expr.children.map(expr => toString(tableDef, expr)).join(' OR ')
      );
  }
}

/**
 * This is just the identity function to make things look like SQL.
 * @param expr
 * @returns
 */
export function where<Column extends string>(expr: Expr<Column>): Expr<Column> {
  return expr;
}

type MembershipChange = 'enter' | 'leave' | 'stayIn' | 'stayOut';

function classifyMembership<
  Col extends string,
  R extends Record<string, unknown>,
>(where: Expr<Col> | undefined, oldRow: R, newRow: R): MembershipChange {
  // No filter: everything is in, so updates are always "stayIn".
  if (!where) {
    return 'stayIn';
  }

  const oldIn = evaluate(where, oldRow);
  const newIn = evaluate(where, newRow);

  if (oldIn && !newIn) {
    return 'leave';
  }
  if (!oldIn && newIn) {
    return 'enter';
  }
  if (oldIn && newIn) {
    return 'stayIn';
  }
  return 'stayOut';
}

/**
 * Extracts the column names from a RowType whose values are of type Value.
 * Note that this will exclude columns that are of type object, array, etc.
 */
type ColumnsFromRow<R> = {
  [K in keyof R]-?: R[K] extends Value | undefined ? K : never;
}[keyof R] &
  string;

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
