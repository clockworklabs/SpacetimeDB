import { DatabaseTableClass } from ".";
import { Table } from "./table";
export declare class ClientDB {
    /**
     * The tables in the database.
     */
    tables: Map<string, Table>;
    constructor();
    /**
     * Returns the table with the given name.
     * @param name The name of the table.
     * @returns The table
     */
    getTable(name: string): Table;
    getOrCreateTable(tableName: string, pkCol: number | undefined, entityClass: DatabaseTableClass): Table;
}
