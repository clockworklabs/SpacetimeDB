export type EventType =
  | 'update'
  | 'insert'
  | 'delete'
  | 'initialStateSync'
  | 'connected'
  | 'disconnected'
  | 'client_error';

export type CallbackInit = {
  signal?: AbortSignal;
};
