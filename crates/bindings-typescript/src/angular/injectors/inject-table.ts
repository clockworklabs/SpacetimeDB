import {
  assertInInjectionContext,
  inject,
  signal,
  effect,
  type Signal,
} from '@angular/core';
import type { RowType, UntypedTableDef } from '../../lib/table';
import type { Prettify } from '../../lib/type_util';
import { SPACETIMEDB_CONNECTION } from '../connection_state';
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
} from '../../lib/filter';
import type { EventContextInterface } from '../../sdk';
import type { UntypedRemoteModule } from '../../sdk/spacetime_module';

export { eq, and, or, isEq, isAnd, isOr, where };
export type { Value, Expr };

export type RowTypeDef<TableDef extends UntypedTableDef> = Prettify<
  RowType<TableDef>
>;

export interface TableRows<TableDef extends UntypedTableDef> {
  rows: readonly RowTypeDef<TableDef>[];
  isLoading: boolean;
}

export interface InjectTableCallbacks<RowType> {
  onInsert?: (row: RowType) => void;
  onDelete?: (row: RowType) => void;
  onUpdate?: (oldRow: RowType, newRow: RowType) => void;
}

export interface InjectTableOptions<TableDef extends UntypedTableDef> {
  where?: Expr<ColumnsFromRow<RowType<TableDef>>>;
  callbacks?: InjectTableCallbacks<RowTypeDef<TableDef>>;
}

/**
 * Angular injection function to subscribe to a table in SpacetimeDB and receive live updates.
 *
 * Must be called within an injection context (component field initializer or constructor).
 *
 * @template TableDef The table definition type.
 *
 * @param tableDef - The table definition to subscribe to.
 * @param options - Optional configuration including where clause and callbacks.
 *
 * @returns A signal containing the current rows and loading state.
 *
 * @example
 * ```typescript
 * export class UsersComponent {
 *   users = injectTable(User, {
 *     where: where(eq('isActive', true)),
 *     callbacks: {
 *       onInsert: (row) => console.log('Inserted:', row),
 *       onDelete: (row) => console.log('Deleted:', row),
 *       onUpdate: (oldRow, newRow) => console.log('Updated:', oldRow, newRow),
 *     }
 *   });
 *
 *   // In template: {{ users().rows.length }} users
 *   // Loading state: {{ users().isLoading }}
 * }
 * ```
 */
export function injectTable<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  options?: InjectTableOptions<TableDef>
): Signal<TableRows<TableDef>> {
  assertInInjectionContext(injectTable);

  type UseTableRowType = RowType<TableDef>;

  const connState = inject(SPACETIMEDB_CONNECTION);

  const tableName = tableDef.name;
  const accessorName = tableDef.accessorName;
  const whereClause = options?.where;
  const callbacks = options?.callbacks;

  const tableSignal = signal<TableRows<TableDef>>({
    isLoading: true,
    rows: [],
  });

  let latestTransactionEvent: any = null;
  let subscribeApplied = false;

  const whereKey = whereClause ? toString(tableDef, whereClause) : '';
  const query =
    `SELECT * FROM ${tableName}` + (whereClause ? ` WHERE ${whereKey}` : '');

  // Note: this code is mostly derived from the React useTable implementation
  // in order to keep behavior consistent across frameworks.

  const computeSnapshot = (): readonly RowTypeDef<TableDef>[] => {
    const state = connState();
    if (!state.isActive) {
      return [];
    }

    const connection = state.getConnection();
    if (!connection) {
      return [];
    }

    const table = connection.db[accessorName];

    if (whereClause) {
      return Array.from(table.iter()).filter(row =>
        evaluate(whereClause, row as UseTableRowType)
      ) as RowTypeDef<TableDef>[];
    }

    return Array.from(table.iter()) as RowTypeDef<TableDef>[];
  };

  const updateSnapshot = () => {
    tableSignal.set({
      rows: computeSnapshot(),
      isLoading: !subscribeApplied,
    });
  };

  effect(onCleanup => {
    const state = connState();
    if (!state.isActive) {
      return;
    }

    const connection = state.getConnection();
    if (!connection) {
      return;
    }

    const table = connection.db[accessorName];

    const onInsert = (
      ctx: EventContextInterface<UntypedRemoteModule>,
      row: any
    ) => {
      if (whereClause && !evaluate(whereClause, row)) {
        return;
      }

      callbacks?.onInsert?.(row);

      if (ctx.event !== latestTransactionEvent || !latestTransactionEvent) {
        latestTransactionEvent = ctx.event;
        updateSnapshot();
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

      if (ctx.event !== latestTransactionEvent || !latestTransactionEvent) {
        latestTransactionEvent = ctx.event;
        updateSnapshot();
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
          return;
      }

      if (ctx.event !== latestTransactionEvent || !latestTransactionEvent) {
        latestTransactionEvent = ctx.event;
        updateSnapshot();
      }
    };

    table.onInsert(onInsert);
    table.onDelete(onDelete);
    table.onUpdate?.(onUpdate);

    const subscription = connection
      .subscriptionBuilder()
      .onApplied(() => {
        subscribeApplied = true;
        updateSnapshot();
      })
      .subscribe(query);

    onCleanup(() => {
      table.removeOnInsert(onInsert);
      table.removeOnDelete(onDelete);
      table.removeOnUpdate?.(onUpdate);
      subscription.unsubscribe();
    });
  });

  return tableSignal.asReadonly();
}
