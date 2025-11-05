import type { TableNamesOf, UntypedSchemaDef } from '../lib/schema.ts';
import type { UntypedTableDef } from '../lib/table.ts';
import type { UntypedRemoteModule } from './spacetime_module.ts';
import { TableCache } from './table_cache.ts';

type TableName<SchemaDef> =
  [SchemaDef] extends [UntypedSchemaDef] ? TableNamesOf<SchemaDef> : string;

export type TableDefForTableName<SchemaDef extends UntypedSchemaDef, N extends TableName<SchemaDef>> =
  [SchemaDef] extends [UntypedSchemaDef]
    ? (SchemaDef['tables'][number] & { name: N })
    : UntypedTableDef;

type TableCacheForTableName<
  RemoteModule extends UntypedRemoteModule,
  TableName extends TableNamesOf<RemoteModule>,
> = TableCache<RemoteModule, TableName>;

/**
 * This is a helper class that provides a mapping from table names to their corresponding TableCache instances
 * while preserving the correspondence between the key and value type.
 */
class TableMap<RemoteModule extends UntypedRemoteModule> {
  private readonly map: Map<string, TableCacheForTableName<RemoteModule, TableName<RemoteModule>>> = new Map();

  get<K extends TableName<RemoteModule>>(key: K): TableCacheForTableName<RemoteModule, K> | undefined {
    // Cast required: a Map<string, Union> can't refine the union to the exact K-specific member on get<K>(key: K).
    return this.map.get(key) as TableCacheForTableName<RemoteModule, K> | undefined;
  }

  set<K extends TableName<RemoteModule>>(key: K, value: TableCacheForTableName<RemoteModule, K>): this {
    this.map.set(key, value);
    return this;
  }

  has(key: TableName<RemoteModule>): boolean {
    return this.map.has(key);
  }

  delete(key: TableName<RemoteModule>): boolean {
    return this.map.delete(key);
  }

  // optional: iteration stays broadly typed (cannot express per-key relation here)
  keys(): IterableIterator<string> { return this.map.keys(); }
  values(): IterableIterator<TableCacheForTableName<RemoteModule, TableName<RemoteModule>>> { return this.map.values(); }
  entries(): IterableIterator<[string, TableCacheForTableName<RemoteModule, TableName<RemoteModule>>]> { return this.map.entries(); }
  [Symbol.iterator]() { return this.entries(); }
}

/**
 * ClientCache maintains a cache of TableCache instances for each table in the database.
 * It provides methods to get or create TableCache instances by table name,
 * ensuring type safety based on the provided SchemaDef.
 */
export class ClientCache<RemoteModule extends UntypedRemoteModule> {
  /**
   * The tables in the database.
   */
  readonly tables = new TableMap<RemoteModule>();

  /**
   * Returns the table with the given name.
   * - If SchemaDef is a concrete schema, `name` is constrained to known table names,
   *   and the return type matches that table.
   * - If SchemaDef is undefined, `name` is string and the return type is untyped.
   */
  getTable<N extends TableName<RemoteModule>>(name: N): TableCacheForTableName<RemoteModule, N> {
    const table = this.tables.get(name);
    if (!table) {
      console.error(
        'The table has not been registered for this client. Please register the table before using it. If you have registered global tables using the SpacetimeDBClient.registerTables() or `registerTable()` method, please make sure that is executed first!'
      );
      throw new Error(`Table ${String(name)} does not exist`);
    }
    return table;
  }

  /**
   * Returns the table with the given name, creating it if needed.
   * - Typed mode: `tableTypeInfo.tableName` is constrained to known names and
   *   the return type matches that table.
   * - Untyped mode: accepts any string and returns an untyped TableCache.
   */
  getOrCreateTable<N extends TableName<RemoteModule>>(
    tableDef: TableDefForTableName<RemoteModule, N>
  ): TableCacheForTableName<RemoteModule, N> {
    const name = tableDef.name as N;

    let table = this.tables.get(name);
    if (table) {
      return table;
    }

    const newTable = new TableCache<RemoteModule, N>(tableDef);
    this.tables.set(name, newTable);
    return newTable;
  }
}
