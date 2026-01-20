import { ConnectionId } from './connection_id';
import { Identity } from './identity';
import type { ColumnIndex, IndexColumns, IndexOpts } from './indexes';
import type { UntypedSchemaDef } from './schema';
import type { TableSchema } from './table_schema';
import type {
  ColumnBuilder,
  ColumnMetadata,
  RowBuilder,
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

// internal only — NOT exported.
// This is how we make sure queries are only created with our helpers.
const QueryBrand = Symbol('QueryBrand');

export interface TableTypedQuery<TableDef extends TypedTableDef> {
  readonly [QueryBrand]: true;
  readonly __table?: TableDef;
}

export interface RowTypedQuery<Row, ST> {
  readonly [QueryBrand]: true;
  // Phantom type to track the row type.
  readonly __row?: Row;
  readonly __algebraicType?: ST;
}

export type Query<TableDef extends TypedTableDef> = RowTypedQuery<
  RowType<TableDef>,
  TableDef['rowType']
>;

export const isRowTypedQuery = (val: unknown): val is RowTypedQuery<any, any> =>
  !!val && typeof val === 'object' && QueryBrand in (val as object);

export const isTypedQuery = (val: unknown): val is TableTypedQuery<any> =>
  !!val && typeof val === 'object' && QueryBrand in (val as object);

export function toSql(q: Query<any>): string {
  return (q as unknown as { toSql(): string }).toSql();
}

// A query builder with a single table.
type From<TableDef extends TypedTableDef> = Readonly<{
  where(
    predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
  ): From<TableDef>;
  rightSemijoin<RightTable extends TypedTableDef>(
    other: TableRef<RightTable>,
    on: (
      left: IndexedRowExpr<TableDef>,
      right: IndexedRowExpr<RightTable>
    ) => EqExpr<TableDef | RightTable>
  ): SemijoinBuilder<RightTable>;
  leftSemijoin<RightTable extends TypedTableDef>(
    other: TableRef<RightTable>,
    on: (
      left: IndexedRowExpr<TableDef>,
      right: IndexedRowExpr<RightTable>
    ) => EqExpr<TableDef | RightTable>
  ): SemijoinBuilder<TableDef>;
  build(): Query<TableDef>;
}>;

// A query builder with a semijoin.
type SemijoinBuilder<TableDef extends TypedTableDef> = Readonly<{
  where(
    predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
  ): SemijoinBuilder<TableDef>;
  build(): Query<TableDef>;
}>;

class SemijoinImpl<TableDef extends TypedTableDef>
  implements SemijoinBuilder<TableDef>, TableTypedQuery<TableDef>
{
  readonly [QueryBrand] = true;
  readonly type = 'semijoin' as const;
  constructor(
    readonly sourceQuery: FromBuilder<TableDef>,
    readonly filterQuery: FromBuilder<any>,
    readonly joinCondition: EqExpr<any>
  ) {
    if (sourceQuery.table.name === filterQuery.table.name) {
      // TODO: Handle aliasing properly instead of just forbidding it.
      throw new Error('Cannot semijoin a table to itself');
    }
  }

  build(): Query<TableDef> {
    return this as Query<TableDef>;
  }

  where(
    predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
  ): SemijoinImpl<TableDef> {
    const nextSourceQuery = this.sourceQuery.where(predicate);
    return new SemijoinImpl<TableDef>(
      nextSourceQuery,
      this.filterQuery,
      this.joinCondition
    );
  }

  toSql(): string {
    const left = this.filterQuery;
    const right = this.sourceQuery;
    const leftTable = quoteIdentifier(left.table.name);
    const rightTable = quoteIdentifier(right.table.name);
    let sql = `SELECT ${rightTable}.* FROM ${leftTable} JOIN ${rightTable} ON ${booleanExprToSql(this.joinCondition)}`;

    const clauses: string[] = [];
    if (left.whereClause) {
      clauses.push(booleanExprToSql(left.whereClause));
    }
    if (right.whereClause) {
      clauses.push(booleanExprToSql(right.whereClause));
    }

    if (clauses.length > 0) {
      const whereSql =
        clauses.length === 1
          ? clauses[0]
          : clauses.map(wrapInParens).join(' AND ');
      sql += ` WHERE ${whereSql}`;
    }

    return sql;
  }
}

class FromBuilder<TableDef extends TypedTableDef>
  implements From<TableDef>, TableTypedQuery<TableDef>
{
  readonly [QueryBrand] = true;
  constructor(
    readonly table: TableRef<TableDef>,
    readonly whereClause?: BooleanExpr<TableDef>
  ) {}

  where(
    predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
  ): FromBuilder<TableDef> {
    const newCondition = predicate(this.table.cols);
    const nextWhere = this.whereClause
      ? and(this.whereClause, newCondition)
      : newCondition;
    return new FromBuilder<TableDef>(this.table, nextWhere);
  }

  rightSemijoin<OtherTable extends TypedTableDef>(
    right: TableRef<OtherTable>,
    on: (
      left: IndexedRowExpr<TableDef>,
      right: IndexedRowExpr<OtherTable>
    ) => EqExpr<TableDef | OtherTable>
  ): SemijoinBuilder<OtherTable> {
    const sourceQuery = new FromBuilder(right);
    const joinCondition = on(
      this.table.indexedCols,
      right.indexedCols
    ) as EqExpr<any>;
    return new SemijoinImpl<OtherTable>(sourceQuery, this, joinCondition);
  }

  leftSemijoin<OtherTable extends TypedTableDef>(
    right: TableRef<OtherTable>,
    on: (
      left: IndexedRowExpr<TableDef>,
      right: IndexedRowExpr<OtherTable>
    ) => EqExpr<TableDef | OtherTable>
  ): SemijoinBuilder<TableDef> {
    const filterQuery = new FromBuilder(right);
    const joinCondition = on(
      this.table.indexedCols,
      right.indexedCols
    ) as EqExpr<any>;
    return new SemijoinImpl<TableDef>(this, filterQuery, joinCondition);
  }

  toSql(): string {
    return renderSelectSqlWithJoins(this.table, this.whereClause);
  }

  build(): Query<TableDef> {
    return this as Query<TableDef>;
  }
}

export type QueryBuilder<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in SchemaDef['tables'][number] as Tbl['name']]: TableRef<Tbl> &
    From<Tbl>;
} & {};

