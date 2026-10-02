import { ConnectionId } from './connection_id';
import { Identity } from './identity';
import type { ColumnIndex, IndexColumns, IndexOpts } from './indexes';
import type { UntypedSchemaDecl } from './schema';
import type { UntypedTableDecl } from './table';
import type { UntypedTableBody } from './table_body';
import { Timestamp } from './timestamp';
import type {
  ColumnBuilder,
  ColumnMetadata,
  RowBuilder,
  TypeBuilder,
} from './type_builders';
import type { Values } from './type_util';
import type { Bool as SatsBool } from './algebraic_type_variants';
import { Uuid } from './uuid';

/**
 * Helper to get the set of table names.
 */
export type TableNames<SchemaDecl extends UntypedSchemaDecl> = Values<
  SchemaDecl['tables']
>['accessorName'] &
  string;

/** helper: pick the table def object from the schema by its name */
export type TableDefByName<
  SchemaDecl extends UntypedSchemaDecl,
  Name extends TableNames<SchemaDecl>,
> = Extract<Values<SchemaDecl['tables']>, { accessorName: Name }>;

// internal only — NOT exported.
// This is how we make sure queries are only created with our helpers.
const QueryBrand = Symbol('QueryBrand');

export interface TableTypedQuery<TableDecl extends TypedTableDecl> {
  readonly [QueryBrand]: true;
  readonly __table?: TableDecl;
}

export interface RowTypedQuery<Row, ST> {
  readonly [QueryBrand]: true;
  // Phantom type to track the row type.
  readonly __row?: Row;
  readonly __algebraicType?: ST;
}

export type Query<TableDecl extends TypedTableDecl> = RowTypedQuery<
  RowType<TableDecl>,
  TableDecl['rowType']
>;

export const isRowTypedQuery = (val: unknown): val is RowTypedQuery<any, any> =>
  !!val && typeof val === 'object' && QueryBrand in (val as object);

export const isTypedQuery = (val: unknown): val is TableTypedQuery<any> =>
  !!val && typeof val === 'object' && QueryBrand in (val as object);

export function toSql(q: Query<any>): string {
  return (q as unknown as { toSql(): string }).toSql();
}

// A query builder with a single table.
type From<TableDecl extends TypedTableDecl> = RowTypedQuery<
  RowType<TableDecl>,
  TableDecl['rowType']
> &
  Readonly<{
    toSql(): string;
    where(
      predicate: (row: RowExpr<TableDecl>) => PredicateExpr<TableDecl>
    ): From<TableDecl>;
    rightSemijoin<RightTable extends TypedTableDecl>(
      other: TableRef<RightTable>,
      on: (
        left: IndexedRowExpr<TableDecl>,
        right: IndexedRowExpr<RightTable>
      ) => BooleanExpr<TableDecl | RightTable>
    ): SemijoinBuilder<RightTable>;
    leftSemijoin<RightTable extends TypedTableDecl>(
      other: TableRef<RightTable>,
      on: (
        left: IndexedRowExpr<TableDecl>,
        right: IndexedRowExpr<RightTable>
      ) => BooleanExpr<TableDecl | RightTable>
    ): SemijoinBuilder<TableDecl>;
    /** @deprecated No longer needed — builder is already a valid query. */
    build(): Query<TableDecl>;
  }>;

// A query builder with a semijoin.
type SemijoinBuilder<TableDecl extends TypedTableDecl> = RowTypedQuery<
  RowType<TableDecl>,
  TableDecl['rowType']
> &
  Readonly<{
    toSql(): string;
    where(
      predicate: (row: RowExpr<TableDecl>) => PredicateExpr<TableDecl>
    ): SemijoinBuilder<TableDecl>;
    /** @deprecated No longer needed — builder is already a valid query. */
    build(): Query<TableDecl>;
  }>;

