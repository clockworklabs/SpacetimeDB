import type {
  ColumnExpr,
  IndexExpr,
  IndexExprs,
  IndexValueType,
  RowExpr,
  TableIndexNames,
  TableSchema,
  RowType,
  UntypedTableDef,
} from './table';
import type { IndexOpts } from './indexes';
import type { Identity } from '../lib/identity';

type ColumnNames<TableDef extends UntypedTableDef> = keyof RowType<TableDef> &
  string;

export type TableRef<TableDef extends UntypedTableDef> = {
  tableDef: TableDef;
  tableName: TableDef['name'];
  row: RowExpr<TableDef>;
  indexes: IndexExprs<TableDef>;
};

export type Semijoin<
  LeftTable extends UntypedTableDef,
  RightTable extends UntypedTableDef,
  LeftIndex extends TableIndexNames<LeftTable>,
  RightIndex extends TableIndexNames<RightTable>,
> = Readonly<{
  left: TableScan<LeftTable>;
  leftIndex: IndexExpr<LeftTable, LeftIndex>;
  rightIndex: IndexExpr<RightTable, RightIndex>;
  toSql: () => string;
}>;

  type SameIndexValueType<
  T1 extends UntypedTableDef,
  I1 extends TableIndexNames<T1>,
  T2 extends UntypedTableDef,
  I2 extends TableIndexNames<T2>,
> = IndexValueType<T1, I1> extends IndexValueType<T2, I2>
  ? IndexValueType<T2, I2> extends IndexValueType<T1, I1>
    ? true
    : false
  : false;

export type ToSql = {
  /**
   * Converts the query to its SQL representation.
   * @returns The SQL string representing the query.
   */
  toSql(): string;
};

type ColumnExprForValue<TableDef extends UntypedTableDef, Value> = {
  [C in ColumnNames<TableDef>]: RowType<TableDef>[C] extends Value
    ? ColumnExpr<TableDef, C>
    : never;
}[ColumnNames<TableDef>];

type LiteralExpr<Value> = {
  type: 'literal';
  value: Value;
};

export type ValueExpr<TableDef extends UntypedTableDef, Value> =
  | ColumnExprForValue<TableDef, Value>
  | LiteralExpr<Value>;

type ExprValueType<
  TableDef extends UntypedTableDef,
  ExprT,
> = ExprT extends ColumnExpr<TableDef, infer Column extends ColumnNames<TableDef>>
  ? RowType<TableDef>[Column]
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

type ComparableValueExprs<
  TableDef extends UntypedTableDef,
  LeftExpr,
  RightExpr,
> = WidenComparableLiteral<ExprValueType<TableDef, LeftExpr>> extends
  | null
  | undefined
  ? true
  : WidenComparableLiteral<ExprValueType<TableDef, RightExpr>> extends
        | null
        | undefined
    ? true
    : [
          WidenComparableLiteral<ExprValueType<TableDef, LeftExpr>> &
            WidenComparableLiteral<ExprValueType<TableDef, RightExpr>>,
        ] extends [never]
      ? false
      : true;

type BooleanExpr<TableDef extends UntypedTableDef> =
  | {
      type: 'eq';
      left: ValueExpr<TableDef, any>;
      right: ValueExpr<TableDef, any>;
    }
  | {
      type: 'gt';
      left: ValueExpr<TableDef, any>;
      right: ValueExpr<TableDef, any>;
    }
  | {
      type: 'lt';
      left: ValueExpr<TableDef, any>;
      right: ValueExpr<TableDef, any>;
    }
  | {
    type: 'not',
    val: BooleanExpr<TableDef>;
  }
  | {
      type: 'and';
      left: BooleanExpr<TableDef>;
      right: BooleanExpr<TableDef>;
    };

export type Expr<
  TableDef extends UntypedTableDef,
  Value,
> = Value extends boolean ? BooleanExpr<TableDef> : ValueExpr<TableDef, Value>;

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

function isNullLiteral<TableDef extends UntypedTableDef>(
  expr: ValueExpr<TableDef, any>
): boolean {
  return (
    expr.type === 'literal' &&
    (expr.value === null || expr.value === undefined)
  );
}

function valueExprToSql<TableDef extends UntypedTableDef>(
  expr: ValueExpr<TableDef, any>,
  options: RenderOptions = {}
): string {
  if (expr.type === 'literal') {
    return valueToSql(expr.value);
  }
  const alias = options.tableAlias ?? expr.table;
  return `${quoteIdentifier(alias)}.${quoteIdentifier(expr.column)}`;
}

