import { moduleHooks, type ModuleDefaultExport } from 'spacetime:sys@2.0';
import { CaseConversionPolicy, Lifecycle } from '../lib/autogen/types';
import {
  type ParamsAsObject,
  type ParamsObj,
  type Reducer,
  type ReducerCtx,
} from '../lib/reducers';
import {
  ModuleContext,
  tableToSchema,
  type TablesToSchema,
  type UntypedSchemaDef,
} from '../lib/schema';
import type { UntypedTableSchema } from '../lib/table_schema';
import { ColumnBuilder, TypeBuilder } from '../lib/type_builders';
import {
  makeProcedureExport,
  type ProcedureExport,
  type ProcedureFn,
  type ProcedureOpts,
  type Procedures,
} from './procedures';
import {
  makeReducerExport,
  type ReducerExport,
  type ReducerOpts,
  type Reducers,
} from './reducers';
import { makeHooks } from './runtime';

import {
  makeAnonViewExport,
  makeViewExport,
  type AnonViews,
  type AnonymousViewFn,
  type ViewExport,
  type ViewFn,
  type ViewOpts,
  type ViewReturnTypeBuilder,
  type Views,
} from './views';
import type { UntypedTableDef } from '../lib/table';

export class SchemaInner<
  S extends UntypedSchemaDef = UntypedSchemaDef,
