import {
  ModuleContext,
  tablesToSchema,
  type TablesToSchema,
  type UntypedSchemaDef,
} from '../lib/schema';
import type { UntypedTableSchema } from '../lib/table_schema';

class Tables<S extends UntypedSchemaDef> {
  constructor(readonly schemaType: S) {}
}

/**
 * Creates a schema from table definitions
 * @param handles - Array of table handles created by table() function
 * @returns ColumnBuilder representing the complete database schema
 * @example
 * ```ts
 * const s = schema(
 *   table({ name: 'user' }, userType),
 *   table({ name: 'post' }, postType)
 * );
 * ```
 */
export function schema<const H extends Record<string, UntypedTableSchema>>(
  tables: H
): Tables<TablesToSchema<H>> {
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
