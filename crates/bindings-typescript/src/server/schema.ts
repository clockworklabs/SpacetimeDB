import type RawTableDefV9 from '../lib/autogen/raw_table_def_v_9_type';
import type Typespace from '../lib/autogen/typespace_type';
import {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  type ColumnBuilder,
  type RowObj,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  type TypeBuilder,
} from './type_builders';
import type { TableSchema, UntypedTableDef } from './table';
import {
  clientConnected,
  clientDisconnected,
  init,
  reducer,
  type ParamsObj,
  type Reducer,
} from './reducers';
import type RawModuleDefV9 from '../lib/autogen/raw_module_def_v_9_type';
import {
  AlgebraicType,
  type AlgebraicTypeVariants,
} from '../lib/algebraic_type';
import type RawScopedTypeNameV9 from '../lib/autogen/raw_scoped_type_name_v_9_type';

/**
 * The global module definition that gets populated by calls to `reducer()` and lifecycle hooks.
 */
export const MODULE_DEF: RawModuleDefV9 = {
  typespace: { types: [] },
  tables: [],
  reducers: [],
  types: [],
  miscExports: [],
  rowLevelSecurity: [],
};

const COMPOUND_TYPES = new Map<
  AlgebraicTypeVariants.Product | AlgebraicTypeVariants.Sum,
  AlgebraicTypeVariants.Ref
>();

export function addType<T extends AlgebraicType>(
  name: string | undefined,
  ty: T
): T | AlgebraicTypeVariants.Ref {
  if (
    (ty.tag === 'Product' && (ty.value.elements.length > 0 || name != null)) ||
    (ty.tag === 'Sum' && (ty.value.variants.length > 0 || name != null))
  ) {
    let r = COMPOUND_TYPES.get(ty);
    if (r == null) {
      r = AlgebraicType.Ref(MODULE_DEF.typespace.types.length);
      MODULE_DEF.typespace.types.push(ty);
      COMPOUND_TYPES.set(ty, r);
      if (name != null)
        MODULE_DEF.types.push({
          name: splitName(name),
          ty: r.value,
          customOrdering: true,
        });
    }
    return r;
  } else {
    return ty;
  }
}

export function splitName(name: string): RawScopedTypeNameV9 {
  const scope = name.split('.');
  return { name: scope.pop()!, scope };
}

/**
 * An untyped representation of the database schema.
 */
export type UntypedSchemaDef = {
  tables: readonly UntypedTableDef[];
};

/**
 * Helper type to convert an array of TableSchema into a schema definition
 */
type TablesToSchema<T extends readonly TableSchema<any, any, any>[]> = {
  tables: {
    /** @type {UntypedTableDef} */
    readonly [i in keyof T]: {
      name: T[i]['tableName'];
      columns: T[i]['rowType']['row'];
      indexes: T[i]['idxs'];
    };
  };
};

/**
 * The Schema class represents the database schema for a SpacetimeDB application.
 * It encapsulates the table definitions and typespace, and provides methods to define
 * reducers and lifecycle hooks.
 *
 * Schema has a generic parameter S which represents the inferred schema type. This type
 * is automatically inferred when creating a schema using the `schema()` function and is
 * used to type the database view in reducer contexts.
 *
 * The methods on this class are used to register reducers and lifecycle hooks
 * with the SpacetimeDB runtime. Theey forward to free functions that handle the actual
 * registration logic, but having them as methods on the Schema class helps with type inference.
 *
 * @template S - The inferred schema type of the SpacetimeDB module.
 *
 * @example
 * ```typescript
 * const spacetime = schema(
 *   table({ name: 'user' }, userType),
 *   table({ name: 'post' }, postType)
 * );
 * spacetime.reducer(
 *   'create_user',
 *   {  username: t.string(), email: t.string() },
 *   (ctx, { username, email }) => {
 *     ctx.db.user.insert({ username, email, created_at: ctx.timestamp });
 *     console.log(`User ${username} created by ${ctx.sender.identityId}`);
 *   }
 * );
 * ```
 */
