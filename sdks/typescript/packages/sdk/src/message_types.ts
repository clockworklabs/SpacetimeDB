import { ConnectionId } from './connection_id';
import type { UpdateStatus } from './client_api/index.ts';
import { Identity } from './identity.ts';
import type { TableUpdate } from './table_cache.ts';
import { Timestamp } from './timestamp.ts';

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
    originalReducerName: string;
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

export type Message =
  | InitialSubscriptionMessage
  | TransactionUpdateMessage
  | TransactionUpdateLightMessage
  | IdentityTokenMessage;
