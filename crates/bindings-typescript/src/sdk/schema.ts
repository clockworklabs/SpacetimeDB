import type { ProductType } from '../lib/algebraic_type';
import type { ParamsObj } from '../lib/reducers';
import {
  ModuleContext,
  tablesToSchema,
  type SchemaDef,
  type TableDef,
  type UntypedSchemaDef,
} from '../lib/schema';
import { table, type CoerceRow } from '../lib/table';
import type { TableDecl, UntypedTableDecl } from '../lib/table_schema';
import {
  ArrayBuilder,
  ProductBuilder,
  type OptionBuilder,
  type RowBuilder,
  type RowObj,
  type TypeBuilder,
} from '../lib/type_builders';
import type { Prettify } from '../lib/type_util';
import { toSnakeCase, type CoerceParams } from '../lib/util';
import type {
  ProcedureOptsWithOptionalName,
  ProcedureSignature,
} from '../server/procedures';
import type {
  ReducerOptsWithOptionalName,
  ReducerSignature,
} from '../server/reducers';
import {
  exportSignature,
  schema as moduleSchema,
  Schema,
  type ModuleSettings,
  type SignedProcedure,
  type SignedReducer,
  type SignedView,
  type WithSignature,
} from '../server/schema';
import type {
  AnonymousViewFn,
  ViewFn,
  ViewOpts,
  ViewReturnTypeBuilder,
  ViewSignature,
} from '../server/views';
import { procedureSchema, type UntypedProcedureDef } from './procedures';
import { reducerSchema, reducersToSchema } from './reducers';
import type { ModuleDef } from './spacetime_module';
import { _MINIMUM_CLI_VERSION } from './version';

/**
 * A module's {@link Schema} as client bindings see it. Client bindings declare
 * the module's reducers, procedures, and views on it, without their bodies.
 * A module declares them with bodies, on the `schema()` from `spacetimedb/server`.
 */
export interface ClientSchema<S extends UntypedSchemaDef> {
  readonly schemaType: S;

  /** Declares a reducer without its body. */
  reducer<Params extends ParamsObj>(params: Params): SignedReducer<S, Params>;
  reducer<Params extends ParamsObj, Name extends string = string>(
    opts: ReducerOptsWithOptionalName<Params, Name>,
    params: Params
  ): SignedReducer<S, Params, Name>;

  /** Declares a procedure without its body. */
  procedure<Params extends ParamsObj, Ret extends TypeBuilder<any, any>>(
    params: Params,
    ret: Ret
  ): SignedProcedure<S, Params, Ret>;
  procedure<
    Params extends ParamsObj,
    Ret extends TypeBuilder<any, any>,
    Name extends string = string,
  >(
    opts: ProcedureOptsWithOptionalName<Params, Ret, Name>,
    params: Params,
    ret: Ret
  ): SignedProcedure<S, Params, Ret, Name>;

  /** Declares a view without its body. */
  view<Ret extends ViewReturnTypeBuilder>(
    opts: ViewOpts,
    ret: Ret
  ): SignedView<ViewFn<S, {}, Ret>, Ret>;

  /** Declares an anonymous view without its body. */
  anonymousView<Ret extends ViewReturnTypeBuilder>(
    opts: ViewOpts,
    ret: Ret
  ): SignedView<AnonymousViewFn<S, {}, Ret>, Ret>;
}

/**
 * Creates a schema from table declarations. It returns the same {@link Schema}
 * that a module's `schema()` does, typed as a {@link ClientSchema}, so client
 * bindings can declare the module's reducers, procedures, and views on it,
 * without their bodies.
 * @param tables - The table declarations, keyed by accessor name
 * @param moduleSettings - The module's settings, such as its case conversion policy
 * @returns The {@link ClientSchema} of the module
 * @example
 * ```ts
 * const spacetimedb = schema({
 *   user: table({}, userType),
 *   post: table({}, postType)
 * });
 * ```
 */
export function schema<const H extends Record<string, UntypedTableDecl>>(
  tables: H,
  moduleSettings?: ModuleSettings
): ClientSchema<SchemaDef<H>> {
  return moduleSchema(tables, moduleSettings) as unknown as ClientSchema<
    SchemaDef<H>
  >;
}

/**
 * A module's exports, with its schema as the default export: a
 * {@link ClientSchema} in client bindings, or a {@link Schema} in module source.
 */
export type ModuleExports = {
  readonly default: Schema<any> | ClientSchema<any>;
};

type SignatureOf<E> = E extends WithSignature<infer Sig> ? Sig : never;

/** The row of a view's return type, which is an array or an option of rows. */
type ViewRow<Ret> =
  Ret extends ArrayBuilder<infer E>
    ? RowOf<E>
    : Ret extends OptionBuilder<infer E>
      ? RowOf<E>
      : never;
type RowOf<E> =
  E extends RowBuilder<infer Row>
    ? Row
    : E extends ProductBuilder<infer Elements>
      ? Elements
      : never;

// These take the signature as a type parameter so that they distribute over it,
// and an export without a signature (`never`) contributes nothing.

/** A client sees each view as a table whose rows are the view's rows. */
type ViewTable<K extends string, Sig> =
  Sig extends ViewSignature<infer Ret>
    ? TableDef<K, TableDecl<CoerceRow<ViewRow<Ret>>, []>>
    : never;
type ViewKey<K extends string, Sig> =
  Sig extends ViewSignature<any> ? K : never;
type ViewTables<M> = {
  readonly [K in keyof M & string as ViewKey<K, SignatureOf<M[K]>>]: ViewTable<
    K,
    SignatureOf<M[K]>
  >;
};

