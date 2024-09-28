import { ReducerEvent } from './reducer_event.ts';
import {
  DBConnectionBase,
  DbContext,
  type CallbackInit,
} from './spacetimedb.ts';
import { _tableProxy } from './utils.ts';

export class DatabaseTable<
  TableType,
  EventContext extends DbContext<any, any> = any,
> {
  tableName: string;

  #client: DBConnectionBase;

  constructor(client: DBConnectionBase, tableName: string) {
    this.#client = client;
    this.tableName = tableName;
  }

  static with<TableType, T extends DatabaseTable<TableType>>(
    this: T,
    client: DBConnectionBase
  ): T {
    return _tableProxy<T>(this, client) as unknown as T;
  }

  count(): number {
    return this.#client.db.getTable(this.tableName).count();
  }

  all<T extends DatabaseTable<TableType>>(): T[] {
    return this.#client.db.getTable(this.tableName).getInstances() as T[];
  }

  onInsert<T extends DatabaseTable<TableType>>(
    callback: (
      ctx: EventContext,
      value: T,
      reducerEvent: ReducerEvent | undefined
    ) => void,
    init?: CallbackInit
  ): void {
    this.#client.db.getTable(this.tableName).onInsert(callback, init);
  }

  onUpdate<T extends DatabaseTable<TableType>>(
    callback: (
      ctx: EventContext,
      oldValue: T,
      newValue: T,
      reducerEvent: ReducerEvent | undefined
    ) => void,
    init?: CallbackInit
  ): void {
    this.#client.db.getTable(this.tableName).onUpdate(callback, init);
  }

  onDelete<T extends DatabaseTable<TableType>>(
    callback: (
      ctx: EventContext,
      value: T,
      reducerEvent: ReducerEvent | undefined
    ) => void,
    init?: CallbackInit
  ): void {
    this.#client.db.getTable(this.tableName).onDelete(callback, init);
  }
}
