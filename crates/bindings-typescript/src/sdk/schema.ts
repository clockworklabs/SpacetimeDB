import type { ProductType } from '../lib/algebraic_type';
import type { ParamsObj } from '../lib/reducers';
import {
  ModuleContext,
  tablesToSchema,
  type SchemaDecl,
  type TableDecl,
  type UntypedSchemaDecl,
} from '../lib/schema';
import { table, type CoerceRow, type UntypedTableDecl } from '../lib/table';
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
import type { CoerceParams } from '../lib/util';
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

// The host converts a procedure's explicit name under the module's case
// conversion policy, which the type of a module does not record, so the name
// is a string.
type ProcedureDecl<K extends string, Sig> =
  Sig extends ProcedureSignature<string, infer Params, infer Ret>
    ? {
        name: string;
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

// TODO: A client could import module source as types only, as Convex's
// generated `api` does, and fetch the module's schema from the host when it
// connects. That would need no export condition and put no module code in the
// client's bundle.
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
      "remoteModuleDeclFromExports expects a module whose default export is a schema() from this copy of 'spacetimedb'. If it is a schema(), the client loaded a second copy of the SDK. The usual causes are:\n" +
        "- The client was built without the 'spacetimedb-client' export condition, so 'spacetimedb/server' is the host build. Add it with Vite's resolve.conditions, esbuild's conditions, webpack's resolve.conditionNames, or node --conditions=spacetimedb-client.\n" +
        "- A second installation of the package, such as the module folder's own node_modules/spacetimedb. Install it once, for example in a workspace, or dedupe it with the bundler (Vite's resolve.dedupe).\n" +
        '- The module folder\'s package.json lacks "type": "module", so the module loaded as CommonJS. Add it.'
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
  // `makeProcedureExport`), and so does this.
  const { caseConversionPolicy } = spacetimedb.moduleDef;
  const convert = (name: string) =>
    caseConversionPolicy.tag === 'None' ? name : toCanonicalSnakeCase(name);
  const canonicalName = (accessorName: string, name?: string) =>
    name ?? convert(accessorName);

  const views: Record<string, UntypedTableBody> = {};
  const reducers = [];
  const procedures: UntypedProcedureDecl[] = [];
  for (const [accessorName, value] of Object.entries(module)) {
    const signature = (value as Partial<WithSignature<AnySignature>> | null)?.[
      exportSignature
    ];
    if (signature === undefined) continue;
    const name =
      signature.kind === 'procedure'
        ? convert(signature.name ?? accessorName)
        : canonicalName(accessorName, signature.name);
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
    tables: clientTables(
      {
        ...spacetimedb.schemaType.tables,
        ...tablesToSchema(new ModuleContext(), views).tables,
      },
      canonicalName
    ),
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

/**
 * Gives each table, and each column, the canonical name that the host gives
 * it, so that the client's queries and table updates use the host's names.
 * Generated bindings state every canonical name; module source need not.
 * Also makes each index a btree, as generated bindings declare it, since the
 * client's cache supports only btree indexes, which serve any lookup that a
 * hash or direct index does.
 */
function clientTables(
  tables: Record<string, UntypedTableDecl>,
  canonicalName: (accessorName: string, name?: string) => string
): Record<string, UntypedTableDecl> {
  return Object.fromEntries(
    Object.entries(tables).map(([accessorName, table]) => [
      accessorName,
      {
        ...table,
        sourceName: canonicalName(accessorName, table.tableName),
        resolvedIndexes: table.resolvedIndexes.map(index => ({
          ...index,
          algorithm: 'btree' as const,
        })),
        columns: Object.fromEntries(
          Object.entries(table.columns).map(([key, column]) => {
            const name = canonicalName(key, column.columnMetadata.name);
            if (name === key || name === column.columnMetadata.name) {
              return [key, column];
            }
            // Copy the column builder with the name, rather than call
            // `.name()`, which the `Result` and unit column builders lack.
            return [
              key,
              Object.assign(Object.create(Object.getPrototypeOf(column)), {
                ...column,
                columnMetadata: { ...column.columnMetadata, name },
              }),
            ];
          })
        ),
      },
    ])
  );
}

/**
 * Converts an accessor name to snake case as the host does when it derives a
 * canonical name under the `SnakeCase` policy (`convert_case`'s `Case::Snake`).
 * Unlike `toSnakeCase`, it splits acronyms (`XMLParser` is `xml_parser`)
 * and digits (`userId2` is `user_id_2`), and drops leading, trailing, and
 * repeated separators. Like the host, it reads graphemes, where the runtime
 * has `Intl.Segmenter`, and code points otherwise, which differs only for
 * combining characters.
 */
export function toCanonicalSnakeCase(name: string): string {
  const isUpper = (c: string) =>
    c.toUpperCase() !== c.toLowerCase() && c === c.toUpperCase();
  const isLower = (c: string) =>
    c.toUpperCase() !== c.toLowerCase() && c === c.toLowerCase();
  const isDigit = (c: string) => /^[0-9]+$/.test(c);
  const chars =
    typeof Intl !== 'undefined' && typeof Intl.Segmenter === 'function'
      ? Array.from(
          new Intl.Segmenter(undefined, { granularity: 'grapheme' }).segment(
            name
          ),
          s => s.segment
        )
      : Array.from(name);
  const words: string[] = [];
  let word = '';
  chars.forEach((c, i) => {
    if (c === '_' || c === '-' || c === ' ') {
      words.push(word);
      word = '';
      return;
    }
    const prev = chars[i - 1] ?? '';
    const next = chars[i + 1] ?? '';
    if (
      (isLower(prev) && (isUpper(c) || isDigit(c))) ||
      (isDigit(prev) && (isUpper(c) || isLower(c))) ||
      (isUpper(prev) && isDigit(c)) ||
      (isUpper(prev) && isUpper(c) && isLower(next))
    ) {
      words.push(word);
      word = '';
    }
    word += c;
  });
  words.push(word);
  return words
    .filter(w => w !== '')
    .map(w => w.toLowerCase())
    .join('_');
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
