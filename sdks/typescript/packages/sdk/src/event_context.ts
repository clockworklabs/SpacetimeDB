import type { DBContext } from './db_context';
import type { Event } from './event.ts';

export interface EventContextInterface<
  DBView = any,
  Reducers = any,
  Reducer extends { name: string; args?: any } = { name: string; args?: any },
> extends DBContext<DBView, Reducers> {
  /** Enum with variants for all possible events. */
  event: Event<Reducer>;
}
