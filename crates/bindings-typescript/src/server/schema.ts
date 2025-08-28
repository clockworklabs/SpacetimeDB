// import {
//   AlgebraicType,
//   ProductType,
//   ProductTypeElement,
// } from '../algebraic_type';
// import type RawConstraintDefV9 from '../autogen/raw_constraint_def_v_9_type';
// import RawIndexAlgorithm from '../autogen/raw_index_algorithm_type';
// import type RawIndexDefV9 from '../autogen/raw_index_def_v_9_type';
// import { RawModuleDefV9 } from "../autogen/raw_module_def_v_9_type";
// import type RawReducerDefV9 from '../autogen/raw_reducer_def_v_9_type';
// import type RawSequenceDefV9 from '../autogen/raw_sequence_def_v_9_type';
// import Lifecycle from '../autogen/lifecycle_type';
// import ScheduleAt from '../schedule_at';
// import RawTableDefV9 from '../autogen/raw_table_def_v_9_type';
// import type Typespace from '../autogen/typespace_type';
// import type { ColumnBuilder } from './type_builders';
// import t from "./type_builders";

// type AlgebraicTypeRef = number;
// type ColId = number;
// type ColList = ColId[];

// /*****************************************************************
//  * shared helpers
//  *****************************************************************/
// type Merge<M1, M2> = M1 & Omit<M2, keyof M1>;
// type Values<T> = T[keyof T];

// /*****************************************************************
//  * the run‑time catalogue that we are filling
//  *****************************************************************/
// export const MODULE_DEF: RawModuleDefV9 = {
//   typespace: { types: [] },
//   tables: [],
//   reducers: [],
//   types: [],
//   miscExports: [],
//   rowLevelSecurity: [],
// };


// /*****************************************************************
//  *  Type helpers
//  *****************************************************************/
// type ColumnType<C> = C extends ColumnBuilder<infer JS, any> ? JS : never;
// export type Infer<S> = S extends ColumnBuilder<infer JS, any> ? JS : never;

// /*****************************************************************
//  * Index helper type used *inside* table() to enforce that only
//  * existing column names are referenced.
//  *****************************************************************/
// type PendingIndex<AllowedCol extends string> = {
//   name?: string;
//   accessor_name?: string;
//   algorithm:
//   | { tag: 'BTree'; value: { columns: readonly AllowedCol[] } }
//   | { tag: 'Hash'; value: { columns: readonly AllowedCol[] } }
//   | { tag: 'Direct'; value: { column: AllowedCol } };
// };

// /*****************************************************************
//  * table()
//  *****************************************************************/
// type TableOpts<
//   N extends string,
//   Def extends Record<string, ColumnBuilder<any, any>>,
//   Idx extends PendingIndex<keyof Def & string>[] | undefined = undefined,
// > = {
//   name: N;
//   public?: boolean;
//   indexes?: Idx; // declarative multi‑column indexes
//   scheduled?: string; // reducer name for cron‑like tables
// };

// /*****************************************************************
//  * Branded types for better IDE navigation
//  *****************************************************************/

// // Create unique symbols for each table to enable better IDE navigation
// declare const TABLE_BRAND: unique symbol;
// declare const SCHEMA_BRAND: unique symbol;

// /*****************************************************************
//  * Opaque handle that `table()` returns, now remembers the NAME literal
//  *****************************************************************/

// // Helper types for extracting info from table handles
// type RowOf<H> = H extends TableHandle<infer R, any> ? R : never;
// type NameOf<H> = H extends TableHandle<any, infer N> ? N : never;

// /*****************************************************************
//  * table() – unchanged behavior, but return typed Name on the handle
//  *****************************************************************/
// /**
//  * Defines a database table with schema and options
//  * @param opts - Table configuration including name, indexes, and access control
//  * @param row - Product type defining the table's row structure
//  * @returns Table handle for use in schema() function
//  * @example
//  * ```ts
//  * const playerTable = table(
//  *   { name: 'player', public: true },
//  *   t.object({
//  *     id: t.u32().primary_key(),
//  *     name: t.string().index('btree')
//  *   })
//  * );
//  * ```
//  */
// export function table<
//   const TableName extends string,
//   Row extends Record<string, ColumnBuilder<any, any>>,
//   Idx extends PendingIndex<keyof Row & string>[] | undefined = undefined,
// >(opts: TableOpts<TableName, Row, Idx>, row: Row): TableHandle<TableName, Infer<Row>> {
//   const {
//     name,
//     public: isPublic = false,
//     indexes: userIndexes = [],
//     scheduled,
//   } = opts;

//   /** 1. column catalogue + helpers */
//   const def = row.__def__;
//   const colIds = new Map<keyof Row & string, ColId>();
//   const colIdList: ColList = [];

