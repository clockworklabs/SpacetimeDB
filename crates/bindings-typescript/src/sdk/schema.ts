import type { SchemaDef } from '../lib/schema';
import type { UntypedTableDecl } from '../lib/table_schema';
import {
  schema as moduleSchema,
  type ModuleSettings,
  type Schema,
} from '../server/schema';

/**
 * Creates a schema from table declarations. This is the `schema()` a module
 * uses, and it returns the same {@link Schema}, so client bindings can declare
 * the module's reducers, procedures, and views on it, without their bodies.
 * @param tables - The table declarations, keyed by accessor name
 * @param moduleSettings - The module's settings, such as its case conversion policy
 * @returns The {@link Schema} of the module
 * @example
 * ```ts
 * const spacetimedb = schema({
 *   user: table({}, userType),
 *   post: table({}, postType)
 * });
 * ```
 */
export function schema<const H extends Record<string, UntypedTableDecl>>(
  tables: H,
  moduleSettings?: ModuleSettings
): Schema<SchemaDef<H>> {
  return moduleSchema(tables, moduleSettings) as Schema<any>;
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
