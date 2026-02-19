import type { ReducerEvent } from './reducer_event';
import type { ReducerEventInfo } from './reducers';

type WithId = {
  /**
   * A client-generated id to distinguish between different events.
   */
  id: string;
};

export type Event<Reducer extends ReducerEventInfo> = WithId &
  (
    | { tag: 'Reducer'; value: ReducerEvent<Reducer> }
    | { tag: 'SubscribeApplied' }
    | { tag: 'UnsubscribeApplied' }
    | { tag: 'Error'; value: Error }
    | { tag: 'UnknownTransaction' }
  );
