import { ProductType } from './algebraic_type';
import Lifecycle from './autogen/lifecycle_type';
import type RawReducerDefV9 from './autogen/raw_reducer_def_v_9_type';
import type { ConnectionId } from './connection_id';
import type { Identity } from './identity';
import type { Timestamp } from './timestamp';
import type { UntypedReducersDef } from '../sdk/reducers';
import type { DbView } from '../server/db_view';
import {
  MODULE_DEF,
  registerTypesRecursively,
  resolveType,
  type UntypedSchemaDef,
} from './schema';
import {
  ColumnBuilder,
  RowBuilder,
  type Infer,
  type InferTypeOfRow,
  type RowObj,
  type TypeBuilder,
} from './type_builders';
import type { ReducerSchema } from './reducer_schema';
import { toCamelCase, toPascalCase } from './util';
import type { CamelCase } from './type_util';
import type { Random } from '../server/rng';

/**
 * Helper to extract the parameter types from an object type
 */
export type ParamsObj = Record<
  string,
  TypeBuilder<any, any> | ColumnBuilder<any, any, any>
>;

/**
 * Helper to convert a ParamsObj or RowObj into an object type
 */
type ParamsAsObject<ParamDef extends ParamsObj> = InferTypeOfRow<ParamDef>;

/**
 * Defines a SpacetimeDB reducer function.
 * Reducers are the primary way to modify the state of your SpacetimeDB application.
 * They are atomic, meaning that either all operations within a reducer succeed,
 * or none of them do.
 * @template S - The inferred schema type of the SpacetimeDB module.
 * @template Params - The type of the parameters object expected by the reducer.
 * @param ctx - The reducer context, providing access to `sender`, `timestamp`, `connection_id`, and `db`.
 * @param payload - An object containing the arguments passed to the reducer, typed according to `params`.
 * @example
 * ```typescript
 * // Define a reducer named 'create_user' that takes 'username' (string) and 'email' (string)
 * reducer(
 *   'create_user',
 *   {
 *    username: t.string(),
 *    email: t.string(),
 *   },
 *   (ctx, { username, email }) => {
 *     // Access the 'user' table from the database view in the context
 *     ctx.db.user.insert({ username, email, created_at: ctx.timestamp });
 *     console.log(`User ${username} created by ${ctx.sender.identityId}`);
 *   }
 * );
 * ```
 */
export type Reducer<S extends UntypedSchemaDef, Params extends ParamsObj> = (
  ctx: ReducerCtx<S>,
  payload: ParamsAsObject<Params>
) => void | { tag: 'ok' } | { tag: 'err'; value: string };

/**
 * Authentication information for the caller of a reducer.
 */
export type AuthCtx = Readonly<{
  /** Whether the caller is an internal system process. */
  isInternal: boolean;
  /** Whether the caller has authenticated with a JWT token. */
  hasJWT: boolean;
  /** The JWT claims associated with the caller, or null if hasJWT == false. */
  jwt: JwtClaims | null;
}>;

export type JsonValue =
  | string
  | number
  | boolean
  | null
  | Array<JsonValue>
  | JsonObject;

export interface JsonObject {
  [key: string]: JsonValue;
}

/**
 * Auth Claims extracted from the payload of a JWT token
 */
export interface JwtClaims {
  /** The full payload as a JSON string */
  readonly rawPayload: string;
  /** The subject of the JWT token ('sub') */
  readonly subject: string;
  /** The issuer of the JWT token ('iss') */
  readonly issuer: string;
  /** The audience of the JWT token ('aud') */
  readonly audience: readonly string[];
  /** The identity associated with the JWT token, which is based on the sub and iss */
  readonly identity: Identity;
  /** The full payload as a JsonObject */
  readonly fullPayload: JsonObject;
}

/**
 * Reducer context parametrized by the inferred Schema
 */
export type ReducerCtx<SchemaDef extends UntypedSchemaDef> = Readonly<{
  sender: Identity;
  identity: Identity;
  timestamp: Timestamp;
  connectionId: ConnectionId | null;
  db: DbView<SchemaDef>;
  senderAuth: AuthCtx;
  random: Random;
}>;

