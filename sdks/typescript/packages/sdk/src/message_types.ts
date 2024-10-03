import { Address } from './address.ts';
import type { Timestamp, UpdateStatus } from './client_api/index.ts';
import { Identity } from './identity.ts';
import type { TableUpdate } from './table_cache.ts';

export type InitialSubscriptionMessage = {
  tag: 'InitialSubscription';
  tableUpdates: TableUpdate[];
};

export type TransactionUpdateMessage = {
  tag: 'TransactionUpdate';
  tableUpdates: TableUpdate[];
  identity: Identity;
  address: Address | null;
  originalReducerName: string;
  reducerName: string;
  args: Uint8Array;
  status: UpdateStatus;
  message: string;
  timestamp: Timestamp;
  energyConsumed: bigint;
};

export type IdentityTokenMessage = {
  tag: 'IdentityToken';
  identity: Identity;
  token: string;
  address: Address;
};

export type Message =
  | InitialSubscriptionMessage
  | TransactionUpdateMessage
  | IdentityTokenMessage;
