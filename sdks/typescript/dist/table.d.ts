/// <reference types="node" />
import { EventEmitter } from "events";
import { DatabaseTable } from "./spacetimedb";
import { ReducerEvent } from "./reducer_event";
declare class DBOp {
    type: "insert" | "delete";
    instance: any;
    rowPk: string;
    constructor(type: "insert" | "delete", rowPk: string, instance: any);
}
export declare class TableOperation {
    /**
     * The type of CRUD operation.
     *
     * NOTE: An update is a `delete` followed by a 'insert' internally.
     */
    type: "insert" | "delete";
    rowPk: string;
    row: Uint8Array | any;
    constructor(type: "insert" | "delete", rowPk: string, row: Uint8Array | any);
}
export declare class TableUpdate {
    tableName: string;
    operations: TableOperation[];
    constructor(tableName: string, operations: TableOperation[]);
}
/**
 * Builder to generate calls to query a `table` in the database
 */
export declare class Table {
    name: string;
    instances: Map<string, DatabaseTable>;
    emitter: EventEmitter;
    private entityClass;
    pkCol?: number;
    /**
     * @param name the table name
     * @param pkCol column designated as `#[primarykey]`
     * @param entityClass the entityClass
     */
    constructor(name: string, pkCol: number | undefined, entityClass: any);
    /**
     * @returns number of entries in the table
     */
    count(): number;
    /**
     * @returns The values of the entries in the table
     */
    getInstances(): any[];
    applyOperations: (protocol: "binary" | "json", operations: TableOperation[], reducerEvent: ReducerEvent | undefined) => void;
    update: (newDbOp: DBOp, oldDbOp: DBOp, reducerEvent: ReducerEvent | undefined) => void;
    insert: (dbOp: DBOp, reducerEvent: ReducerEvent | undefined) => void;
    delete: (dbOp: DBOp, reducerEvent: ReducerEvent | undefined) => void;
    /**
     * Register a callback for when a row is newly inserted into the database.
     *
     * ```ts
     * User.onInsert((user, reducerEvent) => {
     *   if (reducerEvent) {
     *      console.log("New user on reducer", reducerEvent, user);
     *   } else {
     *      console.log("New user received during subscription update on insert", user);
     *  }
     * });
     * ```
     *
     * @param cb Callback to be called when a new row is inserted
     */
    onInsert: (cb: (value: any, reducerEvent: ReducerEvent | undefined) => void) => void;
    /**
     * Register a callback for when a row is deleted from the database.
     *
     * ```ts
     * User.onDelete((user, reducerEvent) => {
     *   if (reducerEvent) {
     *      console.log("Deleted user on reducer", reducerEvent, user);
     *   } else {
     *      console.log("Deleted user received during subscription update on update", user);
     *  }
     * });
     * ```
     *
     * @param cb Callback to be called when a new row is inserted
     */
    onDelete: (cb: (value: any, reducerEvent: ReducerEvent | undefined) => void) => void;
    /**
     * Register a callback for when a row is updated into the database.
     *
     * ```ts
     * User.onInsert((user, reducerEvent) => {
     *   if (reducerEvent) {
     *      console.log("Updated user on reducer", reducerEvent, user);
     *   } else {
     *      console.log("Updated user received during subscription update on delete", user);
     *  }
     * });
     * ```
     *
     * @param cb Callback to be called when a new row is inserted
     */
    onUpdate: (cb: (value: any, oldValue: any, reducerEvent: ReducerEvent | undefined) => void) => void;
    /**
     * Removes the event listener for when a new row is inserted
     * @param cb Callback to be called when the event listener is removed
     */
    removeOnInsert: (cb: (value: any, reducerEvent: ReducerEvent | undefined) => void) => void;
    /**
     * Removes the event listener for when a row is deleted
     * @param cb Callback to be called when the event listener is removed
     */
    removeOnDelete: (cb: (value: any, reducerEvent: ReducerEvent | undefined) => void) => void;
    /**
     * Removes the event listener for when a row is updated
     * @param cb Callback to be called when the event listener is removed
     */
    removeOnUpdate: (cb: (value: any, oldValue: any, reducerEvent: ReducerEvent | undefined) => void) => void;
}
export {};
