import type { EnvironmentFor } from './environment';
import type { DbView } from '../server/db_view';
import type { Random } from '../server/rng';
import type { ConnectionId } from './connection_id';
import type { Identity } from './identity';
import { type UntypedSchemaDecl } from './schema';
import { type Timestamp } from './timestamp';
import {
  ColumnBuilder,
  type InferTypeOfRow,
  type TypeBuilder,
} from './type_builders';
import { Uuid } from './uuid.ts';

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
export type ParamsAsObject<Params extends ParamsObj> = InferTypeOfRow<Params>;

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
export type Reducer<S extends UntypedSchemaDecl, Params extends ParamsObj> = (
  ctx: ReducerContext<S>,
  payload: ParamsAsObject<Params>
) => void;

/**
 * Authentication information for the caller of a reducer.
 */
export type AuthContext = Readonly<{
  /** Whether the caller is an internal system process. */
  isInternal: boolean;
  /** Whether the caller has authenticated with a JWT token. */
  hasJWT: boolean;
  /** The JWT claims associated with the caller, or null if hasJWT == false. */
  jwt: JwtClaims | null;
}>;

/** @deprecated Use `AuthContext` instead. */
export type AuthCtx = AuthContext;

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

export type AliasViews<SchemaDecl extends UntypedSchemaDecl> =
  SchemaDecl extends {
    namespaces: infer NS extends Record<string, UntypedSchemaDecl>;
  }
    ? { readonly [K in keyof NS]: ReducerContext<NS[K]> }
    : {};

/**
 * Reducer context parametrized by the inferred Schema
 */
export type ReducerContext<SchemaDecl extends UntypedSchemaDecl> = Readonly<{
  sender: Identity;
  databaseIdentity: Identity;
  /** @deprecated Use `databaseIdentity` instead. */
  identity: Identity;
  timestamp: Timestamp;
  connectionId: ConnectionId | null;
  db: DbView<SchemaDecl>;
  env: EnvironmentFor<SchemaDecl>;
  senderAuth: AuthContext;
  newUuidV4(): Uuid;
  newUuidV7(): Uuid;
  random: Random;
  as: AliasViews<SchemaDecl>;
}>;

/** @deprecated Use `ReducerContext` instead. */
export type ReducerCtx<SchemaDecl extends UntypedSchemaDecl> =
  ReducerContext<SchemaDecl>;
