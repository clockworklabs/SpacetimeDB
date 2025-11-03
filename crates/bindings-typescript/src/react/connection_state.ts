import type { ConnectionId } from "../lib/connection_id";
import type { Identity } from "../lib/identity";
import type { DbConnectionImpl } from "../sdk/db_connection_impl";
import type { UntypedRemoteModule } from "../sdk/spacetime_module";

export type ConnectionState<DbConnection extends DbConnectionImpl<UntypedRemoteModule>> = {
  isActive: boolean;
  identity?: Identity;
  token?: string;
  connectionId: ConnectionId;
  connectionError?: Error;
  getConnection(): DbConnection | null;
};
