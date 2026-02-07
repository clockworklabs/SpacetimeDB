import Lifecycle from '../lib/autogen/lifecycle_type';
import type RawReducerDefV9 from '../lib/autogen/raw_reducer_def_v_9_type';
import type {
  ParamsAsObject,
  ParamsObj,
  Reducer,
  ReducerCtx,
} from '../lib/reducers';
import { type UntypedSchemaDef } from '../lib/schema';
import {
  ColumnBuilder,
  RowBuilder,
  type Infer,
  type RowObj,
  type TypeBuilder,
} from '../lib/type_builders';
import { toPascalCase } from '../lib/util';
import type { SchemaInner } from './schema';

/**
 * internal: pushReducer() helper used by reducer() and lifecycle wrappers
 *
 * @param name - The name of the reducer.
 * @param params - The parameters for the reducer.
 * @param fn - The reducer function.
 * @param lifecycle - Optional lifecycle hooks for the reducer.
 */
export function pushReducer(
  ctx: SchemaInner,
  name: string,
  params: RowObj | RowBuilder<RowObj>,
  fn: Reducer<any, any>,
  lifecycle?: Infer<typeof RawReducerDefV9>['lifecycle']
): void {
  ctx.defineFunction(name);

  if (!(params instanceof RowBuilder)) {
    params = new RowBuilder(params);
  }

  if (params.typeName === undefined) {
    params.typeName = toPascalCase(name);
  }

  const ref = ctx.registerTypesRecursively(params);
  const paramsType = ctx.resolveType(ref).value;

  ctx.moduleDef.reducers.push({
    name,
    params: paramsType,
    lifecycle, // <- lifecycle flag lands here
  });

  // If the function isn't named (e.g. `function foobar() {}`), give it the same
  // name as the reducer so that it's clear what it is in in backtraces.
  if (!fn.name) {
    Object.defineProperty(fn, 'name', { value: name, writable: false });
  }

  ctx.reducers.push(fn);
}

export type Reducers = Reducer<any, any>[];

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
  ctx: SchemaInner,
  name: string,
  params: Params,
  fn: Reducer<S, Params>
): void {
  pushReducer(ctx, name, params, fn);
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
  ctx: SchemaInner,
  name: string,
  params: Params,
  fn: Reducer<S, Params>
): void {
  pushReducer(ctx, name, params, fn, Lifecycle.Init);
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
>(
  ctx: SchemaInner,
  name: string,
  params: Params,
  fn: Reducer<S, Params>
): void {
  pushReducer(ctx, name, params, fn, Lifecycle.OnConnect);
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
>(
  ctx: SchemaInner,
  name: string,
  params: Params,
  fn: Reducer<S, Params>
): void {
  pushReducer(ctx, name, params, fn, Lifecycle.OnDisconnect);
}
