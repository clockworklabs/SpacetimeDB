"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ClientDB = void 0;
const table_1 = require("./table");
class ClientDB {
    /**
     * The tables in the database.
     */
    tables;
    constructor() {
        this.tables = new Map();
    }
    /**
     * Returns the table with the given name.
     * @param name The name of the table.
     * @returns The table
     */
    getTable(name) {
        const table = this.tables.get(name);
        // ! This should not happen as the table should be available but an exception is thrown just in case.
        if (!table) {
            console.error("The table has not been registered for this client. Please register the table before using it. If you have registered global tables using the SpacetimeDBClient.registerTables() or `registerTable()` method, please make sure that is executed first!");
            throw new Error(`Table ${name} does not exist`);
        }
        return table;
    }
    getOrCreateTable(tableName, pkCol, entityClass) {
        let table;
        if (!this.tables.has(tableName)) {
            table = new table_1.Table(tableName, pkCol, entityClass);
            this.tables.set(tableName, table);
        }
        else {
            table = this.tables.get(tableName);
        }
        return table;
    }
}
exports.ClientDB = ClientDB;
