import type { DbConnectionImpl } from '../sdk/db_connection_impl.ts';
import type { ConnectionState as ManagerConnectionState } from '../sdk/connection_manager.ts';

export type ConnectionState = ManagerConnectionState & {
  getConnection(): DbConnectionImpl<any> | null;
};
