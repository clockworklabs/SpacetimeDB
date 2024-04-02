"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.DatabaseTable = void 0;
const utils_1 = require("./utils");
class DatabaseTable {
    static db;
    static tableName;
    static with(client) {
        return (0, utils_1._tableProxy)(this, client);
    }
    static getDB() {
        if (!this.db) {
            throw "You can't query the database without creating a client first";
        }
        return this.db;
    }
    static count() {
        return this.getDB().getTable(this.tableName).count();
    }
    static all() {
        return this.getDB()
            .getTable(this.tableName)
            .getInstances();
    }
    static onInsert(callback) {
        this.getDB().getTable(this.tableName).onInsert(callback);
    }
    static onUpdate(callback) {
        this.getDB().getTable(this.tableName).onUpdate(callback);
    }
    static onDelete(callback) {
        this.getDB().getTable(this.tableName).onDelete(callback);
    }
    static removeOnInsert(callback) {
        this.getDB().getTable(this.tableName).removeOnInsert(callback);
    }
    static removeOnUpdate(callback) {
        this.getDB().getTable(this.tableName).removeOnUpdate(callback);
    }
    static removeOnDelete(callback) {
        this.getDB().getTable(this.tableName).removeOnDelete(callback);
    }
}
exports.DatabaseTable = DatabaseTable;
