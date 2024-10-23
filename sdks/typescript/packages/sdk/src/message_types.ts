import { Address } from './address.ts';
import type {
  Timestamp,
  UpdateStatus,
  IdentityToken,
  IdsToNames,
} from './client_api/index.ts';
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
  reducerId: number;
  args: Uint8Array;
  status: UpdateStatus;
  message: string;
  timestamp: Timestamp;
  energyConsumed: bigint;
};

export type TransactionUpdateLightMessage = {
  tag: 'TransactionUpdateLight';
  tableUpdates: TableUpdate[];
};

export type AfterConnectingMessage = {
  tag: 'AfterConnecting';
  identityToken: IdentityToken;
  idsToNames: IdsToNames;
};

export type Message =
  | InitialSubscriptionMessage
  | TransactionUpdateMessage
  | TransactionUpdateLightMessage
  | AfterConnectingMessage;
