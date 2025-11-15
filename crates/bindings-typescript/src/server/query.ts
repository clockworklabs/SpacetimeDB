import { ConnectionId } from '../lib/connection_id';
import { Identity } from '../lib/identity';
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

export type Query<TableDef extends TypedTableDef> = ScanQuery<TableDef>;

type ScanQuery<TableDef extends TypedTableDef> = Readonly<{
  where(
    predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
  ): ScanQuery<TableDef>;
  /**
   * Query for rows in a different table that match the results of this query.
   * @param other The table we want results from.
   * @param leftCol The column from the existing query that we are using to join.
   * @param rightCol The column from the result table that we are using to join.
   */
  semijoinTo<OtherTable extends TypedTableDef>(
    other: RefSource<OtherTable>,
    leftCol: (left: RowExpr<TableDef>) => AnyColumnExpr<TableDef>,
    rightCol: (right: RowExpr<OtherTable>) => AnyColumnExpr<OtherTable>
  ): SemijoinQuery<OtherTable>;
  toSql(): string;
}>;

type SemijoinQuery<ResultTable extends TypedTableDef> = Readonly<{
  toSql(): string;
}>;

export type QueryBuilder<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in SchemaDef['tables'][number] as Tbl['name']]: TableRef<Tbl>;
} & {};

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

export type RefSource<TableDef extends TypedTableDef> =
  | TableRef<TableDef>
  | { ref(): TableRef<TableDef> };

