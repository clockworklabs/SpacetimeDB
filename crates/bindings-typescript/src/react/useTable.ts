import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { useSpacetimeDB } from './useSpacetimeDB';
import { DbConnectionImpl, TableCache } from '../sdk';

export interface UseQueryCallbacks<RowType> {
  onInsert?: (row: RowType) => void;
  onDelete?: (row: RowType) => void;
  onUpdate?: (oldRow: RowType, newRow: RowType) => void;
}

type WhereClause = {
  key: string;
  operator: '=';
  value: string | number | boolean;
};

export function eq<ColumnType extends string>(
  key: ColumnType,
  value: string | number | boolean
): WhereClause {
  return { key, operator: '=', value };
}

export function where(condition: WhereClause) {
  return `${condition.key} ${condition.operator} ${JSON.stringify(condition.value)}`;
}

function matchesWhereClause(
  row: Record<string, any>,
  whereClause: WhereClause
): boolean {
  const { key, operator, value } = whereClause;
  if (!(key in row)) {
    return false;
  }
  switch (operator) {
    case '=':
      return row[key] === value;
    default:
      return false;
  }
}

type Snapshot<RowType> = {
  readonly rows: readonly RowType[];
  readonly state: 'loading' | 'ready';
};

export function useTable<
  DbConnection extends DbConnectionImpl,
  RowType extends Record<string, any>,
  TableName extends keyof DbConnection['db'] &
    string = keyof DbConnection['db'] & string,
>(
  tableName: TableName,
  whereClauseOrCallbacks?: WhereClause | UseQueryCallbacks<RowType>,
  callbacks?: UseQueryCallbacks<RowType>
): Snapshot<RowType> {
  let whereClause: WhereClause | undefined;
  if (whereClauseOrCallbacks && 'key' in whereClauseOrCallbacks) {
    whereClause = whereClauseOrCallbacks;
  } else {
    callbacks = whereClauseOrCallbacks as
      | UseQueryCallbacks<RowType>
      | undefined;
  }
  const [subscribeApplied, setSubscribeApplied] = useState(false);
  const [isActive, setIsActive] = useState(false);
  let spacetime: DbConnection | undefined;
  try {
    spacetime = useSpacetimeDB<DbConnection>();
  } catch {
    throw new Error(
      'Could not find SpacetimeDB client! Did you forget to add a' +
        '`SpacetimeDBProvider`? `useTable` must be used in the React component tree' +
        'under a `SpacetimeDBProvider` component.'
    );
  }
  const client = spacetime;

  const query =
    `SELECT * FROM ${tableName}` +
    (whereClause ? ` WHERE ${where(whereClause)}` : '');

  const latestTransactionEvent = useRef<any>(null);
  const lastSnapshotRef = useRef<Snapshot<RowType> | null>(null);

  const whereKey = whereClause
    ? `${whereClause.key}|${whereClause.operator}|${JSON.stringify(whereClause.value)}`
    : '';

  const computeSnapshot = useCallback((): Snapshot<RowType> => {
    const table = client.db[
      tableName as keyof typeof client.db
    ] as unknown as TableCache<RowType>;
    const result: readonly RowType[] = whereClause
      ? table.iter().filter(row => matchesWhereClause(row, whereClause))
      : table.iter();
    return {
      rows: result,
      state: subscribeApplied ? 'ready' : 'loading',
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, tableName, whereKey, subscribeApplied]);

  useEffect(() => {
    const onConnect = () => {
      setIsActive(client.isActive);
    };
    const onDisconnect = () => {
      setIsActive(client.isActive);
    };
    const onConnectError = () => {
      setIsActive(client.isActive);
    };
    client['on']('connect', onConnect);
    client['on']('disconnect', onDisconnect);
    client['on']('connectError', onConnectError);
    return () => {
      client['off']('connect', onConnect);
      client['off']('disconnect', onDisconnect);
      client['off']('connectError', onConnectError);
    };
  }, [client]);

  useEffect(() => {
    if (isActive) {
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
  }, [query, isActive, client]);

  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      const onInsert = (ctx: any, row: RowType) => {
        if (whereClause && !matchesWhereClause(row, whereClause)) {
          return;
        }
        if (tableName === 'message') {
          console.log('onInsert for messages table:', row);
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
        if (whereClause && !matchesWhereClause(row, whereClause)) {
          return;
        }
        if (tableName === 'message') {
          console.log('onDelete for messages table:', row);
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
        // If your filtering is based on newRow membership; adjust if you also care when it LEAVES the filter
        const affected =
          !whereClause ||
          matchesWhereClause(oldRow, whereClause) ||
          matchesWhereClause(newRow, whereClause);
        if (!affected) {
          return;
        }
        callbacks?.onUpdate?.(oldRow, newRow);
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
