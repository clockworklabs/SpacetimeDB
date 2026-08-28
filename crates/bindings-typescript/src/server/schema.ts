import { moduleHooks, type ModuleDefaultExport } from 'spacetime:sys@2.0';
import {
  CaseConversionPolicy,
  Lifecycle,
  type MethodOrAny,
  type RawModuleDefV10,
  type RawProcedureDefV10,
  type RawReducerDefV10,
  type RawTableDefV10,
  type Typespace,
} from '../lib/autogen/types';
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
import { TypeBuilder, type ColumnBuilder } from '../lib/type_builders';
import { hasOwn } from '../lib/util';
import {
  Router,
  type HandlerFn,
  type HttpHandlerExport,
  type HttpHandlerOpts,
  makeHttpHandlerExport,
  makeHttpRouterExport,
} from './http_handlers';
import {
  makeProcedureExport,
  type ProcedureExport,
  type ProcedureFn,
  type ProcedureOpts,
  type ProcedureOptsWithOptionalName,
  type Procedures,
} from './procedures';
import {
  makeReducerExport,
  type ReducerExport,
  type ReducerOpts,
  type ReducerOptsWithOptionalName,
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
  type ValidateViewPrimaryKey,
  type Views,
} from './views';
import type { UntypedTableDef } from '../lib/table';

/**
 * Internal erased form of a scheduled reducer/procedure export.
 *
 * Public reducer/procedure schedule options preserve row/return-type checks
 * before values enter `pendingSchedules`. Legacy table schedules still resolve
 * through export object identity.
 */
type UntypedScheduledFunctionExport =
  | ReducerExport<any, any>
  | ProcedureExport<any, any, any>;

export type SubmoduleDispatchInfo = {
  namespace: string;
  reducerFns: Reducers;
  reducerDefs: RawReducerDefV10[];
  procedureFns: Procedures;
  procedureDefs: RawProcedureDefV10[];
  anonViewFns: AnonViews;
  viewFns: Views;
  typespace: Typespace;
  tables: Array<{ accessorName: string; tableDef: RawTableDefV10 }>;
  /** The submodule's own schemaType tables, used to build namespace-scoped query builders. */
  schemaTables: Record<string, UntypedTableDef>;
  subDispatches: SubmoduleDispatchInfo[];
};

export class SchemaInner<
  S extends UntypedSchemaDef = UntypedSchemaDef,