// TODO(cloutiertyler): It might be nice to have a way to access the types
// for the tables from the schema object, e.g. `spacetimedb.user.type` would
// be the type of the user table.
class Schema<S extends UntypedSchemaDef> {
  readonly tablesDef: { tables: RawTableDefV9[] };
  readonly typespace: Typespace;
  readonly schemaType!: S;

  constructor(tables: RawTableDefV9[], typespace: Typespace) {
    this.tablesDef = { tables };
    this.typespace = typespace;
  }

  /**
   * Defines a SpacetimeDB reducer function.
   *
   * Reducers are the primary way to modify the state of your SpacetimeDB application.
   * They are atomic, meaning that either all operations within a reducer succeed,
   * or none of them do.
   *
   * @template S - The inferred schema type of the SpacetimeDB module.
   * @template Params - The type of the parameters object expected by the reducer.
   *
   * @param {string} name - The name of the reducer. This name will be used to call the reducer from clients.
   * @param {Params} params - An object defining the parameters that the reducer accepts.
   *                          Each key-value pair represents a parameter name and its corresponding
   *                          {@link TypeBuilder} or {@link ColumnBuilder}.
   * @param {(ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void} fn - The reducer function itself.
   *   - `ctx`: The reducer context, providing access to `sender`, `timestamp`, `connection_id`, and `db`.
   *   - `payload`: An object containing the arguments passed to the reducer, typed according to `params`.
   *
   * @example
   * ```typescript
   * // Define a reducer named 'create_user' that takes 'username' (string) and 'email' (string)
   * spacetime.reducer(
   *   'create_user',
   *   {
   *     username: t.string(),
   *     email: t.string(),
   *   },
   *   (ctx, { username, email }) => {
   *     // Access the 'user' table from the database view in the context
   *     ctx.db.user.insert({ username, email, created_at: ctx.timestamp });
   *     console.log(`User ${username} created by ${ctx.sender.identityId}`);
   *   }
   * );
   * ```
   */
  reducer<Params extends ParamsObj | RowObj>(
    name: string,
    params: Params,
    fn: Reducer<S, Params>
  ): Reducer<S, Params>;
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  reducer(name: string, fn: Reducer<S, {}>): Reducer<S, {}>;
  reducer<Params extends ParamsObj | RowObj>(
    name: string,
    paramsOrFn: Params | Reducer<S, any>,
    fn?: Reducer<S, Params>
  ): Reducer<S, Params> {
    if (typeof paramsOrFn === 'function') {
      // This is the case where params are omitted.
      // The second argument is the reducer function.
      // We pass an empty object for the params.
      reducer(name, {}, paramsOrFn);
      return paramsOrFn;
    } else {
      // This is the case where params are provided.
      // The second argument is the params object, and the third is the function.
      // The `fn` parameter is guaranteed to be defined here.
      reducer(name, paramsOrFn, fn!);
      return fn!;
    }
  }

  /**
   * Registers an initialization reducer that runs when the SpacetimeDB module is published
   * for the first time.
   *
   * This function is useful to set up any initial state of your database that is guaranteed
   * to run only once, and before any other reducers or client connections.
   *
   * @template S - The inferred schema type of the SpacetimeDB module.
   * @param {Reducer<S, {}>} fn - The initialization reducer function.
   *  - `ctx`: The reducer context, providing access to `sender`, `timestamp`, `connection_id`, and `db`.
   * @example
   * ```typescript
   * spacetime.init((ctx) => {
   *   ctx.db.user.insert({ username: 'admin', email: 'admin@example.com' });
   * });
   * ```
   */
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  init(fn: Reducer<S, {}>): void;
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  init(name: string, fn: Reducer<S, {}>): void;
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  init(nameOrFn: any, maybeFn?: Reducer<S, {}>): void {
    const [name, fn] =
      typeof nameOrFn === 'string' ? [nameOrFn, maybeFn] : ['init', nameOrFn];
    init(name, {}, fn);
  }