> extends ModuleContext {
  schemaType: S;
  existingFunctions = new Set<string>();
  reducers: Reducers = [];
  procedures: Procedures = [];
  views: Views = [];
  anonViews: AnonViews = [];
  /**
   * Maps ReducerExport objects to the name of the reducer.
   * Used for resolving the reducers of scheduled tables.
   */
  functionExports: Map<
    | ReducerExport<UntypedSchemaDef, any>
    | ProcedureExport<UntypedSchemaDef, any, any>,
    string
  > = new Map();
  pendingSchedules: PendingSchedule[] = [];

  constructor(getSchemaType: (ctx: SchemaInner<S>) => S) {
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

  resolveSchedules() {
    for (const { reducer, scheduleAtCol, tableName } of this.pendingSchedules) {
      const functionName = this.functionExports.get(reducer());
      if (functionName === undefined) {
        const msg = `Table ${tableName} defines a schedule, but it seems like the associated function was not exported.`;
        throw new TypeError(msg);
      }
      this.moduleDef.schedules.push({
        sourceName: undefined,
        tableName,
        scheduleAtCol,
        functionName,
      });
    }
  }
}

type PendingSchedule = UntypedTableSchema['schedule'] & { tableName: string };

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
 * const spacetimedb = schema({
 *   user: table({}, userType),
 *   post: table({}, postType)
 * });
 * spacetimedb.reducer(
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
export class Schema<S extends UntypedSchemaDef> implements ModuleDefaultExport {
  #ctx: SchemaInner<S>;

  constructor(ctx: SchemaInner<S>) {
    // TODO: TableSchema and TableDef should really be unified
    this.#ctx = ctx;
  }

  [moduleHooks](exports: object) {
    // if (!(hasOwn(exports, 'default') && exports.default instanceof Schema)) {
    //   throw new TypeError('must export schema as default export');
    // }
    const registeredSchema = this.#ctx;
    for (const [name, moduleExport] of Object.entries(exports)) {
      if (name === 'default') continue;
      if (!isModuleExport(moduleExport)) {
        throw new TypeError(
          'exporting something that is not a spacetime export'
        );
      }
      checkExportContext(moduleExport, registeredSchema);
      moduleExport[registerExport](registeredSchema, name);
    }
    registeredSchema.resolveSchedules();
    return makeHooks(registeredSchema);
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
   * export const create_user = spacetime.reducer(
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
    params: Params,
    fn: Reducer<S, Params>
  ): ReducerExport<S, Params>;
  reducer(fn: Reducer<S, {}>): ReducerExport<S, {}>;
  reducer<Params extends ParamsObj>(
    opts: ReducerOpts,
    params: Params,
    fn: Reducer<S, Params>
  ): ReducerExport<S, Params>;
  reducer(opts: ReducerOpts, fn: Reducer<S, {}>): ReducerExport<S, {}>;
  reducer<Params extends ParamsObj>(
    ...args:
      | [Params, Reducer<S, Params>]
      | [Reducer<S, {}>]
      | [ReducerOpts, Params, Reducer<S, Params>]
      | [ReducerOpts, Reducer<S, {}>]
  ): ReducerExport<S, Params> {
    let opts: ReducerOpts | undefined,
      params: Params = {} as Params,
      fn: Reducer<S, Params>;
    switch (args.length) {
      case 1:
        [fn] = args;
        break;
      case 2: {
        let arg1;
        [arg1, fn] = args;
        if (typeof arg1.name === 'string') opts = arg1 as ReducerOpts;
        else params = arg1 as Params;
        break;
      }
      case 3:
        [opts, params, fn] = args;
        break;
    }
    return makeReducerExport(this.#ctx, opts, params, fn);
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
   * export const init = spacetime.init((ctx) => {
   *   ctx.db.user.insert({ username: 'admin', email: 'admin@example.com' });
   * });
   * ```
   */
  init(fn: Reducer<S, {}>): ReducerExport<S, {}>;
  init(opts: ReducerOpts, fn: Reducer<S, {}>): ReducerExport<S, {}>;
  init(
    ...args: [Reducer<S, {}>] | [ReducerOpts, Reducer<S, {}>]
  ): ReducerExport<S, {}> {
    let opts: ReducerOpts | undefined, fn: Reducer<S, {}>;
    switch (args.length) {
      case 1:
        [fn] = args;
        break;
      case 2:
        [opts, fn] = args;
        break;
    }
    return makeReducerExport(this.#ctx, opts, {}, fn, Lifecycle.Init);
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
   * export const onConnect = spacetime.clientConnected(
   *   (ctx) => {
   *     console.log(`Client ${ctx.connectionId} connected`);
   *   }
   * );
   */
  clientConnected(fn: Reducer<S, {}>): ReducerExport<S, {}>;
  clientConnected(opts: ReducerOpts, fn: Reducer<S, {}>): ReducerExport<S, {}>;
  clientConnected(
    ...args: [Reducer<S, {}>] | [ReducerOpts, Reducer<S, {}>]
  ): ReducerExport<S, {}> {
    let opts: ReducerOpts | undefined, fn: Reducer<S, {}>;
    switch (args.length) {
      case 1:
        [fn] = args;
        break;
      case 2:
        [opts, fn] = args;
        break;
    }
    return makeReducerExport(this.#ctx, opts, {}, fn, Lifecycle.OnConnect);
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
   * export const onDisconnect = spacetime.clientDisconnected(
   *   (ctx) => {
   *     console.log(`Client ${ctx.connectionId} disconnected`);
   *   }
   * );
   * ```
   */
  clientDisconnected(fn: Reducer<S, {}>): ReducerExport<S, {}>;
  clientDisconnected(
    opts: ReducerOpts,
    fn: Reducer<S, {}>
  ): ReducerExport<S, {}>;
  clientDisconnected(
    ...args: [Reducer<S, {}>] | [ReducerOpts, Reducer<S, {}>]
  ): ReducerExport<S, {}> {
    let opts: ReducerOpts | undefined, fn: Reducer<S, {}>;
    switch (args.length) {
      case 1:
        [fn] = args;
        break;
      case 2:
        [opts, fn] = args;
        break;
    }
    return makeReducerExport(this.#ctx, opts, {}, fn, Lifecycle.OnDisconnect);
  }

  view<Ret extends ViewReturnTypeBuilder, F extends ViewFn<S, {}, Ret>>(
    opts: ViewOpts,
    ret: Ret,
    fn: F
  ): ViewExport<F> {
    return makeViewExport<S, {}, Ret, F>(this.#ctx, opts, {}, ret, fn);
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

  anonymousView<
    Ret extends ViewReturnTypeBuilder,
    F extends AnonymousViewFn<S, {}, Ret>,
  >(opts: ViewOpts, ret: Ret, fn: F): ViewExport<F> {
    return makeAnonViewExport<S, {}, Ret, F>(this.#ctx, opts, {}, ret, fn);
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
    params: Params,
    ret: Ret,
    fn: ProcedureFn<S, Params, Ret>
  ): ProcedureFn<S, Params, Ret>;
  procedure<Ret extends TypeBuilder<any, any>>(
    ret: Ret,
    fn: ProcedureFn<S, {}, Ret>
  ): ProcedureFn<S, {}, Ret>;
  procedure<Params extends ParamsObj, Ret extends TypeBuilder<any, any>>(
    opts: ProcedureOpts,
    params: Params,
    ret: Ret,
    fn: ProcedureFn<S, Params, Ret>
  ): ProcedureFn<S, Params, Ret>;
  procedure<Ret extends TypeBuilder<any, any>>(
    opts: ProcedureOpts,
    ret: Ret,
    fn: ProcedureFn<S, {}, Ret>
  ): ProcedureFn<S, {}, Ret>;
  procedure<Params extends ParamsObj, Ret extends TypeBuilder<any, any>>(
    ...args:
      | [Params, Ret, ProcedureFn<S, Params, Ret>]
      | [Ret, ProcedureFn<S, Params, Ret>]
      | [ProcedureOpts, Params, Ret, ProcedureFn<S, Params, Ret>]
      | [ProcedureOpts, Ret, ProcedureFn<S, Params, Ret>]
  ): ProcedureExport<S, Params, Ret> {
    let opts: ProcedureOpts | undefined,
      params: Params = {} as Params,
      ret: Ret,
      fn: ProcedureFn<S, Params, Ret>;
    switch (args.length) {
      case 2:
        [ret, fn] = args;
        break;
      case 3: {
        let arg1;
        [arg1, ret, fn] = args;
        if (typeof arg1.name === 'string') opts = arg1 as ProcedureOpts;
        else params = arg1 as Params;
        break;
      }
      case 4:
        [opts, params, ret, fn] = args;
        break;
    }
    return makeProcedureExport(this.#ctx, opts, params, ret, fn);
  }

  /**
   * Bundle multiple reducers, procedures, etc into one value to export.
   * The name they will be exported with is their corresponding key in the `exports` argument.
   */
  exportGroup(exports: Record<string, ModuleExport>): ModuleExport {
    return {
      [exportContext]: this.#ctx,
      [registerExport](ctx, _exportName) {
        for (const [exportName, moduleExport] of Object.entries(exports)) {
          checkExportContext(moduleExport, ctx);
          moduleExport[registerExport](ctx, exportName);
        }
      },
    };
  }

  clientVisibilityFilter = {
    sql: (filter: string): ModuleExport => ({
      [exportContext]: this.#ctx,
      [registerExport](ctx, _exportName) {
        ctx.moduleDef.rowLevelSecurity.push({ sql: filter });
      },
    }),
  };
}

export const registerExport = Symbol('SpacetimeDB.registerExport');
export const exportContext = Symbol('SpacetimeDB.exportContext');

export interface ModuleExport {
  [registerExport](ctx: SchemaInner, exportName: string): void;
  [exportContext]?: SchemaInner;
}

function isModuleExport(x: unknown): x is ModuleExport {
  return (
    (typeof x === 'function' || typeof x === 'object') &&
    x !== null &&
    registerExport in x
  );
}

/** Verify that the ModuleContext that `exp` comes from is the same as `schema` */
function checkExportContext(exp: ModuleExport, schema: SchemaInner) {
  if (exp[exportContext] != null && exp[exportContext] !== schema) {
    throw new TypeError('multiple schemas are not supported');
  }
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
 * const spacetimedb = schema({
 *   user: table({}, userType),
 *   post: table({}, postType)
 * });
 * ```
 */
/**
 * Module-level settings that can be passed to `schema()`.
 */
export interface ModuleSettings {
  /**
   * The case conversion policy for this module.
   * Defaults to `SnakeCase` if not specified.
   *
   * @example
   * ```ts
   * export default schema({
   *   player,
   * }, { CASE_CONVERSION_POLICY: CaseConversionPolicy.None });
   * ```
   */
  CASE_CONVERSION_POLICY?: CaseConversionPolicy;
}

export function schema<const H extends Record<string, UntypedTableSchema>>(
  tables: H,
  moduleSettings?: ModuleSettings
): Schema<TablesToSchema<H>> {
  const ctx = new SchemaInner<TablesToSchema<H>>(ctx => {
    // Apply module settings.
    if (moduleSettings?.CASE_CONVERSION_POLICY != null) {
      ctx.setCaseConversionPolicy(moduleSettings.CASE_CONVERSION_POLICY);
    }

    const tableSchemas: Record<string, UntypedTableDef> = {};
    for (const [accName, table] of Object.entries(tables)) {
      const tableDef = table.tableDef(ctx, accName);
      tableSchemas[accName] = tableToSchema(accName, table, tableDef);
      ctx.moduleDef.tables.push(tableDef);
      if (table.schedule) {
        ctx.pendingSchedules.push({
          ...table.schedule,
          tableName: tableDef.sourceName,
        });
      }
      if (table.tableName) {
        ctx.moduleDef.explicitNames.entries.push({
          tag: 'Table',
          value: {
            sourceName: accName,
            canonicalName: table.tableName,
          },
        });
      }
    }
    return { tables: tableSchemas } as TablesToSchema<H>;
  });

  return new Schema(ctx);
}
