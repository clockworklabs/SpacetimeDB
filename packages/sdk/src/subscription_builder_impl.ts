import type { DbConnectionImpl } from './db_connection_impl';
import type {
  ErrorContextInterface,
  SubscriptionEventContextInterface,
} from './event_context';
import { EventEmitter } from './event_emitter';

export class SubscriptionBuilderImpl<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
> {
  #onApplied?: (
    ctx: SubscriptionEventContextInterface<DBView, Reducers, SetReducerFlags>
  ) => void = undefined;
  #onError?: (
    ctx: ErrorContextInterface<DBView, Reducers, SetReducerFlags>
  ) => void = undefined;
  constructor(
    private db: DbConnectionImpl<DBView, Reducers, SetReducerFlags>
  ) {}

  /**
   * Registers `callback` to run when this query is successfully added to our subscribed set,
   * I.e. when its `SubscriptionApplied` message is received.
   *
   * The database state exposed via the `&EventContext` argument
   * includes all the rows added to the client cache as a result of the new subscription.
   *
   * The event in the `&EventContext` argument is `Event::SubscribeApplied`.
   *
   * Multiple `on_applied` callbacks for the same query may coexist.
   * No mechanism for un-registering `on_applied` callbacks is exposed.
   *
   * @param cb - Callback to run when the subscription is applied.
   * @returns The current `SubscriptionBuilder` instance.
   */
  onApplied(
    cb: (
      ctx: SubscriptionEventContextInterface<DBView, Reducers, SetReducerFlags>
    ) => void
  ): SubscriptionBuilderImpl<DBView, Reducers, SetReducerFlags> {
    this.#onApplied = cb;
    return this;
  }

  /**
   * Registers `callback` to run when this query either:
   * - Fails to be added to our subscribed set.
   * - Is unexpectedly removed from our subscribed set.
   *
   * If the subscription had previously started and has been unexpectedly removed,
   * the database state exposed via the `&EventContext` argument contains no rows
   * from any subscriptions removed within the same error event.
   * As proposed, it must therefore contain no rows.
   *
   * The event in the `&EventContext` argument is `Event::SubscribeError`,
   * containing a dynamic error object with a human-readable description of the error
   * for diagnostic purposes.
   *
   * Multiple `on_error` callbacks for the same query may coexist.
   * No mechanism for un-registering `on_error` callbacks is exposed.
   *
   * @param cb - Callback to run when there is an error in subscription.
   * @returns The current `SubscriptionBuilder` instance.
   */
  onError(
    cb: (ctx: ErrorContextInterface<DBView, Reducers, SetReducerFlags>) => void
  ): SubscriptionBuilderImpl<DBView, Reducers, SetReducerFlags> {
    this.#onError = cb;
    return this;
  }

  /**
   * Subscribe to a single query. The results of the query will be merged into the client
   * cache and deduplicated on the client.
   *
   * @param query_sql A `SQL` query to subscribe to.
   *
   * @example
   *
   * ```ts
   * const subscription = connection.subscriptionBuilder().onApplied(() => {
   *   console.log("SDK client cache initialized.");
   * }).subscribe("SELECT * FROM User");
   *
   * subscription.unsubscribe();
   * ```
   */
  subscribe(
    query_sql: string
  ): SubscriptionHandleImpl<DBView, Reducers, SetReducerFlags> {
    return new SubscriptionHandleImpl(
      this.db,
      query_sql,
      this.#onApplied,
      this.#onError
    );
  }

  /**
   * Subscribes to all rows from all tables.
   *
   * This method is intended as a convenience
   * for applications where client-side memory use and network bandwidth are not concerns.
   * Applications where these resources are a constraint
   * should register more precise queries via `subscribe`
   * in order to replicate only the subset of data which the client needs to function.
   *
   * This method should not be combined with `subscribe` on the same `DbConnection`.
   * A connection may either `subscribe` to particular queries,
   * or `subscribeToAllTables`, but not both.
   * Attempting to call `subscribe`
   * on a `DbConnection` that has previously used `subscribeToAllTables`,
   * or vice versa, may misbehave in any number of ways,
   * including dropping subscriptions, corrupting the client cache, or throwing errors.
   */
  subscribeToAllTables(): void {
    this.subscribe('SELECT * FROM *');
  }
}