class SemijoinImpl<TableDecl extends TypedTableDecl>
  implements SemijoinBuilder<TableDecl>, TableTypedQuery<TableDecl>
{
  readonly [QueryBrand] = true;
  readonly type = 'semijoin' as const;
  constructor(
    readonly sourceQuery: FromBuilder<TableDecl>,
    readonly filterQuery: FromBuilder<any>,
    readonly joinCondition: BooleanExpr<any>
  ) {
    if (sourceQuery.table.sourceName === filterQuery.table.sourceName) {
      // TODO: Handle aliasing properly instead of just forbidding it.
      throw new Error('Cannot semijoin a table to itself');
    }
  }

  build(): Query<TableDecl> {
    return this as Query<TableDecl>;
  }

  where(
    predicate: (row: RowExpr<TableDecl>) => PredicateExpr<TableDecl>
  ): SemijoinImpl<TableDecl> {
    const nextSourceQuery = this.sourceQuery.where(predicate);
    return new SemijoinImpl<TableDecl>(
      nextSourceQuery,
      this.filterQuery,
      this.joinCondition
    );
  }

  toSql(): string {
    const left = this.filterQuery;
    const right = this.sourceQuery;
    const leftTable = quoteIdentifier(left.table.sourceName);
    const rightTable = quoteIdentifier(right.table.sourceName);
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

class FromBuilder<TableDecl extends TypedTableDecl>
  implements From<TableDecl>, TableTypedQuery<TableDecl>
{
  readonly [QueryBrand] = true;
  constructor(
    readonly table: TableRef<TableDecl>,
    readonly whereClause?: BooleanExpr<TableDecl>
  ) {}

  where(
    predicate: (row: RowExpr<TableDecl>) => PredicateExpr<TableDecl>
  ): FromBuilder<TableDecl> {
    const newCondition = normalizePredicateExpr(predicate(this.table.cols));
    const nextWhere = this.whereClause
      ? this.whereClause.and(newCondition)
      : newCondition;
    return new FromBuilder<TableDecl>(this.table, nextWhere);
  }

  rightSemijoin<OtherTable extends TypedTableDecl>(
    right: TableRef<OtherTable>,
    on: (
      left: IndexedRowExpr<TableDecl>,
      right: IndexedRowExpr<OtherTable>
    ) => BooleanExpr<TableDecl | OtherTable>
  ): SemijoinBuilder<OtherTable> {
    const sourceQuery = new FromBuilder(right);
    const joinCondition = on(
      this.table.indexedCols,
      right.indexedCols
    ) as BooleanExpr<any>;
    return new SemijoinImpl<OtherTable>(sourceQuery, this, joinCondition);
  }

  leftSemijoin<OtherTable extends TypedTableDecl>(
    right: TableRef<OtherTable>,
    on: (
      left: IndexedRowExpr<TableDecl>,
      right: IndexedRowExpr<OtherTable>
    ) => BooleanExpr<TableDecl | OtherTable>
  ): SemijoinBuilder<TableDecl> {
    const filterQuery = new FromBuilder(right);
    const joinCondition = on(
      this.table.indexedCols,
      right.indexedCols
    ) as BooleanExpr<any>;
    return new SemijoinImpl<TableDecl>(this, filterQuery, joinCondition);
  }

  toSql(): string {
    return renderSelectSqlWithJoins(this.table, this.whereClause);
  }

  build(): Query<TableDecl> {
    return this as Query<TableDecl>;
  }
}

export type QueryBuilder<SchemaDecl extends UntypedSchemaDecl> = {
  readonly [Tbl in Values<
    SchemaDecl['tables']
  > as Tbl['accessorName']]: TableRef<Tbl> & From<Tbl>;
} & {};

/**
 * Like `QueryBuilder`, but with declared namespaces also exposed as sub-objects.
 * This is the type of the `tables` argument in a `subscribe` query-builder callback.
 *
 * Root-level tables appear as direct properties (same as `QueryBuilder`).
 * Declared namespaces appear as sub-objects — each is itself a `QueryBuilder` for that
 * namespace's schema, so `tables.<namespace>.<table>` is fully typed.
 *
 * When `SchemaDecl['namespaces']` is absent or `{}`, no namespace properties appear —
 * accessing an undeclared namespace is a compile error.
 */
export type NamespacedQueryBuilder<SchemaDecl extends UntypedSchemaDecl> =
  QueryBuilder<SchemaDecl> & {
    readonly [NS in keyof NonNullable<SchemaDecl['namespaces']>]: NonNullable<
      SchemaDecl['namespaces']
    >[NS] extends UntypedSchemaDecl
      ? QueryBuilder<NonNullable<SchemaDecl['namespaces']>[NS]>
      : never;
  };

/**
 * A runtime reference to a table. This materializes the RowExpr for us.
 * TODO: Maybe add the full SchemaDecl to the type signature depending on how joins will work.
 */
export type TableRef<TableDecl extends TypedTableDecl> = Readonly<{
  type: 'table';
  sourceName: TableDecl['sourceName'];
  accessorName: string;
  cols: RowExpr<TableDecl>;
  indexedCols: IndexedRowExpr<TableDecl>;
  tableDef: TableDecl;
  // Delegated UntypedTableDecl properties for compatibility.
  columns: TableDecl['columns'];
  indexes: TableDecl['indexes'];
  rowType: TableDecl['rowType'];
  constraints: any;
}>;

class TableRefImpl<TableDecl extends TypedTableDecl>
  implements TableRef<TableDecl>, From<TableDecl>
{
  readonly [QueryBrand] = true;
  readonly type = 'table' as const;
  sourceName: string;
  accessorName: string;
  cols: RowExpr<TableDecl>;
  indexedCols: IndexedRowExpr<TableDecl>;
  tableDef: TableDecl;
  // Delegate UntypedTableDecl properties from tableDef so this can be used as a table declaration.
  get columns() {
    return this.tableDef.columns;
  }
  get indexes() {
    return this.tableDef.indexes;
  }
  get rowType() {
    return this.tableDef.rowType;
  }
  get constraints() {
    return (this.tableDef as any).constraints;
  }
  constructor(tableDef: TableDecl) {
    this.sourceName = tableDef.sourceName;
    this.accessorName = tableDef.accessorName;
    this.cols = createRowExpr(tableDef);
    // this.indexedCols = createIndexedRowExpr(tableDef, this.cols);
    // TODO: we could create an indexedRowExpr to avoid having the extra columns.
    // Right now, the objects we pass will actually have all the columns, but the
    // type system will consider it an error.
    this.indexedCols = this.cols;
    this.tableDef = tableDef;
    Object.freeze(this);
  }

  asFrom(): FromBuilder<TableDecl> {
    return new FromBuilder<TableDecl>(this);
  }

  rightSemijoin<RightTable extends TypedTableDecl>(
    other: TableRef<RightTable>,
    on: (
      left: IndexedRowExpr<TableDecl>,
      right: IndexedRowExpr<RightTable>
    ) => EqExpr<TableDecl | RightTable>
  ): SemijoinBuilder<RightTable> {
    return this.asFrom().rightSemijoin(other, on);
  }

  leftSemijoin<RightTable extends TypedTableDecl>(
    other: TableRef<RightTable>,
    on: (
      left: IndexedRowExpr<TableDecl>,
      right: IndexedRowExpr<RightTable>
    ) => EqExpr<TableDecl | RightTable>
  ): SemijoinBuilder<TableDecl> {
    return this.asFrom().leftSemijoin(other, on);
  }

  build(): Query<TableDecl> {
    return this.asFrom().build();
  }

  toSql(): string {
    return this.asFrom().toSql();
  }

  where(
    predicate: (row: RowExpr<TableDecl>) => PredicateExpr<TableDecl>
  ): FromBuilder<TableDecl> {
    return this.asFrom().where(predicate);
  }
}

export function createTableRefFromDef<TableDecl extends TypedTableDecl>(
  tableDef: TableDecl
): TableRef<TableDecl> {
  return new TableRefImpl<TableDecl>(tableDef);
}

export function makeQueryBuilder<SchemaDecl extends UntypedSchemaDecl>(
  schema: SchemaDecl
): QueryBuilder<SchemaDecl> {
  const qb = Object.create(null) as QueryBuilder<SchemaDecl>;
  for (const table of Object.values(schema.tables)) {
    const ref = createTableRefFromDef(
      table as TableDefByName<SchemaDecl, TableNames<SchemaDecl>>
    );
    (qb as Record<string, TableRef<any>>)[table.accessorName] = ref;
  }
  return Object.freeze(qb) as QueryBuilder<SchemaDecl>;
}

/**
 * Builds the namespace-aware `tables` object passed to `subscribe`'s query-builder callback.
 *
 * Tables whose `sourceName` contains no `.` are placed at the root.
 * Tables with a dotted `sourceName` (e.g. `"namespace.table"`) are grouped under a
 * sub-object keyed by the namespace alias, with the part after the dot as the
 * property key within that namespace.
 */
export function makeFromBuilder<SchemaDecl extends UntypedSchemaDecl>(
  tables: SchemaDecl['tables']
): NamespacedQueryBuilder<SchemaDecl> {
  const result: Record<string, unknown> = Object.create(null);
  const namespaces: Record<string, Record<string, unknown>> = Object.create(
    null
  );

  for (const table of Object.values(tables) as UntypedTableDecl[]) {
    const dotIdx = table.sourceName.indexOf('.');
    if (dotIdx === -1) {
      result[table.accessorName] = createTableRefFromDef(table as any);
    } else {
      const ns = table.sourceName.slice(0, dotIdx);
      const key = table.sourceName.slice(dotIdx + 1);
      (namespaces[ns] ??= Object.create(null))[key] = createTableRefFromDef(
        table as any
      );
    }
  }

  for (const [ns, nsTables] of Object.entries(namespaces)) {
    result[ns] = Object.freeze(nsTables);
  }

  return Object.freeze(result) as unknown as NamespacedQueryBuilder<SchemaDecl>;
}

function createRowExpr<TableDecl extends TypedTableDecl>(
  tableDef: TableDecl
): RowExpr<TableDecl> {
  const row: Record<string, ColumnExpr<TableDecl, any>> = {};
  for (const columnName of Object.keys(tableDef.columns) as Array<
    keyof TableDecl['columns'] & string
  >) {
    const columnBuilder = tableDef.columns[columnName];
    const column = new ColumnExpression<TableDecl, typeof columnName>(
      tableDef.sourceName,
      columnName,
      columnBuilder.typeBuilder.algebraicType as InferSpacetimeTypeOfColumn<
        TableDecl,
        typeof columnName
      >,
      columnBuilder.columnMetadata.name
    );
    row[columnName] = Object.freeze(column);
  }
  return Object.freeze(row) as RowExpr<TableDecl>;
}

function renderSelectSqlWithJoins<Table extends TypedTableDecl>(
  table: TableRef<Table>,
  where?: BooleanExpr<Table>,
  extraClauses: readonly string[] = []
): string {
  const quotedTable = quoteIdentifier(table.sourceName);
  const sql = `SELECT * FROM ${quotedTable}`;
  const clauses: string[] = [];
  if (where) clauses.push(booleanExprToSql(where));
  clauses.push(...extraClauses);
  if (clauses.length === 0) return sql;
  const whereSql =
    clauses.length === 1 ? clauses[0] : clauses.map(wrapInParens).join(' AND ');
  return `${sql} WHERE ${whereSql}`;
}

// TODO: Just use UntypedTableDecl if they end up being the same.
export type TypedTableDecl<
  Columns extends Record<
    string,
    ColumnBuilder<any, any, ColumnMetadata<any>>
  > = Record<string, ColumnBuilder<any, any, ColumnMetadata<any>>>,
> = {
  sourceName: string;
  accessorName: string;
  columns: Columns;
  indexes: readonly IndexOpts<any>[];
  rowType: RowBuilder<Columns>['algebraicType']['value'];
};

/** @deprecated Use `TypedTableDecl` instead. */
export type TypedTableDef<
  Columns extends Record<
    string,
    ColumnBuilder<any, any, ColumnMetadata<any>>
  > = Record<string, ColumnBuilder<any, any, ColumnMetadata<any>>>,
> = TypedTableDecl<Columns>;

/** @deprecated This type is not used by the SDK. */
export type TableSchemaAsTableDef<TSchema extends UntypedTableBody> = {
  name: TSchema['tableName'];
  columns: TSchema['rowType']['row'];
  indexes: TSchema['idxs'];
};

type RowType<TableDecl extends TypedTableDecl> = {
  [K in keyof TableDecl['columns']]: TableDecl['columns'][K] extends ColumnBuilder<
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
  TableDecl extends TypedTableDecl,
  ColumnName extends ColumnNames<TableDecl>,
> = ColumnExpression<TableDecl, ColumnName>;

type ColumnSpacetimeType<Col extends ColumnExpr<any, any>> =
  Col extends ColumnExpr<infer T, infer N>
    ? InferSpacetimeTypeOfColumn<T, N>
    : never;

// TODO: This checks that they match, but we also need to make sure that they are comparable types,
// since you can use product types at all.
type ColumnSameSpacetime<
  ThisTable extends TypedTableDecl,
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
  TableDecl extends TypedTableDecl,
  ColumnName extends ColumnNames<TableDecl>,
> {
  readonly type = 'column' as const;
  // This is the column accessor
  readonly column: ColumnName;
  // The name of the column in the database.
  readonly columnName: string;
  readonly table: TableDecl['sourceName'];
  // phantom: actual runtime value is undefined
  readonly tsValueType?: RowType<TableDecl>[ColumnName];
  readonly spacetimeType: InferSpacetimeTypeOfColumn<TableDecl, ColumnName>;

  constructor(
    table: TableDecl['sourceName'],
    column: ColumnName,
    spacetimeType: InferSpacetimeTypeOfColumn<TableDecl, ColumnName>,
    columnName?: string
  ) {
    this.table = table;
    this.column = column;
    this.columnName = columnName || column;
    this.spacetimeType = spacetimeType;
  }

  eq(
    literal: LiteralValue & RowType<TableDecl>[ColumnName]
  ): BooleanExpr<TableDecl>;
  eq<OtherCol extends ColumnExpr<any, any>>(
    value: ColumnSameSpacetime<TableDecl, ColumnName, OtherCol>
  ): BooleanExpr<TableDecl | ExtractTable<OtherCol>>;

  eq(x: any): any {
    return new BooleanExpr({
      type: 'eq',
      left: this as unknown as ValueExpr<TableDecl, any>,
      right: normalizeValue(x) as ValueExpr<TableDecl, any>,
    });
  }

  ne(
    literal: LiteralValue & RowType<TableDecl>[ColumnName]
  ): BooleanExpr<TableDecl>;
  ne<OtherCol extends ColumnExpr<any, any>>(
    value: ColumnSameSpacetime<TableDecl, ColumnName, OtherCol>
  ): BooleanExpr<TableDecl | ExtractTable<OtherCol>>;

  ne(x: any): any {
    return new BooleanExpr({
      type: 'ne',
      left: this as unknown as ValueExpr<TableDecl, any>,
      right: normalizeValue(x) as ValueExpr<TableDecl, any>,
    });
  }

  lt(
    literal: LiteralValue & RowType<TableDecl>[ColumnName]
  ): BooleanExpr<TableDecl>;
  lt<OtherCol extends ColumnExpr<any, any>>(
    value: ColumnSameSpacetime<TableDecl, ColumnName, OtherCol>
  ): BooleanExpr<TableDecl | ExtractTable<OtherCol>>;

  lt(x: any): any {
    return new BooleanExpr({
      type: 'lt',
      left: this as unknown as ValueExpr<TableDecl, any>,
      right: normalizeValue(x) as ValueExpr<TableDecl, any>,
    });
  }

  lte(
    literal: LiteralValue & RowType<TableDecl>[ColumnName]
  ): BooleanExpr<TableDecl>;
  lte<OtherCol extends ColumnExpr<any, any>>(
    value: ColumnSameSpacetime<TableDecl, ColumnName, OtherCol>
  ): BooleanExpr<TableDecl | ExtractTable<OtherCol>>;

  lte(x: any): any {
    return new BooleanExpr({
      type: 'lte',
      left: this as unknown as ValueExpr<TableDecl, any>,
      right: normalizeValue(x) as ValueExpr<TableDecl, any>,
    });
  }

  gt(
    literal: LiteralValue & RowType<TableDecl>[ColumnName]
  ): BooleanExpr<TableDecl>;
  gt<OtherCol extends ColumnExpr<any, any>>(
    value: ColumnSameSpacetime<TableDecl, ColumnName, OtherCol>
  ): BooleanExpr<TableDecl | ExtractTable<OtherCol>>;

  gt(x: any): any {
    return new BooleanExpr({
      type: 'gt',
      left: this as unknown as ValueExpr<TableDecl, any>,
      right: normalizeValue(x) as ValueExpr<TableDecl, any>,
    });
  }

  gte(
    literal: LiteralValue & RowType<TableDecl>[ColumnName]
  ): BooleanExpr<TableDecl>;
  gte<OtherCol extends ColumnExpr<any, any>>(
    value: ColumnSameSpacetime<TableDecl, ColumnName, OtherCol>
  ): BooleanExpr<TableDecl | ExtractTable<OtherCol>>;

  gte(x: any): any {
    return new BooleanExpr({
      type: 'gte',
      left: this as unknown as ValueExpr<TableDecl, any>,
      right: normalizeValue(x) as ValueExpr<TableDecl, any>,
    });
  }
}

/**
 * Helper to get the spacetime type of a column.
 */
type InferSpacetimeTypeOfColumn<
  TableDecl extends TypedTableDecl,
  ColumnName extends ColumnNames<TableDecl>,
> =
  TableDecl['columns'][ColumnName]['typeBuilder'] extends TypeBuilder<
    any,
    infer U
  >
    ? U
    : never;

type ColumnNames<TableDecl extends TypedTableDecl> = keyof RowType<TableDecl> &
  string;

// For composite indexes, we only consider it as an index over the first column in the index.
type FirstIndexColumn<I extends IndexOpts<any>> =
  IndexColumns<I> extends readonly [infer Head extends string, ...infer _Rest]
    ? Head
    : never;

// Columns that are indexed by something in the indexes: [...] part.
type ExplicitIndexedColumns<TableDecl extends TypedTableDecl> =
  TableDecl['indexes'][number] extends infer I
    ? I extends IndexOpts<ColumnNames<TableDecl>>
      ? FirstIndexColumn<I> & ColumnNames<TableDecl>
      : never
    : never;

// Columns with an index defined on the column definition.
type MetadataIndexedColumns<TableDecl extends TypedTableDecl> = {
  [K in ColumnNames<TableDecl>]: ColumnIndex<
    K,
    TableDecl['columns'][K]['columnMetadata']
  > extends never
    ? never
    : K;
}[ColumnNames<TableDecl>];

export type IndexedColumnNames<TableDecl extends TypedTableDecl> =
  | ExplicitIndexedColumns<TableDecl>
  | MetadataIndexedColumns<TableDecl>;

export type IndexedRowExpr<TableDecl extends TypedTableDecl> = Readonly<{
  readonly [C in IndexedColumnNames<TableDecl>]: ColumnExpr<TableDecl, C>;
}>;

/**
 * Acts as a row when writing filters for queries. It is a way to get column references.
 */
export type RowExpr<TableDecl extends TypedTableDecl> = Readonly<{
  readonly [C in ColumnNames<TableDecl>]: ColumnExpr<TableDecl, C>;
}>;

/**
 * Union of ColumnExprs from Table whose spacetimeType is compatible with Value
 * (produces a union of ColumnExpr<Table, C> for matching columns).
 */
export type ColumnExprForValue<Table extends TypedTableDecl, Value> = {
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
  | Uuid
  | Timestamp
  | ConnectionId;

type ValueLike = LiteralValue | ColumnExpr<any, any> | LiteralExpr<any>;
type ValueInput<TableDecl extends TypedTableDecl> =
  | ValueLike
  | ValueExpr<TableDecl, any>;

export type ValueExpr<TableDecl extends TypedTableDecl, Value> =
  | LiteralExpr<Value & LiteralValue>
  | ColumnExprForValue<TableDecl, Value>;

type PredicateExpr<TableDecl extends TypedTableDecl> =
  | BooleanExpr<TableDecl>
  | ColumnExprForValue<TableDecl, SatsBool>
  | boolean;

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

function normalizePredicateExpr<TableDecl extends TypedTableDecl>(
  value: PredicateExpr<TableDecl>
): BooleanExpr<TableDecl> {
  if (value instanceof BooleanExpr) return value;
  if (typeof value === 'boolean') {
    return new BooleanExpr({
      type: 'eq',
      left: literal(value),
      right: literal(true),
    });
  }
  return new BooleanExpr({
    type: 'eq',
    left: value as ValueExpr<TableDecl, any>,
    right: literal(true),
  });
}

type EqExpr<Table extends TypedTableDecl = any> = BooleanExpr<Table>;

type BooleanExprData<Table extends TypedTableDecl> = (
  | {
      type: 'eq' | 'ne' | 'gt' | 'lt' | 'gte' | 'lte';
      left: ValueExpr<Table, any>;
      right: ValueExpr<Table, any>;
    }
  | {
      type: 'and';
      clauses: readonly [
        BooleanExprData<Table>,
        BooleanExprData<Table>,
        ...BooleanExprData<Table>[],
      ];
    }
  | {
      type: 'or';
      clauses: readonly [
        BooleanExprData<Table>,
        BooleanExprData<Table>,
        ...BooleanExprData<Table>[],
      ];
    }
  | {
      type: 'not';
      clause: BooleanExprData<Table>;
    }
) & {
  _tableType?: Table;
};

type AndOrMixedTableScopeError = {
  readonly 'Cannot combine predicates from different table scopes with and/or. In semijoin on(...), keep only the join equality and move extra predicates to .where(...).': never;
};

type RequireSameAndOrTable<
  Expected extends TypedTableDecl,
  Actual extends TypedTableDecl,
> = [Expected] extends [Actual]
  ? [Actual] extends [Expected]
    ? unknown
    : AndOrMixedTableScopeError
  : AndOrMixedTableScopeError;

export class BooleanExpr<Table extends TypedTableDecl> {
  constructor(readonly data: BooleanExprData<Table>) {}

  and<OtherTable extends TypedTableDecl>(
    other: BooleanExpr<OtherTable> & RequireSameAndOrTable<Table, OtherTable>
  ): BooleanExpr<Table> {
    return new BooleanExpr({
      type: 'and',
      clauses: [this.data, other.data as BooleanExprData<Table>],
    });
  }

  or<OtherTable extends TypedTableDecl>(
    other: BooleanExpr<OtherTable> & RequireSameAndOrTable<Table, OtherTable>
  ): BooleanExpr<Table> {
    return new BooleanExpr({
      type: 'or',
      clauses: [this.data, other.data as BooleanExprData<Table>],
    });
  }

  not(): BooleanExpr<Table> {
    return new BooleanExpr({ type: 'not', clause: this.data });
  }
}

export function not<T extends TypedTableDecl>(
  clause: BooleanExpr<T>
): BooleanExpr<T> {
  return new BooleanExpr({ type: 'not', clause: clause.data });
}

export function and<
  Table extends TypedTableDecl,
  OtherTable extends TypedTableDecl,
>(
  first: BooleanExpr<Table>,
  second: BooleanExpr<OtherTable> & RequireSameAndOrTable<Table, OtherTable>,
  ...rest: readonly BooleanExpr<Table>[]
): BooleanExpr<Table> {
  const clauses = [first, second, ...rest];
  return new BooleanExpr({
    type: 'and',
    clauses: clauses.map(c => c.data) as [
      BooleanExprData<Table>,
      BooleanExprData<Table>,
      ...BooleanExprData<Table>[],
    ],
  });
}

export function or<
  Table extends TypedTableDecl,
  OtherTable extends TypedTableDecl,
>(
  first: BooleanExpr<Table>,
  second: BooleanExpr<OtherTable> & RequireSameAndOrTable<Table, OtherTable>,
  ...rest: readonly BooleanExpr<Table>[]
): BooleanExpr<Table> {
  const clauses = [first, second, ...rest];
  return new BooleanExpr({
    type: 'or',
    clauses: clauses.map(c => c.data) as [
      BooleanExprData<Table>,
      BooleanExprData<Table>,
      ...BooleanExprData<Table>[],
    ],
  });
}

function booleanExprToSql<Table extends TypedTableDecl>(
  expr: BooleanExpr<Table> | BooleanExprData<Table>,
  tableAlias?: string
): string {
  const data = expr instanceof BooleanExpr ? expr.data : expr;
  switch (data.type) {
    case 'eq':
      return `${valueExprToSql(data.left, tableAlias)} = ${valueExprToSql(data.right, tableAlias)}`;
    case 'ne':
      return `${valueExprToSql(data.left, tableAlias)} <> ${valueExprToSql(data.right, tableAlias)}`;
    case 'gt':
      return `${valueExprToSql(data.left, tableAlias)} > ${valueExprToSql(data.right, tableAlias)}`;
    case 'gte':
      return `${valueExprToSql(data.left, tableAlias)} >= ${valueExprToSql(data.right, tableAlias)}`;
    case 'lt':
      return `${valueExprToSql(data.left, tableAlias)} < ${valueExprToSql(data.right, tableAlias)}`;
    case 'lte':
      return `${valueExprToSql(data.left, tableAlias)} <= ${valueExprToSql(data.right, tableAlias)}`;
    case 'and':
      return data.clauses
        .map(c => booleanExprToSql(c, tableAlias))
        .map(wrapInParens)
        .join(' AND ');
    case 'or':
      return data.clauses
        .map(c => booleanExprToSql(c, tableAlias))
        .map(wrapInParens)
        .join(' OR ');
    case 'not':
      return `NOT ${wrapInParens(booleanExprToSql(data.clause, tableAlias))}`;
  }
}

function wrapInParens(sql: string): string {
  return `(${sql})`;
}

function valueExprToSql<Table extends TypedTableDecl>(
  expr: ValueExpr<Table, any>,
  tableAlias?: string
): string {
  if (isLiteralExpr(expr)) {
    return literalValueToSql(expr.value);
  }
  const table = tableAlias ?? expr.table;
  return `${quoteIdentifier(table)}.${quoteIdentifier(expr.columnName)}`;
}

function literalValueToSql(value: unknown): string {
  if (value === null || value === undefined) {
    return 'NULL';
  }
  if (
    value instanceof Identity ||
    value instanceof ConnectionId ||
    value instanceof Uuid
  ) {
    // We use this hex string syntax.
    return `0x${value.toHexString()}`;
  }
  if (value instanceof Timestamp) {
    return `'${value.toISOString()}'`;
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
  return name
    .split('.')
    .map(part => `"${part.replace(/"/g, '""')}"`)
    .join('.');
}

function isLiteralExpr<Value>(
  expr: ValueExpr<any, Value>
): expr is LiteralExpr<Value & LiteralValue> {
  return (expr as LiteralExpr<Value>).type === 'literal';
}

/**
 * Evaluate a BooleanExpr against a row at runtime for client-side filtering.
 */
export function evaluateBooleanExpr(
  expr: BooleanExpr<any>,
  row: Record<string, any>
): boolean {
  return evaluateData(expr.data, row);
}

function evaluateData(
  data: BooleanExprData<any>,
  row: Record<string, any>
): boolean {
  switch (data.type) {
    case 'eq':
      return resolveValue(data.left, row) === resolveValue(data.right, row);
    case 'ne':
      return resolveValue(data.left, row) !== resolveValue(data.right, row);
    case 'gt':
      return resolveValue(data.left, row) > resolveValue(data.right, row);
    case 'gte':
      return resolveValue(data.left, row) >= resolveValue(data.right, row);
    case 'lt':
      return resolveValue(data.left, row) < resolveValue(data.right, row);
    case 'lte':
      return resolveValue(data.left, row) <= resolveValue(data.right, row);
    case 'and':
      return data.clauses.every(c => evaluateData(c, row));
    case 'or':
      return data.clauses.some(c => evaluateData(c, row));
    case 'not':
      return !evaluateData(data.clause, row);
  }
}

function resolveValue(
  expr: ValueExpr<any, any>,
  row: Record<string, any>
): any {
  if (isLiteralExpr(expr)) {
    return toComparableValue(expr.value);
  }
  return toComparableValue(row[expr.column]);
}

type TimestampLike = {
  __timestamp_micros_since_unix_epoch__: bigint;
};

type HexSerializableLike = {
  toHexString: () => string;
};

function isHexSerializableLike(value: unknown): value is HexSerializableLike {
  return (
    !!value &&
    typeof value === 'object' &&
    typeof (value as { toHexString?: unknown }).toHexString === 'function'
  );
}

// Check if this value is a Timestamp-like object. This is here because
// running locally can end up with different versions of the Timestamp class,
// which breaks the simple instanceof version.
function isTimestampLike(value: unknown): value is TimestampLike {
  if (!value || typeof value !== 'object') return false;

  if (value instanceof Timestamp) return true;

  const micros = (value as Record<string, unknown>)[
    '__timestamp_micros_since_unix_epoch__'
  ];
  return typeof micros === 'bigint';
}

// Exported for tests.
export function toComparableValue(value: any): any {
  // Handle `ConnectionId` and `Identity`.
  if (isHexSerializableLike(value)) {
    return value.toHexString();
  }
  if (isTimestampLike(value)) {
    return value.__timestamp_micros_since_unix_epoch__;
  }
  return value;
}

/**
 * Extract the table name from a query builder expression.
 */
export function getQueryTableName(query: any): string {
  if (query.table) return query.table.name; // FromBuilder
  if (query.name) return query.name; // TableRefImpl
  if (query.sourceQuery) return query.sourceQuery.table.name; // SemijoinImpl (source table)
  throw new Error('Cannot extract table name from query');
}

/**
 * Extract the accessor name from a query builder expression.
 */
export function getQueryAccessorName(query: any): string {
  if (query.table) return query.table.accessorName; // FromBuilder
  if (query.accessorName) return query.accessorName; // TableRefImpl
  if (query.sourceQuery) return query.sourceQuery.table.accessorName; // SemijoinImpl
  throw new Error('Cannot extract accessor name from query');
}

/**
 * Extract the BooleanExpr from a query builder, if any.
 */
export function getQueryWhereClause(query: any): BooleanExpr<any> | undefined {
  if (query.whereClause) return query.whereClause; // FromBuilder
  return undefined; // TableRefImpl has no where clause
}

// TODO: Fix this.
function _createIndexedRowExpr<TableDecl extends TypedTableDecl>(
  tableDef: TableDecl,
  cols: RowExpr<TableDecl>
): IndexedRowExpr<TableDecl> {
  const indexed = new Set<ColumnNames<TableDecl>>();
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
  ) as IndexedRowExpr<TableDecl>;
}