//   let nextCol: number = 0;
//   for (const colName of Object.keys(def) as (keyof Row & string)[]) {
//     colIds.set(colName, nextCol++);
//     colIdList.push(colIds.get(colName)!);
//   }

//   /** 2. gather primary keys, per‑column indexes, uniques, sequences */
//   const pk: ColList = [];
//   const indexes: RawIndexDefV9[] = [];
//   const constraints: RawConstraintDefV9[] = [];
//   const sequences: RawSequenceDefV9[] = [];

//   let scheduleAtCol: ColId | undefined;

//   for (const [name, builder] of Object.entries(def) as [
//     keyof Row & string,
//     ColumnBuilder<any, any>,
//   ][]) {
//     const meta: any = builder.__meta__;

//     /* primary key */
//     if (meta.primaryKey) pk.push(colIds.get(name)!);

//     /* implicit 1‑column indexes */
//     if (meta.index) {
//       const algo = (meta.index ?? 'btree') as 'BTree' | 'Hash' | 'Direct';
//       const id = colIds.get(name)!;
//       indexes.push(
//         algo === 'Direct'
//           ? { name: "TODO", accessorName: "TODO", algorithm: RawIndexAlgorithm.Direct(id) }
//           : { name: "TODO", accessorName: "TODO", algorithm: { tag: algo, value: [id] } }
//       );
//     }

//     /* uniqueness */
//     if (meta.unique) {
//       constraints.push({
//         name: "TODO",
//         data: { tag: 'Unique', value: { columns: [colIds.get(name)!] } },
//       });
//     }

//     /* auto increment */
//     if (meta.autoInc) {
//       sequences.push({
//         name: "TODO",
//         start: 0n, // TODO
//         minValue: 0n, // TODO
//         maxValue: 0n, // TODO
//         column: colIds.get(name)!,
//         increment: 1n,
//       });
//     }

//     /* scheduleAt */
//     if (meta.scheduleAt) scheduleAtCol = colIds.get(name)!;
//   }

//   /** 3. convert explicit multi‑column indexes coming from options.indexes */
//   for (const pending of (userIndexes ?? []) as PendingIndex<
//     keyof Row & string
//   >[]) {
//     const converted: RawIndexDefV9 = {
//       name: pending.name,
//       accessorName: pending.accessor_name,
//       algorithm: ((): RawIndexAlgorithm => {
//         if (pending.algorithm.tag === 'Direct')
//           return {
//             tag: 'Direct',
//             value: colIds.get(pending.algorithm.value.column)!,
//           };
//         return {
//           tag: pending.algorithm.tag,
//           value: pending.algorithm.value.columns.map(c => colIds.get(c)!),
//         };
//       })(),
//     };
//     indexes.push(converted);
//   }

//   /** 4. add the product type to the global Typespace */
//   const productTypeRef: AlgebraicTypeRef = MODULE_DEF.typespace.types.length;
//   MODULE_DEF.typespace.types.push(row.__spacetime_type__);

//   /** 5. finalise table record */
//   const tableDef: RawTableDefV9 = {
//     name,
//     productTypeRef,
//     primaryKey: pk,
//     indexes,
//     constraints,
//     sequences,
//     schedule:
//       scheduled && scheduleAtCol !== undefined
//         ? {
//           name: "TODO",
//           reducerName: scheduled,
//           scheduledAtColumn: scheduleAtCol,
//         }
//         : undefined,
//     tableType: { tag: 'User' },
//     tableAccess: { tag: isPublic ? 'Public' : 'Private' },
//   };
//   MODULE_DEF.tables.push(tableDef);

//   return {
//     __table_name__: name as TableName,
//     __row_type__: {} as Infer<Row>,
//     __row_spacetime_type__: row.__spacetime_type__,
//   } as TableHandle<TableName, Infer<Row>>;
// }

// /*****************************************************************
//  * schema() – Fixed to properly infer table names and row types
//  *****************************************************************/

// /*****************************************************************
//  * reducer()
//  *****************************************************************/
// type ParamsAsObject<ParamDef extends Record<string, ColumnBuilder<any>>> = {
//   [K in keyof ParamDef]: Infer<ParamDef[K]>;
// };

// /*****************************************************************
//  * procedure()
//  *
//  * Stored procedures are opaque to the DB engine itself, so we just
//  * keep them out of `RawModuleDefV9` for now – you can forward‑declare
//  * a companion `RawMiscModuleExportV9` type later if desired.
//  *****************************************************************/
// export function procedure<
//   Name extends string,
//   Params extends Record<string, ColumnBuilder<any>>,
//   Ctx,
//   R,
// >(
//   _name: Name,
//   _params: Params,
//   _fn: (ctx: Ctx, payload: ParamsAsObject<Params>) => Promise<R> | R
// ): void {
//   /* nothing to push yet — left for your misc export section */
// }

