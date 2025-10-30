import type { ColumnExpr, RowExpr, TableSchema } from './table';
import type { RowType, UntypedTableDef } from './table';

type ColumnNames<TableDef extends UntypedTableDef> =
  keyof RowType<TableDef> & string;

type ColumnExprForValue<
  TableDef extends UntypedTableDef,
  Value,
> = {
  [C in ColumnNames<TableDef>]: RowType<TableDef>[C] extends Value
    ? ColumnExpr<TableDef, C>
    : never;
}[ColumnNames<TableDef>];

type LiteralExpr<Value> = {
  type: 'literal';
  value: Value;
};

export type ValueExpr<
  TableDef extends UntypedTableDef,
  Value,
> = ColumnExprForValue<TableDef, Value> | LiteralExpr<Value>;

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
      type: 'and';
      left: BooleanExpr<TableDef>;
      right: BooleanExpr<TableDef>;
    };

export type Expr<
  TableDef extends UntypedTableDef,
  Value,
> = Value extends boolean
  ? BooleanExpr<TableDef>
  : ValueExpr<TableDef, Value>;

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
    predicate: (row: RowExpr<TableDef>) => Expr<TableDef, boolean>
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
    Record<
      ColumnNames<TableDef>,
      ColumnExpr<TableDef, ColumnNames<TableDef>>
    >
  > = Object.create(null);
  for (const columnName of Object.keys(tableDef.columns) as ColumnNames<TableDef>[]) {
    row[columnName] = {
      type: 'column',
      column: columnName,
      table: tableDef.name,
      valueType: undefined as unknown as RowType<TableDef>[typeof columnName],
    } as ColumnExpr<TableDef, typeof columnName>;
  }
  return row as RowExpr<TableDef>;
}

class QueryBuilder<TableDef extends UntypedTableDef>
  implements Query<TableDef>
{
  #tableDef: TableDef;
  #filters: readonly Expr<TableDef, boolean>[];

  constructor(
    tableDef: TableDef,
    filters: readonly Expr<TableDef, boolean>[] = []
  ) {
    this.#tableDef = tableDef;
    this.#filters = filters;
  }

  filter(
    predicate: (row: RowExpr<TableDef>) => Expr<TableDef, boolean>
  ): Query<TableDef> {
    const rowExpr = createRowExpr(this.#tableDef);
    const condition = predicate(rowExpr);
    return new QueryBuilder(
      this.#tableDef,
      this.#filters.concat(condition)
    );
  }

  toSql(): string {
    const tableIdent = this.#quoteIdentifier(this.#tableDef.name);
    let sql = `SELECT ${tableIdent}.* FROM ${tableIdent}`;
    if (this.#filters.length > 0) {
      const where = this.#filters
        .map(condition => this.#conditionToSql(condition, tableIdent))
        .join(' AND ');
      sql += ` WHERE ${where}`;
    }
    return sql;
  }

  #conditionToSql(
    condition: Expr<TableDef, boolean>,
    tableIdent: string
  ): string {
    switch (condition.type) {
      case 'eq': {
        const left = condition.left;
        const right = condition.right;
        const leftIsNull = this.#isNullLiteral(left);
        const rightIsNull = this.#isNullLiteral(right);
        if (leftIsNull && rightIsNull) {
          return 'NULL IS NULL';
        }
        if (leftIsNull) {
          return `${this.#valueExprToSql(right, tableIdent)} IS NULL`;
        }
        if (rightIsNull) {
          return `${this.#valueExprToSql(left, tableIdent)} IS NULL`;
        }
        const leftSql = this.#valueExprToSql(left, tableIdent);
        const rightSql = this.#valueExprToSql(right, tableIdent);
        return `${leftSql} = ${rightSql}`;
      }
      case 'gt': {
        const leftSql = this.#valueExprToSql(condition.left, tableIdent);
        const rightSql = this.#valueExprToSql(condition.right, tableIdent);
        return `${leftSql} > ${rightSql}`;
      }
      case 'lt': {
        const leftSql = this.#valueExprToSql(condition.left, tableIdent);
        const rightSql = this.#valueExprToSql(condition.right, tableIdent);
        return `${leftSql} < ${rightSql}`;
      }
      case 'and': {
        const left = this.#conditionToSql(condition.left, tableIdent);
        const right = this.#conditionToSql(condition.right, tableIdent);
        return `(${left}) AND (${right})`;
      }
      default: {
        return assertNever(condition as never);
      }
    }
  }

  #valueToSql(value: unknown): string {
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
      return `'${this.#escapeString(value)}'`;
    }
    if (value instanceof Date) {
      return `'${value.toISOString()}'`;
    }
    if (
      value != null &&
      typeof (value as { toString: () => string }).toString === 'function' &&
      (value as { toString: () => string }).toString !== Object.prototype.toString
    ) {
      return `'${this.#escapeString(value.toString())}'`;
    }
    throw new TypeError(
      `Unsupported value for SQL serialization: ${String(value)}`
    );
  }

  #quoteIdentifier(identifier: string): string {
    return `"${identifier.replace(/"/g, '""')}"`;
  }

  #escapeString(value: string): string {
    return value.replace(/'/g, "''");
  }

  #valueExprToSql(
    expr: ValueExpr<TableDef, any>,
    tableIdent: string
  ): string {
    if ('type' in expr && expr.type === 'literal') {
      return this.#valueToSql(expr.value);
    }
    const column = expr as ColumnExpr<
      TableDef,
      ColumnNames<TableDef>
    >;
    return `${tableIdent}.${this.#quoteIdentifier(column.column)}`;
  }

  #isNullLiteral(expr: ValueExpr<TableDef, any>): boolean {
    return (
      'type' in expr &&
      expr.type === 'literal' &&
      (expr.value === null || expr.value === undefined)
    );
  }
}

export function createQuery<TableDef extends UntypedTableDef>(
  tableDef: TableDef
): Query<TableDef>;
export function createQuery<
  TSchema extends TableSchema<any, any, readonly any[]>,
>(
  tableSchema: TSchema
): Query<TableSchemaAsTableDef<TSchema>>;
export function createQuery(
  tableDefOrSchema:
    | UntypedTableDef
    | TableSchema<any, Record<string, any>, readonly any[]>
): Query<UntypedTableDef> {
  if ('rowType' in tableDefOrSchema) {
    const tableSchema = tableDefOrSchema;
    return new QueryBuilder<TableSchemaAsTableDef<typeof tableSchema>>({
      name: tableSchema.tableName,
      columns: tableSchema.rowType.row,
      indexes: Array.from(tableSchema.idxs) as TableSchemaAsTableDef<
        typeof tableSchema
      >['indexes'],
    } as TableSchemaAsTableDef<typeof tableSchema>);
  }
  return new QueryBuilder(tableDefOrSchema);
}

export function literal<Value>(value: Value): LiteralExpr<Value> {
  return { type: 'literal', value };
}

export function eq<TableDef extends UntypedTableDef, Value>(
  left: ValueExpr<TableDef, Value>,
  right: ValueExpr<TableDef, Value>
): Expr<TableDef, boolean> {
  return {
    type: 'eq',
    left,
    right,
  };
}

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