/**
 * A runtime reference to a table. This materializes the RowExpr for us.
 * TODO: Maybe add the full SchemaDef to the type signature depending on how joins will work.
 */
export type TableRef<TableDef extends TypedTableDef> = Readonly<{
  type: 'table';
  name: TableDef['name'];
  cols: RowExpr<TableDef>;
  indexedCols: IndexedRowExpr<TableDef>;
  // Maybe redundant.
  tableDef: TableDef;
}>;

class TableRefImpl<TableDef extends TypedTableDef>
  implements TableRef<TableDef>, From<TableDef>
{
  readonly type = 'table' as const;
  name: string;
  cols: RowExpr<TableDef>;
  indexedCols: IndexedRowExpr<TableDef>;
  tableDef: TableDef;
  constructor(tableDef: TableDef) {
    this.name = tableDef.name;
    this.cols = createRowExpr(tableDef);
    // this.indexedCols = createIndexedRowExpr(tableDef, this.cols);
    // TODO: we could create an indexedRowExpr to avoid having the extra columns.
    // Right now, the objects we pass will actually have all the columns, but the
    // type system will consider it an error.
    this.indexedCols = this.cols;
    this.tableDef = tableDef;
    Object.freeze(this);
  }

  asFrom(): FromBuilder<TableDef> {
    return new FromBuilder<TableDef>(this);
  }

  rightSemijoin<RightTable extends TypedTableDef>(
    other: TableRef<RightTable>,
    on: (
      left: IndexedRowExpr<TableDef>,
      right: IndexedRowExpr<RightTable>
    ) => EqExpr<TableDef | RightTable>
  ): SemijoinBuilder<RightTable> {
    return this.asFrom().rightSemijoin(other, on);
  }

  leftSemijoin<RightTable extends TypedTableDef>(
    other: TableRef<RightTable>,
    on: (
      left: IndexedRowExpr<TableDef>,
      right: IndexedRowExpr<RightTable>
    ) => EqExpr<TableDef | RightTable>
  ): SemijoinBuilder<TableDef> {
    return this.asFrom().leftSemijoin(other, on);
  }

  build(): Query<TableDef> {
    return this.asFrom().build();
  }

  toSql(): string {
    return this.asFrom().toSql();
  }

  where(
    predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
  ): FromBuilder<TableDef> {
    return this.asFrom().where(predicate);
  }
}

export type RefSource<TableDef extends TypedTableDef> =
  | TableRef<TableDef>
  | { ref(): TableRef<TableDef> };

