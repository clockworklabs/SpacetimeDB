import type { ReducerEvent } from './reducer_event';

export type Event<
  Reducer extends { name: string; args?: any } = { name: string; args?: any },
> =
  | { tag: 'Reducer'; value: ReducerEvent<Reducer> }
  | { tag: 'SubscribeApplied' }
  | { tag: 'UnsubscribeApplied' }
  | { tag: 'Error'; value: Error }
  | { tag: 'UnknownTransaction' };
