import type { RowType, table, UntypedTableDecl } from './table';
import type { ColumnMetadata, IndexTypes } from './type_builders';
import type { CollapseTuple, Prettify } from './type_util';
import { Range } from '../server/range';
import type { ColumnIsUnique } from './constraints';

/**
 * Index helper type used *inside* {@link table} to enforce that only
 * existing column names are referenced.
 */
export type IndexOpts<AllowedCol extends string> = {
  accessor: string;
  name?: string;
} & (
  | { algorithm: 'btree'; columns: readonly AllowedCol[] }
  | { algorithm: 'hash'; columns: readonly AllowedCol[] }
  | { algorithm: 'direct'; column: AllowedCol }
);

/**
 * An untyped representation of an index definition.
 */
export type UntypedIndex<AllowedCol extends string> = {
  name: string;
  unique?: boolean;
  algorithm: 'btree' | 'direct' | 'hash';
  columns: readonly AllowedCol[];
};

/**
 * A helper type to extract the column names from an index definition.
 */
export type IndexColumns<I extends IndexOpts<any>> = I extends {
  columns: readonly string[];
}
  ? readonly [...I['columns']]
  : I extends { column: infer Name extends string }
    ? readonly [Name]
    : never;

/**
 * A type representing the indexes defined on a table.
 */
export type Indexes<
  TableDecl extends UntypedTableDecl,
  I extends Record<string, UntypedIndex<keyof TableDecl['columns'] & string>>,
> = {
  [k in keyof I]: Index<TableDecl, I[k]>;
};

/**
 * Check whether every column in an index is a primary key column.
 */
type AllColumnsPrimaryKey<
  TableDecl extends UntypedTableDecl,
  Columns extends readonly string[],
> = Columns extends readonly [
  infer Head extends keyof TableDecl['columns'] & string,
  ...infer Tail extends readonly string[],
]
  ? TableDecl['columns'][Head]['columnMetadata'] extends { isPrimaryKey: true }
    ? AllColumnsPrimaryKey<TableDecl, Tail>
    : false
  : true;

/**
 * A type representing a database index,
 * which can either be unique or filter for a single value or range of values.
 * Unique indexes on primary key columns additionally support `update`.
 */
export type Index<
  TableDecl extends UntypedTableDecl,
  I extends UntypedIndex<keyof TableDecl['columns'] & string>,
> = I['unique'] extends true
  ? AllColumnsPrimaryKey<TableDecl, I['columns']> extends true
    ? UniqueIndex<TableDecl, I> & {
        update(row: Prettify<RowType<TableDecl>>): Prettify<RowType<TableDecl>>;
      }
    : UniqueIndex<TableDecl, I>
  : I['algorithm'] extends 'hash'
    ? PointIndex<TableDecl, I>
    : RangedIndex<TableDecl, I>;

/**
 * A type representing a collection of read-only indexes defined on a table.
 */
export type ReadonlyIndexes<
  TableDecl extends UntypedTableDecl,
  I extends Record<string, UntypedIndex<keyof TableDecl['columns'] & string>>,
> = {
  [k in keyof I]: ReadonlyIndex<TableDecl, I[k]>;
};

/**
 * A type representing a read-only database index,
 * which can be either unique, pointed, or ranged.
 * This type only exposes read-only operations.
 */
export type ReadonlyIndex<
  TableDecl extends UntypedTableDecl,
  I extends UntypedIndex<keyof TableDecl['columns'] & string>,
> = I['unique'] extends true
  ? ReadonlyUniqueIndex<TableDecl, I>
  : I['algorithm'] extends 'hash'
    ? ReadonlyPointIndex<TableDecl, I>
    : ReadonlyRangedIndex<TableDecl, I>;

/**
 * A type representing a read-only unique index on a database table.
 */
export type ReadonlyUniqueIndex<
  TableDecl extends UntypedTableDecl,
  I extends UntypedIndex<keyof TableDecl['columns'] & string>,
> = {
  find(colVal: IndexVal<TableDecl, I>): RowType<TableDecl> | null;
};

/**
 * A type representing a unique index on a database table.
 * Unique indexes enforce that the indexed columns contain unique values.
 */
export interface UniqueIndex<
  TableDecl extends UntypedTableDecl,
  I extends UntypedIndex<keyof TableDecl['columns'] & string>,
