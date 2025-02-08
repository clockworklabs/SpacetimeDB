import type { DBConnectionImpl } from './db_connection_impl';
import type {
  ErrorContextInterface,
  SubscriptionEventContextInterface,
} from './event_context';

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
    private db: DBConnectionImpl<DBView, Reducers, SetReducerFlags>
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
  ): SubscriptionBuilderImpl {
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
  ): SubscriptionBuilderImpl {
    this.#onError = cb;
    return this;
  }

  /**
   * Issues a new `Subscribe` message,
   * adding `query` to the client's set of subscribed queries.
   *
   * `query` should be a single SQL `SELECT` statement.
   *
   * Installs the above callbacks into the new `SubscriptionHandle`,
   * before issuing the `Subscribe` message, to avoid race conditions.
   *
   * Consumes the `SubscriptionBuilder`,
   * because the callbacks are not necessarily `Clone`.
   *
   * @param query_sql - The SQL query to subscribe to.
   */
  subscribe(query_sql: string[]): void {
    this.db['subscribe'](query_sql, this.#onApplied, this.#onError);
  }
}
