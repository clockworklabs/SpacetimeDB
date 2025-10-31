import type { ProductType } from '../lib/algebraic_type';
import Lifecycle from '../lib/autogen/lifecycle_type';
import type RawReducerDefV9 from '../lib/autogen/raw_reducer_def_v_9_type';
import type { ConnectionId } from '../lib/connection_id';
import type { Identity } from '../lib/identity';
import type { Timestamp } from '../lib/timestamp';
import { MODULE_DEF, type UntypedSchemaDef } from './schema';
import type { Table } from './table';
import type {
  InferTypeOfRow,
  RowBuilder,
  RowObj,
  TypeBuilder,
} from './type_builders';

/**
 * Helper to extract the parameter types from an object type
 */
export type ParamsObj = Record<string, TypeBuilder<any, any>>;

/**
 * Helper to convert a ParamsObj or RowObj into an object type
 */
type ParamsAsObject<ParamDef extends ParamsObj | RowObj> =
  InferTypeOfRow<ParamDef>;

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
export type Reducer<
  S extends UntypedSchemaDef,
  Params extends ParamsObj | RowObj,
> = (
  ctx: ReducerCtx<S>,
  payload: ParamsAsObject<Params>
) => void | { tag: 'ok' } | { tag: 'err'; value: string };

/**
 * A type representing the database view, mapping table names to their corresponding Table handles.
 */
export type DbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in SchemaDef['tables'][number] as Tbl['name']]: Table<Tbl>;
};

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
  lifecycle?: RawReducerDefV9['lifecycle']
): void {
  if (existingReducers.has(name))
    throw new TypeError(`There is already a reducer with the name '${name}'`);
  existingReducers.add(name);

  const paramType: ProductType = {
    elements: Object.entries(params).map(([n, c]) => ({
      name: n,
      algebraicType: ('typeBuilder' in c ? c.typeBuilder : c).algebraicType,
    })),
  };

  MODULE_DEF.reducers.push({
    name,
    params: paramType,
    lifecycle, // <- lifecycle flag lands here
  });

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
export function reducer<
  S extends UntypedSchemaDef,
  Params extends ParamsObj | RowObj,
>(
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