function booleanExprToSql<TableDef extends UntypedTableDef>(
  expr: BooleanExpr<TableDef>,
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

function renderFilters<TableDef extends UntypedTableDef>(
  filters: readonly BooleanExpr<TableDef>[],
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

function isBooleanExpr<TableDef extends UntypedTableDef>(
  expr: Expr<TableDef, any>
): expr is BooleanExpr<TableDef> {
  return (
    expr.type === 'eq' ||
    expr.type === 'gt' ||
    expr.type === 'lt' ||
    expr.type === 'and'
  );
}

export function exprToSql<TableDef extends UntypedTableDef, Value>(
  expr: Expr<TableDef, Value>,
  options: RenderOptions = {}
): string {
  if (isBooleanExpr(expr)) {
    return booleanExprToSql(expr, options);
  }
  return valueExprToSql(expr as ValueExpr<TableDef, Value>, options);
}

type TableSchemaAsTableDef<
  TSchema extends TableSchema<any, any, readonly any[]>,
> = {
  name: TSchema['tableName'];
  columns: TSchema['rowType']['row'];
  indexes: Array<TSchema['idxs'][number]>;
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

export type Query<TableDef extends UntypedTableDef> = {
  /**
   * Adds a filter expression to the query.
   * @param predicate A function that produces a condition when given a typed
   *                  projection of the table row.
   */
  filter(
    predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
  ): Query<TableDef>;

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
    row[columnName] = {
      type: 'column',
      column: columnName,
      table: tableDef.name,
      valueType: undefined as unknown as RowType<TableDef>[typeof columnName],
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
    const accessorName = indexOpt.name ??
      `${tableDef.name}_${columnNamesExplicit.join('_')}_idx_${algorithm}`;
    const normalizedColumns =
      columnNamesExplicit as readonly ColumnNames<TableDef>[];
    const singleColumn =
      normalizedColumns.length === 1 ? normalizedColumns[0]! : undefined;
    const entry = {
      type: 'index',
      tableDef,
      table: tableDef.name,
      index: accessorName as TableIndexNames<TableDef>,
      name: accessorName,
      unique,
      columns: columnNamesExplicit,
      algorithm,
      valueType:
        singleColumn !== undefined
          ? rowExpr[singleColumn].valueType
          : normalizedColumns.map(column => rowExpr[column].valueType),
    } as IndexExpr<TableDef, TableIndexNames<TableDef>>;
    putIndex(accessorName, entry);
  }

  return indexes as IndexExprs<TableDef>;
}

function normalizeSchemaIndexes<
  TSchema extends TableSchema<any, any, readonly any[]>,
>(
  tableSchema: TSchema
): IndexOpts<
  keyof TableSchemaAsTableDef<TSchema>['columns'] & string
>[] {
  const columnElements = tableSchema
    .rowType
    .resolveType()
    .value.elements;
  const columnNamesById = columnElements.map(element => element.name);
  const normalized: IndexOpts<
    keyof TableSchemaAsTableDef<TSchema>['columns'] & string
  >[] = [];

  for (const rawIndex of tableSchema.tableDef.indexes ?? []) {
    const accessorName = rawIndex.accessorName ?? rawIndex.name ?? undefined;
    switch (rawIndex.algorithm.tag) {
      case 'Direct': {
        const columnName = columnNamesById[rawIndex.algorithm.value];
        if (columnName === undefined) continue;
        normalized.push({
          name: accessorName,
          algorithm: 'direct',
          column: columnName as keyof TableSchemaAsTableDef<TSchema>['columns'] & string,
        });
        break;
      }
      case 'BTree': {
        const columnIds = rawIndex.algorithm.value;
        const names = columnIds
          .map(id => columnNamesById[id])
          .filter((name): name is string => name !== undefined);
        if (names.length !== columnIds.length) continue;
        normalized.push({
          name: accessorName,
          algorithm: 'btree',
          columns: names as readonly (keyof TableSchemaAsTableDef<TSchema>['columns'] & string)[],
        });
        break;
      }
      case 'Hash': {
        // Hash indexes are not yet supported by the query builder.
        break;
      }
    }
  }

  return normalized;
}

function createTableRefFromDef<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  originalIndexes?: readonly IndexOpts<any>[]
): TableRef<TableDef> {
  const row = createRowExpr(tableDef);
  return {
    tableDef,
    tableName: tableDef.name,
    row,
    indexes: createIndexExprs(tableDef, row, originalIndexes),
  };
}

export function createTableRef<TableDef extends UntypedTableDef>(
  tableDef: TableDef
): TableRef<TableDef>;
export function createTableRef<
  TSchema extends TableSchema<any, any, readonly any[]>,
>(
  tableSchema: TSchema
): TableRef<TableSchemaAsTableDef<TSchema>>;
export function createTableRef(
  tableDefOrSchema:
    | UntypedTableDef
    | TableSchema<any, Record<string, any>, readonly any[]>
): TableRef<any> {
  if ('rowType' in tableDefOrSchema) {
    const tableSchema = tableDefOrSchema;
    const normalizedIndexes = normalizeSchemaIndexes(tableSchema);
    const tableDef = {
      name: tableSchema.tableName,
      columns: tableSchema.rowType.row,
      indexes: normalizedIndexes as TableSchemaAsTableDef<
        typeof tableSchema
      >['indexes'],
    } as TableSchemaAsTableDef<typeof tableSchema>;
    return createTableRefFromDef(tableDef, normalizedIndexes);
  }
  return createTableRefFromDef(tableDefOrSchema);
}

export class TableScan<TableDef extends UntypedTableDef> {
  readonly table: TableRef<TableDef>;
  readonly filters: readonly BooleanExpr<TableDef>[];

  constructor(
    table: TableRef<TableDef>,
    filters: readonly BooleanExpr<TableDef>[] = []
  ) {
    this.table = table;
    this.filters = filters;
  }

  addFilter(
    predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
  ): TableScan<TableDef> {
    const filterExpr = predicate(this.table.row);
    return new TableScan(
      this.table,
      [...this.filters, filterExpr] as readonly BooleanExpr<TableDef>[]
    );
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
    LeftIndex extends TableIndexNames<TableDef>,
    RightTable extends UntypedTableDef,
    RightIndex extends TableIndexNames<RightTable>,
  >(
    leftIndex: IndexExpr<TableDef, LeftIndex>,
    rightIndex: SameIndexValueType<TableDef, LeftIndex, RightTable, RightIndex> extends true
      ? IndexExpr<RightTable, RightIndex>
      : never
): Semijoin<TableDef, RightTable, LeftIndex, RightIndex> {
    if (leftIndex.table !== this.table.tableName) {
      throw new Error('Left index must belong to the same table as the scan');
    }
    if (rightIndex.table === this.table.tableName) {
      throw new Error('Right index must belong to a different table');
    }
    if (leftIndex.columns.length !== rightIndex.columns.length) {
      throw new Error('Indexes must have the same number of columns for semijoin');
    }
    const leftScan = this;
    const normalizedRightIndex = rightIndex as IndexExpr<RightTable, RightIndex>;
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
    } as Semijoin<TableDef, RightTable, LeftIndex, RightIndex>;
  }
}

export function createTableScan<TableDef extends UntypedTableDef>(
  tableDef: TableDef
): TableScan<TableDef> {
  return new TableScan(createTableRef(tableDef));
}

class QueryBuilder<TableDef extends UntypedTableDef>
  implements Query<TableDef>
{
  #scan: TableScan<TableDef>;

  constructor(scan: TableScan<TableDef>) {
    this.#scan = scan;
  }

  filter(
    predicate: (row: RowExpr<TableDef>) => BooleanExpr<TableDef>
  ): Query<TableDef> {
    const nextScan = this.#scan.addFilter(predicate);
    return new QueryBuilder(nextScan);
  }

  toSql(): string {
    return this.#scan.toSql();
  }
}

export function createQuery<TableDef extends UntypedTableDef>(
  tableDef: TableDef
): Query<TableDef>;

export function createQuery<
  TSchema extends TableSchema<any, any, readonly any[]>,
>(tableSchema: TSchema): Query<TableSchemaAsTableDef<TSchema>>;
export function createQuery(
  tableDefOrSchema:
    | UntypedTableDef
    | TableSchema<any, Record<string, any>, readonly any[]>
): Query<UntypedTableDef> {
  if ('rowType' in tableDefOrSchema) {
    const tableSchema = tableDefOrSchema;
    const tableDef = {
      name: tableSchema.tableName,
      columns: tableSchema.rowType.row,
      indexes: tableSchema.idxs as TableSchemaAsTableDef<
        typeof tableSchema
      >['indexes'],
    } as TableSchemaAsTableDef<typeof tableSchema>;
    return new QueryBuilder(createTableScan(tableDef));
  }
  return new QueryBuilder(createTableScan(tableDefOrSchema));
}

export function literal<Value>(value: Value): LiteralExpr<Value> {
  return { type: 'literal', value };
}

/*
export function eq<
  TableDef extends UntypedTableDef,
  LeftExpr extends ValueExpr<TableDef, any>,
  RightExpr extends ValueExpr<TableDef, any>,
>(
  left: LeftExpr,
  right: RightExpr
): ComparableValueExprs<TableDef, LeftExpr, RightExpr> extends true
  ? Expr<TableDef, boolean>
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
export function eq<TableDef extends UntypedTableDef, Value>(
  left: ValueExpr<TableDef, Value & ValueTypesThatAreComparable>,
  right: ValueExpr<TableDef, Value & ValueTypesThatAreComparable>
): Expr<TableDef, boolean> {
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
export function eq<TableDef extends UntypedTableDef>(
  left: ValueExpr<TableDef, any>,
  right: ValueExpr<TableDef, any>
): Expr<TableDef, boolean> {
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

export function gt<TableDef extends UntypedTableDef, Value>(
  left: ValueExpr<TableDef, Value>,
  right: ValueExpr<TableDef, Value>
): Expr<TableDef, boolean> {
  return {
    type: 'gt',
    left,
    right,
  };
}

export function lt<TableDef extends UntypedTableDef, Value>(
  left: ValueExpr<TableDef, Value>,
  right: ValueExpr<TableDef, Value>
): Expr<TableDef, boolean> {
  return {
    type: 'lt',
    left,
    right,
  };
}

export function and<TableDef extends UntypedTableDef>(
  left: Expr<TableDef, boolean>,
  right: Expr<TableDef, boolean>
): Expr<TableDef, boolean> {
  return {
    type: 'and',
    left,
    right,
  };
}
