import type {
  DbConnectionBuilder,
  DbConnectionImpl,
} from '../sdk/db_connection_impl.ts';
import type { ConnectionState as ManagerConnectionState } from '../sdk/connection_manager.ts';

export type ConnectionState = ManagerConnectionState & {
  getConnection(): DbConnectionImpl<any> | null;
  /**
   * Tear down the current connection and reconnect using a fresh builder, for
   * example to switch identity after sign-in or sign-out. The builder should
   * carry the new token and the same uri + database name. Hooks re-bind to the
   * new connection automatically.
   */
  reconnect(builder: DbConnectionBuilder<any>): void;
};
