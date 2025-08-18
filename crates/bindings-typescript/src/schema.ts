// import {
//   AlgebraicType,
//   ProductType,
//   ProductTypeElement,
//   SumTypeVariant,
// } from './algebraic_type';
// import { sendMessage } from './reducers';

// type RawIdentifier = string;

// type AlgebraicTypeRef = number;

// type ColId = number;

// type ColList = ColId[];

// type RawIndexAlgorithm =
//   | { tag: 'btree'; value: { columns: ColList } }
//   | { tag: 'hash'; value: { columns: ColList } }
//   | { tag: 'direct'; value: { column: ColId } };

// type Typespace = {
//   types: AlgebraicType[];
// };

// type RawIndexDefV9 = {
//   name?: string;
//   accessor_name?: RawIdentifier;
//   algorithm: RawIndexAlgorithm;
// };

// type RawUniqueConstraintDataV9 = { columns: ColList };

// type RawConstraintDataV9 = { tag: 'unique'; value: RawUniqueConstraintDataV9 };

// type RawConstraintDefV9 = {
//   name?: string;
//   data: RawConstraintDataV9;
// };

// type RawSequenceDefV9 = {
//   name?: RawIdentifier;
//   column: ColId;
//   start?: number;
//   minValue?: number;
//   maxValue?: number;
//   increment: number;
// };

// type TableType = 'system' | 'user';
// type TableAccess = 'public' | 'private';

// type RawScheduleDefV9 = {
//   name?: RawIdentifier;
//   reducerName: RawIdentifier;
//   scheduledAtColumn: ColId;
// };

// type RawTableDefV9 = {
//   name: RawIdentifier;
//   productTypeRef: AlgebraicTypeRef;
//   primaryKey: ColList;
//   indexes: RawIndexDefV9[];
//   constraints: RawConstraintDefV9[];
//   sequences: RawSequenceDefV9[];
//   schedule?: RawScheduleDefV9;
//   tableType: TableType;
//   tableAccess: TableAccess;
// };

// type RawReducerDefV9 = {
//   name: RawIdentifier;
//   params: ProductType;
//   lifecycle?: 'init' | 'on_connect' | 'on_disconnect';
// };

// type RawScopedTypeNameV9 = {
//   name: RawIdentifier;
//   scope: RawIdentifier[];
// };

// type RawTypeDefV9 = {
//   name: RawScopedTypeNameV9;
//   ty: AlgebraicTypeRef;
//   customOrdering: boolean;
// };

// type RawMiscModuleExportV9 = never;

// type RawSql = string;
// type RawRowLevelSecurityDefV9 = {
//   sql: RawSql;
// };

// type RawModuleDefV9 = {
//   typespace: Typespace;
//   tables: RawTableDefV9[];
//   reducers: RawReducerDefV9[];
//   types: RawTypeDefV9[];
//   miscExports: RawMiscModuleExportV9[];
//   rowLevelSecurity: RawRowLevelSecurityDefV9[];
// };

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
//  * ColumnBuilder  – holds the JS type + Spacetime type + metadata
//  *****************************************************************/
// export interface ColumnBuilder<
//   /** JS / TS visible type */ JS,
//   /** accumulated column metadata */ M = {},
// > {
//   /** phantom – exposes the JS type to the compiler only */
//   readonly __type__: JS;
//   /** the SpacetimeDB algebraic type (run‑time value)           */
//   readonly __spacetime_type__: AlgebraicType;
//   /** plain JS object where we accumulate column metadata       */
//   readonly __meta__: M;

//   /** —— builder combinators ——————————————— */
//   index<N extends 'btree' | 'hash' = 'btree'>(
//     algorithm?: N // default = "btree"
//   ): ColumnBuilder<JS, Merge<M, { index: N }>>;

//   primary_key(): ColumnBuilder<JS, Merge<M, { primaryKey: true }>>;
//   unique(): ColumnBuilder<JS, Merge<M, { unique: true }>>;
//   auto_inc(): ColumnBuilder<JS, Merge<M, { autoInc: true }>>;
// }

// /** create the concrete (but still opaque) builder object */
// function col<JS>(__spacetime_type__: AlgebraicType): ColumnBuilder<JS, {}> {
//   const c: any = {
//     __spacetime_type__,
//     __meta__: {},
//   };

