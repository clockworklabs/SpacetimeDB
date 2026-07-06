import type {
  DbConnectionBuilder,
  DbConnectionImpl,
} from '../sdk/db_connection_impl';
import type { ConnectionState as ManagerConnectionState } from '../sdk/connection_manager';

export const SPACETIMEDB_CONTEXT_KEY = Symbol('spacetimedb');

export type ConnectionState = ManagerConnectionState & {
  /** The live connection, or `null` before it is first established. */
  getConnection(): DbConnectionImpl<any> | null;
  /**
   * Tear down the current connection and reconnect using a fresh builder —
   * typically to apply a new auth token after sign-in or sign-out. The builder
   * should carry the new token and the same uri + database name. Table and
   * reducer subscriptions re-bind automatically once the new connection is
   * live, so there is no need to reload the page to swap a token.
   */
  reconnect(builder: DbConnectionBuilder<any>): void;
};
