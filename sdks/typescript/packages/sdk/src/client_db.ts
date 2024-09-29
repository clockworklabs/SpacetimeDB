import { Table } from './table.ts';

export class ClientDB {
  /**
   * The tables in the database.
   */
  tables: Map<string, Table>;

  constructor() {
    this.tables = new Map();
  }

  /**
   * Returns the table with the given name.
   * @param name The name of the table.
   * @returns The table
   */
  getTable(name: string): Table {
    const table = this.tables.get(name);

    // ! This should not happen as the table should be available but an exception is thrown just in case.
    if (!table) {
      console.error(
        'The table has not been registered for this client. Please register the table before using it. If you have registered global tables using the SpacetimeDBClient.registerTables() or `registerTable()` method, please make sure that is executed first!'
      );
      throw new Error(`Table ${name} does not exist`);
    }

    return table;
  }

  getOrCreateTable(
    tableName: string,
    pkCol: number | undefined,
    entityClass: any
  ): Table {
    let table: Table;
    if (!this.tables.has(tableName)) {
      table = new Table(tableName, pkCol, entityClass);
      this.tables.set(tableName, table);
    } else {
      table = this.tables.get(tableName)!;
    }
    return table;
  }
}
