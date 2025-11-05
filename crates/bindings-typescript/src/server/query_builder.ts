import type {
  ColumnExpr,
  ColumnSpacetimeType,
  IndexExpr,
  IndexExprs,
  IndexValueType,
  IndexValWithSpacetime,
  RowExpr,
  TableIndexNames,
  TableIndexes,
  TableSchema,
  RowType,
  UntypedTableDef,
} from './table';
import type { IndexOpts } from './indexes';
import type { Identity } from '../lib/identity';

type TableLike = TableSchema<any, any, readonly any[]> | UntypedTableDef;

type TableCtx<Table extends TableLike> = Table extends TableSchema<
  any,
  any,
  readonly any[]
>
  ? TableSchemaAsTableDef<Table>
  : Table;

type ColumnNames<Table extends TableLike> = keyof RowType<TableCtx<Table>> &
  string;

export type TableRef<Table extends TableLike> = {
  tableInput: Table;
  tableDef: TableCtx<Table>;
  tableName: TableCtx<Table>['name'];
  row: RowExpr<TableCtx<Table>>;
  indexes: IndexExprs<TableCtx<Table>>;
};

export type Semijoin<
  LeftTable extends TableLike,
  RightTable extends TableLike,
  LeftIndex extends TableIndexNames<TableCtx<LeftTable>>,
  RightIndex extends TableIndexNames<TableCtx<RightTable>>,
> = Readonly<{
  left: TableScan<LeftTable>;
  leftIndex: IndexExpr<TableCtx<LeftTable>, LeftIndex>;
  rightIndex: IndexExpr<TableCtx<RightTable>, RightIndex>;
  toSql: () => string;
}>;

type SameIndexSpacetimeType<
  T1 extends TableLike,
  I1 extends TableIndexNames<TableCtx<T1>>,
  T2 extends TableLike,
  I2 extends TableIndexNames<TableCtx<T2>>,
> = SameIndexSpacetimeExpr<
  IndexExpr<TableCtx<T1>, I1>,
  IndexExpr<TableCtx<T2>, I2>
>;

type SameIndexSpacetimeExpr<
  Left,
  Right,
> = Left extends { valueInfo: infer LeftInfo }
  ? Right extends { valueInfo: infer RightInfo }
    ? LeftInfo extends RightInfo
      ? RightInfo extends LeftInfo
        ? true
        : false
      : false
    : false
  : false;

export type ToSql = {
  /**
   * Converts the query to its SQL representation.
   * @returns The SQL string representing the query.
   */
  toSql(): string;
};

type ColumnExprForValue<Table extends TableLike, Value> = {
  [C in ColumnNames<Table>]: RowType<TableCtx<Table>>[C] extends Value
    ? ColumnExpr<TableCtx<Table>, C>
    : never;
}[ColumnNames<Table>];

type LiteralExpr<Value> = {
  type: 'literal';
  value: Value;
};

export type ValueExpr<Table extends TableLike, Value> =
  | ColumnExprForValue<Table, Value>
  | LiteralExpr<Value>;

type ExprValueType<Table extends TableLike, ExprT> =
  ExprT extends ColumnExpr<
      TableCtx<Table>,
      infer Column extends ColumnNames<Table>
    >
    ? RowType<TableCtx<Table>>[Column]
    : ExprT extends LiteralExpr<infer LiteralValue>
      ? LiteralValue
      : never;

type WidenComparableLiteral<T> = [T] extends [null | undefined]
  ? null | undefined
  : T extends number
    ? number
    : T extends string
      ? string
      : T extends boolean
        ? boolean
        : T;

type ExprSpacetimeType<Table extends TableLike, ExprT> =
  ExprT extends ColumnExpr<
      TableCtx<Table>,
      infer Column extends ColumnNames<Table>
    >
    ? ColumnSpacetimeType<TableCtx<Table>, Column>
    : never;

type ValueTypesComparable<
  Table extends TableLike,
  LeftExpr,
  RightExpr,
> =
  WidenComparableLiteral<ExprValueType<Table, LeftExpr>> extends
    | null
    | undefined
    ? true
    : WidenComparableLiteral<ExprValueType<Table, RightExpr>> extends
          | null
          | undefined
      ? true
      : [
            WidenComparableLiteral<ExprValueType<Table, LeftExpr>> &
              WidenComparableLiteral<ExprValueType<Table, RightExpr>>,
          ] extends [never]
        ? false
        : true;

