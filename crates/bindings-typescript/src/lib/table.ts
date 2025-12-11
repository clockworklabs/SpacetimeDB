import type RawConstraintDefV9 from './autogen/raw_constraint_def_v_9_type';
import RawIndexAlgorithm from './autogen/raw_index_algorithm_type';
import type RawIndexDefV9 from './autogen/raw_index_def_v_9_type';
import type RawSequenceDefV9 from './autogen/raw_sequence_def_v_9_type';
import type RawTableDefV9 from './autogen/raw_table_def_v_9_type';
import type { AllUnique, ConstraintOpts } from './constraints';
import type {
  ColumnIndex,
  IndexColumns,
  Indexes,
  IndexOpts,
  ReadonlyIndexes,
} from './indexes';
import ScheduleAt from './schedule_at';
import type { ModuleContext } from './schema';
import type { TableSchema } from './table_schema';
import {
  RowBuilder,
  type ColumnBuilder,
  type ColumnMetadata,
  type Infer,
  type InferTypeOfRow,
  type RowObj,
  type TypeBuilder,
} from './type_builders';
import type { Prettify } from './type_util';
import { toPascalCase } from './util';

export type AlgebraicTypeRef = number;
type ColId = number;
type ColList = ColId[];

/**
 * A helper type to extract the row type from a TableDef
 */
export type RowType<TableDef extends Pick<UntypedTableDef, 'columns'>> =
  InferTypeOfRow<TableDef['columns']>;

/**
 * Coerces a column which may be a TypeBuilder or ColumnBuilder into a ColumnBuilder
 */
export type CoerceColumn<
  Col extends TypeBuilder<any, any> | ColumnBuilder<any, any, any>,
> =
  Col extends TypeBuilder<infer T, infer U>
    ? ColumnBuilder<T, U, ColumnMetadata<any>>
    : Col;

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
  accessorName: string;
  columns: Record<string, ColumnBuilder<any, any, ColumnMetadata<any>>>;
  // This is really just a ProductType where all the elements have names.
  rowType: RowBuilder<RowObj>['algebraicType']['value'];
  indexes: readonly IndexOpts<any>[];
  constraints: readonly ConstraintOpts<any>[];
};

/**
 * A type representing the indexes defined on a table.
 */
export type TableIndexes<TableDef extends UntypedTableDef> = {
  [K in keyof TableDef['columns'] & string as ColumnIndex<
    K,
    TableDef['columns'][K]['columnMetadata']
  > extends never
    ? never
    : K]: ColumnIndex<K, TableDef['columns'][K]['columnMetadata']>;
} & {
  [I in TableDef['indexes'][number] as I['name'] & {}]: TableIndexFromDef<
    TableDef,
    I
  >;
};

type TableIndexFromDef<
  TableDef extends UntypedTableDef,
  I extends IndexOpts<keyof TableDef['columns'] & string>,
> =
  NormalizeIndexColumns<TableDef, I> extends infer Cols extends ReadonlyArray<
    keyof TableDef['columns'] & string
  >
    ? {
        name: I['name'];
        unique: AllUnique<TableDef, Cols>;
        algorithm: Lowercase<I['algorithm']>;
        columns: Cols;
      }
    : never;

type NormalizeIndexColumns<
  TableDef extends UntypedTableDef,
  I extends IndexOpts<keyof TableDef['columns'] & string>,
> =
  IndexColumns<I> extends ReadonlyArray<keyof TableDef['columns'] & string>
    ? IndexColumns<I>
    : never;

/**
 * Options for configuring a database table.
 * - `name`: The name of the table.
 * - `public`: Whether the table is publicly accessible. Defaults to `false`.
 * - `indexes`: An array of index configurations for the table.
 * - `constraints`: An array of constraint configurations for the table.
 * - `scheduled`: The name of the reducer to be executed based on the scheduled rows in this table.
 */
