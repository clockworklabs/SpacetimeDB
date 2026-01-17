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
import { Uuid } from '../lib/uuid';
import type { Prettify } from '../lib/type_util';

export interface UseTableCallbacks<RowType> {
  onInsert?: (row: RowType) => void;
  onDelete?: (row: RowType) => void;
  onUpdate?: (oldRow: RowType, newRow: RowType) => void;
}

export type Value = string | number | boolean | Uuid;

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
      // The actual value of the Column
      const v = row[expr.key];
      if (
        typeof v === 'string' ||
        typeof v === 'number' ||
        typeof v === 'boolean'
      ) {
        return v === expr.value;
      }
      if (typeof v === 'object') {
        // Value of the Column and passed Value are both a Uuid so do an integer comparison.
        if (v instanceof Uuid && expr.value instanceof Uuid) {
          return v.asBigInt() === expr.value.asBigInt();
        }
        // Value of the Column is a Uuid but passed Value is a String so compare them via string.
        if (v instanceof Uuid && typeof expr.value === 'string') {
          return v.toString() === expr.value;
        }
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
    case 'object': {
      if (v instanceof Uuid) {
        return `'${v.toString()}'`;
      }

      return '';
    }
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
  const tableName = tableDef.name;
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