> extends ModuleContext {
  schemaType: S;
  exportsRegistered = false;
  schedulesResolved = false;
  existingFunctions = new Set<string>();
  existingHttpHandlers = new Set<string>();
  reducers: Reducers = [];
  procedures: Procedures = [];
  views: Views = [];
  anonViews: AnonViews = [];
  httpHandlers: HandlerFn[] = [];
  /**
   * Maps reducer/procedure export objects to their source names.
   * Used for resolving scheduled table targets.
   */
  functionExports: Map<UntypedScheduledFunctionExport, string> = new Map();
  tableSourceNames: Map<UntypedTableSchema, string[]> = new Map();
  httpHandlerExports: Map<HttpHandlerExport<UntypedSchemaDef>, string> =
    new Map();
  pendingSchedules: PendingSchedule[] = [];
  pendingHttpRoutes: PendingHttpRoute[] = [];
  submoduleDispatchInfos: SubmoduleDispatchInfo[] = [];

  constructor(getSchemaType: (ctx: SchemaInner<S>) => S) {
    super();
    this.schemaType = getSchemaType(this);
  }

  defineFunction(name: string) {
    if (this.existingFunctions.has(name)) {
      throw new TypeError(
        `There is already a reducer, procedure, or view with the name '${name}'`
      );
    }
    this.existingFunctions.add(name);
  }

  defineHttpHandler(name: string) {
    if (this.existingHttpHandlers.has(name)) {
      throw new TypeError(
        `There is already an HTTP handler with the name '${name}'`
      );
    }
    this.existingHttpHandlers.add(name);
  }

  resolveSchedules() {
    if (this.schedulesResolved) {
      return;
    }
    this.schedulesResolved = true;
    // Pending schedules come from two API paths:
    // - legacy table({ scheduled }) schedules already know their tableName and
    //   scheduleAtCol because schema() resolves them while iterating table keys
    // - reducer/procedure({ onSchedule }) schedules only know the table handle,
    //   so resolve them through tableSourceNames and reject duplicate table
    //   handle registrations because the target table name is ambiguous
    const scheduledTables = new Map<string, string>();
    for (const {
      functionName: knownFunctionName,
      reducer,
      table,
      tableName: knownTableName,
      scheduleAtCol: knownScheduleAtCol,
    } of this.pendingSchedules) {
      let tableName = knownTableName;
      if (tableName === undefined) {
        const tableNames = this.tableSourceNames.get(table);
        if (tableNames !== undefined && tableNames.length > 1) {
          throw new TypeError(
            'Schedule target table is registered more than once in this schema. Use a distinct table handle for each scheduled table.'
          );
        }
        tableName = tableNames?.[0];
      }
      if (tableName === undefined) {
        throw new TypeError(
          'Schedule target table is not part of this schema.'
        );
      }

      const scheduleAtCol = knownScheduleAtCol ?? table.scheduleAtCol;
      if (scheduleAtCol === undefined) {
        throw new TypeError(
          `Table ${tableName} defines a schedule, but it does not have a ScheduleAt column.`
        );
      }

      const functionName =
        knownFunctionName ??
        (reducer === undefined
          ? undefined
          : this.functionExports.get(reducer()));
      if (functionName === undefined) {
        const msg = `Table ${tableName} defines a schedule, but it seems like the associated function was not exported.`;
        throw new TypeError(msg);
      }
      const existingFunctionName = scheduledTables.get(tableName);
      if (existingFunctionName !== undefined) {
        throw new TypeError(
          `Table ${tableName} defines multiple schedules: ${existingFunctionName} and ${functionName}. A schedule table can only be used by one reducer or procedure.`
        );
      }
      scheduledTables.set(tableName, functionName);
      this.moduleDef.schedules.push({
        sourceName: undefined,
        tableName,
        scheduleAtCol,
        functionName,
      });
    }
  }

  resolveHttpRoutes() {
    for (const route of this.pendingHttpRoutes) {
      const handlerFunction = this.httpHandlerExports.get(route.handler);
      if (handlerFunction === undefined) {
        throw new TypeError(
          `HTTP route for path '${route.path}' refers to a handler that was not exported.`
        );
      }
      this.moduleDef.httpRoutes.push({
        handlerFunction,
        method: route.method,
        path: route.path,
      });
    }
  }
}

