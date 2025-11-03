import type { UntypedSchemaDef } from '../server/schema.ts';
import type { DbContext } from './db_context';
import type { Event } from './event.ts';
import type { ReducerEvent } from './reducer_event.ts';
import type { UntypedReducersDef } from './reducers.ts';

export type UntypedEventContext = EventContextInterface<UntypedSchemaDef, UntypedReducersDef>;

export interface EventContextInterface<
  SchemaDef extends UntypedSchemaDef,
  Reducers extends UntypedReducersDef,
> extends DbContext<SchemaDef, Reducers> {
  /** Enum with variants for all possible events. */
  event: Event<Reducers['reducers'][number]>;
}

export interface ReducerEventContextInterface<
  SchemaDef extends UntypedSchemaDef,
  Reducers extends UntypedReducersDef,
> extends DbContext<SchemaDef, Reducers> {
  /** Enum with variants for all possible events. */
  event: ReducerEvent<Reducers['reducers'][number]>;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface SubscriptionEventContextInterface<
  SchemaDef extends UntypedSchemaDef,
  Reducers extends UntypedReducersDef,
> extends DbContext<SchemaDef, Reducers> {
  /** No event is provided **/
}

export interface ErrorContextInterface<
  SchemaDef extends UntypedSchemaDef,
  Reducers extends UntypedReducersDef,
> extends DbContext<SchemaDef, Reducers> {
  /** Enum with variants for all possible events. */
  event?: Error;
}
