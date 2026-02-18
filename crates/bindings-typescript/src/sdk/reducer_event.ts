import { type Infer } from '../';
import { Timestamp } from '../';
import type { ReducerOutcome } from './client_api/types';
import type { ReducerEventInfo } from './reducers.ts';

export type ReducerEvent<Reducer extends ReducerEventInfo> = {
  /**
   * The time when the reducer started running.
   *
   * @internal This is a number and not Date, as JSON.stringify with date in it gives number, but JSON.parse of the same string does not give date. TO avoid
   * confusion in typing we'll keep it a number
   */
  timestamp: Timestamp;

  /**
   * The reducer outcome, including optional return value and updates.
   */
  outcome: Infer<typeof ReducerOutcome>;

  reducer: Reducer;
};
