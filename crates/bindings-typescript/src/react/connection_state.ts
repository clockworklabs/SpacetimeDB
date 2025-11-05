import type { ConnectionId } from "../lib/connection_id";
import type { Identity } from "../lib/identity";
import type { DbConnectionImpl } from "../sdk/db_connection_impl";

export type ConnectionState = {
  isActive: boolean;
  identity?: Identity;
  token?: string;
  connectionId: ConnectionId;
  connectionError?: Error;
  getConnection<DbConnection extends DbConnectionImpl<any>>(): DbConnection | null;
};
