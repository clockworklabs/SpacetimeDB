import { ConnectionId } from '../';
import type { UpdateStatus } from './client_api/index.ts';
import { Identity } from '../';
import type { TableUpdate } from './table_cache.ts';
import { Timestamp } from '../';

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
    | SubscribeAppliedMessage<RowType>
    | UnsubscribeAppliedMessage<RowType>
    | SubscriptionError;
