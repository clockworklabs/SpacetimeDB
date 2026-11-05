import type {
  DbConnectionBuilder,
  DbConnectionImpl,
} from '../sdk/db_connection_impl';
import type { ConnectionState as ManagerConnectionState } from '../sdk/connection_manager';

export type ConnectionState = ManagerConnectionState & {
  getConnection(): DbConnectionImpl<any> | null;
  /**
   * Close the current connection and connect again with `builder`, for
   * example with a new token after sign-in or sign-out. The builder should
   * use the same uri and database name.
   */
  reconnect(builder: DbConnectionBuilder<any>): void;
};