type ReducerDef<K extends string, Sig> =
  Sig extends ReducerSignature<infer Name, infer Params>
    ? {
        name: Name;
        accessorName: K;
        params: CoerceRow<Params>;
        paramsType: ProductType;
      }
    : never;
type ReducerDefs<M> = {
  [K in keyof M & string]: ReducerDef<K, SignatureOf<M[K]>>;
}[keyof M & string];

type ProcedureDef<K extends string, Sig> =
  Sig extends ProcedureSignature<infer Name, infer Params, infer Ret>
    ? {
        name: Name;
        accessorName: K;
        params: CoerceParams<Params>;
        returnType: Ret;
      }
    : never;
type ProcedureDefs<M> = {
  [K in keyof M & string]: ProcedureDef<K, SignatureOf<M[K]>>;
}[keyof M & string];

/**
 * The {@link ModuleDef} of a module, inferred from the type of its exports.
 * It is the type of `moduleDefFromExports(module)`.
 */
export type InferModule<M extends ModuleExports> = ModuleDef<
  {
    tables: Prettify<M['default']['schemaType']['tables'] & ViewTables<M>>;
  },
  { reducers: readonly ReducerDefs<M>[] },
  { procedures: readonly ProcedureDefs<M>[] }
>;

// TODO: A client could import module source as types only, as Convex's
// generated `api` does, and fetch the module's schema from the host when it
// connects. That would need no export condition and put no module code in the
// client's bundle.
/**
 * Builds a client's {@link ModuleDef} from a module's exports: the default
 * export is the module's schema, and the named exports are its reducers,
 * procedures, and views, with or without their bodies. Each export's name is
 * its accessor name. A client sees each view as a table, and cannot call
 * lifecycle reducers, so they are left out.
 *
 * @example
 * ```ts
 * import * as module from './module';
 * const moduleDef = moduleDefFromExports(module);
 * class DbConnection extends DbConnectionImpl<typeof moduleDef> {}
 * ```
 */
export function moduleDefFromExports<const M extends ModuleExports>(
  module: M
): InferModule<M> {
  const spacetimedb = module.default;
  if (!(spacetimedb instanceof Schema)) {
    throw new TypeError(
      "moduleDefFromExports expects a module whose default export is a schema() from this copy of 'spacetimedb'. If it is a schema(), the client loaded a second copy of the SDK. The usual causes are:\n" +
        "- The client was built without the 'spacetimedb-client' export condition, so 'spacetimedb/server' is the host build. Add it with Vite's resolve.conditions, esbuild's conditions, webpack's resolve.conditionNames, or node --conditions=spacetimedb-client.\n" +
        "- A second installation of the package, such as the module folder's own node_modules/spacetimedb. Install it once, for example in a workspace, or dedupe it with the bundler (Vite's resolve.dedupe).\n" +
        '- The module folder\'s package.json lacks "type": "module", so the module loaded as CommonJS. Add it.'
    );
  }
  if (spacetimedb.submoduleDispatchInfos.length > 0) {
    throw new TypeError(
      'moduleDefFromExports does not support modules that mount submodules yet'
    );
  }
  // Register the exports as the host would, so that the client rejects the
  // declarations that the host would reject.
  spacetimedb.buildRawModuleDefV10(module, {
    ignoreNonModuleExports: true,
    declarationsOnly: true,
  });
  // As on the host, a canonical name is the explicit name, if given, or else
  // the accessor name under the module's case conversion policy. Procedures are
  // the exception: the host also converts an explicit procedure name (see
  // `makeProcedureExport`), which this does not yet mirror.
  const { caseConversionPolicy } = spacetimedb.moduleDef;
  const canonicalName = (accessorName: string, name: string | undefined) =>
    name ??
    (caseConversionPolicy.tag === 'None'
      ? accessorName
      : toSnakeCase(accessorName));

  const views: Record<string, UntypedTableDecl> = {};
  const reducers = [];
  const procedures: UntypedProcedureDef[] = [];
  for (const [accessorName, value] of Object.entries(module)) {
    const signature = (value as Partial<WithSignature<AnySignature>> | null)?.[
      exportSignature
    ];
    if (signature === undefined) continue;
    const name = canonicalName(accessorName, signature.name);
    switch (signature.kind) {
      case 'reducer':
        reducers.push(reducerSchema(name, signature.params, accessorName));
        break;
      case 'procedure':
        procedures.push(
          procedureSchema(
            name,
            signature.params,
            signature.returnType,
            accessorName
          )
        );
        break;
      case 'view':
        views[accessorName] = table({ name }, viewRow(signature.returnType));
        break;
    }
  }

  return {
    // No CLI generated this module def. This SDK built it, so it meets the
    // SDK's minimum by construction.
    versionInfo: { cliVersion: _MINIMUM_CLI_VERSION.toString() },
    tables: {
      ...spacetimedb.schemaType.tables,
      ...tablesToSchema(new ModuleContext(), views).tables,
    },
    ...reducersToSchema(reducers),
    procedures,
  } as unknown as InferModule<M>;
}

type AnySignature =
  | ReducerSignature<string, ParamsObj>
  | ProcedureSignature<string, ParamsObj, any>
  | ViewSignature<ViewReturnTypeBuilder>;

/** The row of a view's return type, which is an array or an option of rows. */
function viewRow(ret: ViewReturnTypeBuilder): RowObj | RowBuilder<RowObj> {
  const element =
    ret instanceof ArrayBuilder
      ? ret.element
      : (ret as OptionBuilder<any>).value;
  return element instanceof ProductBuilder ? element.elements : element;
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
