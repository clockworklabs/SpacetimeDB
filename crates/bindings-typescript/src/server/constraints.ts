import type { UntypedTableDef } from './table';
import type { ColumnMetadata } from './type_builders';

/**
 * A helper type to determine if all columns in an index are unique.
 */
export type AllUnique<
  TableDef extends UntypedTableDef,
  Columns extends ReadonlyArray<keyof TableDef['columns']>,
> = Columns extends readonly [
  infer Head extends keyof TableDef['columns'],
  ...infer Tail extends ReadonlyArray<keyof TableDef['columns']>,
]
  ? ColumnIsUnique<TableDef['columns'][Head]['columnMetadata']> extends true
    ? AllUnique<TableDef, Tail>
    : false
  : true;

/**
 * A helper type to determine if a column is unique based on its metadata.
 * A column is considered unique if it has either `isUnique` or `isPrimaryKey` set to true in its metadata.
 * @template M - The column metadata to check.
 * @returns `true` if the column is unique, otherwise `false`.
 * @example
 * ```typescript
 * type Meta1 = { isUnique: true };
 * type Meta2 = { isPrimaryKey: true };
 * type Meta3 = { isUnique: false };
 * type Meta4 = {};
 * type Result1 = ColumnIsUnique<Meta1>; // true
 * type Result2 = ColumnIsUnique<Meta2>; // true
 * type Result3 = ColumnIsUnique<Meta3>; // false
 * type Result4 = ColumnIsUnique<Meta4>; // false
 * ```
 */
export type ColumnIsUnique<M extends ColumnMetadata<any>> = M extends
  | { isUnique: true }
  | { isPrimaryKey: true }
  ? true
  : false;
