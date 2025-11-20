import type { ReducerEvent } from './reducer_event';
import type { ReducerEventInfo } from './reducers';

export type Event<Reducer extends ReducerEventInfo> =
  | { tag: 'Reducer'; value: ReducerEvent<Reducer> }
  | { tag: 'SubscribeApplied' }
  | { tag: 'UnsubscribeApplied' }
  | { tag: 'Error'; value: Error }
  | { tag: 'UnknownTransaction' };
