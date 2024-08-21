import { ClientDB } from './client_db.ts';
import { ReducerEvent } from './reducer_event.ts';
import { SpacetimeDBClient } from './spacetimedb.ts';
import { _tableProxy } from './utils.ts';

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
  static db?: ClientDB;
  static tableName: string;

  static with<T extends DatabaseTable>(this: T, client: SpacetimeDBClient): T {
    return _tableProxy<T>(this, client) as unknown as T;
  }

  static getDB(): ClientDB {
    if (!this.db) {
      throw "You can't query the database without creating a client first";
    }

    return this.db;
  }

  static count(): number {
    return this.getDB().getTable(this.tableName).count();
  }

  static all<T extends DatabaseTable>(this: ThisDatabaseType<T>): T[] {
    return this.getDB()
      .getTable(this.tableName)
      .getInstances() as unknown as T[];
  }

  static onInsert<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (value: T, reducerEvent: ReducerEvent | undefined) => void
  ): void {
    this.getDB().getTable(this.tableName).onInsert(callback);
  }

  static onUpdate<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (
      oldValue: T,
      newValue: T,
      reducerEvent: ReducerEvent | undefined
    ) => void
  ): void {
    this.getDB().getTable(this.tableName).onUpdate(callback);
  }

  static onDelete<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (value: T, reducerEvent: ReducerEvent | undefined) => void
  ): void {
    this.getDB().getTable(this.tableName).onDelete(callback);
  }

  static removeOnInsert<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (value: T, reducerEvent: ReducerEvent | undefined) => void
  ): void {
    this.getDB().getTable(this.tableName).removeOnInsert(callback);
  }

  static removeOnUpdate<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (
      oldValue: T,
      newValue: T,
      reducerEvent: ReducerEvent | undefined
    ) => void
  ): void {
    this.getDB().getTable(this.tableName).removeOnUpdate(callback);
  }

  static removeOnDelete<T extends DatabaseTable>(
    this: ThisDatabaseType<T>,
    callback: (value: T, reducerEvent: ReducerEvent | undefined) => void
  ): void {
    this.getDB().getTable(this.tableName).removeOnDelete(callback);
  }
}
