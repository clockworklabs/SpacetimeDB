import type {
  QueryClient,
  QueryKey,
  QueryFunction,
} from '@tanstack/react-query';
import type { UntypedTableDef, RowType } from '../lib/table';
import {
  type Expr,
  type ColumnsFromRow,
  evaluate,
  toString,
} from '../lib/filter';

const tableRegistry = new Map<string, UntypedTableDef>();
const whereRegistry = new Map<string, Expr<any>>();

export interface SpacetimeDBQueryOptions {
  queryKey: readonly ['spacetimedb', string, string];
  staleTime: number;
}

export interface SpacetimeDBQueryOptionsSkipped
  extends SpacetimeDBQueryOptions {
  enabled: false;
}

// creates query options for useQuery/useSuspenseQuery.
// useQuery(spacetimeDBQuery(tables.person));
// useQuery(spacetimeDBQuery(tables.user, where(eq('role', 'admin'))));
// useQuery(spacetimeDBQuery(tables.user, userId ? where(eq('id', userId)) : 'skip'));
export function spacetimeDBQuery<TableDef extends UntypedTableDef>(
  table: TableDef,
  whereOrSkip: 'skip'
): SpacetimeDBQueryOptionsSkipped;

export function spacetimeDBQuery<TableDef extends UntypedTableDef>(
  table: TableDef,
  where?: Expr<ColumnsFromRow<RowType<TableDef>>>
): SpacetimeDBQueryOptions;

export function spacetimeDBQuery<TableDef extends UntypedTableDef>(
  table: TableDef,
  whereOrSkip?: Expr<ColumnsFromRow<RowType<TableDef>>> | 'skip'
): SpacetimeDBQueryOptions | SpacetimeDBQueryOptionsSkipped {
  tableRegistry.set(table.name, table);

  if (whereOrSkip === 'skip') {
    return {
      queryKey: ['spacetimedb', table.name, 'skip'] as const,
      staleTime: Infinity,
      enabled: false,
    };
  }

  const where = whereOrSkip;
  const whereStr = where ? toString(table, where) : '';

  if (where) {
    const whereKey = `${table.name}:${whereStr}`;
    whereRegistry.set(whereKey, where);
  }

  return {
    queryKey: ['spacetimedb', table.name, whereStr] as const,
    staleTime: Infinity,
  };
}

interface SpacetimeConnection {
  db: Record<string, any>;
  subscriptionBuilder: () => {
    onApplied: (cb: () => void) => any;
    subscribe: (query: string) => { unsubscribe: () => void };
  };
}

interface SubscriptionState {
  unsubscribe: () => void;
  tableInstance: any;
  applied: boolean;
}

// push updates to cache via setQueryData when SpacetimeDB data changes
export class SpacetimeDBQueryClient {
  private connection: SpacetimeConnection | null = null;
  private queryClient: QueryClient | null = null;
  private subscriptions = new Map<string, SubscriptionState>();
  private pendingQueries = new Map<
    string,
    Array<{
      resolve: (data: any[]) => void;
      tableDef: any;
      whereClause?: Expr<any>;
    }>
  >();
  private cacheUnsubscribe: (() => void) | null = null;

  // set connection, called on onConnect callback
  setConnection(connection: SpacetimeConnection): void {
    this.connection = connection;
    this.processPendingQueries();
  }

  connect(queryClient: QueryClient): void {
    this.queryClient = queryClient;

    this.cacheUnsubscribe = queryClient.getQueryCache().subscribe(event => {
      if (
        event.type === 'removed' &&
        event.query.queryKey[0] === 'spacetimedb'
      ) {
        const keyStr = JSON.stringify(event.query.queryKey);
        const sub = this.subscriptions.get(keyStr);
        if (sub) {
          sub.unsubscribe();
          this.subscriptions.delete(keyStr);
        }
      }
    });
  }

  queryFn: QueryFunction<any[], QueryKey> = async ({ queryKey }) => {
    const keyStr = JSON.stringify(queryKey);
    const [prefix, tableName, whereStr] = queryKey as [string, string, string];

    if (prefix !== 'spacetimedb') {
      throw new Error(
        `SpacetimeDBQueryClient can only handle spacetimedb queries, got: ${prefix}`
      );
    }

    const tableDef = tableRegistry.get(tableName);
    const whereKey = `${tableName}:${whereStr}`;
    const whereClause = whereStr ? whereRegistry.get(whereKey) : undefined;

    const existingSub = this.subscriptions.get(keyStr);
    if (existingSub?.applied) {
      return this.getTableData(existingSub.tableInstance, whereClause);
    }

    // queue query if connection not ready yet
    if (!this.connection) {
      return new Promise<any[]>(resolve => {
        const pending = this.pendingQueries.get(keyStr) || [];
        pending.push({ resolve, tableDef, whereClause });
        this.pendingQueries.set(keyStr, pending);
      });
    }

    return this.setupSubscription(queryKey, tableName, tableDef, whereClause);
  };