export function createTableRefFromDef<TableDef extends TypedTableDef>(
  tableDef: TableDef
): TableRef<TableDef> {
  return new TableRefImpl<TableDef>(tableDef);
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
    const column = new ColumnExpression<TableDef, typeof columnName>(
      tableDef.name,
      columnName,
      columnBuilder.typeBuilder.algebraicType as InferSpacetimeTypeOfColumn<
        TableDef,
        typeof columnName
      >
    );
    row[columnName] = Object.freeze(column);
  }
  return Object.freeze(row) as RowExpr<TableDef>;
}

export function from<TableDef extends TypedTableDef>(
  source: RefSource<TableDef>
): From<TableDef> {
  return new FromBuilder(resolveTableRef(source));
}

function resolveTableRef<TableDef extends TypedTableDef>(
  source: RefSource<TableDef>
): TableRef<TableDef> {
  if (typeof (source as { ref?: unknown }).ref === 'function') {
    return (source as { ref(): TableRef<TableDef> }).ref();
  }
  return source as TableRef<TableDef>;
}

function renderSelectSqlWithJoins<Table extends TypedTableDef>(
  table: TableRef<Table>,
  where?: BooleanExpr<Table>,
  extraClauses: readonly string[] = []
): string {
  const quotedTable = quoteIdentifier(table.name);
  const sql = `SELECT * FROM ${quotedTable}`;
  const clauses: string[] = [];
  if (where) clauses.push(booleanExprToSql(where));
  clauses.push(...extraClauses);
  if (clauses.length === 0) return sql;
  const whereSql =
    clauses.length === 1 ? clauses[0] : clauses.map(wrapInParens).join(' AND ');
  return `${sql} WHERE ${whereSql}`;
}

// TODO: Just use UntypedTableDef if they end up being the same.
export type TypedTableDef<
  Columns extends Record<
    string,
    ColumnBuilder<any, any, ColumnMetadata<any>>
  > = Record<string, ColumnBuilder<any, any, ColumnMetadata<any>>>,
> = {
  name: string;
  columns: Columns;
  indexes: readonly IndexOpts<any>[];
  rowType: RowBuilder<Columns>['algebraicType']['value'];
};

export type TableSchemaAsTableDef<
  TSchema extends TableSchema<any, any, readonly any[]>,
> = {
  name: TSchema['tableName'];
  columns: TSchema['rowType']['row'];
  indexes: TSchema['idxs'];
};

type RowType<TableDef extends TypedTableDef> = {
  [K in keyof TableDef['columns']]: TableDef['columns'][K] extends ColumnBuilder<
    infer T,
    any,
    any
  >
    ? T
    : never;
};

// TODO: Consider making a smaller version of these types that doesn't expose the internals.
// Restricting it later should not break anyone in practice.
export type ColumnExpr<
  TableDef extends TypedTableDef,
  ColumnName extends ColumnNames<TableDef>,
> = ColumnExpression<TableDef, ColumnName>;

type ColumnSpacetimeType<Col extends ColumnExpr<any, any>> =
  Col extends ColumnExpr<infer T, infer N>
    ? InferSpacetimeTypeOfColumn<T, N>
    : never;

// TODO: This checks that they match, but we also need to make sure that they are comparable types,
// since you can use product types at all.
type ColumnSameSpacetime<
  ThisTable extends TypedTableDef,
  ThisCol extends ColumnNames<ThisTable>,
  OtherCol extends ColumnExpr<any, any>,
> = [InferSpacetimeTypeOfColumn<ThisTable, ThisCol>] extends [
  ColumnSpacetimeType<OtherCol>,
]
  ? [ColumnSpacetimeType<OtherCol>] extends [
      InferSpacetimeTypeOfColumn<ThisTable, ThisCol>,
    ]
    ? OtherCol
    : never
  : never;

// Helper to get the table back from a column.
type ExtractTable<Col extends ColumnExpr<any, any>> =
  Col extends ColumnExpr<infer T, any> ? T : never;

export class ColumnExpression<
  TableDef extends TypedTableDef,
  ColumnName extends ColumnNames<TableDef>,
