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
import type {
  ColumnBuilder,
  ColumnMetadata,
  InferTypeOfRow,
  TypeBuilder,
} from './type_builders';

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
type TableHandle<
  TableName extends string,
  Row extends Record<string, ColumnBuilder<any, any, any>>,
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

type TableOpts<
  TableName extends string,
  Row extends RowObj,
  Idx extends PendingIndex<keyof Row & string>[] | undefined = undefined,
> = {
  name: TableName;
  public?: boolean;
  indexes?: Idx; // declarative multi‑column indexes
  scheduled?: string; // reducer name for cron‑like tables
};

/**
 * Index helper type used *inside* {@link table} to enforce that only
 * existing column names are referenced.
 */
type PendingIndex<AllowedCol extends string> = {
  name?: string;
  accessor_name?: string;
  algorithm:
    | { tag: 'BTree'; value: { columns: readonly AllowedCol[] } }
    | { tag: 'Hash'; value: { columns: readonly AllowedCol[] } }
    | { tag: 'Direct'; value: { column: AllowedCol } };
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
export function table<
  const TableName extends string,
  Row extends RowObj,
  Idx extends PendingIndex<keyof Row & string>[] | undefined = undefined,
>(
  opts: TableOpts<TableName, Row, Idx>,
  row: Row
): TableHandle<TableName, CoerceRow<Row>> {
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

  for (const [name, builder] of Object.entries(row) as [
    keyof Row & string,
    ColumnBuilder<any, any>,
  ][]) {
    const meta: any = builder.columnMetadata;

    /* primary key */
    if (meta.primaryKey) pk.push(colIds.get(name)!);

    /* implicit 1‑column indexes */
    if (meta.index) {
      const algo = (meta.index ?? 'btree') as 'BTree' | 'Hash' | 'Direct';
      const id = colIds.get(name)!;
      indexes.push(
        algo === 'Direct'
          ? {
              name: 'TODO',
              accessorName: 'TODO',
              algorithm: RawIndexAlgorithm.Direct(id),
            }
          : {
              name: 'TODO',
              accessorName: 'TODO',
              algorithm: { tag: algo, value: [id] },
            }
      );
    }

    /* uniqueness */
    if (meta.unique) {
      constraints.push({
        name: 'TODO',
        data: { tag: 'Unique', value: { columns: [colIds.get(name)!] } },
      });
    }

    /* auto increment */
    if (meta.autoInc) {
      sequences.push({
        name: 'TODO',
        start: 0n, // TODO
        minValue: 0n, // TODO
        maxValue: 0n, // TODO
        column: colIds.get(name)!,
        increment: 1n,
      });
    }

    /* scheduleAt */
    if (meta.scheduleAt) scheduleAtCol = colIds.get(name)!;
  }

  /** 3. convert explicit multi‑column indexes coming from options.indexes */
  for (const pending of (userIndexes ?? []) as PendingIndex<
    keyof Row & string
  >[]) {
    const converted: RawIndexDefV9 = {
      name: pending.name,
      accessorName: pending.accessor_name,
      algorithm: ((): RawIndexAlgorithm => {
        if (pending.algorithm.tag === 'Direct')
          return {
            tag: 'Direct',
            value: colIds.get(pending.algorithm.value.column)!,
          };
        return {
          tag: pending.algorithm.tag,
          value: pending.algorithm.value.columns.map(c => colIds.get(c)!),
        };
      })(),
    };
    indexes.push(converted);
  }

  // TODO: Temporarily set the type ref to 0. We will set this later
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
            name: 'TODO',
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
          ? (columnBuilder as ColumnBuilder<any, any, any>).typeBuilder
              .algebraicType
          : (columnBuilder as TypeBuilder<any, any>).algebraicType;
      return { name: columnName, algebraicType };
    }),
  });

  const handle: TableHandle<TableName, CoerceRow<Row>> = {
    tableName: name, // stays the literal "users" | "posts"
    rowSpacetimeType: productType,
    tableDef,
    rowType: {} as CoerceRow<Row>,
  };
  return handle;
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

