type Result<T = undefined> =
  | {
      tag: 'Ok';
      value: T;
    }
  | {
      tag: 'Err';
      value: Error;
    };

/**
 * Interface representing a subscription handle for managing queries.
 */
interface SubscriptionHandle {
  /**
   * Consumes self and issues an `Unsubscribe` message,
   * removing this query from the client's set of subscribed queries.
   * It is only valid to call this method if `is_active()` is `true`.
   */
  unsubscribe(): Result;

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
  unsubscribeThen(onEnd: () => void): Result;

  /**
   * True if this `SubscriptionHandle` has ended,
   * either due to an error or a call to `unsubscribe`.
   *
   * This is initially false, and becomes true when either the `on_end` or `on_error` callback is invoked.
   * A subscription which has not yet been applied is not active, but is also not ended.
   */
  isEnded(): boolean;

  /**
   * True if this `SubscriptionHandle` is active, meaning it has been successfully applied
   * and has not since ended, either due to an error or a complete `unsubscribe` request-response pair.
   *
   * This corresponds exactly to the interval bounded at the start by the `on_applied` callback
   * and at the end by either the `on_end` or `on_error` callback.
   */
  isActive(): boolean;
}

/**
 * Interface representing a database context.
 *
 * @template DBView - Type representing the database view.
 * @template Reducers - Type representing the reducers.
 * @template SetReducerFlags - Type representing the reducer flags collection.
 */
export interface DBContext<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
> {
  db: DBView;
  reducers: Reducers;
  setReducerFlags: SetReducerFlags;
  isActive: boolean;

  /**
   * Disconnects from the database.
   */
  disconnect(): void;
}
