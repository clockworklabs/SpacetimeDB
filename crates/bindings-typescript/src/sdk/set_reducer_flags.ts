import type { CallReducerFlags } from './db_connection_impl';

export type UntypedSetReducerFlags = Record<
  string,
  (flags: CallReducerFlags) => void
>;