export function createTableRefFromDef<TableDef extends TypedTableDef>(
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

/**
 * This is only used in tests as a helper to get the TableRefs.
 * @param schema
 * @returns
 */
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

export type JoinCondition<
  LeftTable extends TypedTableDef,
  RightTable extends TypedTableDef,
> = {
  leftColumn: AnyColumnExpr<LeftTable>;
  rightColumn: AnyColumnExpr<RightTable>;
};

class TableScan<TableDef extends TypedTableDef> {
  constructor(
    readonly table: TableRef<TableDef>,
    readonly where?: BooleanExpr<TableDef>
  ) {}
}

export function from<TableDef extends TypedTableDef>(
  source: RefSource<TableDef>
): ScanQuery<TableDef> {
  return createScanQuery(new TableScan(resolveTableRef(source)));
}

function resolveTableRef<TableDef extends TypedTableDef>(
  source: RefSource<TableDef>
): TableRef<TableDef> {
  if (typeof (source as { ref?: unknown }).ref === 'function') {
    return (source as { ref(): TableRef<TableDef> }).ref();
  }
  return source as TableRef<TableDef>;
}

function createScanQuery<TableDef extends TypedTableDef>(
  scan: TableScan<TableDef>
): ScanQuery<TableDef> {
  return Object.freeze({
    where(
      predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
    ): ScanQuery<TableDef> {
      const newCondition = predicate(scan.table.cols);
      const nextWhere = scan.where
        ? and(scan.where, newCondition)
        : newCondition;
      return createScanQuery(new TableScan<TableDef>(scan.table, nextWhere));
    },
    semijoinTo<OtherTable extends TypedTableDef>(
      other: RefSource<OtherTable>,
      leftCol: (left: RowExpr<TableDef>) => AnyColumnExpr<TableDef>,
      rightCol: (right: RowExpr<OtherTable>) => AnyColumnExpr<OtherTable>
    ): SemijoinQuery<TableDef> {
      const otherRef = resolveTableRef(other);
      const leftColumn = leftCol(scan.table.cols);
      const rightColumn = rightCol(otherRef.cols);
      const semijoin: Semijoin<TableDef, OtherTable> = {
        type: 'semijoin',
        left: scan.table,
        right: otherRef,
        leftWhereClause: scan.where,
        joinClause: {
          leftColumn,
          rightColumn,
        },
      };
      return createSemijoinQuery(semijoin);
    },
    toSql(): string {
      return renderSelectSql(scan.table.name, scan.where);
    },
  });
}

function createSemijoinQuery<
  LeftTable extends TypedTableDef,
  RightTable extends TypedTableDef,
>(semijoin: Semijoin<LeftTable, RightTable>): SemijoinQuery<LeftTable> {
  return Object.freeze({
    toSql(): string {
      return renderSemijoinToSql(semijoin);
    },
  });
}

export type Semijoin<
  LeftTable extends TypedTableDef,
  RightTable extends TypedTableDef,
> = Readonly<{
  type: 'semijoin';
  left: TableRef<LeftTable>;
  right: TableRef<RightTable>;
  joinClause: JoinCondition<LeftTable, RightTable>;
  leftWhereClause?: BooleanExpr<LeftTable>;
}>;

export function renderSemijoinToSql<
  LeftTable extends TypedTableDef,
  RightTable extends TypedTableDef,
>(semijoin: Semijoin<LeftTable, RightTable>): string {
  const leftAlias = quoteIdentifier('left');
  const rightAlias = quoteIdentifier('right');
  const quotedRightTable = quoteIdentifier(semijoin.right.name);
  const quotedLeftTable = quoteIdentifier(semijoin.left.name);
  const base = `SELECT ${rightAlias}.* from ${quotedLeftTable} ${leftAlias} join ${quotedRightTable} ${rightAlias} on `;
  const joinClause = `${leftAlias}.${quoteIdentifier(semijoin.joinClause.leftColumn.column)} = ${rightAlias}.${quoteIdentifier(semijoin.joinClause.rightColumn.column)}`;
  return (
    base + joinClause + renderWhereClauseSql('left', semijoin.leftWhereClause)
  );
}

function renderWhereClauseSql(
  tableName: string,
  where?: BooleanExpr<any>
): string {
  if (where == undefined) {
    return '';
  }
  return ` WHERE ${booleanExprToSql(where, tableName)}`;
}

function renderSelectSql<Table extends TypedTableDef>(
  tableName: string,
  where?: BooleanExpr<Table>,
  extraClauses: readonly string[] = []
): string {
  const quotedTable = quoteIdentifier(tableName);
  const base = `SELECT * FROM ${quotedTable}`;
  const clauses: string[] = [];
  if (where) {
    clauses.push(booleanExprToSql(where));
  }
  clauses.push(...extraClauses);
  if (clauses.length === 0) {
    return base;
  }
  const whereSql =
    clauses.length === 1 ? clauses[0] : clauses.map(wrapInParens).join(' AND ');
  return `${base} WHERE ${whereSql}`;
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

type LiteralValue =
  | string
  | number
  | bigint
  | boolean
  | Identity
  | ConnectionId;

export type ValueExpr<TableDef extends TypedTableDef, Value> =
  | LiteralExpr<Value & LiteralValue>
  | ColumnExprForValue<TableDef, Value>;

type LiteralExpr<Value> = {
  type: 'literal';
  value: Value;
};

export function literal<Value extends LiteralValue>(
  value: Value
): LiteralExpr<Value> {
  return { type: 'literal', value };
}

type BooleanExpr<Table extends TypedTableDef> =
  | {
      type: 'eq';
      left: ValueExpr<Table, any>;
      right: ValueExpr<Table, any>;
    }
  | {
      type: 'and';
      clauses: readonly [
        BooleanExpr<Table>,
        BooleanExpr<Table>,
        ...BooleanExpr<Table>[],
      ];
    }
  | {
      type: 'or';
      clauses: readonly [
        BooleanExpr<Table>,
        BooleanExpr<Table>,
        ...BooleanExpr<Table>[],
      ];
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

export function not<Table extends TypedTableDef>(
  clause: BooleanExpr<Table>
): BooleanExpr<Table> {
  return { type: 'not', clause };
}

export function and<Table extends TypedTableDef>(
  ...clauses: readonly [
    BooleanExpr<Table>,
    BooleanExpr<Table>,
    ...BooleanExpr<Table>[],
  ]
): BooleanExpr<Table> {
  return { type: 'and', clauses };
}

export function or<Table extends TypedTableDef>(
  ...clauses: readonly [
    BooleanExpr<Table>,
    BooleanExpr<Table>,
    ...BooleanExpr<Table>[],
  ]
): BooleanExpr<Table> {
  return { type: 'or', clauses };
}

function booleanExprToSql<Table extends TypedTableDef>(
  expr: BooleanExpr<Table>,
  tableAlias?: string
): string {
  switch (expr.type) {
    case 'eq':
      return `${valueExprToSql(expr.left, tableAlias)} = ${valueExprToSql(expr.right, tableAlias)}`;
    case 'and':
      return expr.clauses
        .map(c => booleanExprToSql(c, tableAlias))
        .map(wrapInParens)
        .join(' AND ');
    case 'or':
      return expr.clauses
        .map(c => booleanExprToSql(c, tableAlias))
        .map(wrapInParens)
        .join(' OR ');
    case 'not':
      return `NOT ${wrapInParens(booleanExprToSql(expr.clause, tableAlias))}`;
  }
}

function wrapInParens(sql: string): string {
  return `(${sql})`;
}

function valueExprToSql<Table extends TypedTableDef>(
  expr: ValueExpr<Table, any>,
  tableAlias?: string
): string {
  if (isLiteralExpr(expr)) {
    return literalValueToSql(expr.value);
  }
  const table = tableAlias ?? expr.table;
  return `${quoteIdentifier(table)}.${quoteIdentifier(expr.column)}`;
}

function literalValueToSql(value: unknown): string {
  if (value === null || value === undefined) {
    return 'NULL';
  }
  if (value instanceof Identity || value instanceof ConnectionId) {
    // We use this hex string syntax.
    return `0x${value.toHexString()}`;
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
      // It might be safer to error here?
      return `'${JSON.stringify(value).replace(/'/g, "''")}'`;
  }
}

function quoteIdentifier(name: string): string {
  return `"${name.replace(/"/g, '""')}"`;
}

function isLiteralExpr<Value>(
  expr: ValueExpr<any, Value>
): expr is LiteralExpr<Value & LiteralValue> {
  return (expr as LiteralExpr<Value>).type === 'literal';
}