export type TableOpts<Row extends RowObj> = {
  name: string;
  public?: boolean;
  indexes?: IndexOpts<keyof Row & string>[]; // declarative multi‑column indexes
  constraints?: ConstraintOpts<keyof Row & string>[];
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
 * Extracts the constraints from TableOpts, defaulting to an empty array if none are provided.
 */
type OptsConstraints<Opts extends TableOpts<any>> = Opts extends {
  constraints: infer Constraints extends NonNullable<any[]>;
}
  ? Constraints
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

export type ReadonlyTable<TableDef extends UntypedTableDef> = Prettify<
  ReadonlyTableMethods<TableDef> &
    ReadonlyIndexes<TableDef, TableIndexes<TableDef>>
>;

export interface ReadonlyTableMethods<TableDef extends UntypedTableDef> {
  /** Returns the number of rows in the TX state. */
  count(): bigint;

  /** Iterate over all rows in the TX state. Rust Iterator<Item=Row> → TS IterableIterator<Row>. */
  iter(): IteratorObject<Prettify<RowType<TableDef>>, undefined>;
  [Symbol.iterator](): IteratorObject<Prettify<RowType<TableDef>>, undefined>;
}

/**
 * A type representing the methods available on a table.
 */
export interface TableMethods<TableDef extends UntypedTableDef>
  extends ReadonlyTableMethods<TableDef> {
  /**
   * Insert and return the inserted row (auto-increment fields filled).
   *
   * May throw on error:
   * * If there are any unique or primary key columns in this table, may throw {@link UniqueAlreadyExists}.
   * * If there are any auto-incrementing columns in this table, may throw {@link AutoIncOverflow}.
   * */
  insert(row: Prettify<RowType<TableDef>>): Prettify<RowType<TableDef>>;

  /** Delete a row equal to `row`. Returns true if something was deleted. */
  delete(row: Prettify<RowType<TableDef>>): boolean;
}

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
 *     id: t.u32().primaryKey(),
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

  if (row.typeName === undefined) {
    row.typeName = toPascalCase(name);
  }

  row.algebraicType.value.elements.forEach((elem, i) => {
    colIds.set(elem.name, i);
    colNameList.push(elem.name);
  });

  // gather primary keys, per‑column indexes, uniques, sequences
  const pk: ColList = [];
  const indexes: Infer<typeof RawIndexDefV9>[] = [];
  const constraints: Infer<typeof RawConstraintDefV9>[] = [];
  const sequences: Infer<typeof RawSequenceDefV9>[] = [];

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
      let algorithm: Infer<typeof RawIndexAlgorithm>;
      switch (algo) {
        case 'btree':
          algorithm = RawIndexAlgorithm.BTree([id]);
          break;
        case 'direct':
          algorithm = RawIndexAlgorithm.Direct(id);
          break;
      }
      indexes.push({
        name: undefined, // Unnamed indexes will be assigned a globally unique name
        accessorName: name, // The name of this column will be used as the accessor name
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

    // If this column is shaped like ScheduleAtAlgebraicType, mark it as the schedule‑at column
    if (scheduled) {
      const algebraicType = builder.typeBuilder.algebraicType;
      if (ScheduleAt.isScheduleAt(algebraicType)) {
        scheduleAtCol = colIds.get(name)!;
      }
    }
  }

  // convert explicit multi‑column indexes coming from options.indexes
  for (const indexOpts of userIndexes ?? []) {
    let algorithm: Infer<typeof RawIndexAlgorithm>;
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
    // unnamed indexes will be assigned a globally unique name
    // The name users supply is actually the accessor name which will be used
    // in TypeScript to access the index. This will be used verbatim.
    // This is confusing because it is not the index name and there is
    // no actual way for the user to set the actual index name.
    // I think we should standardize: name and accessorName as the way to set
    // the name and accessor name of an index across all SDKs.
    indexes.push({ name: undefined, accessorName: indexOpts.name, algorithm });
  }

  // add explicit constraints from options.constraints
  for (const constraintOpts of opts.constraints ?? []) {
    if (constraintOpts.constraint === 'unique') {
      const data: Infer<typeof RawConstraintDefV9>['data'] = {
        tag: 'Unique',
        value: { columns: constraintOpts.columns.map(c => colIds.get(c)!) },
      };
      constraints.push({ name: constraintOpts.name, data });
      continue;
    }
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

  const tableDef = (ctx: ModuleContext): Infer<typeof RawTableDefV9> => ({
    name,
    productTypeRef: ctx.registerTypesRecursively(row).ref,
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
  });

  const productType = row.algebraicType.value as RowBuilder<
    CoerceRow<Row>
  >['algebraicType']['value'];

  return {
    rowType: row as RowBuilder<CoerceRow<Row>>,
    tableName: name,
    rowSpacetimeType: productType,
    tableDef,
    idxs: {} as OptsIndices<Opts>,
    constraints: constraints as OptsConstraints<Opts>,
  };
}
