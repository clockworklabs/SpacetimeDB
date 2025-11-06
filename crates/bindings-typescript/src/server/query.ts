import type { Index, IndexOpts, UntypedIndex } from './indexes';
import type { RowType, TableIndexes, TableSchema } from './table';
import type {
  ColumnBuilder,
  ColumnMetadata,
  InferSpacetimeTypeOfRow,
  InferSpacetimeTypeOfTypeBuilder,
  TypeBuilder,
} from './type_builders';
import type { CollapseTuple } from './type_util';

// TODO: Just use UntypedTableDef if they end up being the same.
export type TypedTableDef = {
  name: string;
  columns: Record<string, ColumnBuilder<any, any, ColumnMetadata<any>>>;
  indexes: readonly IndexOpts<any>[];
};

type TableSchemaAsTableDef<
  TSchema extends TableSchema<any, any, readonly any[]>,
> = {
  name: TSchema['tableName'];
  columns: TSchema['rowType']['row'];
  indexes: TSchema['idxs'];
};

export type ColumnExpr<
  TableDef extends TypedTableDef,
  ColumnName extends ColumnNames<TableDef>,
> = Readonly<{
  type: 'column';
  column: ColumnName;
  table: TableDef['name'];
  // This is here as a phantom type. You can pull it back with NonNullable<>
  tsValueType?: RowType<TableDef>[ColumnName];
  /**
   * docs
   */
  spacetimeType: InferSpacetimeTypeOfColumn<TableDef, ColumnName>;
}>;

/**
 * Helper to get the spacetime type of a column.
 */
type InferSpacetimeTypeOfColumn<
  TableDef extends TypedTableDef,
  ColumnName extends ColumnNames<TableDef>,
> =
  TableDef['columns'][ColumnName]['typeBuilder'] extends TypeBuilder<
    any,
    infer U
  >
    ? U
    : never;

type ColumnNames<TableDef extends TypedTableDef> = keyof RowType<TableDef> &
  string;

/**
 * Acts as a row when writing filters for queries. It is a way to get column references.
 */
export type RowExpr<TableDef extends TypedTableDef> = {
  readonly [C in ColumnNames<TableDef>]: ColumnExpr<TableDef, C>;
};

type IndexNames<TableDef extends TypedTableDef> = TableIndexes<TableDef>;

export type IndexNameUnion<TableDef extends TypedTableDef> = Extract<
  keyof TableIndexes<TableDef>,
  string
>;

/*
export type IndexExpr<TableDef extends TypedTableDef,
I extends IndexNameUnion<TableDef> = CollapseTuple<_IndexVal<TableDef, TableIndexes<TableDef>[I]['columns'];
*/

export type IndexExprs<TableDef extends TypedTableDef> = {
  readonly [I in IndexNameUnion<TableDef>]: IndexExpr<
    TableDef,
    TableIndexes<TableDef>[I]
  >;
};

/**
 * A helper type to extract the types of the columns that make up an index.
 */
type _IndexVal<TableDef extends TypedTableDef, Columns extends string[]> = {
  [i in keyof Columns]: TableDef['columns'][Columns[i]]['typeBuilder'] extends TypeBuilder<
    any,
    infer U
  >
    ? U
    : never;
};

export type IndexExpr<
  TableDef extends TypedTableDef,
  I extends UntypedIndex<keyof TableDef['columns'] & string>,
> = CollapseTuple<_IndexVal<TableDef, I['columns']>>;