type PendingSchedule = {
  table: UntypedTableSchema;
  tableName?: string;
  scheduleAtCol?: number;
  reducer?: () => UntypedScheduledFunctionExport;
  functionName?: string;
};
type PendingHttpRoute = {
  handler: HttpHandlerExport<UntypedSchemaDef>;
  method: MethodOrAny;
  path: string;
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
    this.buildRawModuleDefV10(exports);
    this.#ctx.resolveHttpRoutes();
    return makeHooks(this.#ctx);
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

  get submoduleDispatchInfos(): SubmoduleDispatchInfo[] {
    return this.#ctx.submoduleDispatchInfos;
  }

  /** Internal: register exports and materialize the RawModuleDefV10 for upload. */
  buildRawModuleDefV10(
    exports: object,
    opts?: { ignoreNonModuleExports?: boolean }
  ): RawModuleDefV10 {
    registerModuleExports(this.#ctx, exports, {
      ignoreNonModuleExports: opts?.ignoreNonModuleExports ?? false,
    });
    this.#ctx.resolveSchedules();
    return this.#ctx.rawModuleDefV10();
  }

  /**
   * @internal – called by schema() when processing a submodule namespace entry.
   * Registers the library's exports and returns both the serialized module def
   * and the runtime dispatch info needed by ModuleHooksImpl for __call_reducer__.
   */
  buildSubmoduleDispatch(exports: object): {
    rawDef: RawModuleDefV10;
    dispatch: SubmoduleDispatchInfo;
  } {
    const rawDef = this.buildRawModuleDefV10(exports, {
      ignoreNonModuleExports: true,
    });
    this.#ctx.resolveHttpRoutes();
    return {
      rawDef,
      dispatch: {
        namespace: '',
        reducerFns: [...this.#ctx.reducers],
        reducerDefs: [...this.#ctx.moduleDef.reducers],
        procedureFns: [...this.#ctx.procedures],
        procedureDefs: [...this.#ctx.moduleDef.procedures],
        anonViewFns: [...this.#ctx.anonViews],
        viewFns: [...this.#ctx.views],
        typespace: this.#ctx.moduleDef.typespace,
        tables: Object.values(this.#ctx.schemaType.tables).map(t => ({
          accessorName: t.accessorName,
          tableDef: t.tableDef,
        })),
        schemaTables: this.#ctx.schemaType.tables,
        subDispatches: [...this.#ctx.submoduleDispatchInfos],
      },
    };
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
    opts: ReducerOptsWithOptionalName<Params>,
    params: Params,
    fn: Reducer<S, Params>
  ): ReducerExport<S, Params>;
  reducer(opts: ReducerOpts<{}>, fn: Reducer<S, {}>): ReducerExport<S, {}>;
  reducer<Params extends ParamsObj>(
    ...args:
      | [Params, Reducer<S, Params>]
      | [Reducer<S, {}>]
      | [ReducerOptsWithOptionalName<Params>, Params, Reducer<S, Params>]
      | [ReducerOpts<{}>, Reducer<S, {}>]
  ): ReducerExport<S, Params> {
    let opts: ReducerOptsWithOptionalName<Params> | undefined,
      params: Params = {} as Params,
      fn: Reducer<S, Params>;
    switch (args.length) {
      case 1:
        [fn] = args;
        break;
      case 2: {
        let arg1;
        [arg1, fn] = args;
        if (typeof arg1.name === 'string')
          opts = arg1 as ReducerOptsWithOptionalName<Params>;
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
  init(opts: ReducerOpts<{}>, fn: Reducer<S, {}>): ReducerExport<S, {}>;
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
  clientConnected(
    opts: ReducerOpts<{}>,
    fn: Reducer<S, {}>
  ): ReducerExport<S, {}>;
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
    opts: ReducerOpts<{}>,
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
    fn: F,
    // Compile-time-only guard: this rest parameter is `[]` for valid return
    // builders, but becomes a required error tuple when a returned row builder
    // marks more than one column with `.primaryKey()`.
    ..._: ValidateViewPrimaryKey<Ret>
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
  >(
    opts: ViewOpts,
    ret: Ret,
    fn: F,
    // Compile-time-only guard: this rest parameter is `[]` for valid return
    // builders, but becomes a required error tuple when a returned row builder
    // marks more than one column with `.primaryKey()`.
    ..._: ValidateViewPrimaryKey<Ret>
  ): ViewExport<F> {
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
  ): ProcedureExport<S, Params, Ret>;
  procedure<Ret extends TypeBuilder<any, any>>(
    ret: Ret,
    fn: ProcedureFn<S, {}, Ret>
  ): ProcedureExport<S, {}, Ret>;
  procedure<Params extends ParamsObj, Ret extends TypeBuilder<any, any>>(
    opts: ProcedureOptsWithOptionalName<Params, Ret>,
    params: Params,
    ret: Ret,
    fn: ProcedureFn<S, Params, Ret>
  ): ProcedureExport<S, Params, Ret>;
  procedure<Ret extends TypeBuilder<any, any>>(
    opts: ProcedureOpts<{}, Ret>,
    ret: Ret,
    fn: ProcedureFn<S, {}, Ret>
  ): ProcedureExport<S, {}, Ret>;
  procedure<Params extends ParamsObj, Ret extends TypeBuilder<any, any>>(
    ...args:
      | [Params, Ret, ProcedureFn<S, Params, Ret>]
      | [Ret, ProcedureFn<S, Params, Ret>]
      | [
          ProcedureOptsWithOptionalName<Params, Ret>,
          Params,
          Ret,
          ProcedureFn<S, Params, Ret>,
        ]
      | [ProcedureOpts<{}, Ret>, Ret, ProcedureFn<S, Params, Ret>]
  ): ProcedureExport<S, Params, Ret> {
    let opts: ProcedureOptsWithOptionalName<Params, Ret> | undefined,
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
        if (typeof arg1.name === 'string')
          opts = arg1 as ProcedureOptsWithOptionalName<Params, Ret>;
        else params = arg1 as Params;
        break;
      }
      case 4:
        [opts, params, ret, fn] = args;
        break;
    }
    return makeProcedureExport(this.#ctx, opts, params, ret, fn);
  }

  httpHandler(fn: HandlerFn<S>): HttpHandlerExport<S>;
  httpHandler(opts: HttpHandlerOpts, fn: HandlerFn<S>): HttpHandlerExport<S>;
  httpHandler(
    ...args: [HandlerFn<S>] | [HttpHandlerOpts, HandlerFn<S>]
  ): HttpHandlerExport<S> {
    let opts: HttpHandlerOpts | undefined, fn: HandlerFn<S>;
    switch (args.length) {
      case 1:
        [fn] = args;
        break;
      case 2:
        [opts, fn] = args;
        break;
    }
    return makeHttpHandlerExport(this.#ctx, opts, fn);
  }

  httpRouter(router: Router): ModuleExport {
    return makeHttpRouterExport(this.#ctx, router);
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

type SubmoduleNamespace = {
  default: Schema<any>;
  [key: string]: unknown;
};

type SchemaEntry = UntypedTableSchema | SubmoduleNamespace;

type ExtractTableEntries<H extends Record<string, SchemaEntry>> = {
  [K in keyof H as H[K] extends UntypedTableSchema ? K : never]: Extract<
    H[K],
    UntypedTableSchema
  >;
};

type ExtractSubmoduleSchemas<H extends Record<string, SchemaEntry>> = {
  [K in keyof H as H[K] extends { default: Schema<any> }
    ? K
    : never]: H[K] extends { default: Schema<infer S extends UntypedSchemaDef> }
    ? S
    : never;
};

type SchemaDefForEntries<H extends Record<string, SchemaEntry>> =
  TablesToSchema<ExtractTableEntries<H>> & {
    namespaces: ExtractSubmoduleSchemas<H>;
  };

function isUntypedTableSchema(x: unknown): x is UntypedTableSchema {
  return typeof x === 'object' && x !== null && hasOwn(x, 'tableDef');
}

function isSubmoduleNamespace(x: unknown): x is SubmoduleNamespace {
  return (
    typeof x === 'object' &&
    x !== null &&
    hasOwn(x, 'default') &&
    x.default instanceof Schema
  );
}

function registerModuleExports(
  schema: SchemaInner,
  exports: object,
  opts?: { ignoreNonModuleExports?: boolean }
) {
  if (schema.exportsRegistered) {
    return;
  }
  schema.exportsRegistered = true;

  for (const [name, moduleExport] of Object.entries(exports)) {
    if (name === 'default') continue;
    if (!isModuleExport(moduleExport)) {
      if (opts?.ignoreNonModuleExports) {
        continue;
      }
      throw new TypeError('exporting something that is not a spacetime export');
    }
    checkExportContext(moduleExport, schema);
    moduleExport[registerExport](schema, name);
  }
}

export function schema<const H extends Record<string, SchemaEntry>>(
  entries: H,
  moduleSettings?: ModuleSettings
): Schema<SchemaDefForEntries<H>> {
  const ctx = new SchemaInner<SchemaDefForEntries<H>>(ctx => {
    // Apply module settings.
    if (moduleSettings?.CASE_CONVERSION_POLICY != null) {
      ctx.setCaseConversionPolicy(moduleSettings.CASE_CONVERSION_POLICY);
    }

    const tableSchemas: Record<string, UntypedTableDef> = {};
    for (const [accName, entry] of Object.entries(entries)) {
      if (entry instanceof Schema) {
        throw new TypeError(
          `schema entry '${accName}' looks like a default import; use \`import * as ${accName} from '...'\` so the submodule can see the library's named reducer exports.`
        );
      }
      if (isSubmoduleNamespace(entry)) {
        const { rawDef, dispatch } =
          entry.default.buildSubmoduleDispatch(entry);
        dispatch.namespace = accName;
        ctx.addSubmodule({ namespace: accName, module: rawDef });
        ctx.submoduleDispatchInfos.push(dispatch);
        continue;
      }
      if (!isUntypedTableSchema(entry)) {
        throw new TypeError(
          `schema entry '${accName}' must be a table or a submodule namespace object`
        );
      }

      const table = entry;
      const tableDef = table.tableDef(ctx, accName);
      tableSchemas[accName] = tableToSchema(accName, table, tableDef);
      const tableSourceNames = ctx.tableSourceNames.get(table);
      if (tableSourceNames === undefined) {
        ctx.tableSourceNames.set(table, [tableDef.sourceName]);
      } else {
        tableSourceNames.push(tableDef.sourceName);
      }
      ctx.moduleDef.tables.push(tableDef);
      if (table.schedule) {
        ctx.pendingSchedules.push({
          table,
          tableName: tableDef.sourceName,
          scheduleAtCol: table.schedule.scheduleAtCol,
          reducer: table.schedule.reducer,
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
    return { tables: tableSchemas } as SchemaDefForEntries<H>;
  });

  return new Schema(ctx);
}