//   /** all mutators simply stamp metadata and re‑use the same object */
//   c.index = (algo: any = 'btree') => {
//     c.__meta__.index = algo;
//     return c;
//   };
//   c.primary_key = () => {
//     c.__meta__.primaryKey = true;
//     return c;
//   };
//   c.unique = () => {
//     c.__meta__.unique = true;
//     return c;
//   };
//   c.auto_inc = () => {
//     c.__meta__.autoInc = true;
//     return c;
//   };

//   return c;
// }

// /*****************************************************************
//  * Primitive factories – unchanged except we add scheduleAt()
//  *****************************************************************/
// export const t = {
//   /* ───── primitive scalars ───── */
//   bool: (): ColumnBuilder<boolean> => col(AlgebraicType.createBoolType()),
//   string: (): ColumnBuilder<string> => col(AlgebraicType.createStringType()),
//   number: (): ColumnBuilder<number> => col(AlgebraicType.createF64Type()),

//   /* integers share JS = number but differ in Kind */
//   i8: (): ColumnBuilder<number> => col(AlgebraicType.createI8Type()),
//   u8: (): ColumnBuilder<number> => col(AlgebraicType.createU8Type()),
//   i16: (): ColumnBuilder<number> => col(AlgebraicType.createI16Type()),
//   u16: (): ColumnBuilder<number> => col(AlgebraicType.createU16Type()),
//   i32: (): ColumnBuilder<number> => col(AlgebraicType.createI32Type()),
//   u32: (): ColumnBuilder<number> => col(AlgebraicType.createU32Type()),
//   i64: (): ColumnBuilder<number> => col(AlgebraicType.createI64Type()),
//   u64: (): ColumnBuilder<number> => col(AlgebraicType.createU64Type()),
//   i128: (): ColumnBuilder<number> => col(AlgebraicType.createI128Type()),
//   u128: (): ColumnBuilder<number> => col(AlgebraicType.createU128Type()),
//   i256: (): ColumnBuilder<number> => col(AlgebraicType.createI256Type()),
//   u256: (): ColumnBuilder<number> => col(AlgebraicType.createU256Type()),

//   f32: (): ColumnBuilder<number> => col(AlgebraicType.createF32Type()),
//   f64: (): ColumnBuilder<number> => col(AlgebraicType.createF64Type()),

//   /* ───── structured builders ───── */
//   object<Def extends Record<string, ColumnBuilder<any>>>(def: Def): ProductTypeColumnBuilder<Def> {
//     const productTy = AlgebraicType.createProductType(
//       Object.entries(def).map(
//         ([n, c]) => new ProductTypeElement(n, c.__spacetime_type__)
//       )
//     );
//     /** carry the *definition* alongside so `table()` can introspect */
//     return Object.assign(
//       col<{ [K in keyof Def]: ColumnType<Def[K]> }>(productTy),
//       {
//         __is_product_type__: true as const,
//         __def__: def,
//       }
//     ) as ProductTypeColumnBuilder<Def>;
//   },

//   array<E extends ColumnBuilder<any>>(e: E): ColumnBuilder<Infer<E>[]> {
//     return col<Infer<E>[]>(AlgebraicType.createArrayType(e.__spacetime_type__));
//   },

//   enum<V extends Record<string, ColumnBuilder<any>>>(
//     variants: V
//   ): ColumnBuilder<
//     {
//       [K in keyof V]: {
//         tag: K;
//         value: Infer<V[K]>;
//       };
//     }[keyof V]
//   > {
//     const ty = AlgebraicType.createSumType(
//       Object.entries(variants).map(
//         ([n, c]) => new SumTypeVariant(n, c.__spacetime_type__)
//       )
//     );
//     type JS = { [K in keyof V]: { tag: K; value: Infer<V[K]> } }[keyof V];
//     return col<JS>(ty);
//   },

//   /* ───── scheduling helper ───── */
//   scheduleAt() {
//     /* we model it as a 64‑bit timestamp for now */
//     const b = col<number>(AlgebraicType.createI64Type());
//     b.interval = (isoLike: string) => {
//       b.__meta__.scheduleAt = isoLike; // remember interval
//       return b;
//     };
//     return b as ColumnBuilder<number> & {
//       /** chainable convenience to attach the run‑time interval */
//       interval: (isoLike: string) => typeof b;
//     };
//   },

