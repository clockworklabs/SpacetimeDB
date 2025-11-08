import type { Index, IndexOpts, UntypedIndex } from './indexes';
import type { UntypedSchemaDef } from './schema';
import type { RowType, TableIndexes, TableSchema } from './table';
import type {
  ColumnBuilder,
  ColumnMetadata,
  InferSpacetimeTypeOfTypeBuilder,
  TypeBuilder,
} from './type_builders';

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
  readonly [Tbl in SchemaDef['tables'][number] as Tbl['name']]: TableRef<Tbl>;
} & {
  query<Name extends TableNames<SchemaDef>>(
    table: Name
  ): TableScan<SchemaDef, TableDefByName<SchemaDef, Name>>;
};

/**
 * A runtime reference to a table. This materializes the RowExpr for us.
 * TODO: Maybe add the full SchemaDef to the type signature depending on how joins will work.
 */
export type TableRef<TableDef extends TypedTableDef> = Readonly<{
  type: 'table';
  name: TableDef['name'];
  cols: RowExpr<TableDef>;
  // Maybe redundant.
  tableDef: TableDef;
}>;

function createTableRefFromDef<TableDef extends TypedTableDef>(
  tableDef: TableDef
): TableRef<TableDef> {
  const cols = createRowExpr(tableDef);
  return {
    type: 'table',
    tableDef: tableDef,
    cols: cols,
    name: tableDef.name,
  };
}

export function makeQueryBuilder<SchemaDef extends UntypedSchemaDef>(
  schema: SchemaDef
): QueryBuilder<SchemaDef> {
  const qb = Object.create(null) as QueryBuilder<SchemaDef>;
  for (const table of schema.tables) {
    const ref = createTableRefFromDef(
      table as TableDefByName<SchemaDef, TableNames<SchemaDef>>
    );
    (qb as Record<string, TableRef<any>>)[table.name] = ref;
  }
  const builder = qb as QueryBuilder<SchemaDef>;
  builder.query = function <Name extends TableNames<SchemaDef>>(
    table: Name
  ): TableScan<SchemaDef, TableDefByName<SchemaDef, Name>> {
    const ref = this[table] as TableRef<TableDefByName<SchemaDef, Name>>;
    return new TableScan<SchemaDef, TableDefByName<SchemaDef, Name>>(ref);
  };
  return Object.freeze(qb) as QueryBuilder<SchemaDef>;
}

function createRowExpr<TableDef extends TypedTableDef>(
  tableDef: TableDef
): RowExpr<TableDef> {
  const row: Record<string, ColumnExpr<TableDef, any>> = {};
  for (const columnName of Object.keys(tableDef.columns) as Array<
    keyof TableDef['columns'] & string
  >) {
    const columnBuilder = tableDef.columns[columnName];
    row[columnName] = Object.freeze({
      type: 'column',
      table: tableDef.name,
      column: columnName,
      spacetimeType: columnBuilder.typeBuilder.resolveType(),
    }) as ColumnExpr<TableDef, typeof columnName>;
  }
  return Object.freeze(row) as RowExpr<TableDef>;
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

type ColumnExprList<
  SchemaDef extends UntypedSchemaDef,
  TableName extends TableNames<SchemaDef>,
> = readonly AnyColumnExpr<TableDefByName<SchemaDef, TableName>>[];

export class TableScan<
  SchemaDef extends UntypedSchemaDef,
  TableDef extends TypedTableDef,
> {
  constructor(
    readonly table: TableRef<TableDef>,
    readonly where?: BooleanExpr<TableDef>
  ) {}

  filter(
    predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
  ): TableScan<SchemaDef, TableDef> {
    const nextWhere = predicate(this.table.cols);
    return new TableScan<SchemaDef, TableDef>(this.table, nextWhere);
  }

  toSql(): string {
    const tableName = quoteIdentifier(this.table.name);
    const base = `SELECT * FROM ${tableName}`;
    if (!this.where) {
      return base;
    }
    return `${base} WHERE ${booleanExprToSql(this.where)}`;
  }
}

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
export type RowExpr<TableDef extends TypedTableDef> = Readonly<{
  readonly [C in ColumnNames<TableDef>]: ColumnExpr<TableDef, C>;
}>;

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

type BooleanExpr<Table extends TypedTableDef> =
  | {
      type: 'eq';
      left: ValueExpr<Table, any>;
      right: ValueExpr<Table, any>;
    }
  | {
      type: 'and';
      clauses: readonly [BooleanExpr<Table>, BooleanExpr<Table>, ...BooleanExpr<Table>[]];
    }
  | {
      type: 'or';
      clauses: readonly [BooleanExpr<Table>, BooleanExpr<Table>, ...BooleanExpr<Table>[]];
    }
  | {
      type: 'not';
      clause: BooleanExpr<Table>;
    };

export function eq<Table extends TypedTableDef>(
  left: ValueExpr<Table, any>,
  right: ValueExpr<Table, any>
): BooleanExpr<Table> {
  // TODO: Not sure if normalizing like this is actually helpful.
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

export function not<Table extends TypedTableDef>(
  clause: BooleanExpr<Table>
): BooleanExpr<Table> {
  return { type: 'not', clause };
}

export function and<Table extends TypedTableDef>(
  ...clauses: readonly [BooleanExpr<Table>, BooleanExpr<Table>, ...BooleanExpr<Table>[]]
): BooleanExpr<Table> {
  return { type: 'and', clauses };
}

export function or<Table extends TypedTableDef>(
  ...clauses: readonly [BooleanExpr<Table>, BooleanExpr<Table>, ...BooleanExpr<Table>[]]
): BooleanExpr<Table> {
  return { type: 'or', clauses };
}

function booleanExprToSql<Table extends TypedTableDef>(
  expr: BooleanExpr<Table>
): string {
  switch (expr.type) {
    case 'eq':
      return `${valueExprToSql(expr.left)} = ${valueExprToSql(expr.right)}`;
    case 'and':
      return expr.clauses.map(booleanExprToSql).map(wrapInParens).join(' AND ');
    case 'or':
      return expr.clauses.map(booleanExprToSql).map(wrapInParens).join(' OR ');
    case 'not':
      return `NOT ${wrapInParens(booleanExprToSql(expr.clause))}`;
  }
}
function wrapInParens(sql: string): string {
  return `(${sql})`;
}

function valueExprToSql<Table extends TypedTableDef>(
  expr: ValueExpr<Table, any>
): string {
  if (isLiteralExpr(expr)) {
    return literalValueToSql(expr.value);
  }
  return `${quoteIdentifier(expr.table)}.${quoteIdentifier(expr.column)}`;
}

function literalValueToSql(value: unknown): string {
  if (value === null || value === undefined) {
    return 'NULL';
  }
  switch (typeof value) {
    case 'number':
    case 'bigint':
      return String(value);
    case 'boolean':
      return value ? 'TRUE' : 'FALSE';
    case 'string':
      return `'${value.replace(/'/g, "''")}'`;
    default:
      return `'${JSON.stringify(value).replace(/'/g, "''")}'`;
  }
}

function quoteIdentifier(name: string): string {
  return `"${name.replace(/"/g, '""')}"`;
}

function isLiteralExpr<Value>(
  expr: ValueExpr<any, Value>
): expr is LiteralExpr<Value> {
  return (expr as LiteralExpr<Value>).type === 'literal';
}
