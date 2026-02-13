import { type Infer } from '../';
import type { TableUpdate } from './table_cache.ts';
import type { UntypedTableDef } from '../lib/table.ts';
import type ReducerOutcome from './client_api/reducer_outcome_type.ts';

export type TransactionUpdateMessage = {
  tag: 'TransactionUpdate';
  tableUpdates: TableUpdate<UntypedTableDef>[];
};

export type SubscribeAppliedMessage = {
  tag: 'SubscribeApplied';
  querySetId: number;
  tableUpdates: TableUpdate<UntypedTableDef>[];
};

export type UnsubscribeAppliedMessage = {
  tag: 'UnsubscribeApplied';
  querySetId: number;
  tableUpdates: TableUpdate<UntypedTableDef>[];
};

export type SubscriptionError = {
  tag: 'SubscriptionError';
  querySetId: number;
  error: string;
};

export type ReducerResultMessage = {
  tag: 'ReducerResult';
  requestId: number;
  result: Infer<typeof ReducerOutcome>;
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