//   /* ───── generic index‑builder to be used in table options ───── */
//   index<IdxName extends string = string>(opts?: {
//     name?: IdxName;
//   }): {
//     btree<Cols extends readonly string[]>(def: {
//       columns: Cols;
//     }): PendingIndex<(typeof def.columns)[number]>;
//     hash<Cols extends readonly string[]>(def: {
//       columns: Cols;
//     }): PendingIndex<(typeof def.columns)[number]>;
//     direct<Col extends string>(def: { column: Col }): PendingIndex<Col>;
//   } {
//     const common = { name: opts?.name };
//     return {
//       btree<Cols extends readonly string[]>(def: { columns: Cols }) {
//         return {
//           ...common,
//           algorithm: {
//             tag: 'btree',
//             value: { columns: def.columns },
//           },
//         } as PendingIndex<(typeof def.columns)[number]>;
//       },
//       hash<Cols extends readonly string[]>(def: { columns: Cols }) {
//         return {
//           ...common,
//           algorithm: {
//             tag: 'hash',
//             value: { columns: def.columns },
//           },
//         } as PendingIndex<(typeof def.columns)[number]>;
//       },
//       direct<Col extends string>(def: { column: Col }) {
//         return {
//           ...common,
//           algorithm: {
//             tag: 'direct',
//             value: { column: def.column },
//           },
//         } as PendingIndex<Col>;
//       },
//     };
//   },
// } as const;

// /*****************************************************************
//  *  Type helpers
//  *****************************************************************/
// interface ProductTypeBrand {
//   readonly __is_product_type__: true;
// }

// export type ProductTypeColumnBuilder<
//   Def extends Record<string, ColumnBuilder<any>>,
// > = ColumnBuilder<{ [K in keyof Def]: ColumnType<Def[K]> }> &
//   ProductTypeBrand & { __def__: Def };

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
//     | { tag: 'btree'; value: { columns: readonly AllowedCol[] } }
//     | { tag: 'hash'; value: { columns: readonly AllowedCol[] } }
//     | { tag: 'direct'; value: { column: AllowedCol } };
// };

// /*****************************************************************
//  * table()
//  *****************************************************************/
// type TableOpts<
//   N extends string,
//   Def extends Record<string, ColumnBuilder<any>>,
//   Idx extends PendingIndex<keyof Def & string>[] | undefined = undefined,
// > = {
//   name: N;
//   public?: boolean;
//   indexes?: Idx; // declarative multi‑column indexes
//   scheduled?: string; // reducer name for cron‑like tables
// };

// /** Opaque handle that `table()` returns, carrying row & type info for `schema()` */
// type TableHandle<Row> = {
//   readonly __table_name__: string;
//   /** algebraic type for the *row* product type */
//   readonly __row_spacetime_type__: AlgebraicType;
//   /** phantom only: row JS shape */
//   readonly __row_js__?: Row;
// };

// /** Infer the JS row type from a TableHandle */
// type RowOf<H> = H extends TableHandle<infer R> ? R : never;

// export function table<
//   Name extends string,
//   Def extends Record<string, ColumnBuilder<any>>,
//   Row extends ProductTypeColumnBuilder<Def>,
//   Idx extends PendingIndex<keyof Def & string>[] | undefined = undefined,
// >(
//   opts: TableOpts<Name, Def, Idx>,
//   row: Row
// ): TableHandle<Infer<Row>> { 
//   const {
//     name,
//     public: isPublic = false,
//     indexes: userIndexes = [],
//     scheduled,
//   } = opts;

//   /** 1. column catalogue + helpers */
//   const def = row.__def__;
//   const colIds = new Map<keyof Def & string, ColId>();
//   const colIdList: ColList = [];

//   let nextCol: number = 0;
//   for (const colName of Object.keys(def) as (keyof Def & string)[]) {
//     colIds.set(colName, nextCol++);
//     colIdList.push(colIds.get(colName)!);
//   }

//   /** 2. gather primary keys, per‑column indexes, uniques, sequences */
//   const pk: ColList = [];
//   const indexes: RawIndexDefV9[] = [];
//   const constraints: RawConstraintDefV9[] = [];
//   const sequences: RawSequenceDefV9[] = [];

//   let scheduleAtCol: ColId | undefined;