type SpacetimeTypesComparable<
  Table extends TableLike,
  LeftExpr,
  RightExpr,
> =
  ExprSpacetimeType<Table, LeftExpr> extends never
    ? true
    : ExprSpacetimeType<Table, RightExpr> extends never
      ? true
      : [
            ExprSpacetimeType<Table, LeftExpr>,
            ExprSpacetimeType<Table, RightExpr>,
          ] extends [infer LeftSpace, infer RightSpace]
        ? LeftSpace extends RightSpace
          ? RightSpace extends LeftSpace
            ? true
            : false
          : false
        : false;

type ComparableValueExprs<
  Table extends TableLike,
  LeftExpr,
  RightExpr,
> =
  ValueTypesComparable<Table, LeftExpr, RightExpr> extends true
    ? SpacetimeTypesComparable<Table, LeftExpr, RightExpr>
    : false;

type BooleanExpr<Table extends TableLike> =
  | {
      type: 'eq';
      left: ValueExpr<Table, any>;
      right: ValueExpr<Table, any>;
    }
  | {
      type: 'gt';
      left: ValueExpr<Table, any>;
      right: ValueExpr<Table, any>;
    }
  | {
      type: 'lt';
      left: ValueExpr<Table, any>;
      right: ValueExpr<Table, any>;
    }
  | {
      type: 'not';
      val: BooleanExpr<Table>;
    }
  | {
      type: 'and';
      left: BooleanExpr<Table>;
      right: BooleanExpr<Table>;
    };

export type Expr<
  Table extends TableLike,
  Value,
> = Value extends boolean ? BooleanExpr<Table> : ValueExpr<Table, Value>;

type RenderOptions = {
  tableAlias?: string;
};

function quoteIdentifier(identifier: string): string {
  return `"${identifier.replace(/"/g, '""')}"`;
}

function escapeString(value: string): string {
  return value.replace(/'/g, "''");
}

function valueToSql(value: unknown): string {
  if (value === null || value === undefined) {
    return 'NULL';
  }
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) {
      throw new TypeError('Cannot serialize non-finite numbers to SQL');
    }
    return String(value);
  }
  if (typeof value === 'bigint') {
    return value.toString();
  }
  if (typeof value === 'boolean') {
    return value ? 'TRUE' : 'FALSE';
  }
  if (typeof value === 'string') {
    return `'${escapeString(value)}'`;
  }
  if (value instanceof Date) {
    return `'${value.toISOString()}'`;
  }
  if (
    value != null &&
    typeof (value as { toString: () => string }).toString === 'function' &&
    (value as { toString: () => string }).toString !== Object.prototype.toString
  ) {
    return `'${escapeString(value.toString())}'`;
  }
  throw new TypeError(
    `Unsupported value for SQL serialization: ${String(value)}`
  );
}

function isNullLiteral<Table extends TableLike>(
  expr: ValueExpr<Table, any>
): boolean {
  return (
    expr.type === 'literal' && (expr.value === null || expr.value === undefined)
  );
}

function valueExprToSql<Table extends TableLike>(
  expr: ValueExpr<Table, any>,
  options: RenderOptions = {}
): string {
  if (expr.type === 'literal') {
    return valueToSql(expr.value);
  }
  const alias = options.tableAlias ?? expr.table;
  return `${quoteIdentifier(alias)}.${quoteIdentifier(expr.column)}`;
}

function booleanExprToSql<Table extends TableLike>(
  expr: BooleanExpr<Table>,
  options: RenderOptions = {}
): string {
  switch (expr.type) {
    case 'eq': {
      const leftNull = isNullLiteral(expr.left);
      const rightNull = isNullLiteral(expr.right);
      if (leftNull && rightNull) return 'NULL IS NULL';
      if (leftNull) {
        return `${valueExprToSql(expr.right, options)} IS NULL`;
      }
      if (rightNull) {
        return `${valueExprToSql(expr.left, options)} IS NULL`;
      }
      const leftSql = valueExprToSql(expr.left, options);
      const rightSql = valueExprToSql(expr.right, options);
      return `${leftSql} = ${rightSql}`;
    }
    case 'gt': {
      const leftSql = valueExprToSql(expr.left, options);
      const rightSql = valueExprToSql(expr.right, options);
      return `${leftSql} > ${rightSql}`;
    }
    case 'lt': {
      const leftSql = valueExprToSql(expr.left, options);
      const rightSql = valueExprToSql(expr.right, options);
      return `${leftSql} < ${rightSql}`;
    }
    case 'and': {
      const left = booleanExprToSql(expr.left, options);
      const right = booleanExprToSql(expr.right, options);
      return `(${left}) AND (${right})`;
    }
    default: {
      return assertNever(expr as never);
    }
  }
}

