import { ConnectionId, type Infer } from '../';
import { Identity } from '../';
import type { TableUpdate } from './table_cache.ts';
import { Timestamp } from '../';
import type { UntypedTableDef } from '../lib/table.ts';
import type UpdateStatus from './client_api/update_status_type.ts';

export type InitialSubscriptionMessage = {
  tag: 'InitialSubscription';
  tableUpdates: TableUpdate<UntypedTableDef>[];
};

export type TransactionUpdateMessage = {
  tag: 'TransactionUpdate';
  tableUpdates: TableUpdate<UntypedTableDef>[];
  identity: Identity;
  connectionId: ConnectionId | null;
  reducerInfo?: {
    reducerName: string;
    args: Uint8Array;
  };
  status: Infer<typeof UpdateStatus>;
  message: string;
  timestamp: Timestamp;
  energyConsumed: bigint;
};

export type TransactionUpdateLightMessage = {
  tag: 'TransactionUpdateLight';
  tableUpdates: TableUpdate<UntypedTableDef>[];
};

export type IdentityTokenMessage = {
  tag: 'IdentityToken';
  identity: Identity;
  token: string;
  connectionId: ConnectionId;
};

export type SubscribeAppliedMessage = {
  tag: 'SubscribeApplied';
  queryId: number;
  tableUpdates: TableUpdate<UntypedTableDef>[];
};

export type UnsubscribeAppliedMessage = {
  tag: 'UnsubscribeApplied';
  queryId: number;
  tableUpdates: TableUpdate<UntypedTableDef>[];
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
  | SubscribeAppliedMessage
  | UnsubscribeAppliedMessage
  | SubscriptionError;
