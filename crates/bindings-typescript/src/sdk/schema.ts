import type { ProductType } from '../lib/algebraic_type';
import type { ParamsObj } from '../lib/reducers';
import {
  ModuleContext,
  tablesToSchema,
  type SchemaDecl,
  type TableDecl,
  type UntypedSchemaDecl,
} from '../lib/schema';
import { table, type CoerceRow } from '../lib/table';
import type { TableBody, UntypedTableBody } from '../lib/table_body';
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
import { procedureSchema, type UntypedProcedureDecl } from './procedures';
import { reducerSchema, reducersToSchema } from './reducers';
import type { RemoteModuleDecl } from './spacetime_module';
import { _MINIMUM_CLI_VERSION } from './version';

/**
 * A module's {@link Schema} as client bindings see it. Client bindings declare
 * the module's reducers, procedures, and views on it, without their bodies.
 * A module declares them with bodies, on the `schema()` from `spacetimedb/server`.
 */
export interface ClientSchema<S extends UntypedSchemaDecl> {
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
 * Creates a schema from table bodies. It returns the same {@link Schema}
 * that a module's `schema()` does, typed as a {@link ClientSchema}, so client
 * bindings can declare the module's reducers, procedures, and views on it,
 * without their bodies.
 * @param tables - The table bodies, keyed by accessor name
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
export function schema<const H extends Record<string, UntypedTableBody>>(
  tables: H,
  moduleSettings?: ModuleSettings
): ClientSchema<SchemaDecl<H>> {
  return moduleSchema(tables, moduleSettings) as unknown as ClientSchema<
    SchemaDecl<H>
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
    ? TableDecl<K, TableBody<CoerceRow<ViewRow<Ret>>, []>>
    : never;
type ViewKey<K extends string, Sig> =
  Sig extends ViewSignature<any> ? K : never;
type ViewTables<M> = {
  readonly [K in keyof M & string as ViewKey<K, SignatureOf<M[K]>>]: ViewTable<
    K,
    SignatureOf<M[K]>
  >;
};

type ReducerDecl<K extends string, Sig> =
  Sig extends ReducerSignature<infer Name, infer Params>
    ? {
        name: Name;
        accessorName: K;
        params: CoerceRow<Params>;
        paramsType: ProductType;
      }
    : never;
type ReducerDecls<M> = {
  [K in keyof M & string]: ReducerDecl<K, SignatureOf<M[K]>>;
}[keyof M & string];

type ProcedureDecl<K extends string, Sig> =
  Sig extends ProcedureSignature<infer Name, infer Params, infer Ret>
    ? {
        name: Name;
        accessorName: K;
        params: CoerceParams<Params>;
        returnType: Ret;
      }
    : never;
type ProcedureDecls<M> = {
  [K in keyof M & string]: ProcedureDecl<K, SignatureOf<M[K]>>;
}[keyof M & string];

/**
 * The {@link RemoteModuleDecl} of a module, inferred from the type of its exports.
 * It is the type of `remoteModuleDeclFromExports(module)`.
 */
export type InferRemoteModuleDecl<M extends ModuleExports> = RemoteModuleDecl<
  {
    tables: Prettify<M['default']['schemaType']['tables'] & ViewTables<M>>;
  },
  { reducers: readonly ReducerDecls<M>[] },
  { procedures: readonly ProcedureDecls<M>[] }
>;

/**
 * Builds a client's {@link RemoteModuleDecl} from a module's exports: the default
 * export is the module's schema, and the named exports are its reducers,
 * procedures, and views, with or without their bodies. Each export's name is
 * its accessor name. A client sees each view as a table, and cannot call
 * lifecycle reducers, so they are left out.
 *
 * @example
 * ```ts
 * import * as module from './module';
 * const remoteModuleDecl = remoteModuleDeclFromExports(module);
 * class DbConnection extends DbConnectionImpl<typeof remoteModuleDecl> {}
 * ```
 */
export function remoteModuleDeclFromExports<const M extends ModuleExports>(
  module: M
): InferRemoteModuleDecl<M> {
  const spacetimedb = module.default;
  if (!(spacetimedb instanceof Schema)) {
    throw new TypeError(
      "remoteModuleDeclFromExports expects a module whose default export is the schema() from 'spacetimedb'. A module's own source, which uses the schema() from 'spacetimedb/server', is not supported yet."
    );
  }
  if (spacetimedb.submoduleDispatchInfos.length > 0) {
    throw new TypeError(
      'remoteModuleDeclFromExports does not support modules that mount submodules yet'
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

  const views: Record<string, UntypedTableBody> = {};
  const reducers = [];
  const procedures: UntypedProcedureDecl[] = [];
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
    // No CLI generated this remote module declaration. This SDK built it, so
    // it meets the SDK's minimum by construction.
    versionInfo: { cliVersion: _MINIMUM_CLI_VERSION.toString() },
    tables: {
      ...spacetimedb.schemaType.tables,
      ...tablesToSchema(new ModuleContext(), views).tables,
    },
    ...reducersToSchema(reducers),
    procedures,
  } as unknown as InferRemoteModuleDecl<M>;
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
