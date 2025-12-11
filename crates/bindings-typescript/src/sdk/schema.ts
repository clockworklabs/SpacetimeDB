import {
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
export function schema<const H extends readonly UntypedTableSchema[]>(
  ...handles: H
): Tables<TablesToSchema<H>>;

/**
 * Creates a schema from table definitions (array overload)
 * @param handles - Array of table handles created by table() function
 * @returns ColumnBuilder representing the complete database schema
 */
export function schema<const H extends readonly UntypedTableSchema[]>(
  handles: H
): Tables<TablesToSchema<H>>;

/**
 * Creates a schema from table definitions
 * @param args - Either an array of table handles or a variadic list of table handles
 * @returns ColumnBuilder representing the complete database schema
 * @example
 * ```ts
 * const s = schema(
 *  table({ name: 'user' }, userType),
 *  table({ name: 'post' }, postType)
 * );
 * ```
 */
export function schema<const H extends readonly UntypedTableSchema[]>(
  ...args: [H] | H
): Tables<TablesToSchema<H>> {
  const handles = (
    args.length === 1 && Array.isArray(args[0]) ? args[0] : args
  ) as H;

  return new Tables(tablesToSchema(handles));
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