> {
  readonly type = 'column' as const;
  readonly column: ColumnName;
  readonly table: TableDef['name'];
  // phantom: actual runtime value is undefined
  readonly tsValueType?: RowType<TableDef>[ColumnName];
  readonly spacetimeType: InferSpacetimeTypeOfColumn<TableDef, ColumnName>;

  constructor(
    table: TableDef['name'],
    column: ColumnName,
    spacetimeType: InferSpacetimeTypeOfColumn<TableDef, ColumnName>
  ) {
    this.table = table;
    this.column = column;
    this.spacetimeType = spacetimeType;
  }

  eq(literal: LiteralValue & RowType<TableDef>[ColumnName]): EqExpr<TableDef>;
  eq<OtherCol extends ColumnExpr<any, any>>(
    value: ColumnSameSpacetime<TableDef, ColumnName, OtherCol>
  ): EqExpr<TableDef | ExtractTable<OtherCol>>;

  // These types could be tighted, but since we declare the overloads above, it doesn't weaken the API surface.
  eq(x: any): any {
    return {
      type: 'eq',
      left: this as unknown as ValueExpr<TableDef, any>,
      right: normalizeValue(x) as ValueExpr<TableDef, any>,
    } as EqExpr<TableDef>;
  }

  lt(
    literal: LiteralValue & RowType<TableDef>[ColumnName]
  ): BooleanExpr<TableDef>;
  lt<OtherCol extends ColumnExpr<any, any>>(
    value: ColumnSameSpacetime<TableDef, ColumnName, OtherCol>
  ): BooleanExpr<TableDef | ExtractTable<OtherCol>>;

  // These types could be tighted, but since we declare the overloads above, it doesn't weaken the API surface.
  lt(x: any): any {
    return {
      type: 'lt',
      left: this as unknown as ValueExpr<TableDef, any>,
      right: normalizeValue(x) as ValueExpr<TableDef, any>,
    } as BooleanExpr<TableDef>;
  }
  lte(
    literal: LiteralValue & RowType<TableDef>[ColumnName]
  ): BooleanExpr<TableDef>;
  lte<OtherCol extends ColumnExpr<any, any>>(
    value: ColumnSameSpacetime<TableDef, ColumnName, OtherCol>
  ): BooleanExpr<TableDef | ExtractTable<OtherCol>>;

  // These types could be tighted, but since we declare the overloads above, it doesn't weaken the API surface.
  lte(x: any): any {
    return {
      type: 'lte',
      left: this as unknown as ValueExpr<TableDef, any>,
      right: normalizeValue(x) as ValueExpr<TableDef, any>,
    } as BooleanExpr<TableDef>;
  }

  gt(
    literal: LiteralValue & RowType<TableDef>[ColumnName]
  ): BooleanExpr<TableDef>;
  gt<OtherCol extends ColumnExpr<any, any>>(
    value: ColumnSameSpacetime<TableDef, ColumnName, OtherCol>
  ): BooleanExpr<TableDef | ExtractTable<OtherCol>>;

  // These types could be tighted, but since we declare the overloads above, it doesn't weaken the API surface.
  gt(x: any): any {
    return {
      type: 'gt',
      left: this as unknown as ValueExpr<TableDef, any>,
      right: normalizeValue(x) as ValueExpr<TableDef, any>,
    } as BooleanExpr<TableDef>;
  }
  gte(
    literal: LiteralValue & RowType<TableDef>[ColumnName]
  ): BooleanExpr<TableDef>;
  gte<OtherCol extends ColumnExpr<any, any>>(
    value: ColumnSameSpacetime<TableDef, ColumnName, OtherCol>
  ): BooleanExpr<TableDef | ExtractTable<OtherCol>>;

  // These types could be tighted, but since we declare the overloads above, it doesn't weaken the API surface.
  gte(x: any): any {
    return {
      type: 'gte',
      left: this as unknown as ValueExpr<TableDef, any>,
      right: normalizeValue(x) as ValueExpr<TableDef, any>,
    } as BooleanExpr<TableDef>;
  }
}

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

// For composite indexes, we only consider it as an index over the first column in the index.
type FirstIndexColumn<I extends IndexOpts<any>> =
  IndexColumns<I> extends readonly [infer Head extends string, ...infer _Rest]
    ? Head
    : never;

// Columns that are indexed by something in the indexes: [...] part.
type ExplicitIndexedColumns<TableDef extends TypedTableDef> =
  TableDef['indexes'][number] extends infer I
    ? I extends IndexOpts<ColumnNames<TableDef>>
      ? FirstIndexColumn<I> & ColumnNames<TableDef>
      : never
    : never;

// Columns with an index defined on the column definition.
type MetadataIndexedColumns<TableDef extends TypedTableDef> = {
  [K in ColumnNames<TableDef>]: ColumnIndex<
    K,
    TableDef['columns'][K]['columnMetadata']
  > extends never
    ? never
    : K;
}[ColumnNames<TableDef>];

