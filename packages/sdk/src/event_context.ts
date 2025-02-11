import type { DBContext } from './db_context';
import type { Event } from './event.ts';
import type { ReducerEvent, ReducerInfoType } from './reducer_event.ts';

export interface EventContextInterface<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
  Reducer extends ReducerInfoType = never,
> extends DBContext<DBView, Reducers, SetReducerFlags> {
  /** Enum with variants for all possible events. */
  event: Event<Reducer>;
}

export interface ReducerEventContextInterface<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
  Reducer extends ReducerInfoType = never,
> extends DBContext<DBView, Reducers, SetReducerFlags> {
  /** Enum with variants for all possible events. */
  event: ReducerEvent<Reducer>;
}

export interface SubscriptionEventContextInterface<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
> extends DBContext<DBView, Reducers, SetReducerFlags> {
  /** No event is provided **/
}

export interface ErrorContextInterface<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
> extends DBContext<DBView, Reducers, SetReducerFlags> {
  /** Enum with variants for all possible events. */
  event?: Error;
}
