import {
  ModuleContext,
  tablesToSchema,
  type SchemaDecl,
  type UntypedSchemaDecl,
} from '../lib/schema';
import type { UntypedTableBody } from '../lib/table_body';

class Tables<S extends UntypedSchemaDecl> {
  constructor(readonly schemaType: S) {}
}

/**
 * Creates a schema from table definitions
 * @param handles - Array of table handles created by table() function
 * @returns ColumnBuilder representing the complete database schema
 * @example
 * ```ts
 * const spacetimedb = schema({
 *   user: table({}, userType),
 *   post: table({}, postType)
 * });
 * ```
 */
export function schema<const H extends Record<string, UntypedTableBody>>(
  tables: H
): Tables<SchemaDecl<H>> {
  const ctx = new ModuleContext();

  return new Tables(tablesToSchema(ctx, tables));
}

type HasAccessor = { accessorName: PropertyKey };

export type ConvertToAccessorMap<TableDefs extends readonly HasAccessor[]> = {
  [Tbl in TableDefs[number] as Tbl['accessorName']]: Tbl;
};

export function convertToAccessorMap<T extends readonly HasAccessor[]>(
  arr: T
): ConvertToAccessorMap<T> {
  return Object.fromEntries(
    arr.map(v => [v.accessorName, v])
  ) as ConvertToAccessorMap<T>;
}
