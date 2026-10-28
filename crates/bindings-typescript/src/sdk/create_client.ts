import {
  FunctionDeclaration,
  Schema,
  type ModuleDeclarations,
  type SchemaEntries,
} from '../schema/declarations';
import {
  ModuleContext,
  tableToSchema,
  type TableToSchema,
} from '../lib/schema';
import { table, type CoerceRow, type UntypedTableDef } from '../lib/table';
import type { TableSchema, UntypedTableSchema } from '../lib/table_schema';
import {
  ArrayBuilder,
  OptionBuilder,
  RowBuilder,
  type TypeBuilder,
} from '../lib/type_builders';
import { makeQueryBuilder, type QueryBuilder } from '../lib/query';
import type { NestAccessors, SnakeCase } from '../lib/type_util';
import { toSnakeCase, type CoerceParams } from '../lib/util';
import type { DbConnectionConfig } from './db_connection_impl';
import { DbConnectionImpl } from './db_connection_impl';
import { DbConnectionBuilder as Builder } from './db_connection_builder';
import { SubscriptionBuilderImpl } from './subscription_builder_impl';
import { reducerSchema, reducersToSchema } from './reducers';
import { procedureSchema } from './procedures';
import { _MINIMUM_CLI_VERSION } from './version';
import type { ClientDbView } from './db_view';
import type { UntypedRemoteModule } from './spacetime_module';

// The SDK consumes the same declarations at runtime and at the type level.
type AnyFunction = FunctionDeclaration<any, any, any, any>;
type Entries<M> = M extends { default: Schema<infer E> } ? E : never;
type Child<E> = E extends { module: infer M } ? M : E;
type Intersection<U> = (U extends unknown ? (x: U) => void : never) extends (
  x: infer I
) => void
  ? I
  : {};
type ViewRow<D> =
  D extends FunctionDeclaration<'view' | 'anonymousView', any, infer R, any>
    ? R extends ArrayBuilder<infer B> | OptionBuilder<infer B>
      ? B extends RowBuilder<infer Row>
        ? Row
        : never
      : never
    : never;
type ViewTable<K extends string, D> = TableToSchema<
  K,
  TableSchema<CoerceRow<ViewRow<D>>, readonly []>
>;
type Tables<M, P extends string = ''> = string extends keyof Entries<M>
  ? Record<string, UntypedTableDef>
  : Intersection<
      {
        [K in keyof Entries<M> &
          string]: Entries<M>[K] extends UntypedTableSchema
          ? { [A in `${P}${K}`]: TableToSchema<A, Entries<M>[K]> }
          : Tables<Child<Entries<M>[K]>, `${P}${K}.`>;
      }[keyof Entries<M> & string]
    > & {
      [K in keyof M & string as M[K] extends FunctionDeclaration<
        'view' | 'anonymousView',
        any,
        any,
        any
      >
        ? `${P}${K}`
        : never]: ViewTable<`${P}${K}`, M[K]>;
    };
type WireName<K extends string, D extends AnyFunction> = D['options'] extends {
  name: infer N extends string;
}
  ? N
  : SnakeCase<K>;
type NamespaceName<E, K extends string> = E extends {
  name: infer N extends string;
}
  ? N
  : SnakeCase<K>;
type Functions<
  M,
  Kind extends 'reducer' | 'procedure',
  P extends string = '',
  W extends string = '',
> = string extends keyof Entries<M>
  ? never
  :
      | {
          [K in keyof M & string]: M[K] extends FunctionDeclaration<
            Kind,
            infer Args,
            infer Ret,
            any
          >
            ? {
                name: `${W}${WireName<K, M[K]>}`;
                accessorName: `${P}${K}`;
                params: Kind extends 'procedure' ? CoerceParams<Args> : Args;
                paramsType: import('../lib/algebraic_type').ProductType;
                returnType: Ret;
              }
            : never;
        }[keyof M & string]
      | {
          [K in keyof Entries<M> &
            string]: Entries<M>[K] extends UntypedTableSchema
            ? never
            : Functions<
                Child<Entries<M>[K]>,
                Kind,
                `${P}${K}.`,
                `${W}${NamespaceName<Entries<M>[K], K>}.`
              >;
        }[keyof Entries<M> & string];

