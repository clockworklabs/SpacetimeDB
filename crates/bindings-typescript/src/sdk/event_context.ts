import type { InferTypeOfParams } from '../lib/type_builders.ts';
import type { DbContext } from './db_context';
import type { Event } from './event.ts';
import type { ReducerEvent } from './reducer_event.ts';
import type { ReducerEventInfo } from './reducers.ts';
import type { UntypedRemoteModuleDecl } from './spacetime_module.ts';

export type UntypedEventContext =
  EventContextInterface<UntypedRemoteModuleDecl>;

export interface EventContextInterface<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> extends DbContext<RemoteModuleDecl> {
  /** Enum with variants for all possible events. */
  event: Event<
    ReducerEventInfo<
      RemoteModuleDecl['reducers'][number]['name'],
      InferTypeOfParams<RemoteModuleDecl['reducers'][number]['params']>
    >
  >;
}

export interface ReducerEventContextInterface<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> extends DbContext<RemoteModuleDecl> {
  /** Enum with variants for all possible events. */
  event: ReducerEvent<
    ReducerEventInfo<
      RemoteModuleDecl['reducers'][number]['name'],
      InferTypeOfParams<RemoteModuleDecl['reducers'][number]['params']>
    >
  >;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface ProcedureEventContextInterface<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> extends DbContext<RemoteModuleDecl> {
  /** No event is provided */
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface SubscriptionEventContextInterface<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> extends DbContext<RemoteModuleDecl> {
  /** No event is provided **/
}

export interface ErrorContextInterface<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> extends DbContext<RemoteModuleDecl> {
  /** Enum with variants for all possible events. */
  event?: Error;
}
