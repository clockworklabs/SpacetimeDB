import type { Index, IndexOpts, UntypedIndex } from './indexes';
import type { UntypedSchemaDef } from './schema';
import type { RowType, TableIndexes, TableSchema } from './table';
import type {
  ColumnBuilder,
  ColumnMetadata,
  InferSpacetimeTypeOfTypeBuilder,
  TypeBuilder,
} from './type_builders';
import type { CollapseTuple } from './type_util';

/**
 * Helper to get the set of table names.
 */
export type TableNames<SchemaDef extends UntypedSchemaDef> =
  SchemaDef['tables'][number]['name'] & string;

/** helper: pick the table def object from the schema by its name */
export type TableDefByName<
  SchemaDef extends UntypedSchemaDef,
  Name extends TableNames<SchemaDef>,
> = Extract<SchemaDef['tables'][number], { name: Name }>;

export type QueryBuilder<SchemaDef extends UntypedSchemaDef> = {
  // readonly [Tbl in SchemaDef['tables'][number] as Tbl['name']]: TableRef<Tbl>;
  query<Name extends TableNames<SchemaDef>>(
    table: Name
  ): TableScan<SchemaDef, TableDefByName<SchemaDef, Name>>;
  //query(table: TableNames<SchemaDef>): TableScan<table>
};

export function fakeQueryBuilder<
  SchemaDef extends UntypedSchemaDef,
>(): QueryBuilder<SchemaDef> {
  throw 'unimplemented';
}

// A static list of column names for a table.
// type ColumnList<
//   SchemaDef extends UntypedSchemaDef,
//   TableName extends TableNames<SchemaDef>,
// > = readonly ColumnNames<TableDefByName<SchemaDef, TableName>>[];

export type ColumnList<
  SchemaDef extends UntypedSchemaDef,
  Table extends TableNames<SchemaDef>,
  T extends readonly ColumnNames<
    TableDefByName<SchemaDef, Table>
  >[] = readonly ColumnNames<TableDefByName<SchemaDef, Table>>[],
> = T;

export type JoinCondition<
  SchemaDef extends UntypedSchemaDef,
  LeftTable extends TableNames<SchemaDef>,
  RightTable extends TableNames<SchemaDef>,
> = {
  leftColumns: ColumnList<SchemaDef, LeftTable>;
  rightColumns: ColumnList<SchemaDef, RightTable>;
};

type EqualLength<
  A extends readonly any[],
  B extends readonly any[],
> = A['length'] extends B['length']
  ? B['length'] extends A['length']
    ? A
    : never
  : never;

export type JoinCondition7<
  SchemaDef extends UntypedSchemaDef,
  LeftTable extends TableNames<SchemaDef>,
  RightTable extends TableNames<SchemaDef>,
  LCols extends readonly ColumnNames<
    TableDefByName<SchemaDef, LeftTable>
  >[] = readonly ColumnNames<TableDefByName<SchemaDef, LeftTable>>[],
  RCols extends readonly ColumnNames<
    TableDefByName<SchemaDef, RightTable>
  >[] = readonly ColumnNames<TableDefByName<SchemaDef, RightTable>>[],
> =
  EqualLength<LCols, RCols> extends never
    ? never
    : {
        leftColumns: LCols;
        rightColumns: RCols;
      };

export type JoinCondition5<
  SchemaDef extends UntypedSchemaDef,
  LeftTable extends TableNames<SchemaDef>,
  RightTable extends TableNames<SchemaDef>,
  LCols extends ColumnList<SchemaDef, LeftTable> = ColumnList<
    SchemaDef,
    LeftTable
  >,
  RCols extends ColumnList<SchemaDef, RightTable> = ColumnList<
    SchemaDef,
    RightTable
  >,
> =
  HasEqualLength<LCols, RCols> extends never
    ? never
    : {
        leftColumns: LCols;
        rightColumns: RCols;
      };

type ColumnExprList<
  SchemaDef extends UntypedSchemaDef,
  TableName extends TableNames<SchemaDef>,
> = readonly AnyColumnExpr<TableDefByName<SchemaDef, TableName>>[];

type ColumnExprListExtractor<
  SchemaDef extends UntypedSchemaDef,
  TableName extends TableNames<SchemaDef>,
> = (
  row: RowExpr<TableDefByName<SchemaDef, TableName>>
) => ColumnExprList<SchemaDef, TableName>;
// type ColumnListExtractor<SchemaDef extends UntypedSchemaDef, TableName extends TableNames<SchemaDef>>> = ReadonlyArray<ColumnExpr<
export type JoinCondition2<
  SchemaDef extends UntypedSchemaDef,
  LeftTable extends TableNames<SchemaDef>,
  RightTable extends TableNames<SchemaDef>,
> = {
  leftColumns: ColumnExprListExtractor<SchemaDef, LeftTable>;
  rightColumns: ColumnExprListExtractor<SchemaDef, RightTable>;
};

// type Zip<A extends readonly any[], B extends readonly any[]> =
//   A extends [infer AH, ...infer AT]
//     ? B extends [infer BH, ...infer BT]
//       ? [[AH, BH], ...Zip<AT, BT>]
//       : never
//     : B extends [] ? [] : never;

type Zip<
  A extends readonly any[],
  B extends readonly any[],