// /*****************************************************************
//  * internal: pushReducer() helper used by reducer() and lifecycle wrappers
//  *****************************************************************/
// function pushReducer<
//   S,
//   Name extends string = string,
//   Params extends Record<string, ColumnBuilder<any>> = Record<
//     string,
//     ColumnBuilder<any>
//   >,
// >(
//   name: Name,
//   params: Params | ProductTypeColumnBuilder<Params>,
//   lifecycle?: RawReducerDefV9['lifecycle']
// ): void {
//   // Allow either a product-type ColumnBuilder or a plain params object
//   const paramsInternal: Params =
//     (params as any).__is_product_type__ === true
//       ? (params as ProductTypeColumnBuilder<Params>).__def__
//       : (params as Params);

//   const paramType = {
//     elements: Object.entries(paramsInternal).map(
//       ([n, c]) =>
//         ({ name: n, algebraicType: (c as ColumnBuilder<any>).__spacetime_type__ })
//     )
//   };

//   MODULE_DEF.reducers.push({
//     name,
//     params: paramType,
//     lifecycle, // <- lifecycle flag lands here
//   });
// }

// /*****************************************************************
//  * reducer() – leave behavior the same; delegate to pushReducer()
//  *****************************************************************/


// /*****************************************************************
//  * Lifecycle reducers
//  * - register with lifecycle: 'init' | 'on_connect' | 'on_disconnect'
//  * - keep the same call shape you're already using
//  *****************************************************************/
// export function init<
//   S extends Record<string, any> = any,
//   Params extends Record<string, ColumnBuilder<any>> = {},
// >(
//   name: 'init' = 'init',
//   params: Params | ProductTypeColumnBuilder<Params> = {} as any,
//   _fn?: (ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void
// ): void {
//   pushReducer(name, params, Lifecycle.Init);
// }

// export function clientConnected<
//   S extends Record<string, any> = any,
//   Params extends Record<string, ColumnBuilder<any>> = {},
// >(
//   name: 'on_connect' = 'on_connect',
//   params: Params | ProductTypeColumnBuilder<Params> = {} as any,
//   _fn?: (ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void
// ): void {
//   pushReducer(name, params, Lifecycle.OnConnect);
// }

// export function clientDisconnected<
//   S extends Record<string, any> = any,
//   Params extends Record<string, ColumnBuilder<any>> = {},
// >(
//   name: 'on_disconnect' = 'on_disconnect',
//   params: Params | ProductTypeColumnBuilder<Params> = {} as any,
//   _fn?: (ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void
// ): void {
//   pushReducer(name, params, Lifecycle.OnDisconnect);
// }

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
// //   Def extends Record<string, ColumnBuilder<any>>,
// //   Idx extends PendingIndex<keyof Def & string>[] | undefined = undefined,
// // > = {
// //   name: N;
// //   public?: boolean;
// //   indexes?: Idx; // declarative multi‑column indexes
// //   scheduled?: string; // reducer name for cron‑like tables
// // };


// // export function table<
// //   const Name extends string,
// //   Def extends Record<string, ColumnBuilder<any>>,
// //   Row extends ProductTypeColumnBuilder<Def>,
// //   Idx extends PendingIndex<keyof Def & string>[] | undefined = undefined,
// // >(opts: TableOpts<Name, Def, Idx>, row: Row): TableHandle<Infer<Row>, Name> {



// // /**
// //  * Creates a schema from table definitions
// //  * @param handles - Array of table handles created by table() function
// //  * @returns ColumnBuilder representing the complete database schema
// //  * @example
// //  * ```ts
// //  * const s = schema(
// //  *   table({ name: 'users' }, userTable),
// //  *   table({ name: 'posts' }, postTable)
// //  * );
// //  * ```
// //  */
// // export function schema<
// //   const H extends readonly TableHandle<any, any>[]
// // >(...handles: H): ColumnBuilder<TupleToSchema<H>> & {
// //   /** @internal - for IDE navigation to schema variable */
// //   readonly __schema_definition__?: never;
// // };

// // /**
// //  * Creates a schema from table definitions (array overload)
// //  * @param handles - Array of table handles created by table() function
// //  * @returns ColumnBuilder representing the complete database schema
// //  */
// // export function schema<
// //   const H extends readonly TableHandle<any, any>[]
// // >(handles: H): ColumnBuilder<TupleToSchema<H>> & {
// //   /** @internal - for IDE navigation to schema variable */
// //   readonly __schema_definition__?: never;
// // };

