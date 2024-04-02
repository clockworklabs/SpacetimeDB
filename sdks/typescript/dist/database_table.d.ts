import { ClientDB } from "./client_db";
import { ReducerEvent } from "./reducer_event";
import { SpacetimeDBClient } from "./spacetimedb";
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
export declare class DatabaseTable {
    static db?: ClientDB;
    static tableName: string;
    static with<T extends DatabaseTable>(this: T, client: SpacetimeDBClient): T;
    static getDB(): ClientDB;
    static count(): number;
    static all<T extends DatabaseTable>(this: ThisDatabaseType<T>): T[];
    static onInsert<T extends DatabaseTable>(this: ThisDatabaseType<T>, callback: (value: T, reducerEvent: ReducerEvent | undefined) => void): void;
    static onUpdate<T extends DatabaseTable>(this: ThisDatabaseType<T>, callback: (oldValue: T, newValue: T, reducerEvent: ReducerEvent | undefined) => void): void;
    static onDelete<T extends DatabaseTable>(this: ThisDatabaseType<T>, callback: (value: T, reducerEvent: ReducerEvent | undefined) => void): void;
    static removeOnInsert<T extends DatabaseTable>(this: ThisDatabaseType<T>, callback: (value: T, reducerEvent: ReducerEvent | undefined) => void): void;
    static removeOnUpdate<T extends DatabaseTable>(this: ThisDatabaseType<T>, callback: (oldValue: T, newValue: T, reducerEvent: ReducerEvent | undefined) => void): void;
    static removeOnDelete<T extends DatabaseTable>(this: ThisDatabaseType<T>, callback: (value: T, reducerEvent: ReducerEvent | undefined) => void): void;
}
export {};