  /**
   * Registers a reducer to be called when a client connects to the SpacetimeDB module.
   * This function allows you to define custom logic that should execute
   * whenever a new client establishes a connection.
   * @template S - The inferred schema type of the SpacetimeDB module.
   *
   * @param fn - The reducer function to execute on client connection.
   *
   * @example
   * ```typescript
   * spacetime.clientConnected(
   *   (ctx) => {
   *     console.log(`Client ${ctx.connectionId} connected`);
   *   }
   * );
   */
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  clientConnected(fn: Reducer<S, {}>): void;
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  clientConnected(name: string, fn: Reducer<S, {}>): void;
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  clientConnected(nameOrFn: any, maybeFn?: Reducer<S, {}>): void {
    const [name, fn] =
      typeof nameOrFn === 'string'
        ? [nameOrFn, maybeFn]
        : ['on_connect', nameOrFn];
    clientConnected(name, {}, fn);
  }

  /**
   * Registers a reducer to be called when a client disconnects from the SpacetimeDB module.
   * This function allows you to define custom logic that should execute
   * whenever a client disconnects.
   * @template S - The inferred schema type of the SpacetimeDB module.
   *
   * @param fn - The reducer function to execute on client disconnection.
   *
   * @example
   * ```typescript
   * spacetime.clientDisconnected(
   *   (ctx) => {
   *     console.log(`Client ${ctx.connectionId} disconnected`);
   *   }
   * );
   * ```
   */
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  clientDisconnected(fn: Reducer<S, {}>): void;
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  clientDisconnected(name: string, fn: Reducer<S, {}>): void;
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  clientDisconnected(nameOrFn: any, maybeFn?: Reducer<S, {}>): void {
    const [name, fn] =
      typeof nameOrFn === 'string'
        ? [nameOrFn, maybeFn]
        : ['on_disconnect', nameOrFn];
    clientDisconnected(name, {}, fn);
  }

  clientVisibilityFilter = {
    sql(filter: string): void {
      MODULE_DEF.rowLevelSecurity.push({ sql: filter });
    },
  };
}

/**
 * Extracts the inferred schema type from a Schema instance
 */
export type InferSchema<SchemaDef extends Schema<any>> =
  SchemaDef extends Schema<infer S> ? S : never;

/**
 * Creates a schema from table definitions
 * @param handles - Array of table handles created by table() function
 * @returns ColumnBuilder representing the complete database schema
 * @example
 * ```ts
 * const s = schema(
 *   table({ name: 'user' }, userType),
 *   table({ name: 'post' }, postType)
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
 *   table({ name: 'user' }, userType),
 *   table({ name: 'post' }, postType)
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

/**
 * Creates a schema from table definitions
 * @param args - Either an array of table handles or a variadic list of table handles
 * @returns ColumnBuilder representing the complete database schema
 * @example
 * ```ts
 * const s = schema(
 *  table({ name: 'user' }, userType),
 *  table({ name: 'post' }, postType)
 * );
 * ```
 */
export function schema(
  ...args:
    | [readonly TableSchema<any, any, any>[]]
    | readonly TableSchema<any, any, any>[]
): Schema<UntypedSchemaDef> {
  const handles: readonly TableSchema<any, any, any>[] =
    args.length === 1 && Array.isArray(args[0]) ? args[0] : args;

  const tableDefs = handles.map(h => h.tableDef);

  // Side-effect:
  // Modify the `MODULE_DEF` which will be read by
  // __describe_module__
  MODULE_DEF.tables.push(...tableDefs);
  // MODULE_DEF.typespace = typespace;
  // throw new Error(
  //   MODULE_DEF.tables
  //     .map(t => {
  //       const p = MODULE_DEF.typespace.types[t.productTypeRef];
  //       return `${t.name}: ${t.productTypeRef} ${p && (p as AlgebraicTypeVariants.Product).value.elements.map(x => x.name)}`;
  //     })
  //     .join('\n')
  // );

  return new Schema(tableDefs, MODULE_DEF.typespace);
}
