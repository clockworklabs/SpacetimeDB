import type { InjectionKey } from 'vue';
import type { ConnectionId } from '../lib/connection_id';
import type { Identity } from '../lib/identity';
import type {
  DbConnectionBuilder,
  DbConnectionImpl,
} from '../sdk/db_connection_impl';

export interface ConnectionState {
  isActive: boolean;
  identity?: Identity;
  token?: string;
  connectionId: ConnectionId;
  connectionError?: Error;
  getConnection<
    DbConnection extends DbConnectionImpl<any>,
  >(): DbConnection | null;
  /** Replace the pooled connection with a fresh builder, e.g. after sign-in. */
  reconnect(builder: DbConnectionBuilder<any>): void;
}

export const SPACETIMEDB_INJECTION_KEY = Symbol(
  'spacetimedb'
) as InjectionKey<ConnectionState>;
