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
  type Query,
  type BooleanExpr,
  toSql,
  evaluateBooleanExpr,
  getQueryAccessorName,
  getQueryWhereClause,
} from '../../lib/query';
import type { EventContextInterface } from '../../sdk';
import type { UntypedRemoteModule } from '../../sdk/spacetime_module';

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
 * Angular injection function to subscribe to a table in SpacetimeDB and receive live updates.
 *
 * Must be called within an injection context (component field initializer or constructor).
 *
 * Accepts a query builder expression as the first argument:
 * - `tables.user` — subscribe to all rows
 * - `tables.user.where(r => r.online.eq(true))` — subscribe with a filter
 *
 * @template TableDef The table definition type.
 *
 * @param query - A query builder expression (table reference or filtered query).
 * @param callbacks - Optional callbacks for row insert, delete, and update events.
 *
 * @returns A signal containing the current rows and loading state.
 *
 * @example
 * ```typescript
 * export class UsersComponent {
 *   users = injectTable(tables.user);
 *
 *   // With a filter:
 *   onlineUsers = injectTable(
 *     tables.user.where(r => r.online.eq(true)),
 *     {
 *       onInsert: (row) => console.log('Inserted:', row),
 *       onDelete: (row) => console.log('Deleted:', row),
 *       onUpdate: (oldRow, newRow) => console.log('Updated:', oldRow, newRow),
 *     }
 *   );
 *
 *   // In template: {{ users().rows.length }} users
 *   // Loading state: {{ users().isLoading }}
 * }
 * ```
 */
export function injectTable<TableDef extends UntypedTableDef>(
  query: Query<TableDef>,
  callbacks?: InjectTableCallbacks<RowTypeDef<TableDef>>
): Signal<TableRows<TableDef>> {
  assertInInjectionContext(injectTable);

  const connState = inject(SPACETIMEDB_CONNECTION);

  const accessorName = getQueryAccessorName(query);
  const whereExpr = getQueryWhereClause(query);
  const querySql = toSql(query);

  const tableSignal = signal<TableRows<TableDef>>({
    isLoading: true,
    rows: [],
  });

  let latestTransactionEvent: any = null;
  let subscribeApplied = false;

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

    if (whereExpr) {
      return Array.from(table.iter()).filter(row =>
        evaluateBooleanExpr(whereExpr, row as Record<string, any>)
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

  effect((onCleanup: (fn: () => void) => void) => {
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
      if (whereExpr && !evaluateBooleanExpr(whereExpr, row)) {
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
      if (whereExpr && !evaluateBooleanExpr(whereExpr, row)) {
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
      .subscribe(querySql);

    onCleanup(() => {
      table.removeOnInsert(onInsert);
      table.removeOnDelete(onDelete);
      table.removeOnUpdate?.(onUpdate);
      subscription.unsubscribe();
    });
  });

  return tableSignal.asReadonly();
}