export type RemoteModuleOfDeclarations<M> = {
  tables: Extract<Tables<M>, Record<string, UntypedTableDef>>;
  reducers: readonly Functions<M, 'reducer'>[];
  procedures: readonly Functions<M, 'procedure'>[];
  versionInfo: { cliVersion: string };
  viewsWithArgs: readonly string[];
};

type AccessorMap<T extends { accessorName: string }> = NestAccessors<{
  [F in T as F['accessorName']]: F;
}>;

type WithAliases<T, A extends Record<string, string>> = T & {
  readonly [K in keyof A]: A[K] extends keyof T ? T[A[K]] : never;
};
export type ClientConnection<
  M,
  A extends Record<string, string> = {},
> = DbConnectionImpl<RemoteModuleOfDeclarations<M>> & {
  db: WithAliases<ClientDbView<RemoteModuleOfDeclarations<M>>, A>;
};

export interface Client<M, A extends Record<string, string> = {}> {
  DbConnection: {
    new (
      config: DbConnectionConfig<RemoteModuleOfDeclarations<M>>
    ): ClientConnection<M, A>;
    builder(): Builder<ClientConnection<M, A>>;
    prototype: ClientConnection<M, A>;
  };
  DbConnectionBuilder: typeof Builder<ClientConnection<M, A>>;
  SubscriptionBuilder: typeof SubscriptionBuilderImpl<
    RemoteModuleOfDeclarations<M>
  >;
  tables: WithAliases<
    NestAccessors<QueryBuilder<RemoteModuleOfDeclarations<M>>>,
    A
  >;
  reducers: AccessorMap<Functions<M, 'reducer'>>;
  procedures: AccessorMap<Functions<M, 'procedure'>>;
}

function withAliases<T extends object>(
  value: T,
  aliases: Record<string, string>
): T {
  const result = Object.create(Object.getPrototypeOf(value));
  Object.defineProperties(result, Object.getOwnPropertyDescriptors(value));
  for (const [alias, target] of Object.entries(aliases)) {
    if (!(target in value))
      throw new TypeError(`Unknown table alias target: ${target}`);
    if (!(alias in result))
      Object.defineProperty(result, alias, {
        enumerable: true,
        get: () => (value as Record<string, unknown>)[target],
      });
  }
  return result;
}

/** Turn dotted accessor paths into the same nested API used by connections. */
function nest<T extends Record<string, unknown>>(flat: T): NestAccessors<T> {
  const result = Object.create(null);
  for (const [path, value] of Object.entries(flat)) {
    const parts = path.split('.');
    let target = result;
    for (const part of parts.slice(0, -1))
      target = target[part] ??= Object.create(null);
    const key = parts.at(-1)!;
    if (key in target)
      throw new TypeError(`Duplicate client accessor: ${path}`);
    target[key] = value;
  }
  return result;
}

/** Construct the client API from body-free module declarations. */
export function createClient<
  const M extends ModuleDeclarations,
  const A extends Record<string, string> = {},