> extends ReadonlyUniqueIndex<TableDecl, I> {
  delete(colVal: IndexVal<TableDecl, I>): boolean;
}

/**
 * A type representing a read-only point index on a database table.
 */
export interface ReadonlyPointIndex<
  TableDecl extends UntypedTableDecl,
  I extends UntypedIndex<keyof TableDecl['columns'] & string>,
> {
  filter(
    point: IndexVal<TableDecl, I>
  ): IteratorObject<Prettify<RowType<TableDecl>>, undefined>;
}

/**
 * A type representing a point index on a database table.
 * Point indexes allow for exact match queries on the indexed columns.
 */
export interface PointIndex<
  TableDecl extends UntypedTableDecl,
  I extends UntypedIndex<keyof TableDecl['columns'] & string>,
> extends ReadonlyPointIndex<TableDecl, I> {
  delete(point: IndexVal<TableDecl, I>): number;
}

/**
 * A type representing a read-only ranged index on a database table.
 */
export interface ReadonlyRangedIndex<
  TableDecl extends UntypedTableDecl,
  I extends UntypedIndex<keyof TableDecl['columns'] & string>,
> {
  filter(
    range: IndexScanRangeBounds<TableDecl, I>
  ): IteratorObject<Prettify<RowType<TableDecl>>, undefined>;
}

/**
 * A type representing a ranged index on a database table.
 * Ranged indexes allow for range queries on the indexed columns.
 */
export interface RangedIndex<
  TableDecl extends UntypedTableDecl,
  I extends UntypedIndex<keyof TableDecl['columns'] & string>,
> extends ReadonlyRangedIndex<TableDecl, I> {
  delete(range: IndexScanRangeBounds<TableDecl, I>): number;
}

/**
 * A helper type to extract the value type of an index based on the table definition and index definition.
 * This type constructs a tuple of the types of the columns that make up the index.
 */
export type IndexVal<
  TableDecl extends UntypedTableDecl,
  I extends UntypedIndex<keyof TableDecl['columns'] & string>,
> = CollapseTuple<_IndexVal<TableDecl, I['columns']>>;

/**
 * A helper type to extract the types of the columns that make up an index.
 */
type _IndexVal<
  TableDecl extends UntypedTableDecl,
  Columns extends readonly string[],
> = Columns extends readonly [
  infer Head extends string,
  ...infer Tail extends readonly string[],
]
  ? [
      TableDecl['columns'][Head]['typeBuilder']['type'],
      ..._IndexVal<TableDecl, Tail>,
    ]
  : [];

/**
 * A helper type to define the bounds for scanning an index.
 * This type allows for specifying exact values or ranges for each column in the index.
 * It supports omitting trailing columns if the index is multi-column.
 */
export type IndexScanRangeBounds<
  TableDecl extends UntypedTableDecl,
  I extends UntypedIndex<keyof TableDecl['columns'] & string>,
> = _IndexScanRangeBounds<_IndexVal<TableDecl, I['columns']>>;

/**
 * A helper type to define the bounds for scanning an index.
 * This type allows for specifying exact values or ranges for each column in the index.
 * It supports omitting trailing columns if the index is multi-column.
 * This version only allows omitting the array if the index is single-column to avoid ambiguity.
 */
type _IndexScanRangeBounds<Columns extends readonly any[]> = Columns extends [
  infer Term,
]
  ? Term | Range<Term>
  : _IndexScanRangeBoundsCase<Columns>;

/**
 * A helper type to define the bounds for scanning an index.
 * This type allows for specifying exact values or ranges for each column in the index.
 * It supports omitting trailing columns if the index is multi-column.
 */
type _IndexScanRangeBoundsCase<Columns extends readonly any[]> =
  Columns extends [...infer Prefix, infer Term]
    ? readonly [...Prefix, Term | Range<Term>] | _IndexScanRangeBounds<Prefix>
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
    columns: readonly [Name];
    algorithm: 'btree' | 'direct' | 'hash';
  } & (M extends {
    indexType: infer I extends NonNullable<IndexTypes>;
  }
    ? { algorithm: I }
    : ColumnIsUnique<M> extends true
      ? { algorithm: 'btree' }
      : never)
>;
