import {
  FunctionSchema,
  FunctionGroup,
  Schema,
  type ModuleSchemaExports,
  type SchemaEntries,
} from '../schema/module.ts';
import {
  ModuleContext,
  tableToSchema,
  type TableToSchema,
} from '../lib/schema.ts';
import { table, type CoerceRow, type UntypedTableDef } from '../lib/table.ts';
import type { TableSchema, UntypedTableSchema } from '../lib/table_schema.ts';
import { ArrayBuilder, OptionBuilder, RowBuilder } from '../lib/type_builders.ts';
import { makeQueryBuilder, type QueryBuilder } from '../lib/query.ts';
import type { NestAccessors, SnakeCase } from '../lib/type_util.ts';
import { toSnakeCase, type CoerceParams } from '../lib/util.ts';
import type { DbConnectionConfig } from './db_connection_impl.ts';
import { DbConnectionImpl } from './db_connection_impl.ts';
import { DbConnectionBuilder as Builder } from './db_connection_builder.ts';
import { SubscriptionBuilderImpl } from './subscription_builder_impl.ts';
import { reducerSchema, reducersToSchema } from './reducers.ts';
import { procedureSchema } from './procedures.ts';
import { _MINIMUM_CLI_VERSION } from './version.ts';
import type { UntypedRemoteModule } from './spacetime_module.ts';

// The SDK consumes the same module schema at runtime and at the type level.
type AnyFunction = FunctionSchema<any, any, any, any>;
type Entries<M> = M extends { default: Schema<infer E> } ? E : never;
type Child<E> = E extends { module: infer M } ? M : E;
type Intersection<U> = (U extends unknown ? (x: U) => void : never) extends (
  x: infer I
) => void
  ? I
  : {};
type FunctionEntries<M> = Intersection<
  {
    [K in keyof M & string]: M[K] extends FunctionGroup<infer E>
      ? E
      : M[K] extends AnyFunction
        ? { [A in K]: M[K] }
        : {};
  }[keyof M & string]
>;
type ViewRow<D> =
  D extends FunctionSchema<'view' | 'anonymousView', any, infer R, any>
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
      [K in keyof FunctionEntries<M> &
        string as FunctionEntries<M>[K] extends FunctionSchema<
        'view' | 'anonymousView',
        any,
        any,
        any
      >
        ? `${P}${K}`
        : never]: ViewTable<`${P}${K}`, FunctionEntries<M>[K]>;
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
          [K in keyof FunctionEntries<M> &
            string]: FunctionEntries<M>[K] extends FunctionSchema<
            Kind,
            infer Args,
            infer Ret,
            any
          >
            ? {
                name: `${W}${WireName<K, FunctionEntries<M>[K]>}`;
                accessorName: `${P}${K}`;
              } & (Kind extends 'procedure'
                ? {
                    params: CoerceParams<Args>;
                    returnType: Ret;
                  }
                : {
                    params: Args;
                    paramsType: import('../lib/algebraic_type').ProductType;
                  })
            : never;
        }[keyof FunctionEntries<M> & string]
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

type TableAliases<T, A extends Record<string, string>> = {
  readonly [K in Exclude<keyof A, keyof T>]: A[K] extends keyof T
    ? Omit<T[A[K]], 'accessorName'> & { readonly accessorName: K }
    : never;
};

export type RemoteModuleOfSchema<M, A extends Record<string, string> = {}> = {
  tables: Extract<Tables<M>, Record<string, UntypedTableDef>> &
    TableAliases<Tables<M>, A>;
  reducers: readonly Functions<M, 'reducer'>[];
  procedures: readonly Functions<M, 'procedure'>[];
  versionInfo: { cliVersion: string };
  viewsWithArgs: readonly string[];
};

type AccessorMap<T extends { accessorName: string }> = NestAccessors<{
  [F in T as F['accessorName']]: F;
}>;

type WithAliases<T, A extends Record<string, string>> = T & {
  readonly [K in Exclude<keyof A, keyof T>]: A[K] extends keyof T
    ? T[A[K]]
    : never;
};
export type ClientConnection<
  M,
  A extends Record<string, string> = {},
> = DbConnectionImpl<RemoteModuleOfSchema<M, A>>;

export interface Client<M, A extends Record<string, string> = {}> {
  DbConnection: {
    new (
      config: DbConnectionConfig<RemoteModuleOfSchema<M, A>>
    ): ClientConnection<M, A>;
    builder(): Builder<ClientConnection<M, A>>;
    prototype: ClientConnection<M, A>;
  };
  DbConnectionBuilder: {
    new (
      remoteModule: RemoteModuleOfSchema<M, A>,
      connection: (
        config: DbConnectionConfig<RemoteModuleOfSchema<M, A>>
      ) => ClientConnection<M, A>
    ): Builder<ClientConnection<M, A>>;
    prototype: Builder<ClientConnection<M, A>>;
  };
  SubscriptionBuilder: typeof SubscriptionBuilderImpl<
    RemoteModuleOfSchema<M, A>
  >;
  tables: WithAliases<NestAccessors<QueryBuilder<RemoteModuleOfSchema<M>>>, A>;
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

/** Construct the client API from body-free module schemas. */
export function createClient<
  const M extends ModuleSchemaExports,
  const A extends Record<string, string> = {},
>(
  moduleSchema: M,
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
    module: ModuleSchemaExports,
    accessorPrefix = '',
    canonicalPrefix = ''
  ) {
    const schema = module.default;
    if (!(schema instanceof Schema))
      throw new TypeError('Expected a module with a schema default export');
    if (active.has(schema)) throw new TypeError('Cyclic module schemas');
    active.add(schema);
    const ctx = new ModuleContext();
    const canonical = (name: string) =>
      schema.options.CASE_CONVERSION_POLICY?.tag === 'None'
        ? name
        : toSnakeCase(name);
    const registerTable = (
      accessor: string,
      tableSchema: UntypedTableSchema
    ) => {
      const def = tableToSchema(
        accessor,
        tableSchema,
        tableSchema.tableDef(ctx, accessor)
      );
      def.sourceName =
        canonicalPrefix + (tableSchema.tableName ?? canonical(accessor));
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
    const functionEntries = Object.entries(
      module as unknown as Record<string, unknown>
    ).flatMap(([name, value]): [string, unknown][] => {
      if (value instanceof FunctionGroup) {
        if (value.schema !== schema)
          throw new TypeError(
            `Function group ${name} belongs to another schema`
          );
        return Object.entries(value.exports);
      }
      return [[name, value]];
    });
    const names = new Set<string>();
    for (const [accessor, value] of functionEntries) {
      if (!(value instanceof FunctionSchema)) continue;
      if (value.schema !== schema)
        throw new TypeError(`Function ${accessor} belongs to another schema`);
      if (names.has(accessor))
        throw new TypeError(`Duplicate function: ${accessor}`);
      names.add(accessor);
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
  collect(moduleSchema);
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