// // export function schema(...args: any[]): ColumnBuilder<any> {
// //   const handles =
// //     (args.length === 1 && Array.isArray(args[0]) ? args[0] : args) as TableHandle<any, any>[];

// //   const productTy = AlgebraicType.Product({
// //     elements: handles.map(h => ({
// //       name: h.__table_name__,
// //       algebraicType: h.__row_spacetime_type__,
// //     })),
// //   });

// //   return col<any>(productTy);
// // }

// type UntypedTablesTuple = TableHandle<any, any>[]; 
// function schema<TablesTuple extends UntypedTablesTuple>(...tablesTuple: TablesTuple): Schema<TablesTuple> {
//   return {
//     tables: tablesTuple
//   }
// }

// type UntypedSchemaDef = {
//   typespace: Typespace,
//   tables: [RawTableDefV9],
// }

// type Schema<Tables> = {
//   tables: Tables,
// }

// type TableHandle<TableName extends string, Row> = {
//   readonly __table_name__: TableName;
//   readonly __row_type__: Row;
//   readonly __row_spacetime_type__: AlgebraicType;
// };

// type InferSchema<SchemaDef> = SchemaDef extends Schema<infer Tables> ? Tables : never;

// /** 
//  * Reducer context parametrized by the inferred Schema
//  */
// export type ReducerCtx<SchemaDef extends UntypedSchemaDef> = {
//   db: DbView<SchemaDef>;
// };


// type DbView<SchemaDef extends UntypedSchemaDef> = {
//   [K in keyof SchemaDef]: Table<TableHandleTupleToObject<SchemaDef>>
// };


// // schema provided -> ctx.db is precise
// export function reducer<
//   S extends Record<string, any>,
//   Name extends string = string,
//   Params extends Record<string, ColumnBuilder<any>> = Record<
//     string,
//     ColumnBuilder<any>
//   >,
//   F = (ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void,
// >(name: Name, params: Params | ProductTypeColumnBuilder<Params>, fn: F): F;

// // no schema provided -> ctx.db is permissive
// export function reducer<
//   Name extends string = string,
//   Params extends Record<string, ColumnBuilder<any>> = Record<
//     string,
//     ColumnBuilder<any>
//   >,
//   F = (ctx: ReducerCtx<any>, payload: ParamsAsObject<Params>) => void,
// >(name: Name, params: Params | ProductTypeColumnBuilder<Params>, fn: F): F;

// // single implementation (S defaults to any -> JS-like)
// export function reducer<
//   S extends Record<string, any> = any,
//   Name extends string = string,
//   Params extends Record<string, ColumnBuilder<any>> = Record<
//     string,
//     ColumnBuilder<any>
//   >,
//   F = (ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void,
// >(name: Name, params: Params | ProductTypeColumnBuilder<Params>, fn: F): F {
//   pushReducer<S>(name, params);
//   return fn;
// }







// // export type Infer<S> = S extends ColumnBuilder<infer JS, any> ? JS : never;

// // Create interfaces for each table to enable better navigation
// type TableHandleTupleToObject<T extends readonly TableHandle<any, any>[]> =
//   T extends readonly [TableHandle<infer R1, infer N1>, ...infer Rest]
//   ? Rest extends readonly TableHandle<any, any>[]
//   ? { [K in N1]: R1 } & TableHandleTupleToObject<Rest>
//   : { [K in N1]: R1 }
//   : {};

// // Alternative approach - direct tuple iteration with interfaces
// type TupleToSchema<T extends readonly TableHandle<any, any>[]> = TableHandleTupleToObject<T>;

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

// /**
//  * Table<Row, UniqueConstraintViolation = never, AutoIncOverflow = never>
//  *
//  * - Row: row shape
//  * - UCV: unique-constraint violation error type (never if none)
//  * - AIO: auto-increment overflow error type (never if none)
//  */
// export type Table<TableDef extends UntypedTableDef> = {
//   /** Returns the number of rows in the TX state. */
//   count(): number;

//   /** Iterate over all rows in the TX state. Rust IteratorIterator<Item=Row> → TS Iterable<Row>. */
//   iter(): IterableIterator<Row>;

//   /** Insert and return the inserted row (auto-increment fields filled). May throw on error. */
//   insert(row: Row): Row;

//   /** Like insert, but returns a Result instead of throwing. */
//   try_insert(row: Row): Result<Row, UCV | AIO>;

//   /** Delete a row equal to `row`. Returns true if something was deleted. */
//   delete(row: Row): boolean;
// };


// type DbContext<DbView extends DbView<Row>> = {
//   db: DbView,
// };
