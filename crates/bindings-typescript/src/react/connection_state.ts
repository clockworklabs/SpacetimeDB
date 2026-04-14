import type { DbConnectionImpl } from '../sdk/db_connection_impl';
import type { ConnectionState as ManagerConnectionState } from '../sdk/connection_manager';

export type ConnectionState = ManagerConnectionState & {
  getConnection(): DbConnectionImpl<any> | null;
};
