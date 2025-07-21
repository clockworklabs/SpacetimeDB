import type { OneOffTable } from './client_api';
import type { DbConnectionImpl } from './db_connection_impl';
import type {
  ErrorContextInterface,
  QueryEventContextInterface,
} from './event_context';
import { EventEmitter } from './event_emitter';
import type { TimeDuration } from './time_duration';

export class QueryBuilderImpl<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
> {
  #onResolved?: (
    ctx: QueryEventContextInterface<DBView, Reducers, SetReducerFlags>,
    tables: Map<string, any>,
    totalHostExecutionDuration: TimeDuration
  ) => void = undefined;
  #onError?: (
    ctx: ErrorContextInterface<DBView, Reducers, SetReducerFlags>,
    error: Error,
    totalHostExecutionDuration: TimeDuration
  ) => void = undefined;
  constructor(
    private db: DbConnectionImpl<DBView, Reducers, SetReducerFlags>
  ) {}

  onResolved(
    cb: (
      ctx: QueryEventContextInterface<DBView, Reducers, SetReducerFlags>,
      tables: Map<string, any>,
      totalHostExecutionDuration: TimeDuration
    ) => void
  ): QueryBuilderImpl<DBView, Reducers, SetReducerFlags> {
    this.#onResolved = cb;
    return this;
  }

  onError(
    cb: (
      ctx: ErrorContextInterface<DBView, Reducers, SetReducerFlags>,
      error: Error,
      totalHostExecutionDuration: TimeDuration
    ) => void
  ): QueryBuilderImpl<DBView, Reducers, SetReducerFlags> {
    this.#onError = cb;
    return this;
  }

  query(
    query_sql: string
  ): QueryHandleImpl<DBView, Reducers, SetReducerFlags> {
    return new QueryHandleImpl(
      this.db,
      query_sql,
      this.#onResolved,
      this.#onError
    );
  }
}

export type QueryEvent = 'resolved' | 'error';

export class QueryManager {
  queries: Map<
    number,
    { handle: QueryHandleImpl; emitter: EventEmitter<QueryEvent> }
  > = new Map();
}

export class QueryHandleImpl<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
> {
  #queryId: number;
  #endedState: boolean = false;
  #resolvedState: boolean = false;
  #emitter: EventEmitter<QueryEvent, (...args: any[]) => void> =
    new EventEmitter();

  constructor(
    private db: DbConnectionImpl<DBView, Reducers, SetReducerFlags>,
    querySql: string,
    onResolved?: (
      ctx: QueryEventContextInterface<DBView, Reducers, SetReducerFlags>,
      tables: Map<string, any>,
      totalHostExecutionDuration: TimeDuration
    ) => void,
    onError?: (
      ctx: ErrorContextInterface<DBView, Reducers, SetReducerFlags>,
      error: Error,
      totalHostExecutionDuration: TimeDuration
    ) => void
  ) {
    this.#emitter.on(
      'resolved',
      (
        ctx: QueryEventContextInterface<
          DBView,
          Reducers,
          SetReducerFlags
        >,
        tables: Map<string, any>,
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
        ctx: ErrorContextInterface<DBView, Reducers, SetReducerFlags>,
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
