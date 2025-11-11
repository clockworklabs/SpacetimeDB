import type RawTableDefV9 from './autogen/raw_table_def_v_9_type';
import type Typespace from './autogen/typespace_type';
import {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  ColumnBuilder,
  ProductBuilder,
  RefBuilder,
  RowBuilder,
  SumBuilder,
  type ElementsObj,
  type Infer,
  type RowObj,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  type TypeBuilder,
  type VariantsObj,
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
  type AlgebraicTypeType,
  type AlgebraicTypeVariants,
} from './algebraic_type';
import type RawScopedTypeNameV9 from './autogen/raw_scoped_type_name_v_9_type';
import type { CamelCase } from './type_util';
import type { TableSchema } from './table_schema';
import { toCamelCase } from './util';
import {
  defineView,
  type AnonymousViewFn,
  type ViewFn,
  type ViewOpts,
  type ViewReturnTypeBuilder,
} from './views';
import RawIndexDefV9 from './autogen/raw_index_def_v_9_type';
import type { IndexOpts } from './indexes';

export type TableNamesOf<S extends UntypedSchemaDef> =
  S['tables'][number]['name'];

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
      accessorName: CamelCase<T[i]['tableName']>;
      columns: T[i]['rowType']['row'];
      rowType: T[i]['rowSpacetimeType'];
      indexes: T[i]['idxs'];
      constraints: T[i]['constraints'];
    };
  };
};

export function tablesToSchema<
  const T extends readonly TableSchema<any, any, readonly any[]>[],
>(tables: T): TablesToSchema<T> {
  const result = {
    tables: tables.map(schema => {
      const colNameList: string[] = [];
      schema.rowType.algebraicType.value.elements.forEach(elem => {
        colNameList.push(elem.name);
      });

      return {
        name: schema.tableName,
        accessorName: toCamelCase(schema.tableName),
        columns: schema.rowType.row, // typed as T[i]['rowType']['row'] under TablesToSchema<T>
        rowType: schema.rowSpacetimeType,
        constraints: [
          ...schema.tableDef.constraints.map(c => ({
            name: c.name,
            constraint: 'unique' as const,
            columns: Array.from(c.data.value.columns.map(i => colNameList[i])),
          })),
        ],
        // UntypedTableDef expects mutable array; idxs are readonly, spread to copy.
        indexes: [
          ...schema.idxs.map(
            (idx: Infer<typeof RawIndexDefV9>): IndexOpts<any> =>
              ({
                name: idx.accessorName,
                unique: schema.tableDef.constraints
                  .map(c => {
                    if (idx.algorithm.tag == 'BTree') {
                      return c.data.value.columns.every(col => {
                        const idxColumns = idx.algorithm.value;
                        if (Array.isArray(idxColumns)) {
                          return idxColumns.includes(col);
                        } else {
                          return col === idxColumns;
                        }
                      });
                    }
                  })
                  .includes(true),
                algorithm: idx.algorithm.tag.toLowerCase() as 'btree',
                columns: (() => {
                  const cols =
                    idx.algorithm.tag === 'Direct'
                      ? [idx.algorithm.value]
                      : idx.algorithm.value;
                  return cols.map(i => colNameList[i]);
                })(),
              }) as IndexOpts<any>
          ),
        ],
      } as const;
    }) as {
      // preserve tuple indices so the return type matches `[i in keyof T]`
      readonly [I in keyof T]: {
        name: T[I]['tableName'];
        accessorName: CamelCase<T[I]['tableName']>;
        columns: T[I]['rowType']['row'];
        rowType: T[I]['rowSpacetimeType'];
        indexes: T[I]['idxs'];
        constraints: T[I]['constraints'];
      };
    },
  } satisfies TablesToSchema<T>;
  return result;
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
  RefBuilder
>();

/**
 * Resolves the actual type of a TypeBuilder by following its references until it reaches a non-ref type.
 * @param typespace The typespace to resolve types against.
 * @param typeBuilder The TypeBuilder to resolve.
 * @returns The resolved algebraic type.
 */
export function resolveType<AT extends AlgebraicTypeType>(
  typespace: Infer<typeof Typespace>,
  typeBuilder: TypeBuilder<any, AT>
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
export function registerTypesRecursively(
  typeBuilder:
    | SumBuilder<VariantsObj>
    | ProductBuilder<ElementsObj>
    | RowBuilder<RowObj>
): RefBuilder {
  const ty = typeBuilder.algebraicType;
  const name = typeBuilder.typeName;

  let r = COMPOUND_TYPES.get(ty);
  if (r != null) {
    // Already added to typespace
    return r;
  }

  // Recursively register nested compound types
  if (typeBuilder instanceof RowBuilder) {
    for (const [name, elem] of Object.entries(typeBuilder.row)) {
      if (
        !(
          elem instanceof ProductBuilder ||
          elem instanceof SumBuilder ||
          elem instanceof RowBuilder
        )
      ) {
        continue;
      }
      typeBuilder.row[name] = new ColumnBuilder(
        registerTypesRecursively(elem),
        {}
      );
    }
  } else if (typeBuilder instanceof ProductBuilder) {
    for (const [name, elem] of Object.entries(typeBuilder.elements)) {
      if (
        !(
          elem instanceof ProductBuilder ||
          elem instanceof SumBuilder ||
          elem instanceof RowBuilder
        )
      ) {
        continue;
      }
      typeBuilder.elements[name] = registerTypesRecursively(elem);
    }
  } else if (typeBuilder instanceof SumBuilder) {
    for (const [name, variant] of Object.entries(typeBuilder.variants)) {
      if (
        !(
          variant instanceof ProductBuilder ||
          variant instanceof SumBuilder ||
          variant instanceof RowBuilder
        )
      ) {
        continue;
      }
      typeBuilder.variants[name] = registerTypesRecursively(variant);
    }
  }

  // Add to typespace and return a Ref type
  r = new RefBuilder(MODULE_DEF.typespace.types.length);
  MODULE_DEF.typespace.types.push(ty);

  COMPOUND_TYPES.set(ty, r);
  if (name !== undefined)
    MODULE_DEF.types.push({
      name: splitName(name),
      ty: r.ref,
      customOrdering: true,
    });
  return r;
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
    handles: readonly TableSchema<any, any, any>[]
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

  anyonymousView<Ret extends ViewReturnTypeBuilder>(
    opts: ViewOpts,
    ret: Ret,
    fn: AnonymousViewFn<S, {}, Ret>
  ): void {
    defineView(opts, true, {}, ret, fn);
  }

  // TODO: re-enable once parameterized views are supported in SQL
  // anyonymousView<Ret extends ViewReturnTypeBuilder>(
  //   opts: ViewOpts,
  //   ret: Ret,
  //   fn: AnonymousViewFn<S, {}, Ret>
  // ): void;
  // anyonymousView<Params extends ParamsObj, Ret extends ViewReturnTypeBuilder>(
  //   opts: ViewOpts,
  //   params: Params,
  //   ret: Ret,
  //   fn: AnonymousViewFn<S, {}, Ret>
  // ): void;
  // anyonymousView<Params extends ParamsObj, Ret extends ViewReturnTypeBuilder>(
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
export function schema<const H extends readonly TableSchema<any, any, any>[]>(
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
