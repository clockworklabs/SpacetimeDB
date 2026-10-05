import type { InferTypeOfParams } from '../lib/type_builders.ts';
import type { DbContext } from './db_context';
import type { Event } from './event.ts';
import type { ReducerEvent } from './reducer_event.ts';
import type { ReducerEventInfo } from './reducers.ts';
import type { UntypedRemoteModuleDecl } from './spacetime_module.ts';

export type UntypedEventContext = EventContextBase<UntypedRemoteModuleDecl>;

export interface EventContextBase<
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

export interface ReducerEventContextBase<
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
export interface ProcedureEventContextBase<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> extends DbContext<RemoteModuleDecl> {
  /** No event is provided */
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface SubscriptionEventContextBase<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> extends DbContext<RemoteModuleDecl> {
  /** No event is provided **/
}

export interface ErrorContextBase<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> extends DbContext<RemoteModuleDecl> {
  /** Enum with variants for all possible events. */
  event?: Error;
}

/** @deprecated Use `EventContextBase` instead. */
export type EventContextInterface<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> = EventContextBase<RemoteModuleDecl>;

/** @deprecated Use `ReducerEventContextBase` instead. */
export type ReducerEventContextInterface<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> = ReducerEventContextBase<RemoteModuleDecl>;

/**
 * @deprecated Use `ProcedureEventContextBase` instead. Kept so that
 * declaration files emitted against older versions of the SDK keep resolving.
 */
export type ProcedureEventContextInterface<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> = ProcedureEventContextBase<RemoteModuleDecl>;

/** @deprecated Use `SubscriptionEventContextBase` instead. */
export type SubscriptionEventContextInterface<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> = SubscriptionEventContextBase<RemoteModuleDecl>;

/** @deprecated Use `ErrorContextBase` instead. */
export type ErrorContextInterface<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
> = ErrorContextBase<RemoteModuleDecl>;