export type IndexedColumnNames<TableDef extends TypedTableDef> =
  | ExplicitIndexedColumns<TableDef>
  | MetadataIndexedColumns<TableDef>;

export type IndexedRowExpr<TableDef extends TypedTableDef> = Readonly<{
  readonly [C in IndexedColumnNames<TableDef>]: ColumnExpr<TableDef, C>;
}>;

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

type ValueLike = LiteralValue | ColumnExpr<any, any> | LiteralExpr<any>;
type ValueInput<TableDef extends TypedTableDef> =
  | ValueLike
  | ValueExpr<TableDef, any>;

export type ValueExpr<TableDef extends TypedTableDef, Value> =
  | LiteralExpr<Value & LiteralValue>
  | ColumnExprForValue<TableDef, Value>;

type LiteralExpr<Value> = {
  type: 'literal';
  value: Value;
};

export function literal<Value extends LiteralValue>(
  value: Value
): ValueExpr<never, Value> {
  return { type: 'literal', value };
}

// This is here to take literal values and wrap them in an AST node.
function normalizeValue(val: ValueInput<any>): ValueExpr<any, any> {
  if ((val as LiteralExpr<any>).type === 'literal')
    return val as LiteralExpr<any>;
  if (
    typeof val === 'object' &&
    val != null &&
    'type' in (val as any) &&
    (val as any).type === 'column'
  ) {
    return val as ColumnExpr<any, any>;
  }
  return literal(val as LiteralValue);
}

type EqExpr<Table extends TypedTableDef = any> = {
  type: 'eq';
  left: ValueExpr<Table, any>;
  right: ValueExpr<Table, any>;
} & {
  _tableType?: Table;
};

type BooleanExpr<Table extends TypedTableDef> = (
  | {
      type: 'eq' | 'ne' | 'gt' | 'lt' | 'gte' | 'lte';
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
    }
) & {
  _tableType?: Table;
  // readonly [BooleanExprBrand]: Table?;
};

export function not<T extends TypedTableDef>(
  clause: BooleanExpr<T>
): BooleanExpr<T> {
  return { type: 'not', clause };
}

export function and<T extends TypedTableDef>(
  ...clauses: readonly [BooleanExpr<T>, BooleanExpr<T>, ...BooleanExpr<T>[]]
): BooleanExpr<T> {
  return { type: 'and', clauses };
}

export function or<T extends TypedTableDef>(
  ...clauses: readonly [BooleanExpr<T>, BooleanExpr<T>, ...BooleanExpr<T>[]]
): BooleanExpr<T> {
  return { type: 'or', clauses };
}

function booleanExprToSql<Table extends TypedTableDef>(
  expr: BooleanExpr<Table>,
  tableAlias?: string
): string {
  switch (expr.type) {
    case 'eq':
      return `${valueExprToSql(expr.left, tableAlias)} = ${valueExprToSql(expr.right, tableAlias)}`;
    case 'ne':
      return `${valueExprToSql(expr.left, tableAlias)} <> ${valueExprToSql(expr.right, tableAlias)}`;
    case 'gt':
      return `${valueExprToSql(expr.left, tableAlias)} > ${valueExprToSql(expr.right, tableAlias)}`;
    case 'gte':
      return `${valueExprToSql(expr.left, tableAlias)} >= ${valueExprToSql(expr.right, tableAlias)}`;
    case 'lt':
      return `${valueExprToSql(expr.left, tableAlias)} < ${valueExprToSql(expr.right, tableAlias)}`;
    case 'lte':
      return `${valueExprToSql(expr.left, tableAlias)} <= ${valueExprToSql(expr.right, tableAlias)}`;
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

// TODO: Fix this.
function _createIndexedRowExpr<TableDef extends TypedTableDef>(
  tableDef: TableDef,
  cols: RowExpr<TableDef>
): IndexedRowExpr<TableDef> {
  const indexed = new Set<ColumnNames<TableDef>>();
  for (const idx of tableDef.indexes) {
    if ('columns' in idx) {
      const [first] = idx.columns;
      if (first) indexed.add(first);
    } else if ('column' in idx) {
      indexed.add(idx.column);
    }
  }
  const pickedEntries = [...indexed].map(name => [name, cols[name]]);
  return Object.freeze(
    Object.fromEntries(pickedEntries)
  ) as IndexedRowExpr<TableDef>;
}
