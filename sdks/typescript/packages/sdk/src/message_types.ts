import type { ConnectionId } from './connection_id';
import type { OneOffTable, UpdateStatus } from './client_api/index';
import type { Identity } from './identity';
import type { TableUpdate } from './table_cache';
import type { TimeDuration } from './time_duration';
import type { Timestamp } from './timestamp';

export type InitialSubscriptionMessage<RowType extends Record<string, any>> = {
  tag: 'InitialSubscription';
  tableUpdates: TableUpdate<RowType>[];
};

export type TransactionUpdateMessage<RowType extends Record<string, any>> = {
  tag: 'TransactionUpdate';
  tableUpdates: TableUpdate<RowType>[];
  identity: Identity;
  connectionId: ConnectionId | null;
  reducerInfo?: {
    reducerName: string;
    args: Uint8Array;
  };
  status: UpdateStatus;
  message: string;
  timestamp: Timestamp;
  energyConsumed: bigint;
};

export type TransactionUpdateLightMessage<RowType extends Record<string, any>> =
  {
    tag: 'TransactionUpdateLight';
    tableUpdates: TableUpdate<RowType>[];
  };

export type IdentityTokenMessage = {
  tag: 'IdentityToken';
  identity: Identity;
  token: string;
  connectionId: ConnectionId;
};

export type QueryResolvedMessage = {
  tag: 'QueryResolved';
  messageId: Uint8Array;
  error?: string;
  tables: OneOffTable[];
  totalHostExecutionDuration: TimeDuration;
};

export type QueryErrorMessage = {
  tag: 'QueryError';
  messageId?: Uint8Array;
  error: string;
};

export type SubscribeAppliedMessage<RowType extends Record<string, any>> = {
  tag: 'SubscribeApplied';
  queryId: number;
  tableUpdates: TableUpdate<RowType>[];
};

export type UnsubscribeAppliedMessage<RowType extends Record<string, any>> = {
  tag: 'UnsubscribeApplied';
  queryId: number;
  tableUpdates: TableUpdate<RowType>[];
};

export type SubscriptionError = {
  tag: 'SubscriptionError';
  queryId?: number;
  error: string;
};

export type Message<RowType extends Record<string, any> = Record<string, any>> =

    | InitialSubscriptionMessage<RowType>
    | TransactionUpdateMessage<RowType>
    | TransactionUpdateLightMessage<RowType>
    | IdentityTokenMessage
    | QueryResolvedMessage
    | QueryErrorMessage
    | SubscribeAppliedMessage<RowType>
    | UnsubscribeAppliedMessage<RowType>
    | SubscriptionError;