//   for (const [name, builder] of Object.entries(def) as [
//     keyof Def & string,
//     ColumnBuilder<any, any>,
//   ][]) {
//     const meta: any = builder.__meta__;

//     /* primary key */
//     if (meta.primaryKey) pk.push(colIds.get(name)!);

//     /* implicit 1‑column indexes */
//     if (meta.index) {
//       const algo = (meta.index ?? 'btree') as 'btree' | 'hash' | 'direct';
//       const id = colIds.get(name)!;
//       indexes.push(
//         algo === 'direct'
//           ? { algorithm: { tag: 'direct', value: { column: id } } }
//           : { algorithm: { tag: algo, value: { columns: [id] } } }
//       );
//     }

//     /* uniqueness */
//     if (meta.unique) {
//       constraints.push({
//         data: { tag: 'unique', value: { columns: [colIds.get(name)!] } },
//       });
//     }

//     /* auto increment */
//     if (meta.autoInc) {
//       sequences.push({
//         column: colIds.get(name)!,
//         increment: 1,
//       });
//     }

//     /* scheduleAt */
//     if (meta.scheduleAt) scheduleAtCol = colIds.get(name)!;
//   }

//   /** 3. convert explicit multi‑column indexes coming from options.indexes */
//   for (const pending of (userIndexes ?? []) as PendingIndex<
//     keyof Def & string
//   >[]) {
//     const converted: RawIndexDefV9 = {
//       name: pending.name,
//       accessor_name: pending.accessor_name,
//       algorithm: ((): RawIndexAlgorithm => {
//         if (pending.algorithm.tag === 'direct')
//           return {
//             tag: 'direct',
//             value: { column: colIds.get(pending.algorithm.value.column)! },
//           };
//         return {
//           tag: pending.algorithm.tag,
//           value: {
//             columns: pending.algorithm.value.columns.map(c => colIds.get(c)!),
//           },
//         };
//       })(),
//     };
//     indexes.push(converted);
//   }

//   /** 4. add the product type to the global Typespace */
//   const productTypeRef: AlgebraicTypeRef = MODULE_DEF.typespace.types.length;
//   MODULE_DEF.typespace.types.push(row.__spacetime_type__);

//   /** 5. finalise table record */
//   MODULE_DEF.tables.push({
//     name,
//     productTypeRef,
//     primaryKey: pk,
//     indexes,
//     constraints,
//     sequences,
//     schedule:
//       scheduled && scheduleAtCol !== undefined
//         ? {
//             reducerName: scheduled,
//             scheduledAtColumn: scheduleAtCol,
//           }
//         : undefined,
//     tableType: 'user',
//     tableAccess: isPublic ? 'public' : 'private',
//   });

//   // NEW: return a typed handle for schema()
//   return {
//     __table_name__: name,
//     __row_spacetime_type__: row.__spacetime_type__,
//   } as TableHandle<Infer<Row>>;
// }

// /** schema() – consume a record of TableHandles and return a ColumnBuilder
//  *  whose JS type is a map of table -> row shape.
//  */
// export function schema<
//   Def extends Record<string, TableHandle<any>>
// >(def: Def) {
//   // Build a Spacetime product type keyed by table name -> row algebraic type
//   const productTy = AlgebraicType.createProductType(
//     Object.entries(def).map(
//       ([name, handle]) =>
//         new ProductTypeElement(name, (handle as TableHandle<any>).__row_spacetime_type__)
//     )
//   );

//   // The JS-level type: { [table]: RowOf<handle> }
//   type JS = { [K in keyof Def]: RowOf<Def[K]> };

//   // Return a regular ColumnBuilder so you can `Infer<typeof schema>` cleanly
//   return col<JS>(productTy);
// }

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
//   Params extends Record<string, ColumnBuilder<any>> = Record<string, ColumnBuilder<any>>
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

//   const paramType = new ProductType(
//     Object.entries(paramsInternal).map(
//       ([n, c]) => new ProductTypeElement(n, (c as ColumnBuilder<any>).__spacetime_type__)
//     )
//   );

//   MODULE_DEF.reducers.push({
//     name,
//     params: paramType,
//     lifecycle, // <- lifecycle flag lands here
//   });
// }