  private getTableData(tableInstance: any, whereClause?: Expr<any>): any[] {
    const allRows = Array.from(tableInstance.iter());
    if (whereClause) {
      return allRows.filter(row =>
        evaluate(whereClause, row as Record<string, unknown>)
      );
    }
    return allRows;
  }

  private setupSubscription(
    queryKey: QueryKey,
    tableName: string,
    tableDef: any,
    whereClause?: Expr<any>
  ): Promise<any[]> {
    if (!this.connection) {
      return Promise.resolve([]);
    }

    const keyStr = JSON.stringify(queryKey);
    const db = this.connection.db;

    const accessorName = tableDef?.accessorName ?? tableName;
    const tableInstance = db[accessorName];

    if (!tableInstance) {
      console.warn(
        `SpacetimeDBQueryClient: table "${tableName}" (accessor: ${accessorName}) not found`
      );
      return Promise.resolve([]);
    }

    // return existing data if already subscribed
    const existingSub = this.subscriptions.get(keyStr);
    if (existingSub) {
      if (existingSub.applied) {
        return Promise.resolve(
          this.getTableData(existingSub.tableInstance, whereClause)
        );
      }
      return new Promise(resolve => {
        const pending = this.pendingQueries.get(keyStr) || [];
        pending.push({ resolve, tableDef, whereClause });
        this.pendingQueries.set(keyStr, pending);
      });
    }

    const query =
      `SELECT * FROM ${tableName}` +
      (whereClause && tableDef
        ? ` WHERE ${toString(tableDef, whereClause as any)}`
        : '');

    return new Promise<any[]>(resolve => {
      const updateCache = () => {
        if (!this.queryClient) return [];
        const data = this.getTableData(tableInstance, whereClause);
        this.queryClient.setQueryData(queryKey, data);
        return data;
      };

      const handle = this.connection!.subscriptionBuilder()
        .onApplied(() => {
          const sub = this.subscriptions.get(keyStr);
          if (sub) {
            sub.applied = true;
          }

          const data = updateCache();
          resolve(data);

          const pending = this.pendingQueries.get(keyStr);
          if (pending) {
            for (const p of pending) {
              p.resolve(data);
            }
            this.pendingQueries.delete(keyStr);
          }
        })
        .subscribe(query);

      // push updates to cache when data changes
      const onTableChange = () => {
        const sub = this.subscriptions.get(keyStr);
        if (sub?.applied) {
          updateCache();
        }
      };

      tableInstance.onInsert(onTableChange);
      tableInstance.onDelete(onTableChange);
      tableInstance.onUpdate?.(onTableChange);

      this.subscriptions.set(keyStr, {
        unsubscribe: () => {
          handle.unsubscribe();
          tableInstance.removeOnInsert(onTableChange);
          tableInstance.removeOnDelete(onTableChange);
          tableInstance.removeOnUpdate?.(onTableChange);
        },
        tableInstance,
        applied: false,
      });
    });
  }

  private processPendingQueries(): void {
    if (!this.connection) return;

    const pendingEntries = Array.from(this.pendingQueries.entries());
    this.pendingQueries.clear();

    for (const [keyStr, pending] of pendingEntries) {
      const queryKey = JSON.parse(keyStr) as QueryKey;
      const [, tableName] = queryKey as [string, string, string];

      if (pending.length > 0) {
        const first = pending[0];
        this.setupSubscription(
          queryKey,
          tableName,
          first.tableDef,
          first.whereClause
        )
          .then(data => {
            for (const p of pending) {
              p.resolve(data);
            }
          })
          .catch(() => {
            for (const p of pending) {
              p.resolve([]);
            }
          });
      }
    }
  }

  // clean up all subscriptions and disconnect
  disconnect(): void {
    if (this.cacheUnsubscribe) {
      this.cacheUnsubscribe();
      this.cacheUnsubscribe = null;
    }

    for (const sub of this.subscriptions.values()) {
      sub.unsubscribe();
    }
    this.subscriptions.clear();
    this.pendingQueries.clear();
    this.connection = null;
  }
}
