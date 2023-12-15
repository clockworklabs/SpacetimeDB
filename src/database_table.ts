import { ReducerEvent } from "./reducer_event";
import { ClientDB, SpacetimeDBClient } from "./spacetimedb";
import { _tableProxy } from "./utils";

export type DatabaseTableClass = {
  new (...args: any[]): any;
  db?: ClientDB;
  tableName: string;
};

type ThisDatabaseType<T extends DatabaseTable> = {
  new (...args: any): T;
  tableName: string;
  getDB: () => ClientDB;
};

export class DatabaseTable {
  public static db?: ClientDB;
  public static tableName: string;

  public static with<T extends DatabaseTable>(
    this: T,
    client: SpacetimeDBClient
  ): T {
    return _tableProxy<T>(this, client) as unknown as T;
  }

  public static getDB(): ClientDB {
    if (!this.db) {
      throw "You can't query the database without creating a client first";
    }

    return this.db;
  }

  public static count(): number {
    return this.getDB().getTable(this.tableName).count();
  }

  public static all<T extends DatabaseTable>(this: ThisDatabaseType<T>): T[] {
    return this.getDB()
      .getTable(this.tableName)
      .getInstances() as unknown as T[];
  }

  public static onInsert<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (value: T, reducerEvent: ReducerEvent | undefined) => void
  ) {
    this.getDB().getTable(this.tableName).onInsert(callback);
  }

  public static onUpdate<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (
      oldValue: T,
      newValue: T,
      reducerEvent: ReducerEvent | undefined
    ) => void
  ) {
    this.getDB().getTable(this.tableName).onUpdate(callback);
  }

  public static onDelete<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (value: T, reducerEvent: ReducerEvent | undefined) => void
  ) {
    this.getDB().getTable(this.tableName).onDelete(callback);
  }

  public static removeOnInsert<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (value: T, reducerEvent: ReducerEvent | undefined) => void
  ) {
    this.getDB().getTable(this.tableName).removeOnInsert(callback);
  }

  public static removeOnUpdate<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (
      oldValue: T,
      newValue: T,
      reducerEvent: ReducerEvent | undefined
    ) => void
  ) {
    this.getDB().getTable(this.tableName).removeOnUpdate(callback);
  }

  public static removeOnDelete<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (value: T, reducerEvent: ReducerEvent | undefined) => void
  ) {
    this.getDB().getTable(this.tableName).removeOnDelete(callback);
  }
}