function renderFilters<Table extends TableLike>(
  filters: readonly BooleanExpr<Table>[],
  tableAlias: string
): string | undefined {
  if (filters.length === 0) return undefined;
  if (filters.length === 1) {
    return booleanExprToSql(filters[0], { tableAlias });
  }
  return filters
    .map(expr => `(${booleanExprToSql(expr, { tableAlias })})`)
    .join(' AND ');
}

function isBooleanExpr<Table extends TableLike>(
  expr: Expr<Table, any>
): expr is BooleanExpr<Table> {
  return (
    expr.type === 'eq' ||
    expr.type === 'gt' ||
    expr.type === 'lt' ||
    expr.type === 'and'
  );
}

export function exprToSql<Table extends TableLike, Value>(
  expr: Expr<Table, Value>,
  options: RenderOptions = {}
): string {
  if (isBooleanExpr(expr)) {
    return booleanExprToSql(expr, options);
  }
  return valueExprToSql(expr as ValueExpr<Table, Value>, options);
}

type TableSchemaAsTableDef<
  TSchema extends TableSchema<any, any, readonly any[]>,
> = {
  name: TSchema['tableName'];
  columns: TSchema['rowType']['row'];
  indexes: TSchema['idxs'];
};

function assertNever(value: never): never {
  throw new Error(
    `Unexpected filter condition ${(value as { type?: unknown })?.type ?? value}`
  );
}

/**
 * Types that we want to define:
 * - SchemaView: a view of the database schema, which will let us get references to tables.
 *     - Our viewContext function will have a schema view.
 * - TableRef: a reference to a table in a database, which has metadata attached including the table name, columns, indexes, etc.
 * - RowExpr: an expression representing a row in a table, with typed columns. This is used to build predicates for queries.
 * - Expr: an expression that represents some kind of value, like a column reference, a literal value, or a computed expression.
 * - IndexRef: a reference to an index on a table, which has to track metadata about the table and columns.
 * - Selection: represents a selection of rows from a table, potentially with a set of filters.
 * - TableQuery: represents a query that returns results for a specific table. This is something we can convert to sql, but doesn't include
 *             operations to extend the query with more filters or joins.
 * - TableQueryBuilder: a query builder for a specific table, which allows filtering, joining, etc.
 *                      because we can only do a single join, a join will return a TableQuery, not a TableQueryBuilder.
 */

/**
 * Key types and interfaces for the query builder.
 *
 * Should I have a subscribable interface?
 */

export type Query<Table extends TableLike> = {
  /**
   * Adds a filter expression to the query.
   * @param predicate A function that produces a condition when given a typed
   *                  projection of the table row.
   */
  filter(
    predicate: (row: RowExpr<TableCtx<Table>>) => BooleanExpr<Table>
  ): Query<Table>;

  /**
   * Converts the query to its SQL representation.
   * @returns The SQL string representing the query.
   */
  toSql(): string;
};

function createRowExpr<TableDef extends UntypedTableDef>(
  tableDef: TableDef
): RowExpr<TableDef> {
  const row: Partial<
    Record<ColumnNames<TableDef>, ColumnExpr<TableDef, ColumnNames<TableDef>>>
  > = Object.create(null);
  for (const columnName of Object.keys(
    tableDef.columns
  ) as ColumnNames<TableDef>[]) {
    const columnBuilder = tableDef.columns[columnName];
    const spacetimeType =
      columnBuilder != null
        ? (columnBuilder.typeBuilder.algebraicType as ColumnSpacetimeType<
            TableDef,
            typeof columnName
          >)
        : (undefined as unknown as ColumnSpacetimeType<
            TableDef,
            typeof columnName
          >);
    row[columnName] = {
      type: 'column',
      column: columnName,
      table: tableDef.name,
      valueType: undefined as unknown as RowType<TableDef>[typeof columnName],
      spacetimeType,
    } as ColumnExpr<TableDef, typeof columnName>;
  }
  return row as RowExpr<TableDef>;
}

