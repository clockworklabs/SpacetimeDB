import type { InferTypeOfRow } from '../lib/type_builders.ts';
import type { DbContext } from './db_context';
import type { Event } from './event.ts';
import type { ReducerEvent } from './reducer_event.ts';
import type { ReducerEventInfo } from './reducers.ts';
import type { UntypedRemoteModule } from './spacetime_module.ts';

export type UntypedEventContext = EventContextInterface<UntypedRemoteModule>;

export interface EventContextInterface<RemoteModule extends UntypedRemoteModule>
  extends DbContext<RemoteModule> {
  /** Enum with variants for all possible events. */
  event: Event<
    ReducerEventInfo<
      RemoteModule['reducers'][number]['name'],
      InferTypeOfRow<RemoteModule['reducers'][number]['params']>
    >
  >;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface QueryEventContextInterface<
  RemoteModule extends UntypedRemoteModule,
> extends DbContext<RemoteModule> {
  /** No event is provided **/
}

export interface ReducerEventContextInterface<
  RemoteModule extends UntypedRemoteModule,
> extends DbContext<RemoteModule> {
  /** Enum with variants for all possible events. */
  event: ReducerEvent<
    ReducerEventInfo<
      RemoteModule['reducers'][number]['name'],
      InferTypeOfRow<RemoteModule['reducers'][number]['params']>
    >
  >;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface ProcedureEventContextInterface<
  RemoteModule extends UntypedRemoteModule,
> extends DbContext<RemoteModule> {
  /** No event is provided */
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface SubscriptionEventContextInterface<
  RemoteModule extends UntypedRemoteModule,
> extends DbContext<RemoteModule> {
  /** No event is provided **/
}

export interface ErrorContextInterface<RemoteModule extends UntypedRemoteModule>
  extends DbContext<RemoteModule> {
  /** Enum with variants for all possible events. */
  event?: Error;
}
