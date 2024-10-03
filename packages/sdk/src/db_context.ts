import type { DBConnectionImpl } from './db_connection_impl.ts';
import type { EventContextInterface } from './event_context.ts';

type Result<T = undefined> =
  | {
      tag: 'Ok';
      value: T;
    }
  | {
      tag: 'Err';
      value: Error;
    };

interface SubscriptionHandle {
  /// Consumes self and issues an `Unsubscribe` message,
  /// removing this query from the client's set of subscribed queries.
  /// It is only valid to call this method if `is_active()` is `true`.
  unsubscribe(): Result;

  /// `Unsubscribe`s and also registers a callback to run upon success.
  /// I.e. when an `UnsubscribeApplied` message is received.
  ///
  /// If `Unsubscribe` returns an error,
  /// or if the `on_error` callback(s) are invoked before this subscription would end normally,
  /// the `on_end` callback is not invoked.
  unsubscribeThen(onEnd: () => void): Result;

  /// True if this `SubscriptionHandle` has ended,
  /// either due to an error or a call to `unsubscribe`.
  ///
  /// This is initially false, and becomes true when either the `on_end` or `on_error` callback is invoked.
  /// A subscription which has not yet been applied is not active, but is also not ended.
  isEnded(): boolean;

  /// True if this `SubscriptionHandle` is active, meaning it has been successfully applied
  /// and has not since ended, either due to an error or a complete `unsubscribe` request-response pair.
  ///
  /// This corresponds exactly to the interval bounded at the start by the `on_applied` callback
  /// and at the end by either the `on_end` or `on_error` callback.
  isActive(): boolean;
}

export class SubscriptionBuilder {
  #onApplied?: (ctx: EventContextInterface) => void = undefined;
  #onError?: (ctx: EventContextInterface) => void = undefined;

  constructor(private db: DBConnectionImpl) {}

  /// Registers `callback` to run when this query is successfully added to our subscribed set,
  /// I.e. when its `SubscriptionApplied` message is received.
  ///
  /// The database state exposed via the `&EventContext` argument
  /// includes all the rows added to the client cache as a result of the new subscription.
  ///
  /// The event in the `&EventContext` argument is `Event::SubscribeApplied`.
  ///
  /// Multiple `on_applied` callbacks for the same query may coexist.
  /// No mechanism for un-registering `on_applied` callbacks is exposed.
  onApplied(cb: (ctx: EventContextInterface) => void): SubscriptionBuilder {
    this.#onApplied = cb;
    return this;
  }

  /// Registers `callback` to run when this query either:
  /// - Fails to be added to our subscribed set.
  /// - Is unexpectedly removed from our subscribed set.
  ///
  /// If the subscription had previously started and has been unexpectedly removed,
  /// the database state exposed via the `&EventContext` argument contains no rows
  /// from any subscriptions removed within the same error event.
  /// As proposed, it must therefore contain no rows.
  ///
  /// The event in the `&EventContext` argument is `Event::SubscribeError`,
  /// containing a dynamic error object with a human-readable description of the error
  /// for diagnostic purposes.
  ///
  /// Multiple `on_error` callbacks for the same query may coexist.
  /// No mechanism for un-registering `on_error` callbacks is exposed.
  onError(cb: (ctx: EventContextInterface) => void): SubscriptionBuilder {
    this.#onError = cb;
    return this;
  }

  /// Issues a new `Subscribe` message,
  /// adding `query` to the client's set of subscribed queries.
  ///
  /// `query` should be a single SQL `SELECT` statement.
  ///
  /// Installs the above callbacks into the new `SubscriptionHandle`,
  /// before issuing the `Subscribe` message, to avoid race conditions.
  ///
  /// Consumes the `SubscriptionBuilder`,
  /// because the callbacks are not necessarily `Clone`.
  subscribe(query_sql: string[]): void {
    this.db.subscribe(query_sql, this.#onApplied, this.#onError);
  }
}

export interface DBContext<DBView = any, Reducers = any> {
  db: DBView;
  reducers: Reducers;
  isActive: boolean;
  disconnect(): void;
  subscriptionBuilder(): SubscriptionBuilder;
}
