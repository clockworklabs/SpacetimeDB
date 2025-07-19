import { ConnectionId, type Infer } from '../';
import { Identity } from '../';
import type { TableUpdate } from './table_cache.ts';
import { TimeDuration, Timestamp } from '../';
import type { UntypedTableDef } from '../lib/table.ts';
import type OneOffTable from './client_api/one_off_table_type.ts';
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

export type QueryResolvedMessage = {
  tag: 'QueryResolved';
  messageId: Uint8Array;
  error?: string;
  tables: Infer<typeof OneOffTable>[];
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

export type ProcedureResultMessage = {
  tag: 'ProcedureResult';
  requestId: number;
  result: { tag: 'Ok'; value: Uint8Array } | { tag: 'Err'; value: string };
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
  | SubscriptionError
  | ProcedureResultMessage;
