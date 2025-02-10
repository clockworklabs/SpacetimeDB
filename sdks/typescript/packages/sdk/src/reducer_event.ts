import { ConnectionId } from './connection_id';
import { Timestamp } from './timestamp.ts';
import type { UpdateStatus } from './client_api/index.ts';
import { Identity } from './identity.ts';

export type ReducerInfoType = { name: string; args?: any } | never;

export type ReducerEvent<Reducer extends ReducerInfoType> = {
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
  status: UpdateStatus;

  /**
   * The identity of the caller.
   * TODO: Revise these to reflect the forthcoming Identity proposal.
   */
  callerIdentity: Identity;

  /**
   * The connection ID of the caller.
   *
   * May be `null`, e.g. for scheduled reducers.
   */
  callerConnectionId?: ConnectionId;

  /**
   * The amount of energy consumed by the reducer run, in eV.
   * (Not literal eV, but our SpacetimeDB energy unit eV.)
   * May be present or undefined at the implementor's discretion;
   * future work may determine an interface for module developers
   * to request this value be published or hidden.
   */
  energyConsumed?: bigint;

  reducer: Reducer;
};