export type SubscribeEvent = 'applied' | 'error' | 'end';

export class SubscriptionManager {
  subscriptions: Map<
    number,
    { handle: SubscriptionHandleImpl; emitter: EventEmitter<SubscribeEvent> }
  > = new Map();
}

export class SubscriptionHandleImpl<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
> {
  #queryId: number;
  #unsubscribeCalled: boolean = false;
  #endedState: boolean = false;
  #activeState: boolean = false;
  #emitter: EventEmitter<SubscribeEvent, (...args: any[]) => void> =
    new EventEmitter();

  constructor(
    private db: DbConnectionImpl<DBView, Reducers, SetReducerFlags>,
    querySql: string,
    onApplied?: (
      ctx: SubscriptionEventContextInterface<DBView, Reducers, SetReducerFlags>
    ) => void,
    private onError?: (
      ctx: ErrorContextInterface<DBView, Reducers, SetReducerFlags>,
      error: Error
    ) => void
  ) {
    this.#emitter.on(
      'applied',
      (
        ctx: SubscriptionEventContextInterface<
          DBView,
          Reducers,
          SetReducerFlags
        >
      ) => {
        this.#activeState = true;
        if (onApplied) {
          onApplied(ctx);
        }
      }
    );
    this.#emitter.on(
      'error',
      (
        ctx: ErrorContextInterface<DBView, Reducers, SetReducerFlags>,
        error: Error
      ) => {
        if (this.onError) {
          this.onError(ctx, error);
        }
      }
    );
    this.#queryId = this.db.registerSubscription(this, this.#emitter, querySql);
  }

  /**
   * Consumes self and issues an `Unsubscribe` message,
   * removing this query from the client's set of subscribed queries.
   * It is only valid to call this method if `is_active()` is `true`.
   */
  unsubscribe(): void {
    if (this.#unsubscribeCalled) {
      throw new Error('Unsubscribe has already been called');
    }
    this.#unsubscribeCalled = true;
    this.db.unregisterSubscription(this.#queryId);
    this.db['unsubscribe'](this.#queryId);
  }

  /**
   * Unsubscribes and also registers a callback to run upon success.
   * I.e. when an `UnsubscribeApplied` message is received.
   *
   * If `Unsubscribe` returns an error,
   * or if the `on_error` callback(s) are invoked before this subscription would end normally,
   * the `on_end` callback is not invoked.
   *
   * @param onEnd - Callback to run upon successful unsubscribe.
   */
  unsubscribeThen(
    onEnd: (
      ctx: SubscriptionEventContextInterface<DBView, Reducers, SetReducerFlags>
    ) => void
  ): void {
    if (this.#endedState) {
      throw new Error('Subscription has already ended');
    }
    if (this.#unsubscribeCalled) {
      throw new Error('Unsubscribe has already been called');
    }
    this.#unsubscribeCalled = true;
    this.#emitter.on(
      'end',
      (
        ctx: SubscriptionEventContextInterface<
          DBView,
          Reducers,
          SetReducerFlags
        >
      ) => {
        this.#endedState = true;
        this.#activeState = false;
        onEnd(ctx);
      }
    );
  }

  /**
   * True if this `SubscriptionHandle` has ended,
   * either due to an error or a call to `unsubscribe`.
   *
   * This is initially false, and becomes true when either the `on_end` or `on_error` callback is invoked.
   * A subscription which has not yet been applied is not active, but is also not ended.
   */
  isEnded(): boolean {
    return this.#endedState;
  }

  /**
   * True if this `SubscriptionHandle` is active, meaning it has been successfully applied
   * and has not since ended, either due to an error or a complete `unsubscribe` request-response pair.
   *
   * This corresponds exactly to the interval bounded at the start by the `on_applied` callback
   * and at the end by either the `on_end` or `on_error` callback.
   */
  isActive(): boolean {
    return this.#activeState;
  }
}
