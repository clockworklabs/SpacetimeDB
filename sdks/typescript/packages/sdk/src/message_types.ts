import type { ConnectionId } from './connection_id';
import type { OneOffTable, UpdateStatus } from './client_api/index.ts';
import type { Identity } from './identity.ts';
import type { TableUpdate } from './table_cache.ts';
import type { TimeDuration } from './time_duration.ts';
import type { Timestamp } from './timestamp.ts';

export type InitialSubscriptionMessage = {
  tag: 'InitialSubscription';
  tableUpdates: TableUpdate[];
};

export type TransactionUpdateMessage = {
  tag: 'TransactionUpdate';
  tableUpdates: TableUpdate[];
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

export type TransactionUpdateLightMessage = {
  tag: 'TransactionUpdateLight';
  tableUpdates: TableUpdate[];
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

export type SubscribeAppliedMessage = {
  tag: 'SubscribeApplied';
  queryId: number;
  tableUpdates: TableUpdate[];
};

export type UnsubscribeAppliedMessage = {
  tag: 'UnsubscribeApplied';
  queryId: number;
  tableUpdates: TableUpdate[];
};

export type SubscriptionError = {
  tag: 'SubscriptionError';
  queryId?: number;
  error: string;
};

export type Message =
  | InitialSubscriptionMessage
  | TransactionUpdateMessage
  | TransactionUpdateLightMessage
  | IdentityTokenMessage
  | QueryResolvedMessage
  | QueryErrorMessage
  | SubscribeAppliedMessage
  | UnsubscribeAppliedMessage
  | SubscriptionError;
