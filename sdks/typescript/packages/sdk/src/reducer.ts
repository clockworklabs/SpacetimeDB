import type { DbContext } from './db_context.ts';
import { DBConnectionBase } from './spacetimedb.ts';
import type { CallbackInit } from './types.ts';
export class Reducer<
  Args extends Array<string> = [],
  DBView = {},
  ReducerView = {},
  EventContext = DbContext<DBView, ReducerView>,
> {
  reducerName: string;

  client: DBConnectionBase;

  constructor(client: DBConnectionBase, reducerName: string) {
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

  static with<T extends typeof Reducer>(
    client: DBConnectionBase,
    reducerName: string
  ): InstanceType<T> {
    return new this(client, reducerName) as InstanceType<T>;
  }
}
