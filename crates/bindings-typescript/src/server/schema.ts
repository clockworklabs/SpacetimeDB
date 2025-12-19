import {
  type ParamsAsObject,
  type ParamsObj,
  type Reducer,
  type ReducerCtx,
} from '../lib/reducers';
import {
  ModuleContext,
  tablesToSchema,
  type TablesToSchema,
  type UntypedSchemaDef,
} from '../lib/schema';
import type { UntypedTableSchema } from '../lib/table_schema';
import { ColumnBuilder, TypeBuilder } from '../lib/type_builders';
import { procedure, type ProcedureFn, type Procedures } from './procedures';
import {
  clientConnected,
  clientDisconnected,
  init,
  reducer,
  type Reducers,
} from './reducers';

import {
  defineView,
  type AnonViews,
  type AnonymousViewFn,
  type ViewFn,
  type ViewOpts,
  type ViewReturnTypeBuilder,
  type Views,
} from './views';

let REGISTERED_SCHEMA: SchemaInner | null = null;

export function getRegisteredSchema(): SchemaInner {
  if (REGISTERED_SCHEMA == null) {
    throw new Error('Schema has not been registered yet. Call schema() first.');
  }
  return REGISTERED_SCHEMA;
}

export class SchemaInner<
  S extends UntypedSchemaDef = UntypedSchemaDef,
