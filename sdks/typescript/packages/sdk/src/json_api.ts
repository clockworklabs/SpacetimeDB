export interface Message {
  IdentityToken?: IdentityToken | undefined;
  SubscriptionUpdate?: SubscriptionUpdate | undefined;
  TransactionUpdate?: TransactionUpdate | undefined;
}

export interface IdentityToken {
  identity: string;
  token: string;
  address: string;
}

export interface SubscriptionUpdate {
  table_updates: TableUpdate[];
}

export interface TableUpdate {
  table_id: number;
  table_name: string;
  table_row_operations: TableRowOperation[];
}

export interface TableRowOperation {
  op: 'insert' | 'delete';
  row: any[];
}

export interface TransactionUpdate {
  event: Event;
  subscription_update: SubscriptionUpdate;
}

export interface Event {
  timestamp: number;
  status: 'committed' | 'failed' | 'out_of_energy';
  caller_identity: string;
  caller_address: string;
  function_call: FunctionCall;
  energy_quanta_used: number;
  message: string;
}

export interface FunctionCall {
  reducer: string;
  args: string;
}
