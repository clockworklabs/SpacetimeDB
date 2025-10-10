import type { RowType, UntypedTableDef } from './table';
import type { ColumnMetadata, IndexTypes } from './type_builders';
import type { CollapseTuple, Prettify } from './type_util';
import { Range } from './range';
import type { ColumnIsUnique } from './constraints';

/**
 * Index helper type used *inside* {@link table} to enforce that only
 * existing column names are referenced.
 */
export type IndexOpts<AllowedCol extends string> = {
  name?: string;
} & (
  | { algorithm: 'btree'; columns: readonly AllowedCol[] }
  | { algorithm: 'direct'; column: AllowedCol }
);

/**
 * An untyped representation of an index definition.
 */
type UntypedIndex<AllowedCol extends string> = {
  name: string;
  unique: boolean;
  algorithm: 'btree' | 'direct';
  columns: AllowedCol[];
};

/**
 * A helper type to extract the column names from an index definition.
 */
export type IndexColumns<I extends IndexOpts<any>> = I extends {
  columns: string[];
}
  ? I['columns']
  : I extends { column: string }
    ? [I['column']]
    : never;

/**
 * A type representing the indexes defined on a table.
 */
export type Indexes<
  TableDef extends UntypedTableDef,
  I extends Record<string, UntypedIndex<keyof TableDef['columns'] & string>>,
> = {
  [k in keyof I]: Index<TableDef, I[k]>;
};

/**
 * A type representing a database index, which can be either unique or ranged.
 */
export type Index<
  TableDef extends UntypedTableDef,
  I extends UntypedIndex<keyof TableDef['columns'] & string>,
> = I['unique'] extends true
  ? UniqueIndex<TableDef, I>
  : RangedIndex<TableDef, I>;

/**
 * A type representing a unique index on a database table.
 * Unique indexes enforce that the indexed columns contain unique values.
 */
export type UniqueIndex<
  TableDef extends UntypedTableDef,
  I extends UntypedIndex<keyof TableDef['columns'] & string>,
> = {
  find(col_val: IndexVal<TableDef, I>): RowType<TableDef> | null;
  delete(col_val: IndexVal<TableDef, I>): boolean;
  update(col_val: RowType<TableDef>): RowType<TableDef>;
};

/**
 * A type representing a ranged index on a database table.
 * Ranged indexes allow for range queries on the indexed columns.
 */
export type RangedIndex<
  TableDef extends UntypedTableDef,
  I extends UntypedIndex<keyof TableDef['columns'] & string>,
> = {
  filter(
    range: IndexScanRangeBounds<TableDef, I>
  ): IterableIterator<RowType<TableDef>>;
  delete(range: IndexScanRangeBounds<TableDef, I>): number;
};

/**
 * A helper type to extract the value type of an index based on the table definition and index definition.
 * This type constructs a tuple of the types of the columns that make up the index.
 */
export type IndexVal<
  TableDef extends UntypedTableDef,
  I extends UntypedIndex<keyof TableDef['columns'] & string>,
> = CollapseTuple<_IndexVal<TableDef, I['columns']>>;

/**
 * A helper type to extract the types of the columns that make up an index.
 */
type _IndexVal<TableDef extends UntypedTableDef, Columns extends string[]> = {
  [i in keyof Columns]: TableDef['columns'][Columns[i]]['typeBuilder']['type'];
};

/**
 * A helper type to define the bounds for scanning an index.
 * This type allows for specifying exact values or ranges for each column in the index.
 * It supports omitting trailing columns if the index is multi-column.
 */
export type IndexScanRangeBounds<
  TableDef extends UntypedTableDef,
  I extends UntypedIndex<keyof TableDef['columns'] & string>,
> = _IndexScanRangeBounds<_IndexVal<TableDef, I['columns']>>;

/**
 * A helper type to define the bounds for scanning an index.
 * This type allows for specifying exact values or ranges for each column in the index.
 * It supports omitting trailing columns if the index is multi-column.
 * This version only allows omitting the array if the index is single-column to avoid ambiguity.
 */
type _IndexScanRangeBounds<Columns extends any[]> = Columns extends [infer Term]
  ? Term | Range<Term>
  : _IndexScanRangeBoundsCase<Columns>;

/**
 * A helper type to define the bounds for scanning an index.
 * This type allows for specifying exact values or ranges for each column in the index.
 * It supports omitting trailing columns if the index is multi-column.
 */
type _IndexScanRangeBoundsCase<Columns extends any[]> = Columns extends [
  ...infer Prefix,
  infer Term,
]
  ? [...Prefix, Term | Range<Term>] | _IndexScanRangeBounds<Prefix>
  : never;

/**
 * A helper type representing a column index definition.
 */
export type ColumnIndex<
  Name extends string,
  M extends ColumnMetadata<any>,
> = Prettify<
  {
    name: Name;
    unique: ColumnIsUnique<M>;
    columns: [Name];
    algorithm: 'btree' | 'direct';
  } & (M extends {
    indexType: infer I extends NonNullable<IndexTypes>;
  }
    ? { algorithm: I }
    : ColumnIsUnique<M> extends true
      ? { algorithm: 'btree' }
      : never)
>;
