import type { Address } from './address.ts';
import type { Identity } from './identity.ts';
import type { ReducerEvent } from './reducer_event.ts';

/**
 * Event when we are notified that a reducer ran in the remote module.
 *
 * This event is passed to reducer callbacks,
 * and to row callbacks resulting from modifications by the reducer.
 */
interface ReducerEventVariant<R> {
  type: 'Reducer';
  data: ReducerEvent<R>;
}

/**
 * Event when one of our subscriptions is applied.
 *
 * This event is passed to subscription-applied callbacks,
 * and to row insert callbacks resulting from the new subscription.
 */
interface SubscribeAppliedEvent {
  type: 'SubscribeApplied';
}

/**
 * Event when one of our subscriptions is removed.
 *
 * This event is passed to unsubscribe-applied callbacks,
 * and to row delete callbacks resulting from the ended subscription.
 */
interface UnsubscribeAppliedEvent {
  type: 'UnsubscribeApplied';
}

/**
 * Event when an error causes one or more of our subscriptions to end prematurely,
 * or to never be started.
 *
 * Payload should be a language-appropriate dynamic error type,
 * likely `Exception` in C# and `Error` in TypeScript.
 *
 * Payload should describe the error in a human-readable format.
 * No requirement is imposed that it be programmatically inspectable.
 */
interface SubscribeErrorEvent {
  type: 'SubscribeError';
  error: Error;
}

/**
 * Event when we are notified of a transaction in the remote module which we cannot associate with a known reducer.
 *
 * This may be an ad-hoc SQL query or a reducer for which we do not have bindings.
 *
 * This event is passed to row callbacks resulting from modifications by the transaction.
 */
interface UnknownTransactionEvent {
  type: 'UnknownTransaction';
}

// To be exported as Event, to avoid clashing with the Event type from the libdom
export type STDBEvent<R> =
  | ReducerEventVariant<R>
  | SubscribeAppliedEvent
  | UnsubscribeAppliedEvent
  | SubscribeErrorEvent
  | UnknownTransactionEvent;
