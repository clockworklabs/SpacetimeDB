// import {
//   AlgebraicType,
//   ProductType,
//   ProductTypeElement,
// } from '../algebraic_type';
// import type RawConstraintDefV9 from '../lib/autogen/raw_constraint_def_v_9_type';
// import RawIndexAlgorithm from '../lib/autogen/raw_index_algorithm_type';
// import type RawIndexDefV9 from '../lib/autogen/raw_index_def_v_9_type';
// import { RawModuleDefV9 } from "../lib/autogen/raw_module_def_v_9_type";
// import type RawReducerDefV9 from '../lib/autogen/raw_reducer_def_v_9_type';
// import type RawSequenceDefV9 from '../lib/autogen/raw_sequence_def_v_9_type';
// import Lifecycle from '../lib/autogen/lifecycle_type';
// import ScheduleAt from '../schedule_at';
// import RawTableDefV9 from '../lib/autogen/raw_table_def_v_9_type';
// import type Typespace from '../lib/autogen/typespace_type';
// import type { ColumnBuilder } from './type_builders';
// import t from "./type_builders";
import {
  AlgebraicType,
  ConnectionId,
  Identity,
  ProductType,
  t,
  Timestamp,
} from '..';
import Lifecycle from '../lib/autogen/lifecycle_type';
import type RawConstraintDefV9 from '../lib/autogen/raw_constraint_def_v_9_type';
import RawIndexAlgorithm from '../lib/autogen/raw_index_algorithm_type';
import type RawIndexDefV9 from '../lib/autogen/raw_index_def_v_9_type';
import type RawModuleDefV9 from '../lib/autogen/raw_module_def_v_9_type';
import type RawReducerDefV9 from '../lib/autogen/raw_reducer_def_v_9_type';
import type RawSequenceDefV9 from '../lib/autogen/raw_sequence_def_v_9_type';
import type RawTableDefV9 from '../lib/autogen/raw_table_def_v_9_type';
import type Typespace from '../lib/autogen/typespace_type';
import type { AutoIncOverflow, UniqueAlreadyExists } from './rt';
import {
  ColumnBuilder,
  type ColumnMetadata,
  type IndexTypes,
  type InferTypeOfRow,
  type TypeBuilder,
} from './type_builders';
import type { Prettify } from './type_util';

/*****************************************************************
 * the run‑time catalogue that we are filling
 *****************************************************************/
export const MODULE_DEF: RawModuleDefV9 = {
  typespace: { types: [] },
  tables: [],
  reducers: [],
  types: [],
  miscExports: [],
  rowLevelSecurity: [],
};

type AlgebraicTypeRef = number;
type ColId = number;
type ColList = ColId[];

/**
 * Represents a handle to a database table, including its name, row type, and row spacetime type.
 */
type TableSchema<
  TableName extends string,
  Row extends Record<string, ColumnBuilder<any, any, any>>,
  Idx extends readonly PendingIndex<keyof Row & string>[],
> = {
  /**
   * The TypeScript phantom type. This is not stored at runtime,
   * but is visible to the compiler
   */
  readonly rowType: Row;

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

  readonly idxs: Idx;
};

type RowObj = Record<
  string,
  TypeBuilder<any, any> | ColumnBuilder<any, any, any>
>;

type CoerceRow<Row extends RowObj> = {
  [k in keyof Row & string]: CoerceColumn<Row[k]>;
};

type CoerceColumn<
  Col extends TypeBuilder<any, any> | ColumnBuilder<any, any, any>,
> =
  Col extends TypeBuilder<infer T, infer U> ? ColumnBuilder<T, U, object> : Col;

type TableOpts<Row extends RowObj> = {
  name: string;
  public?: boolean;
  indexes?: PendingIndex<keyof Row & string>[]; // declarative multi‑column indexes
  scheduled?: string; // reducer name for cron‑like tables
};

/**
 * Index helper type used *inside* {@link table} to enforce that only
 * existing column names are referenced.
 */
type PendingIndex<AllowedCol extends string> = {
  name?: string;
  accessor_name?: string;
  is_unique?: boolean;
  algorithm:
    | { tag: 'BTree'; value: { columns: readonly AllowedCol[] } }
    // | { tag: 'Hash'; value: { columns: readonly AllowedCol[] } }
    | { tag: 'Direct'; value: { column: AllowedCol } };
};