// /*****************************************************************
//  * reducer() – leave behavior the same; delegate to pushReducer()
//  *****************************************************************/
// /** DB API you want inside reducers */
// type TableApi<Row> = {
//   insert: (row: Row) => void | Promise<void>;
//   // You can add more later: get, update, delete, where, etc.
// };

// /** Reducer context parametrized by the inferred Schema */
// export type ReducerCtx<S> = {
//   db: { [K in keyof S & string]: TableApi<S[K]> };
// };

// // no schema provided -> ctx.db is permissive
// export function reducer<
//   Name extends string = string,
//   Params extends Record<string, ColumnBuilder<any>> = Record<string, ColumnBuilder<any>>,
//   F = (ctx: ReducerCtx<any>, payload: ParamsAsObject<Params>) => void
// >(
//   name: Name,
//   params: Params | ProductTypeColumnBuilder<Params>,
//   fn: F
// ): F;

// // schema provided -> ctx.db is precise
// export function reducer<
//   S,
//   Name extends string = string,
//   Params extends Record<string, ColumnBuilder<any>> = Record<string, ColumnBuilder<any>>,
//   F = (ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void
// >(
//   name: Name,
//   params: Params | ProductTypeColumnBuilder<Params>,
//   fn: F
// ): F;

// // single implementation (S defaults to any -> JS-like)
// export function reducer<
//   S = any,
//   Name extends string = string,
//   Params extends Record<string, ColumnBuilder<any>> = Record<string, ColumnBuilder<any>>,
//   F = (ctx: ReducerCtx<any>, payload: ParamsAsObject<Params>) => void
// >(
//   name: Name,
//   params: Params | ProductTypeColumnBuilder<Params>,
//   fn: F
// ): F {
//   pushReducer<S>(name, params);
//   return fn;
// }

// /*****************************************************************
//  * Lifecycle reducers
//  * - register with lifecycle: 'init' | 'on_connect' | 'on_disconnect'
//  * - keep the same call shape you’re already using
//  *****************************************************************/
// export function init<
//   S = unknown,
//   Params extends Record<string, ColumnBuilder<any>> = {},
// >(
//   name: 'init' = 'init',
//   params: Params | ProductTypeColumnBuilder<Params> = {} as any,
//   _fn?: (ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void
// ): void {
//   pushReducer(name, params, 'init');
// }

// export function clientConnected<
//   S = unknown,
//   Params extends Record<string, ColumnBuilder<any>> = {},
// >(
//   name: 'on_connect' = 'on_connect',
//   params: Params | ProductTypeColumnBuilder<Params> = {} as any,
//   _fn?: (ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void
// ): void {
//   pushReducer(name, params, 'on_connect');
// }

// export function clientDisconnected<
//   S = unknown,
//   Params extends Record<string, ColumnBuilder<any>> = {},
// >(
//   name: 'on_disconnect' = 'on_disconnect',
//   params: Params | ProductTypeColumnBuilder<Params> = {} as any,
//   _fn?: (ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void
// ): void {
//   pushReducer(name, params, 'on_disconnect');
// }

// /*****************************************************************
//  * Example usage
//  *****************************************************************/

// export const point = t.object({
//   x: t.f64(),
//   y: t.f64(),
// });
// type Point = Infer<typeof point>;

// export const user = t.object({
//   id: t.string().primary_key(),
//   name: t.string().index('btree'),
//   email: t.string(),
//   age: t.number(),
// });
// type User = Infer<typeof user>;

// export const player = t.object({
//   id: t.u32().primary_key().auto_inc(),
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
// });

// export const sendMessageSchedule = t.object({
//   scheduleId: t.u64().primary_key(),
//   scheduledAt: t.scheduleAt().interval('1h'),
//   text: t.string(),
// });

// const s = schema({
//   user: table({ name: 'user' }, user),
//   logged_out_user: table({ name: 'logged_out_user' }, user),
//   player: table(
//     {
//       name: 'player',
//       public: true,
//       indexes: [
//         t.index({ name: 'my_index' }).btree({ columns: ['name', 'score'] }),
//       ],
//     },
//     player
//   ),
//   send_message_schedule: table(
//     {
//       name: 'send_message_schedule',
//       scheduled: sendMessage,
//     },
//     sendMessageSchedule
//   )
// });

// export type Schema = Infer<typeof s>;

// export const func = () => {
//   return "asdf";
// }