> = A extends readonly [infer AH, ...infer AT]
  ? B extends readonly [infer BH, ...infer BT]
    ? [[AH, BH], ...Zip<AT, BT>]
    : never
  : B extends readonly []
    ? []
    : never;

export type JoinCondition3<
  SchemaDef extends UntypedSchemaDef,
  LeftTable extends TableNames<SchemaDef>,
  RightTable extends TableNames<SchemaDef>,
  L extends ColumnExprList<SchemaDef, LeftTable>,
  R extends ColumnExprList<SchemaDef, RightTable>,
> =
  Zip<L, R> extends never
    ? never
    : {
        leftColumns: (row: RowExpr<TableDefByName<SchemaDef, LeftTable>>) => L;
        rightColumns: (
          row: RowExpr<TableDefByName<SchemaDef, RightTable>>
        ) => R;
      };

/** Helper type to check if two tuples/arrays have equal length */
type HasEqualLength<
  T extends readonly any[],
  U extends readonly any[],
> = T extends { length: infer L }
  ? U extends { length: L }
    ? true
    : false
  : false;

export type JoinIsValid<T extends JoinCondition2<any, any, any>> =
  HasEqualLength<ReturnType<T['leftColumns']>, ReturnType<T['rightColumns']>>;
export type RestrictedJoin<T extends JoinCondition2<any, any, any>> =
  HasEqualLength<
    ReturnType<T['leftColumns']>,
    ReturnType<T['rightColumns']>
  > extends true
    ? T
    : never;

type SameLen<
  A extends readonly any[],
  B extends readonly any[],
> = A['length'] extends B['length']
  ? B['length'] extends A['length']
    ? true
    : never
  : never;

// ─────────────────────────────────────────────────────────────
// Helper that *preserves* tuple literal types and enforces length
export function on<LC extends readonly any[], RC extends readonly any[]>(
  leftColumns: LC,
  rightColumns: RC & (SameLen<LC, RC> extends never ? never : unknown)
) {
  return { leftColumns, rightColumns } as const;
}

// ─────────────────────────────────────────────────────────────
// JoinCondition type (optional, but nice to export)
export type JoinCondition9<
  SD extends UntypedSchemaDef,
  LeftTable extends TableNames<SD>,
  RightTable extends TableNames<SD>,
  LC extends readonly ColumnNames<TableDefByName<SD, LeftTable>>[],
  RC extends readonly ColumnNames<TableDefByName<SD, RightTable>>[],
> = {
  leftColumns: LC;
  rightColumns: RC;
};

export class TableScan<
  SchemaDef extends UntypedSchemaDef,
  TableDef extends TypedTableDef,
> {
  // readonly filters: readonly BooleanExpr<TableDef>[];

  filter(
    predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
  ): TableScan<SchemaDef, TableDef> {
    throw 'unimplemented';
  }

  toSql(): string {
    throw 'unimplemented';
  }
}

/**
 * A type representing a
 */
export type TableRef<Table extends TypedTableDef> = {
  type: 'table';
  row: RowExpr<Table>;
  tableName: Table['name'];
};

// TODO: Just use UntypedTableDef if they end up being the same.
export type TypedTableDef = {
  name: string;
  columns: Record<string, ColumnBuilder<any, any, ColumnMetadata<any>>>;
  indexes: readonly IndexOpts<any>[];
};

export type TableSchemaAsTableDef<
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

type AnyColumnExpr<Table extends TypedTableDef> = {
  [C in ColumnNames<Table>]: ColumnExpr<Table, C>;
}[ColumnNames<Table>];
/**
 * Acts as a row when writing filters for queries. It is a way to get column references.
 */
export type RowExpr<TableDef extends TypedTableDef> = {
  readonly [C in ColumnNames<TableDef>]: ColumnExpr<TableDef, C>;
};

/**
 * Union of ColumnExprs from Table whose spacetimeType is compatible with Value
 * (produces a union of ColumnExpr<Table, C> for matching columns).
 */
export type ColumnExprForValue<Table extends TypedTableDef, Value> = {
  [C in ColumnNames<Table>]: InferSpacetimeTypeOfColumn<Table, C> extends Value
    ? ColumnExpr<Table, C>
    : never;
}[ColumnNames<Table>];

export type ValueExpr<TableDef extends TypedTableDef, Value> =
  | LiteralExpr<Value>
  | ColumnExprForValue<TableDef, Value>;

type LiteralExpr<Value> = {
  type: 'literal';
  value: Value;
};

type BooleanExpr<Table extends TypedTableDef> = {
  type: 'eq';
  left: ValueExpr<Table, any>;
  right: ValueExpr<Table, any>;
};

export function eq<Table extends TypedTableDef>(
  left: ValueExpr<Table, any>,
  right: ValueExpr<Table, any>
): BooleanExpr<Table> {
  const lk = 'type' in left && left.type === 'literal';
  const rk = 'type' in right && right.type === 'literal';
  if (lk && !rk) {
    return {
      type: 'eq',
      left: right,
      right: left,
    };
  }
  return {
    type: 'eq',
    left,
    right,
  };
}

export function literal<Value>(value: Value): LiteralExpr<Value> {
  return { type: 'literal', value };
}
