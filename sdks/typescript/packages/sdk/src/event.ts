import type { Address } from './address.ts';
import type { Identity } from './identity.ts';

/**
/**
 * Indicates the status of the reducer execution.
 * Whether the reducer committed, was aborted due to insufficient energy, or failed with an error message.
 */
type Status =
  | { type: 'Committed' }
  | { type: 'AbortedDueToInsufficientEnergy' }
  | { type: 'FailedWithErrorMessage'; message: string };

/**
 * Event when we are notified that a reducer ran in the remote module.
 *
 * This event is passed to reducer callbacks,
 * and to row callbacks resulting from modifications by the reducer.
 */
interface ReducerEvent<R> {
  /**
   * Whether the reducer committed, was aborted due to insufficient energy, or failed with an error message.
   */
  status: Status;

  /**
   * The time when the reducer started running.
   * Should be a language-appropriate point-in-time type,
   * i.e., `DateTimeOffset` in C# or `Date` in TypeScript.
   */
  timestamp: Date;

  /**
   * The identity of the caller.
   * TODO: Revise these to reflect the forthcoming Identity proposal.
   */
  caller_identity: Identity;

  /**
   * The address of the caller.
   */
  caller_address?: Address;

  /**
   * The amount of energy consumed by the reducer run, in eV.
   * (Not literal eV, but our SpacetimeDB energy unit eV.)
   * May be present or undefined at the implementor's discretion;
   * future work may determine an interface for module developers
   * to request this value be published or hidden.
   */
  energy_consumed?: bigint;

  /**
   * The `Reducer` enum defined by the `module_bindings`, which encodes which reducer ran and its arguments.
   */
  reducer: R;
}

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