function createIndexExprs<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  rowExpr: RowExpr<TableDef>,
  originalIndexes?: readonly IndexOpts<any>[]
): IndexExprs<TableDef> {
  const indexes: Partial<IndexExprs<TableDef>> = Object.create(null);

  const putIndex = (
    name: string,
    entry: IndexExpr<TableDef, TableIndexNames<TableDef>>
  ) => {
    if ((indexes as Record<string, IndexExpr<TableDef, any>>)[name] != null) {
      return;
    }
    (indexes as Record<string, IndexExpr<TableDef, any>>)[name] = entry;
  };

  const columnNames = Object.keys(tableDef.columns) as ColumnNames<TableDef>[];
  for (const columnName of columnNames) {
    const columnBuilder = tableDef.columns[columnName];
    if (!columnBuilder || typeof columnBuilder !== 'object') continue;
    const metadata = columnBuilder.columnMetadata ?? {};
    const isUnique =
      metadata.isUnique === true || metadata.isPrimaryKey === true;
    const algorithm = metadata.indexType ?? (isUnique ? 'btree' : undefined);
    if (algorithm == null) continue;

    const columnKey = columnName as TableIndexNames<TableDef>;
    const entry = {
      type: 'index',
      tableDef,
      table: tableDef.name,
      index: columnKey,
      name: columnName,
      unique: isUnique,
      columns: [columnName],
      algorithm,
      valueType: rowExpr[columnName].valueType,
      valueInfo: undefined as IndexExpr<TableDef, typeof columnKey>['valueInfo'],
    } as IndexExpr<TableDef, TableIndexNames<TableDef>>;
    putIndex(columnKey, entry);
  }

  const explicitIndexes: Array<IndexOpts<string>> = originalIndexes
    ? Array.from(originalIndexes)
    : Array.from(tableDef.indexes ?? []);

  for (const indexOpt of explicitIndexes) {
    const columnNamesExplicit =
      'columns' in indexOpt ? indexOpt.columns : [indexOpt.column];
    if (columnNamesExplicit.length === 0) continue;
    const algorithm = indexOpt.algorithm;
    const unique = columnNamesExplicit.every(columnName => {
      const metadata = tableDef.columns[columnName]?.columnMetadata ?? {};
      return metadata.isUnique === true || metadata.isPrimaryKey === true;
    });
    const accessorName =
      indexOpt.name ??
      `${tableDef.name}_${columnNamesExplicit.join('_')}_idx_${algorithm}`;
    const normalizedColumns =
      columnNamesExplicit as readonly ColumnNames<TableDef>[];
    const singleColumn =
      normalizedColumns.length === 1 ? normalizedColumns[0]! : undefined;
    const indexName = accessorName as TableIndexNames<TableDef>;
    const entry = {
      type: 'index',
      tableDef,
      table: tableDef.name,
      index: indexName,
      name: accessorName,
      unique,
      columns: columnNamesExplicit,
      algorithm,
      valueType:
        singleColumn !== undefined
          ? rowExpr[singleColumn].valueType
          : normalizedColumns.map(column => rowExpr[column].valueType),
      valueInfo: undefined as IndexExpr<TableDef, typeof indexName>['valueInfo'],
    } as IndexExpr<TableDef, typeof indexName>;
    putIndex(accessorName, entry);
  }

  return indexes as IndexExprs<TableDef>;
}

function createTableRefFromDef<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  originalIndexes?: readonly IndexOpts<any>[]
): TableRef<TableDef> {
  const row = createRowExpr(tableDef);
  return {
    tableInput: tableDef,
    tableDef,
    tableName: tableDef.name,
    row,
    indexes: createIndexExprs(tableDef, row, originalIndexes),
  };
}

function materializeSchemaIndexes<
  TSchema extends TableSchema<any, any, readonly any[]>,