>(
  declarations: M,
  options: { cliVersion?: string; tableAliases?: A } = {}
): Client<M, A> {
  const remote: UntypedRemoteModule = {
    tables: Object.create(null),
    reducers: [],
    procedures: [],
    versionInfo: {
      cliVersion: options.cliVersion ?? _MINIMUM_CLI_VERSION.toString(),
    },
    viewsWithArgs: [],
  };
  const active = new Set<Schema<any>>();
  function collect(
    module: ModuleDeclarations,
    accessorPrefix = '',
    canonicalPrefix = ''
  ) {
    const schema = module.default;
    if (!(schema instanceof Schema))
      throw new TypeError('Expected a module with a schema default export');
    if (active.has(schema)) throw new TypeError('Cyclic module declarations');
    active.add(schema);
    const ctx = new ModuleContext();
    const canonical = (name: string) =>
      schema.options.CASE_CONVERSION_POLICY?.tag === 'None'
        ? name
        : toSnakeCase(name);
    const registerTable = (
      accessor: string,
      declaration: UntypedTableSchema
    ) => {
      const def = tableToSchema(
        accessor,
        declaration,
        declaration.tableDef(ctx, accessor)
      );
      def.sourceName =
        canonicalPrefix + (declaration.tableName ?? canonical(accessor));
      def.accessorName = accessorPrefix + accessor;
      if (def.accessorName in remote.tables)
        throw new TypeError(`Duplicate table: ${def.accessorName}`);
      remote.tables[def.accessorName] = def;
    };
    for (const [accessor, entry] of Object.entries(
      schema.entries as SchemaEntries
    )) {
      if ('rowType' in entry) registerTable(accessor, entry);
      else {
        const child = 'module' in entry ? entry.module : entry;
        const name = 'module' in entry ? entry.name : canonical(accessor);
        collect(
          child,
          `${accessorPrefix}${accessor}.`,
          `${canonicalPrefix}${name}.`
        );
      }
    }
    for (const [accessor, value] of Object.entries(module)) {
      if (!(value instanceof FunctionDeclaration)) continue;
      if (value.schema !== schema)
        throw new TypeError(
          `Declaration ${accessor} belongs to another schema`
        );
      const wireName =
        canonicalPrefix + (value.options.name ?? canonical(accessor));
      const accessorName = accessorPrefix + accessor;
      if (value.kind === 'reducer') {
        const descriptor = reducerSchema(wireName, value.params, accessorName);
        (remote.reducers as any[]).push(
          ...reducersToSchema([descriptor]).reducers
        );
      } else if (value.kind === 'procedure') {
        (remote.procedures as any[]).push(
          procedureSchema(
            wireName,
            value.params,
            value.returnType,
            accessorName
          )
        );
      } else {
        const builder = value.returnType;
        const row =
          builder instanceof ArrayBuilder
            ? builder.element
            : builder instanceof OptionBuilder
              ? builder.value
              : undefined;
        if (!(row instanceof RowBuilder))
          throw new TypeError(
            `View ${accessor} must return an array or optional row`
          );
        registerTable(
          accessor,
          table(
            { name: value.options.name ?? canonical(accessor), public: true },
            row
          )
        );
        if (Object.keys(value.params).length)
          (remote.viewsWithArgs as string[]).push(wireName);
      }
    }
    active.delete(schema);
  }
  collect(declarations);
  const module = remote;
  class SubscriptionBuilder extends SubscriptionBuilderImpl<typeof module> {}
  class DbConnection extends DbConnectionImpl<typeof module> {
    constructor(config: DbConnectionConfig<typeof module>) {
      super(config);
      this.db = withAliases(this.db, options.tableAliases ?? {});
    }
    static builder(): DbConnectionBuilder {
      return new DbConnectionBuilder(
        module,
        config => new DbConnection(config)
      );
    }
    override subscriptionBuilder = (): SubscriptionBuilder =>
      new SubscriptionBuilder(this);
  }
  class DbConnectionBuilder extends Builder<DbConnection> {}
  const tables = Object.freeze(
    withAliases(nest(makeQueryBuilder(module)), options.tableAliases ?? {})
  );
  const reducers = nest(
    Object.fromEntries(module.reducers.map(r => [r.accessorName, r]))
  );
  const procedures = nest(
    Object.fromEntries(module.procedures.map(p => [p.accessorName, p]))
  );
  return {
    DbConnection,
    DbConnectionBuilder,
    SubscriptionBuilder,
    tables,
    reducers,
    procedures,
  } as unknown as Client<M, A>;
}
