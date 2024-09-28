import type { DbContext } from './db_context.ts';
import { DBConnectionBase } from './spacetimedb.ts';
import type { CallbackInit } from './types.ts';
export class Reducer<
  Args extends Array<string> = [],
  DBView = {},
  ReducerView = {},
  EventContext = DbContext<DBView, ReducerView>,
  ReducerEnum = {},
> {
  reducerName: string;

  client: DBConnectionBase<ReducerEnum>;

  constructor(client: DBConnectionBase<ReducerEnum>, reducerName: string) {
    this.reducerName = reducerName;
    this.client = client;
  }

  call(..._args: any[]): void {
    throw 'not implemented';
  }

  on(
    callback: (ctx: EventContext, ...args: Args) => void,
    init?: CallbackInit
  ): void {
    this.client.on('reducer:' + this.reducerName, callback);

    if (init?.signal) {
      init.signal.addEventListener('abort', () => {
        this.client.off(`reducer:${this.reducerName}`, callback);
      });
    }
  }
}
