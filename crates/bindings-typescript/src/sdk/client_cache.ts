import type { TableRuntimeTypeInfo } from './spacetime_module.ts';
import { TableCache } from './table_cache.ts';

export class ClientCache {
  /**
   * The tables in the database.
   */
  tables: Map<string, TableCache<any>>;

  constructor() {
    this.tables = new Map();
  }

  /**
   * Returns the table with the given name.
   * @param name The name of the table.
   * @returns The table
   */
  getTable<RowType extends Record<string, any>>(
    name: string
  ): TableCache<RowType> {
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

  getOrCreateTable<RowType extends Record<string, any>>(
    tableTypeInfo: TableRuntimeTypeInfo
  ): TableCache<RowType> {
    let table: TableCache<RowType>;
    if (!this.tables.has(tableTypeInfo.tableName)) {
      table = new TableCache<RowType>(tableTypeInfo);
      this.tables.set(tableTypeInfo.tableName, table);
    } else {
      table = this.tables.get(tableTypeInfo.tableName)!;
    }
    return table;
  }
}
