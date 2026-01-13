import type RawTableDefV9 from './autogen/raw_table_def_v_9_type';
import type Typespace from './autogen/typespace_type';
import {
  ArrayBuilder,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  ColumnBuilder,
  OptionBuilder,
  ProductBuilder,
  RefBuilder,
  RowBuilder,
  SumBuilder,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  TypeBuilder,
  type ElementsObj,
  type Infer,
  type InferSpacetimeTypeOfTypeBuilder,
  type RowObj,
  type VariantsObj,
  ResultBuilder,
} from './type_builders';
import type { UntypedTableDef } from './table';
import {
  clientConnected,
  clientDisconnected,
  init,
  reducer,
  type ParamsObj,
  type Reducer,
} from './reducers';
import type RawModuleDefV9 from './autogen/raw_module_def_v_9_type';
import {
  AlgebraicType,
  ProductType,
  SumType,
  type AlgebraicTypeType,
  type AlgebraicTypeVariants,
} from './algebraic_type';
import type RawScopedTypeNameV9 from './autogen/raw_scoped_type_name_v_9_type';
import type { CamelCase } from './type_util';
import type { UntypedTableSchema } from './table_schema';
import { toCamelCase } from './util';
import {
  defineView,
  type AnonymousViewFn,
  type ViewFn,
  type ViewOpts,
  type ViewReturnTypeBuilder,
} from './views';
import type { UntypedIndex } from './indexes';
import { procedure, type ProcedureFn } from './procedures';

export type TableNamesOf<S extends UntypedSchemaDef> =
  S['tables'][number]['name'];

/**
 * An untyped representation of the database schema.
 */
export type UntypedSchemaDef = {
  tables: readonly UntypedTableDef[];
};

let REGISTERED_SCHEMA: UntypedSchemaDef | null = null;

export function getRegisteredSchema(): UntypedSchemaDef {
  if (REGISTERED_SCHEMA == null) {
    throw new Error('Schema has not been registered yet. Call schema() first.');
  }
  return REGISTERED_SCHEMA;
}

/**
 * Helper type to convert an array of TableSchema into a schema definition
 */
type TablesToSchema<T extends readonly UntypedTableSchema[]> = {
  tables: {
    readonly [i in keyof T]: TableToSchema<T[i]>;
  };
};

export interface TableToSchema<T extends UntypedTableSchema>
  extends UntypedTableDef {
  name: T['tableName'];
  accessorName: CamelCase<T['tableName']>;
  columns: T['rowType']['row'];
  rowType: T['rowSpacetimeType'];
  indexes: T['idxs'];
  constraints: T['constraints'];
}

export function tablesToSchema<const T extends readonly UntypedTableSchema[]>(
  tables: T
): TablesToSchema<T> {
  return { tables: tables.map(tableToSchema) as TablesToSchema<T>['tables'] };
}

function tableToSchema<T extends UntypedTableSchema>(
  schema: T
): TableToSchema<T> {
  const getColName = (i: number) =>
    schema.rowType.algebraicType.value.elements[i].name;

  type AllowedCol = keyof T['rowType']['row'] & string;
  return {
    name: schema.tableName,
    accessorName: toCamelCase(schema.tableName as T['tableName']),
    columns: schema.rowType.row, // typed as T[i]['rowType']['row'] under TablesToSchema<T>
    rowType: schema.rowSpacetimeType,
    constraints: schema.tableDef.constraints.map(c => ({
      name: c.name,
      constraint: 'unique',
      columns: c.data.value.columns.map(getColName) as [string],
    })),
    // TODO: horrible horrible horrible. we smuggle this `Array<UntypedIndex>`
    // by casting it to an `Array<IndexOpts>` as `TableToSchema` expects.
    // This is then used in `TableCacheImpl.constructor` and who knows where else.
    // We should stop lying about our types.
    indexes: schema.tableDef.indexes.map((idx): UntypedIndex<AllowedCol> => {
      const columnIds =
        idx.algorithm.tag === 'Direct'
          ? [idx.algorithm.value]
          : idx.algorithm.value;
      return {
        name: idx.accessorName!,
        unique: schema.tableDef.constraints.some(c =>
          c.data.value.columns.every(col => columnIds.includes(col))
        ),
        algorithm: idx.algorithm.tag.toLowerCase() as 'btree',
        columns: columnIds.map(getColName),
      };
    }) as T['idxs'],
  };
}

