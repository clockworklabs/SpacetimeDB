import type { DbConnectionBase } from '../sdk/db_connection_impl';
import type { ConnectionState as ManagerConnectionState } from '../sdk/connection_manager';

export type ConnectionState = ManagerConnectionState & {
  getConnection(): DbConnectionBase<any> | null;
};