// Create interfaces for each table to enable better navigation
type TableHandleTupleToObject<T extends readonly TableHandle<any, any>[]> = {
  readonly [i in keyof T]: {
    name: T[i]['tableName'];
    columns: T[i]['rowType'];
  };
};

type TupleToSchema<T extends readonly TableHandle<any, any>[]> = {
  tables: TableHandleTupleToObject<T>;
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
export function schema<const H extends readonly TableHandle<any, any>[]>(
  ...handles: H
): Schema<TupleToSchema<H>>;

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
export function schema<const H extends readonly TableHandle<any, any>[]>(
  ...handles: H
): Schema<TupleToSchema<H>>;

/**
 * Creates a schema from table definitions (array overload)
 * @param handles - Array of table handles created by table() function
 * @returns ColumnBuilder representing the complete database schema
 */
export function schema<const H extends readonly TableHandle<any, any>[]>(
  handles: H
): Schema<TupleToSchema<H>>;

export function schema(
  ...args: [readonly TableHandle<any, any>[]] | readonly TableHandle<any, any>[]
): Schema<UntypedSchemaDef> {
  const handles: readonly TableHandle<any, any>[] =
    args.length === 1 && Array.isArray(args[0]) ? args[0] : args;

  // typespace: Typespace;
  // tables: RawTableDefV9[];
  // reducers: RawReducerDefV9[];
  // types: RawTypeDefV9[];
  // miscExports: RawMiscModuleExportV9[];
  // rowLevelSecurity: RawRowLevelSecurityDefV9[];
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
type Merge<M1, M2> = M1 & Omit<M2, keyof M1>;
type Values<T> = T[keyof T];

/*****************************************************************
 *  Type helpers
 *****************************************************************/
type ColumnType<C> = C extends ColumnBuilder<infer JS, any> ? JS : never;

type ParamsObj = Record<string, TypeBuilder<any, any>>;

/*****************************************************************
 * reducer()
 *****************************************************************/
type ParamsAsObject<ParamDef extends ParamsObj | RowObj> =
  InferTypeOfRow<ParamDef>;

// type ParamsOrRowAsObject<Params

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

// /*****************************************************************
//  * Example usage with explicit interfaces for better navigation
//  *****************************************************************/
// const point = t.object({
//   x: t.f64(),
//   y: t.f64(),
// });
// type Point = Infer<typeof point>;

// const user = {
//   id: t.string().primaryKey(),
//   name: t.string().index('btree'),
//   email: t.string(),
//   age: t.number(),
// };
// type User = Infer<typeof user>;

// const player = {
//   id: t.u32().primaryKey().autoInc(),
//   name: t.string().index('btree'),
//   score: t.number(),
//   level: t.number(),
//   foo: t.number().unique(),
//   bar: t.object({
//     x: t.f64(),
//     y: t.f64(),
//   }),
//   baz: t.enum({
//     Foo: t.f64(),
//     Bar: t.f64(),
//     Baz: t.string(),
//   }),
// };

// const sendMessageSchedule = t.object({
//   scheduleId: t.u64().primaryKey(),
//   scheduledAt: t.scheduleAt(),
//   text: t.string(),
// });

// // Create the schema with named references
// const s = schema(
//   table({
//     name: 'player',
//     public: true,
//     indexes: [
//       t.index({ name: 'my_index' }).btree({ columns: ['name', 'score'] }),
//     ],
//   }, player),
//   table({ name: 'logged_out_user' }, user),
//   table({ name: 'user' }, user),
//   table({
//     name: 'send_message_schedule',
//     scheduled: 'move_player',
//   }, sendMessageSchedule)
// );

// // Export explicit type alias for the schema
// export type Schemar = InferSchema<typeof s>;

// const foo = reducer<Schemar>('move_player', { user, point, player }, (ctx, { user, point, player }) => {
//   ctx.db.send_message_schedule.insert({
//     scheduleId: 1,
//     scheduledAt: ScheduleAt.Interval(234_000n),
//     text: 'Move player'
//   });

//   ctx.db.player.insert(player);

//   if (player.baz.tag === 'Foo') {
//     player.baz.value += 1;
//   } else if (player.baz.tag === 'Bar') {
//     player.baz.value += 2;
//   } else if (player.baz.tag === 'Baz') {
//     player.baz.value += '!';
//   }
// });

// const bar = reducer<Schemar>('foobar', {}, (ctx) => {
//   bar(ctx, {});
// })

// init('init', {}, (_ctx) => {

// })

// // Result<T, E> like Rust
// export type Result<T, E> =
//   | { ok: true; value: T }
//   | { ok: false; error: E };

//   // /* ───── generic index‑builder to be used in table options ───── */
//   // index<IdxName extends string = string>(opts?: {
//   //   name?: IdxName;
//   // }): {
//   //   btree<Cols extends readonly string[]>(def: {
//   //     columns: Cols;
//   //   }): PendingIndex<(typeof def.columns)[number]>;
//   //   hash<Cols extends readonly string[]>(def: {
//   //     columns: Cols;
//   //   }): PendingIndex<(typeof def.columns)[number]>;
//   //   direct<Col extends string>(def: { column: Col }): PendingIndex<Col>;
//   // } {
//   //   const common = { name: opts?.name };
//   //   return {
//   //     btree<Cols extends readonly string[]>(def: { columns: Cols }) {
//   //       return {
//   //         ...common,
//   //         algorithm: {
//   //           tag: 'BTree',
//   //           value: { columns: def.columns },
//   //         },
//   //       } as PendingIndex<(typeof def.columns)[number]>;
//   //     },
//   //     hash<Cols extends readonly string[]>(def: { columns: Cols }) {
//   //       return {
//   //         ...common,
//   //         algorithm: {
//   //           tag: 'Hash',
//   //           value: { columns: def.columns },
//   //         },
//   //       } as PendingIndex<(typeof def.columns)[number]>;
//   //     },
//   //     direct<Col extends string>(def: { column: Col }) {
//   //       return {
//   //         ...common,
//   //         algorithm: {
//   //           tag: 'Direct',
//   //           value: { column: def.column },
//   //         },
//   //       } as PendingIndex<Col>;
//   //     },
//   //   };
//   // },

// // type TableOpts<
// //   N extends string,
// //   Def extends Record<string, ColumnBuilder<any,any,any>>,
// //   Idx extends PendingIndex<keyof Def & string>[] | undefined = undefined,
// // > = {
// //   name: N;
// //   public?: boolean;
// //   indexes?: Idx; // declarative multi‑column indexes
// //   scheduled?: string; // reducer name for cron‑like tables
// // };

// // export function table<
// //   const Name extends string,
// //   Def extends Record<string, ColumnBuilder<any,any,any>>,
// //   Row extends ProductColumnBuilder<Def>,
// //   Idx extends PendingIndex<keyof Def & string>[] | undefined = undefined,
// // >(opts: TableOpts<Name, Def, Idx>, row: Row): TableHandle<InferTypeOfRow<Row>, Name> {

// type UntypedTablesTuple = TableHandle<any, any>[];
// function schema<TablesTuple extends UntypedTablesTuple>(...tablesTuple: TablesTuple): Schema<TablesTuple> {
//   return {
//     tables: tablesTuple
//   }
// }

type UntypedSchemaDef = {
  tables: readonly UntypedTableDef[];
};

// type Schema<Tables> = {
//   tables: Tables,
// }

// type TableHandle<TableName extends string, Row> = {
//   readonly __table_name__: TableName;
//   readonly __row_type__: Row;
//   readonly __row_spacetime_type__: AlgebraicType;
// };

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

type UntypedTableDef = {
  name: string;
  columns: Record<string, ColumnBuilder<any, any, any>>;
};

type RowType<TableDef extends UntypedTableDef> = InferTypeOfRow<
  TableDef['columns']
>;

// // export type Infer<S> = S extends ColumnBuilder<infer JS, any> ? JS : never;

// type TableNamesInSchemaDef<SchemaDef extends UntypedSchemaDef> =
//   keyof SchemaDef & string;

// type TableByName<
//   SchemaDef extends UntypedSchemaDef,
//   TableName extends TableNamesInSchemaDef<SchemaDef>,
// > = SchemaDef[TableName];

// type RowFromTable<TableDef extends UntypedTableDef> =
//   TableDef["row"];

// /**
//  * Reducer context parametrized by the inferred Schema
//  */
// type ReducerContext<SchemaDef extends UntypedSchemaDef> = {
//   db: DbView<SchemaDef>;
// };

// type AnyTable = Table<any>;
// type AnySchema = Record<TableName, Row>;

// type Outer = {

// }

// type ReducerBuilder<S> = {

// }

// type Local = {};

/**
 * Table<Row, UniqueConstraintViolation = never, AutoIncOverflow = never>
 *
 * - Row: row shape
 * - UCV: unique-constraint violation error type (never if none)
 * - AIO: auto-increment overflow error type (never if none)
 */
export type Table<TableDef extends UntypedTableDef> = {
  /** Returns the number of rows in the TX state. */
  count(): bigint;

  /** Iterate over all rows in the TX state. Rust Iterator<Item=Row> → TS IterableIterator<Row>. */
  iter(): IterableIterator<RowType<TableDef>>;
  [Symbol.iterator](): IterableIterator<RowType<TableDef>>;

  /** Insert and return the inserted row (auto-increment fields filled). May throw on error. */
  insert(row: RowType<TableDef>): RowType<TableDef>;

  /** Like insert, but returns a Result instead of throwing. */
  try_insert(
    row: RowType<TableDef>
  ): Result<RowType<TableDef>, TryInsertError<TableDef>>;

  /** Delete a row equal to `row`. Returns true if something was deleted. */
  delete(row: RowType<TableDef>): boolean;
};

type CheckAnyMetadata<
  TableDef extends UntypedTableDef,
  Metadata extends ColumnMetadata,
  T,
> = Values<TableDef['columns']>['columnMetadata'] extends Metadata ? T : never;

type TryInsertError<TableDef extends UntypedTableDef> =
  | CheckAnyMetadata<
      TableDef,
      { isUnique: true } | { isPrimaryKey: true },
      UniqueAlreadyExists
    >
  | CheckAnyMetadata<TableDef, { isAutoIncrement: true }, AutoIncOverflow>;

type Result<T, E> = { ok: true; val: T } | { ok: false; err: E };

const x = schema(table({ name: 'hello' }, { x: t.i32() }));
// const y = x.schemaType.hello.x;
// type Y = Infer<import('./type_builders').I32Builder>;
// type A = import('./type_builders').I32Builder;
// type Z = A['type'];

const s = schema(
  table(
    { name: 'users' },
    {
      id: t.string().primaryKey(),
    }
  ),
  table(
    { name: 'posts' },
    {
      id: t.string().primaryKey(),
      title: t.string(),
      content: t.string(),
      authorId: t.string(),
    }
  )
);

type S = InferSchema<typeof s>;

reducer('foo', { x: t.i32() }, (ctx: ReducerCtx<S>, { x }) => {
  type AssertEquals<T, U> =
    (<G>() => G extends T ? 1 : 2) extends <G>() => G extends U ? 1 : 2
      ? true
      : false;

  const _t1: AssertEquals<typeof x, number> = true;
  x;
});