>(
  tableSchema: TSchema
): TableSchemaAsTableDef<TSchema>['indexes'] {
  const columnNames = Object.keys(tableSchema.rowType.row);
  const indexes = tableSchema.tableDef.indexes.map(index => {
    const name = index.accessorName ?? index.name;
    if (index.algorithm.tag === 'Direct') {
      const column = columnNames[index.algorithm.value]!;
      const base = {
        algorithm: 'direct' as const,
        column,
      };
      return name != null ? { ...base, name } : base;
    }
    const columns = index.algorithm.value.map(idx => columnNames[idx]!);
    const base = {
      algorithm: 'btree' as const,
      columns: columns as readonly string[],
    };
    return name != null ? { ...base, name } : base;
  });
  return indexes as TableSchemaAsTableDef<TSchema>['indexes'];
}

export function createTableRef<TableDef extends UntypedTableDef>(
  tableDef: TableDef
): TableRef<TableDef>;
export function createTableRef<
  TSchema extends TableSchema<any, any, readonly any[]>,
>(tableSchema: TSchema): TableRef<TSchema>;
export function createTableRef(
  tableDefOrSchema:
    | UntypedTableDef
    | TableSchema<any, Record<string, any>, readonly any[]>
): TableRef<any> {
  if ('rowType' in tableDefOrSchema) {
    const tableSchema = tableDefOrSchema;
    const materializedIndexes = materializeSchemaIndexes(tableSchema);
    const tableDef = {
      name: tableSchema.tableName,
      columns: tableSchema.rowType.row,
      indexes: materializedIndexes,
    } as TableSchemaAsTableDef<typeof tableSchema>;
    const ref = createTableRefFromDef(tableDef, materializedIndexes);
    return {
      ...ref,
      tableInput: tableSchema,
    } as TableRef<typeof tableSchema>;
  }
  return createTableRefFromDef(tableDefOrSchema);
}

export class TableScan<Table extends TableLike> {
  readonly table: TableRef<Table>;
  readonly filters: readonly BooleanExpr<Table>[];

  constructor(
    table: TableRef<Table>,
    filters: readonly BooleanExpr<Table>[] = []
  ) {
    this.table = table;
    this.filters = filters;
  }

  addFilter(
    predicate: (row: RowExpr<TableCtx<Table>>) => BooleanExpr<Table>
  ): TableScan<Table> {
    const filterExpr = predicate(this.table.row);
    return new TableScan(this.table, [
      ...this.filters,
      filterExpr,
    ] as readonly BooleanExpr<Table>[]);
  }

  toSql(): string {
    const tableName = this.table.tableName;
    const tableIdent = quoteIdentifier(tableName);
    let sql = `SELECT ${tableIdent}.* FROM ${tableIdent}`;
    const where = renderFilters(this.filters, tableName);
    if (where) {
      sql += ` WHERE ${where}`;
    }
    return sql;
  }

  semijoin<
    LeftIndex extends TableIndexNames<TableCtx<Table>>,
    RightTable extends TableLike,
    RightIndex extends TableIndexNames<TableCtx<RightTable>>,
  >(
    leftIndex: IndexExpr<TableCtx<Table>, LeftIndex>,
    rightIndex: SameIndexSpacetimeType<
      Table,
      LeftIndex,
      RightTable,
      RightIndex
    > extends true
      ? IndexExpr<TableCtx<RightTable>, RightIndex>
      : never
  ): Semijoin<Table, RightTable, LeftIndex, RightIndex> {
    if (leftIndex.table !== this.table.tableName) {
      throw new Error('Left index must belong to the same table as the scan');
    }
    if (rightIndex.table === this.table.tableName) {
      throw new Error('Right index must belong to a different table');
    }
    if (leftIndex.columns.length !== rightIndex.columns.length) {
      throw new Error(
        'Indexes must have the same number of columns for semijoin'
      );
    }
    const leftScan = this;
    const normalizedRightIndex = rightIndex as IndexExpr<
      TableCtx<RightTable>,
      RightIndex
    >;
    return {
      left: leftScan,
      leftIndex,
      rightIndex: normalizedRightIndex,
      toSql(): string {
        const leftTableName = leftScan.table.tableName;
        const leftIdent = quoteIdentifier(leftTableName);
        const whereClauses: string[] = [];
        const leftFiltersSql = renderFilters(leftScan.filters, leftTableName);
        if (leftFiltersSql) {
          whereClauses.push(leftFiltersSql);
        }
        const rightTableName = normalizedRightIndex.table;
        const rightIdent = quoteIdentifier(rightTableName);
        const joinConditions = leftIndex.columns.map((leftColumn, idx) => {
          const rightColumn = normalizedRightIndex.columns[idx];
          const leftSql = `${leftIdent}.${quoteIdentifier(leftColumn)}`;
          const rightSql = `${rightIdent}.${quoteIdentifier(rightColumn)}`;
          return `(${leftSql} = ${rightSql})`;
        });
        const joinConditionSql =
          joinConditions.length === 1
            ? joinConditions[0]
            : joinConditions.join(' AND ');
        const existsSql = `EXISTS (SELECT 1 FROM ${rightIdent} WHERE ${joinConditionSql})`;
        whereClauses.push(existsSql);
        let sql = `SELECT ${leftIdent}.* FROM ${leftIdent}`;
        if (whereClauses.length > 0) {
          sql += ` WHERE ${whereClauses.join(' AND ')}`;
        }
        return sql;
      },
    } as Semijoin<Table, RightTable, LeftIndex, RightIndex>;
  }
}

