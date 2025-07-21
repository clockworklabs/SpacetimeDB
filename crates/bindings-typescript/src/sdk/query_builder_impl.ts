import type { TableMap } from './client_cache';
import type { DbConnectionImpl } from './db_connection_impl';
import type {
  ErrorContextInterface,
  QueryEventContextInterface,
} from './event_context';
import { EventEmitter } from './event_emitter';
import type { TimeDuration } from '../';
import type { UntypedRemoteModule } from './spacetime_module';

export class QueryBuilderImpl<RemoteModule extends UntypedRemoteModule> {
  #onResolved?: (
    ctx: QueryEventContextInterface<RemoteModule>,
    tables: TableMap<RemoteModule>,
    totalHostExecutionDuration: TimeDuration
  ) => void = undefined;
  #onError?: (
    ctx: ErrorContextInterface<RemoteModule>,
    error: Error,
    totalHostExecutionDuration: TimeDuration
  ) => void = undefined;
  constructor(private db: DbConnectionImpl<RemoteModule>) {}

  onResolved(
    cb: (
      ctx: QueryEventContextInterface<RemoteModule>,
      tables: TableMap<RemoteModule>,
      totalHostExecutionDuration: TimeDuration
    ) => void
  ): QueryBuilderImpl<RemoteModule> {
    this.#onResolved = cb;
    return this;
  }

  onError(
    cb: (
      ctx: ErrorContextInterface<RemoteModule>,
      error: Error,
      totalHostExecutionDuration: TimeDuration
    ) => void
  ): QueryBuilderImpl<RemoteModule> {
    this.#onError = cb;
    return this;
  }

  query(
    query_sql: string
  ): QueryHandleImpl<RemoteModule> {
    return new QueryHandleImpl(
      this.db,
      query_sql,
      this.#onResolved,
      this.#onError
    );
  }
}

export type QueryEvent = 'resolved' | 'error';

export class QueryManager<RemoteModule extends UntypedRemoteModule> {
  queries: Map<
    number,
    {
      handle: QueryHandleImpl<RemoteModule>;
      emitter: EventEmitter<QueryEvent>;
    }
  > = new Map();
}

export class QueryHandleImpl<RemoteModule extends UntypedRemoteModule> {
  #queryId: number;
  #endedState: boolean = false;
  #resolvedState: boolean = false;
  #emitter: EventEmitter<QueryEvent, (...args: any[]) => void> =
    new EventEmitter();

  constructor(
    private db: DbConnectionImpl<RemoteModule>,
    querySql: string,
    onResolved?: (
      ctx: QueryEventContextInterface<RemoteModule>,
      tables: TableMap<RemoteModule>,
      totalHostExecutionDuration: TimeDuration
    ) => void,
    onError?: (
      ctx: ErrorContextInterface<RemoteModule>,
      error: Error,
      totalHostExecutionDuration: TimeDuration
    ) => void
  ) {
    this.#emitter.on(
      'resolved',
      (
        ctx: QueryEventContextInterface<RemoteModule>,
        tables: TableMap<RemoteModule>,
        totalHostExecutionDuration: TimeDuration
      ) => {
        this.#resolvedState = true;
        if (onResolved) {
          onResolved(ctx, tables, totalHostExecutionDuration);
        }
      }
    );
    this.#emitter.on(
      'error',
      (
        ctx: ErrorContextInterface<RemoteModule>,
        error: Error,
        totalHostExecutionDuration: TimeDuration
      ) => {
        this.#resolvedState = false;
        this.#endedState = true;
        if (onError) {
          onError(ctx, error, totalHostExecutionDuration);
        }
      }
    );
    this.#queryId = this.db.registerQuery(this, this.#emitter, querySql);
  }

  isEnded(): boolean {
    return this.#endedState;
  }

  isResolved(): boolean {
    return this.#resolvedState;
  }
}