type OptsIndices<Opts extends TableOpts<any>> = Opts extends {
  indexes: infer Ixs extends NonNullable<any[]>;
}
  ? Ixs
  : CoerceArray<[]>;
type CoerceArray<X extends PendingIndex<any>[]> = X;

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
  row: Row
): TableSchema<Opts['name'], CoerceRow<Row>, OptsIndices<Opts>> {
  const {
    name,
    public: isPublic = false,
    indexes: userIndexes = [],
    scheduled,
  } = opts;

  /** 1. column catalogue + helpers */
  const colIds = new Map<keyof Row & string, ColId>();
  const colIdList: ColList = [];

  let nextCol: number = 0;
  for (const colName of Object.keys(row) as (keyof Row & string)[]) {
    colIds.set(colName, nextCol++);
    colIdList.push(colIds.get(colName)!);
  }

  /** 2. gather primary keys, per‑column indexes, uniques, sequences */
  const pk: ColList = [];
  const indexes: RawIndexDefV9[] = [];
  const constraints: RawConstraintDefV9[] = [];
  const sequences: RawSequenceDefV9[] = [];

  let scheduleAtCol: ColId | undefined;

  for (const [name, builder] of Object.entries(row)) {
    const meta: ColumnMetadata =
      'columnMetadata' in builder ? builder.columnMetadata : {};

    /* primary key */
    if (meta.isPrimaryKey) pk.push(colIds.get(name)!);

    const isUnique = meta.isUnique || meta.isPrimaryKey;

    /* implicit 1‑column indexes */
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

    /* uniqueness */
    if (isUnique) {
      constraints.push({
        name: undefined,
        data: { tag: 'Unique', value: { columns: [colIds.get(name)!] } },
      });
    }

    /* auto increment */
    if (meta.isAutoIncrement) {
      sequences.push({
        name: undefined,
        start: 0n,
        minValue: 0n,
        maxValue: 0n,
        column: colIds.get(name)!,
        increment: 1n,
      });
    }

    /* scheduleAt */
    if (meta.isScheduleAt) scheduleAtCol = colIds.get(name)!;
  }

  /** 3. convert explicit multi‑column indexes coming from options.indexes */
  for (const pending of userIndexes ?? []) {
    const converted: RawIndexDefV9 = {
      name: pending.name,
      accessorName: pending.accessor_name,
      algorithm:
        pending.algorithm.tag === 'Direct'
          ? {
              tag: 'Direct',
              value: colIds.get(pending.algorithm.value.column)!,
            }
          : {
              tag: pending.algorithm.tag,
              value: pending.algorithm.value.columns.map(c => colIds.get(c)!),
            },
    };
    indexes.push(converted);
  }

  // Temporarily set the type ref to 0. We will set this later
  // in the schema function.
  const productTypeRef = 0;

  /** 5. finalise table record */
  const tableDef: RawTableDefV9 = {
    name,
    productTypeRef,
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

  const productType = AlgebraicType.Product({
    elements: Object.entries(row).map(([columnName, columnBuilder]) => {
      // If it's a ColumnBuilder, use .typeBuilder.algebraicType, else use .algebraicType directly
      const algebraicType =
        'typeBuilder' in columnBuilder
          ? columnBuilder.typeBuilder.algebraicType
          : columnBuilder.algebraicType;
      return { name: columnName, algebraicType };
    }),
  });

  return {
    tableName: name, // stays the literal "users" | "posts"
    rowSpacetimeType: productType,
    tableDef,
    idxs: userIndexes as OptsIndices<Opts>,
    rowType: {} as CoerceRow<Row>,
  };
}

class Schema<S extends UntypedSchemaDef> {
  readonly tablesDef: { tables: RawTableDefV9[] };
  readonly typespace: Typespace;
  private readonly schemaType!: S;

  constructor(tables: RawTableDefV9[], typespace: Typespace) {
    this.tablesDef = { tables };
    this.typespace = typespace;
  }
}

/** @returns {UntypedSchemaDef} */
type TablesToSchema<T extends readonly TableSchema<any, any, any>[]> = {
  tables: {
    /** @type {UntypedTableDef} */
    readonly [i in keyof T]: {
      name: T[i]['tableName'];
      columns: T[i]['rowType'];
      indexes: T[i]['idxs'];
    };
  };
};

type InferSchema<SchemaDef extends Schema<any>> =
  SchemaDef extends Schema<infer S> ? S : never;

/**
 * Creates a schema from table definitions
 * @param handles - Array of table handles created by table() function
 * @returns ColumnBuilder representing the complete database schema
 * @example
 * ```ts
 * const s = schema(
 *   table({ name: 'users' }, userTable),
 *   table({ name: 'posts' }, postTable)
 * );
 * ```
 */
export function schema<const H extends readonly TableSchema<any, any, any>[]>(
  ...handles: H
): Schema<TablesToSchema<H>>;

/**
 * Creates a schema from table definitions
 * @param handles - Array of table handles created by table() function
 * @returns ColumnBuilder representing the complete database schema
 * @example
 * ```ts
 * const s = schema(
 *   table({ name: 'users' }, userTable),
 *   table({ name: 'posts' }, postTable)
 * );
 * ```
 */
export function schema<const H extends readonly TableSchema<any, any, any>[]>(
  ...handles: H
): Schema<TablesToSchema<H>>;

/**
 * Creates a schema from table definitions (array overload)
 * @param handles - Array of table handles created by table() function
 * @returns ColumnBuilder representing the complete database schema
 */
export function schema<const H extends readonly TableSchema<any, any, any>[]>(
  handles: H
): Schema<TablesToSchema<H>>;

export function schema(
  ...args:
    | [readonly TableSchema<any, any, any>[]]
    | readonly TableSchema<any, any, any>[]
): Schema<UntypedSchemaDef> {
  const handles: readonly TableSchema<any, any, any>[] =
    args.length === 1 && Array.isArray(args[0]) ? args[0] : args;

  const tableDefs = handles.map(h => h.tableDef);

  // Traverse the tables in order. For each newly encountered
  // insert the type into the typespace and increment the product
  // type reference, inserting the product type reference into the
  // table.
  let productTypeRef: AlgebraicTypeRef = 0;
  const typespace: Typespace = {
    types: [],
  };
  handles.forEach(h => {
    const tableType = h.rowSpacetimeType;
    // Insert the table type into the typespace
    typespace.types.push(tableType);
    h.tableDef.productTypeRef = productTypeRef;
    // Increment the product type reference
    productTypeRef++;
  });

  // Side-effect:
  // Modify the `MODULE_DEF` which will be read by
  // __describe_module__
  MODULE_DEF.tables.push(...tableDefs);
  MODULE_DEF.typespace = typespace;

  return new Schema(tableDefs, typespace);
}

/**
 * shared helpers
 */
type Values<T> = T[keyof T];

/*****************************************************************
 *  Type helpers
 *****************************************************************/
type ParamsObj = Record<string, TypeBuilder<any, any>>;

/*****************************************************************
 * reducer()
 *****************************************************************/
type ParamsAsObject<ParamDef extends ParamsObj | RowObj> =
  InferTypeOfRow<ParamDef>;

/*****************************************************************
 * procedure()
 *
 * Stored procedures are opaque to the DB engine itself, so we just
 * keep them out of `RawModuleDefV9` for now – you can forward‑declare
 * a companion `RawMiscModuleExportV9` type later if desired.
 *****************************************************************/
export function procedure<
  Name extends string,
  Params extends Record<string, ColumnBuilder<any, any, any>>,
  Ctx,
  R,
>(
  _name: Name,
  _params: Params,
  _fn: (ctx: Ctx, payload: ParamsAsObject<Params>) => Promise<R> | R
): void {
  /* nothing to push yet — left for your misc export section */
}

/*****************************************************************
 * internal: pushReducer() helper used by reducer() and lifecycle wrappers
 *****************************************************************/
function pushReducer(
  name: string,
  params: RowObj,
  fn: Reducer<any, any>,
  lifecycle?: RawReducerDefV9['lifecycle']
): void {
  const paramType: ProductType = {
    elements: Object.entries(params).map(([n, c]) => ({
      name: n,
      algebraicType: ('typeBuilder' in c ? c.typeBuilder : c).algebraicType,
    })),
  };

  MODULE_DEF.reducers.push({
    name,
    params: paramType,
    lifecycle, // <- lifecycle flag lands here
  });

  REDUCERS.push(fn);
}

type Reducer<S extends UntypedSchemaDef, Params extends RowObj> = (
  ctx: ReducerCtx<S>,
  payload: ParamsAsObject<Params>
) => void;

export const REDUCERS: Reducer<any, any>[] = [];

/*****************************************************************
 * reducer() – leave behavior the same; delegate to pushReducer()
 *****************************************************************/

/*****************************************************************
 * Lifecycle reducers
 * - register with lifecycle: 'init' | 'on_connect' | 'on_disconnect'
 * - keep the same call shape you're already using
 *****************************************************************/
export function init<S extends UntypedSchemaDef, Params extends ParamsObj>(
  params: Params,
  fn: Reducer<S, Params>
): void {
  pushReducer('init', params, fn, Lifecycle.Init);
}

export function clientConnected<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
>(params: Params, fn: Reducer<S, Params>): void {
  pushReducer('on_connect', params, fn, Lifecycle.OnConnect);
}

export function clientDisconnected<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
>(params: Params, fn: Reducer<S, Params>): void {
  pushReducer('on_disconnect', params, fn, Lifecycle.OnDisconnect);
}

type UntypedSchemaDef = {
  tables: readonly UntypedTableDef[];
};

/**
 * Reducer context parametrized by the inferred Schema
 */
export type ReducerCtx<SchemaDef extends UntypedSchemaDef> = Readonly<{
  sender: Identity;
  timestamp: Timestamp;
  connection_id: ConnectionId | null;
  db: DbView<SchemaDef>;
}>;

export type DbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in SchemaDef['tables'][number] as Tbl['name']]: Table<Tbl>;
};