> extends ModuleContext {
  schemaType: S;
  existingFunctions = new Set<string>();
  reducers: Reducers = [];
  procedures: Procedures = [];
  views: Views = [];
  anonViews: AnonViews = [];

  constructor(getSchemaType: (ctx: ModuleContext) => S) {
    super();
    this.schemaType = getSchemaType(this);
  }

  defineFunction(name: string) {
    if (this.existingFunctions.has(name)) {
      throw new TypeError(
        `There is already a reducer or procedure with the name '${name}'`
      );
    }
    this.existingFunctions.add(name);
  }
}

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
  #ctx: SchemaInner<S>;

  constructor(ctx: SchemaInner<S>) {
    // TODO: TableSchema and TableDef should really be unified
    this.#ctx = ctx;
  }

  get schemaType(): S {
    return this.#ctx.schemaType;
  }

  get moduleDef() {
    return this.#ctx.moduleDef;
  }

  get typespace() {
    return this.#ctx.typespace;
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
  reducer<Params extends ParamsObj>(
    name: string,
    params: Params,
    fn: Reducer<S, Params>
  ): Reducer<S, Params>;
  reducer(name: string, fn: Reducer<S, {}>): Reducer<S, {}>;
  reducer<Params extends ParamsObj>(
    name: string,
    paramsOrFn: Params | Reducer<S, any>,
    fn?: Reducer<S, Params>
  ): Reducer<S, Params> {
    if (typeof paramsOrFn === 'function') {
      // This is the case where params are omitted.
      // The second argument is the reducer function.
      // We pass an empty object for the params.
      reducer(this.#ctx, name, {}, paramsOrFn);
      return paramsOrFn;
    } else {
      // This is the case where params are provided.
      // The second argument is the params object, and the third is the function.
      // The `fn` parameter is guaranteed to be defined here.
      reducer(this.#ctx, name, paramsOrFn, fn!);
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
  init(fn: Reducer<S, {}>): void;
  init(name: string, fn: Reducer<S, {}>): void;
  init(nameOrFn: any, maybeFn?: Reducer<S, {}>): void {
    const [name, fn] =
      typeof nameOrFn === 'string' ? [nameOrFn, maybeFn] : ['init', nameOrFn];
    init(this.#ctx, name, {}, fn);
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
  clientConnected(fn: Reducer<S, {}>): void;
  clientConnected(name: string, fn: Reducer<S, {}>): void;
  clientConnected(nameOrFn: any, maybeFn?: Reducer<S, {}>): void {
    const [name, fn] =
      typeof nameOrFn === 'string'
        ? [nameOrFn, maybeFn]
        : ['on_connect', nameOrFn];
    clientConnected(this.#ctx, name, {}, fn);
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
  clientDisconnected(fn: Reducer<S, {}>): void;
  clientDisconnected(name: string, fn: Reducer<S, {}>): void;
  clientDisconnected(nameOrFn: any, maybeFn?: Reducer<S, {}>): void {
    const [name, fn] =
      typeof nameOrFn === 'string'
        ? [nameOrFn, maybeFn]
        : ['on_disconnect', nameOrFn];
    clientDisconnected(this.#ctx, name, {}, fn);
  }

  view<Ret extends ViewReturnTypeBuilder>(
    opts: ViewOpts,
    ret: Ret,
    fn: ViewFn<S, {}, Ret>
  ): void {
    defineView(this.#ctx, opts, false, {}, ret, fn);
  }

  // TODO: re-enable once parameterized views are supported in SQL
  // view<Ret extends ViewReturnTypeBuilder>(
  //   opts: ViewOpts,
  //   ret: Ret,
  //   fn: ViewFn<S, {}, Ret>
  // ): void;
  // view<Params extends ParamsObj, Ret extends ViewReturnTypeBuilder>(
  //   opts: ViewOpts,
  //   params: Params,
  //   ret: Ret,
  //   fn: ViewFn<S, {}, Ret>
  // ): void;
  // view<Params extends ParamsObj, Ret extends ViewReturnTypeBuilder>(
  //   opts: ViewOpts,
  //   paramsOrRet: Ret | Params,
  //   retOrFn: ViewFn<S, {}, Ret> | Ret,
  //   maybeFn?: ViewFn<S, Params, Ret>
  // ): void {
  //   if (typeof retOrFn === 'function') {
  //     defineView(name, false, {}, paramsOrRet as Ret, retOrFn);
  //   } else {
  //     defineView(name, false, paramsOrRet as Params, retOrFn, maybeFn!);
  //   }
  // }

  anonymousView<Ret extends ViewReturnTypeBuilder>(
    opts: ViewOpts,
    ret: Ret,
    fn: AnonymousViewFn<S, {}, Ret>
  ): void {
    defineView(this.#ctx, opts, true, {}, ret, fn);
  }

  // TODO: re-enable once parameterized views are supported in SQL
  // anonymousView<Ret extends ViewReturnTypeBuilder>(
  //   opts: ViewOpts,
  //   ret: Ret,
  //   fn: AnonymousViewFn<S, {}, Ret>
  // ): void;
  // anonymousView<Params extends ParamsObj, Ret extends ViewReturnTypeBuilder>(
  //   opts: ViewOpts,
  //   params: Params,
  //   ret: Ret,
  //   fn: AnonymousViewFn<S, {}, Ret>
  // ): void;
  // anonymousView<Params extends ParamsObj, Ret extends ViewReturnTypeBuilder>(
  //   opts: ViewOpts,
  //   paramsOrRet: Ret | Params,
  //   retOrFn: AnonymousViewFn<S, {}, Ret> | Ret,
  //   maybeFn?: AnonymousViewFn<S, Params, Ret>
  // ): void {
  //   if (typeof retOrFn === 'function') {
  //     defineView(name, true, {}, paramsOrRet as Ret, retOrFn);
  //   } else {
  //     defineView(name, true, paramsOrRet as Params, retOrFn, maybeFn!);
  //   }
  // }

  procedure<Params extends ParamsObj, Ret extends TypeBuilder<any, any>>(
    name: string,
    params: Params,
    ret: Ret,
    fn: ProcedureFn<S, Params, Ret>
  ): ProcedureFn<S, Params, Ret>;
  procedure<Ret extends TypeBuilder<any, any>>(
    name: string,
    ret: Ret,
    fn: ProcedureFn<S, {}, Ret>
  ): ProcedureFn<S, {}, Ret>;
  procedure<Params extends ParamsObj, Ret extends TypeBuilder<any, any>>(
    name: string,
    paramsOrRet: Ret | Params,
    retOrFn: ProcedureFn<S, {}, Ret> | Ret,
    maybeFn?: ProcedureFn<S, Params, Ret>
  ): ProcedureFn<S, Params, Ret> {
    if (typeof retOrFn === 'function') {
      procedure(this.#ctx, name, {}, paramsOrRet as Ret, retOrFn);
      return retOrFn;
    } else {
      procedure(this.#ctx, name, paramsOrRet as Params, retOrFn, maybeFn!);
      return maybeFn!;
    }
  }

  clientVisibilityFilter = {
    sql: (filter: string) => {
      this.#ctx.moduleDef.rowLevelSecurity.push({ sql: filter });
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
export function schema<const H extends readonly UntypedTableSchema[]>(
  ...handles: H
): Schema<TablesToSchema<H>>;

/**
 * Creates a schema from table definitions (array overload)
 * @param handles - Array of table handles created by table() function
 * @returns ColumnBuilder representing the complete database schema
 */
export function schema<const H extends readonly UntypedTableSchema[]>(
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
export function schema<const H extends readonly UntypedTableSchema[]>(
  ...args: [H] | H
): Schema<TablesToSchema<H>> {
  const handles = (
    args.length === 1 && Array.isArray(args[0]) ? args[0] : args
  ) as H;

  const ctx = new SchemaInner(ctx => {
    const tableDefs = handles.map(h => h.tableDef(ctx));
    ctx.moduleDef.tables.push(...tableDefs);

    return tablesToSchema(ctx, handles);
  });

  REGISTERED_SCHEMA = ctx;

  return new Schema(ctx);
}