/**
 * The global module definition that gets populated by calls to `reducer()` and lifecycle hooks.
 */
export const MODULE_DEF: Infer<typeof RawModuleDefV9> = {
  typespace: { types: [] },
  tables: [],
  reducers: [],
  types: [],
  miscExports: [],
  rowLevelSecurity: [],
};

const COMPOUND_TYPES = new Map<
  AlgebraicTypeVariants.Product | AlgebraicTypeVariants.Sum,
  RefBuilder<any, any>
>();

/**
 * Resolves the actual type of a TypeBuilder by following its references until it reaches a non-ref type.
 * @param typespace The typespace to resolve types against.
 * @param typeBuilder The TypeBuilder to resolve.
 * @returns The resolved algebraic type.
 */
export function resolveType<AT extends AlgebraicTypeType>(
  typespace: Infer<typeof Typespace>,
  typeBuilder: RefBuilder<any, AT>
): AT {
  let ty: AlgebraicType = typeBuilder.algebraicType;
  while (ty.tag === 'Ref') {
    ty = typespace.types[ty.value];
  }
  return ty as AT;
}

/**
 * Adds a type to the module definition's typespace as a `Ref` if it is a named compound type (Product or Sum).
 * Otherwise, returns the type as is.
 * @param name
 * @param ty
 * @returns
 */
export function registerTypesRecursively<
  T extends TypeBuilder<any, AlgebraicType>,
>(
  typeBuilder: T
): T extends SumBuilder<any> | ProductBuilder<any> | RowBuilder<any>
  ? RefBuilder<Infer<T>, InferSpacetimeTypeOfTypeBuilder<T>>
  : T {
  if (
    (typeBuilder instanceof ProductBuilder && !isUnit(typeBuilder)) ||
    typeBuilder instanceof SumBuilder ||
    typeBuilder instanceof RowBuilder
  ) {
    return registerCompoundTypeRecursively(typeBuilder) as any;
  } else if (typeBuilder instanceof OptionBuilder) {
    return new OptionBuilder(
      registerTypesRecursively(typeBuilder.value)
    ) as any;
  } else if (typeBuilder instanceof ResultBuilder) {
    return new ResultBuilder(
      registerTypesRecursively(typeBuilder.ok),
      registerTypesRecursively(typeBuilder.err)
    ) as any;
  } else if (typeBuilder instanceof ArrayBuilder) {
    return new ArrayBuilder(
      registerTypesRecursively(typeBuilder.element)
    ) as any;
  } else {
    return typeBuilder as any;
  }
}

function registerCompoundTypeRecursively<
  T extends
    | SumBuilder<VariantsObj>
    | ProductBuilder<ElementsObj>
    | RowBuilder<RowObj>,
>(typeBuilder: T): RefBuilder<Infer<T>, InferSpacetimeTypeOfTypeBuilder<T>> {
  const ty = typeBuilder.algebraicType;
  // NB! You must ensure that all TypeBuilder passed into this function
  // have a name. This function ensures that nested types always have a
  // name by assigning them one if they are missing it.
  const name = typeBuilder.typeName;
  if (name === undefined) {
    throw new Error(
      `Missing type name for ${typeBuilder.constructor.name ?? 'TypeBuilder'} ${JSON.stringify(typeBuilder)}`
    );
  }

  let r = COMPOUND_TYPES.get(ty);
  if (r != null) {
    // Already added to typespace
    return r;
  }

  // Recursively register nested compound types
  const newTy =
    typeBuilder instanceof RowBuilder || typeBuilder instanceof ProductBuilder
      ? ({
          tag: 'Product',
          value: { elements: [] },
        } as AlgebraicTypeVariants.Product)
      : ({ tag: 'Sum', value: { variants: [] } } as AlgebraicTypeVariants.Sum);

  r = new RefBuilder(MODULE_DEF.typespace.types.length);
  MODULE_DEF.typespace.types.push(newTy);

  COMPOUND_TYPES.set(ty, r);

  if (typeBuilder instanceof RowBuilder) {
    for (const [name, elem] of Object.entries(typeBuilder.row)) {
      (newTy.value as ProductType).elements.push({
        name,
        algebraicType: registerTypesRecursively(elem.typeBuilder).algebraicType,
      });
    }
  } else if (typeBuilder instanceof ProductBuilder) {
    for (const [name, elem] of Object.entries(typeBuilder.elements)) {
      (newTy.value as ProductType).elements.push({
        name,
        algebraicType: registerTypesRecursively(elem).algebraicType,
      });
    }
  } else if (typeBuilder instanceof SumBuilder) {
    for (const [name, variant] of Object.entries(typeBuilder.variants)) {
      (newTy.value as SumType).variants.push({
        name,
        algebraicType: registerTypesRecursively(variant).algebraicType,
      });
    }
  }

  MODULE_DEF.types.push({
    name: splitName(name),
    ty: r.ref,
    customOrdering: true,
  });

  return r;
}