export type ModuleDef<S extends UntypedSchemaDef> = {
  reducer<Params extends ParamsObj | RowObj>(
    name: string,
    params: Params,
    fn: Reducer<S, Params>
  ): void;

  init<Params extends ParamsObj>(params: Params, fn: Reducer<S, Params>): void;

  clientConnected<Params extends ParamsObj>(
    params: Params,
    fn: Reducer<S, Params>
  ): void;

  clientDisconnected<Params extends ParamsObj>(
    params: Params,
    fn: Reducer<S, Params>
  ): void;
};

export function moduleDef<S extends UntypedSchemaDef>(): ModuleDef<S> {
  return { reducer, init, clientConnected, clientDisconnected };
}

export function reducer<
  S extends UntypedSchemaDef,
  Params extends ParamsObj | RowObj,
>(
  name: string,
  params: Params,
  fn: (ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void
): void {
  pushReducer(name, params, fn);
}

export type UntypedTableDef = {
  name: string;
  columns: Record<string, ColumnBuilder<any, any, ColumnMetadata>>;
  indexes: PendingIndex<any>[];
};

export type RowType<TableDef extends UntypedTableDef> = InferTypeOfRow<
  TableDef['columns']
>;

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

export type TableMethods<TableDef extends UntypedTableDef> = {
  /** Returns the number of rows in the TX state. */
  count(): bigint;

  /** Iterate over all rows in the TX state. Rust Iterator<Item=Row> → TS IterableIterator<Row>. */
  iter(): IterableIterator<RowType<TableDef>>;
  [Symbol.iterator](): IterableIterator<RowType<TableDef>>;

  /** Insert and return the inserted row (auto-increment fields filled). May throw on error. */
  insert(row: RowType<TableDef>): RowType<TableDef>;

  /** Like insert, but returns a Result instead of throwing. */
  tryInsert(
    row: RowType<TableDef>
  ): Result<RowType<TableDef>, TryInsertError<TableDef>>;

  /** Delete a row equal to `row`. Returns true if something was deleted. */
  delete(row: RowType<TableDef>): boolean;
};

export type TableIndexes<TableDef extends UntypedTableDef> = {
  [k in keyof TableDef['columns'] & string]: ColumnIndex<
    k,
    TableDef['columns'][k]['columnMetadata']
  >;
} & {
  [I in TableDef['indexes'][number] as I['name'] & {}]: {
    name: I['name'];
    unique: AllUnique<TableDef, IndexColumns<I>>;
    algorithm: Lowercase<I['algorithm']['tag']>;
    columns: IndexColumns<I>;
  };
};

type AllUnique<
  TableDef extends UntypedTableDef,
  Columns extends Array<keyof TableDef['columns']>,
> = {
  [i in keyof Columns]: ColumnIsUnique<
    TableDef['columns'][Columns[i]]['columnMetadata']
  >;
} extends true[]
  ? true
  : false;

type IndexColumns<I extends PendingIndex<any>> = I['algorithm'] extends {
  value: { columns: infer C extends string[] };
}
  ? C
  : I['algorithm'] extends { value: { column: infer C extends string } }
    ? [C]
    : never;

type CollapseTuple<A extends any[]> = A extends [infer T] ? T : A;

type UntypedIndex<AllowedCol extends string> = {
  name: string;
  unique: boolean;
  algorithm: 'btree' | 'direct';
  columns: AllowedCol[];
};

export type Indexes<
  TableDef extends UntypedTableDef,
  I extends Record<string, UntypedIndex<keyof TableDef['columns'] & string>>,
> = {
  [k in keyof I]: Index<TableDef, I[k]>;
};

export type Index<
  TableDef extends UntypedTableDef,
  I extends UntypedIndex<keyof TableDef['columns'] & string>,
> = I['unique'] extends true
  ? UniqueIndex<TableDef, I>
  : RangedIndex<TableDef, I>;

export type UniqueIndex<
  TableDef extends UntypedTableDef,
  I extends UntypedIndex<keyof TableDef['columns'] & string>,
> = {
  find(col_val: IndexVal<TableDef, I>): RowType<TableDef> | null;
  delete(col_val: IndexVal<TableDef, I>): boolean;
  update(col_val: RowType<TableDef>): RowType<TableDef>;
};

export type RangedIndex<
  TableDef extends UntypedTableDef,
  I extends UntypedIndex<keyof TableDef['columns'] & string>,
> = {
  filter(
    range: IndexScanRangeBounds<TableDef, I>
  ): IterableIterator<RowType<TableDef>>;
  delete(range: IndexScanRangeBounds<TableDef, I>): number;
};

export type IndexVal<
  TableDef extends UntypedTableDef,
  I extends UntypedIndex<keyof TableDef['columns'] & string>,
> = CollapseTuple<_IndexVal<TableDef, I['columns']>>;

type _IndexVal<TableDef extends UntypedTableDef, Columns extends string[]> = {
  [i in keyof Columns]: TableDef['columns'][Columns[i]]['typeBuilder']['type'];
};

export type IndexScanRangeBounds<
  TableDef extends UntypedTableDef,
  I extends UntypedIndex<keyof TableDef['columns'] & string>,
> = _IndexScanRangeBounds<_IndexVal<TableDef, I['columns']>>;

// only allow omitting an array if the index is single-column - otherwise there's ambiguity
type _IndexScanRangeBounds<Columns extends any[]> = Columns extends [infer Term]
  ? Term | Range<Term>
  : _IndexScanRangeBoundsCase<Columns>;

type _IndexScanRangeBoundsCase<Columns extends any[]> = Columns extends [
  ...infer Prefix,
  infer Term,
]
  ? [...Prefix, Term | Range<Term>] | _IndexScanRangeBounds<Prefix>
  : never;

export class Range<T> {
  #from: Bound<T>;
  #to: Bound<T>;
  public constructor(from?: Bound<T> | null, to?: Bound<T> | null) {
    this.#from = from ?? { tag: 'unbounded' };
    this.#to = to ?? { tag: 'unbounded' };
  }

  public get from(): Bound<T> {
    return this.#from;
  }
  public get to(): Bound<T> {
    return this.#to;
  }
}

export type Bound<T> =
  | { tag: 'included'; value: T }
  | { tag: 'excluded'; value: T }
  | { tag: 'unbounded' };

type ColumnIndex<Name extends string, M extends ColumnMetadata> = Prettify<
  {
    name: Name;
    unique: ColumnIsUnique<M>;
    columns: [Name];
    algorithm: 'btree' | 'direct';
  } & (M extends {
    indexType: infer I extends NonNullable<IndexTypes>;
  }
    ? { algorithm: I }
    : ColumnIsUnique<M> extends true
      ? { algorithm: 'btree' }
      : never)
>;

type ColumnIsUnique<M extends ColumnMetadata> = M extends
  | { isUnique: true }
  | { isPrimaryKey: true }
  ? true
  : false;

type CheckAnyMetadata<
  TableDef extends UntypedTableDef,
  Metadata extends ColumnMetadata,
  T,
> = Values<TableDef['columns']>['columnMetadata'] extends Metadata ? T : never;

export type TryInsertError<TableDef extends UntypedTableDef> =
  | CheckAnyMetadata<
      TableDef,
      { isUnique: true } | { isPrimaryKey: true },
      UniqueAlreadyExists
    >
  | CheckAnyMetadata<TableDef, { isAutoIncrement: true }, AutoIncOverflow>;

export type Result<T, E> = { ok: true; val: T } | { ok: false; err: E };
