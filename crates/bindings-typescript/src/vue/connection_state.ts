import type { InjectionKey } from 'vue';
import type { ConnectionId } from '../lib/connection_id.ts';
import type { Identity } from '../lib/identity.ts';
import type { DbConnectionImpl } from '../sdk/db_connection_impl.ts';

export interface ConnectionState {
  isActive: boolean;
  identity?: Identity;
  token?: string;
  connectionId: ConnectionId;
  connectionError?: Error;
  getConnection<
    DbConnection extends DbConnectionImpl<any>,
  >(): DbConnection | null;
}

export const SPACETIMEDB_INJECTION_KEY = Symbol(
  'spacetimedb'
) as InjectionKey<ConnectionState>;