/**
 * internal: pushReducer() helper used by reducer() and lifecycle wrappers
 *
 * @param name - The name of the reducer.
 * @param params - The parameters for the reducer.
 * @param fn - The reducer function.
 * @param lifecycle - Optional lifecycle hooks for the reducer.
 */
export function pushReducer(
  name: string,
  params: RowObj | RowBuilder<RowObj>,
  fn: Reducer<any, any>,
  lifecycle?: Infer<typeof RawReducerDefV9>['lifecycle']
): void {
  if (existingReducers.has(name)) {
    throw new TypeError(`There is already a reducer with the name '${name}'`);
  }
  existingReducers.add(name);

  if (!(params instanceof RowBuilder)) {
    params = new RowBuilder(params);
  }

  if (params.typeName === undefined) {
    params.typeName = toPascalCase(name);
  }

  const ref = registerTypesRecursively(params);
  const paramsType = resolveType(MODULE_DEF.typespace, ref).value;

  MODULE_DEF.reducers.push({
    name,
    params: paramsType,
    lifecycle, // <- lifecycle flag lands here
  });

  // If the function isn't named (e.g. `function foobar() {}`), give it the same
  // name as the reducer so that it's clear what it is in in backtraces.
  if (!fn.name) {
    Object.defineProperty(fn, 'name', { value: name, writable: false });
  }

  REDUCERS.push(fn);
}

const existingReducers = new Set<string>();
export const REDUCERS: Reducer<any, any>[] = [];

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
 * reducer(
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
export function reducer<S extends UntypedSchemaDef, Params extends ParamsObj>(
  name: string,
  params: Params,
  fn: (ctx: ReducerCtx<S>, payload: ParamsAsObject<Params>) => void
): void {
  pushReducer(name, params, fn);
}

/**
 * Registers an initialization reducer that runs when the SpacetimeDB module is published
 * for the first time.
 * This function is useful to set up any initial state of your database that is guaranteed
 * to run only once, and before any other reducers or client connections.
 * @template S - The inferred schema type of the SpacetimeDB module.
 * @template Params - The type of the parameters object expected by the initialization reducer.
 *
 * @param params - The parameters object defining the expected input for the initialization reducer.
 * @param fn - The initialization reducer function.
 * - `ctx`: The reducer context, providing access to `sender`, `timestamp`, `connection_id`, and `db`.
 */
export function init<S extends UntypedSchemaDef, Params extends ParamsObj>(
  name: string,
  params: Params,
  fn: Reducer<S, Params>
): void {
  pushReducer(name, params, fn, Lifecycle.Init);
}

/**
 * Registers a reducer to be called when a client connects to the SpacetimeDB module.
 * This function allows you to define custom logic that should execute
 * whenever a new client establishes a connection.
 * @template S - The inferred schema type of the SpacetimeDB module.
 * @template Params - The type of the parameters object expected by the connection reducer.
 * @param params - The parameters object defining the expected input for the connection reducer.
 * @param fn - The connection reducer function itself.
 */
export function clientConnected<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
>(name: string, params: Params, fn: Reducer<S, Params>): void {
  pushReducer(name, params, fn, Lifecycle.OnConnect);
}

/**
 * Registers a reducer to be called when a client disconnects from the SpacetimeDB module.
 * This function allows you to define custom logic that should execute
 * whenever a client disconnects.
 *
 * @template S - The inferred schema type of the SpacetimeDB module.
 * @template Params - The type of the parameters object expected by the disconnection reducer.
 * @param params - The parameters object defining the expected input for the disconnection reducer.
 * @param fn - The disconnection reducer function itself.
 * @example
 * ```typescript
 * spacetime.clientDisconnected(
 *   { reason: t.string() },
 *   (ctx, { reason }) => {
 *      console.log(`Client ${ctx.connection_id} disconnected: ${reason}`);
 *   }
 * );
 * ```
 */
export function clientDisconnected<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
>(name: string, params: Params, fn: Reducer<S, Params>): void {
  pushReducer(name, params, fn, Lifecycle.OnDisconnect);
}

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