export function createTableScan<Table extends TableLike>(
  table: Table
): TableScan<Table> {
  const ref = (
    'rowType' in table
      ? createTableRef(table)
      : createTableRef(table as UntypedTableDef)
  ) as TableRef<Table>;
  return new TableScan(ref);
}

class QueryBuilder<Table extends TableLike> implements Query<Table> {
  #scan: TableScan<Table>;

  constructor(scan: TableScan<Table>) {
    this.#scan = scan;
  }

  filter(
    predicate: (row: RowExpr<TableCtx<Table>>) => BooleanExpr<Table>
  ): Query<Table> {
    const nextScan = this.#scan.addFilter(predicate);
    return new QueryBuilder(nextScan);
  }

  toSql(): string {
    return this.#scan.toSql();
  }
}

export function createQuery<Table extends TableLike>(
  table: Table
): Query<Table>;
export function createQuery<
  TSchema extends TableSchema<any, any, readonly any[]>,
>(tableSchema: TSchema): Query<TSchema>;
export function createQuery(
  table: TableLike
): Query<TableLike> {
  if ('rowType' in table) {
    const tableSchema = table;
    const tableRef = createTableRef(tableSchema);
    return new QueryBuilder(new TableScan(tableRef));
  }
  const scan = createTableScan(table as UntypedTableDef);
  return new QueryBuilder(scan);
}

export function literal<Value>(value: Value): LiteralExpr<Value> {
  return { type: 'literal', value };
}

/*
export function eq<
  Table extends TableLike,
  LeftExpr extends ValueExpr<Table, any>,
  RightExpr extends ValueExpr<Table, any>,
>(
  left: LeftExpr,
  right: RightExpr
): ComparableValueExprs<Table, LeftExpr, RightExpr> extends true
  ? Expr<Table, boolean>
  : never;
  */
/**
 * Other restrictions to consider encoding:
 *  - eq shouldn't allow any option types.
 *  -
 */

type ValueTypesThatAreComparable = string | number | boolean | Identity;
/**
 *
 * @param left
 * @param right
 * @returns
 */
export function eq<Table extends TableLike, Value>(
  left: ValueExpr<Table, Value & ValueTypesThatAreComparable>,
  right: ValueExpr<Table, Value & ValueTypesThatAreComparable>
): Expr<Table, boolean> {
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
/*
export function eq<Table extends TableLike>(
  left: ValueExpr<Table, any>,
  right: ValueExpr<Table, any>
): Expr<Table, boolean> {
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
  */

export function gt<Table extends TableLike, Value>(
  left: ValueExpr<Table, Value>,
  right: ValueExpr<Table, Value>
): Expr<Table, boolean> {
  return {
    type: 'gt',
    left,
    right,
  };
}

export function lt<Table extends TableLike, Value>(
  left: ValueExpr<Table, Value>,
  right: ValueExpr<Table, Value>
): Expr<Table, boolean> {
  return {
    type: 'lt',
    left,
    right,
  };
}

export function and<Table extends TableLike>(
  left: Expr<Table, boolean>,
  right: Expr<Table, boolean>
): Expr<Table, boolean> {
  return {
    type: 'and',
    left,
    right,
  };
}
