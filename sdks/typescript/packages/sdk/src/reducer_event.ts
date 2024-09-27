import { Address } from './address.ts';
import type { Timestamp } from './client_api.ts';
import { Identity } from './identity.ts';

/**
 * Indicates the status of the reducer execution.
 * Whether the reducer committed, was aborted due to insufficient energy, or failed with an error message.
 */
export type ReducerEventStatus =
  | { type: 'Committed' }
  | { type: 'AbortedDueToInsufficientEnergy' }
  | { type: 'FailedWithErrorMessage'; message: string };

export class ReducerEvent<ReducerEnum> {
  /**
   * The time when the reducer started running.
   *
   * @internal This is a number and not Date, as JSON.stringify with date in it gives number, but JSON.parse of the same string does not give date. TO avoid
   * confusion in typing we'll keep it a number
   */
  timestamp: Timestamp;

  /**
   * Whether the reducer committed, was aborted due to insufficient energy, or failed with an error message.
   */
  status: ReducerEventStatus;

  /**
   * The identity of the caller.
   * TODO: Revise these to reflect the forthcoming Identity proposal.
   */
  callerIdentity: Identity;

  /**
   * The address of the caller.
   */
  callerAddress?: Address;

  /**
   * The amount of energy consumed by the reducer run, in eV.
   * (Not literal eV, but our SpacetimeDB energy unit eV.)
   * May be present or undefined at the implementor's discretion;
   * future work may determine an interface for module developers
   * to request this value be published or hidden.
   */
  energyConsumed?: bigint;

  /**
   * The `Reducer` enum defined by the `module_bindings`, which encodes which reducer ran and its arguments.
   */
  reducer: ReducerEnum;

  constructor({
    callerIdentity,
    callerAddress,
    status,
    timestamp,
    energyConsumed,
    reducer,
  }: {
    callerIdentity: Identity;
    status: ReducerEventStatus;
    message: string;
    callerAddress?: Address;
    timestamp: Timestamp;
    energyConsumed?: bigint;
    reducer: ReducerEnum;
  }) {
    this.callerIdentity = callerIdentity;
    this.callerAddress = callerAddress;
    this.status = status;
    this.timestamp = timestamp;
    this.energyConsumed = energyConsumed;
    this.reducer = reducer;
  }
}
