import type { TableUpdate } from './table_cache.ts';
import type { UntypedTableDecl } from '../lib/table.ts';
import type { ReducerOutcome } from './client_api/types';

export type TransactionUpdateMessage = {
  tag: 'TransactionUpdate';
  tableUpdates: TableUpdate<UntypedTableDecl>[];
};

export type SubscribeAppliedMessage = {
  tag: 'SubscribeApplied';
  querySetId: number;
  tableUpdates: TableUpdate<UntypedTableDecl>[];
};

export type UnsubscribeAppliedMessage = {
  tag: 'UnsubscribeApplied';
  querySetId: number;
  tableUpdates: TableUpdate<UntypedTableDecl>[];
};

export type SubscriptionError = {
  tag: 'SubscriptionError';
  querySetId: number;
  error: string;
};

export type ReducerResultMessage = {
  tag: 'ReducerResult';
  requestId: number;
  result: ReducerOutcome;
};

export type ProcedureResultMessage = {
  tag: 'ProcedureResult';
  requestId: number;
  result: { tag: 'Ok'; value: Uint8Array } | { tag: 'Err'; value: string };
};

export type Message =
  | TransactionUpdateMessage
  | SubscribeAppliedMessage
  | UnsubscribeAppliedMessage
  | SubscriptionError
  | ReducerResultMessage
  | ProcedureResultMessage;
