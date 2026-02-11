import type { ProductType } from '../lib/algebraic_type';
import type { ReducerSchema } from '../lib/reducer_schema';
import type { ParamsObj } from '../lib/reducers';
import type { CoerceRow } from '../lib/table';
import { RowBuilder, type InferTypeOfRow } from '../lib/type_builders';
import type { CamelCase, PascalCase } from '../lib/type_util';
import { toCamelCase } from '../lib/util';
import type { CallReducerFlags } from './db_connection_impl';
import type {
  ReducerEventContextInterface,
  SubscriptionEventContextInterface,
} from './event_context';
import type { UntypedRemoteModule } from './spacetime_module';

export type ReducerEventCallback<
  RemoteModule extends UntypedRemoteModule,
  ReducerArgs extends object = object,
> = (
  ctx: ReducerEventContextInterface<RemoteModule>,
  args: ReducerArgs
) => void;

export type SubscriptionEventCallback<
  RemoteModule extends UntypedRemoteModule,
> = (ctx: SubscriptionEventContextInterface<RemoteModule>) => void;

// Utility: detect 'any'
type IfAny<T, Y, N> = 0 extends 1 & T ? Y : N;

// Loose shape that allows all three families even when key names are unknown
type ReducersViewLoose = {
  // call: camelCase(name)
  [k: string]: (params: any) => void;
} & {
  // onX
  [k: `on${string}`]: (callback: ReducerEventCallback<any, any>) => void;
} & {
  // removeOnX
  [k: `removeOn${string}`]: (callback: ReducerEventCallback<any, any>) => void;
};

export type ReducersView<RemoteModule> = IfAny<
  RemoteModule,
  ReducersViewLoose,
  RemoteModule extends UntypedRemoteModule
    ? // x: camelCase(name)
      {
        [K in RemoteModule['reducers'][number] as CamelCase<
          K['accessorName']
        >]: (params: InferTypeOfRow<K['params']>) => void;
      } & {
        // onX: `on${PascalCase(name)}`
        [K in RemoteModule['reducers'][number] as `on${PascalCase<K['accessorName']>}`]: (
          callback: ReducerEventCallback<
            RemoteModule,
            InferTypeOfRow<K['params']>
          >
        ) => void;
      } & {
        // removeOnX: `removeOn${PascalCase(name)}`
        [K in RemoteModule['reducers'][number] as `removeOn${PascalCase<K['accessorName']>}`]: (
          callback: ReducerEventCallback<
            RemoteModule,
            InferTypeOfRow<K['params']>
          >
        ) => void;
      }
    : never
>;

export type ReducerEventInfo<
  Name extends string = string,
  Args extends object = object,
> = {
  name: Name;
  args: Args;
};

export type UntypedReducerDef = {
  name: string;
  accessorName: string;
  params: CoerceRow<ParamsObj>;
  paramsType: ProductType;
};

export type UntypedReducersDef = {
  reducers: readonly UntypedReducerDef[];
};

export type SetReducerFlags<R extends UntypedReducersDef> = {
  [K in keyof R['reducers'] as CamelCase<R['reducers'][number]['name']>]: (
    flags: CallReducerFlags
  ) => void;
};

class Reducers<ReducersDef extends UntypedReducersDef> {
  reducersType: ReducersDef;

  constructor(handles: readonly ReducerSchema<any, any>[]) {
    this.reducersType = reducersToSchema(handles) as ReducersDef;
  }
}

/**
 * Helper type to convert an array of TableSchema into a schema definition
 */
type ReducersToSchema<T extends readonly ReducerSchema<any, any>[]> = {
  reducers: {
    /** @type {UntypedReducerDef} */
    readonly [i in keyof T]: {
      name: T[i]['reducerName'];
      accessorName: CamelCase<T[i]['accessorName']>;
      params: T[i]['params']['row'];
      paramsType: T[i]['paramsSpacetimeType'];
    };
  };
};

export function reducersToSchema<
  const T extends readonly ReducerSchema<any, any>[],
>(reducers: T): ReducersToSchema<T> {
  const mapped = reducers.map(r => {
    const paramsRow = r.params.row;

    return {
      name: r.reducerName,
      // Prefer the schema's own accessorName if present at runtime; otherwise derive it.
      accessorName: r.accessorName,
      params: paramsRow,
      paramsType: r.paramsSpacetimeType,
    } as const;
  }) as {
    readonly [I in keyof T]: {
      name: T[I]['reducerName'];
      accessorName: T[I]['accessorName'];
      params: T[I]['params']['row'];
      paramsType: T[I]['paramsSpacetimeType'];
    };
  };

  const result = { reducers: mapped } satisfies ReducersToSchema<T>;
  return result;
}

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
export function reducers<const H extends readonly ReducerSchema<any, any>[]>(
  ...handles: H
): Reducers<ReducersToSchema<H>>;

/**
 * Creates a schema from table definitions (array overload)
 * @param handles - Array of table handles created by table() function
 * @returns ColumnBuilder representing the complete database schema
 */
export function reducers<const H extends readonly ReducerSchema<any, any>[]>(
  handles: H
): Reducers<ReducersToSchema<H>>;

export function reducers<const H extends readonly ReducerSchema<any, any>[]>(
  ...args: [H] | H
): Reducers<ReducersToSchema<H>> {
  const handles = (
    args.length === 1 && Array.isArray(args[0]) ? args[0] : args
  ) as H;
  return new Reducers(handles);
}

export function reducerSchema<
  ReducerName extends string,
  Params extends ParamsObj,
>(name: ReducerName, params: Params): ReducerSchema<ReducerName, Params> {
  const paramType: ProductType = {
    elements: Object.entries(params).map(([n, c]) => ({
      name: n,
      algebraicType:
        'typeBuilder' in c ? c.typeBuilder.algebraicType : c.algebraicType,
    })),
  };
  return {
    reducerName: name,
    accessorName: toCamelCase(name),
    params: new RowBuilder<Params>(params),
    paramsSpacetimeType: paramType,
    reducerDef: {
      name,
      params: paramType,
      lifecycle: undefined,
    },
  };
}
