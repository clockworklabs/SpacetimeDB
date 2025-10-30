import { AlgebraicType } from '../lib/algebraic_type';
import type RawConstraintDefV9 from '../lib/autogen/raw_constraint_def_v_9_type';
import RawIndexAlgorithm from '../lib/autogen/raw_index_algorithm_type';
import type RawIndexDefV9 from '../lib/autogen/raw_index_def_v_9_type';
import type RawSequenceDefV9 from '../lib/autogen/raw_sequence_def_v_9_type';
import type RawTableDefV9 from '../lib/autogen/raw_table_def_v_9_type';
import type { AllUnique } from './constraints';
import type { ColumnIndex, IndexColumns, Indexes, IndexOpts } from './indexes';
import { MODULE_DEF, splitName } from './schema';
import {
  RowBuilder,
  type ColumnBuilder,
  type ColumnMetadata,
  type InferTypeOfRow,
  type RowObj,
  type TypeBuilder,
} from './type_builders';
import type { Prettify } from './type_util';

export type AlgebraicTypeRef = number;
type ColId = number;
type ColList = ColId[];

/**
 * A helper type to extract the row type from a TableDef
 */
export type RowType<TableDef extends UntypedTableDef> = InferTypeOfRow<
  TableDef['columns']
>;

/**
 * Coerces a column which may be a TypeBuilder or ColumnBuilder into a ColumnBuilder
 */
export type CoerceColumn<
  Col extends TypeBuilder<any, any> | ColumnBuilder<any, any, any>,
> =
  Col extends TypeBuilder<infer T, infer U> ? ColumnBuilder<T, U, object> : Col;

/**
 * Coerces a RowObj where TypeBuilders are replaced with ColumnBuilders
 */
export type CoerceRow<Row extends RowObj> = {
  [k in keyof Row & string]: CoerceColumn<Row[k]>;
};

/**
 * Helper type to coerce an array of IndexOpts
 */
type CoerceArray<X extends IndexOpts<any>[]> = X;

/**
 * An untyped representation of a table's schema.
 */
export type UntypedTableDef = {
  name: string;
  columns: Record<string, ColumnBuilder<any, any, ColumnMetadata<any>>>;
  indexes: IndexOpts<any>[];
};

/**
 * A type representing the indexes defined on a table.
 */
export type TableIndexes<TableDef extends UntypedTableDef> = {
  [k in keyof TableDef['columns'] & string]: ColumnIndex<
    k,
    TableDef['columns'][k]['columnMetadata']
  >;
} & {
  [I in TableDef['indexes'][number] as I['name'] & {}]: {
    name: I['name'];
    unique: AllUnique<TableDef, IndexColumns<I>>;
    algorithm: Lowercase<I['algorithm']>;
    columns: IndexColumns<I>;
  };
};

/**
 * Options for configuring a database table.
 * - `name`: The name of the table.
 * - `public`: Whether the table is publicly accessible. Defaults to `false`.
 * - `indexes`: An array of index configurations for the table.
 * - `scheduled`: The name of the reducer to be executed based on the scheduled rows in this table.
 */
export type TableOpts<Row extends RowObj> = {
  name: string;
  public?: boolean;
  indexes?: IndexOpts<keyof Row & string>[]; // declarative multi‑column indexes
  scheduled?: string;
};

/**
 * Extracts the indices from TableOpts, defaulting to an empty array if none are provided.
 */
type OptsIndices<Opts extends TableOpts<any>> = Opts extends {
  indexes: infer Ixs extends NonNullable<any[]>;
}
  ? Ixs
  : CoerceArray<[]>;

/**
 * Table<Row, UniqueConstraintViolation = never, AutoIncOverflow = never>
 *
 * - Row: row shape
 * - UCV: unique-constraint violation error type (never if none)
 * - AIO: auto-increment overflow error type (never if none)
 */
export type Table<TableDef extends UntypedTableDef> = Prettify<
  TableMethods<TableDef> & Indexes<TableDef, TableIndexes<TableDef>>
>;

/**
 * A type representing the methods available on a table.
 */
export type TableMethods<TableDef extends UntypedTableDef> = {
  /** Returns the number of rows in the TX state. */
  count(): bigint;

  /** Iterate over all rows in the TX state. Rust Iterator<Item=Row> → TS IterableIterator<Row>. */
  iter(): IterableIterator<RowType<TableDef>>;
  [Symbol.iterator](): IterableIterator<RowType<TableDef>>;

  /**
   * Insert and return the inserted row (auto-increment fields filled).
   *
   * May throw on error:
   * * If there are any unique or primary key columns in this table, may throw {@link UniqueAlreadyExists}.
   * * If there are any auto-incrementing columns in this table, may throw {@link AutoIncOverflow}.
   * */
  insert(row: RowType<TableDef>): RowType<TableDef>;

  /** Delete a row equal to `row`. Returns true if something was deleted. */
  delete(row: RowType<TableDef>): boolean;
};

/**
 * Represents a handle to a database table, including its name, row type, and row spacetime type.
 */
export type TableSchema<
  TableName extends string,
  Row extends Record<string, ColumnBuilder<any, any, any>>,
  Idx extends readonly IndexOpts<keyof Row & string>[],
