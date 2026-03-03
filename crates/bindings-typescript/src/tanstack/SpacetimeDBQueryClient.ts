import type {
  QueryClient,
  QueryKey,
  QueryFunction,
} from '@tanstack/react-query';
import {
  type Query,
  toSql,
  type BooleanExpr,
  evaluateBooleanExpr,
  getQueryAccessorName,
  getQueryWhereClause,
} from '../lib/query';

type QueryInput = Query<any>;

const queryRegistry = new Map<
  string,
  { accessorName: string; whereExpr?: BooleanExpr<any> }
>();

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
// useQuery(spacetimeDBQuery(tables.user.where(r => r.online.eq(true))));
// useQuery(spacetimeDBQuery(condition ? tables.user : 'skip'));
export function spacetimeDBQuery(
  queryOrSkip: 'skip'
): SpacetimeDBQueryOptionsSkipped;

export function spacetimeDBQuery(query: QueryInput): SpacetimeDBQueryOptions;

export function spacetimeDBQuery(
  queryOrSkip: QueryInput | 'skip'
): SpacetimeDBQueryOptions | SpacetimeDBQueryOptionsSkipped {
  if (queryOrSkip === 'skip') {
    return {
      queryKey: ['spacetimedb', '', 'skip'] as const,
      staleTime: Infinity,
      enabled: false,
    };
  }

  const query = queryOrSkip;
  const accessorName = getQueryAccessorName(query);
  const whereExpr = getQueryWhereClause(query);
  const querySql = toSql(query);

  queryRegistry.set(querySql, { accessorName, whereExpr });

  return {
    queryKey: ['spacetimedb', accessorName, querySql] as const,
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
      querySql: string;
      whereExpr?: BooleanExpr<any>;
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

    this.cacheUnsubscribe = queryClient
      .getQueryCache()
      .subscribe((event: any) => {
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

  queryFn: QueryFunction<any[], QueryKey> = async ({
    queryKey,
  }: {
    queryKey: QueryKey;
  }) => {
    const keyStr = JSON.stringify(queryKey);
    const [prefix, accessorName, querySql] = queryKey as [
      string,
      string,
      string,
    ];

    if (prefix !== 'spacetimedb') {
      throw new Error(
        `SpacetimeDBQueryClient can only handle spacetimedb queries, got: ${prefix}`
      );
    }

    const registered = queryRegistry.get(querySql);
    const whereExpr = registered?.whereExpr;

    const existingSub = this.subscriptions.get(keyStr);
    if (existingSub?.applied) {
      return this.getTableData(existingSub.tableInstance, whereExpr);
    }

    // queue query if connection not ready yet
    if (!this.connection) {
      return new Promise<any[]>(resolve => {
        const pending = this.pendingQueries.get(keyStr) || [];
        pending.push({ resolve, querySql, whereExpr });
        this.pendingQueries.set(keyStr, pending);
      });
    }

    return this.setupSubscription(queryKey, accessorName, querySql, whereExpr);
  };

  private getTableData(
    tableInstance: any,
    whereExpr?: BooleanExpr<any>
  ): any[] {
    const allRows = Array.from(tableInstance.iter());
    if (whereExpr) {
      return allRows.filter(row =>
        evaluateBooleanExpr(whereExpr, row as Record<string, any>)
      );
    }
    return allRows;
  }

  private setupSubscription(
    queryKey: QueryKey,
    accessorName: string,
    querySql: string,
    whereExpr?: BooleanExpr<any>
  ): Promise<any[]> {
    if (!this.connection) {
      return Promise.resolve([]);
    }

    const keyStr = JSON.stringify(queryKey);
    const db = this.connection.db;

    const tableInstance = db[accessorName];

    if (!tableInstance) {
      console.warn(`SpacetimeDBQueryClient: table "${accessorName}" not found`);
      return Promise.resolve([]);
    }

    // return existing data if already subscribed
    const existingSub = this.subscriptions.get(keyStr);
    if (existingSub) {
      if (existingSub.applied) {
        return Promise.resolve(
          this.getTableData(existingSub.tableInstance, whereExpr)
        );
      }
      return new Promise(resolve => {
        const pending = this.pendingQueries.get(keyStr) || [];
        pending.push({ resolve, querySql, whereExpr });
        this.pendingQueries.set(keyStr, pending);
      });
    }

    return new Promise<any[]>(resolve => {
      const updateCache = () => {
        if (!this.queryClient) return [];
        const data = this.getTableData(tableInstance, whereExpr);
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
        .subscribe(querySql);

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
      const [, accessorName] = queryKey as [string, string, string];

      if (pending.length > 0) {
        const first = pending[0];
        this.setupSubscription(
          queryKey,
          accessorName,
          first.querySql,
          first.whereExpr
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
