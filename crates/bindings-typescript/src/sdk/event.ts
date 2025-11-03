import type { ReducerEvent } from './reducer_event';
import type { UntypedReducerDef } from './reducers';

export type Event<Reducer extends UntypedReducerDef> =
  | { tag: 'Reducer'; value: ReducerEvent<Reducer> }
  | { tag: 'SubscribeApplied' }
  | { tag: 'UnsubscribeApplied' }
  | { tag: 'Error'; value: Error }
  | { tag: 'UnknownTransaction' };