> = {
  readonly rowType: RowBuilder<Row>;

  /**
   * The name of the table.
   */
  readonly tableName: TableName;

  /**
   * The {@link AlgebraicType} representing the structure of a row in the table.
   */
  readonly rowSpacetimeType: AlgebraicType;

  /**
   * The {@link RawTableDefV9} of the configured table
   */
  readonly tableDef: RawTableDefV9;

  /**
   * The indexes defined on the table.
   */
  readonly idxs: Idx;
};

/**
 * Defines a database table with schema and options
 * @param opts - Table configuration including name, indexes, and access control
 * @param row - Product type defining the table's row structure
 * @returns Table handle for use in schema() function
 * @example
 * ```ts
 * const playerTable = table(
 *   { name: 'player', public: true },
 *   t.object({
 *     id: t.u32().primary_key(),
 *     name: t.string().index('btree')
 *   })
 * );
 * ```
 */
export function table<Row extends RowObj, const Opts extends TableOpts<Row>>(
  opts: Opts,
  row: Row | RowBuilder<Row>
): TableSchema<Opts['name'], CoerceRow<Row>, OptsIndices<Opts>> {
  const {
    name,
    public: isPublic = false,
    indexes: userIndexes = [],
    scheduled,
  } = opts;

  // 1. column catalogue + helpers
  const colIds = new Map<keyof Row & string, ColId>();
  const colNameList: string[] = [];

  if (!(row instanceof RowBuilder)) {
    row = new RowBuilder(row);
  }

  row.resolveType().value.elements.forEach((elem, i) => {
    colIds.set(elem.name, i);
    colNameList.push(elem.name);
  });

  // gather primary keys, per‑column indexes, uniques, sequences
  const pk: ColList = [];
  const indexes: RawIndexDefV9[] = [];
  const constraints: RawConstraintDefV9[] = [];
  const sequences: RawSequenceDefV9[] = [];

  let scheduleAtCol: ColId | undefined;

  for (const [name, builder] of Object.entries(row.row)) {
    const meta: ColumnMetadata<any> = builder.columnMetadata;

    if (meta.isPrimaryKey) {
      pk.push(colIds.get(name)!);
    }

    const isUnique = meta.isUnique || meta.isPrimaryKey;

    // implicit 1‑column indexes
    if (meta.indexType || isUnique) {
      const algo = meta.indexType ?? 'btree';
      const id = colIds.get(name)!;
      let algorithm: RawIndexAlgorithm;
      switch (algo) {
        case 'btree':
          algorithm = RawIndexAlgorithm.BTree([id]);
          break;
        case 'direct':
          algorithm = RawIndexAlgorithm.Direct(id);
          break;
      }
      indexes.push({
        name: undefined,
        accessorName: name,
        algorithm,
      });
    }

    if (isUnique) {
      constraints.push({
        name: undefined,
        data: { tag: 'Unique', value: { columns: [colIds.get(name)!] } },
      });
    }

    if (meta.isAutoIncrement) {
      sequences.push({
        name: undefined,
        start: undefined,
        minValue: undefined,
        maxValue: undefined,
        column: colIds.get(name)!,
        increment: 1n,
      });
    }

    if (meta.isScheduleAt) {
      scheduleAtCol = colIds.get(name)!;
    }
  }

  // convert explicit multi‑column indexes coming from options.indexes
  for (const indexOpts of userIndexes ?? []) {
    let algorithm: RawIndexAlgorithm;
    switch (indexOpts.algorithm) {
      case 'btree':
        algorithm = {
          tag: 'BTree',
          value: indexOpts.columns.map(c => colIds.get(c)!),
        };
        break;
      case 'direct':
        algorithm = { tag: 'Direct', value: colIds.get(indexOpts.column)! };
        break;
    }
    indexes.push({ name: undefined, accessorName: indexOpts.name, algorithm });
  }

  for (const index of indexes) {
    const cols =
      index.algorithm.tag === 'Direct'
        ? [index.algorithm.value]
        : index.algorithm.value;
    const colS = cols.map(i => colNameList[i]).join('_');
    index.name = `${name}_${colS}_idx_${index.algorithm.tag.toLowerCase()}`;
  }

  // Temporarily set the type ref to 0. We will set this later
  // in the schema function.

  const tableDef: RawTableDefV9 = {
    name,
    productTypeRef: row.algebraicType.value as number,
    primaryKey: pk,
    indexes,
    constraints,
    sequences,
    schedule:
      scheduled && scheduleAtCol !== undefined
        ? {
            name: undefined,
            reducerName: scheduled,
            scheduledAtColumn: scheduleAtCol,
          }
        : undefined,
    tableType: { tag: 'User' },
    tableAccess: { tag: isPublic ? 'Public' : 'Private' },
  };

  if (!row.nameProvided) {
    MODULE_DEF.types.push({
      customOrdering: true,
      name: splitName(name),
      ty: row.algebraicType.value as number,
    });
  }

  const productType = AlgebraicType.Product({
    elements: row.resolveType().value.elements.map(elem => {
      return { name: elem.name, algebraicType: elem.algebraicType };
    }),
  });

  return {
    rowType: row as RowBuilder<CoerceRow<Row>>,
    tableName: name,
    rowSpacetimeType: productType,
    tableDef,
    idxs: indexes as OptsIndices<Opts>,
  };
}
