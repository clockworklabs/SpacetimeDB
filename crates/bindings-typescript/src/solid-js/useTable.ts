import {
  createSignal,
  createEffect,
  onCleanup,
  createMemo,
} from 'solid-js';
import { useSpacetimeDB } from './useSpacetimeDB';
import { type EventContextInterface } from '../sdk/db_connection_impl';
import type { ConnectionState } from './connection_state';
import type { UntypedRemoteModule } from '../sdk/spacetime_module';
import type { RowType, UntypedTableDef } from '../lib/table';
import type { Prettify } from '../lib/type_util';
import {
  type Query,
  toSql,
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

export function useTable<TableDef extends UntypedTableDef>(
  query: Query<TableDef>,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
) {
  type UseTableRowType = RowType<TableDef>;

  const accessorName = getQueryAccessorName(query);
  const whereExpr = getQueryWhereClause(query);
  const querySql = toSql(query);

  const connectionState: ConnectionState = useSpacetimeDB();

  const [rows, setRows] = createSignal<
    readonly Prettify<UseTableRowType>[]
  >([]);

  const [isReady, setIsReady] = createSignal(false);

  let latestTransactionEventId: string | null = null;

  const computeSnapshot = () => {
    const connection = connectionState.getConnection();
    if (!connection) {
      setRows([]);
      setIsReady(false);
      return;
    }

    const table = connection.db[accessorName];

    const result = whereExpr
      ? Array.from(table.iter()).filter(row =>
          evaluateBooleanExpr(whereExpr, row as Record<string, any>)
        )
      : Array.from(table.iter());

    setRows(result as Prettify<UseTableRowType>[]);
    setIsReady(true);
  };

  // Subscription Effect (runs reactively)
  createEffect(() => {
    const connection = connectionState.getConnection();
    if (!connectionState.isActive || !connection) return;

    const cancel = connection
      .subscriptionBuilder()
      .onApplied(() => {
        setIsReady(true);
      })
      .subscribe(querySql);

    onCleanup(() => {
      cancel.unsubscribe();
    });
  });

  // Table event bindings
  createEffect(() => {
    const connection = connectionState.getConnection();
    if (!connection) return;

    const table = connection.db[accessorName];

    const onInsert = (
      ctx: EventContextInterface<UntypedRemoteModule>,
      row: any
    ) => {
      if (whereExpr && !evaluateBooleanExpr(whereExpr, row)) return;

      callbacks?.onInsert?.(row);

      if (ctx.event.id !== latestTransactionEventId) {
        latestTransactionEventId = ctx.event.id;
        computeSnapshot();
      }
    };

    const onDelete = (
      ctx: EventContextInterface<UntypedRemoteModule>,
      row: any
    ) => {
      if (whereExpr && !evaluateBooleanExpr(whereExpr, row)) return;

      callbacks?.onDelete?.(row);

      if (ctx.event.id !== latestTransactionEventId) {
        latestTransactionEventId = ctx.event.id;
        computeSnapshot();
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

      if (ctx.event.id !== latestTransactionEventId) {
        latestTransactionEventId = ctx.event.id;
        computeSnapshot();
      }
    };

    table.onInsert(onInsert);
    table.onDelete(onDelete);
    table.onUpdate?.(onUpdate);

    computeSnapshot(); // initial load

    onCleanup(() => {
      table.removeOnInsert(onInsert);
      table.removeOnDelete(onDelete);
      table.removeOnUpdate?.(onUpdate);
    });
  });

  return createMemo(() => [rows(), isReady()] as const);
}