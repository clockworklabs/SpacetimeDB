import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { useSpacetimeDB } from './useSpacetimeDB';
import { DbConnectionImpl, TableCache } from '../sdk/db_connection_impl';
import type { TableNamesFromDb } from '../sdk/table_handle';

export interface UseQueryCallbacks<RowType> {
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

type RecordLike<Column extends string> = Record<Column, unknown>;

export function evaluate<Column extends string>(
  expr: Expr<Column>,
  row: RecordLike<Column>
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

export function toString<Column extends string>(expr: Expr<Column>): string {
  switch (expr.type) {
    case 'eq':
      return `${escapeIdent(expr.key)} = ${formatValue(expr.value)}`;
    case 'and':
      return parenthesize(expr.children.map(toString).join(' AND '));
    case 'or':
      return parenthesize(expr.children.map(toString).join(' OR '));
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

type Snapshot<RowType> = {
  readonly rows: readonly RowType[];
  readonly state: 'loading' | 'ready';
};

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
export function useTable<
  DbConnection extends DbConnectionImpl,
  RowType extends Record<string, any>,
  TableName extends TableNamesFromDb<DbConnection['db']> = TableNamesFromDb<
    DbConnection['db']
  >,
>(
  tableName: TableName,
  where: Expr<ColumnsFromRow<RowType>>,
  callbacks?: UseQueryCallbacks<RowType>
): Snapshot<RowType>;

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
export function useTable<
  DbConnection extends DbConnectionImpl,
  RowType extends Record<string, any>,
  TableName extends TableNamesFromDb<DbConnection['db']> = TableNamesFromDb<
    DbConnection['db']
  >,
>(
  tableName: TableName,
  callbacks?: UseQueryCallbacks<RowType>
): Snapshot<RowType>;

export function useTable<
  DbConnection extends DbConnectionImpl,
  RowType extends Record<string, any>,
  TableName extends TableNamesFromDb<DbConnection['db']> = TableNamesFromDb<
    DbConnection['db']
  >,
>(
  tableName: TableName,
  whereClauseOrCallbacks?:
    | Expr<ColumnsFromRow<RowType>>
    | UseQueryCallbacks<RowType>,
  callbacks?: UseQueryCallbacks<RowType>
): Snapshot<RowType> {
  let whereClause: Expr<ColumnsFromRow<RowType>> | undefined;
  if (
    whereClauseOrCallbacks &&
    typeof whereClauseOrCallbacks === 'object' &&
    'type' in whereClauseOrCallbacks
  ) {
    whereClause = whereClauseOrCallbacks as Expr<ColumnsFromRow<RowType>>;
  } else {
    callbacks = whereClauseOrCallbacks as
      | UseQueryCallbacks<RowType>
      | undefined;
  }
  const [subscribeApplied, setSubscribeApplied] = useState(false);
  let spacetime: DbConnection | undefined;
  try {
    spacetime = useSpacetimeDB<DbConnection>();
  } catch {
    throw new Error(
      'Could not find SpacetimeDB client! Did you forget to add a ' +
        '`SpacetimeDBProvider`? `useTable` must be used in the React component tree ' +
        'under a `SpacetimeDBProvider` component.'
    );
  }
  const client = spacetime;

  const query =
    `SELECT * FROM ${tableName}` +
    (whereClause ? ` WHERE ${toString(whereClause)}` : '');

  const latestTransactionEvent = useRef<any>(null);
  const lastSnapshotRef = useRef<Snapshot<RowType> | null>(null);

  const whereKey = whereClause ? toString(whereClause) : '';

  const computeSnapshot = useCallback((): Snapshot<RowType> => {
    const table = client.db[
      tableName as keyof typeof client.db
    ] as unknown as TableCache<RowType>;
    const result: readonly RowType[] = whereClause
      ? table.iter().filter(row => evaluate(whereClause, row))
      : table.iter();
    return {
      rows: result,
      state: subscribeApplied ? 'ready' : 'loading',
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, tableName, whereKey, subscribeApplied]);

  useEffect(() => {
    if (client.isActive) {
      const cancel = client
        .subscriptionBuilder()
        .onApplied(() => {
          setSubscribeApplied(true);
        })
        .subscribe(query);
      return () => {
        cancel.unsubscribe();
      };
    }
  }, [query, client.isActive, client]);

  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      const onInsert = (ctx: any, row: RowType) => {
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

      const onDelete = (ctx: any, row: RowType) => {
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

      const onUpdate = (ctx: any, oldRow: RowType, newRow: RowType) => {
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

      const table = client.db[
        tableName as keyof typeof client.db
      ] as unknown as TableCache<RowType>;
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
      client,
      tableName,
      whereKey,
      callbacks?.onDelete,
      callbacks?.onInsert,
      callbacks?.onUpdate,
    ]
  );

  const getSnapshot = useCallback((): Snapshot<RowType> => {
    if (!lastSnapshotRef.current) {
      lastSnapshotRef.current = computeSnapshot();
    }
    return lastSnapshotRef.current;
  }, [computeSnapshot]);

  // SSR fallback can be the same getter
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