function isUnit(typeBuilder: ProductBuilder<ElementsObj>): boolean {
  return (
    typeBuilder.typeName == null &&
    typeBuilder.algebraicType.value.elements.length === 0
  );
}

export function splitName(name: string): Infer<typeof RawScopedTypeNameV9> {
  const scope = name.split('.');
  return { name: scope.pop()!, scope };
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
  readonly tablesDef: { tables: Infer<typeof RawTableDefV9>[] };
  readonly typespace: Infer<typeof Typespace>;
  readonly schemaType: S;

  constructor(
    tables: Infer<typeof RawTableDefV9>[],
    typespace: Infer<typeof Typespace>,
    handles: readonly UntypedTableSchema[]
  ) {
    this.tablesDef = { tables };
    this.typespace = typespace;
    // TODO: TableSchema and TableDef should really be unified
    this.schemaType = tablesToSchema(handles) as S;
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
  init(fn: Reducer<S, {}>): void;
  init(name: string, fn: Reducer<S, {}>): void;
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
  clientConnected(fn: Reducer<S, {}>): void;
  clientConnected(name: string, fn: Reducer<S, {}>): void;
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
  clientDisconnected(fn: Reducer<S, {}>): void;
  clientDisconnected(name: string, fn: Reducer<S, {}>): void;
  clientDisconnected(nameOrFn: any, maybeFn?: Reducer<S, {}>): void {
    const [name, fn] =
      typeof nameOrFn === 'string'
        ? [nameOrFn, maybeFn]
        : ['on_disconnect', nameOrFn];
    clientDisconnected(name, {}, fn);
  }

  view<Ret extends ViewReturnTypeBuilder>(
    opts: ViewOpts,
    ret: Ret,
    fn: ViewFn<S, {}, Ret>
  ): void {
    defineView(opts, false, {}, ret, fn);
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
    defineView(opts, true, {}, ret, fn);
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
      procedure(name, {}, paramsOrRet as Ret, retOrFn);
      return retOrFn;
    } else {
      procedure(name, paramsOrRet as Params, retOrFn, maybeFn!);
      return maybeFn!;
    }
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
  const tableDefs = handles.map(h => h.tableDef);

  // Side-effect:
  // Modify the `MODULE_DEF` which will be read by
  // __describe_module__
  MODULE_DEF.tables.push(...tableDefs);
  REGISTERED_SCHEMA = {
    tables: handles.map(handle => ({
      name: handle.tableName,
      accessorName: handle.tableName,
      columns: handle.rowType.row,
      rowType: handle.rowSpacetimeType,
      indexes: handle.idxs,
      constraints: handle.constraints,
    })),
  };
  // MODULE_DEF.typespace = typespace;
  // throw new Error(
  //   MODULE_DEF.tables
  //     .map(t => {
  //       const p = MODULE_DEF.typespace.types[t.productTypeRef];
  //       return `${t.name}: ${t.productTypeRef} ${p && (p as AlgebraicTypeVariants.Product).value.elements.map(x => x.name)}`;
  //     })
  //     .join('\n')
  // );

  return new Schema(tableDefs, MODULE_DEF.typespace, handles);
}

type HasAccessor = { accessorName: PropertyKey };

export type ConvertToAccessorMap<TableDefs extends readonly HasAccessor[]> = {
  [Tbl in TableDefs[number] as Tbl['accessorName']]: Tbl;
};

export function convertToAccessorMap<T extends readonly HasAccessor[]>(
  arr: T
): ConvertToAccessorMap<T> {
  return Object.fromEntries(
    arr.map(v => [v.accessorName, v])
  ) as ConvertToAccessorMap<T>;
}
